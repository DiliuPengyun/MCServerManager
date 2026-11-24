use std::sync::Arc;

use crate::{
    config::ServerId,
    core::{ServerManager, ServerState, StartResult, provisioning},
    server::install::{InstallService, ProgressCallback},
};

use super::super::{JobId, LifecycleAction, Message, MessageSender, Refresh, TaskResult};
use super::error_text;

pub(in crate::ui::tasks) async fn run(
    manager: Arc<ServerManager>,
    id: ServerId,
    action: LifecycleAction,
    job: JobId,
    sender: MessageSender,
) -> TaskResult<StartResult> {
    match action {
        LifecycleAction::Start {
            restart,
            allow_no_rcon,
        } => start_or_restart(&manager, id, restart, allow_no_rcon).await,
        LifecycleAction::AgreeEula { restart } => {
            prepare_explicit_restart(&manager, id, restart).await?;
            manager.agree_eula_and_start(id).await.map_err(error_text)
        }
        LifecycleAction::EnableRcon { restart } => {
            prepare_explicit_restart(&manager, id, restart).await?;
            manager.enable_rcon_and_start(id).await.map_err(error_text)
        }
        LifecycleAction::RunInstaller { restart } => {
            // 先取得租约再快照配置和定位安装器，避免 relocate 后仍向旧目录写入。
            let _work = manager.begin_server_work(id).map_err(error_text)?;
            let server = manager.server_config(id).map_err(error_text)?;
            let directory = server.working_dir.clone();
            let installer = provisioning::locate_installer(&directory)
                .await
                .map_err(|error| format!("后台任务异常: {error}"))?
                .ok_or_else(|| "目录中找不到 Forge/NeoForge 安装器".to_owned())?;
            let progress: ProgressCallback = {
                let sender = sender.clone();
                Arc::new(move |line: &str| {
                    let _ = sender.send(Message::Progress {
                        job,
                        line: line.to_owned(),
                    });
                })
            };
            let service = InstallService::new().map_err(error_text)?;
            provisioning::run_registered_installer(
                &manager, &service, id, &server, &installer, progress,
            )
            .await
            .map_err(error_text)?;
            drop(_work);
            start_or_restart(&manager, id, restart, false).await
        }
    }
}

pub(in crate::ui::tasks) async fn stop(
    manager: &Arc<ServerManager>,
    id: ServerId,
) -> TaskResult<(String, Refresh)> {
    let state = manager.stop_server(id).await.map_err(error_text)?;
    // 无法确认 tmux 会话已结束时 stop 返回 Unknown，不能一律提示已停止。
    let message = if state == ServerState::Stopped {
        "服务器已停止"
    } else {
        "无法确认 tmux 会话已结束，服务器可能仍在运行"
    };
    Ok((message.to_owned(), Refresh::None))
}

async fn prepare_explicit_restart(
    manager: &Arc<ServerManager>,
    id: ServerId,
    restart: bool,
) -> TaskResult<()> {
    if restart && manager.state(id) != ServerState::Stopped {
        let state = manager.stop_server(id).await.map_err(error_text)?;
        if state != ServerState::Stopped {
            return Err("无法确认旧进程已停止，后续操作已取消".to_owned());
        }
    }
    Ok(())
}

async fn start_or_restart(
    manager: &Arc<ServerManager>,
    id: ServerId,
    restart: bool,
    allow_no_rcon: bool,
) -> TaskResult<StartResult> {
    if restart {
        manager.restart_server(id, allow_no_rcon).await
    } else {
        manager.start_server(id, allow_no_rcon).await
    }
    .map_err(error_text)
}
