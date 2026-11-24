use crate::error::{AppError, Result};

/// 把文件服务的内部错误码映射为公开 API 契约。
pub(crate) fn file_error(mut error: AppError) -> AppError {
    match error.code.as_str() {
        "binary_file" => {
            error.code = "not_text".into();
        }
        "file_too_large" => {
            error.code = "too_large".into();
        }
        _ => {}
    }
    error
}

pub(crate) fn join_error(error: tokio::task::JoinError) -> AppError {
    AppError::internal(format!("后台任务异常: {error}"))
}

/// 服务器名称校验。
pub(crate) fn validate_server_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\'])
        || name.contains("..")
        || name.chars().any(char::is_control)
    {
        Err(AppError::invalid(
            "invalid_name",
            "服务器名称无效（不能为空、为 . 或 ..，也不能包含 /、\\ 或控制字符）",
        ))
    } else {
        Ok(())
    }
}
