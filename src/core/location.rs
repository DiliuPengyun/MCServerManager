use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::Serialize;
use uuid::Uuid;

use super::{
    AppConfig, ServerConfig, ServerId, ServerManager,
    session::stable_session_name,
    validation::{comparable_path, ensure_unique_working_dir, normalize_lexical, validate_server},
};
use crate::{
    error::{AppError, Result},
    server::{
        detect,
        fs_copy::{copy_directory_contents, verify_directory_copy},
        tmux::SessionStatus,
    },
};

#[derive(Clone, Debug, Serialize)]
pub struct LocationChange {
    pub server: ServerConfig,
    /// 数据和配置已切换成功时，旧目录清理失败不属于移动失败。
    pub warning: Option<String>,
}

struct MovePlan {
    id: ServerId,
    server: ServerConfig,
    source: PathBuf,
    destination: PathBuf,
}

impl ServerManager {
    /// 由管理器移动服务器目录。调用方必须先取得用户明确动作，本方法不会启动或停止服务器。
    pub async fn move_server(
        self: &Arc<Self>,
        id: ServerId,
        destination: &Path,
    ) -> Result<LocationChange> {
        let _work = self.begin_server_work(id)?;
        let plan = self.prepare_move(id, destination)?;
        let process = self
            .process(id)
            .ok_or_else(|| AppError::not_found("服务器运行态不存在"))?;
        let _operation = process.lock_stopped().await?;
        self.ensure_no_tmux_session(&plan.server).await?;

        match attempt_rename(&plan).await? {
            RenameOutcome::Moved => self.complete_same_filesystem_move(&plan).await,
            RenameOutcome::CrossDevice => self.complete_cross_filesystem_move(&plan).await,
        }
    }

    fn prepare_move(&self, id: ServerId, destination: &Path) -> Result<MovePlan> {
        let server = self.server_config(id)?;
        let source = server.working_dir.clone();
        let destination = checked_destination(destination)?;
        if comparable_path(&source) == comparable_path(&destination) {
            return Err(AppError::already_exists("目标目录就是当前服务器目录"));
        }
        if destination.starts_with(&source) {
            return Err(AppError::invalid(
                "invalid_path",
                "不能把服务器目录移动到它自己的子目录中",
            ));
        }
        self.ensure_directory_available(id, &destination)?;
        ensure_real_directory(&source, "源服务器目录")?;
        ensure_absent(&destination)?;
        Ok(MovePlan {
            id,
            server,
            source,
            destination,
        })
    }

    async fn complete_same_filesystem_move(&self, plan: &MovePlan) -> Result<LocationChange> {
        if let Err(config_error) = self.commit_location(plan.id, &plan.destination) {
            return Err(rollback_renamed_directory(plan, config_error).await);
        }
        Ok(LocationChange {
            server: self.server_config(plan.id)?,
            warning: None,
        })
    }

    async fn complete_cross_filesystem_move(&self, plan: &MovePlan) -> Result<LocationChange> {
        let copy_source = plan.source.clone();
        let copy_destination = plan.destination.clone();
        tokio::task::spawn_blocking(move || {
            copy_verified_directory(&copy_source, &copy_destination)
        })
        .await
        .map_err(|error| AppError::internal(format!("跨文件系统复制任务异常: {error}")))??;

        if let Err(config_error) = self.commit_location(plan.id, &plan.destination) {
            // 原目录仍完整保留；只有配置无法切换时才移除刚创建的副本。
            if let Err(cleanup_error) = fs::remove_dir_all(&plan.destination) {
                return Err(AppError::internal(format!(
                    "配置保存失败: {config_error}；新副本清理也失败: {cleanup_error}；原目录仍在 {}，副本位于 {}",
                    plan.source.display(),
                    plan.destination.display()
                )));
            }
            return Err(config_error);
        }

        let cleanup_source = plan.source.clone();
        let warning =
            match tokio::task::spawn_blocking(move || fs::remove_dir_all(cleanup_source)).await {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(format!(
                    "服务器已移动到 {}，但旧目录 {} 未能清理: {error}",
                    plan.destination.display(),
                    plan.source.display()
                )),
                Err(error) => Some(format!(
                    "服务器已移动到 {}，但旧目录清理任务异常: {error}；旧目录为 {}",
                    plan.destination.display(),
                    plan.source.display()
                )),
            };
        Ok(LocationChange {
            server: self.server_config(plan.id)?,
            warning,
        })
    }

