use axum::{
    Json, Router,
    extract::{Path as AxumPath, State},
    routing::{delete, get, post},
};
use serde_json::Value;

use super::WebState;
use crate::error::Result;

mod cleanup;
mod directory;
mod install;
mod registration;

pub(super) use cleanup::cleanup_expired_uploads;

pub fn routes() -> Router<WebState> {
    Router::new()
        .route("/api/tasks/{id}", get(get_task))
        .route("/api/servers", post(install::add_server))
        .route("/api/uploads", post(directory::create_upload))
        .route(
            "/api/uploads/{id}/directory",
            post(directory::create_directory),
        )
        .route(
            "/api/uploads/{id}/file",
            post(directory::upload_part).layer(axum::extract::DefaultBodyLimit::disable()),
        )
        .route("/api/uploads/{id}/finish", post(directory::finish_upload))
        .route("/api/uploads/{id}", delete(directory::cancel_upload))
}

async fn get_task(
    State(state): State<WebState>,
    AxumPath(id): AxumPath<u64>,
) -> Result<Json<Value>> {
    let task = state.tasks.find(id)?;
    Ok(Json(task.lock().json()))
}
