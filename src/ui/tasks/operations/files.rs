use std::{path::PathBuf, sync::Arc};

use crate::{
    AppError,
    config::ServerId,
    core::ServerManager,
    server::files::{FileBrowser, FileEntry},
};

use super::super::TaskResult;
use super::{blocking, error_text};

pub(in crate::ui::tasks) async fn list(
    manager: &Arc<ServerManager>,
    id: ServerId,
    path: PathBuf,
) -> TaskResult<Vec<FileEntry>> {
    with_browser(manager, id, move |browser| browser.list(path)).await
}

pub(in crate::ui::tasks) async fn read(
    manager: &Arc<ServerManager>,
    id: ServerId,
    path: PathBuf,
) -> TaskResult<String> {
    with_browser(manager, id, move |browser| browser.read_text(path)).await
}

pub(in crate::ui::tasks) async fn write(
    manager: &Arc<ServerManager>,
    id: ServerId,
    path: PathBuf,
    content: String,
) -> TaskResult<()> {
    with_browser(manager, id, move |browser| {
        browser.write_text(path, &content)
    })
    .await
}

pub(in crate::ui::tasks) async fn create(
    manager: &Arc<ServerManager>,
    id: ServerId,
    path: PathBuf,
    is_dir: bool,
) -> TaskResult<String> {
    with_browser(manager, id, move |browser| {
        browser.create(path, is_dir)?;
        Ok::<_, AppError>(
            if is_dir {
                "目录已创建"
            } else {
                "文件已创建"
            }
            .to_owned(),
        )
    })
    .await
}

pub(in crate::ui::tasks) async fn rename(
    manager: &Arc<ServerManager>,
    id: ServerId,
    path: PathBuf,
    new_name: String,
) -> TaskResult<String> {
    with_browser(manager, id, move |browser| {
        browser.rename(path, new_name)?;
        Ok::<_, AppError>("已重命名".to_owned())
    })
    .await
}

pub(in crate::ui::tasks) async fn delete(
    manager: &Arc<ServerManager>,
    id: ServerId,
    path: PathBuf,
) -> TaskResult<String> {
    with_browser(manager, id, move |browser| {
        browser.remove(path)?;
        Ok::<_, AppError>("已删除".to_owned())
    })
    .await
}

/// 统一 FileBrowser 构造与 spawn_blocking 样板，具体操作仍在阻塞线程执行。
async fn with_browser<T, F>(
    manager: &Arc<ServerManager>,
    id: ServerId,
    operation: F,
) -> TaskResult<T>
where
    T: Send + 'static,
    F: FnOnce(FileBrowser) -> std::result::Result<T, AppError> + Send + 'static,
{
    let browser = manager
        .server_config(id)
        .map(|server| FileBrowser::new(server.working_dir))
        .map_err(error_text)?;
    blocking(move || operation(browser)).await
}
