use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::Mutex as SyncMutex;
use parking_lot::RwLock;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
    sync::Mutex,
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use crate::{
    config::{AppConfig, ServerConfig, ServerId},
    core::ServerState,
    error::{AppError, Result},
    server::{
        console::sanitize_console_line,
        console_buffer::ConsoleBuffer,
        launch::LaunchConfig,
        tmux::{SessionStatus, Tmux},
    },
};

const MAX_LINE_BYTES: usize = 64 * 1024;
const ATTACH_TAIL_BYTES: u64 = 256 * 1024;
const ROTATE_THRESHOLD: u64 = 64 * 1024 * 1024;

struct Shared {
    tmux: Arc<Tmux>,
    console: Arc<ConsoleBuffer>,
    state: RwLock<ServerState>,
}

/// 单个服务器的生命周期。operation 串行化启停/重连，日志任务不参与该锁。
pub struct ServerProcess {
    #[allow(dead_code)]
    id: ServerId,
    shared: Arc<Shared>,
    operation: Mutex<()>,
    tail_task: SyncMutex<Option<JoinHandle<()>>>,
    tail_cancel: SyncMutex<Option<CancellationToken>>,
    session: SyncMutex<Option<String>>,
    script: SyncMutex<Option<PathBuf>>,
}

impl ServerProcess {
    pub fn new(id: ServerId, tmux: Arc<Tmux>, console: Arc<ConsoleBuffer>) -> Arc<Self> {
        Arc::new(Self {
            id,
            shared: Arc::new(Shared {
                tmux,
                console,
                state: RwLock::new(ServerState::Stopped),
            }),
            operation: Mutex::new(()),
            tail_task: SyncMutex::new(None),
            tail_cancel: SyncMutex::new(None),
            session: SyncMutex::new(None),
            script: SyncMutex::new(None),
        })
    }

    pub fn state(&self) -> ServerState {
        *self.shared.state.read()
    }

    pub fn console(&self) -> Arc<ConsoleBuffer> {
        self.shared.console.clone()
    }

    pub fn session_name(&self) -> Option<String> {
        self.session.lock().clone()
    }