    /// 用户已经自行搬迁目录后，仅把同一 ServerId 重新指向新位置，不复制或删除任何文件。
    pub async fn relocate_server(
        self: &Arc<Self>,
        id: ServerId,
        destination: &Path,
    ) -> Result<LocationChange> {
        let _work = self.begin_server_work(id)?;
        let server = self.server_config(id)?;
        let destination = checked_destination(destination)?;
        if comparable_path(&server.working_dir) == comparable_path(&destination) {
            return Ok(LocationChange {
                server,
                warning: None,
            });
        }
        self.ensure_directory_available(id, &destination)?;
        ensure_real_directory(&destination, "新服务器目录")?;

        let process = self
            .process(id)
            .ok_or_else(|| AppError::not_found("服务器运行态不存在"))?;
        let _operation = process.lock_stopped().await?;
        self.ensure_no_tmux_session(&server).await?;

        let candidate = remapped_server(&server, &destination);
        validate_launchable_directory(&candidate)?;
        self.commit_location(id, &destination)?;
        Ok(LocationChange {
            server: self.server_config(id)?,
            warning: None,
        })
    }

    fn ensure_directory_available(&self, id: ServerId, destination: &Path) -> Result<()> {
        let config = self.config();
        let mut candidate = ServerConfig::new("目录检查", destination.to_path_buf());
        candidate.id = id;
        ensure_unique_working_dir(&config, &candidate, Some(id))
    }

    /// 移动、重新定位、备份恢复和删除服务器共用本检查，文案不绑定具体操作。
    pub(crate) async fn ensure_no_tmux_session(&self, server: &ServerConfig) -> Result<()> {
        let session = stable_session_name(server.id);
        match self.tmux.status(&session).await {
            SessionStatus::Exists => Err(AppError::conflict(
                "server_active",
                format!("检测到 tmux 会话 {session}，服务器可能仍在运行，操作已取消"),
            )),
            SessionStatus::Unknown => Err(AppError::unavailable(
                "tmux_status_unknown",
                "无法确认 tmux 会话状态，操作已取消",
            )),
            SessionStatus::Missing => Ok(()),
        }
    }

    /// 基于提交时的最新配置重映射路径，避免长时间复制覆盖并发修改的内存或 JVM 设置。
    fn commit_location(&self, id: ServerId, destination: &Path) -> Result<()> {
        let _mutation = self.mutation.lock();
        let mut next: AppConfig = (*self.config()).clone();
        let current = next
            .server(id)
            .cloned()
            .ok_or_else(|| AppError::not_found("服务器不存在"))?;
        let updated = remapped_server(&current, destination);
        validate_server(&updated)?;
        ensure_unique_working_dir(&next, &updated, Some(id))?;
        *next.server_mut(id).expect("上面已确认服务器存在") = updated;
        self.store.save(&next)?;
        *self.config.write() = Arc::new(next);
        Ok(())
    }
}

async fn attempt_rename(plan: &MovePlan) -> Result<RenameOutcome> {
    let source = plan.source.clone();
    let destination = plan.destination.clone();
    tokio::task::spawn_blocking(move || rename_directory(&source, &destination))
        .await
        .map_err(|error| AppError::internal(format!("目录移动任务异常: {error}")))?
}

async fn rollback_renamed_directory(plan: &MovePlan, config_error: AppError) -> AppError {
    let source = plan.destination.clone();
    let destination = plan.source.clone();
    let rollback = tokio::task::spawn_blocking(move || fs::rename(source, destination)).await;
    match rollback {
        Ok(Ok(())) => config_error,
        Ok(Err(rollback_error)) => AppError::internal(format!(
            "配置保存失败: {config_error}；目录回滚也失败: {rollback_error}；服务器文件当前位于 {}",
            plan.destination.display()
        )),
        Err(join_error) => AppError::internal(format!(
            "配置保存失败: {config_error}；目录回滚任务异常: {join_error}；请检查 {} 和 {}",
            plan.source.display(),
            plan.destination.display()
        )),
    }
}

fn checked_destination(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(AppError::invalid(
            "invalid_path",
            "目标服务器目录必须是非空绝对路径",
        ));
    }
    let path = normalize_lexical(path);
    validate_server(&ServerConfig::new("目录检查", path.clone()))?;
    Ok(path)
}

