//! 核心版本查询、下载与安装器执行。
//!
//! 对外路径为 `crate::server::install`：`InstallService`、版本描述、
//! `ProgressCallback` 以及模块级自由函数都由本模块导出。

mod compat;
mod download;
mod installer;
mod service;
mod version;

#[cfg(test)]
mod tests;

use std::{sync::Arc, time::Duration};

use reqwest::Client;

use crate::error::{AppError, Result};

pub use compat::{
    fetch_fabric_versions, fetch_forge_versions, fetch_neoforge_versions, fetch_paper_versions,
    fetch_vanilla_versions, get_paper_download_url, get_vanilla_server_url, install_fabric,
    install_forge, install_neoforge, install_paper, install_vanilla, run_installer_jar,
};
pub use version::{CoreVersionInfo, VersionInfo};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);
const DOWNLOAD_STALL_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_INSTALLER_LINE_BYTES: usize = 64 * 1024;
const INSTALLER_ERROR_TAIL_LINES: usize = 5;

/// Web 后台任务可直接把回调转发到任务日志；每次调用只交付一条有长度上限的输出。
pub type ProgressCallback = Arc<dyn Fn(&str) + Send + Sync + 'static>;

/// 持有带超时与 User-Agent 的 HTTP 客户端，以及一组固定官方 API 端点。
///
/// 通常直接使用模块级异步函数即可；需要复用连接池时，可持有本服务并调用同名方法。
#[derive(Clone, Debug)]
pub struct InstallService {
    client: Client,
    endpoints: version::ApiEndpoints,
}

impl InstallService {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent(concat!("MCServerManager/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| {
                AppError::internal(format!("无法创建核心下载 HTTP 客户端: {error}"))
            })?;
        Ok(Self {
            client,
            endpoints: version::ApiEndpoints::official(),
        })
    }

    #[cfg(test)]
    fn with_mock_root(root: reqwest::Url) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .user_agent("MCServerManager-tests")
            .build()
            .unwrap();
        Self {
            client,
            endpoints: version::ApiEndpoints::mock(&root),
        }
    }
}

pub(super) fn notify(progress: Option<&ProgressCallback>, message: impl AsRef<str>) {
    if let Some(progress) = progress {
        progress(message.as_ref());
    }
}
