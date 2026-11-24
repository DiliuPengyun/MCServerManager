use tempfile::TempDir;

use super::super::{MAX_INSTALLER_LINE_BYTES, installer::InstallerLines};
use super::*;
use crate::{config::CoreType, error::ErrorKind};

#[tokio::test]
async fn forge_and_neoforge_use_argument_arrays_current_dir_and_detection() {
    let server = MockServer::start(|path, _| {
        if path.starts_with("/forge-maven/") || path.starts_with("/neo-maven/") {
            MockResponse::binary(b"fake installer bytes".to_vec())
        } else {
            not_found()
        }
    })
    .await;
    let service = server.service();
    let temp = TempDir::new().unwrap();

    let forge_dir = temp.path().join("Forge 服务器 with spaces");
    let forge_java = temp.path().join("模拟 Java Forge.sh");
    write_fake_forge_java(&forge_java, "minecraftforge", "forge", "1.20.1-47.2.0");
    let (progress, lines) = progress_log();
    let detected = service
        .install_forge(
            "1.20.1",
            "1.20.1-47.2.0",
            &forge_dir,
            forge_java.to_str().unwrap(),
            Some(progress),
        )
        .await
        .unwrap();
    assert_eq!(detected.core_type, CoreType::Forge);
    assert_eq!(detected.minecraft_version.as_deref(), Some("1.20.1"));
    assert_eq!(detected.core_version.as_deref(), Some("1.20.1-47.2.0"));
    assert!(detected.launch_args_file.is_some());
    assert_eq!(
        fs::read_to_string(forge_dir.join("java-cwd.txt"))
            .unwrap()
            .trim(),
        forge_dir.canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(
        fs::read_to_string(forge_dir.join("java-args.txt"))
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        [
            "-Djava.awt.headless=true",
            "-jar",
            forge_dir
                .join("forge-1.20.1-47.2.0-installer.jar")
                .to_str()
                .unwrap(),
            "--installServer",
        ]
    );
    assert!(forge_dir.join("user_jvm_args.txt").is_file());
    {
        let lines = lines.lock().unwrap();
        assert!(lines.iter().any(|line| line.contains("标准输出 你好")));
        assert!(lines.iter().any(|line| line.contains("错误输出")));
    }
    assert!(server.requests().iter().any(|path| {
        path == "/forge-maven/releases/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-installer.jar"
    }));

    let neo_dir = temp.path().join("NeoForge 服务器");
    let neo_java = temp.path().join("模拟 Java NeoForge.sh");
    write_fake_forge_java(&neo_java, "neoforged", "neoforge", "21.1.1");
    let detected = service
        .install_neoforge(
            "1.21.1",
            "21.1.1",
            &neo_dir,
            neo_java.to_str().unwrap(),
            None,
        )
        .await
        .unwrap();
    assert_eq!(detected.core_type, CoreType::Neoforge);
    assert_eq!(detected.minecraft_version.as_deref(), Some("1.21.1"));
    assert!(server.requests().iter().any(|path| {
        path == "/neo-maven/net/neoforged/neoforge/21.1.1/neoforge-21.1.1-installer.jar"
    }));
}

#[tokio::test]
async fn installer_exit_zero_without_launch_artifacts_is_rejected() {
    let server = MockServer::start(|path, _| {
        if path.starts_with("/forge-maven/") {
            MockResponse::binary(b"installer without installation".to_vec())
        } else {
            not_found()
        }
    })
    .await;
    let service = server.service();
    let temp = TempDir::new().unwrap();
    let java = temp.path().join("模拟空安装器 Java.sh");
    write_executable(&java, "#!/bin/sh\nexit 0\n");
    let target = temp.path().join("只有安装器");
    let error = service
        .install_forge(
            "1.20.1",
            "1.20.1-47.2.0",
            &target,
            java.to_str().unwrap(),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Unavailable);
    assert_eq!(error.code, "installer_incomplete");
    assert!(error.message.contains("启动 JAR 或启动参数文件"));
    assert!(!target.join("user_jvm_args.txt").exists());

    let provided_target = temp.path().join("手动补跑只有安装器");
    fs::create_dir(&provided_target).unwrap();
    let error = service
        .run_installer_jar(
            &target.join("forge-1.20.1-47.2.0-installer.jar"),
            &provided_target,
            java.to_str().unwrap(),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Unavailable);
    assert_eq!(error.code, "installer_incomplete");
    assert!(!provided_target.join("user_jvm_args.txt").exists());
}

#[tokio::test]
async fn fabric_installer_receives_exact_arguments_then_downloads_vanilla() {
    let server = MockServer::start(|path, root| match path {
        "/fabric-maven/net/fabricmc/fabric-installer/1.0.1/fabric-installer-1.0.1.jar" => {
            MockResponse::binary(b"fabric installer".to_vec())
        }
        "/mojang/version_manifest_v2.json" => MockResponse::json(format!(
            r#"{{"versions":[{{"id":"1.21.1","type":"release","url":"{root}fabric-vanilla-meta"}}]}}"#
        )),
        "/fabric-vanilla-meta" => MockResponse::json(format!(
            r#"{{"downloads":{{"server":{{"url":"{root}fabric-server.jar"}}}}}}"#
        )),
        "/fabric-server.jar" => MockResponse::binary(b"vanilla for fabric".to_vec()),
        _ => not_found(),
    })
    .await;
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("Fabric 服务器 with spaces");
    let java = temp.path().join("模拟 Fabric Java.sh");
    write_executable(
        &java,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$PWD/fabric-args.txt\"\nprintf '%s\\n' \"$PWD\" > \"$PWD/fabric-cwd.txt\"\n: > \"$PWD/fabric-server-launch.jar\"\nprintf 'Fabric 行输出\\n'\n",
    );
    let (progress, lines) = progress_log();
    let detected = server
        .service()
        .install_fabric(
            "1.21.1",
            "1.0.1",
            &target,
            java.to_str().unwrap(),
            Some(progress),
        )
        .await
        .unwrap();
    assert_eq!(detected.core_type, CoreType::Fabric);
    assert_eq!(detected.minecraft_version.as_deref(), Some("1.21.1"));
    assert_eq!(
        fs::read(target.join("server.jar")).unwrap(),
        b"vanilla for fabric"
    );
    assert_eq!(
        fs::read_to_string(target.join("fabric-args.txt"))
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        [
            "-jar",
            target.join("fabric-installer-1.0.1.jar").to_str().unwrap(),
            "server",
            "-mcversion",
            "1.21.1",
            "-dir",
            target.to_str().unwrap(),
        ]
    );
    assert_eq!(
        fs::read_to_string(target.join("fabric-cwd.txt"))
            .unwrap()
            .trim(),
        target.canonicalize().unwrap().to_str().unwrap()
    );
    assert!(
        lines
            .lock()
            .unwrap()
            .iter()
            .any(|line| line.contains("Fabric 行输出"))
    );
}

#[tokio::test]
async fn provided_installer_runs_in_existing_nonempty_modpack_without_overwrite() {
    let server = MockServer::start(|_, _| not_found()).await;
    let temp = TempDir::new().unwrap();
    let source_dir = temp.path().join("下载 目录");
    let target = temp.path().join("已有 整合包");
    fs::create_dir(&source_dir).unwrap();
    fs::create_dir(&target).unwrap();
    let source = source_dir.join("安装器 with spaces.jar");
    fs::write(&source, b"installer source").unwrap();
    fs::write(target.join("modpack.txt"), "原有整合包文件").unwrap();
    let java = temp.path().join("已有 Java.sh");
    write_fake_forge_java(&java, "minecraftforge", "forge", "1.20.1-47.2.0");
    let detected = server
        .service()
        .run_installer_jar(&source, &target, java.to_str().unwrap(), None)
        .await
        .unwrap();
    assert_eq!(detected.core_type, CoreType::Forge);
    assert_eq!(
        fs::read_to_string(target.join("modpack.txt")).unwrap(),
        "原有整合包文件"
    );
    assert_eq!(
        fs::read(target.join("安装器 with spaces.jar")).unwrap(),
        b"installer source"
    );
    assert!(target.join("user_jvm_args.txt").is_file());
}

#[tokio::test]
async fn failed_installer_reports_bounded_tail_and_spawn_errors_are_clear() {
    let server = MockServer::start(|_, _| not_found()).await;
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("失败 安装");
    fs::create_dir(&target).unwrap();
    let installer = target.join("forge-installer.jar");
    fs::write(&installer, b"installer").unwrap();
    let java = temp.path().join("失败 Java.sh");
    write_executable(
        &java,
        "#!/bin/sh\nfor line in 1 2 3 4 5 6 7; do printf '失败行 %s\\n' \"$line\"; done\nprintf '最后的错误\\n' >&2\nexit 9\n",
    );
    let error = server
        .service()
        .run_installer_jar(&installer, &target, java.to_str().unwrap(), None)
        .await
        .unwrap_err();
    assert_eq!(error.code, "installer_failed");
    assert!(error.message.contains("退出状态: 9"));
    assert!(!error.message.contains("失败行 1"));
    assert!(error.message.contains("失败行 7"));
    assert!(error.message.contains("最后的错误"));
    assert!(!target.join("user_jvm_args.txt").exists());

    let missing_target = temp.path().join("缺少 Java");
    fs::create_dir(&missing_target).unwrap();
    let missing_installer = missing_target.join("forge-installer.jar");
    fs::write(&missing_installer, b"installer").unwrap();
    let error = server
        .service()
        .run_installer_jar(
            &missing_installer,
            &missing_target,
            &temp.path().join("不存在的 Java").to_string_lossy(),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "installer_spawn_failed");
}

#[test]
fn installer_lines_bound_overlong_output_and_keep_unicode_lines() {
    let mut splitter = InstallerLines::default();
    let mut lines = Vec::new();
    splitter.push("第一行\r\n未完成".as_bytes(), &mut |line| {
        lines.push(line)
    });
    splitter.push("完成\n".as_bytes(), &mut |line| lines.push(line));
    let long = vec![b'x'; MAX_INSTALLER_LINE_BYTES * 2 + 20];
    splitter.push(&long, &mut |line| lines.push(line));
    splitter.push("仍应丢弃\n短行\n末尾".as_bytes(), &mut |line| {
        lines.push(line)
    });
    splitter.finish(&mut |line| lines.push(line));
    assert_eq!(lines[0], "第一行");
    assert_eq!(lines[1], "未完成完成");
    assert!(lines[2].contains("输出过长已截断"));
    assert_eq!(lines[3], "短行");
    assert_eq!(lines[4], "末尾");
    assert_eq!(lines.len(), 5);
    assert!(lines.iter().all(|line| {
        line.len() <= MAX_INSTALLER_LINE_BYTES * 3 + " …[输出过长已截断]".len()
    }));
}
