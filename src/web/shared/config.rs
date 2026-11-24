use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use crate::{
    config::ServerId,
    core::ServerManager,
    error::{AppError, Result},
};

use super::errors::validate_server_name;

/// 服务器配置更新。
pub(crate) fn update_server_config(
    manager: &ServerManager,
    id: ServerId,
    body: &Value,
) -> Result<Value> {
    let mut server = manager.server_config(id)?;
    if let Some(name) = body.get("name").and_then(Value::as_str) {
        validate_server_name(name)?;
        server.name = name.to_owned();
    }
    if let Some(value) = body.get("java_path").and_then(Value::as_str) {
        server.java_path = value.to_owned();
    }
    if let Some(value) = body.get("additional_jvm_args").and_then(Value::as_str) {
        server.additional_jvm_args = value.to_owned();
    }
    if let Some(value) = body.get("backup_dir").and_then(Value::as_str) {
        server.backup_dir = PathBuf::from(value);
    }
    if let Some(value) = body.get("world_dir").and_then(Value::as_str) {
        server.world_dir = PathBuf::from(value);
    }
    if let Some(value) = body.get("min_memory_mb").and_then(Value::as_i64) {
        server.min_memory_mb = i32::try_from(value.max(0)).unwrap_or(i32::MAX);
    }
    if let Some(value) = body.get("max_memory_mb").and_then(Value::as_i64) {
        server.max_memory_mb = i32::try_from(value.max(0)).unwrap_or(i32::MAX);
    }
    manager.update_server(id, server)?;
    Ok(json!({"ok": true}))
}

/// 设置页面的只读映射。
pub(crate) fn settings_json(manager: &ServerManager) -> Value {
    let config = manager.config();
    let java = if config.global_java_path.is_empty() {
        "java"
    } else {
        &config.global_java_path
    };
    json!({
        "servers_parent_folder": config.default_server_dir.to_string_lossy(),
        "default_server_dir": config.default_server_dir.to_string_lossy(),
        "effective_parent_folder": config.default_server_dir.to_string_lossy(),
        "global_java_path": config.global_java_path,
        "effective_java_path": java,
        "web_port": config.web_port,
    })
}

/// 设置更新：端口校验、默认目录、全局 Java 路径和待生效端口。
pub(crate) fn update_settings(manager: &ServerManager, body: &Value) -> Result<Value> {
    if let Some(port) = body.get("web_port").and_then(Value::as_u64)
        && !(1..=65535).contains(&port)
    {
        return Err(AppError::invalid("invalid_port", "端口无效"));
    }
    if let Some(path) = body
        .get("default_server_dir")
        .or_else(|| body.get("servers_parent_folder"))
        .and_then(Value::as_str)
    {
        manager.update_default_server_dir(Path::new(path))?;
    }
    if let Some(java) = body.get("global_java_path").and_then(Value::as_str) {
        manager.update_global_java_path(java.to_owned())?;
    }
    // 只有端口确实变化才提示重启，避免仅修改目录或 Java 路径时也提示需要重启。
    let mut note = None;
    if let Some(port) = body.get("web_port").and_then(Value::as_u64) {
        if port != u64::from(manager.config().web_port) {
            note = Some("web_port 需重启 web 服务后生效");
        }
        manager.update_web_port(port as u16)?;
    }
    Ok(match note {
        Some(note) => json!({"ok": true, "note": note}),
        None => json!({"ok": true}),
    })
}

/// 主机目录浏览；不依赖服务器注册表。
pub(crate) fn browse_host(query: &HashMap<String, String>) -> Result<Value> {
    let requested = query
        .get("path")
        .filter(|path| !path.is_empty())
        .cloned()
        .or_else(|| std::env::var_os("HOME").map(|home| home.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "/".into());
    let show_files = query.get("show_files").is_some_and(|value| value == "1");
    let directory = fs::canonicalize(requested)
        .map_err(|_| AppError::invalid("invalid_path", "目录不存在或不可访问"))?;
    if !directory.is_dir() {
        return Err(AppError::invalid("invalid_path", "目录不存在或不可访问"));
    }
    let mut entries = Vec::new();
    for entry in
        fs::read_dir(&directory).map_err(|error| AppError::io("无法列举主机目录", error))?
    {
        let entry = entry.map_err(|error| AppError::io("读取主机目录条目失败", error))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        if !is_dir && !show_files {
            continue;
        }
        entries.push(json!({"name": name, "is_dir": is_dir}));
    }
    entries.sort_by(|left, right| {
        right["is_dir"]
            .as_bool()
            .cmp(&left["is_dir"].as_bool())
            .then_with(|| left["name"].as_str().cmp(&right["name"].as_str()))
    });
    let parent = directory.parent().unwrap_or(&directory);
    Ok(json!({
        "path": directory.to_string_lossy(),
        "parent": parent.to_string_lossy(),
        "entries": entries,
    }))
}
