use std::{cmp::Ordering, collections::BTreeMap};

use reqwest::{Response, Url};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::error::{AppError, Result};

use super::InstallService;

const MOJANG_MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const PAPER_PROJECT_URL: &str = "https://fill.papermc.io/v3/projects/paper";
const NEOFORGE_VERSIONS_URL: &str =
    "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
const NEOFORGE_RELEASE_URL: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge/";
const FORGE_METADATA_URL: &str =
    "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
const FORGE_RELEASE_URL: &str =
    "https://maven.minecraftforge.net/releases/net/minecraftforge/forge/";
const FABRIC_GAME_URL: &str = "https://meta.fabricmc.net/v2/versions/game";
const FABRIC_RELEASE_URL: &str = "https://maven.fabricmc.net/net/fabricmc/fabric-installer/";

/// 对外版本查询与安装接口共用的版本描述。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionInfo {
    pub id: String,
    #[serde(default)]
    pub url: String,
    pub release_type: String,
}

/// 同时携带 Minecraft 版本、核心版本与下载地址的版本描述。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreVersionInfo {
    pub mc_version: String,
    pub core_version: String,
    pub download_url: String,
}

#[derive(Clone, Debug)]
pub(super) struct ApiEndpoints {
    vanilla_manifest: Url,
    paper_project: Url,
    neoforge_versions: Url,
    pub(super) neoforge_release: Url,
    forge_metadata: Url,
    pub(super) forge_release: Url,
    fabric_game: Url,
    pub(super) fabric_release: Url,
}

impl ApiEndpoints {
    pub(super) fn official() -> Self {
        Self {
            vanilla_manifest: official_url(MOJANG_MANIFEST_URL),
            paper_project: official_url(PAPER_PROJECT_URL),
            neoforge_versions: official_url(NEOFORGE_VERSIONS_URL),
            neoforge_release: official_url(NEOFORGE_RELEASE_URL),
            forge_metadata: official_url(FORGE_METADATA_URL),
            forge_release: official_url(FORGE_RELEASE_URL),
            fabric_game: official_url(FABRIC_GAME_URL),
            fabric_release: official_url(FABRIC_RELEASE_URL),
        }
    }

    #[cfg(test)]
    pub(super) fn mock(root: &Url) -> Self {
        Self {
            vanilla_manifest: root.join("mojang/version_manifest_v2.json").unwrap(),
            paper_project: root.join("paper").unwrap(),
            neoforge_versions: root.join("neo-api").unwrap(),
            neoforge_release: root.join("neo-maven/net/neoforged/neoforge/").unwrap(),
            forge_metadata: root.join("forge-metadata.xml").unwrap(),
            forge_release: root
                .join("forge-maven/releases/net/minecraftforge/forge/")
                .unwrap(),
            fabric_game: root.join("fabric-game").unwrap(),
            fabric_release: root
                .join("fabric-maven/net/fabricmc/fabric-installer/")
                .unwrap(),
        }
    }
}

fn official_url(value: &str) -> Url {
    Url::parse(value).expect("内置官方下载地址必须是有效 URL")
}

impl InstallService {
    pub async fn fetch_vanilla_versions(&self) -> Result<Vec<VersionInfo>> {
        let manifest = self.vanilla_manifest().await?;
        Ok(manifest
            .versions
            .into_iter()
            .filter(|version| {
                !version.id.is_empty()
                    && matches!(version.release_type.as_str(), "release" | "snapshot")
            })
            .map(|version| VersionInfo {
                id: version.id,
                url: version.url,
                release_type: version.release_type,
            })
            .collect())
    }

    pub async fn get_vanilla_server_url(&self, version_id: &str) -> Result<String> {
        validate_component(version_id, "Minecraft 版本")?;
        let manifest_url = self.endpoints.vanilla_manifest.clone();
        let manifest = self.vanilla_manifest().await?;
        let version = manifest
            .versions
            .into_iter()
            .find(|version| version.id == version_id)
            .ok_or_else(|| AppError::not_found(format!("未找到 Vanilla 版本 {version_id}")))?;
        if version.url.is_empty() {
            return Err(invalid_response(
                &manifest_url,
                format!("版本 {version_id} 缺少元数据地址"),
            ));
        }
        let metadata_url = parse_remote_url(&version.url, "Vanilla 版本元数据地址")?;
        let metadata: VanillaVersionMetadata = self.get_json(metadata_url.clone()).await?;
        let url = metadata
            .downloads
            .and_then(|downloads| downloads.server)
            .map(|server| server.url)
            .filter(|url| !url.is_empty())
            .ok_or_else(|| invalid_response(&metadata_url, "版本元数据缺少服务端下载地址"))?;
        parse_remote_url(&url, "Vanilla 服务端下载地址")?;
        Ok(url)
    }

