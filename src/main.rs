use std::{process::ExitCode, sync::Arc};

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use mcsm::{AppError, Result, config::ConfigStore, core::ServerManager};

#[derive(Debug, Parser)]
#[command(
    name = "MCServerManager",
    version,
    about = "轻量的 Minecraft 服务器管理器"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 启动 Web 管理界面；默认在后台运行。
    Web {
        /// 覆盖配置文件中的 Web 端口。
        port: Option<u16>,
        /// 保持前台运行，便于开发和隔离测试。
        #[arg(long)]
        foreground: bool,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("MCServerManager: {}", error.message);
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_ansi(false)
        .init();
    ensure_tmux()?;
    let cli = Cli::parse();
    let store = ConfigStore::new()?;
    let manager = ServerManager::load(store)?;
    match cli.command {
        Some(Command::Web { port, foreground }) => mcsm::web::run(manager, port, foreground),
        None => mcsm::ui::run(manager),
    }
}

fn ensure_tmux() -> Result<()> {
    std::process::Command::new("tmux")
        .arg("-V")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .filter(|status| status.success())
        .map(|_| ())
        .ok_or_else(|| {
            AppError::unavailable("tmux_unavailable", "未找到可用的 tmux，请先安装 tmux")
        })
}

#[allow(dead_code)]
fn assert_manager_is_send_sync(manager: Arc<ServerManager>) {
    fn assert<T: Send + Sync>(_: T) {}
    assert(manager);
}
