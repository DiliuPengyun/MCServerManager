use std::{
    collections::VecDeque,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

use crate::error::{AppError, ErrorKind, Result};

use super::{INSTALLER_ERROR_TAIL_LINES, MAX_INSTALLER_LINE_BYTES, ProgressCallback, notify};

pub(super) async fn run_forge_installer(
    java_path: &str,
    installer_path: &Path,
    target_dir: &Path,
    progress: Option<&ProgressCallback>,
) -> Result<()> {
    let arguments = vec![
        OsString::from("-Djava.awt.headless=true"),
        OsString::from("-jar"),
        installer_path.as_os_str().to_owned(),
        OsString::from("--installServer"),
    ];
    run_java_command(java_path, arguments, target_dir, "Java 安装器", progress).await
}

fn resolve_java_program(java_path: &str) -> Result<OsString> {
    let path = Path::new(java_path);
    if path.is_absolute()
        || path
            .parent()
            .is_some_and(|parent| parent.as_os_str().is_empty())
    {
        // 裸名称继续由 PATH 解析，保持配置中的 java 写法。
        return Ok(path.as_os_str().to_owned());
    }
    // current_dir 只影响安装器；相对 Java 路径仍按调用方进程目录解析。
    std::path::absolute(path)
        .map(PathBuf::into_os_string)
        .map_err(|error| AppError::io(format!("无法解析 Java 路径 {java_path}"), error))
}

pub(super) async fn run_java_command(
    java_path: &str,
    arguments: Vec<OsString>,
    target_dir: &Path,
    display_name: &str,
    progress: Option<&ProgressCallback>,
) -> Result<()> {
    if java_path.is_empty() {
        return Err(AppError::invalid(
            "invalid_java",
            "Java 可执行文件路径不能为空",
        ));
    }
    let java_program = resolve_java_program(java_path)?;
    let mut command = Command::new(java_program);
    command
        .args(arguments)
        .current_dir(target_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|error| {
        let app_error = AppError::unavailable(
            "installer_spawn_failed",
            format!("无法启动{display_name}（Java: {java_path}）: {error}"),
        );
        notify(progress, &app_error.message);
        app_error
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::internal("无法接管安装器标准输出"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::internal("无法接管安装器错误输出"))?;
    let tail = Arc::new(Mutex::new(VecDeque::<String>::new()));
    let stdout_task = tokio::spawn(read_installer_output(
        stdout,
        tail.clone(),
        progress.cloned(),
    ));
    let stderr_task = tokio::spawn(read_installer_output(
        stderr,
        tail.clone(),
        progress.cloned(),
    ));

    let status = child.wait().await;
    let stdout_result = stdout_task.await;
    let stderr_result = stderr_task.await;
    for result in [stdout_result, stderr_result] {
        result.map_err(|error| AppError::internal(format!("安装器输出任务失败: {error}")))??;
    }
    let status = status.map_err(|error| AppError::io("等待 Java 安装器退出失败", error))?;
    if !status.success() {
        let status_text = status
            .code()
            .map_or_else(|| "被信号终止".to_owned(), |code| code.to_string());
        let mut message = format!("{display_name}执行失败，退出状态: {status_text}");
        let tail = tail.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if !tail.is_empty() {
            message.push_str(":\n  ");
            message.push_str(&tail.iter().cloned().collect::<Vec<_>>().join("\n  "));
        }
        drop(tail);
        notify(progress, &message);
        return Err(AppError::new(
            ErrorKind::Internal,
            "installer_failed",
            message,
        ));
    }
    Ok(())
}

async fn read_installer_output<R>(
    mut reader: R,
    tail: Arc<Mutex<VecDeque<String>>>,
    progress: Option<ProgressCallback>,
) -> Result<()>
where
    R: AsyncRead + Unpin,
{
    let mut lines = InstallerLines::default();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader
            .read(&mut buffer)
            .await
            .map_err(|error| AppError::io("读取 Java 安装器输出失败", error))?;
        if count == 0 {
            break;
        }
        lines.push(&buffer[..count], &mut |line| {
            emit_installer_line(line, &tail, progress.as_ref());
        });
    }
    lines.finish(&mut |line| {
        emit_installer_line(line, &tail, progress.as_ref());
    });
    Ok(())
}

fn emit_installer_line(
    line: String,
    tail: &Mutex<VecDeque<String>>,
    progress: Option<&ProgressCallback>,
) {
    {
        let mut tail = tail.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        tail.push_back(line.clone());
        while tail.len() > INSTALLER_ERROR_TAIL_LINES {
            tail.pop_front();
        }
    }
    if let Some(progress) = progress {
        progress(&line);
    }
}

#[derive(Default)]
pub(super) struct InstallerLines {
    pending: Vec<u8>,
    discarding: bool,
}

impl InstallerLines {
    pub(super) fn push<F>(&mut self, mut chunk: &[u8], emit: &mut F)
    where
        F: FnMut(String),
    {
        while !chunk.is_empty() {
            if self.discarding {
                if let Some(newline) = chunk.iter().position(|byte| *byte == b'\n') {
                    self.discarding = false;
                    chunk = &chunk[newline + 1..];
                    continue;
                }
                return;
            }
            let available = MAX_INSTALLER_LINE_BYTES.saturating_sub(self.pending.len());
            if let Some(newline) = chunk.iter().position(|byte| *byte == b'\n') {
                let segment = &chunk[..newline];
                let take = segment.len().min(available);
                self.pending.extend_from_slice(&segment[..take]);
                let truncated = take < segment.len();
                chunk = &chunk[newline + 1..];
                self.emit(truncated, emit);
            } else if chunk.len() <= available {
                self.pending.extend_from_slice(chunk);
                return;
            } else {
                self.pending.extend_from_slice(&chunk[..available]);
                self.emit(true, emit);
                self.discarding = true;
                return;
            }
        }
    }

    pub(super) fn finish<F>(&mut self, emit: &mut F)
    where
        F: FnMut(String),
    {
        if !self.discarding && !self.pending.is_empty() {
            self.emit(false, emit);
        }
        self.discarding = false;
    }

    fn emit<F>(&mut self, truncated: bool, emit: &mut F)
    where
        F: FnMut(String),
    {
        if self.pending.last() == Some(&b'\r') {
            self.pending.pop();
        }
        let mut line = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        if truncated {
            line.push_str(" …[输出过长已截断]");
        }
        emit(line);
    }
}

pub(super) async fn ensure_user_jvm_args(target_dir: &Path) {
    let path = target_dir.join("user_jvm_args.txt");
    let mut file = match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
    {
        Ok(file) => file,
        Err(_) => return,
    };
    // 与官方 Forge/NeoForge 安装后的兼容文件保持一致；绝不覆盖用户已有配置。
    let _ = file
        .write_all(
            b"# Xmx and Xms set the maximum and minimum RAM usage, respectively.\n\
# They can take any number, followed by an M or a G.\n\
# M means Megabyte, G means Gigabyte.\n\
# For example, to set the maximum to 3GB: -Xmx3G\n\
# To set the minimum to 2.5GB: -Xms2500M\n\
# A good default for a modded server is 4GB.\n\
# Uncomment the next line to set it.\n\
# -Xmx4G\n",
        )
        .await;
}
