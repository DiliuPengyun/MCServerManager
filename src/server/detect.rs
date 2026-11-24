use std::{
    ffi::OsStr,
    fs::{self, File},
    path::{Path, PathBuf},
};

use walkdir::WalkDir;
use zip::ZipArchive;

use crate::config::{CoreType, ServerConfig};

/// 对现有服务器目录的尽力检测结果。
///
/// 路径和版本使用 `Option`，避免把猜测出来的值写回服务器配置。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedCore {
    pub core_type: CoreType,
    pub minecraft_version: Option<String>,
    pub core_version: Option<String>,
    pub server_jar_path: Option<PathBuf>,
    pub launch_args_file: Option<PathBuf>,
    pub installer_jar: Option<PathBuf>,
}

impl DetectedCore {
    fn new(core_type: CoreType) -> Self {
        Self {
            core_type,
            minecraft_version: None,
            core_version: None,
            server_jar_path: None,
            launch_args_file: None,
            installer_jar: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoaderKind {
    Neoforge,
    Forge,
    Fabric,
}

impl LoaderKind {
    fn core_type(self) -> CoreType {
        match self {
            Self::Neoforge => CoreType::Neoforge,
            Self::Forge => CoreType::Forge,
            Self::Fabric => CoreType::Fabric,
        }
    }

    fn jar_prefix(self) -> Option<&'static str> {
        match self {
            Self::Neoforge => Some("neoforge"),
            Self::Forge => Some("forge"),
            Self::Fabric => None,
        }
    }
}

#[derive(Clone, Debug)]
struct LoaderCandidate {
    path: PathBuf,
    kind: LoaderKind,
    installer: bool,
}

/// 检测目录中的服务端核心；目录不存在或没有可用线索时返回 `None`。
///
/// 已安装的加载器优先按 libraries/net 下的 neoforged、minecraftforge、
/// fabricmc 顺序判定。没有这些目录时再按 JAR 文件名和 ZIP 中央目录识别：
/// 先看顶层 JAR，找不到时递归扫描并跳过 mods 与 libraries。
pub fn detect_directory(directory: &Path) -> Option<DetectedCore> {
    let base = canonical_directory(directory)?;
    let candidates = loader_candidates(&base);
    let installer_jar = preferred_installer(&candidates);

    if base.join("libraries/net/neoforged").is_dir() {
        return Some(detect_forge_family(
            &base,
            &candidates,
            installer_jar,
            LoaderKind::Neoforge,
        ));
    }
    if base.join("libraries/net/minecraftforge").is_dir() {
        return Some(detect_forge_family(
            &base,
            &candidates,
            installer_jar,
            LoaderKind::Forge,
        ));
    }
    if base.join("libraries/net/fabricmc").is_dir() {
        let mut result = DetectedCore::new(CoreType::Fabric);
        result.minecraft_version = minecraft_version_from_json(&base);
        result.server_jar_path =
            loader_server(&candidates, LoaderKind::Fabric).or_else(|| find_server_jar(&base));
        result.installer_jar = installer_jar;
        return Some(result);
    }

    detect_without_libraries(&base, &candidates, installer_jar)
}

/// 在顶层 JAR 中查找 Forge/NeoForge 安装器。
///
/// 加载器类型优先按 ZIP 中央目录条目判定，文件名只作补充，因此安装器即使
/// 被改名为 server.jar 或 forge.jar，仍可根据实际条目识别；文件名含 installer
/// 的 JAR 也按安装器处理。NeoForge 优先于 Forge。
pub fn find_installer_jar(directory: &Path) -> Option<PathBuf> {
    let base = canonical_directory(directory)?;
    preferred_installer(&loader_candidates(&base))
}

/// 按文件名（含 installer）及 ZIP 中央目录中的 Forge/NeoForge 安装器条目判断。
pub fn jar_is_installer(jar_path: &Path) -> bool {
    lower_file_name(jar_path).contains("installer") || zip_installer_kind(jar_path).is_some()
}

/// 启动前是否仍需运行安装器。
///
/// 已配置且存在的启动参数文件优先；其次，存在且并非安装器的 JAR 也视为
/// 已安装。找不到安装器时不拦截启动，让后续启动流程报告原始错误。
pub fn needs_installer(server: &ServerConfig) -> bool {
    if !server.core_type.uses_installer() {
        return false;
    }
    if server.launch_args_path().is_some_and(|path| path.is_file()) {
        return false;
    }
    if server
        .jar_path()
        .is_some_and(|path| path.is_file() && !jar_is_installer(&path))
    {
        return false;
    }
    !server.working_dir.as_os_str().is_empty() && find_installer_jar(&server.working_dir).is_some()
}

fn canonical_directory(directory: &Path) -> Option<PathBuf> {
    fs::canonicalize(directory)
        .ok()
        .filter(|path| path.is_dir())
}

fn detect_forge_family(
    base: &Path,
    candidates: &[LoaderCandidate],
    installer_jar: Option<PathBuf>,
    kind: LoaderKind,
) -> DetectedCore {
    let (organization, artifact, args_marker) = match kind {
        LoaderKind::Neoforge => ("neoforged", "neoforge", "neoforged"),
        LoaderKind::Forge => ("minecraftforge", "forge", "minecraftforge"),
        LoaderKind::Fabric => unreachable!("Fabric 不使用 Forge 启动参数检测"),
    };
    let core_version = core_version_from_libraries(base, organization, artifact);
    let mut result = DetectedCore::new(kind.core_type());
    result.minecraft_version = minecraft_version_from_json(base).or_else(|| {
        (kind == LoaderKind::Forge)
            .then(|| core_version.as_deref().and_then(forge_minecraft_version))
            .flatten()
    });
    result.launch_args_file = find_unix_args_file(base, args_marker);
    result.server_jar_path =
        find_library_server_jar(base, organization, artifact, core_version.as_deref())
            .or_else(|| loader_server(candidates, kind))
            .or_else(|| find_server_jar(base));
    result.core_version = core_version;
    result.installer_jar = installer_jar;
    result
}

fn detect_without_libraries(
    base: &Path,
    candidates: &[LoaderCandidate],
    installer_jar: Option<PathBuf>,
) -> Option<DetectedCore> {
    // neoforge 包含 forge 子串，因此所有按名称分类和选择都必须先处理 NeoForge。
    for kind in [LoaderKind::Neoforge, LoaderKind::Forge] {
        if let Some(path) = loader_server(candidates, kind) {
            return Some(loader_result(base, kind, path, installer_jar));
        }
    }
    if let Some(path) = loader_server(candidates, LoaderKind::Fabric) {
        let mut result = DetectedCore::new(CoreType::Fabric);
        result.minecraft_version = minecraft_version_from_json(base);
        result.server_jar_path = Some(path);
        result.installer_jar = installer_jar;
        return Some(result);
    }

    // 只有安装器时不能再把旁边的通用 JAR 当作 Vanilla 启动。
    for kind in [LoaderKind::Neoforge, LoaderKind::Forge] {
        if let Some(path) = installer_for_kind(candidates, kind) {
            return Some(loader_result(base, kind, path, installer_jar));
        }
    }

    let server_jar = find_server_jar(base)?;
    let mut result = DetectedCore::new(classify_generic_jar(&server_jar));
    result.minecraft_version = minecraft_version_from_json(base);
    result.server_jar_path = Some(server_jar);
    result.installer_jar = installer_jar;
    Some(result)
}

fn loader_result(
    base: &Path,
    kind: LoaderKind,
    jar: PathBuf,
    installer_jar: Option<PathBuf>,
) -> DetectedCore {
    let (minecraft_version, core_version) = parse_loader_versions(&jar, kind);
    let mut result = DetectedCore::new(kind.core_type());
    result.minecraft_version = minecraft_version_from_json(base).or(minecraft_version);
    result.core_version = core_version;
    result.installer_jar = installer_jar;
    if jar_is_installer(&jar) {
        result.installer_jar = Some(jar);
    } else {
        result.server_jar_path = Some(jar);
    }
    result
}

fn loader_candidates(base: &Path) -> Vec<LoaderCandidate> {
    sorted_files(base)
        .into_iter()
        .filter(|path| is_jar(path))
        .filter_map(|path| {
            let lower_name = lower_file_name(&path);
            let content_kind = zip_installer_kind(&path);
            let name_kind = if lower_name.contains("neoforge") {
                Some(LoaderKind::Neoforge)
            } else if lower_name.contains("forge") {
                Some(LoaderKind::Forge)
            } else if lower_name.contains("fabric") {
                Some(LoaderKind::Fabric)
            } else {
                None
            };
            // 内容比文件名可靠：名为 forge.jar 的 NeoForge 安装器仍是 NeoForge。
            let kind = content_kind.or(name_kind)?;
            Some(LoaderCandidate {
                path,
                kind,
                installer: content_kind.is_some() || lower_name.contains("installer"),
            })
        })
        .collect()
}

fn preferred_installer(candidates: &[LoaderCandidate]) -> Option<PathBuf> {
    [LoaderKind::Neoforge, LoaderKind::Forge]
        .into_iter()
        .find_map(|kind| installer_for_kind(candidates, kind))
}

fn installer_for_kind(candidates: &[LoaderCandidate], kind: LoaderKind) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|candidate| candidate.kind == kind && candidate.installer)
        .map(|candidate| candidate.path.clone())
}

