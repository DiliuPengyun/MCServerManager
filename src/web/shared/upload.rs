use std::path::{Path, PathBuf};

use axum::{extract::Request, http::header};
use futures_util::StreamExt;
use serde_json::{Value, json};

use crate::{
    config::ServerId,
    core::ServerManager,
    error::{AppError, Result},
    server::files::FileBrowser,
};

use super::errors::{file_error, join_error};

const MIN_FREE_BYTES: u64 = 512 * 1024 * 1024;
const SPACE_CHECK_CHUNK: u64 = 16 * 1024 * 1024;

/// 单文件流式上传入口。
pub(crate) async fn file(
    manager: &ServerManager,
    id: ServerId,
    path: String,
    overwrite: bool,
    request: Request,
) -> Result<Value> {
    let server = manager.server_config(id)?;
    let space_dir = server.working_dir.clone();
    let browser = FileBrowser::new(server.working_dir);
    stream_upload(browser, space_dir, path, overwrite, false, request).await?;
    Ok(json!({"ok": true}))
}

/// 流式写入暂存文件并原子落盘；目录上传通过 `create_parents` 复用。
pub(crate) async fn stream_upload(
    browser: FileBrowser,
    space_dir: PathBuf,
    path: String,
    overwrite: bool,
    create_parents: bool,
    request: Request,
) -> Result<()> {
    if create_parents
        && let Some((parent, _)) = path.rsplit_once('/')
        && !parent.is_empty()
    {
        browser.create_dirs(parent).map_err(file_error)?;
    }
    let content_length = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    if content_length > 0 {
        ensure_space(&space_dir, content_length)?;
    }
    let mut upload = browser.begin_upload(&path, overwrite).map_err(file_error)?;
    let mut stream = request.into_body().into_data_stream();
    let mut pending_space_check = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| AppError::io("读取上传请求体失败", error))?;
        if chunk.is_empty() {
            continue;
        }
        pending_space_check = pending_space_check.saturating_add(chunk.len() as u64);
        if pending_space_check >= SPACE_CHECK_CHUNK {
            ensure_space(&space_dir, 0)?;
            pending_space_check = 0;
        }
        let (next, result) = tokio::task::spawn_blocking(move || {
            let mut upload = upload;
            let result = upload.write_chunk(&chunk);
            (upload, result)
        })
        .await
        .map_err(join_error)?;
        upload = next;
        result?;
    }
    tokio::task::spawn_blocking(move || upload.commit())
        .await
        .map_err(join_error)??;
    Ok(())
}

fn ensure_space(directory: &Path, incoming: u64) -> Result<()> {
    let Ok(available) = fs2::available_space(directory) else {
        return Ok(());
    };
    if available <= MIN_FREE_BYTES || incoming > available - MIN_FREE_BYTES {
        Err(AppError::no_space("服务器磁盘可用空间不足，已中止上传"))
    } else {
        Ok(())
    }
}
