use std::{
    env, fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

use serde_json::Value;

use crate::{
    config::{AppConfig, CoreType},
    error::{AppError, Result},
};

#[derive(Debug)]
pub struct LoadedConfig {
    pub config: AppConfig,
    pub existed: bool,
}

/// 配置位置；保存的路径一律是绝对路径，不依赖当前工作目录。
#[derive(Clone, Debug)]
pub struct ConfigStore {
    config_path: PathBuf,
    data_dir: PathBuf,
}

impl ConfigStore {
    pub fn new() -> Result<Self> {
        Ok(Self {
            config_path: config_dir_from_env()?.join("config.json"),
            data_dir: data_dir_from_env()?,
        })
    }

    /// 为隔离测试和嵌入调用显式指定位置，避免读取真实用户配置。
    pub fn with_paths(config_path: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            config_path,
            data_dir,
        }
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn config_dir(&self) -> &Path {
        self.config_path.parent().unwrap_or(Path::new("."))
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn pid_path(&self) -> PathBuf {
        self.config_dir().join("web.pid")
    }

    pub fn log_path(&self) -> PathBuf {
        self.config_dir().join("web.log")
    }

    pub fn fresh_default_server_dir(&self) -> PathBuf {
        self.data_dir.join("servers")
    }

    pub fn load(&self) -> Result<LoadedConfig> {
        let bytes = match fs::read(&self.config_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LoadedConfig {
                    config: AppConfig::fresh(self.fresh_default_server_dir()),
                    existed: false,
                });
            }
            Err(error) => {
                return Err(AppError::io(
                    format!("无法读取配置 {}", self.config_path.display()),
                    error,
                ));
            }
        };

        // 读取失败不能退化成空配置，否则下一次保存会覆盖用户全部服务器。
        let mut raw: Value = serde_json::from_slice(&bytes).map_err(|error| {
            AppError::invalid(
                "config_invalid",
                format!("配置 {} 无法解析: {error}", self.config_path.display()),
            )
        })?;
        if !raw.is_object() {
            return Err(AppError::invalid(
                "config_invalid",
                "配置根节点必须是 JSON 对象",
            ));
        }

        if let Some(servers) = raw.get_mut("servers").and_then(Value::as_array_mut) {
            for server in servers {
                if let Some(object) = server.as_object_mut() {
                    let known_core = object
                        .get("core_type")
                        .and_then(Value::as_str)
                        .map(|id| CoreType::from_id(id).id() == id)
                        .unwrap_or(true);
                    if !known_core {
                        object.insert("core_type".into(), Value::String("custom".into()));
                    }
                }
            }
        }

        let configured_dir = raw
            .get("default_server_dir")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let mut config: AppConfig = serde_json::from_value(raw).map_err(|error| {
            AppError::invalid("config_invalid", format!("配置字段无效: {error}"))
        })?;
        config.schema_version = 2;
        config.default_server_dir = match configured_dir {
            Some(path) => normalize_absolute(&path, &self.data_dir),
            None => self.fresh_default_server_dir(),
        };

        for server in &mut config.servers {
            if !server.working_dir.is_absolute() {
                return Err(AppError::invalid(
                    "config_invalid",
                    format!("服务器「{}」的 working_dir 必须是绝对路径", server.name),
                ));
            }
            server.working_dir = normalize_lexical(&server.working_dir);
        }

        if config
            .active_server_id
            .is_some_and(|id| config.server(id).is_none())
        {
            config.active_server_id = config.servers.first().map(|server| server.id);
        }

        Ok(LoadedConfig {
            config,
            existed: true,
        })
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        let parent = self.config_dir();
        fs::create_dir_all(parent).map_err(|error| {
            AppError::io(format!("无法创建配置目录 {}", parent.display()), error)
        })?;

        let mut output = config.clone();
        output.schema_version = 2;
        let data = serde_json::to_vec_pretty(&output)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|error| AppError::io("无法创建配置临时文件", error))?;
        temporary
            .write_all(&data)
            .and_then(|()| temporary.write_all(b"\n"))
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|error| AppError::io("无法写入配置临时文件", error))?;

        if self.config_path.exists() {
            let backup = self.config_path.with_extension("json.bak");
            fs::copy(&self.config_path, &backup)
                .map_err(|error| AppError::io("无法备份旧配置", error))?;
        }
        temporary
            .persist(&self.config_path)
            .map_err(|error| AppError::io("无法原子替换配置", error.error))?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| AppError::io("无法同步配置目录", error))?;
        Ok(())
    }
}

fn normalize_absolute(path: &Path, base: &Path) -> PathBuf {
    if path.is_absolute() {
        normalize_lexical(path)
    } else {
        normalize_lexical(&base.join(path))
    }
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() && !path.is_absolute() {
                    result.push("..");
                }
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

fn config_dir_from_env() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path).join("MCServerManager"));
    }
    let home = env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::invalid("config_unavailable", "HOME 与 XDG_CONFIG_HOME 均不可用")
        })?;
    Ok(PathBuf::from(home).join(".config/MCServerManager"))
}

fn data_dir_from_env() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path).join("MCServerManager"));
    }
    let home = env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::invalid("config_unavailable", "HOME 与 XDG_DATA_HOME 均不可用"))?;
    Ok(PathBuf::from(home).join(".local/share/MCServerManager"))
}

#[cfg(test)]
mod tests {
    use std::{fs, str::FromStr};

    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::config::{ServerConfig, ServerId};

