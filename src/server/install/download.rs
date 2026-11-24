use std::{io::ErrorKind as IoErrorKind, path::Path};

use futures_util::StreamExt;
use reqwest::Url;
use tempfile::{NamedTempFile, TempPath};
use tokio::{fs::File, io::AsyncWriteExt};

use crate::error::{AppError, Result};

use super::{DOWNLOAD_STALL_TIMEOUT, InstallService, REQUEST_TIMEOUT};

impl InstallService {
    /// 响应体逐块写入目标目录中的临时文件，成功后通过同文件系统 rename 原子落盘。
    /// 落盘前任一步骤失败都由 TempPath 自动清理，且 persist_noclobber 永不覆盖既有文件。
    /// 落盘后若目录同步失败会返回错误，但不会删除已经落盘的目标文件。
    pub(super) async fn download_file(&self, url: Url, destination: &Path) -> Result<()> {
        let response = tokio::time::timeout(REQUEST_TIMEOUT, self.client.get(url.clone()).send())
            .await
            .map_err(|_| {
                AppError::unavailable("download_timeout", format!("等待 {url} 下载响应超时"))
            })?
            .map_err(|error| {
                AppError::unavailable("download_failed", format!("下载 {url} 失败: {error}"))
            })?;
        if !response.status().is_success() {
            return Err(AppError::unavailable(
                "download_http_status",
                format!("下载 {url} 返回 HTTP {}", response.status()),
            ));
        }
        let parent = destination_parent(destination)?;
        let (mut file, temporary) = create_temporary(parent)?;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = tokio::time::timeout(DOWNLOAD_STALL_TIMEOUT, stream.next())
            .await
            .map_err(|_| {
                AppError::unavailable(
                    "download_stalled",
                    format!("下载 {url} 连续 120 秒没有收到数据"),
                )
            })?
        {
            let chunk = chunk.map_err(|error| {
                AppError::unavailable(
                    "download_interrupted",
                    format!("下载 {url} 的响应体中断: {error}"),
                )
            })?;
            file.write_all(&chunk).await.map_err(|error| {
                AppError::io(format!("写入下载临时文件 {}", destination.display()), error)
            })?;
        }
        persist_temporary(file, temporary, destination, "核心下载文件").await
    }
}

fn destination_parent(destination: &Path) -> Result<&Path> {
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(AppError::not_found(format!(
            "目标文件的父目录不存在: {}",
            parent.display()
        )));
    }
    Ok(parent)
}

fn create_temporary(parent: &Path) -> Result<(File, TempPath)> {
    let (file, temporary) = NamedTempFile::new_in(parent)
        .map_err(|error| AppError::io("创建下载临时文件失败", error))?
        .into_parts();
    Ok((File::from_std(file), temporary))
}

async fn persist_temporary(
    mut file: File,
    temporary: TempPath,
    destination: &Path,
    description: &str,
) -> Result<()> {
    file.flush()
        .await
        .map_err(|error| AppError::io(format!("刷新{description}失败"), error))?;
    file.sync_all()
        .await
        .map_err(|error| AppError::io(format!("同步{description}失败"), error))?;
    drop(file);
    temporary.persist_noclobber(destination).map_err(|error| {
        if error.error.kind() == IoErrorKind::AlreadyExists {
            AppError::already_exists(format!(
                "目标文件已存在，拒绝覆盖: {}",
                destination.display()
            ))
        } else {
            AppError::io(
                format!("原子保存{description}到 {}", destination.display()),
                &error.error,
            )
        }
    })?;
    if let Some(parent) = destination.parent() {
        let directory = File::open(parent)
            .await
            .map_err(|error| AppError::io("打开目标目录以同步失败", error))?;
        directory
            .sync_all()
            .await
            .map_err(|error| AppError::io("同步目标目录失败", error))?;
    }
    Ok(())
}

pub(super) async fn copy_file_atomic(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination_parent(destination)?;
    let mut source_file = File::open(source)
        .await
        .map_err(|error| AppError::io(format!("打开安装器 {}", source.display()), error))?;
    let (mut destination_file, temporary) = create_temporary(parent)?;
    tokio::io::copy(&mut source_file, &mut destination_file)
        .await
        .map_err(|error| AppError::io("复制安装器到临时文件失败", error))?;
    persist_temporary(destination_file, temporary, destination, "安装器副本").await
}
