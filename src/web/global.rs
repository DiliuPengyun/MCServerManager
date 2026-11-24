use std::collections::HashMap;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Query, State},
    routing::get,
};
use serde_json::{Value, json};

use crate::{
    config::CoreType,
    error::{AppError, ErrorKind, Result},
};

use super::{WebState, shared, shared::body::parse_body};

pub fn routes() -> Router<WebState> {
    Router::new()
        .route("/api/coretypes", get(core_types))
        .route("/api/versions", get(versions))
        .route("/api/settings", get(get_settings).put(update_settings))
        .route("/api/browse", get(browse_host))
}

async fn core_types() -> Json<Value> {
    Json(Value::Array(
        CoreType::ALL
            .into_iter()
            .map(|kind| {
                json!({
                    "id": kind.id(),
                    "name": kind.display_name(),
                    "uses_installer": kind.uses_installer(),
                })
            })
            .collect(),
    ))
}

async fn versions(
    State(state): State<WebState>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>> {
    let core = CoreType::from_id(query.get("core").map(String::as_str).unwrap_or(""));
    let upstream = |error: AppError| AppError::new(ErrorKind::Upstream, error.code, error.message);
    let mut output = Vec::new();
    match core {
        CoreType::Vanilla => {
            for version in state
                .installer
                .fetch_vanilla_versions()
                .await
                .map_err(upstream)?
            {
                if version.release_type == "release" {
                    output.push(
                        json!({"id": version.id, "label": format!("{} (release)", version.id)}),
                    );
                }
            }
        }
        CoreType::Paper => {
            for version in state
                .installer
                .fetch_paper_versions()
                .await
                .map_err(upstream)?
            {
                output.push(json!({"id": version.id, "label": version.id}));
            }
        }
        CoreType::Neoforge => {
            for version in ["1.21.1", "1.21", "1.20.6", "1.20.4", "1.20.1"] {
                output.push(json!({"id": version, "label": version}));
            }
        }
        CoreType::Forge => {
            for version in state
                .installer
                .fetch_forge_versions()
                .await
                .map_err(upstream)?
            {
                output.push(json!({"id": version.id, "label": version.id}));
            }
        }
        CoreType::Fabric => {
            for version in state
                .installer
                .fetch_fabric_versions()
                .await
                .map_err(upstream)?
            {
                if version.release_type == "release" {
                    output.push(json!({"id": version.id, "label": version.id}));
                }
            }
        }
        CoreType::Spigot | CoreType::Custom => {}
    }
    Ok(Json(json!({"versions": output})))
}

async fn get_settings(State(state): State<WebState>) -> Json<Value> {
    Json(shared::config::settings_json(&state.manager))
}

async fn update_settings(State(state): State<WebState>, body: Bytes) -> Result<Json<Value>> {
    let body = parse_body(&body)?;
    Ok(Json(shared::config::update_settings(
        &state.manager,
        &body,
    )?))
}

async fn browse_host(Query(query): Query<HashMap<String, String>>) -> Result<Json<Value>> {
    Ok(Json(shared::config::browse_host(&query)?))
}