    const FIRST_ID: &str = "123e4567-e89b-42d3-a456-426614174000";
    const SECOND_ID: &str = "223e4567-e89b-42d3-a456-426614174001";

    fn store(temp: &TempDir) -> ConfigStore {
        let root = temp.path();
        ConfigStore::with_paths(
            root.join("config/config.json"),
            root.join("data/MCServerManager"),
        )
    }

    fn write_config(store: &ConfigStore, value: serde_json::Value) {
        fs::create_dir_all(store.config_dir()).unwrap();
        fs::write(store.config_path(), serde_json::to_vec(&value).unwrap()).unwrap();
    }

    fn server(id: &str, name: &str, working_dir: &str) -> serde_json::Value {
        json!({
            "id": id,
            "name": name,
            "working_dir": working_dir,
            "server_jar_path": "server.jar",
        })
    }

    #[test]
    fn fresh_config_uses_user_data_dir() {
        let temp = TempDir::new().unwrap();
        let store = store(&temp);
        let loaded = store.load().unwrap();
        assert!(!loaded.existed);
        assert_eq!(
            loaded.config.default_server_dir,
            store.data_dir().join("servers")
        );
    }

    #[test]
    fn relative_default_dir_is_resolved_against_user_data_dir() {
        let temp = TempDir::new().unwrap();
        let store = store(&temp);
        write_config(&store, json!({"default_server_dir": "instances/./new"}));
        let loaded = store.load().unwrap();
        assert_eq!(
            loaded.config.default_server_dir,
            store.data_dir().join("instances/new")
        );
    }

    #[test]
    fn unknown_active_server_falls_back_to_the_first_one() {
        let temp = TempDir::new().unwrap();
        let store = store(&temp);
        write_config(
            &store,
            json!({
                "active_server_id": "00000000-0000-0000-0000-000000000000",
                "servers": [
                    server(FIRST_ID, "甲", "/srv/one"),
                    server(SECOND_ID, "乙", "/srv/two")
                ]
            }),
        );
        let loaded = store.load().unwrap();
        assert_eq!(loaded.config.servers.len(), 2);
        assert_eq!(
            loaded.config.active_server_id,
            Some(ServerId::from_str(FIRST_ID).unwrap())
        );
    }

    #[test]
    fn unknown_core_type_loads_as_custom() {
        let temp = TempDir::new().unwrap();
        let store = store(&temp);
        write_config(
            &store,
            json!({"servers": [{
                "id": FIRST_ID,
                "name": "甲",
                "working_dir": "/srv/one",
                "server_jar_path": "server.jar",
                "core_type": "不认识的类型"
            }]}),
        );
        let loaded = store.load().unwrap();
        assert_eq!(loaded.config.servers[0].core_type, CoreType::Custom);
    }

    #[test]
    fn server_without_id_or_absolute_working_dir_is_rejected_and_file_is_kept() {
        let temp = TempDir::new().unwrap();
        let store = store(&temp);
        let base = server(FIRST_ID, "甲", "/srv/one");
        let mut without_id = base.clone();
        without_id.as_object_mut().unwrap().remove("id");
        let mut relative_dir = base.clone();
        relative_dir["working_dir"] = json!("server one");
        let mut without_dir = base;
        without_dir.as_object_mut().unwrap().remove("working_dir");

        for server in [without_id, relative_dir, without_dir] {
            write_config(&store, json!({"servers": [server]}));
            let before = fs::read(store.config_path()).unwrap();
            let error = store.load().unwrap_err();
            assert_eq!(error.code, "config_invalid");
            assert_eq!(fs::read(store.config_path()).unwrap(), before);
        }
    }

    #[test]
    fn damaged_config_is_not_replaced_by_empty_config() {
        let temp = TempDir::new().unwrap();
        let store = store(&temp);
        fs::create_dir_all(store.config_dir()).unwrap();
        fs::write(store.config_path(), b"{broken").unwrap();
        let error = store.load().unwrap_err();
        assert_eq!(error.code, "config_invalid");
        assert_eq!(fs::read(store.config_path()).unwrap(), b"{broken");
    }

    #[test]
    fn saved_config_reloads_with_the_same_ids_and_active_server() {
        let temp = TempDir::new().unwrap();
        let store = store(&temp);
        let mut config = AppConfig::fresh(PathBuf::from("/srv/new-servers"));
        let first = ServerConfig::new("外置服", PathBuf::from("/mnt/minecraft/server"));
        let second = ServerConfig::new("第二个", PathBuf::from("/mnt/minecraft/two/../two"));
        config.servers = vec![first.clone(), second.clone()];
        config.active_server_id = Some(second.id);
        store.save(&config).unwrap();

        let raw: Value = serde_json::from_slice(&fs::read(store.config_path()).unwrap()).unwrap();
        assert_eq!(raw["schema_version"], 2);
        assert_eq!(raw["default_server_dir"], "/srv/new-servers");

        let loaded = store.load().unwrap();
        let servers = &loaded.config.servers;
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].id, first.id);
        assert_eq!(servers[1].id, second.id);
        assert_eq!(servers[1].working_dir, PathBuf::from("/mnt/minecraft/two"));
        assert_eq!(
            servers[0].world_dir(),
            PathBuf::from("/mnt/minecraft/server/world")
        );
        assert_eq!(loaded.config.active_server_id, Some(second.id));
        assert_eq!(
            loaded.config.default_server_dir,
            PathBuf::from("/srv/new-servers")
        );
    }
}
