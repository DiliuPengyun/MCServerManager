use std::{fs, path::PathBuf, sync::Arc};

use super::{ServerConfig, ServerId, ServerManager};
use crate::error::{AppError, Result};

/// 配置提交前先原子改名，保存失败时仍可把完整目录放回原位。
struct StagedDeletion {
    original: PathBuf,
    trash: PathBuf,
}

impl StagedDeletion {
    fn new(server: &ServerConfig) -> Result<Self> {
        let directory = &server.working_dir;
        let parent = directory
            .parent()
            .ok_or_else(|| AppError::invalid("invalid_path", "不能删除文件系统根目录"))?;
        let name = directory
            .file_name()
            .ok_or_else(|| AppError::invalid("invalid_path", "服务器目录名称无效"))?
            .to_string_lossy();
        let trash = parent.join(format!(".{name}.mcsm-delete-{}", ServerId::new().simple()));
        fs::rename(directory, &trash).map_err(|error| {
            AppError::io(format!("无法暂存待删除目录 {}", directory.display()), error)
        })?;
        Ok(Self {
            original: directory.clone(),
            trash,
        })
    }

    fn rollback(self, error: AppError) -> AppError {
        match fs::rename(&self.trash, &self.original) {
            Ok(()) => error,
            Err(rollback) => AppError::internal(format!(
                "移除服务器配置失败: {error}；目录回滚也失败: {rollback}；文件仍保留在 {}",
                self.trash.display()
            )),
        }
    }

    async fn cleanup(self) -> Result<()> {
        let cleanup = tokio::task::spawn_blocking(move || fs::remove_dir_all(&self.trash))
            .await
            .map_err(|error| AppError::internal(format!("删除任务异常: {error}")))?;
        if let Err(error) = cleanup {
            return Err(AppError::internal(format!(
                "服务器配置已移除，但待删除目录清理失败: {error}"
            )));
        }
        Ok(())
    }
}

impl ServerManager {
    pub(super) async fn remove_server_after_stop(
        &self,
        id: ServerId,
        delete_files: bool,
    ) -> Result<()> {
        let process = self
            .process(id)
            .ok_or_else(|| AppError::not_found("服务器运行态不存在"))?;
        // 已停止检查、tmux 确认和配置提交必须在同一生命周期临界区内。
        // 否则已经越过工作租约检查的启动仍可能在目录改名后创建 JVM。
        let _operation = process.lock_stopped().await?;
        let server = self.server_config(id)?;
        self.ensure_no_tmux_session(&server).await?;

        let staged = if delete_files {
            Some(StagedDeletion::new(&server)?)
        } else {
            None
        };
        if let Err(error) = self.commit_removal(id) {
            return Err(match staged {
                Some(staged) => staged.rollback(error),
                None => error,
            });
        }
        self.forget_runtime(id);

        if let Some(staged) = staged {
            staged.cleanup().await?;
        }
        Ok(())
    }

    fn commit_removal(&self, id: ServerId) -> Result<()> {
        let _guard = self.mutation.lock();
        let mut next = (*self.config()).clone();
        let Some(index) = next.index_of(id) else {
            return Err(AppError::not_found("服务器不存在"));
        };
        next.servers.remove(index);
        next.active_server_id = next
            .servers
            .get(index.min(next.servers.len().saturating_sub(1)))
            .map(|server| server.id);
        self.store.save(&next)?;
        *self.config.write() = Arc::new(next);
        Ok(())
    }

    fn forget_runtime(&self, id: ServerId) {
        self.processes.write().remove(&id);
        self.players.write().remove(&id);
        self.rcon_connections.lock().remove(&id);
    }
}
