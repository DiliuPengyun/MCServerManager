use std::{fs, path::PathBuf, sync::Arc};

use axum::{Json, body::Bytes, extract::State};
use serde_json::{Value, json};

use super::super::{
    WebState,
    shared::{
        body::parse_body,
        errors::{join_error, validate_server_name},
    },
    tasks::{TaskRef, WebTasks},
};
use super::registration::{configured_server, ensure_name_available, target_for_name};
use crate::{
    config::CoreType,
    core::{
        ServerManager,
        provisioning::{self, AutoInstallRequest},
    },
    error::{AppError, Result},
    server::{
        detect,
        install::{InstallService, ProgressCallback},
    },
};

pub(super) async fn add_server(State(state): State<WebState>, body: Bytes) -> Result<Json<Value>> {
    let body = parse_body(&body)?;
    let method = body.get("method").and_then(Value::as_str).unwrap_or("");
    let name = body.get("name").and_then(Value::as_str).unwrap_or("");
    if !matches!(method, "auto" | "import" | "installer" | "jar") {
        return Err(AppError::invalid("bad_request", "未知方法 method"));
    }
    validate_server_name(name)?;
    if method == "auto"
        && body
            .get("mc_version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .is_empty()
    {
        return Err(AppError::invalid("bad_request", "缺少 mc_version"));
    }
    if method != "auto"
        && body
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .is_empty()
    {
        return Err(AppError::invalid("bad_request", "缺少 path"));
    }

    let task = state.tasks.create();
    let id = task.lock().id;
    // 导入已有目录或 JAR 不经过安装，日志里的动作名与方法保持一致。
    let action = if matches!(method, "import" | "jar") {
        "导入"
    } else {
        "安装"
    };
    WebTasks::append(&task, format!("开始{action}: {name}"));
    let manager = state.manager.clone();
    let installer = state.installer.clone();
    let background = task.clone();
    let progress_task = task.clone();
    if let Err(error) = state
        .jobs
        .spawn(async move {
            let result = run_add_server(manager, installer, body, progress_task).await;
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

async fn run_add_server(
    manager: Arc<ServerManager>,
    installer: Arc<InstallService>,
    body: Value,
    task: TaskRef,
) -> Result<()> {
    let method = body.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "import" => import_server(manager, &body),
        "jar" => import_jar(manager, &body).await,
        "auto" => install_new_server(manager, installer, &body, task, false).await,
        "installer" => install_new_server(manager, installer, &body, task, true).await,
        _ => Err(AppError::invalid("bad_request", "未知的添加方式")),
    }
}

async fn install_new_server(
    manager: Arc<ServerManager>,
    installer: Arc<InstallService>,
    body: &Value,
    task: TaskRef,
    from_installer: bool,
) -> Result<()> {
    let target = target_for_name(&manager, body)?;
    let java = body
        .get("java_path")
        .and_then(Value::as_str)
        .filter(|java| !java.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            let global = manager.config().global_java_path.clone();
            if global.is_empty() {
                "java".into()
            } else {
                global
            }
        });
    let progress = task_progress(&task);
    let version = body.get("mc_version").and_then(Value::as_str).unwrap_or("");
    let detected = if from_installer {
        let source = PathBuf::from(body.get("path").and_then(Value::as_str).unwrap_or(""));
        installer
            .run_installer_jar(&source, &target, &java, Some(progress))
            .await?
    } else {
        let core = CoreType::from_id(
            body.get("core_type")
                .and_then(Value::as_str)
                .unwrap_or("custom"),
        );
        let core_version = match core {
            CoreType::Vanilla => String::new(),
            CoreType::Paper => "latest".to_owned(),
            CoreType::Neoforge => {
                let versions = installer.fetch_neoforge_versions(version).await?;
                versions
                    .last()
                    .ok_or_else(|| {
                        AppError::not_found(format!(
                            "没有适用于 Minecraft {version} 的 NeoForge 版本"
                        ))
                    })?
                    .core_version
                    .clone()
            }
            CoreType::Forge => version.to_owned(),
            CoreType::Fabric => "1.0.1".to_owned(),
            _ => {
                return Err(AppError::invalid(
                    "unsupported_core",
                    "该核心类型不支持自动安装",
                ));
            }
        };
        provisioning::install_auto(
            &installer,
            AutoInstallRequest {
                core_type: core,
                minecraft_version: version,
                core_version: &core_version,
                target_dir: &target,
                java_path: &java,
            },
            progress,
        )
        .await?
    };
    ensure_name_available(
        &manager,
        body.get("name").and_then(Value::as_str).unwrap_or(""),
    )?;
    let server = configured_server(body, target, detected)?;
    let name = server.name.clone();
    manager.add_server(server)?;
    WebTasks::append(&task, format!("服务器已添加: {name}"));
    Ok(())
}

fn task_progress(task: &TaskRef) -> ProgressCallback {
    let task = task.clone();
    Arc::new(move |line| WebTasks::append(&task, line))
}

fn import_server(manager: Arc<ServerManager>, body: &Value) -> Result<()> {
    let source = PathBuf::from(body.get("path").and_then(Value::as_str).unwrap_or(""));
    let detected = detect::detect_directory(&source).ok_or_else(|| {
        AppError::invalid(
            "core_not_found",
            format!("无法在该目录中识别服务端核心: {}", source.display()),
        )
    })?;
    let working_dir = source
        .canonicalize()
        .map_err(|error| AppError::io("无法解析导入目录", error))?;
    let server = configured_server(body, working_dir, detected)?;
    manager.add_server(server)?;
    Ok(())
}

async fn import_jar(manager: Arc<ServerManager>, body: &Value) -> Result<()> {
    let source = PathBuf::from(body.get("path").and_then(Value::as_str).unwrap_or(""));
    if !source.is_file() {
        return Err(AppError::not_found("源 JAR 文件不存在或不是普通文件"));
    }
    let target = target_for_name(&manager, body)?;
    let target_for_task = target.clone();
    let source_for_task = source.clone();
    tokio::task::spawn_blocking(move || {
        fs::create_dir(&target_for_task)
            .map_err(|error| AppError::io("无法创建服务器目录", error))?;
        let name = source_for_task
            .file_name()
            .ok_or_else(|| AppError::invalid("invalid_path", "源 JAR 名称无效"))?;
        fs::copy(&source_for_task, target_for_task.join(name))
            .map_err(|error| AppError::io("复制 JAR 失败", error))?;
        Ok::<_, AppError>(())
    })
    .await
    .map_err(join_error)??;
    let detected = detect::detect_directory(&target)
        .ok_or_else(|| AppError::invalid("core_not_found", "无法识别复制后的 JAR"))?;
    let server = configured_server(body, target, detected)?;
    manager.add_server(server)?;
    Ok(())
}
