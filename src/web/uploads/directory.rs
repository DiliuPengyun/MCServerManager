use std::{fs, path::PathBuf, sync::Arc};

use axum::{
    Json,
    body::Bytes,
    extract::{Path as AxumPath, Query, Request, State},
};
use serde_json::{Value, json};

use super::super::{
    WebState,
    shared::{
        body::parse_body,
        errors::{file_error, join_error, validate_server_name},
        upload::stream_upload,
    },
    tasks::{TaskRef, WebTasks},
};
use super::registration::{configured_server, ensure_name_available, ensure_target_available};
use crate::{
    core::ServerManager,
    error::{AppError, ErrorKind, Result},
    server::{detect, files::FileBrowser},
};

pub(super) async fn create_upload(
    State(state): State<WebState>,
    body: Bytes,
) -> Result<Json<Value>> {
    let body = parse_body(&body)?;
    let name = body.get("name").and_then(Value::as_str).unwrap_or("");
    validate_server_name(name)?;
    if name.starts_with('.') {
        return Err(AppError::invalid(
            "invalid_name",
            "服务器名称不能以点号开头",
        ));
    }
    ensure_name_available(&state.manager, name)?;
    let parent = state.manager.config().default_server_dir.clone();
    fs::create_dir_all(&parent).map_err(|error| AppError::io("无法创建默认服务器目录", error))?;
    let parent = parent
        .canonicalize()
        .map_err(|error| AppError::io("无法解析默认服务器目录", error))?;
    let target = parent.join(name);
    ensure_target_available(&state.manager, &target)?;

    let task = state.tasks.create();
    let (id, unique) = {
        let mut task_guard = task.lock();
        task_guard.directory_upload = true;
        task_guard.request_body = body;
        task_guard.target_dir = target.clone();
        (task_guard.id, crate::config::ServerId::new().simple())
    };
    let staging = parent.join(format!(".mcsm-upload-{id}-{unique}"));
    if let Err(error) = fs::create_dir(&staging) {
        let error = AppError::io("无法创建上传暂存目录", error);
        WebTasks::finish(&task, Err(error.clone()));
        return Err(error);
    }
    task.lock().staging_dir = staging;
    WebTasks::append(&task, "上传任务已创建，等待接收文件");
    Ok(Json(
        json!({"ok": true, "task_id": id, "target_dir": target.to_string_lossy()}),
    ))
}

struct ActivityGuard(TaskRef);

impl Drop for ActivityGuard {
    fn drop(&mut self) {
        WebTasks::finish_activity(&self.0);
    }
}

fn begin_activity(task: &TaskRef) -> Result<(ActivityGuard, PathBuf)> {
    let mut task_guard = task.lock();
    if task_guard.status != "running" || task_guard.finalizing || task_guard.cancelled {
        return Err(AppError::conflict(
            "upload_not_active",
            "上传任务当前不可写入",
        ));
    }
    task_guard.active_uploads += 1;
    Ok((ActivityGuard(task.clone()), task_guard.staging_dir.clone()))
}

pub(super) async fn create_directory(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<u64>,
    body: Bytes,
) -> Result<Json<Value>> {
    let body = parse_body(&body)?;
    let path = body
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid("bad_request", "缺少目录路径"))?;
    let task = directory_task(&state, id)?;
    let (_activity, staging) = begin_activity(&task)?;
    FileBrowser::new(staging)
        .create_dirs(path)
        .map_err(file_error)?;
    Ok(Json(json!({"ok": true})))
}

pub(super) async fn upload_part(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<u64>,
    Query(query): Query<std::collections::HashMap<String, String>>,
    request: Request,
) -> Result<Json<Value>> {
    let path = query
        .get("path")
        .cloned()
        .ok_or_else(|| AppError::invalid("bad_request", "缺少文件路径"))?;
    let task = directory_task(&state, id)?;
    let (_activity, staging) = begin_activity(&task)?;
    let browser = FileBrowser::new(staging.clone());
    stream_upload(browser, staging, path, true, true, request).await?;
    Ok(Json(json!({"ok": true})))
}

