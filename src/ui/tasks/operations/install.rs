use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    config::{CoreType, ServerConfig},
    core::{
        ServerManager,
        provisioning::{self, AutoInstallRequest},
    },
    server::{
        detect::{self, DetectedCore},
        files::FileBrowser,
        install::{InstallService, ProgressCallback},
    },
};

use super::super::{JobId, Message, MessageSender, TaskResult, WizardMethod, WizardRequest};
use super::{blocking, error_text};

pub(in crate::ui::tasks) async fn run_wizard(
    manager: Arc<ServerManager>,
    request: WizardRequest,
    job: JobId,
    sender: MessageSender,
) -> TaskResult<ServerConfig> {
    let service = InstallService::new().map_err(error_text)?;
    let generation = request.generation;
    let progress: ProgressCallback = {
        let sender = sender.clone();
        Arc::new(move |line: &str| {
            let _ = sender.send(Message::WizardProgress {
                job,
                generation,
                line: line.to_owned(),
            });
        })
    };
    let config = match request.method {
        WizardMethod::Auto => install_auto(&service, &request, progress).await?,
        WizardMethod::Import => import_directory(&request).await?,
        WizardMethod::Installer => install_with_installer(&service, &request, progress).await?,
        WizardMethod::Custom => custom_jar(&request).await?,
    };
    let added = blocking(move || manager.add_server(config)).await?;
    Ok(added)
}

async fn install_auto(
    service: &InstallService,
    request: &WizardRequest,
    progress: ProgressCallback,
) -> TaskResult<ServerConfig> {
    let minecraft_version = if matches!(request.core_type, CoreType::Forge | CoreType::Neoforge) {
        &request.mc_version
    } else {
        &request.version
    };
    let core_version = match request.core_type {
        CoreType::Paper => "latest",
        CoreType::Fabric => "1.0.1",
        CoreType::Forge | CoreType::Neoforge => &request.version,
        _ => "",
    };
    let detected = provisioning::install_auto(
        service,
        AutoInstallRequest {
            core_type: request.core_type,
            minecraft_version,
            core_version,
            target_dir: &request.target_dir,
            java_path: &request.java_path,
        },
        progress,
    )
    .await
    .map_err(error_text)?;
    Ok(config_from_detection(request, detected))
}

async fn import_directory(request: &WizardRequest) -> TaskResult<ServerConfig> {
    let detected = {
        let path = request.source_path.clone();
        blocking(move || {
            detect::detect_directory(&path).ok_or_else(|| {
                "目录中没有可识别的服务端核心；单个 JAR 请改用「自定义 JAR」方式".to_owned()
            })
        })
        .await?
    };
    let mut config = config_from_detection(request, detected);
    config.working_dir = request.source_path.clone();
    Ok(config)
}

async fn install_with_installer(
    service: &InstallService,
    request: &WizardRequest,
    progress: ProgressCallback,
) -> TaskResult<ServerConfig> {
    let detected = service
        .run_installer_jar(
            &request.source_path,
            &request.target_dir,
            &request.java_path,
            Some(progress),
        )
        .await
        .map_err(error_text)?;
    Ok(config_from_detection(request, detected))
}

async fn custom_jar(request: &WizardRequest) -> TaskResult<ServerConfig> {
    let jar = copy_custom_jar(&request.source_path, &request.target_dir).await?;
    let detected = {
        let directory = request.target_dir.clone();
        blocking(move || Ok::<_, String>(detect::detect_directory(&directory))).await?
    };
    let mut config = if let Some(detected) = detected {
        config_from_detection(request, detected)
    } else {
        base_config(request)
    };
    if config.server_jar_path.as_os_str().is_empty() {
        config.server_jar_path = jar;
    }
    Ok(config)
}

fn base_config(request: &WizardRequest) -> ServerConfig {
    let mut config = ServerConfig::new(&request.name, request.target_dir.clone());
    config.java_path = request.java_path.clone();
    config.min_memory_mb = request.min_memory_mb;
    config.max_memory_mb = request.max_memory_mb;
    config.additional_jvm_args = request.jvm_args.clone();
    config.core_type = if request.method == WizardMethod::Custom {
        CoreType::Custom
    } else {
        request.core_type
    };
    config
}

fn config_from_detection(request: &WizardRequest, detected: DetectedCore) -> ServerConfig {
    let mut config = base_config(request);
    provisioning::apply_detection_to_config(&mut config, detected);
    config
}

async fn copy_custom_jar(source: &Path, target: &Path) -> TaskResult<PathBuf> {
    let source = source.to_path_buf();
    let target = target.to_path_buf();
    blocking(move || {
        let canonical_source = source
            .canonicalize()
            .map_err(|error| format!("无法读取自定义 JAR {}: {error}", source.display()))?;
        if !canonical_source.is_file() {
            return Err("自定义 JAR 路径不是普通文件".to_owned());
        }
        let name = canonical_source
            .file_name()
            .ok_or_else(|| "自定义 JAR 缺少文件名".to_owned())?
            .to_os_string();
        fs::create_dir_all(&target)
            .map_err(|error| format!("无法创建目标目录 {}: {error}", target.display()))?;
        let canonical_target = target
            .canonicalize()
            .map_err(|error| format!("无法解析目标目录 {}: {error}", target.display()))?;
        let destination = canonical_target.join(&name);
        if canonical_source.parent() != Some(canonical_target.as_path()) {
            let browser = FileBrowser::new(&canonical_target);
            let relative = PathBuf::from(&name);
            let mut upload = browser.begin_upload(relative, false).map_err(error_text)?;
            let mut input = fs::File::open(&canonical_source)
                .map_err(|error| format!("无法打开自定义 JAR: {error}"))?;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let count = input
                    .read(&mut buffer)
                    .map_err(|error| format!("读取自定义 JAR 失败: {error}"))?;
                if count == 0 {
                    break;
                }
                upload.write_chunk(&buffer[..count]).map_err(error_text)?;
            }
            upload.commit().map_err(error_text)?;
        }
        Ok(destination)
    })
    .await
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn custom_jar_streams_to_unicode_target_and_never_overwrites() {
        let temp = TempDir::new().unwrap();
        let source_dir = temp.path().join("下载目录");
        fs::create_dir(&source_dir).unwrap();
        let source = source_dir.join("自定义 核心.jar");
        let payload = [0, 1, 2, 0xff, 0, 42];
        fs::write(&source, payload).unwrap();
        let target = temp.path().join("服务器 目标/中文");

        let destination = copy_custom_jar(&source, &target).await.unwrap();
        assert_eq!(fs::read(&destination).unwrap(), payload);
        let error = copy_custom_jar(&source, &target).await.unwrap_err();
        assert!(error.contains("同名") || error.contains("已存在"));
        assert_eq!(fs::read(destination).unwrap(), payload);
    }
}