fn loader_server(candidates: &[LoaderCandidate], kind: LoaderKind) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|candidate| candidate.kind == kind && !candidate.installer)
        .map(|candidate| candidate.path.clone())
}

/// 只读取 ZIP 条目名，不解压文件内容，也不信任 JAR 的文件名。
fn zip_installer_kind(jar_path: &Path) -> Option<LoaderKind> {
    let file = File::open(jar_path).ok()?;
    let archive = ZipArchive::new(file).ok()?;
    let mut forge = false;
    let mut neoforge = false;

    for entry_name in archive.file_names() {
        let normalized = entry_name.replace('\\', "/");
        if contains_package_marker(&normalized, "neoforged/installer") {
            neoforge = true;
        }
        if contains_package_marker(&normalized, "minecraftforge/installer")
            || contains_package_marker(&normalized, "fml/installer")
        {
            forge = true;
        }
    }

    if neoforge {
        Some(LoaderKind::Neoforge)
    } else if forge {
        Some(LoaderKind::Forge)
    } else {
        None
    }
}

fn contains_package_marker(entry_name: &str, marker: &str) -> bool {
    entry_name.match_indices(marker).any(|(start, _)| {
        let end = start + marker.len();
        let starts_at_component = start == 0 || entry_name.as_bytes()[start - 1] == b'/';
        let ends_at_component =
            end == entry_name.len() || entry_name.as_bytes().get(end) == Some(&b'/');
        starts_at_component && ends_at_component
    })
}

