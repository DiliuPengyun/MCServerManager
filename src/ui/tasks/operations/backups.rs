use std::sync::Arc;

use crate::{config::ServerId, core::ServerManager, server::backup};

use super::super::TaskResult;
use super::{blocking, error_text};

pub(in crate::ui::tasks) async fn list(
    manager: &Arc<ServerManager>,
    id: ServerId,
) -> TaskResult<Vec<String>> {
    let server = manager.server_config(id).map_err(error_text)?;
    blocking(move || backup::list(&server)).await
}

pub(in crate::ui::tasks) async fn create(
    manager: &Arc<ServerManager>,
    id: ServerId,
) -> TaskResult<(String, Vec<String>)> {
    let name = manager.create_backup(id).await.map_err(error_text)?;
    let items = list(manager, id).await?;
    Ok((name, items))
}

pub(in crate::ui::tasks) async fn change(
    manager: Arc<ServerManager>,
    id: ServerId,
    name: String,
    restore: bool,
) -> TaskResult<(String, Vec<String>)> {
    if restore {
        manager
            .restore_backup(id, &name)
            .await
            .map_err(error_text)?;
    } else {
        manager.delete_backup(id, &name).await.map_err(error_text)?;
    }
    let items = list(&manager, id).await?;
    Ok((
        if restore {
            format!("已从 {name} 恢复世界")
        } else {
            format!("已删除备份 {name}")
        },
        items,
    ))
}