    pub async fn fetch_paper_versions(&self) -> Result<Vec<VersionInfo>> {
        let manifest: PaperVersions = self.get_json(self.endpoints.paper_project.clone()).await?;
        let mut versions: Vec<String> = manifest
            .versions
            .into_values()
            .flatten()
            .filter(|version| !version.is_empty() && !version.contains('-'))
            .collect();
        versions.sort_by(|left, right| compare_version_numbers(right, left));
        versions.dedup();
        Ok(versions
            .into_iter()
            .map(|version| VersionInfo {
                id: version,
                url: String::new(),
                release_type: "release".to_owned(),
            })
            .collect())
    }

    pub(super) async fn get_paper_download(
        &self,
        mc_version: &str,
        build_number: &str,
    ) -> Result<PaperDownload> {
        validate_component(mc_version, "Minecraft 版本")?;
        validate_component(build_number, "Paper 构建号")?;
        let metadata_url = append_url(
            &self.endpoints.paper_project,
            &["versions", mc_version, "builds"],
        );
        let builds: Vec<PaperBuild> = self.get_json(metadata_url.clone()).await?;
        let build = if build_number == "latest" {
            builds
                .into_iter()
                .filter(|build| build.channel == "STABLE")
                .max_by_key(|build| build.id)
        } else {
            let build_id = build_number.parse::<u64>().map_err(|_| {
                AppError::invalid(
                    "invalid_build",
                    format!("Paper 构建号必须是数字或 latest: {build_number}"),
                )
            })?;
            builds.into_iter().find(|build| build.id == build_id)
        }
        .ok_or_else(|| {
            AppError::not_found(format!("未找到 Paper {mc_version} 构建 {build_number}"))
        })?;
        let file = build
            .downloads
            .into_iter()
            .find_map(|(kind, file)| (kind == "server:default").then_some(file))
            .filter(|file| !file.name.is_empty() && !file.url.is_empty())
            .ok_or_else(|| invalid_response(&metadata_url, "Paper 构建缺少 server:default 下载"))?;
        let url = parse_remote_url(&file.url, "Paper 服务端下载地址")?;
        Ok(PaperDownload {
            build_number: build.id.to_string(),
            url: url.into(),
        })
    }

    pub async fn get_paper_download_url(
        &self,
        mc_version: &str,
        build_number: &str,
    ) -> Result<String> {
        Ok(self.get_paper_download(mc_version, build_number).await?.url)
    }

    pub async fn fetch_neoforge_versions(&self, mc_version: &str) -> Result<Vec<CoreVersionInfo>> {
        validate_component(mc_version, "Minecraft 版本")?;
        let manifest: NeoForgeVersions = self
            .get_json(self.endpoints.neoforge_versions.clone())
            .await?;
        let prefix = neoforge_prefix(mc_version);
        Ok(manifest
            .versions
            .into_iter()
            .filter(|version| version.starts_with(&prefix))
            .map(|version| {
                let file_name = format!("neoforge-{version}-installer.jar");
                CoreVersionInfo {
                    mc_version: mc_version.to_owned(),
                    download_url: append_url(
                        &self.endpoints.neoforge_release,
                        &[&version, &file_name],
                    )
                    .into(),
                    core_version: version,
                }
            })
            .collect())
    }

    pub async fn fetch_forge_versions(&self) -> Result<Vec<VersionInfo>> {
        let metadata_url = self.endpoints.forge_metadata.clone();
        let body = self.get_text(metadata_url).await?;
        let mut latest_by_mc = BTreeMap::<String, String>::new();
        for full_version in forge_xml_versions(&body) {
            let Some((mc_version, _)) = full_version.split_once('-') else {
                continue;
            };
            if mc_version.len() < 4 {
                continue;
            }
            // 官方 metadata 按发布时间递增，最后一次出现即该 MC 版本的最新构建。
            latest_by_mc.insert(mc_version.to_owned(), full_version);
        }
        let mut versions: Vec<VersionInfo> = latest_by_mc
            .into_iter()
            .map(|(mc_version, full_version)| VersionInfo {
                id: full_version,
                url: mc_version,
                release_type: "release".to_owned(),
            })
            .collect();
        versions.sort_by(|left, right| compare_version_numbers(&right.url, &left.url));
        Ok(versions)
    }

    pub async fn fetch_fabric_versions(&self) -> Result<Vec<VersionInfo>> {
        let versions: Vec<FabricGameVersion> =
            self.get_json(self.endpoints.fabric_game.clone()).await?;
        Ok(versions
            .into_iter()
            .filter(|version| !version.version.is_empty())
            .map(|version| VersionInfo {
                id: version.version,
                url: String::new(),
                release_type: if version.stable {
                    "release"
                } else {
                    "snapshot"
                }
                .to_owned(),
            })
            .collect())
    }

    async fn vanilla_manifest(&self) -> Result<VanillaManifest> {
        self.get_json(self.endpoints.vanilla_manifest.clone()).await
    }

    async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T> {
        let response = self.get(url.clone()).await?;
        response
            .json::<T>()
            .await
            .map_err(|error| invalid_response(&url, format!("响应不是预期 JSON: {error}")))
    }

    async fn get_text(&self, url: Url) -> Result<String> {
        let response = self.get(url.clone()).await?;
        response.text().await.map_err(|error| {
            AppError::unavailable(
                "response_read_failed",
                format!("读取 {url} 响应失败: {error}"),
            )
        })
    }