fn ensure_real_directory(path: &Path, description: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            AppError::not_found(format!("{description}不存在: {}", path.display()))
        } else {
            AppError::io(format!("无法检查{description} {}", path.display()), error)
        }
    })?;
    if !metadata.file_type().is_dir() {
        return Err(AppError::invalid(
            "invalid_path",
            format!("{description}必须是真实目录，不能是文件或符号链接"),
        ));
    }
    Ok(())
}

fn ensure_absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(AppError::already_exists(format!(
            "目标目录已存在，不会覆盖: {}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::io(
            format!("无法检查目标目录 {}", path.display()),
            error,
        )),
    }
}

enum RenameOutcome {
    Moved,
    CrossDevice,
}

fn rename_directory(source: &Path, destination: &Path) -> Result<RenameOutcome> {
    let parent = destination
        .parent()
        .ok_or_else(|| AppError::invalid("invalid_path", "目标目录没有有效父目录"))?;
    fs::create_dir_all(parent)
        .map_err(|error| AppError::io(format!("无法创建目标父目录 {}", parent.display()), error))?;
    // create_dir_all 后再次拒绝目标已存在，rename 在 Unix 上可能替换空目录。
    ensure_absent(destination)?;
    match fs::rename(source, destination) {
        Ok(()) => Ok(RenameOutcome::Moved),
        Err(error) if error.raw_os_error() == Some(libc::EXDEV) => Ok(RenameOutcome::CrossDevice),
        Err(error) => Err(AppError::io(
            format!(
                "无法移动服务器目录 {} 到 {}",
                source.display(),
                destination.display()
            ),
            error,
        )),
    }
}

/// 跨文件系统时先在目标端建立隐藏副本，校验通过后才出现正式目标目录；源目录保持不动。
fn copy_verified_directory(source: &Path, destination: &Path) -> Result<()> {
    ensure_absent(destination)?;
    let parent = destination
        .parent()
        .ok_or_else(|| AppError::invalid("invalid_path", "目标目录没有有效父目录"))?;
    fs::create_dir_all(parent)
        .map_err(|error| AppError::io(format!("无法创建目标父目录 {}", parent.display()), error))?;
    let name = destination
        .file_name()
        .ok_or_else(|| AppError::invalid("invalid_path", "目标目录名称无效"))?
        .to_string_lossy();
    let stage = parent.join(format!(".{name}.mcsm-move-{}", Uuid::new_v4().simple()));
    fs::create_dir(&stage).map_err(|error| {
        AppError::io(format!("无法创建移动暂存目录 {}", stage.display()), error)
    })?;
    let mut cleanup = DirectoryCleanup::new(stage.clone());
    copy_directory_contents(source, &stage)?;
    verify_directory_copy(source, &stage)?;
    let permissions = fs::metadata(source)
        .map_err(|error| AppError::io("无法读取源目录权限", error))?
        .permissions();
    fs::set_permissions(&stage, permissions)
        .map_err(|error| AppError::io("无法保留目标根目录权限", error))?;
    fs::rename(&stage, destination).map_err(|error| {
        AppError::io(
            format!("无法换入已校验副本 {}", destination.display()),
            error,
        )
    })?;
    cleanup.armed = false;
    Ok(())
}

struct DirectoryCleanup {
    path: PathBuf,
    armed: bool,
}

impl DirectoryCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }
}

impl Drop for DirectoryCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn remapped_server(current: &ServerConfig, destination: &Path) -> ServerConfig {
    let mut updated = current.clone();
    let source = current.working_dir.clone();
    updated.working_dir = destination.to_path_buf();
    updated.server_jar_path = remap_path(&updated.server_jar_path, &source, destination);
    updated.launch_args_file = remap_path(&updated.launch_args_file, &source, destination);
    updated.world_dir = remap_path(&updated.world_dir, &source, destination);
    updated.backup_dir = remap_path(&updated.backup_dir, &source, destination);
    updated
}

fn remap_path(path: &Path, old_root: &Path, new_root: &Path) -> PathBuf {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return path.to_path_buf();
    }
    let normalized = normalize_lexical(path);
    match normalized.strip_prefix(old_root) {
        Ok(relative) => new_root.join(relative),
        Err(_) => normalized,
    }
}

