use std::path::PathBuf;

use crate::config::CoreType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum WizardMethod {
    Auto,
    Import,
    Installer,
    Custom,
}

impl WizardMethod {
    pub(in crate::ui) const ALL: [Self; 4] =
        [Self::Auto, Self::Import, Self::Installer, Self::Custom];

    pub(in crate::ui) fn title(self) -> &'static str {
        match self {
            Self::Auto => "自动安装：查询官方版本并下载核心",
            Self::Import => "导入主机目录：只注册，不复制文件",
            Self::Installer => "已有安装器：复制并运行 Forge/NeoForge 安装器",
            Self::Custom => "自定义 JAR：复制到目标目录并注册",
        }
    }

    /// 后台任务的动作名：导入主机目录与自定义 JAR 只注册或复制，不经过安装。
    pub(in crate::ui) fn action_name(self) -> &'static str {
        match self {
            Self::Auto | Self::Installer => "安装",
            Self::Import | Self::Custom => "导入",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct VersionChoice {
    pub label: String,
    pub version: String,
    pub mc_version: String,
}

#[derive(Clone, Debug)]
pub(in crate::ui) struct WizardRequest {
    pub generation: u64,
    pub method: WizardMethod,
    pub core_type: CoreType,
    pub name: String,
    pub source_path: PathBuf,
    pub target_dir: PathBuf,
    pub java_path: String,
    pub min_memory_mb: i32,
    pub max_memory_mb: i32,
    pub jvm_args: String,
    pub version: String,
    pub mc_version: String,
}
