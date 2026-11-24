use tempfile::TempDir;

use super::*;
use crate::{config::CoreType, error::ErrorKind};

#[tokio::test]
async fn automatic_install_rejects_nonempty_target_before_network() {
    let server = MockServer::start(|_, _| not_found()).await;
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("已有服务器");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("保留.txt"), "不能覆盖").unwrap();
    let error = server
        .service()
        .install_vanilla("1.21.1", &target, "java", None)
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::AlreadyExists);
    assert!(server.requests().is_empty());
    assert_eq!(
        fs::read_to_string(target.join("保留.txt")).unwrap(),
        "不能覆盖"
    );
}

#[tokio::test]
async fn vanilla_and_paper_install_to_unicode_paths() {
    let server = MockServer::start(|path, root| match path {
        "/mojang/version_manifest_v2.json" => MockResponse::json(format!(
            r#"{{"versions":[{{"id":"1.21.1","type":"release","url":"{root}vanilla-meta"}}]}}"#
        )),
        "/vanilla-meta" => MockResponse::json(format!(
            r#"{{"downloads":{{"server":{{"url":"{root}vanilla.jar"}}}}}}"#
        )),
        "/vanilla.jar" => MockResponse::binary(b"vanilla jar".to_vec()),
        "/paper/versions/1.21.1/builds" => MockResponse::json(
            r#"[{"id":7,"channel":"STABLE","downloads":{"server:default":{"name":"paper-custom-name.jar","url":"$ROOTpaper-official.jar"}}}]"#
                .replace("$ROOT", root),
        ),
        "/paper-official.jar" => MockResponse::binary(b"paper jar".to_vec()),
        _ => not_found(),
    })
    .await;
    let service = server.service();
    let temp = TempDir::new().unwrap();
    let vanilla_dir = temp.path().join("原版 服务器");
    let vanilla = service
        .install_vanilla("1.21.1", &vanilla_dir, "java", None)
        .await
        .unwrap();
    assert_eq!(vanilla.core_type, CoreType::Vanilla);
    assert_eq!(vanilla.minecraft_version.as_deref(), Some("1.21.1"));
    assert_eq!(
        fs::read(vanilla_dir.join("server.jar")).unwrap(),
        b"vanilla jar"
    );

    let paper_dir = temp.path().join("Paper 服务器");
    let paper = service
        .install_paper("1.21.1", "latest", &paper_dir, "java", None)
        .await
        .unwrap();
    assert_eq!(paper.core_type, CoreType::Paper);
    assert_eq!(paper.core_version.as_deref(), Some("7"));
    assert_eq!(
        paper.server_jar_path,
        Some(paper_dir.join("paper-1.21.1-7.jar"))
    );
}
