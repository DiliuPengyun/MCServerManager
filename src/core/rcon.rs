use std::{
    collections::HashSet,
    net::TcpListener,
    sync::{Arc, Weak},
    time::Duration,
};

use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;

use crate::{
    config::ServerId,
    error::{AppError, Result},
    server::{properties::ServerProperties, rcon::RconClient},
};

use super::{ServerManager, ServerState};

pub(super) type SharedRcon = Arc<AsyncMutex<Option<RconClient>>>;

impl ServerManager {
    pub(super) fn sync_rcon_from_properties(&self, id: ServerId) -> Result<bool> {
        let server = self.server_config(id)?;
        let path = server.working_dir.join("server.properties");
        let properties = if path.exists() {
            ServerProperties::load(&path)?
        } else {
            return Ok(false);
        };
        let status = properties.rcon_status();
        let Some((port, password)) = status.credentials() else {
            return Ok(false);
        };
        if server.rcon_port != port || server.rcon_password != password {
            let mut updated = server;
            updated.rcon_port = port;
            updated.rcon_password = password.to_owned();
            self.update_server(id, updated)?;
        }
        Ok(true)
    }

    pub(super) fn available_rcon_port(&self, id: ServerId, preferred: u16) -> Result<u16> {
        let config = self.config();
        let used: HashSet<u16> = config
            .servers
            .iter()
            .filter(|server| server.id != id && server.rcon_port != 0)
            .map(|server| server.rcon_port)
            .collect();
        let mut port = if preferred == 0 { 25575 } else { preferred };
        loop {
            if !used.contains(&port) && TcpListener::bind(("127.0.0.1", port)).is_ok() {
                return Ok(port);
            }
            port = port.checked_add(1).ok_or_else(|| {
                AppError::unavailable("rcon_port_unavailable", "没有可用的 RCON 端口")
            })?;
        }
    }

    fn rcon_slot(&self, id: ServerId) -> SharedRcon {
        self.rcon_connections
            .lock()
            .entry(id)
            .or_insert_with(|| Arc::new(AsyncMutex::new(None)))
            .clone()
    }

    pub(super) async fn rcon_command(&self, id: ServerId, command: &str) -> Result<String> {
        if self.state(id) != ServerState::Running {
            return Err(AppError::conflict("not_running", "服务器当前未运行"));
        }
        let server = self.server_config(id)?;
        if !server.rcon_available() {
            return Err(AppError::conflict("rcon_unavailable", "服务器未启用 RCON"));
        }
        let slot = self.rcon_slot(id);
        let mut connection = slot.lock().await;
        if connection.is_none() {
            *connection = Some(
                RconClient::connect(
                    "127.0.0.1",
                    server.rcon_port,
                    &server.rcon_password,
                    Duration::from_secs(2),
                )
                .await?,
            );
        }
        let result = connection
            .as_mut()
            .expect("上面已建立连接")
            .command(command)
            .await;
        if result.is_err() {
            *connection = None;
        }
        result
    }

    pub(super) fn start_background(self: &Arc<Self>) {
        let mut background = self.background.lock();
        if background.is_some() {
            return;
        }
        let weak = Arc::downgrade(self);
        let cancellation = self.shutdown.clone();
        *background = Some(tokio::spawn(rcon_poll_loop(weak, cancellation)));
    }

    async fn poll_rcon(&self) {
        let targets: Vec<_> = self
            .config()
            .servers
            .iter()
            .filter(|server| {
                server.rcon_available() && self.state(server.id) == ServerState::Running
            })
            .cloned()
            .collect();
        let active: HashSet<_> = targets.iter().map(|server| server.id).collect();
        self.rcon_connections
            .lock()
            .retain(|id, _| active.contains(id));
        self.players.write().retain(|id, _| active.contains(id));

        for server in targets {
            let slot = self.rcon_slot(server.id);
            let mut connection = slot.lock().await;
            if connection.is_none() {
                match RconClient::connect(
                    "127.0.0.1",
                    server.rcon_port,
                    &server.rcon_password,
                    Duration::from_secs(2),
                )
                .await
                {
                    Ok(client) => *connection = Some(client),
                    Err(_) => {
                        self.players.write().remove(&server.id);
                        continue;
                    }
                }
            }
            match connection
                .as_mut()
                .expect("上面已建立连接")
                .list_players()
                .await
            {
                Ok(players) => {
                    self.players.write().insert(server.id, players);
                }
                Err(_) => {
                    *connection = None;
                    self.players.write().remove(&server.id);
                }
            }
        }
    }
}

async fn rcon_poll_loop(manager: Weak<ServerManager>, cancellation: CancellationToken) {
    loop {
        let Some(manager) = manager.upgrade() else {
            break;
        };
        manager.poll_rcon().await;
        drop(manager);
        if wait_or_cancel(&cancellation, Duration::from_secs(3)).await {
            break;
        }
    }
}

async fn wait_or_cancel(cancellation: &CancellationToken, duration: Duration) -> bool {
    tokio::select! {
        _ = cancellation.cancelled() => true,
        _ = tokio::time::sleep(duration) => false,
    }
}
