use std::sync::Arc;

use crate::{AppError, core::ServerManager};

use super::super::{Refresh, SimpleCommand, TaskResult};
use super::{blocking, error_text};

pub(in crate::ui::tasks) async fn run(
    manager: Arc<ServerManager>,
    command: SimpleCommand,
) -> TaskResult<(String, Refresh)> {
    match command {
        SimpleCommand::SaveGlobal {
            default_dir,
            java_path,
        } => {
            blocking(move || {
                manager.update_default_server_dir(&default_dir)?;
                manager.update_global_java_path(java_path)?;
                Ok::<(), AppError>(())
            })
            .await?;
            Ok((
                "全局设置已保存；已有服务器目录未改变".to_owned(),
                Refresh::None,
            ))
        }
        SimpleCommand::SaveServer { id, config } => {
            blocking(move || manager.update_server(id, *config).map(|_| ())).await?;
            Ok(("服务器设置已保存".to_owned(), Refresh::ServerData(id)))
        }
        SimpleCommand::MoveServer {
            id,
            destination,
            relocate,
        } => {
            let change = if relocate {
                manager.relocate_server(id, &destination).await
            } else {
                manager.move_server(id, &destination).await
            }
            .map_err(error_text)?;
            let mut message = if relocate {
                format!("已重新指向 {}", change.server.working_dir.display())
            } else {
                format!("服务器已移动到 {}", change.server.working_dir.display())
            };
            if let Some(warning) = change.warning {
                message.push_str(&format!("；警告：{warning}"));
            }
            Ok((message, Refresh::ServerData(id)))
        }
        SimpleCommand::RemoveServer { id, delete_files } => {
            manager
                .remove_server(id, delete_files)
                .await
                .map_err(error_text)?;
            Ok((
                if delete_files {
                    "服务器配置与文件已删除"
                } else {
                    "服务器已移除，磁盘文件保留"
                }
                .to_owned(),
                Refresh::ServerRemoved(id),
            ))
        }
        SimpleCommand::SendCommand { id, command } => {
            manager
                .send_command(id, &command)
                .await
                .map_err(error_text)?;
            Ok((format!("指令已发送：{command}"), Refresh::None))
        }
        SimpleCommand::KickPlayer { id, player } => {
            manager.kick_player(id, &player).await.map_err(error_text)?;
            Ok((format!("已请求踢出 {player}"), Refresh::None))
        }
        SimpleCommand::BanPlayer { id, player } => {
            manager.ban_player(id, &player).await.map_err(error_text)?;
            Ok((format!("已请求封禁 {player}"), Refresh::None))
        }
        SimpleCommand::ClearConsole(id) => {
            let console = manager
                .console(id)
                .ok_or_else(|| "控制台不存在".to_owned())?;
            console.clear();
            Ok(("控制台已清空".to_owned(), Refresh::None))
        }
    }
}
