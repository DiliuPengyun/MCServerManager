use super::super::version::neoforge_prefix;
use super::*;
use crate::error::ErrorKind;

#[tokio::test]
async fn parses_vanilla_paper_and_fabric_json_and_download_urls() {
    let server = MockServer::start(|path, root| match path {
        "/mojang/version_manifest_v2.json" => MockResponse::json(format!(
            r#"{{"versions":[{{"id":"1.21.1","type":"release","url":"{root}meta/release.json"}},{{"id":"snapshot 测试","type":"snapshot","url":"{root}meta/snapshot.json"}},{{"id":"old_alpha","type":"old_alpha","url":""}}]}}"#
        )),
        "/meta/release.json" => MockResponse::json(format!(
            r#"{{"downloads":{{"server":{{"url":"{root}downloads/server%20%E4%B8%AD.jar"}}}}}}"#
        )),
        "/paper" => MockResponse::json(
            r#"{"versions":{"1.21":["1.21.1","1.21.1-rc1"],"1.20":["1.20.6",""]}}"#,
        ),
        "/paper/versions/1.21.1/builds" => MockResponse::json(
            r#"[{"id":8,"channel":"BETA","downloads":{"server:default":{"name":"beta.jar","url":"$ROOTbeta.jar"}}},{"id":7,"channel":"STABLE","downloads":{"server:default":{"name":"paper 中文.jar","url":"$ROOTdownloads/paper%20%E4%B8%AD%E6%96%87.jar"}}}]"#
                .replace("$ROOT", root),
        ),
        "/fabric-game" => MockResponse::json(
            r#"[{"version":"1.21.1","stable":true},{"version":"24w01a","stable":false},{"version":"","stable":true}]"#,
        ),
        _ => not_found(),
    })
    .await;
    let service = server.service();

    let vanilla = service.fetch_vanilla_versions().await.unwrap();
    assert_eq!(vanilla.len(), 2);
    assert_eq!(vanilla[0].id, "1.21.1");
    assert_eq!(vanilla[1].release_type, "snapshot");
    let server_url = service.get_vanilla_server_url("1.21.1").await.unwrap();
    assert!(server_url.ends_with("server%20%E4%B8%AD.jar"));

    let paper = service.fetch_paper_versions().await.unwrap();
    assert_eq!(
        paper
            .iter()
            .map(|version| version.id.as_str())
            .collect::<Vec<_>>(),
        ["1.21.1", "1.20.6"]
    );
    let paper_url = service
        .get_paper_download_url("1.21.1", "latest")
        .await
        .unwrap();
    assert!(
        paper_url.ends_with("/downloads/paper%20%E4%B8%AD%E6%96%87.jar"),
        "实际 Paper URL: {paper_url}"
    );

    let fabric = service.fetch_fabric_versions().await.unwrap();
    assert_eq!(fabric.len(), 2);
    assert_eq!(fabric[0].release_type, "release");
    assert_eq!(fabric[1].release_type, "snapshot");
}

#[tokio::test]
async fn parses_neoforge_and_forge_versions_with_cpp_ordering() {
    let server = MockServer::start(|path, _| match path {
        "/neo-api" => MockResponse::json(
            r#"{"versions":["20.4.10","21.0.9","21.1.1","21.1.2-beta","21.2.0"]}"#,
        ),
        "/forge-metadata.xml" => MockResponse::new(
            b"<metadata><version>1.20.1-47.1.0</version><version>1.19.2-43.3.0</version><version>bad</version><version>1.20.1-47.2.0</version><version>1.20.2-48.0.1</version><version>1.20.10-49.0.0</version></metadata>".to_vec(),
        )
        .content_type("application/xml"),
        _ => not_found(),
    })
    .await;
    let service = server.service();
    let neo = service.fetch_neoforge_versions("1.21.1").await.unwrap();
    assert_eq!(
        neo.iter()
            .map(|version| version.core_version.as_str())
            .collect::<Vec<_>>(),
        ["21.1.1", "21.1.2-beta"]
    );
    assert!(
        neo[0]
            .download_url
            .ends_with("/neo-maven/net/neoforged/neoforge/21.1.1/neoforge-21.1.1-installer.jar")
    );
    assert_eq!(neoforge_prefix("1.21"), "21.0.");
    assert_eq!(neoforge_prefix("1.21.1"), "21.1.");
    assert_eq!(neoforge_prefix("1.21.1.1"), "21.1.");

    let forge = service.fetch_forge_versions().await.unwrap();
    assert_eq!(
        forge
            .iter()
            .map(|version| version.url.as_str())
            .collect::<Vec<_>>(),
        ["1.20.10", "1.20.2", "1.20.1", "1.19.2"]
    );
    assert_eq!(forge[2].id, "1.20.1-47.2.0");
}

#[tokio::test]
async fn malformed_json_and_http_errors_are_clear_app_errors() {
    let malformed = MockServer::start(|_, _| MockResponse::json("{不是 JSON")).await;
    let error = malformed
        .service()
        .fetch_vanilla_versions()
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Unavailable);
    assert_eq!(error.code, "invalid_response");
    assert!(error.message.contains("响应不是预期 JSON"));

    let unavailable = MockServer::start(|_, _| not_found().status(503)).await;
    let error = unavailable
        .service()
        .fetch_paper_versions()
        .await
        .unwrap_err();
    assert_eq!(error.code, "http_status");
    assert!(error.message.contains("503"));
}
