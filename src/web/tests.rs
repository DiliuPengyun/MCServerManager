use std::{path::PathBuf, sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{Method, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

use crate::{
    config::{ConfigStore, ServerConfig},
    core::ServerManager,
    server::tmux::Tmux,
};

fn manager(temp: &TempDir) -> Arc<ServerManager> {
    let root = temp.path();
    let store = ConfigStore::with_paths(root.join("config/config.json"), root.join("data"));
    let tmux = Arc::new(
        Tmux::isolated(
            root.join("runtime"),
            format!("mcsm-web-{}", Uuid::new_v4().simple()),
        )
        .unwrap(),
    );
    ServerManager::load_with_tmux(store, tmux).unwrap()
}

fn server(manager: &ServerManager, root: &std::path::Path, name: &str) -> ServerConfig {
    let directory = root.join(name);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("server.jar"), "核心").unwrap();
    let mut server = ServerConfig::new(name, directory);
    server.server_jar_path = "server.jar".into();
    manager.add_server(server).unwrap()
}

async fn request(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(key) = key {
        builder = builder.header("Idempotency-Key", key);
    }
    let body = if let Some(body) = body {
        builder = builder.header("Content-Type", "application/json");
        Body::from(serde_json::to_vec(&body).unwrap())
    } else {
        Body::empty()
    };
    let response = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, body)
}

