use serde_json::{Value, json};

use crate::{
    config::ServerId,
    core::{ServerManager, StartResult},
    error::{AppError, Result},
};

/// 服务器列表的完整 JSON 映射。
pub(crate) fn servers_json(manager: &ServerManager) -> Result<Value> {
    let ids: Vec<_> = manager
        .config()
        .servers
        .iter()
        .map(|server| server.id)
        .collect();
    let mut output = Vec::with_capacity(ids.len());
    for id in ids {
        output.push(server_json(manager, id)?);
    }
    Ok(Value::Array(output))
}

/// 单个服务器的完整 JSON 映射；`index` 仅供客户端本地选择，不用于 URL。
pub(crate) fn server_json(manager: &ServerManager, id: ServerId) -> Result<Value> {
    let config = manager.config();
    let server = config
        .server(id)
        .ok_or_else(|| AppError::not_found("服务器不存在"))?;
    let index = config.index_of(id).unwrap_or(0);
    Ok(json!({
        "index": index,
        "id": id.to_string(),
        "name": server.name,
        "core_id": server.core_type.id(),
        "core_name": server.core_type.display_name(),
        "mc_version": server.minecraft_version,
        "core_version": server.core_version,
        "working_dir": server.working_dir.to_string_lossy(),
        "server_jar_path": server.server_jar_path.to_string_lossy(),
        "launch_args_file": server.launch_args_file.to_string_lossy(),
        "java_path": server.java_path,
        "min_memory_mb": server.min_memory_mb,
        "max_memory_mb": server.max_memory_mb,
        "additional_jvm_args": server.additional_jvm_args,
        "backup_dir": server.backup_dir.to_string_lossy(),
        "world_dir": server.world_dir.to_string_lossy(),
        "rcon_port": server.rcon_port,
        "state": manager.state(id).as_str(),
        "rcon": server.rcon_available(),
        "players": manager.players(id),
    }))
}

/// 启动结果映射为 JSON 或结构化冲突错误。
pub(crate) fn start_value(result: StartResult) -> Result<Value> {
    match result {
        StartResult::Started => Ok(json!({"ok":true})),
        StartResult::AlreadyRunning => Ok(json!({"ok":true,"note":"already_running"})),
        StartResult::InstallerRequired => Err(AppError::conflict(
            "installer_required",
            "需要先运行整合包安装器",
        )),
        StartResult::EulaRequired => Err(AppError::conflict(
            "eula_required",
            "需要用户明确同意 Minecraft EULA",
        )),
        StartResult::RconRequired => Err(AppError::conflict(
            "rcon_required",
            "需要用户选择是否启用 RCON",
        )),
    }
}
