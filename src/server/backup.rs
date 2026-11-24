use std::{
    fs, io,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use chrono::Local;

use super::fs_copy::{copy_directory_contents, verify_directory_copy};

use crate::{
    config::ServerConfig,
    error::{AppError, Result},
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// 按名称倒序列出真实备份目录；尚未创建备份目录时返回空列表。
pub fn list(server: &ServerConfig) -> Result<Vec<String>> {
    let backup_dir = server.backup_dir();
    let metadata = match fs::metadata(&backup_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(AppError::io(
                format!("无法读取备份目录 {}", backup_dir.display()),
                error,
            ));
        }
    };
    if !metadata.is_dir() {
        return Err(invalid_path("备份路径不是目录"));
    }

    let reader = fs::read_dir(&backup_dir).map_err(|error| {
        AppError::io(format!("无法列举备份目录 {}", backup_dir.display()), error)
    })?;
    let mut backups = Vec::new();
    for entry in reader {
        let entry = entry.map_err(|error| AppError::io("读取备份条目失败", error))?;
        // 不把符号链接当作备份，避免恢复或删除时引用目录外内容。
        if !entry
            .file_type()
            .map_err(|error| AppError::io("读取备份条目类型失败", error))?
            .is_dir()
        {
            continue;
        }
        // 服务生成的备份名必为 UTF-8；无法安全回传的其他目录名不列入 API。
        if let Ok(name) = entry.file_name().into_string() {
            backups.push(name);
        }
    }
    backups.sort_by(|left, right| right.cmp(left));
    Ok(backups)
}

/// 创建世界目录快照。服务器是否已停止由调用方在生命周期锁中保证。
pub fn create(server: &ServerConfig) -> Result<String> {
    let world_dir = server.world_dir();
    let world_metadata = fs::metadata(&world_dir).map_err(|error| {
        map_not_found(
            error,
            || AppError::not_found("世界目录不存在"),
            "无法读取世界目录",
        )
    })?;
    if !world_metadata.is_dir() {
        return Err(invalid_path("世界路径不是目录"));
    }

    let backup_dir = server.backup_dir();
    fs::create_dir_all(&backup_dir).map_err(|error| {
        AppError::io(format!("无法创建备份目录 {}", backup_dir.display()), error)
    })?;
    // 备份目录位于世界内会把暂存目录再次复制，必须显式拒绝。
    let canonical_world = world_dir
        .canonicalize()
        .map_err(|error| AppError::io("无法解析世界目录", error))?;
    let canonical_backups = backup_dir
        .canonicalize()
        .map_err(|error| AppError::io("无法解析备份目录", error))?;
    if canonical_backups.starts_with(&canonical_world) {
        return Err(invalid_path("备份目录不能位于世界目录内"));
    }

    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    for index in 0..10_000 {
        let name = if index == 0 {
            format!("backup_{timestamp}")
        } else {
            // 同一秒连续创建时仍保留可读时间戳，并以序号保证名称唯一。
            format!("backup_{timestamp}_{index:03}")
        };
        let final_path = backup_dir.join(&name);
        match fs::symlink_metadata(&final_path) {
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(AppError::io(
                    format!("无法检查备份 {}", final_path.display()),
                    error,
                ));
            }
        }

        let stage = temporary_path(&backup_dir, &name, "create");
        match fs::create_dir(&stage) {
            Ok(()) => {
                let mut cleanup = TempDirCleanup::new(stage.clone());
                copy_directory_contents(&world_dir, &stage)?;
                fs::rename(&stage, &final_path).map_err(|error| match error.kind() {
                    io::ErrorKind::AlreadyExists => {
                        AppError::already_exists("同名备份已经存在，请重试")
                    }
                    _ => AppError::io(format!("无法完成备份 {}", final_path.display()), error),
                })?;
                cleanup.disarm();
                return Ok(name);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(AppError::io(
                    format!("无法创建备份暂存目录 {}", stage.display()),
                    error,
                ));
            }
        }
    }
    Err(AppError::conflict(
        "backup_name_unavailable",
        "无法分配唯一备份名，请稍后重试",
    ))
}

