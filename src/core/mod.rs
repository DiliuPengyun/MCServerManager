use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{
    config::{AppConfig, ConfigStore, ServerConfig, ServerId},
    error::{AppError, Result},
    server::{
        backup,
        console_buffer::ConsoleBuffer,
        detect::{self, DetectedCore},
        eula,
        process::ServerProcess,
        properties::ServerProperties,
        tmux::Tmux,
    },
};

pub mod location;
pub mod provisioning;
mod rcon;
mod removal;
mod session;
pub mod tasks;
mod validation;

#[cfg(test)]
mod tests;

use rcon::SharedRcon;
use session::stable_session_name;
use validation::{
    comparable_path, ensure_unique_working_dir, normalize_lexical, validate_player_name,
    validate_server,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerState {
    #[default]
    Stopped,
    Starting,
    Running,
    Stopping,
    Unknown,
}

impl ServerState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "STOPPED",
            Self::Starting => "STARTING",
            Self::Running => "RUNNING",
            Self::Stopping => "STOPPING",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Starting | Self::Running | Self::Stopping)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartResult {
    Started,
    AlreadyRunning,
    InstallerRequired,
    EulaRequired,
    RconRequired,
}

#[derive(Clone, Debug, Serialize)]
pub struct ServerSnapshot {
    pub index: usize,
    pub id: ServerId,
    pub name: String,
    pub working_dir: PathBuf,
    pub state: ServerState,
    pub rcon_available: bool,
    pub players: Vec<String>,
}

/// 安装、备份和目录事务使用的服务器级任务租约。
pub struct ServerWorkGuard {
    work: Arc<Mutex<HashSet<ServerId>>>,
    id: ServerId,
}

impl Drop for ServerWorkGuard {
    fn drop(&mut self) {
        self.work.lock().remove(&self.id);
    }
}

/// TUI 与 Web 共用的注册表。配置修改短暂串行化，长时间运行任务不持有配置锁。
pub struct ServerManager {
    store: ConfigStore,
    config: RwLock<Arc<AppConfig>>,
    mutation: Mutex<()>,
    tmux: Arc<Tmux>,
    processes: RwLock<HashMap<ServerId, Arc<ServerProcess>>>,
    players: RwLock<HashMap<ServerId, Vec<String>>>,
    rcon_connections: Mutex<HashMap<ServerId, SharedRcon>>,
    work: Arc<Mutex<HashSet<ServerId>>>,
    background: Mutex<Option<JoinHandle<()>>>,
    shutdown: CancellationToken,
}

impl ServerManager {
    pub fn load(store: ConfigStore) -> Result<Arc<Self>> {
        let tmux = Arc::new(Tmux::new(store.config_dir().join("runtime"))?);
        Self::load_with_tmux(store, tmux)
    }

    pub(crate) fn load_with_tmux(store: ConfigStore, tmux: Arc<Tmux>) -> Result<Arc<Self>> {
        let loaded = store.load()?;
        let processes = loaded
            .config
            .servers
            .iter()
            .map(|server| {
                (
                    server.id,
                    ServerProcess::new(server.id, tmux.clone(), ConsoleBuffer::new(5000)),
                )
            })
            .collect();
        Ok(Arc::new(Self {
            store,
            config: RwLock::new(Arc::new(loaded.config)),
            mutation: Mutex::new(()),
            tmux,
            processes: RwLock::new(processes),
            players: RwLock::new(HashMap::new()),
            rcon_connections: Mutex::new(HashMap::new()),
            work: Arc::new(Mutex::new(HashSet::new())),
            background: Mutex::new(None),
            shutdown: CancellationToken::new(),
        }))
    }

    pub fn store(&self) -> &ConfigStore {
        &self.store
    }

    pub fn begin_server_work(&self, id: ServerId) -> Result<ServerWorkGuard> {
        self.server_config(id)?;
        let mut work = self.work.lock();
        if !work.insert(id) {
            return Err(AppError::busy(
                "该服务器已有安装、目录、备份或删除任务正在执行，请稍后再试",
            ));
        }
        drop(work);
        Ok(ServerWorkGuard {
            work: self.work.clone(),
            id,
        })
    }

