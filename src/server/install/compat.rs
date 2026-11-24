//! 模块级自由函数，转发到 `InstallService` 的同名方法；目前 crate 内没有调用方。

use std::path::Path;

use crate::{error::Result, server::detect::DetectedCore};

use super::{CoreVersionInfo, InstallService, ProgressCallback, VersionInfo};

pub async fn fetch_vanilla_versions() -> Result<Vec<VersionInfo>> {
    InstallService::new()?.fetch_vanilla_versions().await
}

pub async fn get_vanilla_server_url(version_id: &str) -> Result<String> {
    InstallService::new()?
        .get_vanilla_server_url(version_id)
        .await
}

pub async fn fetch_paper_versions() -> Result<Vec<VersionInfo>> {
    InstallService::new()?.fetch_paper_versions().await
}

pub async fn get_paper_download_url(mc_version: &str, build_number: &str) -> Result<String> {
    InstallService::new()?
        .get_paper_download_url(mc_version, build_number)
        .await
}

pub async fn fetch_neoforge_versions(mc_version: &str) -> Result<Vec<CoreVersionInfo>> {
    InstallService::new()?
        .fetch_neoforge_versions(mc_version)
        .await
}

pub async fn fetch_forge_versions() -> Result<Vec<VersionInfo>> {
    InstallService::new()?.fetch_forge_versions().await
}

pub async fn fetch_fabric_versions() -> Result<Vec<VersionInfo>> {
    InstallService::new()?.fetch_fabric_versions().await
}

pub async fn install_vanilla(
    mc_version: &str,
    target_dir: &Path,
    java_path: &str,
    progress: Option<ProgressCallback>,
) -> Result<DetectedCore> {
    InstallService::new()?
        .install_vanilla(mc_version, target_dir, java_path, progress)
        .await
}

pub async fn install_paper(
    mc_version: &str,
    build_number: &str,
    target_dir: &Path,
    java_path: &str,
    progress: Option<ProgressCallback>,
) -> Result<DetectedCore> {
    InstallService::new()?
        .install_paper(mc_version, build_number, target_dir, java_path, progress)
        .await
}

pub async fn install_neoforge(
    mc_version: &str,
    core_version: &str,
    target_dir: &Path,
    java_path: &str,
    progress: Option<ProgressCallback>,
) -> Result<DetectedCore> {
    InstallService::new()?
        .install_neoforge(mc_version, core_version, target_dir, java_path, progress)
        .await
}

pub async fn install_forge(
    mc_version: &str,
    core_version: &str,
    target_dir: &Path,
    java_path: &str,
    progress: Option<ProgressCallback>,
) -> Result<DetectedCore> {
    InstallService::new()?
        .install_forge(mc_version, core_version, target_dir, java_path, progress)
        .await
}

pub async fn install_fabric(
    mc_version: &str,
    installer_version: &str,
    target_dir: &Path,
    java_path: &str,
    progress: Option<ProgressCallback>,
) -> Result<DetectedCore> {
    InstallService::new()?
        .install_fabric(
            mc_version,
            installer_version,
            target_dir,
            java_path,
            progress,
        )
        .await
}

pub async fn run_installer_jar(
    installer_jar_path: &Path,
    target_dir: &Path,
    java_path: &str,
    progress: Option<ProgressCallback>,
) -> Result<DetectedCore> {
    InstallService::new()?
        .run_installer_jar(installer_jar_path, target_dir, java_path, progress)
        .await
}
