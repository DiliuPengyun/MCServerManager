use std::path::{Path, PathBuf};

use crate::AppError;

use super::TaskResult;

pub(in crate::ui::tasks) mod backups;
pub(in crate::ui::tasks) mod files;
pub(in crate::ui::tasks) mod install;
pub(in crate::ui::tasks) mod lifecycle;
pub(in crate::ui::tasks) mod properties;
pub(in crate::ui::tasks) mod simple;
pub(in crate::ui::tasks) mod versions;

pub(in crate::ui::tasks) fn parent_path(path: &Path) -> PathBuf {
    path.parent().unwrap_or_else(|| Path::new("")).to_path_buf()
}

async fn blocking<T, F, E>(operation: F) -> TaskResult<T>
where
    T: Send + 'static,
    E: std::fmt::Display + Send + 'static,
    F: FnOnce() -> std::result::Result<T, E> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| format!("后台任务异常: {error}"))?
        .map_err(|error| error.to_string())
}

fn error_text(error: AppError) -> String {
    error.message
}
