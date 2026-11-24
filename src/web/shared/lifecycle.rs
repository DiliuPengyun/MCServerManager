use std::sync::Arc;

use crate::{
    config::ServerId,
    core::{ServerManager, ServerState, StartResult},
    error::Result,
};

pub(crate) async fn start(
    manager: &ServerManager,
    id: ServerId,
    allow_no_rcon: bool,
) -> Result<StartResult> {
    manager.start_server(id, allow_no_rcon).await
}

pub(crate) async fn agree_eula(manager: &ServerManager, id: ServerId) -> Result<StartResult> {
    manager.agree_eula_and_start(id).await
}

pub(crate) async fn enable_rcon(manager: &ServerManager, id: ServerId) -> Result<StartResult> {
    manager.enable_rcon_and_start(id).await
}

pub(crate) async fn stop(manager: &ServerManager, id: ServerId) -> Result<ServerState> {
    manager.stop_server(id).await
}

pub(crate) async fn restart(
    manager: &ServerManager,
    id: ServerId,
    allow_no_rcon: bool,
) -> Result<StartResult> {
    manager.restart_server(id, allow_no_rcon).await
}

/// 默认仅注销；磁盘删除必须是调用方已经确认的显式选择。
pub(crate) async fn remove_registered(
    manager: &Arc<ServerManager>,
    id: ServerId,
    delete_files: bool,
) -> Result<()> {
    manager.remove_registered_server(id, delete_files).await
}