async fn request_raw(app: &Router, method: Method, uri: &str, body: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn wait_operation(app: &Router, initial: &Value) -> Value {
    let id = initial["operation_id"].as_str().unwrap();
    for _ in 0..300 {
        let (status, operation) = request(
            app,
            Method::GET,
            &format!("/api/operations/{id}"),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        if operation["status"] != "running" {
            return operation;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("操作未及时完成");
}

#[tokio::test]
async fn stable_id_move_is_trackable_and_idempotent() {
    let temp = TempDir::new().unwrap();
    let manager = manager(&temp);
    let app = crate::web::router(manager.clone()).unwrap();
    let server = server(&manager, temp.path(), "移动源");
    let id = server.id;
    let destination = temp.path().join("移动目标");
    let body = json!({"path":destination});
    let uri = format!("/api/servers/{id}/move");

    let (status, first) = request(
        &app,
        Method::POST,
        &uri,
        Some(body.clone()),
        Some("move-request"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, replay) = request(&app, Method::POST, &uri, Some(body), Some("move-request")).await;
    assert_eq!(first["operation_id"], replay["operation_id"]);
    let finished = wait_operation(&app, &first).await;
    assert_eq!(finished["status"], "done", "{finished}");
    assert_eq!(manager.server_config(id).unwrap().working_dir, destination);
    assert_eq!(
        std::fs::read_to_string(destination.join("server.jar")).unwrap(),
        "核心"
    );

    let (status, conflict) = request(
        &app,
        Method::POST,
        &uri,
        Some(json!({"path":temp.path().join("另一个目标")})),
        Some("move-request"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"], "idempotency_conflict");
}

#[tokio::test]
async fn remove_defaults_to_unregister_and_disk_delete_needs_exact_confirmation() {
    let temp = TempDir::new().unwrap();
    let manager = manager(&temp);
    let app = crate::web::router(manager.clone()).unwrap();
    let unregister = server(&manager, temp.path(), "仅移除");
    let directory = unregister.working_dir.clone();
    let uri = format!("/api/servers/{}", unregister.id);
    let (_, initial) = request(&app, Method::DELETE, &uri, None, None).await;
    assert_eq!(wait_operation(&app, &initial).await["status"], "done");
    assert!(directory.join("server.jar").is_file());
    assert!(manager.server_config(unregister.id).is_err());

    let delete = server(&manager, temp.path(), "删除文件");
    let uri = format!("/api/servers/{}?delete_files=true", delete.id);
    let (_, initial) = request(&app, Method::DELETE, &uri, None, None).await;
    let failed = wait_operation(&app, &initial).await;
    assert_eq!(failed["status"], "error");
    assert_eq!(failed["error"]["code"], "confirmation_required");
    assert!(delete.working_dir.join("server.jar").is_file());
    assert!(manager.server_config(delete.id).is_ok());

    let uri = format!(
        "/api/servers/{}?delete_files=true&confirm={}",
        delete.id,
        delete.working_dir.to_string_lossy()
    );
    let (_, initial) = request(&app, Method::DELETE, &uri, None, None).await;
    let done = wait_operation(&app, &initial).await;
    assert_eq!(done["status"], "done", "{done}");
    assert!(!delete.working_dir.exists());
    assert!(manager.server_config(delete.id).is_err());
}

#[tokio::test]
async fn config_update_uses_uuid_and_preserves_validation() {
    let temp = TempDir::new().unwrap();
    let manager = manager(&temp);
    let app = crate::web::router(manager.clone()).unwrap();
    let server = server(&manager, temp.path(), "共享配置");
    let uri = format!("/api/servers/{}/config", server.id);

    let (status, body) = request(
        &app,
        Method::PUT,
        &uri,
        Some(json!({
            "name": "第一次改名",
            "java_path": "/usr/bin/java",
            "min_memory_mb": -8,
            "max_memory_mb": 2048,
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let updated = manager.server_config(server.id).unwrap();
    assert_eq!(updated.name, "第一次改名");
    assert_eq!(updated.java_path, "/usr/bin/java");
    assert_eq!(updated.min_memory_mb, 0);
    assert_eq!(updated.max_memory_mb, 2048);

    let (status, body) = request(
        &app,
        Method::PUT,
        &uri,
        Some(json!({
            "name": "第二次改名",
            "additional_jvm_args": "-Dshared=true",
            "world_dir": "共享世界",
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let updated = manager.server_config(server.id).unwrap();
    assert_eq!(updated.name, "第二次改名");
    assert_eq!(updated.additional_jvm_args, "-Dshared=true");
    assert_eq!(updated.world_dir, PathBuf::from("共享世界"));

    let (status, json) = request(&app, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], server.id.to_string());
    assert_eq!(json["name"], "第二次改名");

    let (status, error) = request(
        &app,
        Method::PUT,
        &uri,
        Some(json!({"name": "../越界"})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"], "invalid_name");
}

#[tokio::test]
async fn start_rejects_malformed_json_and_reports_preflight_in_operation() {
    let temp = TempDir::new().unwrap();
    let manager = manager(&temp);
    let app = crate::web::router(manager.clone()).unwrap();
    let server = server(&manager, temp.path(), "启动契约");
    let uri = format!("/api/servers/{}/start", server.id);

    let (status, error) = request_raw(&app, Method::POST, &uri, "{这不是 JSON").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"], "bad_request");

    let (status, initial) = request(&app, Method::POST, &uri, None, None).await;
    assert_eq!(status, StatusCode::OK);
    let finished = wait_operation(&app, &initial).await;
    assert_eq!(finished["status"], "error");
    assert_eq!(finished["error"]["code"], "eula_required");
}

#[tokio::test]
async fn invalid_server_ids_and_unknown_routes_cannot_modify_servers() {
    let temp = TempDir::new().unwrap();
    let manager = manager(&temp);
    let app = crate::web::router(manager.clone()).unwrap();
    let server = server(&manager, temp.path(), "保留服务器");
    let directory = server.working_dir.clone();

    let (status, _) = request(&app, Method::DELETE, "/api/servers/0", None, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = request(
        &app,
        Method::POST,
        "/api/servers/0/install",
        Some(json!({})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = request(&app, Method::GET, "/api/unknown", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    assert!(directory.join("server.jar").is_file());
    assert!(manager.server_config(server.id).is_ok());
}

#[tokio::test]
async fn global_routes_remain_available() {
    let temp = TempDir::new().unwrap();
    let manager = manager(&temp);
    let app = crate::web::router(manager).unwrap();

    let (status, cores) = request(&app, Method::GET, "/api/coretypes", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(cores.as_array().is_some_and(|cores| !cores.is_empty()));
    let (status, versions) =
        request(&app, Method::GET, "/api/versions?core=neoforge", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        versions["versions"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    );

    let (status, settings) = request(&app, Method::GET, "/api/settings", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(settings["default_server_dir"].is_string());
    let (status, _) = request(
        &app,
        Method::PUT,
        "/api/settings",
        Some(json!({"global_java_path":"/隔离/java"})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let uri = format!("/api/browse?path={}&show_files=1", temp.path().display());
    let (status, browse) = request(&app, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(browse["entries"].is_array());
}
