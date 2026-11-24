use std::{
    ffi::OsString,
    io::ErrorKind as IoErrorKind,
    path::{Path, PathBuf},
};

use reqwest::Url;

use crate::{
    config::CoreType,
    error::{AppError, Result},
    server::detect::{DetectedCore, detect_directory},
};

use super::{
    InstallService, ProgressCallback,
    download::copy_file_atomic,
    installer::{ensure_user_jvm_args, run_forge_installer, run_java_command},
    notify,
    version::{append_url, parse_remote_url, validate_component},
};

impl InstallService {
    pub async fn install_vanilla(
        &self,
        mc_version: &str,
        target_dir: &Path,
        _java_path: &str,
        progress: Option<ProgressCallback>,
    ) -> Result<DetectedCore> {
        validate_component(mc_version, "Minecraft 版本")?;
        let target_dir = prepare_empty_target(target_dir).await?;
        notify(
            progress.as_ref(),
            format!("正在查询 Vanilla {mc_version} 下载地址…"),
        );
        let url = self.get_vanilla_server_url(mc_version).await?;
        let url = parse_remote_url(&url, "Vanilla 服务端下载地址")?;
        let jar_path = target_dir.join("server.jar");
        notify(progress.as_ref(), format!("正在下载 Vanilla {mc_version}…"));
        self.download_file(url, &jar_path).await?;

        let mut detected = detect_or_default(&target_dir, CoreType::Vanilla).await?;
        detected
            .minecraft_version
            .get_or_insert_with(|| mc_version.to_owned());
        detected.server_jar_path.get_or_insert(jar_path);
        notify(
            progress.as_ref(),
            format!("Vanilla {mc_version} 安装完成！"),
        );
        Ok(detected)
    }

    pub async fn install_paper(
        &self,
        mc_version: &str,
        build_number: &str,
        target_dir: &Path,
        _java_path: &str,
        progress: Option<ProgressCallback>,
    ) -> Result<DetectedCore> {
        validate_component(mc_version, "Minecraft 版本")?;
        validate_component(build_number, "Paper 构建号")?;
        let target_dir = prepare_empty_target(target_dir).await?;
        notify(
            progress.as_ref(),
            format!("正在查询 Paper {mc_version} 构建 {build_number}…"),
        );
        let download = self.get_paper_download(mc_version, build_number).await?;
        let url = parse_remote_url(&download.url, "Paper 服务端下载地址")?;
        let jar_path = target_dir.join(format!("paper-{mc_version}-{}.jar", download.build_number));
        notify(progress.as_ref(), "正在下载 Paper…");
        self.download_file(url, &jar_path).await?;

        let mut detected = detect_or_default(&target_dir, CoreType::Paper).await?;
        detected
            .minecraft_version
            .get_or_insert_with(|| mc_version.to_owned());
        detected
            .core_version
            .get_or_insert_with(|| download.build_number.clone());
        detected.server_jar_path.get_or_insert(jar_path);
        notify(progress.as_ref(), "Paper 安装完成！");
        Ok(detected)
    }

    pub async fn install_neoforge(
        &self,
        mc_version: &str,
        core_version: &str,
        target_dir: &Path,
        java_path: &str,
        progress: Option<ProgressCallback>,
    ) -> Result<DetectedCore> {
        self.install_forge_like(
            &self.endpoints.neoforge_release,
            "neoforge",
            "NeoForge",
            mc_version,
            core_version,
            target_dir,
            java_path,
            progress,
        )
        .await
    }