fn core_version_from_libraries(base: &Path, organization: &str, artifact: &str) -> Option<String> {
    sorted_directories(&base.join("libraries/net").join(organization).join(artifact))
        .first()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|version| !version.is_empty())
}

fn find_library_server_jar(
    base: &Path,
    organization: &str,
    artifact: &str,
    version: Option<&str>,
) -> Option<PathBuf> {
    let version_dir = base
        .join("libraries/net")
        .join(organization)
        .join(artifact)
        .join(version?);
    let artifact_with_dash = format!("{artifact}-");
    let artifact_plain = format!("{artifact}.jar");
    let mut candidates: Vec<PathBuf> = sorted_files(&version_dir)
        .into_iter()
        .filter(|path| is_jar(path))
        .filter(|path| {
            let name = lower_file_name(path);
            (name.starts_with(&artifact_with_dash) || name == artifact_plain)
                && !["installer", "sources", "javadoc", "client"]
                    .into_iter()
                    .any(|marker| name.contains(marker))
                && zip_installer_kind(path).is_none()
        })
        .collect();

    // 只在该加载器自身的 Maven 版本目录中选择，绝不兜底到任意 library JAR。
    candidates.sort_by(|left, right| {
        let left_rank = usize::from(!lower_file_name(left).contains("universal"));
        let right_rank = usize::from(!lower_file_name(right).contains("universal"));
        left_rank.cmp(&right_rank).then_with(|| left.cmp(right))
    });
    candidates.into_iter().next()
}

fn find_unix_args_file(base: &Path, marker: &str) -> Option<PathBuf> {
    let libraries = base.join("libraries");
    if !libraries.is_dir() {
        return None;
    }
    let marker = OsStr::new(marker);
    let mut candidates: Vec<PathBuf> = WalkDir::new(&libraries)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !(entry.file_type().is_dir() && entry.file_name() == OsStr::new("mods"))
        })
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.file_name() == Some(OsStr::new("unix_args.txt")))
        .filter(|path| path.components().any(|part| part.as_os_str() == marker))
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

fn minecraft_version_from_json(base: &Path) -> Option<String> {
    let data = fs::read(base.join("version.json")).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&data).ok()?;
    value
        .get("id")?
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