    pub fn has_server_work(&self, id: ServerId) -> bool {
        self.work.lock().contains(&id)
    }

    fn ensure_no_server_work(&self, id: ServerId) -> Result<()> {
        if self.has_server_work(id) {
            Err(AppError::busy(
                "该服务器已有安装、目录、备份或删除任务正在执行，请稍后再试",
            ))
        } else {
            Ok(())
        }
    }

    pub fn config(&self) -> Arc<AppConfig> {
        self.config.read().clone()
    }

    pub fn snapshots(&self) -> Vec<ServerSnapshot> {
        let config = self.config();
        let processes = self.processes.read();
        let players = self.players.read();
        config
            .servers
            .iter()
            .enumerate()
            .map(|(index, server)| ServerSnapshot {
                index,
                id: server.id,
                name: server.name.clone(),
                working_dir: server.working_dir.clone(),
                state: processes
                    .get(&server.id)
                    .map(|process| process.state())
                    .unwrap_or(ServerState::Unknown),
                rcon_available: server.rcon_available(),
                players: players.get(&server.id).cloned().unwrap_or_default(),
            })
            .collect()
    }

    pub fn server_config(&self, id: ServerId) -> Result<ServerConfig> {
        self.config()
            .server(id)
            .cloned()
            .ok_or_else(|| AppError::not_found("服务器不存在"))
    }

    pub fn server_config_at(&self, index: usize) -> Result<ServerConfig> {
        self.config().server_at(index).cloned().ok_or_else(|| {
            AppError::new(
                crate::error::ErrorKind::NotFound,
                "invalid_index",
                "服务器下标无效",
            )
        })
    }

    pub fn id_at(&self, index: usize) -> Result<ServerId> {
        Ok(self.server_config_at(index)?.id)
    }

    pub fn state(&self, id: ServerId) -> ServerState {
        self.processes
            .read()
            .get(&id)
            .map(|process| process.state())
            .unwrap_or(ServerState::Unknown)
    }

    pub fn players(&self, id: ServerId) -> Vec<String> {
        self.players.read().get(&id).cloned().unwrap_or_default()
    }

    pub fn console(&self, id: ServerId) -> Option<Arc<ConsoleBuffer>> {
        self.processes
            .read()
            .get(&id)
            .map(|process| process.console())
    }

    pub fn append_console_line(&self, id: ServerId, line: impl Into<String>) -> Result<()> {
        self.console(id)
            .ok_or_else(|| AppError::not_found("服务器不存在"))?
            .push(line);
        Ok(())
    }

    pub fn add_server(&self, mut server: ServerConfig) -> Result<ServerConfig> {
        let _guard = self.mutation.lock();
        let mut next = (*self.config()).clone();
        if server.working_dir.is_relative() {
            server.working_dir = next.default_server_dir.join(&server.working_dir);
        }
        server.working_dir = normalize_lexical(&server.working_dir);
        validate_server(&server)?;
        ensure_unique_working_dir(&next, &server, None)?;
        if next.servers.iter().any(|existing| existing.id == server.id) {
            server.id = ServerId::new();
        }
        next.active_server_id = Some(server.id);
        next.servers.push(server.clone());
        self.store.save(&next)?;
        self.processes.write().insert(
            server.id,
            ServerProcess::new(server.id, self.tmux.clone(), ConsoleBuffer::new(5000)),
        );
        *self.config.write() = Arc::new(next);
        Ok(server)
    }

