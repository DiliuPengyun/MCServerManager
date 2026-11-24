use std::{
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    core::ServerManager,
    error::{AppError, Result},
};

pub fn run(manager: Arc<ServerManager>, port_override: Option<u16>) -> Result<()> {
    if let Some(port) = port_override {
        manager.update_web_port(port)?;
    }
    let port = port_override.unwrap_or_else(|| manager.config().web_port);
    let config_dir = manager.store().config_dir().to_path_buf();
    fs::create_dir_all(&config_dir)
        .map_err(|error| AppError::io("无法创建 Web 配置目录", error))?;
    let pid_path = manager.store().pid_path();
    let log_path = manager.store().log_path();
    let mut pid_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&pid_path)
        .map_err(|error| AppError::io("无法打开 web.pid", error))?;
    fs2::FileExt::try_lock_exclusive(&pid_file).map_err(|error| {
        let running = fs::read_to_string(&pid_path)
            .ok()
            .and_then(|pid| pid.trim().parse::<i32>().ok());
        AppError::conflict(
            "web_already_running",
            match running {
                Some(pid) => format!("Web 服务已在运行 (PID {pid})，请先结束现有进程"),
                None => format!("Web 服务已在运行，请先结束现有进程: {error}"),
            },
        )
    })?;

    let mut pipe = [0_i32; 2];
    // SAFETY：此时尚未创建 Tokio 运行时或后台线程，pipe/fork 不涉及其他线程状态。
    if unsafe { libc::pipe(pipe.as_mut_ptr()) } != 0 {
        return Err(AppError::io(
            "创建守护进程同步管道失败",
            std::io::Error::last_os_error(),
        ));
    }
    // SAFETY：同上，fork 前为单线程；子进程在创建运行时前仅执行 fd/文件操作。
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        close_fd(pipe[0]);
        close_fd(pipe[1]);
        return Err(AppError::io(
            "fork Web 守护进程失败",
            std::io::Error::last_os_error(),
        ));
    }
    if pid > 0 {
        close_fd(pipe[1]);
        drop(pid_file);
        let mut result = [0_u8; 1];
        // SAFETY：pipe[0] 是当前父进程持有的可读 fd，缓冲区长度明确为 1。
        let count = unsafe { libc::read(pipe[0], result.as_mut_ptr().cast(), 1) };
        close_fd(pipe[0]);
        if count == 1 && result[0] == b'O' {
            println!(
                "MCServerManager Web UI 已在后台启动 (PID {pid})\n本机访问:   http://127.0.0.1:{port}/\n局域网访问: http://<本机IP>:{port}/\n日志文件:   {}\n停止服务:   kill {pid}",
                log_path.display()
            );
            return Ok(());
        }
        let mut status = 0;
        // SAFETY：等待本次 fork 出的直接子进程，不接管其他进程。
        unsafe { libc::waitpid(pid, &mut status, 0) };
        return Err(AppError::unavailable(
            "web_start_failed",
            format!(
                "Web 服务启动失败，端口 {port} 可能被占用（详见 {}）",
                log_path.display()
            ),
        ));
    }

    close_fd(pipe[0]);
    // SAFETY：setsid 在刚从单线程进程 fork 出的子进程中调用。
    if unsafe { libc::setsid() } < 0 {
        notify(pipe[1], b'E');
        libc_exit(1);
    }
    redirect_stdio(&log_path);
    pid_file
        .set_len(0)
        .and_then(|()| pid_file.seek(SeekFrom::Start(0)).map(|_| ()))
        .and_then(|()| writeln!(pid_file, "{}", std::process::id()))
        .and_then(|()| pid_file.sync_all())
        .unwrap_or_else(|error| {
            eprintln!("无法写入 web.pid: {error}");
            notify(pipe[1], b'E');
            libc_exit(1);
        });
    child_main(manager, port, pipe[1], pid_path, pid_file)
}

fn child_main(
    manager: Arc<ServerManager>,
    port: u16,
    notify_fd: i32,
    pid_path: PathBuf,
    pid_file: File,
) -> ! {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("无法创建异步运行时: {error}");
            notify(notify_fd, b'E');
            libc_exit(1);
        }
    };
    let result = runtime.block_on(async {
        let listener = match tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("无法绑定 Web 端口 {port}: {error}");
                notify(notify_fd, b'E');
                return Err(AppError::io("Web 端口绑定失败", error));
            }
        };
        notify(notify_fd, b'O');
        close_fd(notify_fd);
        super::serve_listener(manager, listener).await
    });
    if let Err(error) = &result {
        eprintln!("Web 服务异常退出: {error}");
    }
    drop(runtime);
    let _ = fs::remove_file(pid_path);
    drop(pid_file);
    libc_exit(if result.is_ok() { 0 } else { 1 })
}

fn redirect_stdio(log_path: &Path) {
    if let Ok(devnull) = File::open("/dev/null") {
        // SAFETY：dup2 使用有效的已打开 fd，目标为标准输入。
        unsafe { libc::dup2(devnull.as_raw_fd(), libc::STDIN_FILENO) };
    }
    if let Ok(log) = OpenOptions::new().create(true).append(true).open(log_path) {
        // SAFETY：dup2 使用有效的日志 fd，目标为标准输出和错误输出。
        unsafe {
            libc::dup2(log.as_raw_fd(), libc::STDOUT_FILENO);
            libc::dup2(log.as_raw_fd(), libc::STDERR_FILENO);
        }
    }
}

fn notify(fd: i32, value: u8) {
    // SAFETY：写入一个字节到本次进程持有的管道 fd；失败由父进程读到 EOF 处理。
    unsafe { libc::write(fd, (&value as *const u8).cast(), 1) };
}

fn close_fd(fd: i32) {
    // SAFETY：关闭本函数调用方明确持有的 fd；重复关闭由内核返回错误，不影响其他 fd。
    unsafe { libc::close(fd) };
}

fn libc_exit(code: i32) -> ! {
    // SAFETY：守护子进程已完成清理，使用 _exit 避免重复运行 fork 前继承的退出处理器。
    unsafe { libc::_exit(code) }
}