fn parse_loader_versions(jar: &Path, kind: LoaderKind) -> (Option<String>, Option<String>) {
    let Some(prefix) = kind.jar_prefix() else {
        return (None, None);
    };
    let lower_name = lower_file_name(jar);
    let Some(version) = lower_name
        .strip_suffix(".jar")
        .and_then(|name| name.strip_prefix(prefix))
        .and_then(|name| name.strip_prefix('-'))
    else {
        return (None, None);
    };
    let version = ["-universal", "-installer", "-server"]
        .into_iter()
        .find_map(|suffix| version.strip_suffix(suffix))
        .unwrap_or(version);
    if version.is_empty() {
        return (None, None);
    }

    let core_version = Some(version.to_owned());
    // NeoForge 的版本号不是 Minecraft 版本号，不能凭第一个连字符猜测。
    let minecraft_version = (kind == LoaderKind::Forge)
        .then(|| forge_minecraft_version(version))
        .flatten();
    (minecraft_version, core_version)
}

fn forge_minecraft_version(core_version: &str) -> Option<String> {
    core_version
        .split('-')
        .next()
        .filter(|version| {
            version.contains('.')
                && version
                    .chars()
                    .all(|character| character.is_ascii_digit() || character == '.')
        })
        .map(str::to_owned)
}

fn classify_generic_jar(path: &Path) -> CoreType {
    let name = lower_file_name(path);
    if name.contains("paper") {
        CoreType::Paper
    } else if name.contains("spigot") {
        CoreType::Spigot
    } else if name == "server.jar"
        || name.contains("vanilla")
        || name.starts_with("minecraft_server")
        || name.starts_with("minecraft-server")
    {
        CoreType::Vanilla
    } else {
        CoreType::Custom
    }
}

/// 通用 JAR 的兜底顺序固定为：顶层 server 名称、顶层其他 JAR、递归 server
/// 名称、递归其他 JAR。每一组内部均按路径排序，不受目录遍历顺序影响。
fn find_server_jar(base: &Path) -> Option<PathBuf> {
    let top_level: Vec<PathBuf> = sorted_files(base)
        .into_iter()
        .filter(|path| launchable_generic_jar(path))
        .collect();
    if let Some(path) = top_level
        .iter()
        .find(|path| lower_file_name(path).contains("server"))
    {
        return Some(path.clone());
    }
    if let Some(path) = top_level.first() {
        return Some(path.clone());
    }

    let mut nested: Vec<PathBuf> = WalkDir::new(base)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !(entry.file_type().is_dir()
                    && matches!(entry.file_name().to_str(), Some("mods" | "libraries")))
        })
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.depth() > 1 && entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| launchable_generic_jar(path))
        .collect();
    nested.sort();

    // mods 与 libraries 已整棵跳过，避免 mod 或依赖库抢占服务端核心。
    nested
        .iter()
        .find(|path| lower_file_name(path).contains("server"))
        .cloned()
        .or_else(|| nested.into_iter().next())
}

fn launchable_generic_jar(path: &Path) -> bool {
    if !path.is_file() || !is_jar(path) {
        return false;
    }
    let name = lower_file_name(path);
    !["installer", "sources", "javadoc"]
        .into_iter()
        .any(|marker| name.contains(marker))
        && zip_installer_kind(path).is_none()
}

fn sorted_files(directory: &Path) -> Vec<PathBuf> {
    sorted_entries(directory, false)
}

fn sorted_directories(directory: &Path) -> Vec<PathBuf> {
    sorted_entries(directory, true)
}

fn sorted_entries(directory: &Path, directories: bool) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            if directories {
                path.is_dir()
            } else {
                path.is_file()
            }
        })
        .collect();
    paths.sort();
    paths
}

fn is_jar(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
}

