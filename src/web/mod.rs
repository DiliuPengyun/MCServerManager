use std::{
    future::{Future, IntoFuture},
    net::SocketAddr,
    sync::Arc,
};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde_json::json;
use tokio::{
    sync::{Mutex, Semaphore},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

use crate::{
    AppError, ErrorKind, INDEX_HTML, Result, core::ServerManager, server::install::InstallService,
};

mod api;
pub(crate) mod assets;
mod daemon;
mod global;
mod log_buffer;
mod operations;
mod shared;
mod tasks;
mod uploads;

#[cfg(test)]
mod tests;

pub struct Jobs {
    tasks: Mutex<JoinSet<()>>,
}

impl Jobs {
    fn new() -> Self {
        Self {
            tasks: Mutex::new(JoinSet::new()),
        }
    }

    async fn spawn<F>(&self, future: F) -> Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut tasks = self.tasks.lock().await;
        if tasks.len() >= 32 {
            return Err(AppError::busy("后台任务繁忙，请稍后重试"));
        }
        tasks.spawn(future);
        Ok(())
    }

    async fn shutdown(&self) {
        let mut tasks = self.tasks.lock().await;
        while tasks.join_next().await.is_some() {}
    }
}

#[derive(Clone)]
pub struct WebState {
    manager: Arc<ServerManager>,
    tasks: Arc<tasks::WebTasks>,
    operations: Arc<operations::Operations>,
    jobs: Arc<Jobs>,
    installer: Arc<InstallService>,
    sse: Arc<Semaphore>,
    shutdown: CancellationToken,
}

impl WebState {
    fn new(manager: Arc<ServerManager>) -> Result<Self> {
        Ok(Self {
            manager,
            tasks: Arc::new(tasks::WebTasks::default()),
            operations: Arc::new(operations::Operations::default()),
            jobs: Arc::new(Jobs::new()),
            installer: Arc::new(InstallService::new()?),
            sse: Arc::new(Semaphore::new(3)),
            shutdown: CancellationToken::new(),
        })
    }
}

pub fn run(
    manager: Arc<ServerManager>,
    port_override: Option<u16>,
    foreground: bool,
) -> Result<()> {
    if !foreground {
        return daemon::run(manager, port_override);
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| AppError::internal(format!("无法创建异步运行时: {error}")))?;
    runtime.block_on(run_foreground(manager, port_override))
}

pub async fn run_foreground(manager: Arc<ServerManager>, port_override: Option<u16>) -> Result<()> {
    let port = port_override.unwrap_or_else(|| manager.config().web_port);
    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| AppError::io(format!("无法绑定 Web 端口 {port}"), error))?;
    serve_listener(manager, listener).await
}

pub(super) async fn serve_listener(
    manager: Arc<ServerManager>,
    listener: tokio::net::TcpListener,
) -> Result<()> {
    let state = WebState::new(manager.clone())?;
    manager.attach_all().await;
    let cleanup_state = state.clone();
    state
        .jobs
        .spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5 * 60));
            loop {
                tokio::select! {
                    _ = cleanup_state.shutdown.cancelled() => break,
                    _ = interval.tick() => {}
                }
                let tasks = cleanup_state.tasks.clone();
                let parent = cleanup_state.manager.config().default_server_dir.clone();
                let cleanup = tokio::task::spawn_blocking(move || {
                    uploads::cleanup_expired_uploads(&tasks, &parent);
                })
                .await;
                if let Err(error) = cleanup {
                    tracing::warn!(%error, "上传暂存目录清理任务异常");
                }
            }
        })
        .await?;
    let shutdown = state.shutdown.clone();
    let shutdown_started = Arc::new(tokio::sync::Notify::new());
    let shutdown_notifier = shutdown_started.clone();
    let server = axum::serve(listener, router_with_state(state.clone()))
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            shutdown.cancel();
            shutdown_notifier.notify_one();
        })
        .into_future();
    tokio::pin!(server);
    let result = tokio::select! {
        result = &mut server => result.map_err(|error| AppError::io("Web 服务异常退出", error)),
        _ = shutdown_started.notified() => {
            match tokio::time::timeout(std::time::Duration::from_secs(10), &mut server).await {
                Ok(result) => result.map_err(|error| AppError::io("Web 服务异常退出", error)),
                Err(_) => {
                    tracing::warn!("部分 HTTP 连接未在退出通知后 10 秒内结束，已强制关闭监听服务");
                    Ok(())
                }
            }
        }
    };
    state.shutdown.cancel();
    state.jobs.shutdown().await;
    manager.shutdown().await;
    result
}

pub fn router(manager: Arc<ServerManager>) -> Result<Router> {
    Ok(router_with_state(WebState::new(manager)?))
}

fn router_with_state(state: WebState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/favicon.svg", get(favicon))
        .merge(global::routes())
        .merge(uploads::routes())
        .merge(api::routes())
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(256 * 1024 * 1024))
        .layer(middleware::from_fn(reject_oversized_buffered_body))
        .with_state(state)
}

async fn reject_oversized_buffered_body(request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let streaming = path.ends_with("/files/upload")
        || (path.starts_with("/api/uploads/") && path.ends_with("/file"));
    let too_large = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > 256 * 1024 * 1024);
    let chunked = request
        .headers()
        .get(header::TRANSFER_ENCODING)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("chunked"));
    // 缓冲 JSON 必须有明确长度；否则客户端只发送一个巨大 chunk 头就能让
    // Hyper 在路由尚未执行时长期等待。原始文件上传走独立流式例外，不受此限制。
    if !streaming && (too_large || chunked) {
        let mut response = (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "error": "too_large",
                "message": "请求体超过服务端允许的大小，或未提供明确的 Content-Length",
            })),
        )
            .into_response();
        response
            .headers_mut()
            .insert(header::CONNECTION, HeaderValue::from_static("close"));
        return response;
    }
    next.run(request).await
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn favicon() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8")],
        include_bytes!("../../assets/favicon.svg").as_slice(),
    )
}

async fn not_found() -> AppError {
    AppError::not_found("接口不存在")
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self.kind {
            ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::AlreadyExists | ErrorKind::Conflict => StatusCode::CONFLICT,
            ErrorKind::Busy => StatusCode::TOO_MANY_REQUESTS,
            ErrorKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            ErrorKind::Upstream => StatusCode::BAD_GATEWAY,
            ErrorKind::NoSpace => StatusCode::INSUFFICIENT_STORAGE,
            ErrorKind::Io | ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(json!({"error": self.code, "message": self.message})),
        )
            .into_response()
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut terminate = match signal(SignalKind::terminate()) {
            Ok(signal) => signal,
            Err(error) => {
                tracing::error!(%error, "无法注册 SIGTERM 处理器");
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
