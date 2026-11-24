use reqwest::Url;
use tempfile::TempDir;

use super::*;
use crate::error::ErrorKind;

#[tokio::test]
async fn streaming_download_is_atomic_supports_unicode_and_never_overwrites() {
    let payload = "第一块 and the second block 中文内容".as_bytes().to_vec();
    let expected = payload.clone();
    let server = MockServer::start(move |path, _| {
        if path == "/file.jar" {
            MockResponse::binary(payload.clone()).chunk_sizes(&[1, 3, 2, 7])
        } else {
            not_found()
        }
    })
    .await;
    let temp = TempDir::new().unwrap();
    let directory = temp.path().join("目录 with spaces");
    fs::create_dir(&directory).unwrap();
    let destination = directory.join("服务端 核心.jar");
    server
        .service()
        .download_file(
            Url::parse(&format!("{}file.jar", server.root)).unwrap(),
            &destination,
        )
        .await
        .unwrap();
    assert_eq!(fs::read(&destination).unwrap(), expected);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);

    let error = server
        .service()
        .download_file(
            Url::parse(&format!("{}file.jar", server.root)).unwrap(),
            &destination,
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&destination).unwrap(), expected);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
}

#[tokio::test]
async fn interrupted_download_removes_temporary_and_final_files() {
    let server = MockServer::start(|_, _| {
        MockResponse::binary(b"partial response".to_vec())
            .declared_length("partial response".len() + 100)
            .chunk_sizes(&[3, 4])
    })
    .await;
    let temp = TempDir::new().unwrap();
    let destination = temp.path().join("server.jar");
    let error = server
        .service()
        .download_file(
            Url::parse(&format!("{}file.jar", server.root)).unwrap(),
            &destination,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "download_interrupted");
    assert!(!destination.exists());
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}
