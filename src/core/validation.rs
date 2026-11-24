use std::path::{Path, PathBuf};

use crate::{
    config::{AppConfig, ServerConfig, ServerId},
    error::{AppError, Result},
};

pub(super) fn validate_player_name(player: &str) -> Result<()> {
    if player.is_empty()
        || player.len() > 64
        || player
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(AppError::invalid("invalid_player", "玩家名称无效"));
    }
    Ok(())
}

pub(super) fn validate_server(server: &ServerConfig) -> Result<()> {
    if server.name.trim().is_empty() || server.name.chars().any(char::is_control) {
        return Err(AppError::invalid(
            "invalid_name",
            "服务器名称不能为空或包含控制字符",
        ));
    }
    if server.working_dir.as_os_str().is_empty() || !server.working_dir.is_absolute() {
        return Err(AppError::invalid(
            "invalid_path",
            "服务器工作目录必须是绝对路径",
        ));
    }
    if server.working_dir.parent().is_none() {
        return Err(AppError::invalid(
            "invalid_path",
            "不能把文件系统根目录作为服务器目录",
        ));
    }
    if server.min_memory_mb < 0 || server.max_memory_mb < 0 {
        return Err(AppError::invalid("invalid_memory", "内存不能为负数"));
    }
    if server.min_memory_mb > server.max_memory_mb {
        return Err(AppError::invalid(
            "invalid_memory",
            "最小内存不能大于最大内存",
        ));
    }
    Ok(())
}

pub(super) fn ensure_unique_working_dir(
    config: &AppConfig,
    server: &ServerConfig,
    ignored: Option<ServerId>,
) -> Result<()> {
    let candidate = comparable_path(&server.working_dir);
    if config.servers.iter().any(|existing| {
        Some(existing.id) != ignored && comparable_path(&existing.working_dir) == candidate
    }) {
        return Err(AppError::already_exists("该目录已被另一台服务器注册"));
    }
    Ok(())
}

pub(super) fn comparable_path(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|_| normalize_lexical(path))
}

pub(super) fn normalize_lexical(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}