    pub async fn install_forge(
        &self,
        mc_version: &str,
        core_version: &str,
        target_dir: &Path,
        java_path: &str,
        progress: Option<ProgressCallback>,
    ) -> Result<DetectedCore> {
        // core_version 留空时，把 mc_version 当作完整的 Forge 版本使用。
        let version = if core_version.is_empty() {
            mc_version
        } else {
            core_version
        };
        self.install_forge_like(
            &self.endpoints.forge_release,
            "forge",
            "Forge",
            mc_version,
            version,
            target_dir,
            java_path,
            progress,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn install_forge_like(
        &self,
        release_url: &Url,
        core_name: &str,
        display_name: &str,
        mc_version: &str,
        core_version: &str,
        target_dir: &Path,
        java_path: &str,
        progress: Option<ProgressCallback>,
    ) -> Result<DetectedCore> {
        validate_component(mc_version, "Minecraft 版本")?;
        validate_component(core_version, "核心版本")?;
        let target_dir = prepare_empty_target(target_dir).await?;
        let file_name = format!("{core_name}-{core_version}-installer.jar");
        let url = append_url(release_url, &[core_version, &file_name]);
        let installer_path = target_dir.join(&file_name);
        notify(
            progress.as_ref(),
            format!("正在下载 {display_name} {core_version} 安装器…"),
        );
        self.download_file(url, &installer_path).await?;

        notify(
            progress.as_ref(),
            format!("正在运行 {display_name} 安装器（可能需要数分钟）…"),
        );
        run_forge_installer(java_path, &installer_path, &target_dir, progress.as_ref()).await?;

        let mut detected = detect_installed(&target_dir).await?.ok_or_else(|| {
            AppError::internal(format!(
                "{display_name} 安装器已退出，但无法在 {} 识别已安装核心",
                target_dir.display()
            ))
        })?;
        ensure_installation_has_launch_target(&detected, display_name, &target_dir)?;
        detected
            .minecraft_version
            .get_or_insert_with(|| mc_version.to_owned());
        detected
            .core_version
            .get_or_insert_with(|| core_version.to_owned());
        ensure_user_jvm_args(&target_dir).await;
        notify(
            progress.as_ref(),
            format!("{display_name} {core_version} 安装完成！"),
        );
        // 检测器根据实际文件给出精确的核心类型与启动路径。
        Ok(detected)
    }

    pub async fn install_fabric(
        &self,
        mc_version: &str,
        installer_version: &str,
        target_dir: &Path,
        java_path: &str,
        progress: Option<ProgressCallback>,
    ) -> Result<DetectedCore> {
        validate_component(mc_version, "Minecraft 版本")?;
        validate_component(installer_version, "Fabric 安装器版本")?;
        let target_dir = prepare_empty_target(target_dir).await?;
        let file_name = format!("fabric-installer-{installer_version}.jar");
        let url = append_url(
            &self.endpoints.fabric_release,
            &[installer_version, &file_name],
        );
        let installer_path = target_dir.join(file_name);
        notify(progress.as_ref(), "正在下载 Fabric 安装器…");
        self.download_file(url, &installer_path).await?;

        notify(progress.as_ref(), "正在运行 Fabric 安装器…");
        let arguments = vec![
            OsString::from("-jar"),
            installer_path.clone().into_os_string(),
            OsString::from("server"),
            OsString::from("-mcversion"),
            OsString::from(mc_version),
            OsString::from("-dir"),
            target_dir.clone().into_os_string(),
        ];
        run_java_command(
            java_path,
            arguments,
            &target_dir,
            "Fabric 安装器",
            progress.as_ref(),
        )
        .await?;

        // Fabric 官方安装器只安装加载器，仍需补上对应版本的 Vanilla 服务端。
        notify(progress.as_ref(), "正在下载 Minecraft 服务端 JAR…");
        let vanilla_url = self.get_vanilla_server_url(mc_version).await?;
        let vanilla_url = parse_remote_url(&vanilla_url, "Vanilla 服务端下载地址")?;
        self.download_file(vanilla_url, &target_dir.join("server.jar"))
            .await?;

        let mut detected = detect_or_default(&target_dir, CoreType::Fabric).await?;
        detected
            .minecraft_version
            .get_or_insert_with(|| mc_version.to_owned());
        notify(progress.as_ref(), "Fabric 安装完成！");
        Ok(detected)
    }

    /// 运行用户提供的 Forge/NeoForge 安装器，并返回安装后的核心检测结果。
    ///
    /// 与自动安装不同，本方法明确支持已有非空整合包目录，以覆盖“启动前补跑安装器”流程。
    /// 它不会覆盖同名但内容不同的已有安装器文件，也不会删除目录中的任何既有文件。
    pub async fn run_installer_jar(
        &self,
        installer_jar_path: &Path,
        target_dir: &Path,
        java_path: &str,
        progress: Option<ProgressCallback>,
    ) -> Result<DetectedCore> {
        let target_dir = prepare_existing_target(target_dir).await?;
        let source_name = installer_jar_path
            .file_name()
            .ok_or_else(|| AppError::invalid("invalid_installer", "安装器路径缺少文件名"))?;
        let source = std::fs::canonicalize(installer_jar_path).map_err(|error| {
            AppError::io(
                format!("无法访问安装器 {}", installer_jar_path.display()),
                error,
            )
        })?;
        if !source.is_file() {
            return Err(AppError::invalid(
                "invalid_installer",
                format!("安装器不是文件: {}", source.display()),
            ));
        }
        let destination = target_dir.join(source_name);
        let destination_matches_source = destination
            .canonicalize()
            .is_ok_and(|canonical| canonical == source);
        if !destination_matches_source {
            copy_file_atomic(&source, &destination).await?;
        }

        notify(
            progress.as_ref(),
            format!("正在 {} 中运行安装器…", target_dir.display()),
        );
        run_forge_installer(java_path, &destination, &target_dir, progress.as_ref()).await?;
        let detected = detect_installed(&target_dir).await?.ok_or_else(|| {
            AppError::internal(format!(
                "安装器已退出，但无法在 {} 识别已安装核心",
                target_dir.display()
            ))
        })?;
        ensure_installation_has_launch_target(&detected, "安装器", &target_dir)?;
        ensure_user_jvm_args(&target_dir).await;
        notify(progress.as_ref(), "安装器执行完成！");
        Ok(detected)
    }
}

async fn prepare_empty_target(target_dir: &Path) -> Result<PathBuf> {
    let target_dir = prepare_existing_target(target_dir).await?;
    let mut entries = tokio::fs::read_dir(&target_dir).await.map_err(|error| {
        AppError::io(
            format!("无法读取目标服务器目录 {}", target_dir.display()),
            error,
        )
    })?;
    if entries
        .next_entry()
        .await
        .map_err(|error| {
            AppError::io(
                format!("无法检查目标服务器目录 {}", target_dir.display()),
                error,
            )
        })?
        .is_some()
    {
        return Err(AppError::already_exists(format!(
            "目标服务器目录已存在且非空，拒绝覆盖: {}",
            target_dir.display()
        )));
    }
    Ok(target_dir)
}

async fn prepare_existing_target(target_dir: &Path) -> Result<PathBuf> {
    if target_dir.as_os_str().is_empty() {
        return Err(AppError::invalid("invalid_path", "目标服务器目录不能为空"));
    }
    match tokio::fs::symlink_metadata(target_dir).await {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            return Err(AppError::already_exists(format!(
                "目标服务器路径不是普通目录: {}",
                target_dir.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == IoErrorKind::NotFound => {
            tokio::fs::create_dir_all(target_dir)
                .await
                .map_err(|error| {
                    AppError::io(
                        format!("无法创建目标服务器目录 {}", target_dir.display()),
                        error,
                    )
                })?;
        }
        Err(error) => {
            return Err(AppError::io(
                format!("无法检查目标服务器目录 {}", target_dir.display()),
                error,
            ));
        }
    }
    tokio::fs::canonicalize(target_dir).await.map_err(|error| {
        AppError::io(
            format!("无法解析目标服务器目录 {}", target_dir.display()),
            error,
        )
    })
}

async fn detect_installed(target_dir: &Path) -> Result<Option<DetectedCore>> {
    let target_dir = target_dir.to_path_buf();
    tokio::task::spawn_blocking(move || detect_directory(&target_dir))
        .await
        .map_err(|error| AppError::internal(format!("核心检测任务执行失败: {error}")))
}

fn ensure_installation_has_launch_target(
    detected: &DetectedCore,
    display_name: &str,
    target_dir: &Path,
) -> Result<()> {
    if detected.launch_args_file.is_some() || detected.server_jar_path.is_some() {
        return Ok(());
    }
    Err(AppError::unavailable(
        "installer_incomplete",
        format!(
            "{display_name}已退出，但未在 {} 生成启动 JAR 或启动参数文件，不能判定安装成功",
            target_dir.display()
        ),
    ))
}

async fn detect_or_default(target_dir: &Path, core_type: CoreType) -> Result<DetectedCore> {
    Ok(detect_installed(target_dir).await?.unwrap_or(DetectedCore {
        core_type,
        minecraft_version: None,
        core_version: None,
        server_jar_path: None,
        launch_args_file: None,
        installer_jar: None,
    }))
}
