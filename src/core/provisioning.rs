use std::path::{Path, PathBuf};

use super::{ServerId, ServerManager};
use crate::{
    config::{CoreType, ServerConfig},
    error::{AppError, Result},
    server::{
        detect::{self, DetectedCore},
        install::{InstallService, ProgressCallback},
    },
};

/// 调用方先按各自契约解析版本，共享服务只执行确定的安装参数。
pub struct AutoInstallRequest<'a> {
    pub core_type: CoreType,
    pub minecraft_version: &'a str,
    /// Paper 为构建号，Fabric 为安装器版本，Forge/NeoForge 为核心版本。
    pub core_version: &'a str,
    pub target_dir: &'a Path,
    pub java_path: &'a str,
}

pub async fn install_auto(
    service: &InstallService,
    request: AutoInstallRequest<'_>,
    progress: ProgressCallback,
) -> Result<DetectedCore> {
    let AutoInstallRequest {
        core_type,
        minecraft_version,
        core_version,
        target_dir,
        java_path,
    } = request;
    match core_type {
        CoreType::Vanilla => {
            service
                .install_vanilla(minecraft_version, target_dir, java_path, Some(progress))
                .await
        }
        CoreType::Paper => {
            service
                .install_paper(
                    minecraft_version,
                    core_version,
                    target_dir,
                    java_path,
                    Some(progress),
                )
                .await
        }
        CoreType::Forge => {
            service
                .install_forge(
                    minecraft_version,
                    core_version,
                    target_dir,
                    java_path,
                    Some(progress),
                )
                .await
        }
        CoreType::Neoforge => {
            service
                .install_neoforge(
                    minecraft_version,
                    core_version,
                    target_dir,
                    java_path,
                    Some(progress),
                )
                .await
        }
        CoreType::Fabric => {
            service
                .install_fabric(
                    minecraft_version,
                    core_version,
                    target_dir,
                    java_path,
                    Some(progress),
                )
                .await
        }
        other => Err(AppError::invalid(
            "unsupported_core",
            format!("{} 不支持自动安装", other.display_name()),
        )),
    }
}

/// 只共享阻塞检测与 JoinError 边界，错误文案仍由各调用方按原契约映射。
pub async fn locate_installer(
    directory: &Path,
) -> std::result::Result<Option<PathBuf>, tokio::task::JoinError> {
    let directory = directory.to_path_buf();
    tokio::task::spawn_blocking(move || detect::find_installer_jar(&directory)).await
}

pub async fn run_registered_installer(
    manager: &ServerManager,
    service: &InstallService,
    id: ServerId,
    server: &ServerConfig,
    installer_path: &Path,
    progress: ProgressCallback,
) -> Result<()> {
    let java = manager.config().effective_java_path(server).to_owned();
    let process = manager
        .process(id)
        .ok_or_else(|| AppError::not_found("服务器运行态不存在"))?;
    // 调用方的工作租约只能阻止新启动；这里还要等待已越过租约检查的启动结束。
    let _operation = process.lock_stopped().await?;
    let detected = service
        .run_installer_jar(installer_path, &server.working_dir, &java, Some(progress))
        .await?;
    manager.apply_installer_detection(id, &detected)
}

/// 新服务器配置会完整采用检测结果；已有服务器的增量回写仍由 ServerManager 负责。
pub fn apply_detection_to_config(server: &mut ServerConfig, detected: DetectedCore) {
    server.core_type = detected.core_type;
    server.minecraft_version = detected.minecraft_version.unwrap_or_default();
    server.core_version = detected.core_version.unwrap_or_default();
    server.server_jar_path = detected.server_jar_path.unwrap_or_default();
    server.launch_args_file = detected.launch_args_file.unwrap_or_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_replaces_all_launch_fields_for_new_server() {
        let mut server = ServerConfig::new("测试", PathBuf::from("/tmp/服务器"));
        server.core_version = "旧版本".to_owned();
        apply_detection_to_config(
            &mut server,
            DetectedCore {
                core_type: CoreType::Forge,
                minecraft_version: Some("1.21.1".to_owned()),
                core_version: None,
                server_jar_path: None,
                launch_args_file: Some(PathBuf::from("libraries/unix_args.txt")),
                installer_jar: None,
            },
        );
        assert_eq!(server.core_type, CoreType::Forge);
        assert_eq!(server.minecraft_version, "1.21.1");
        assert!(server.core_version.is_empty());
        assert!(server.server_jar_path.as_os_str().is_empty());
        assert_eq!(
            server.launch_args_file,
            PathBuf::from("libraries/unix_args.txt")
        );
    }

    #[tokio::test]
    async fn unsupported_auto_core_fails_before_network() {
        let service = InstallService::new().unwrap();
        let error = install_auto(
            &service,
            AutoInstallRequest {
                core_type: CoreType::Custom,
                minecraft_version: "",
                core_version: "",
                target_dir: Path::new("/tmp/不会创建"),
                java_path: "java",
            },
            std::sync::Arc::new(|_| {}),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "unsupported_core");
    }
}