fn validate_launchable_directory(server: &ServerConfig) -> Result<()> {
    // 已配置的启动参数文件或 JAR 必须仍然存在：相对路径按新目录解析，目录外的绝对路径保持不变。
    // 目录里有其他核心不能替代原配置验证。
    if !server.launch_args_file.as_os_str().is_empty() {
        return if server.launch_args_path().is_some_and(|path| path.is_file()) {
            Ok(())
        } else {
            Err(AppError::invalid(
                "invalid_server_directory",
                "找不到原配置的启动参数文件",
            ))
        };
    }
    if !server.server_jar_path.as_os_str().is_empty() {
        return if server.jar_path().is_some_and(|path| path.is_file()) {
            Ok(())
        } else {
            Err(AppError::invalid(
                "invalid_server_directory",
                "找不到原配置的服务端 JAR",
            ))
        };
    }
    if detect::detect_directory(&server.working_dir).is_some() {
        Ok(())
    } else {
        Err(AppError::invalid(
            "invalid_server_directory",
            "新目录中没有可识别的服务端核心",
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use tempfile::TempDir;

    use super::*;
    use crate::{config::ConfigStore, core::ServerState, server::tmux::Tmux};

    fn manager(temp: &TempDir) -> Arc<ServerManager> {
        let root = temp.path();
        let store = ConfigStore::with_paths(root.join("config/config.json"), root.join("data"));
        let tmux = Arc::new(
            Tmux::isolated(
                root.join("runtime"),
                format!("mcsm-location-{}", Uuid::new_v4().simple()),
            )
            .unwrap(),
        );
        ServerManager::load_with_tmux(store, tmux).unwrap()
    }

    fn populated_server(manager: &ServerManager, root: &Path, name: &str) -> ServerConfig {
        let directory = root.join(name);
        fs::create_dir_all(directory.join("world/区域")).unwrap();
        fs::write(directory.join("server.jar"), "核心内容").unwrap();
        fs::write(directory.join("world/区域/方块.txt"), "世界内容").unwrap();
        let mut server = ServerConfig::new(name, directory);
        server.server_jar_path = "server.jar".into();
        manager.add_server(server).unwrap()
    }

    #[tokio::test]
    async fn move_preserves_identity_and_remaps_only_internal_paths() {
        let temp = TempDir::new().unwrap();
        let manager = manager(&temp);
        let mut server = populated_server(&manager, temp.path(), "中文 服务器");
        let id = server.id;
        let original = server.working_dir.clone();
        let external_world = temp.path().join("外部世界");
        fs::create_dir(&external_world).unwrap();
        server.world_dir = external_world.clone();
        server.backup_dir = server.working_dir.join("absolute-backups");
        manager.update_server(id, server).unwrap();

        let destination = temp.path().join("新位置/搬迁后 服务器");
        let change = manager.move_server(id, &destination).await.unwrap();
        assert_eq!(change.server.id, id);
        assert_eq!(change.server.working_dir, destination);
        assert_eq!(change.server.world_dir, external_world);
        assert_eq!(
            change.server.backup_dir,
            destination.join("absolute-backups")
        );
        assert_eq!(change.server.server_jar_path, PathBuf::from("server.jar"));
        assert_eq!(
            fs::read_to_string(destination.join("world/区域/方块.txt")).unwrap(),
            "世界内容"
        );

        assert!(!original.exists());
        assert_eq!(manager.state(id), ServerState::Stopped);
        assert_eq!(
            manager.config().default_server_dir,
            temp.path().join("data/servers")
        );
    }

    #[tokio::test]
    async fn same_filesystem_move_rolls_back_when_config_save_fails() {
        let temp = TempDir::new().unwrap();
        let manager = manager(&temp);
        let server = populated_server(&manager, temp.path(), "配置失败回滚");
        let source = server.working_dir.clone();
        let destination = temp.path().join("不应保留的目标");

        // 让原子配置保存确定性地失败，不依赖用户权限或真实配置。
        fs::remove_file(manager.store().config_path()).unwrap();
        fs::create_dir(manager.store().config_path()).unwrap();
        assert!(manager.move_server(server.id, &destination).await.is_err());

        assert_eq!(
            manager.server_config(server.id).unwrap().working_dir,
            source
        );
        assert_eq!(
            fs::read_to_string(source.join("world/区域/方块.txt")).unwrap(),
            "世界内容"
        );
        assert!(!destination.exists());
    }

    #[tokio::test]
    async fn cross_filesystem_move_copies_verifies_and_cleans_source() {
        use std::os::unix::fs::MetadataExt;

        let temp = TempDir::new().unwrap();
        let Ok(target_parent) = TempDir::new_in("/dev/shm") else {
            eprintln!("跳过跨文件系统测试：/dev/shm 不可用");
            return;
        };
        if fs::metadata(temp.path()).unwrap().dev()
            == fs::metadata(target_parent.path()).unwrap().dev()
        {
            eprintln!("跳过跨文件系统测试：临时目录与 /dev/shm 位于同一文件系统");
            return;
        }
        let manager = manager(&temp);
        let server = populated_server(&manager, temp.path(), "跨文件系统");
        let source = server.working_dir.clone();
        let destination = target_parent.path().join("移动后的服务器");

        let change = manager.move_server(server.id, &destination).await.unwrap();
        assert_eq!(change.warning, None);
        assert_eq!(change.server.id, server.id);
        assert_eq!(change.server.working_dir, destination);
        assert_eq!(
            fs::read_to_string(destination.join("world/区域/方块.txt")).unwrap(),
            "世界内容"
        );
        assert!(!source.exists());
        assert_eq!(fs::read_dir(target_parent.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn relocate_only_changes_registration_after_user_moved_files() {
        let temp = TempDir::new().unwrap();
        let manager = manager(&temp);
        let server = populated_server(&manager, temp.path(), "原目录");
        let destination = temp.path().join("手动搬迁");
        fs::rename(&server.working_dir, &destination).unwrap();

        let change = manager
            .relocate_server(server.id, &destination)
            .await
            .unwrap();
        assert_eq!(change.server.id, server.id);
        assert_eq!(change.server.working_dir, destination);
        assert_eq!(change.server.server_jar_path, PathBuf::from("server.jar"));
        assert_eq!(
            fs::read_to_string(destination.join("server.jar")).unwrap(),
            "核心内容"
        );
    }

    #[tokio::test]
    async fn move_refuses_existing_or_duplicate_destinations() {
        let temp = TempDir::new().unwrap();
        let manager = manager(&temp);
        let server = populated_server(&manager, temp.path(), "源目录");
        let other = populated_server(&manager, temp.path(), "另一目录");
        let original = server.working_dir.clone();

        let error = manager
            .move_server(server.id, &other.working_dir)
            .await
            .unwrap_err();
        assert_eq!(error.code, "already_exists");
        let existing = temp.path().join("已有目标");
        fs::create_dir(&existing).unwrap();
        assert_eq!(
            manager
                .move_server(server.id, &existing)
                .await
                .unwrap_err()
                .code,
            "already_exists"
        );
        assert_eq!(
            manager.server_config(server.id).unwrap().working_dir,
            original
        );
        assert!(original.join("server.jar").is_file());
    }

    #[test]
    fn verified_copy_keeps_source_and_preserves_symlinks() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("源");
        let destination = temp.path().join("目标");
        fs::create_dir_all(source.join("目录")).unwrap();
        fs::write(source.join("目录/文件.bin"), [0, 1, 2, 255]).unwrap();
        symlink("目录/文件.bin", source.join("链接")).unwrap();

        copy_verified_directory(&source, &destination).unwrap();
        verify_directory_copy(&source, &destination).unwrap();
        assert!(source.join("目录/文件.bin").is_file());
        assert_eq!(
            fs::read_link(destination.join("链接")).unwrap(),
            PathBuf::from("目录/文件.bin")
        );
    }

    #[tokio::test]
    async fn work_lease_blocks_lifecycle_and_second_location_task() {
        let temp = TempDir::new().unwrap();
        let manager = manager(&temp);
        let server = populated_server(&manager, temp.path(), "忙碌");
        let lease = manager.begin_server_work(server.id).unwrap();
        assert_eq!(
            manager
                .start_server(server.id, true)
                .await
                .unwrap_err()
                .code,
            "busy"
        );
        assert_eq!(
            manager
                .move_server(server.id, &temp.path().join("新目录"))
                .await
                .unwrap_err()
                .code,
            "busy"
        );
        drop(lease);
        assert!(!manager.has_server_work(server.id));
    }
}