    pub fn update_server(&self, id: ServerId, mut updated: ServerConfig) -> Result<ServerConfig> {
        let _guard = self.mutation.lock();
        let mut next = (*self.config()).clone();
        let existing = next
            .server(id)
            .cloned()
            .ok_or_else(|| AppError::not_found("服务器不存在"))?;
        updated.id = id;
        if updated.working_dir.is_relative() {
            updated.working_dir = existing.working_dir.join(updated.working_dir);
        }
        updated.working_dir = normalize_lexical(&updated.working_dir);
        validate_server(&updated)?;
        ensure_unique_working_dir(&next, &updated, Some(id))?;
        if comparable_path(&updated.working_dir) != comparable_path(&existing.working_dir)
            && self.state(id) != ServerState::Stopped
        {
            return Err(AppError::conflict(
                "server_active",
                "服务器未处于停止状态，不能更改工作目录",
            ));
        }
        let slot = next.server_mut(id).expect("上面已确认服务器存在");
        *slot = updated.clone();
        self.store.save(&next)?;
        *self.config.write() = Arc::new(next);
        Ok(updated)
    }

    pub fn apply_installer_detection(&self, id: ServerId, detected: &DetectedCore) -> Result<()> {
        let mut server = self.server_config(id)?;
        server.core_type = detected.core_type;
        if let Some(version) = &detected.minecraft_version {
            server.minecraft_version = version.clone();
        }
        if let Some(version) = &detected.core_version {
            server.core_version = version.clone();
        }
        if let Some(path) = &detected.server_jar_path {
            server.server_jar_path = path.clone();
        }
        if let Some(path) = &detected.launch_args_file {
            server.launch_args_file = path.clone();
        }
        self.update_server(id, server)?;
        Ok(())
    }

    pub fn update_default_server_dir(&self, path: &Path) -> Result<PathBuf> {
        let _guard = self.mutation.lock();
        let mut next = (*self.config()).clone();
        next.default_server_dir = if path.as_os_str().is_empty() {
            self.store.fresh_default_server_dir()
        } else if path.is_absolute() {
            normalize_lexical(path)
        } else {
            // 相对的默认目录以用户数据目录为基准。
            normalize_lexical(&self.store.data_dir().join(path))
        };
        self.store.save(&next)?;
        *self.config.write() = Arc::new(next.clone());
        Ok(next.default_server_dir)
    }

    pub fn update_global_java_path(&self, path: String) -> Result<()> {
        let _guard = self.mutation.lock();
        let mut next = (*self.config()).clone();
        next.global_java_path = path;
        self.store.save(&next)?;
        *self.config.write() = Arc::new(next);
        Ok(())
    }

    pub fn update_web_port(&self, port: u16) -> Result<()> {
        if port == 0 {
            return Err(AppError::invalid("invalid_port", "Web 端口必须大于 0"));
        }
        let _guard = self.mutation.lock();
        let mut next = (*self.config()).clone();
        next.web_port = port;
        self.store.save(&next)?;
        *self.config.write() = Arc::new(next);
        Ok(())
    }

    pub fn select_server(&self, id: ServerId) -> Result<()> {
        let _guard = self.mutation.lock();
        let mut next = (*self.config()).clone();
        if !next.select(id) {
            return Err(AppError::not_found("服务器不存在"));
        }
        self.store.save(&next)?;
        *self.config.write() = Arc::new(next);
        Ok(())
    }

    /// 启动时接管各服务器已有的 tmux 会话，不创建或停止任何 Minecraft 进程。
    pub async fn attach_all(self: &Arc<Self>) {
        let config = self.config();
        for server in config.servers.iter() {
            let Some(process) = self.process(server.id) else {
                continue;
            };
            let session = stable_session_name(server.id);
            if let Err(error) = process.attach(server, &session).await {
                process
                    .console()
                    .push(format!("[MCServerManager] 重连 tmux 会话失败: {error}"));
            }
        }
        self.start_background();
    }

    pub fn needs_installer(&self, id: ServerId) -> Result<bool> {
        Ok(detect::needs_installer(&self.server_config(id)?))
    }

