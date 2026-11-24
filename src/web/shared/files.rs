use serde_json::{Value, json};

use crate::{config::ServerId, core::ServerManager, error::Result, server::files::FileBrowser};

use super::errors::file_error;

/// 所有文件用例都通过 `FileBrowser` 的安全方法解析相对路径。
pub(crate) fn list(manager: &ServerManager, id: ServerId, path: String) -> Result<Value> {
    let server = manager.server_config(id)?;
    let entries = FileBrowser::new(server.working_dir)
        .list(&path)
        .map_err(file_error)?;
    Ok(json!({"path": path, "entries": entries}))
}

pub(crate) fn read(manager: &ServerManager, id: ServerId, path: &str) -> Result<Value> {
    let server = manager.server_config(id)?;
    let content = FileBrowser::new(server.working_dir)
        .read_text(path)
        .map_err(file_error)?;
    Ok(json!({"path": path, "content": content}))
}

pub(crate) fn write(
    manager: &ServerManager,
    id: ServerId,
    path: &str,
    content: &str,
) -> Result<Value> {
    let server = manager.server_config(id)?;
    FileBrowser::new(server.working_dir)
        .write_text(path, content)
        .map_err(file_error)?;
    Ok(json!({"ok": true}))
}

pub(crate) fn create(
    manager: &ServerManager,
    id: ServerId,
    path: &str,
    is_dir: bool,
) -> Result<Value> {
    let server = manager.server_config(id)?;
    FileBrowser::new(server.working_dir)
        .create(path, is_dir)
        .map_err(file_error)?;
    Ok(json!({"ok": true}))
}

pub(crate) fn rename(
    manager: &ServerManager,
    id: ServerId,
    path: &str,
    new_name: &str,
) -> Result<Value> {
    let server = manager.server_config(id)?;
    FileBrowser::new(server.working_dir)
        .rename(path, new_name)
        .map_err(file_error)?;
    Ok(json!({"ok": true}))
}

pub(crate) fn remove(manager: &ServerManager, id: ServerId, path: &str) -> Result<Value> {
    let server = manager.server_config(id)?;
    FileBrowser::new(server.working_dir)
        .remove(path)
        .map_err(file_error)?;
    Ok(json!({"ok": true}))
}
