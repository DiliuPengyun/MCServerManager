mod input;
mod render;
mod state;
mod tasks;

use std::{
    io::{self, Stdout},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    time::Duration,
};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{AppError, Result, core::ServerManager};
use state::App;
use tasks::{Message, MessageSender, TaskBook};

/// 启动完整 TUI。网络、安装、文件与备份操作均由独立 Tokio runtime 中的受管任务执行。
pub fn run(manager: Arc<ServerManager>) -> Result<()> {
    let _terminal_guard = TerminalGuard::new()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| AppError::internal(format!("无法创建异步运行时: {error}")))?;

    let operation = catch_unwind(AssertUnwindSafe(|| {
        runtime.block_on(manager.attach_all());
        event_loop(&mut terminal, &runtime, manager.clone())
    }));
    // 即使绘制或事件循环 panic，也尽力通过共享 core  detach；绝不停止 Minecraft。
    let shutdown = catch_unwind(AssertUnwindSafe(|| {
        runtime.block_on(manager.shutdown());
    }));
    runtime.shutdown_background();

    match operation {
        Ok(result) => result?,
        Err(_) => {
            return Err(AppError::internal(
                "TUI 发生异常，终端已恢复，Minecraft 会话保持运行",
            ));
        }
    }
    if shutdown.is_err() {
        return Err(AppError::internal("退出时 detach 任务发生异常"));
    }
    Ok(())
}

struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        Ok(Self { active: true })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
        self.active = false;
    }
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    runtime: &tokio::runtime::Runtime,
    manager: Arc<ServerManager>,
) -> Result<()> {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<Message>();
    let mut tasks = TaskBook::default();
    let mut app = App::new(manager.config(), manager.snapshots());

    loop {
        while let Ok(message) = receiver.try_recv() {
            app.apply_message(message);
        }
        app.sync_manager(manager.config(), manager.snapshots());
        let consoles: Vec<_> = app
            .snapshots
            .iter()
            .filter_map(|server| {
                manager
                    .console(server.id)
                    .map(|console| (server.id, console.lines()))
            })
            .collect();
        for (id, lines) in consoles {
            app.sync_console(id, lines);
        }

        dispatch_all(
            runtime,
            &manager,
            &sender,
            &mut tasks,
            app.ensure_current_panel(),
        );
        terminal.draw(|frame| render::render(frame, &app))?;
        if app.quit {
            break;
        }

        if event::poll(Duration::from_millis(80))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let commands = app.handle_key(key);
                    dispatch_all(runtime, &manager, &sender, &mut tasks, commands);
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
        tasks.reap();
    }
    tasks.abort_all();
    Ok(())
}

fn dispatch_all(
    runtime: &tokio::runtime::Runtime,
    manager: &Arc<ServerManager>,
    sender: &MessageSender,
    tasks: &mut TaskBook,
    commands: Vec<tasks::Command>,
) {
    for command in commands {
        tasks.dispatch(runtime, manager.clone(), sender.clone(), command);
    }
}
