use serde_json::{Value, json};

use crate::{config::ServerId, core::ServerManager, error::Result, server::backup};

use super::errors::join_error;

/// 列出服务器备份；阻塞 I/O 放在 `spawn_blocking`。
pub(crate) async fn list(manager: &ServerManager, id: ServerId) -> Result<Value> {
    let server = manager.server_config(id)?;
    let backups = tokio::task::spawn_blocking(move || backup::list(&server))
        .await
        .map_err(join_error)??;
    Ok(json!({"backups": backups}))
}

pub(crate) async fn create(manager: &ServerManager, id: ServerId) -> Result<Value> {
    let name = manager.create_backup(id).await?;
    Ok(json!({"ok": true, "name": name}))
}

pub(crate) async fn restore(manager: &ServerManager, id: ServerId, name: &str) -> Result<Value> {
    manager.restore_backup(id, name).await?;
    Ok(json!({"ok": true}))
}

pub(crate) async fn delete(manager: &ServerManager, id: ServerId, name: &str) -> Result<Value> {
    manager.delete_backup(id, name).await?;
    Ok(json!({"ok": true}))
}
