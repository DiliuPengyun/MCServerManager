use serde_json::{Value, json};

use crate::{
    config::ServerId,
    core::ServerManager,
    error::{AppError, ErrorKind, Result},
    server::properties::ServerProperties,
};

/// 读取 `server.properties`，保留条目顺序并维持空文件契约。
pub(crate) fn get(manager: &ServerManager, id: ServerId) -> Result<Value> {
    let server = manager.server_config(id)?;
    let path = server.working_dir.join("server.properties");
    if !path.exists() {
        return Ok(json!({"exists": false, "entries": {}, "order": []}));
    }
    let properties = ServerProperties::load(path)?;
    let mut entries = serde_json::Map::new();
    for (key, value) in properties.entries() {
        entries.insert(key.to_owned(), json!(value));
    }
    Ok(json!({
        "exists": true,
        "entries": entries,
        "order": properties.ordered_keys(),
    }))
}

/// 写入 `server.properties`；只有字符串值会被设置，其余类型忽略。
pub(crate) fn set(manager: &ServerManager, id: ServerId, body: &Value) -> Result<Value> {
    let entries = body
        .get("entries")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::invalid("bad_request", "entries 必须是对象"))?;
    let server = manager.server_config(id)?;
    let path = server.working_dir.join("server.properties");
    let mut properties = if path.exists() {
        ServerProperties::load(&path)?
    } else {
        ServerProperties::default()
    };
    for (key, value) in entries {
        if let Some(value) = value.as_str() {
            properties.set(key, value)?;
        }
    }
    properties
        .save(path)
        .map_err(|error| AppError::new(ErrorKind::Io, "save_failed", error.message))?;
    Ok(json!({"ok": true}))
}