    /// 目录事务在整个磁盘操作期间持有同一生命周期锁，防止并发启动或重连。
    pub(crate) async fn lock_stopped(&self) -> Result<tokio::sync::MutexGuard<'_, ()>> {
        let operation = self.operation.lock().await;
        if self.state() != ServerState::Stopped {
            return Err(AppError::conflict(
                "server_active",
                "只有已停止的服务器才能执行此操作",
            ));
        }
        self.stop_tail().await;
        Ok(operation)
    }

    pub async fn start(
        self: &Arc<Self>,
        app_config: &AppConfig,
        config: &ServerConfig,
        session: String,
    ) -> Result<()> {
        let launch = crate::server::launch::build_launch_config(app_config, config)?;
        self.start_with_launch(config, session, launch).await
    }

    pub async fn start_with_launch(
        self: &Arc<Self>,
        config: &ServerConfig,
        session: String,
        launch: LaunchConfig,
    ) -> Result<()> {
        let _operation = self.operation.lock().await;
        match self.state() {
            ServerState::Stopped => {}
            ServerState::Unknown => {
                return Err(AppError::unavailable(
                    "state_unknown",
                    "服务器状态未知，请先确认 tmux 会话后再启动",
                ));
            }
            _ => {
                return Err(AppError::conflict(
                    "already_running",
                    "服务器已在运行或正在启停",
                ));
            }
        }
        match self.shared.tmux.status(&session).await {
            SessionStatus::Exists => {
                self.set_state(ServerState::Running);
                return Err(AppError::conflict(
                    "already_running",
                    "tmux 会话已存在；为防止重复启动，本次操作已取消",
                ));
            }
            SessionStatus::Unknown => {
                return Err(AppError::unavailable(
                    "tmux_status_unknown",
                    "无法确认 tmux 会话状态，启动已取消",
                ));
            }
            SessionStatus::Missing => {}
        }
        self.stop_tail().await;
        if !config.working_dir.is_dir() {
            return Err(AppError::not_found(format!(
                "服务器工作目录不存在或不是目录: {}",
                config.working_dir.display()
            )));
        }
        let log_path = prepare_log(&config.working_dir, true)?;
        let script = match self
            .shared
            .tmux
            .create_session(&session, &launch, &log_path)
            .await
        {
            Ok(script) => script,
            Err(error) => {
                self.set_state(ServerState::Stopped);
                return Err(error);
            }
        };
        *self.session.lock() = Some(session.clone());
        *self.script.lock() = Some(script);
        self.shared.console.clear();
        self.set_state(ServerState::Starting);
        self.spawn_tail(session, log_path, false);
        Ok(())
    }

    pub async fn attach(self: &Arc<Self>, config: &ServerConfig, session: &str) -> Result<bool> {
        let _operation = self.operation.lock().await;
        if self.shared.tmux.status(session).await != SessionStatus::Exists {
            return Ok(false);
        }
        let Some(pane) = self.shared.tmux.pane_current_path(session).await else {
            return Ok(false);
        };
        if comparable_path(&pane) != comparable_path(&config.working_dir) {
            return Ok(false);
        }
        self.stop_tail().await;
        let log_path = prepare_log(&config.working_dir, false)?;
        self.shared.console.clear();
        *self.session.lock() = Some(session.to_owned());
        self.set_state(ServerState::Running);
        self.spawn_tail(session.to_owned(), log_path, true);
        Ok(true)
    }

    pub async fn stop(&self) -> Result<ServerState> {
        let _operation = self.operation.lock().await;
        let session = self.session_name();
        if self.state() == ServerState::Stopped && session.is_none() {
            self.stop_tail().await;
            return Ok(ServerState::Stopped);
        }
        self.set_state(ServerState::Stopping);
        let Some(session) = session else {
            self.set_state(ServerState::Stopped);
            self.stop_tail().await;
            return Ok(ServerState::Stopped);
        };

        let sent = match self.shared.tmux.send_command(&session, "stop").await {
            Ok(()) => true,
            Err(error) => {
                self.shared
                    .console
                    .push(format!("[MCServerManager] 优雅停止请求失败: {error}"));
                false
            }
        };
        if sent {
            let deadline = Instant::now() + Duration::from_secs(30);
            while Instant::now() < deadline {
                if self.shared.tmux.status(&session).await == SessionStatus::Missing {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        if self.shared.tmux.status(&session).await != SessionStatus::Missing {
            let script = self.script.lock().clone();
            if let Err(error) = self
                .shared
                .tmux
                .kill_session(&session, script.as_deref())
                .await
            {
                self.shared
                    .console
                    .push(format!("[MCServerManager] 强制结束失败: {error}"));
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if self.shared.tmux.status(&session).await == SessionStatus::Missing {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        let final_state = if self.shared.tmux.status(&session).await == SessionStatus::Missing {
            let script = self.script.lock().take();
            if let Some(script) = script {
                self.shared.tmux.remove_script(&script).await;
            }
            ServerState::Stopped
        } else {
            self.shared.console.push(format!(
                "[MCServerManager] 警告：无法确认 tmux 会话已结束，服务器可能仍在运行: {session}"
            ));
            ServerState::Unknown
        };
        self.set_state(final_state);
        self.stop_tail().await;
        Ok(final_state)
    }

    /// 仅停止本地日志任务，绝不停止 tmux/Minecraft。
    pub async fn detach(&self) {
        let _operation = self.operation.lock().await;
        self.stop_tail().await;
        self.set_state(ServerState::Stopped);
    }

    pub async fn send_command(&self, command: &str) -> Result<()> {
        if !matches!(self.state(), ServerState::Starting | ServerState::Running) {
            return Err(AppError::conflict("not_running", "服务器当前未运行"));
        }
        let session = self
            .session_name()
            .ok_or_else(|| AppError::conflict("not_running", "服务器尚未关联 tmux 会话"))?;
        self.shared.tmux.send_command(&session, command).await
    }

    fn spawn_tail(self: &Arc<Self>, session: String, log_path: PathBuf, attached: bool) {
        let token = CancellationToken::new();
        *self.tail_cancel.lock() = Some(token.clone());
        let shared = self.shared.clone();
        let task = tokio::spawn(tail_loop(shared, session, log_path, attached, token));
        *self.tail_task.lock() = Some(task);
    }

    async fn stop_tail(&self) {
        if let Some(token) = self.tail_cancel.lock().take() {
            token.cancel();
        }
        let task = self.tail_task.lock().take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }

    fn set_state(&self, state: ServerState) {
        *self.shared.state.write() = state;
    }
}

fn prepare_log(working_dir: &Path, truncate: bool) -> Result<PathBuf> {
    let directory = working_dir.join(".mcsm");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("console.log");
    if truncate {
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
    } else if !path.exists() {
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
    }
    Ok(path)
}

fn comparable_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

async fn tail_loop(
    shared: Arc<Shared>,
    session: String,
    log_path: PathBuf,
    attached: bool,
    cancellation: CancellationToken,
) {
    let mut file: Option<File> = None;
    let mut splitter = LineSplitter::default();
    let mut skip_partial = false;
    let mut last_status = Instant::now() - Duration::from_secs(2);
    let mut last_rotation = Instant::now();
    let mut detected_ready = attached;

    while !cancellation.is_cancelled() {
        if file.is_none() {
            match File::open(&log_path).await {
                Ok(mut opened) => {
                    if attached {
                        let length = opened.metadata().await.map(|meta| meta.len()).unwrap_or(0);
                        let start = length.saturating_sub(ATTACH_TAIL_BYTES);
                        skip_partial = start > 0;
                        if opened.seek(std::io::SeekFrom::Start(start)).await.is_err() {
                            *shared.state.write() = ServerState::Unknown;
                            shared
                                .console
                                .push("[MCServerManager] 无法定位控制台日志尾部");
                            return;
                        }
                    }
                    file = Some(opened);
                }
                Err(_) => {
                    if wait_or_cancel(&cancellation, Duration::from_millis(20)).await {
                        break;
                    }
                    continue;
                }
            }
        }

        let mut bytes = [0_u8; 16 * 1024];
        let read = tokio::select! {
            _ = cancellation.cancelled() => break,
            result = file.as_mut().expect("日志文件已打开").read(&mut bytes) => result,
        };
        let mut at_eof = false;
        match read {
            Ok(0) => at_eof = true,
            Ok(count) => {
                let mut chunk = &bytes[..count];
                if skip_partial {
                    if let Some(newline) = chunk.iter().position(|byte| *byte == b'\n') {
                        chunk = &chunk[newline + 1..];
                        skip_partial = false;
                    } else {
                        chunk = &[];
                    }
                }
                for line in splitter.push(chunk) {
                    if !detected_ready
                        && (line.contains("Done (")
                            || (line.contains("Done!") && !line.contains("For help")))
                    {
                        detected_ready = true;
                        let mut state = shared.state.write();
                        if *state == ServerState::Starting {
                            *state = ServerState::Running;
                        }
                    }
                    shared.console.push(line);
                }
            }
            Err(error) => {
                shared
                    .console
                    .push(format!("[MCServerManager] 控制台日志读取失败: {error}"));
                *shared.state.write() = ServerState::Unknown;
                return;
            }
        }

        if at_eof && last_rotation.elapsed() >= Duration::from_secs(5) {
            last_rotation = Instant::now();
            if rotate_log(&log_path).await.unwrap_or(false) {
                splitter.clear();
                file = File::open(&log_path).await.ok();
                shared
                    .console
                    .push("[MCServerManager] console.log 超过 64 MiB，已轮转到 console.log.1");
            }
        }

        if last_status.elapsed() >= Duration::from_secs(2) {
            last_status = Instant::now();
            if shared.tmux.status(&session).await == SessionStatus::Missing {
                shared.console.push("Server process exited");
                *shared.state.write() = ServerState::Stopped;
                break;
            }
        }
        if at_eof && wait_or_cancel(&cancellation, Duration::from_millis(10)).await {
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

async fn rotate_log(path: &Path) -> Result<bool> {
    let size = match tokio::fs::metadata(path).await {
        Ok(metadata) if metadata.len() >= ROTATE_THRESHOLD => metadata.len(),
        _ => return Ok(false),
    };
    let rotated = path.with_extension("log.1");
    let temporary = path.with_extension("log.rotate.tmp");
    let source = File::open(path).await?;
    let mut source = source.take(size);
    let mut destination = File::create(&temporary).await?;
    let copied = tokio::io::copy(&mut source, &mut destination).await?;
    destination.sync_all().await?;
    drop(destination);
    if copied != size {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Ok(false);
    }
    tokio::fs::rename(&temporary, &rotated).await?;
    tokio::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .await?;
    Ok(true)
}

#[derive(Default)]
struct LineSplitter {
    pending: Vec<u8>,
}

impl LineSplitter {
    fn push(&mut self, mut chunk: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        while !chunk.is_empty() {
            let newline = chunk.iter().position(|byte| *byte == b'\n');
            let end = newline.unwrap_or(chunk.len());
            let take = end.min(MAX_LINE_BYTES.saturating_sub(self.pending.len()));
            self.pending.extend_from_slice(&chunk[..take]);
            chunk = &chunk[take..];
            let truncated = self.pending.len() >= MAX_LINE_BYTES && newline != Some(take);
            if newline.is_some() && take == end {
                chunk = &chunk[1..];
                lines.push(self.emit(false));
            } else if truncated {
                lines.push(self.emit(true));
            }
        }
        lines
    }

    fn emit(&mut self, truncated: bool) -> String {
        if self.pending.last() == Some(&b'\r') {
            self.pending.pop();
        }
        let mut line = sanitize_console_line(&String::from_utf8_lossy(&self.pending));
        self.pending.clear();
        if truncated {
            line.push_str(" …[输出过长已截断]");
        }
        line
    }

    fn clear(&mut self) {
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, fs, os::unix::fs::PermissionsExt};

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn splits_and_truncates_console_lines() {
        let mut splitter = LineSplitter::default();
        assert!(splitter.push("第一行\n不完整".as_bytes()).len() == 1);
        assert_eq!(splitter.push("完成\n".as_bytes())[0], "不完整完成");
        let long = vec![b'x'; MAX_LINE_BYTES + 10];
        let lines = splitter.push(&long);
        assert!(lines[0].contains("输出过长已截断"));
    }

    #[tokio::test]
    async fn isolated_tmux_starts_replays_commands_and_stops() {
        let temp = TempDir::new().unwrap();
        let working = temp.path().join("server with spaces");
        fs::create_dir_all(&working).unwrap();
        let fake_java = temp.path().join("fake java.sh");
        fs::write(
            &fake_java,
            "#!/bin/sh\nprintf '\\033[32mDone (1.0s)!\\033[m\\n'\nwhile IFS= read -r line; do\n  printf '收到:%s\\n' \"$line\"\n  [ \"$line\" = stop ] && exit 0\ndone\n",
        )
        .unwrap();
        fs::set_permissions(&fake_java, fs::Permissions::from_mode(0o700)).unwrap();
        let socket = format!("mcsm-test-{}", ServerId::new().simple());
        let tmux = Arc::new(Tmux::isolated(temp.path().join("runtime"), socket).unwrap());
        let console = ConsoleBuffer::new(100);
        let process = ServerProcess::new(ServerId::new(), tmux.clone(), console.clone());
        let mut config = ServerConfig::new("测试", working.clone());
        config.server_jar_path = PathBuf::from("unused.jar");
        let session = format!("mcsm_{}", process.id.simple());
        let launch = LaunchConfig {
            program: fake_java.to_string_lossy().into_owned(),
            args: vec![OsString::from("参数 with spaces")],
            working_dir: working,
        };

        let exercise = async {
            process
                .start_with_launch(&config, session.clone(), launch)
                .await?;
            wait_for_line(&console, "Done (").await?;
            assert_eq!(process.state(), ServerState::Running);
            process.send_command("say 你好").await?;
            wait_for_line(&console, "收到:say 你好").await?;

            // 分离只结束本地尾随；重连应接管同一会话并回放已有日志。
            process.detach().await;
            assert_eq!(tmux.status(&session).await, SessionStatus::Exists);
            assert!(process.attach(&config, &session).await?);
            wait_for_line(&console, "Done (").await?;
            process.send_command("list").await?;
            wait_for_line(&console, "收到:list").await?;
            assert_eq!(process.stop().await?, ServerState::Stopped);
            assert_eq!(tmux.status(&session).await, SessionStatus::Missing);
            Ok::<(), AppError>(())
        }
        .await;
        let _ = process.stop().await;
        let _ = tmux.kill_session(&session, None).await;
        exercise.unwrap();
    }

    async fn wait_for_line(console: &ConsoleBuffer, needle: &str) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                if console.lines().iter().any(|line| line.contains(needle)) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .map_err(|_| {
            AppError::internal(format!(
                "等待控制台输出超时: {needle}; 当前输出: {:?}",
                console.lines()
            ))
        })
    }
}