fn lower_file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::TempDir;
    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;

    fn write_jar(path: &Path, entries: &[&str]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let file = File::create(path).unwrap();
        let mut writer = ZipWriter::new(file);
        for entry in entries {
            writer
                .start_file(*entry, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"test").unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn renamed_forge_installer_is_found_by_zip_entries() {
        let temp = TempDir::new().unwrap();
        let installer = temp.path().join("forge.jar");
        write_jar(
            &installer,
            &["net/minecraftforge/installer/Installer.class"],
        );
        // 通用 JAR 不能掩盖“核心尚未安装”的事实。
        write_jar(&temp.path().join("server.jar"), &[]);
        let legacy = temp.path().join("unused/legacy.jar");
        write_jar(&legacy, &["cpw/mods/fml/installer/Installer.class"]);

        assert!(jar_is_installer(&installer));
        assert!(jar_is_installer(&legacy));
        assert_eq!(find_installer_jar(temp.path()), Some(installer.clone()));
        let detected = detect_directory(temp.path()).unwrap();
        assert_eq!(detected.core_type, CoreType::Forge);
        assert_eq!(detected.server_jar_path, None);
        assert_eq!(detected.installer_jar, Some(installer.clone()));

        let mut server = ServerConfig::new("Forge", temp.path().to_path_buf());
        server.core_type = CoreType::Forge;
        server.server_jar_path = PathBuf::from("forge.jar");
        assert!(needs_installer(&server));
    }

    #[test]
    fn renamed_neoforge_installer_has_priority_over_forge() {
        let temp = TempDir::new().unwrap();
        let neoforge = temp.path().join("forge.jar");
        write_jar(&neoforge, &["net/neoforged/installer/Installer.class"]);
        write_jar(
            &temp.path().join("forge-1.20.1-47.2.0-installer.jar"),
            &["net/minecraftforge/installer/Installer.class"],
        );

        assert!(jar_is_installer(&neoforge));
        assert_eq!(find_installer_jar(temp.path()), Some(neoforge.clone()));
        let detected = detect_directory(temp.path()).unwrap();
        assert_eq!(detected.core_type, CoreType::Neoforge);
        assert_eq!(detected.server_jar_path, None);
        assert_eq!(detected.installer_jar, Some(neoforge));
        assert_eq!(detected.minecraft_version, None);
        assert_eq!(detected.core_version, None);
    }

    #[test]
    fn fabric_top_level_jar_and_forge_launch_args_are_detected() {
        let fabric = TempDir::new().unwrap();
        write_jar(&fabric.path().join("server.jar"), &[]);
        let fabric_jar = fabric.path().join("fabric-server-launch.jar");
        write_jar(&fabric_jar, &[]);
        fs::write(fabric.path().join("version.json"), r#"{"id":"1.21.1"}"#).unwrap();

        let detected = detect_directory(fabric.path()).unwrap();
        assert_eq!(detected.core_type, CoreType::Fabric);
        assert_eq!(detected.server_jar_path, Some(fabric_jar));
        assert_eq!(detected.minecraft_version.as_deref(), Some("1.21.1"));

        let forge = TempDir::new().unwrap();
        let version_dir = forge
            .path()
            .join("libraries/net/minecraftforge/forge/1.20.1-47.2.0");
        let universal = version_dir.join("forge-1.20.1-47.2.0-universal.jar");
        write_jar(&universal, &[]);
        let args = version_dir.join("unix_args.txt");
        fs::write(&args, "-p libraries\n").unwrap();
        fs::write(forge.path().join("version.json"), r#"{"id":"1.20.1"}"#).unwrap();
        write_jar(&forge.path().join("a-server.jar"), &[]);

        let detected = detect_directory(forge.path()).unwrap();
        assert_eq!(detected.core_type, CoreType::Forge);
        assert_eq!(detected.server_jar_path, Some(universal));
        assert_eq!(detected.launch_args_file, Some(args.clone()));
        assert_eq!(detected.core_version.as_deref(), Some("1.20.1-47.2.0"));
        assert_eq!(detected.minecraft_version.as_deref(), Some("1.20.1"));

        let mut server = ServerConfig::new("Forge", forge.path().to_path_buf());
        server.core_type = CoreType::Forge;
        server.launch_args_file = args;
        assert!(!needs_installer(&server));
    }

    #[test]
    fn candidates_are_sorted_and_library_jars_are_not_generic_cores() {
        let temp = TempDir::new().unwrap();
        // 故意以相反顺序创建，结果仍应按文件名排序。
        write_jar(&temp.path().join("z-server.jar"), &[]);
        let expected = temp.path().join("a-server.jar");
        write_jar(&expected, &[]);

        for _ in 0..3 {
            let detected = detect_directory(temp.path()).unwrap();
            assert_eq!(detected.server_jar_path, Some(expected.clone()));
        }

        let libraries_only = TempDir::new().unwrap();
        write_jar(
            &libraries_only
                .path()
                .join("libraries/com/example/dependency.jar"),
            &[],
        );
        write_jar(&libraries_only.path().join("mods/example-mod.jar"), &[]);
        assert!(detect_directory(libraries_only.path()).is_none());
    }

    #[test]
    fn unknown_top_level_jar_remains_a_custom_core() {
        let temp = TempDir::new().unwrap();
        let custom = temp.path().join("my-core.jar");
        write_jar(&custom, &[]);

        let detected = detect_directory(temp.path()).unwrap();
        assert_eq!(detected.core_type, CoreType::Custom);
        assert_eq!(detected.server_jar_path, Some(custom));
        assert_eq!(detected.minecraft_version, None);
        assert_eq!(detected.core_version, None);
        assert!(!needs_installer(&ServerConfig::new(
            "自定义",
            temp.path().to_path_buf(),
        )));
    }
}