    pub async fn start_server(&self, id: ServerId, allow_no_rcon: bool) -> Result<StartResult> {
        self.ensure_no_server_work(id)?;
        let server = self.server_config(id)?;
        if detect::needs_installer(&server) {
            return Ok(StartResult::InstallerRequired);
        }
        if !eula::eula_accepted(&server.working_dir) {
            return Ok(StartResult::EulaRequired);
        }

        let process = self
            .process(id)
            .ok_or_else(|| AppError::not_found("服务器运行态不存在"))?;
        if process.state() == ServerState::Stopped
            && process.attach(&server, &stable_session_name(id)).await?
        {
            return Ok(StartResult::AlreadyRunning);
        }
        match process.state() {
            ServerState::Stopped => {}
            ServerState::Unknown => {
                return Err(AppError::unavailable(
                    "state_unknown",
                    "服务器状态未知，请先检查 tmux 会话",
                ));
            }
            _ => return Ok(StartResult::AlreadyRunning),
        }
        if !allow_no_rcon && !self.sync_rcon_from_properties(id)? {
            return Ok(StartResult::RconRequired);
        }

        let session = stable_session_name(id);
        match process.start(&self.config(), &server, session).await {
            Ok(()) => Ok(StartResult::Started),
            Err(error) if error.code == "already_running" => Ok(StartResult::AlreadyRunning),
            Err(error) => Err(error),
        }
    }

    /// 只有显式“同意 EULA”动作可以调用；检查启动状态不会调用此方法。
    pub async fn agree_eula_and_start(&self, id: ServerId) -> Result<StartResult> {
        self.ensure_no_server_work(id)?;
        let server = self.server_config(id)?;
        eula::agree_eula(&server.working_dir)?;
        self.start_server(id, false).await
    }

    /// 只有显式“启用 RCON”动作可以调用；会写 properties 和配置中的凭据副本。
    pub async fn enable_rcon_and_start(&self, id: ServerId) -> Result<StartResult> {
        self.ensure_no_server_work(id)?;
        let server = self.server_config(id)?;
        if detect::needs_installer(&server) {
            return Ok(StartResult::InstallerRequired);
        }
        if !eula::eula_accepted(&server.working_dir) {
            return Ok(StartResult::EulaRequired);
        }
        if !self.sync_rcon_from_properties(id)? {
            let path = server.working_dir.join("server.properties");
            let mut properties = if path.exists() {
                ServerProperties::load(&path)?
            } else {
                ServerProperties::default()
            };
            let port = self.available_rcon_port(id, server.rcon_port)?;
            let password = if server.rcon_password.is_empty() {
                format!("{}{}", ServerId::new().simple(), ServerId::new().simple())
            } else {
                server.rcon_password.clone()
            };
            properties.enable_rcon(port, &password)?;
            properties.save(&path).map_err(|error| {
                AppError::new(
                    error.kind,
                    "rcon_write_failed",
                    format!("无法写入 RCON 配置: {error}"),
                )
            })?;
            let mut updated = server;
            updated.rcon_port = port;
            updated.rcon_password = password;
            self.update_server(id, updated)?;
        }
        self.start_server(id, true).await
    }

    pub async fn stop_server(&self, id: ServerId) -> Result<ServerState> {
        let process = self
            .process(id)
            .ok_or_else(|| AppError::not_found("服务器不存在"))?;
        let state = process.stop().await?;
        if state == ServerState::Stopped {
            self.players.write().remove(&id);
            self.rcon_connections.lock().remove(&id);
        }
        Ok(state)
    }

    pub async fn restart_server(&self, id: ServerId, allow_no_rcon: bool) -> Result<StartResult> {
        self.ensure_no_server_work(id)?;
        let server = self.server_config(id)?;
        if detect::needs_installer(&server) {
            return Ok(StartResult::InstallerRequired);
        }
        if !eula::eula_accepted(&server.working_dir) {
            return Ok(StartResult::EulaRequired);
        }
        if !allow_no_rcon && !self.sync_rcon_from_properties(id)? {
            return Ok(StartResult::RconRequired);
        }
        let state = self.stop_server(id).await?;
        if state != ServerState::Stopped {
            return Err(AppError::unavailable(
                "stop_unconfirmed",
                "无法确认旧进程已停止，重启已取消",
            ));
        }
        self.start_server(id, allow_no_rcon).await
    }

