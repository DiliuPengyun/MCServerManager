use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use reqwest::Url;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

use super::{InstallService, ProgressCallback};

mod download;
mod installer;
mod service;
mod version;

struct MockResponse {
    status: u16,
    body: Vec<u8>,
    content_type: &'static str,
    declared_length: Option<usize>,
    chunk_sizes: Vec<usize>,
}

impl MockResponse {
    fn json(body: impl Into<String>) -> Self {
        Self::new(body.into().into_bytes()).content_type("application/json")
    }

    fn binary(body: impl Into<Vec<u8>>) -> Self {
        Self::new(body.into()).content_type("application/octet-stream")
    }

    fn new(body: Vec<u8>) -> Self {
        Self {
            status: 200,
            body,
            content_type: "application/octet-stream",
            declared_length: None,
            chunk_sizes: Vec::new(),
        }
    }

    fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    fn content_type(mut self, content_type: &'static str) -> Self {
        self.content_type = content_type;
        self
    }

    fn declared_length(mut self, length: usize) -> Self {
        self.declared_length = Some(length);
        self
    }

    fn chunk_sizes(mut self, sizes: &[usize]) -> Self {
        self.chunk_sizes = sizes.to_vec();
        self
    }
}

struct MockServer {
    root: String,
    requests: Arc<Mutex<Vec<String>>>,
    task: JoinHandle<()>,
}

impl MockServer {
    async fn start<F>(handler: F) -> Self
    where
        F: Fn(&str, &str) -> MockResponse + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let root = format!("http://{address}/");
        let handler = Arc::new(handler);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let task = {
            let root = root.clone();
            let requests = requests.clone();
            tokio::spawn(async move {
                loop {
                    let Ok((stream, _)) = listener.accept().await else {
                        break;
                    };
                    handle_connection(stream, &root, &requests, handler.as_ref()).await;
                }
            })
        };
        Self {
            root,
            requests,
            task,
        }
    }

    fn service(&self) -> InstallService {
        InstallService::with_mock_root(Url::parse(&self.root).unwrap())
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn handle_connection<F>(
    mut stream: TcpStream,
    root: &str,
    requests: &Mutex<Vec<String>>,
    handler: &F,
) where
    F: Fn(&str, &str) -> MockResponse + Send + Sync + 'static,
{
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    let header_end = loop {
        match stream.read(&mut buffer).await {
            Ok(0) => return,
            Ok(count) => {
                request.extend_from_slice(&buffer[..count]);
                if request.len() > 16 * 1024 {
                    return;
                }
                if let Some(position) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    break position;
                }
            }
            Err(_) => return,
        }
    };
    let _ = header_end;
    let first_line = String::from_utf8_lossy(&request)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    let path = first_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_owned();
    requests.lock().unwrap().push(path.clone());
    let response = handler(&path, root);
    let header = format!(
        "HTTP/1.1 {} Test\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.declared_length.unwrap_or(response.body.len())
    );
    if stream.write_all(header.as_bytes()).await.is_err() {
        return;
    }
    if response.chunk_sizes.is_empty() {
        let _ = stream.write_all(&response.body).await;
        return;
    }
    let mut position = 0;
    for size in response.chunk_sizes {
        let end = (position + size).min(response.body.len());
        if stream
            .write_all(&response.body[position..end])
            .await
            .is_err()
        {
            return;
        }
        let _ = stream.flush().await;
        position = end;
        if position == response.body.len() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    if position < response.body.len() {
        let _ = stream.write_all(&response.body[position..]).await;
    }
}

fn not_found() -> MockResponse {
    MockResponse::json(r#"{"error":"not_found"}"#).status(404)
}

fn progress_log() -> (ProgressCallback, Arc<Mutex<Vec<String>>>) {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let callback = {
        let lines = lines.clone();
        Arc::new(move |line: &str| lines.lock().unwrap().push(line.to_owned())) as ProgressCallback
    };
    (callback, lines)
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn write_fake_forge_java(path: &Path, organization: &str, artifact: &str, version: &str) {
    write_executable(
        path,
        &format!(
            "#!/bin/sh\n\
printf '%s\\n' \"$@\" > \"$PWD/java-args.txt\"\n\
printf '%s\\n' \"$PWD\" > \"$PWD/java-cwd.txt\"\n\
printf '安装器标准输出 你好\\n'\n\
printf '安装器错误输出\\n' >&2\n\
mkdir -p \"$PWD/libraries/net/{organization}/{artifact}/{version}\"\n\
: > \"$PWD/libraries/net/{organization}/{artifact}/{version}/{artifact}-{version}-universal.jar\"\n\
printf '%s\\n' '-p libraries' > \"$PWD/libraries/net/{organization}/{artifact}/{version}/unix_args.txt\"\n"
        ),
    );
}
