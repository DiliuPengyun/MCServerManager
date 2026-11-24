use std::{
    fmt::{Display, Formatter},
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ServerId(Uuid);

impl ServerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn simple(self) -> String {
        self.0.simple().to_string()
    }
}

impl Default for ServerId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for ServerId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl FromStr for ServerId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoreType {
    Vanilla,
    Paper,
    Spigot,
    Forge,
    Neoforge,
    Fabric,
    #[default]
    Custom,
}

impl CoreType {
    pub const ALL: [Self; 7] = [
        Self::Vanilla,
        Self::Paper,
        Self::Spigot,
        Self::Forge,
        Self::Neoforge,
        Self::Fabric,
        Self::Custom,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Paper => "paper",
            Self::Spigot => "spigot",
            Self::Forge => "forge",
            Self::Neoforge => "neoforge",
            Self::Fabric => "fabric",
            Self::Custom => "custom",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Paper => "Paper",
            Self::Spigot => "Spigot",
            Self::Forge => "Forge",
            Self::Neoforge => "NeoForge",
            Self::Fabric => "Fabric",
            Self::Custom => "自定义",
        }
    }

    pub fn uses_installer(self) -> bool {
        matches!(self, Self::Forge | Self::Neoforge)
    }

    pub fn uses_launch_args_file(self) -> bool {
        self.uses_installer()
    }

    pub fn from_id(value: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|kind| kind.id() == value)
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    pub id: ServerId,
    pub name: String,
    #[serde(default)]
    pub server_jar_path: PathBuf,
    #[serde(default = "default_java")]
    pub java_path: String,
    pub working_dir: PathBuf,
    #[serde(default = "default_min_memory")]
    pub min_memory_mb: i32,
    #[serde(default = "default_max_memory")]
    pub max_memory_mb: i32,
    #[serde(default)]
    pub additional_jvm_args: String,
    #[serde(default = "default_backup_dir")]
    pub backup_dir: PathBuf,
    #[serde(default = "default_world_dir")]
    pub world_dir: PathBuf,
    #[serde(default)]
    pub core_type: CoreType,
    #[serde(default)]
    pub minecraft_version: String,
    #[serde(default)]
    pub core_version: String,
    #[serde(default)]
    pub launch_args_file: PathBuf,
    /// 0 表示未配置 RCON。
    #[serde(default)]
    pub rcon_port: u16,
    #[serde(default)]
    pub rcon_password: String,
}

impl ServerConfig {
    pub fn new(name: impl Into<String>, working_dir: PathBuf) -> Self {
        Self {
            id: ServerId::new(),
            name: name.into(),
            server_jar_path: PathBuf::new(),
            java_path: default_java(),
            working_dir,
            min_memory_mb: default_min_memory(),
            max_memory_mb: default_max_memory(),
            additional_jvm_args: String::new(),
            backup_dir: default_backup_dir(),
            world_dir: default_world_dir(),
            core_type: CoreType::Custom,
            minecraft_version: String::new(),
            core_version: String::new(),
            launch_args_file: PathBuf::new(),
            rcon_port: 0,
            rcon_password: String::new(),
        }
    }

    pub fn resolve_from_working_dir(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.working_dir.join(path)
        }
    }

    pub fn jar_path(&self) -> Option<PathBuf> {
        (!self.server_jar_path.as_os_str().is_empty())
            .then(|| self.resolve_from_working_dir(&self.server_jar_path))
    }

    pub fn launch_args_path(&self) -> Option<PathBuf> {
        (!self.launch_args_file.as_os_str().is_empty())
            .then(|| self.resolve_from_working_dir(&self.launch_args_file))
    }

    pub fn world_dir(&self) -> PathBuf {
        self.resolve_from_working_dir(&self.world_dir)
    }

    pub fn backup_dir(&self) -> PathBuf {
        self.resolve_from_working_dir(&self.backup_dir)
    }

    pub fn rcon_available(&self) -> bool {
        self.rcon_port != 0 && !self.rcon_password.is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// 用于新建和上传服务器；不改写已有服务器的路径。
    #[serde(default)]
    pub default_server_dir: PathBuf,
    #[serde(default = "default_java")]
    pub global_java_path: String,
    #[serde(default)]
    pub servers: Vec<ServerConfig>,
    #[serde(default)]
    pub active_server_id: Option<ServerId>,
    #[serde(default = "default_web_port")]
    pub web_port: u16,
}

impl AppConfig {
    pub fn fresh(default_server_dir: PathBuf) -> Self {
        Self {
            schema_version: default_schema_version(),
            default_server_dir,
            global_java_path: default_java(),
            servers: Vec::new(),
            active_server_id: None,
            web_port: default_web_port(),
        }
    }

    pub fn server(&self, id: ServerId) -> Option<&ServerConfig> {
        self.servers.iter().find(|server| server.id == id)
    }

    pub fn server_mut(&mut self, id: ServerId) -> Option<&mut ServerConfig> {
        self.servers.iter_mut().find(|server| server.id == id)
    }

    pub fn server_at(&self, index: usize) -> Option<&ServerConfig> {
        self.servers.get(index)
    }

    pub fn index_of(&self, id: ServerId) -> Option<usize> {
        self.servers.iter().position(|server| server.id == id)
    }

    pub fn select(&mut self, id: ServerId) -> bool {
        if self.server(id).is_none() {
            return false;
        }
        self.active_server_id = Some(id);
        true
    }

    pub fn effective_java_path<'a>(&'a self, server: &'a ServerConfig) -> &'a str {
        if server.java_path.is_empty() || server.java_path == "java" {
            if !self.global_java_path.is_empty() {
                return &self.global_java_path;
            }
            "java"
        } else {
            &server.java_path
        }
    }
}

fn default_schema_version() -> u32 {
    2
}

fn default_java() -> String {
    "java".to_owned()
}

fn default_min_memory() -> i32 {
    1024
}

fn default_max_memory() -> i32 {
    4096
}

fn default_backup_dir() -> PathBuf {
    PathBuf::from("./backups")
}

fn default_world_dir() -> PathBuf {
    PathBuf::from("./world")
}

fn default_web_port() -> u16 {
    8080
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_type_ids_match_existing_api() {
        for kind in CoreType::ALL {
            assert_eq!(CoreType::from_id(kind.id()), kind);
        }
        assert_eq!(CoreType::from_id("未来核心"), CoreType::Custom);
        assert_eq!(CoreType::Neoforge.id(), "neoforge");
    }

    #[test]
    fn server_paths_are_independent_from_process_cwd() {
        let server = ServerConfig::new("测试", PathBuf::from("/srv/minecraft/one"));
        assert_eq!(
            server.world_dir(),
            PathBuf::from("/srv/minecraft/one/world")
        );
        assert_eq!(
            server.backup_dir(),
            PathBuf::from("/srv/minecraft/one/backups")
        );
        assert_eq!(
            ServerConfig::new("另一个", PathBuf::from("/mnt/other")).world_dir(),
            PathBuf::from("/mnt/other/world")
        );
    }
}