    /// 创建备份期间同时持有工作租约和生命周期锁，避免安装、启动、恢复或目录移动交叉修改世界。
    pub async fn create_backup(&self, id: ServerId) -> Result<String> {
        self.stopped_backup_operation(id, |server| backup::create(&server))
            .await
    }

    /// 只有调用方对应的明确恢复动作才能进入；暂存、校验与换入由 backup 模块完成。
    pub async fn restore_backup(&self, id: ServerId, name: &str) -> Result<()> {
        let name = name.to_owned();
        self.stopped_backup_operation(id, move |server| backup::restore(&server, &name))
            .await
    }

    /// 删除可在服务器运行时执行，但必须与创建、恢复、安装和目录事务互斥。
    pub async fn delete_backup(&self, id: ServerId, name: &str) -> Result<()> {
        let _work = self.begin_server_work(id)?;
        let server = self.server_config(id)?;
        let name = name.to_owned();
        tokio::task::spawn_blocking(move || backup::delete(&server, &name))
            .await
            .map_err(|error| AppError::internal(format!("备份任务异常: {error}")))?
    }

    async fn stopped_backup_operation<T, F>(&self, id: ServerId, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(ServerConfig) -> Result<T> + Send + 'static,
    {
        let _work = self.begin_server_work(id)?;
        let process = self
            .process(id)
            .ok_or_else(|| AppError::not_found("服务器运行态不存在"))?;
        let _operation = process.lock_stopped().await?;
        let server = self.server_config(id)?;
        self.ensure_no_tmux_session(&server).await?;
        tokio::task::spawn_blocking(move || operation(server))
            .await
            .map_err(|error| AppError::internal(format!("备份任务异常: {error}")))?
    }

    pub async fn send_command(&self, id: ServerId, command: &str) -> Result<()> {
        self.process(id)
            .ok_or_else(|| AppError::not_found("服务器不存在"))?
            .send_command(command)
            .await
    }

    pub async fn kick_player(&self, id: ServerId, player: &str) -> Result<()> {
        validate_player_name(player)?;
        self.rcon_command(id, &format!("kick {player}"))
            .await
            .map(|_| ())
    }

    pub async fn ban_player(&self, id: ServerId, player: &str) -> Result<()> {
        validate_player_name(player)?;
        self.rcon_command(id, &format!("ban {player}"))
            .await
            .map(|_| ())
    }

    pub async fn detach_all(&self) {
        let processes: Vec<_> = self.processes.read().values().cloned().collect();
        for process in processes {
            process.detach().await;
        }
    }

    /// 退出管理器本身，不停止 tmux 中的 Minecraft。
    pub async fn shutdown(&self) {
        self.shutdown.cancel();
        let task = self.background.lock().take();
        if let Some(task) = task {
            let _ = task.await;
        }
        self.detach_all().await;
    }

    /// TUI 显式删除使用的入口：必要时先停止服务器。Web 使用 remove_registered_server，不会隐式停服。
    pub async fn remove_server(self: &Arc<Self>, id: ServerId, delete_files: bool) -> Result<()> {
        let _work = self.begin_server_work(id)?;
        if self.state(id).is_active() {
            let state = self.stop_server(id).await?;
            if state != ServerState::Stopped {
                return Err(AppError::conflict(
                    "stop_unconfirmed",
                    "无法确认服务器已停止，未删除配置或文件",
                ));
            }
        }
        self.remove_server_after_stop(id, delete_files).await
    }

    /// Web 使用的移除入口：只允许已停止且无 tmux 会话的服务器，绝不为了删除而隐式停服。
    pub async fn remove_registered_server(
        self: &Arc<Self>,
        id: ServerId,
        delete_files: bool,
    ) -> Result<()> {
        let _work = self.begin_server_work(id)?;
        self.remove_server_after_stop(id, delete_files).await
    }

    fn process(&self, id: ServerId) -> Option<Arc<ServerProcess>> {
        self.processes.read().get(&id).cloned()
    }
}