pub(super) async fn finish_upload(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<u64>,
) -> Result<Json<Value>> {
    let task = directory_task(&state, id)?;
    let body = {
        let mut task_guard = task.lock();
        if task_guard.cancelled {
            return Err(AppError::conflict("upload_cancelled", "上传已取消"));
        }
        if task_guard.finalizing {
            return Ok(Json(
                json!({"ok": true, "task_id": id, "already_finalizing": true}),
            ));
        }
        if task_guard.active_uploads > 0 {
            return Err(AppError::conflict(
                "upload_still_active",
                "仍有文件正在上传",
            ));
        }
        if task_guard.status == "done" {
            return Ok(Json(json!({"ok": true, "task_id": id})));
        }
        task_guard.finalizing = true;
        task_guard.status = "running".into();
        task_guard.error.clear();
        let mut body = task_guard.request_body.clone();
        body["method"] = json!("upload");
        body["path"] = json!(task_guard.staging_dir.to_string_lossy());
        body["upload_target_dir"] = json!(task_guard.target_dir.to_string_lossy());
        body
    };
    WebTasks::append(&task, "文件接收完成，正在识别服务端核心并注册");
    let manager = state.manager.clone();
    let background = task.clone();
    if let Err(error) = state
        .jobs
        .spawn(async move {
            let result = finalize_directory(manager, body).await;
            if let Err(error) = &result {
                WebTasks::append(&background, format!("错误: {error}"));
            }
            WebTasks::finish(&background, result);
        })
        .await
    {
        WebTasks::finish(&task, Err(error.clone()));
        return Err(error);
    }
    Ok(Json(json!({"ok": true, "task_id": id})))
}

async fn finalize_directory(manager: Arc<ServerManager>, body: Value) -> Result<()> {
    let staging = PathBuf::from(body.get("path").and_then(Value::as_str).unwrap_or(""));
    let target = PathBuf::from(
        body.get("upload_target_dir")
            .and_then(Value::as_str)
            .unwrap_or(""),
    );
    ensure_name_available(
        &manager,
        body.get("name").and_then(Value::as_str).unwrap_or(""),
    )?;
    ensure_target_available(&manager, &target)?;
    let detected = tokio::task::spawn_blocking({
        let staging = staging.clone();
        let target = target.clone();
        move || {
            fs::rename(&staging, &target)
                .map_err(|error| AppError::io("无法将上传目录移动到服务器目录", error))?;
            match detect::detect_directory(&target) {
                Some(detected) => Ok(detected),
                None => {
                    let rollback = fs::rename(&target, &staging);
                    let message = match rollback {
                        Ok(()) => "无法在上传目录中识别服务端核心".into(),
                        Err(error) => format!("无法识别服务端核心；恢复原暂存目录失败: {error}"),
                    };
                    Err(AppError::invalid("core_not_found", message))
                }
            }
        }
    })
    .await
    .map_err(join_error)??;
    let server = configured_server(&body, target.clone(), detected)?;
    if let Err(error) = manager.add_server(server) {
        let _ = fs::rename(target, staging);
        return Err(error);
    }
    Ok(())
}

pub(super) async fn cancel_upload(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<u64>,
) -> Result<Json<Value>> {
    let task = directory_task(&state, id)?;
    let staging = {
        let mut task_guard = task.lock();
        if task_guard.status == "done" {
            return Err(AppError::conflict(
                "upload_already_finished",
                "上传已经完成",
            ));
        }
        if task_guard.finalizing || task_guard.active_uploads > 0 {
            return Err(AppError::conflict(
                "upload_still_active",
                "上传正在写入或收尾",
            ));
        }
        task_guard.cancelled = true;
        task_guard.status = "error".into();
        task_guard.error = "上传已取消".into();
        task_guard.staging_dir.clone()
    };
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| AppError::io("清理暂存目录失败", error))?;
    }
    Ok(Json(json!({"ok": true})))
}

fn directory_task(state: &WebState, id: u64) -> Result<TaskRef> {
    let task = state.tasks.find(id)?;
    if !task.lock().directory_upload {
        return Err(AppError::new(
            ErrorKind::NotFound,
            "task_not_found",
            "上传任务不存在",
        ));
    }
    Ok(task)
}
