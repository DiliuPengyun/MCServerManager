use std::{collections::HashMap, future::Future, path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path as AxumPath, Query, Request, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use futures_util::FutureExt;
use serde_json::{Value, json};

use crate::{
    config::ServerId,
    core::{ServerManager, ServerState, provisioning},
    error::{AppError, Result},
};

use super::{
    WebState,
    operations::{OperationRef, Operations},
    shared::{self, body::optional_body, errors::join_error},
};

pub fn routes() -> Router<WebState> {
    Router::new()
        .route("/api/capabilities", get(capabilities))
        .route("/api/operations/{id}", get(get_operation))
        .route("/api/servers", get(list_servers))
        .route("/api/servers/{id}", get(show_server).delete(remove_server))
        .route(
            "/api/servers/{id}/config",
            get(show_server).put(update_config),
        )
        .route("/api/servers/{id}/start", post(start_server))
        .route("/api/servers/{id}/stop", post(stop_server))
        .route("/api/servers/{id}/restart", post(restart_server))
        .route("/api/servers/{id}/eula", post(agree_eula))
        .route("/api/servers/{id}/rcon", post(enable_rcon))
        .route("/api/servers/{id}/install", post(install_server))
        .route("/api/servers/{id}/move", post(move_server))
        .route("/api/servers/{id}/relocate", post(relocate_server))
        .route("/api/servers/{id}/command", post(send_command))
        .route("/api/servers/{id}/console", get(console_snapshot))
        .route("/api/servers/{id}/console/stream", get(console_stream))
        .route(
            "/api/servers/{id}/properties",
            get(get_properties).put(set_properties),
        )
        .route(
            "/api/servers/{id}/backups",
            get(list_backups).post(create_backup),
        )
        .route(
            "/api/servers/{id}/backups/{name}/restore",
            post(restore_backup),
        )
        .route("/api/servers/{id}/backups/{name}", delete(delete_backup))
        .route(
            "/api/servers/{id}/files",
            get(list_files).post(create_file).delete(remove_file),
        )
        .route(
            "/api/servers/{id}/files/content",
            get(read_file).put(write_file),
        )
        .route("/api/servers/{id}/files/rename", put(rename_file))
        .route(
            "/api/servers/{id}/files/upload",
            post(upload_file).layer(axum::extract::DefaultBodyLimit::disable()),
        )
}

async fn capabilities() -> Json<Value> {
    Json(json!({
        "schema_version": 2,
        "stable_server_ids": true,
        "operation_tracking": true,
        "idempotency_keys": true,
        "directory_move": true,
        "directory_relocate": true,
        "explicit_file_deletion": true,
        "chunked_buffered_json": false,
    }))
}

async fn list_servers(State(state): State<WebState>) -> Result<Json<Value>> {
    Ok(Json(shared::json::servers_json(&state.manager)?))
}

async fn show_server(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
) -> Result<Json<Value>> {
    Ok(Json(shared::json::server_json(&state.manager, id)?))
}

async fn get_operation(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>> {
    Ok(Json(state.operations.get(&id)?.lock().json()))
}

async fn submit<F>(
    state: &WebState,
    server_id: ServerId,
    action: &str,
    headers: &HeaderMap,
    request: Value,
    future: F,
) -> Result<Json<Value>>
where
    F: Future<Output = Result<Value>> + Send + 'static,
{
    submit_inner(
        state,
        server_id,
        action,
        headers,
        request,
        move |_| future,
        false,
    )
    .await
}

async fn submit_coalesced_with<F, Fut>(
    state: &WebState,
    server_id: ServerId,
    action: &str,
    headers: &HeaderMap,
    request: Value,
    future: F,
) -> Result<Json<Value>>
where
    F: FnOnce(OperationRef) -> Fut + Send,
    Fut: Future<Output = Result<Value>> + Send + 'static,
{
    submit_inner(state, server_id, action, headers, request, future, true).await
}

async fn submit_inner<F, Fut>(
    state: &WebState,
    server_id: ServerId,
    action: &str,
    headers: &HeaderMap,
    request: Value,
    future: F,
    coalesce_running: bool,
) -> Result<Json<Value>>
where
    F: FnOnce(OperationRef) -> Fut + Send,
    Fut: Future<Output = Result<Value>> + Send + 'static,
{
    let key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok());
    let (operation, reused) = if coalesce_running {
        state
            .operations
            .begin_coalesced(Some(server_id), action, key, &request)?
    } else {
        state
            .operations
            .begin(Some(server_id), action, key, &request)?
    };
    if reused {
        return Ok(Json(operation.lock().json()));
    }
    let future = future(operation.clone());
    let background = operation.clone();
    if let Err(error) = state
        .jobs
        .spawn(async move {
            let result = match std::panic::AssertUnwindSafe(future).catch_unwind().await {
                Ok(result) => result,
                Err(_) => Err(AppError::internal("后台操作异常终止")),
            };
            Operations::finish(&background, result);
        })
        .await
    {
        Operations::finish(&operation, Err(error.clone()));
        return Err(error);
    }
    Ok(Json(operation.lock().json()))
}

async fn start_server(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>> {
    let body = optional_body(&body)?;
    let allow_no_rcon = body
        .get("allow_no_rcon")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let manager = state.manager.clone();
    submit(&state, id, "start", &headers, body, async move {
        let result = shared::lifecycle::start(&manager, id, allow_no_rcon).await?;
        shared::json::start_value(result)
    })
    .await
}

async fn stop_server(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>> {
    let body = optional_body(&body)?;
    let manager = state.manager.clone();
    submit(&state, id, "stop", &headers, body, async move {
        let state = shared::lifecycle::stop(&manager, id).await?;
        Ok(json!({"ok": true, "state": state}))
    })
    .await
}

async fn restart_server(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>> {
    let body = optional_body(&body)?;
    let allow_no_rcon = body
        .get("allow_no_rcon")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let manager = state.manager.clone();
    submit(&state, id, "restart", &headers, body, async move {
        let result = shared::lifecycle::restart(&manager, id, allow_no_rcon).await?;
        shared::json::start_value(result)
    })
    .await
}

async fn agree_eula(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>> {
    let body = optional_body(&body)?;
    let manager = state.manager.clone();
    submit(&state, id, "eula", &headers, body, async move {
        let result = shared::lifecycle::agree_eula(&manager, id).await?;
        shared::json::start_value(result)
    })
    .await
}

async fn enable_rcon(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>> {
    let body = optional_body(&body)?;
    let manager = state.manager.clone();
    submit(&state, id, "rcon", &headers, body, async move {
        let result = shared::lifecycle::enable_rcon(&manager, id).await?;
        shared::json::start_value(result)
    })
    .await
}

async fn install_server(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>> {
    let body = optional_body(&body)?;
    let manager = state.manager.clone();
    let installer = state.installer.clone();
    submit_coalesced_with(
        &state,
        id,
        "install",
        &headers,
        body,
        move |operation| async move {
            let _work = manager.begin_server_work(id)?;
            if !manager.needs_installer(id)? {
                return Err(AppError::conflict(
                    "installer_not_required",
                    "该服务器当前不需要运行安装器",
                ));
            }
            if manager.state(id) != ServerState::Stopped {
                return Err(AppError::conflict(
                    "server_active",
                    "请先停止服务器，再运行安装器",
                ));
            }
            let progress_operation = operation.clone();
            let progress_manager = manager.clone();
            let progress = Arc::new(move |line: &str| {
                Operations::append(&progress_operation, line);
                let _ = progress_manager.append_console_line(id, line);
            });
            run_install_with_progress(manager, installer, id, progress).await?;
            let log = operation.lock().log.lines().to_vec();
            Ok(json!({"ok": true, "log": log}))
        },
    )
    .await
}

async fn run_install_with_progress(
    manager: Arc<ServerManager>,
    installer: Arc<crate::server::install::InstallService>,
    id: ServerId,
    progress: crate::server::install::ProgressCallback,
) -> Result<()> {
    let server = manager.server_config(id)?;
    let directory = server.working_dir.clone();
    let installer_path = provisioning::locate_installer(&directory)
        .await
        .map_err(join_error)?
        .ok_or_else(|| AppError::not_found("未在服务器目录中找到安装器 JAR"))?;
    provisioning::run_registered_installer(
        &manager,
        &installer,
        id,
        &server,
        &installer_path,
        progress,
    )
    .await
}

async fn move_server(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>> {
    location_operation(state, id, headers, body, true).await
}

async fn relocate_server(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>> {
    location_operation(state, id, headers, body, false).await
}

async fn location_operation(
    state: WebState,
    id: ServerId,
    headers: HeaderMap,
    bytes: Bytes,
    move_files: bool,
) -> Result<Json<Value>> {
    let body = shared::body::parse_body(&bytes)?;
    let path = body
        .get("path")
        .or_else(|| body.get("working_dir"))
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| AppError::invalid("bad_request", "缺少目标目录 path"))?
        .to_owned();
    let manager = state.manager.clone();
    let action = if move_files { "move" } else { "relocate" };
    submit(&state, id, action, &headers, body, async move {
        let change = if move_files {
            manager
                .move_server(id, PathBuf::from(path).as_path())
                .await?
        } else {
            manager
                .relocate_server(id, PathBuf::from(path).as_path())
                .await?
        };
        Ok(json!({"ok": true, "server": change.server, "warning": change.warning}))
    })
    .await
}

async fn remove_server(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>> {
    let delete_files = query
        .get("delete_files")
        .is_some_and(|value| matches!(value.as_str(), "1" | "true"));
    let request = json!({"delete_files":delete_files,"confirm":query.get("confirm")});
    let manager = state.manager.clone();
    submit(&state, id, "remove", &headers, request, async move {
        if delete_files {
            let server = manager.server_config(id)?;
            let expected = server.working_dir.to_string_lossy();
            if query.get("confirm").map(String::as_str) != Some(expected.as_ref()) {
                return Err(AppError::invalid(
                    "confirmation_required",
                    "删除磁盘文件时 confirm 必须与完整工作目录完全一致",
                ));
            }
        }
        shared::lifecycle::remove_registered(&manager, id, delete_files).await?;
        Ok(json!({"ok": true, "deleted_files": delete_files}))
    })
    .await
}

async fn update_config(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    body: Bytes,
) -> Result<Json<Value>> {
    let body = shared::body::parse_body(&body)?;
    Ok(Json(shared::config::update_server_config(
        &state.manager,
        id,
        &body,
    )?))
}

async fn send_command(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    body: Bytes,
) -> Result<Json<Value>> {
    let body = shared::body::parse_body(&body)?;
    Ok(Json(
        shared::console::send_command(&state.manager, id, &body).await?,
    ))
}

async fn console_snapshot(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>> {
    Ok(Json(shared::console::snapshot(&state.manager, id, &query)?))
}

async fn console_stream(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
) -> Response {
    if let Err(error) = state.manager.server_config(id) {
        return error.into_response();
    }
    shared::console::stream_for(state, id)
}

async fn get_properties(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
) -> Result<Json<Value>> {
    Ok(Json(shared::properties::get(&state.manager, id)?))
}

async fn set_properties(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    body: Bytes,
) -> Result<Json<Value>> {
    let body = shared::body::parse_body(&body)?;
    Ok(Json(shared::properties::set(&state.manager, id, &body)?))
}

async fn list_backups(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
) -> Result<Json<Value>> {
    Ok(Json(shared::backups::list(&state.manager, id).await?))
}

async fn create_backup(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
) -> Result<Json<Value>> {
    Ok(Json(shared::backups::create(&state.manager, id).await?))
}

async fn restore_backup(
    State(state): State<WebState>,
    AxumPath((id, name)): AxumPath<(ServerId, String)>,
) -> Result<Json<Value>> {
    // 运行状态由核心生命周期锁检查；本接口不创建 operation，错误直接作为 HTTP 响应返回。
    Ok(Json(
        shared::backups::restore(&state.manager, id, &name).await?,
    ))
}

async fn delete_backup(
    State(state): State<WebState>,
    AxumPath((id, name)): AxumPath<(ServerId, String)>,
) -> Result<Json<Value>> {
    Ok(Json(
        shared::backups::delete(&state.manager, id, &name).await?,
    ))
}

async fn list_files(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>> {
    let path = query.get("path").cloned().unwrap_or_default();
    Ok(Json(shared::files::list(&state.manager, id, path)?))
}

async fn read_file(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>> {
    let path = shared::body::required_path(&query)?;
    Ok(Json(shared::files::read(&state.manager, id, path)?))
}

async fn write_file(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    body: Bytes,
) -> Result<Json<Value>> {
    let body = shared::body::parse_body(&body)?;
    let path = shared::body::required_body_string(&body, "path", "缺少文件路径")?;
    let content = shared::body::required_body_string(&body, "content", "缺少文件内容")?;
    Ok(Json(shared::files::write(
        &state.manager,
        id,
        path,
        content,
    )?))
}

async fn create_file(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    body: Bytes,
) -> Result<Json<Value>> {
    let body = shared::body::parse_body(&body)?;
    let path = shared::body::required_body_string(&body, "path", "缺少文件路径")?;
    let is_dir = body.get("is_dir").and_then(Value::as_bool).unwrap_or(false);
    Ok(Json(shared::files::create(
        &state.manager,
        id,
        path,
        is_dir,
    )?))
}

async fn rename_file(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    body: Bytes,
) -> Result<Json<Value>> {
    let body = shared::body::parse_body(&body)?;
    let path = shared::body::required_body_string(&body, "path", "缺少文件路径")?;
    let new_name = shared::body::required_body_string(&body, "new_name", "缺少新名称")?;
    Ok(Json(shared::files::rename(
        &state.manager,
        id,
        path,
        new_name,
    )?))
}

async fn remove_file(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>> {
    let path = shared::body::required_path(&query)?;
    Ok(Json(shared::files::remove(&state.manager, id, path)?))
}

async fn upload_file(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<ServerId>,
    Query(query): Query<HashMap<String, String>>,
    request: Request,
) -> Result<Json<Value>> {
    let path = shared::body::required_path(&query)?.to_owned();
    Ok(Json(
        shared::upload::file(
            &state.manager,
            id,
            path,
            query.contains_key("overwrite"),
            request,
        )
        .await?,
    ))
}
