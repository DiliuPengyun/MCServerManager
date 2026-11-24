use std::fmt::{Display, Formatter};

/// 与界面无关的错误类别；Web 层据此映射 HTTP 状态码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidInput,
    NotFound,
    AlreadyExists,
    Conflict,
    Busy,
    Unavailable,
    Upstream,
    NoSpace,
    Io,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError {
    pub kind: ErrorKind,
    pub code: String,
    pub message: String,
}

impl AppError {
    pub fn new(kind: ErrorKind, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind,
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn invalid(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidInput, code, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, "not_found", message)
    }

    pub fn already_exists(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::AlreadyExists, "already_exists", message)
    }

    pub fn conflict(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Conflict, code, message)
    }

    pub fn busy(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Busy, "busy", message)
    }

    pub fn unavailable(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unavailable, code, message)
    }

    pub fn no_space(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NoSpace, "no_space", message)
    }

    pub fn io(context: impl Display, error: impl Display) -> Self {
        Self::new(ErrorKind::Io, "io_error", format!("{context}: {error}"))
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, "internal_error", message)
    }
}

impl Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::io("文件操作失败", value)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::invalid("invalid_json", format!("JSON 格式错误: {value}"))
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
