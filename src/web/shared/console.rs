use std::{collections::HashMap, convert::Infallible, time::Duration};

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use serde_json::{Value, json};

use crate::{
    config::ServerId,
    core::ServerManager,
    error::{AppError, Result},
};

use super::super::WebState;

/// 控制台快照：`tail` 默认 300，限制在 1..=5000。
pub(crate) fn snapshot(
    manager: &ServerManager,
    id: ServerId,
    query: &HashMap<String, String>,
) -> Result<Value> {
    let console = manager
        .console(id)
        .ok_or_else(|| AppError::not_found("控制台不存在"))?;
    let tail = query
        .get("tail")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(300)
        .clamp(1, 5000);
    let lines = console.lines();
    Ok(json!({"lines": lines[lines.len().saturating_sub(tail)..]}))
}

/// 校验并发送控制台指令；空指令返回 `bad_request`。
pub(crate) async fn send_command(
    manager: &ServerManager,
    id: ServerId,
    body: &Value,
) -> Result<Value> {
    let command = body.get("command").and_then(Value::as_str).unwrap_or("");
    if command.is_empty() {
        return Err(AppError::invalid("bad_request", "缺少控制台指令"));
    }
    manager.send_command(id, command).await?;
    Ok(json!({"ok": true}))
}

/// 控制台 SSE 推送；调用方负责先完成服务器存在性检查。
///
/// SSE 配额、事件格式和 ping 行为由统一协议约定。
pub(crate) fn stream_for(state: WebState, id: ServerId) -> Response {
    let Some(console) = state.manager.console(id) else {
        return AppError::not_found("控制台不存在").into_response();
    };
    let permit = match state.sse.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            let mut response = (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "error": "too_many_streams",
                    "message": "实时控制台连接数已达上限，请稍后重试",
                })),
            )
                .into_response();
            response
                .headers_mut()
                .insert(header::CONNECTION, HeaderValue::from_static("close"));
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
            return response;
        }
    };
    let mut receiver = console.subscribe();
    let manager = state.manager.clone();
    let shutdown = state.shutdown.clone();
    let stream = async_stream::stream! {
        let _permit = permit;
        let mut ping = tokio::time::interval(Duration::from_secs(3));
        ping.tick().await;
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                event = receiver.recv() => {
                    match event {
                        Ok(event) => {
                            yield Ok::<_, Infallible>(Event::default().data(json!({"line": event.line}).to_string()));
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = ping.tick() => {
                    if manager.server_config(id).is_err() {
                        break;
                    }
                    yield Ok::<_, Infallible>(Event::default().comment("ping"));
                }
            }
        }
    };
    let mut response = Sse::new(stream).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
        .headers_mut()
        .insert("x-accel-buffering", HeaderValue::from_static("no"));
    response
}
