use std::{
    ffi::OsStr,
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Output,
    time::Duration,
};

use tokio::process::Command;

use crate::{
    error::{AppError, Result},
    server::launch::LaunchConfig,
};

const TMUX_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionStatus {
    Exists,
    Missing,
    Unknown,
}

/// 只通过参数数组调用 tmux；唯一的命令字符串是单引号转义后的 `exec <私有启动脚本>`，
/// 由 tmux 交给 shell 执行。
#[derive(Clone, Debug)]
pub struct Tmux {
    runtime_dir: PathBuf,
    isolated_socket: Option<String>,
}

impl Tmux {
    pub fn new(runtime_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&runtime_dir)
            .map_err(|error| AppError::io("无法创建私有运行目录", error))?;
        fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700))
            .map_err(|error| AppError::io("无法保护私有运行目录", error))?;
        Ok(Self {
            runtime_dir,
            isolated_socket: None,
        })
    }

    #[cfg(test)]
    pub fn isolated(runtime_dir: PathBuf, socket: String) -> Result<Self> {
        let mut tmux = Self::new(runtime_dir)?;
        tmux.isolated_socket = Some(socket);
        Ok(tmux)
    }

    pub async fn status(&self, session: &str) -> SessionStatus {
        match self
            .run(&["has-session", "-t", &exact_target(session)])
            .await
        {
            Ok(output) if output.status.success() => SessionStatus::Exists,
            Ok(output) if output.status.code() == Some(1) => SessionStatus::Missing,
            _ => SessionStatus::Unknown,
        }
    }

    pub async fn list_sessions(&self) -> Vec<String> {
        let Ok(output) = self.run(&["list-sessions", "-F", "#{session_name}"]).await else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    }

    pub async fn pane_current_path(&self, session: &str) -> Option<PathBuf> {
        let output = self
            .run(&[
                "list-panes",
                "-t",
                &exact_target(session),
                "-F",
                "#{pane_current_path}",
            ])
            .await
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    pub async fn create_session(
        &self,
        session: &str,
        launch: &LaunchConfig,
        log_path: &Path,
    ) -> Result<PathBuf> {
        let script = self.write_script(session, launch, log_path)?;
        let command = format!("exec {}", shell_quote(script.as_os_str()));
        let working = launch.working_dir.to_string_lossy().into_owned();
        let result = self
            .run(&["new-session", "-d", "-s", session, "-c", &working, &command])
            .await?;
        if !result.status.success() {
            let _ = fs::remove_file(&script);
            return Err(AppError::unavailable(
                "tmux_start_failed",
                format!("tmux new-session 执行失败: {}", stderr(&result)),
            ));
        }
        let _ = self
            .run(&[
                "set-option",
                "-t",
                &exact_target(session),
                "history-limit",
                "50000",
            ])
            .await;
        Ok(script)
    }

    pub async fn send_command(&self, session: &str, command: &str) -> Result<()> {
        if command.is_empty() || command.contains(['\r', '\n', '\0']) {
            return Err(AppError::invalid(
                "invalid_command",
                "控制台指令必须是非空单行文本",
            ));
        }
        let target = exact_target(session);
        let literal = self
            .run(&["send-keys", "-t", &target, "-l", "--", command])
            .await?;
        if !literal.status.success() {
            return Err(AppError::unavailable(
                "tmux_send_failed",
                format!("无法向 tmux 发送指令: {}", stderr(&literal)),
            ));
        }
        let enter = self.run(&["send-keys", "-t", &target, "Enter"]).await?;
        if !enter.status.success() {
            return Err(AppError::unavailable(
                "tmux_send_failed",
                format!("无法向 tmux 发送回车: {}", stderr(&enter)),
            ));
        }
        Ok(())
    }

    pub async fn kill_session(&self, session: &str, script: Option<&Path>) -> Result<()> {
        let output = self
            .run(&["kill-session", "-t", &exact_target(session)])
            .await?;
        if let Some(script) = script {
            let _ = fs::remove_file(script);
        }
        if output.status.success() || output.status.code() == Some(1) {
            Ok(())
        } else {
            Err(AppError::unavailable(
                "tmux_stop_failed",
                format!("tmux kill-session 执行失败: {}", stderr(&output)),
            ))
        }
    }

    pub async fn remove_script(&self, script: &Path) {
        let _ = fs::remove_file(script);
    }

    async fn run(&self, arguments: &[&str]) -> Result<Output> {
        let mut command = Command::new("tmux");
        if let Some(socket) = &self.isolated_socket {
            command.arg("-L").arg(socket);
        }
        let output = tokio::time::timeout(TMUX_TIMEOUT, command.args(arguments).output())
            .await
            .map_err(|_| AppError::unavailable("tmux_timeout", "tmux 命令超过 5 秒未响应"))?
            .map_err(|error| AppError::io("无法执行 tmux", error))?;
        Ok(output)
    }

    fn write_script(
        &self,
        session: &str,
        launch: &LaunchConfig,
        log_path: &Path,
    ) -> Result<PathBuf> {
        let path = self.runtime_dir.join(format!("{session}.sh"));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o700)
            .open(&path)
            .map_err(|error| AppError::io("无法创建 tmux 启动脚本", error))?;
        file.set_permissions(fs::Permissions::from_mode(0o700))?;
        file.write_all(b"#!/bin/sh\nset -f\ncd ")?;
        file.write_all(shell_quote(launch.working_dir.as_os_str()).as_bytes())?;
        file.write_all(b" || exit 1\n")?;
        file.write_all(shell_quote(OsStr::new(&launch.program)).as_bytes())?;
        for argument in &launch.args {
            file.write_all(b" ")?;
            file.write_all(shell_quote(argument).as_bytes())?;
        }
        file.write_all(b" 2>&1 | tee -a ")?;
        file.write_all(shell_quote(log_path.as_os_str()).as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(path)
    }
}

fn exact_target(session: &str) -> String {
    // 末尾冒号让 target-pane 明确按“精确会话的当前窗格”解析；
    // 只有 =name 时，send-keys 在 tmux 3.4 会误当成窗格名而找不到目标。
    format!("={session}:")
}

fn stderr(output: &Output) -> String {
    let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if message.is_empty() {
        format!("退出码 {:?}", output.status.code())
    } else {
        message
    }
}

/// 仅供私有启动脚本使用；所有外部输入都作为单引号字面量写入。
fn shell_quote(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    if value.is_empty() {
        return "''".into();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn sanitize_session_name(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_names_and_script_quotes_cannot_inject_commands() {
        assert_eq!(sanitize_session_name("a.b:c/中文"), "a_b_c___");
        assert_eq!(
            shell_quote(OsStr::new("a b'touch /tmp/no")),
            "'a b'\\''touch /tmp/no'"
        );
    }
}