/// 先复制并校验暂存副本，再原子换入世界目录；换入失败时恢复原世界。
pub fn restore(server: &ServerConfig, backup_name: &str) -> Result<()> {
    let backup = checked_backup_path(server, backup_name)?;
    let world_dir = server.world_dir();
    let world_parent = world_dir
        .parent()
        .ok_or_else(|| invalid_path("不能把文件系统根目录作为世界目录"))?;
    let world_parent = if world_parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        world_parent
    };
    let world_name = world_dir
        .file_name()
        .ok_or_else(|| invalid_path("世界目录名称无效"))?
        .to_string_lossy()
        .into_owned();

    let had_world = match fs::symlink_metadata(&world_dir) {
        Ok(metadata) if metadata.file_type().is_dir() => true,
        Ok(_) => return Err(invalid_path("世界路径必须是真实目录，不能是文件或符号链接")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(AppError::io(
                format!("无法检查世界目录 {}", world_dir.display()),
                error,
            ));
        }
    };
    fs::create_dir_all(world_parent).map_err(|error| {
        AppError::io(
            format!("无法创建世界父目录 {}", world_parent.display()),
            error,
        )
    })?;

    // 暂存目录与世界目录位于同一文件系统，后续 rename 才是原子换入。
    let stage = temporary_path(world_parent, &world_name, "restore");
    fs::create_dir(&stage).map_err(|error| match error.kind() {
        io::ErrorKind::AlreadyExists => {
            AppError::conflict("temporary_exists", "恢复暂存目录已存在，请重试")
        }
        _ => AppError::io(format!("无法创建恢复暂存目录 {}", stage.display()), error),
    })?;
    let mut stage_cleanup = TempDirCleanup::new(stage.clone());
    copy_directory_contents(&backup, &stage)?;
    verify_directory_copy(&backup, &stage)?;

    let old_world = had_world
        .then(|| unused_temporary_path(world_parent, &world_name, "old"))
        .transpose()?;
    if let Some(old_world) = &old_world {
        // 原世界只改名、不删除；在校验或换入失败时仍能完整回滚。
        fs::rename(&world_dir, old_world).map_err(|error| {
            AppError::io(format!("无法暂存旧世界目录 {}", world_dir.display()), error)
        })?;
    }

    if let Err(error) = fs::rename(&stage, &world_dir) {
        if let Some(old_world) = &old_world
            && let Err(rollback_error) = fs::rename(old_world, &world_dir)
        {
            return Err(AppError::internal(format!(
                "恢复换入失败: {error}；旧世界回滚也失败: {rollback_error}；旧世界仍保留在 {}",
                old_world.display()
            )));
        }
        return Err(AppError::io("无法换入已校验的世界副本", error));
    }
    stage_cleanup.disarm();

    // 新世界已经成功换入。清理失败只遗留隐藏旧目录，不能把已成功恢复误报为失败。
    if let Some(old_world) = old_world {
        let _ = fs::remove_dir_all(old_world);
    }
    Ok(())
}

pub fn delete(server: &ServerConfig, backup_name: &str) -> Result<()> {
    let backup = checked_backup_path(server, backup_name)?;
    fs::remove_dir_all(&backup).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => AppError::not_found("备份不存在"),
        _ => AppError::io(format!("无法删除备份 {}", backup.display()), error),
    })
}

fn checked_backup_path(server: &ServerConfig, backup_name: &str) -> Result<PathBuf> {
    validate_backup_name(backup_name)?;
    let path = server.backup_dir().join(backup_name);
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        map_not_found(
            error,
            || AppError::not_found("备份不存在"),
            "无法读取备份信息",
        )
    })?;
    if !metadata.file_type().is_dir() {
        return Err(invalid_path("备份必须是真实目录，不能是文件或符号链接"));
    }
    Ok(path)
}

fn validate_backup_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.as_bytes().contains(&0)
        || name.contains('/')
        || name.contains('\\')
    {
        return Err(invalid_path("备份名必须是非空的纯目录名"));
    }
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(invalid_path("备份名不能包含路径"));
    }
    Ok(())
}

fn temporary_path(parent: &Path, stem: &str, purpose: &str) -> PathBuf {
    let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".{stem}.mcsm-{purpose}-{}-{sequence}",
        std::process::id()
    ))
}

fn unused_temporary_path(parent: &Path, stem: &str, purpose: &str) -> Result<PathBuf> {
    for _ in 0..1000 {
        let path = temporary_path(parent, stem, purpose);
        match fs::symlink_metadata(&path) {
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(path),
            Err(error) => {
                return Err(AppError::io(
                    format!("无法检查临时路径 {}", path.display()),
                    error,
                ));
            }
        }
    }
    Err(AppError::conflict(
        "temporary_exists",
        "无法分配临时目录，请清理旧临时文件后重试",
    ))
}

struct TempDirCleanup {
    path: PathBuf,
    armed: bool,
}

impl TempDirCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempDirCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn invalid_path(message: impl Into<String>) -> AppError {
    AppError::invalid("invalid_path", message)
}

fn map_not_found(
    error: io::Error,
    not_found: impl FnOnce() -> AppError,
    context: impl std::fmt::Display,
) -> AppError {
    if error.kind() == io::ErrorKind::NotFound {
        not_found()
    } else {
        AppError::io(context, error)
    }
}

#[cfg(test)]
mod tests;