    async fn get(&self, url: Url) -> Result<Response> {
        let response = self
            .client
            .get(url.clone())
            .timeout(super::REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| {
                AppError::unavailable("http_request_failed", format!("请求 {url} 失败: {error}"))
            })?;
        if !response.status().is_success() {
            return Err(AppError::unavailable(
                "http_status",
                format!("请求 {url} 返回 HTTP {}", response.status()),
            ));
        }
        Ok(response)
    }
}

#[derive(Deserialize)]
struct VanillaManifest {
    #[serde(default)]
    versions: Vec<VanillaVersion>,
}

#[derive(Deserialize)]
struct VanillaVersion {
    #[serde(default)]
    id: String,
    #[serde(default)]
    url: String,
    #[serde(rename = "type", default)]
    release_type: String,
}

#[derive(Deserialize)]
struct VanillaVersionMetadata {
    #[serde(default)]
    downloads: Option<VanillaDownloads>,
}

#[derive(Deserialize)]
struct VanillaDownloads {
    #[serde(default)]
    server: Option<VanillaServerDownload>,
}

#[derive(Deserialize)]
struct VanillaServerDownload {
    #[serde(default)]
    url: String,
}

#[derive(Deserialize)]
struct PaperVersions {
    #[serde(default)]
    versions: BTreeMap<String, Vec<String>>,
}

pub(super) struct PaperDownload {
    pub(super) build_number: String,
    pub(super) url: String,
}

#[derive(Deserialize)]
struct PaperBuild {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    channel: String,
    #[serde(default)]
    downloads: BTreeMap<String, PaperDownloadFile>,
}

#[derive(Deserialize)]
struct PaperDownloadFile {
    #[serde(default)]
    name: String,
    #[serde(default)]
    url: String,
}

#[derive(Deserialize)]
struct NeoForgeVersions {
    #[serde(default)]
    versions: Vec<String>,
}

#[derive(Deserialize)]
struct FabricGameVersion {
    #[serde(default)]
    version: String,
    #[serde(default)]
    stable: bool,
}

pub(super) fn append_url(base: &Url, segments: &[&str]) -> Url {
    let mut url = base.clone();
    url.path_segments_mut()
        .expect("HTTP 下载地址必须支持路径段")
        .pop_if_empty()
        .extend(segments.iter().copied());
    url
}

pub(super) fn parse_remote_url(value: &str, context: &str) -> Result<Url> {
    let url = Url::parse(value).map_err(|error| {
        AppError::unavailable("invalid_download_url", format!("{context}无效: {error}"))
    })?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(AppError::unavailable(
            "invalid_download_url",
            format!("{context}必须是带主机名的 HTTP(S) 地址: {url}"),
        ));
    }
    Ok(url)
}

fn invalid_response(url: &Url, message: impl AsRef<str>) -> AppError {
    AppError::unavailable(
        "invalid_response",
        format!("{} 的响应无效: {}", url, message.as_ref()),
    )
}

pub(super) fn validate_component(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.trim() != value
        || value.contains(['/', '\\', '\0'])
        || matches!(value, "." | "..")
    {
        return Err(AppError::invalid(
            "invalid_version",
            format!("{label}无效: 不能为空、为 . 或 ..，也不能包含路径分隔符、NUL 或首尾空白"),
        ));
    }
    Ok(())
}

pub(super) fn neoforge_prefix(mc_version: &str) -> String {
    let version = mc_version.strip_prefix("1.").unwrap_or(mc_version);
    if !version.contains('.') {
        return format!("{version}.0.");
    }
    let first_dot = version.find('.').unwrap();
    if let Some(relative_second) = version[first_dot + 1..].find('.') {
        let second_dot = first_dot + 1 + relative_second;
        format!("{}.", &version[..second_dot])
    } else {
        format!("{version}.")
    }
}

fn forge_xml_versions(body: &str) -> Vec<String> {
    const OPEN: &str = "<version>";
    const CLOSE: &str = "</version>";
    let mut versions = Vec::new();
    let mut position = 0;
    while let Some(relative_start) = body[position..].find(OPEN) {
        let start = position + relative_start + OPEN.len();
        let Some(relative_end) = body[start..].find(CLOSE) else {
            break;
        };
        let end = start + relative_end;
        let version = &body[start..end];
        if !version.is_empty() {
            versions.push(version.to_owned());
        }
        position = end + CLOSE.len();
    }
    versions
}

fn compare_version_numbers(left: &str, right: &str) -> Ordering {
    let mut left_segments = left.split('.');
    let mut right_segments = right.split('.');
    loop {
        match (left_segments.next(), right_segments.next()) {
            (Some(left), Some(right)) => {
                let left_number = left.parse::<i64>().unwrap_or(0);
                let right_number = right.parse::<i64>().unwrap_or(0);
                let ordering = left_number.cmp(&right_number);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (None, None) => return Ordering::Equal,
        }
    }
}
