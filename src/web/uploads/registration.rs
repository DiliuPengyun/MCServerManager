use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

use super::super::shared::errors::validate_server_name;
use crate::{
    config::ServerConfig,
    core::{ServerManager, provisioning},
    error::{AppError, ErrorKind, Result},
    server::detect::DetectedCore,
};

pub(super) fn configured_server(
    body: &Value,
    working_dir: PathBuf,
    detected: DetectedCore,
) -> Result<ServerConfig> {
    let name = body.get("name").and_then(Value::as_str).unwrap_or("");
    validate_server_name(name)?;
    let mut server = ServerConfig::new(name, working_dir.clone());
    server.java_path = body
        .get("java_path")
        .and_then(Value::as_str)
        .filter(|java| !java.is_empty())
        .unwrap_or("java")
        .to_owned();
    server.min_memory_mb = body
        .get("min_memory_mb")
        .and_then(Value::as_i64)
        .map(|value| i32::try_from(value.max(0)).unwrap_or(i32::MAX))
        .unwrap_or(1024);
    server.max_memory_mb = body
        .get("max_memory_mb")
        .and_then(Value::as_i64)
        .map(|value| i32::try_from(value.max(0)).unwrap_or(i32::MAX))
        .unwrap_or(4096);
    server.additional_jvm_args = body
        .get("jvm_args")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    provisioning::apply_detection_to_config(&mut server, detected);
    server.backup_dir = working_dir.join("backups");
    server.world_dir = working_dir.join("world");
    Ok(server)
}

pub(super) fn target_for_name(
    manager: &std::sync::Arc<ServerManager>,
    body: &Value,
) -> Result<PathBuf> {
    let name = body.get("name").and_then(Value::as_str).unwrap_or("");
    ensure_name_available(manager, name)?;
    let parent = manager.config().default_server_dir.clone();
    fs::create_dir_all(&parent).map_err(|error| AppError::io("无法创建默认服务器目录", error))?;
    let target = parent.join(name);
    ensure_target_available(manager, &target)?;
    Ok(target)
}

pub(super) fn ensure_name_available(manager: &ServerManager, name: &str) -> Result<()> {
    validate_server_name(name)?;
    if manager
        .config()
        .servers
        .iter()
        .any(|server| server.name == name)
    {
        Err(AppError::new(
            ErrorKind::AlreadyExists,
            "target_exists",
            "服务器名称已存在，请更换名称",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn ensure_target_available(manager: &ServerManager, target: &Path) -> Result<()> {
    let target_comparable = target
        .canonicalize()
        .unwrap_or_else(|_| target.to_path_buf());
    if target.exists()
        || manager.config().servers.iter().any(|server| {
            server
                .working_dir
                .canonicalize()
                .unwrap_or_else(|_| server.working_dir.clone())
                == target_comparable
        })
    {
        Err(AppError::new(
            ErrorKind::AlreadyExists,
            "target_exists",
            "服务器名称或目标目录已被使用",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::config::CoreType;

    #[test]
    fn oversized_memory_values_saturate_instead_of_wrapping() {
        let server = configured_server(
            &json!({
                "name": "大内存",
                "min_memory_mb": i64::MAX,
                "max_memory_mb": i64::MAX,
            }),
            PathBuf::from("/tmp/大内存"),
            DetectedCore {
                core_type: CoreType::Custom,
                minecraft_version: None,
                core_version: None,
                server_jar_path: None,
                launch_args_file: None,
                installer_jar: None,
            },
        )
        .unwrap();
        assert_eq!(server.min_memory_mb, i32::MAX);
        assert_eq!(server.max_memory_mb, i32::MAX);
    }
}
