use std::{
    fmt::{self, Display, Formatter},
    fs,
    io::Write,
    ops::Range,
    path::Path,
};

use tempfile::NamedTempFile;

use crate::error::{AppError, Result};

/// `server.properties` 中读取到的 RCON 状态。
/// 配置不完整或端口非法时仍保留原始状态，但 `is_available` 为 false。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RconStatus {
    pub enabled: bool,
    pub port: Option<u16>,
    pub password: String,
}

impl RconStatus {
    pub fn is_available(&self) -> bool {
        self.enabled && self.port.is_some() && !self.password.is_empty()
    }

    /// 只有状态完整可用时才返回连接所需凭据。
    pub fn credentials(&self) -> Option<(u16, &str)> {
        self.is_available().then(|| {
            (
                self.port.expect("可用状态已检查端口"),
                self.password.as_str(),
            )
        })
    }
}

/// 保留原始行、注释、空白、顺序与换行符的 properties 文档。
/// 查找遵循 Java properties 的“最后一个同名键生效”规则。
#[derive(Clone, Debug)]
pub struct ServerProperties {
    lines: Vec<PropertyLine>,
    default_line_ending: String,
}

#[derive(Clone, Debug)]
struct PropertyLine {
    /// 不含换行符的原始内容。
    raw: String,
    /// 原始换行符，可能是 LF、CRLF、CR 或空字符串。
    ending: String,
    entry: Option<Entry>,
}

#[derive(Clone, Debug)]
struct Entry {
    key: String,
    /// 值在 `raw` 中的字节区间，周围的布局空白不在其中。
    value: Range<usize>,
    /// 原行是否已有分隔符；只有键的行在首次设值时需要补 `=`。
    has_separator: bool,
}

impl Default for ServerProperties {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            default_line_ending: "\n".to_owned(),
        }
    }
}

impl ServerProperties {
    /// 从 UTF-8 文本解析，不重新排版任何原始行。
    pub fn parse(source: &str) -> Self {
        let mut lines = split_lines(source);
        let default_line_ending = lines
            .iter()
            .find_map(|line| (!line.ending.is_empty()).then(|| line.ending.clone()))
            .unwrap_or_else(|| "\n".to_owned());
        for line in &mut lines {
            line.entry = parse_entry(&line.raw);
        }
        Self {
            lines,
            default_line_ending,
        }
    }

    /// 读取 UTF-8 properties 文件。无效 UTF-8 不会被静默替换，以免保存时破坏内容。
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = fs::read(path)
            .map_err(|error| AppError::io(format!("无法读取 {}", path.display()), error))?;
        let source = String::from_utf8(bytes).map_err(|error| {
            AppError::invalid(
                "properties_invalid",
                format!("{} 不是有效的 UTF-8: {error}", path.display()),
            )
        })?;
        Ok(Self::parse(&source))
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.lines.iter().rev().find_map(|line| {
            let entry = line.entry.as_ref()?;
            (entry.key == key).then(|| &line.raw[entry.value.clone()])
        })
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// 修改最后一个同名键；不存在时在文档末尾追加，不移动注释或其他键。
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        validate_key(key)?;
        validate_value(value)?;

        if let Some(index) = self
            .lines
            .iter()
            .rposition(|line| line.entry.as_ref().is_some_and(|entry| entry.key == key))
        {
            let line = &mut self.lines[index];
            let entry = line.entry.as_mut().expect("定位到的行必须包含配置项");
            if entry.has_separator {
                let start = entry.value.start;
                line.raw.replace_range(entry.value.clone(), value);
                entry.value = start..start + value.len();
            } else {
                let start = entry.value.start;
                line.raw.insert(start, '=');
                line.raw.insert_str(start + 1, value);
                entry.value = start + 1..start + 1 + value.len();
                entry.has_separator = true;
            }
            return Ok(());
        }

        // 新增行需要与前行分隔；前行内容本身保持原样。
        if let Some(last) = self.lines.last_mut()
            && last.ending.is_empty()
        {
            last.ending = self.default_line_ending.clone();
        }
        let mut line = PropertyLine {
            raw: format!("{key}={value}"),
            ending: self.default_line_ending.clone(),
            entry: None,
        };
        line.entry = parse_entry(&line.raw);
        self.lines.push(line);
        Ok(())
    }

    /// 按文档原顺序返回键；重复键也会保留，便于界面呈现原始布局。
    pub fn ordered_keys(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter_map(|line| line.entry.as_ref().map(|entry| entry.key.as_str()))
            .collect()
    }

    pub fn entries(&self) -> Vec<(&str, &str)> {
        self.lines
            .iter()
            .filter_map(|line| {
                let entry = line.entry.as_ref()?;
                Some((entry.key.as_str(), &line.raw[entry.value.clone()]))
            })
            .collect()
    }

    /// 读取 RCON 状态，不修改文档或磁盘。缺省 `rcon.port` 按 vanilla 的 25575 处理。
    pub fn rcon_status(&self) -> RconStatus {
        let enabled = self
            .get("enable-rcon")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"));
        let port = match self.get("rcon.port") {
            Some(value) => value.parse::<u16>().ok().filter(|port| *port != 0),
            None => Some(25575),
        };
        let password = self.get("rcon.password").unwrap_or_default().to_owned();
        RconStatus {
            enabled,
            port,
            password,
        }
    }

    /// 显式设置启用 RCON 所需的三个配置项，仅修改当前内存文档。
    /// 本模块绝不会自行调用此方法；调用方仍须显式 `save` 才会写盘。
    pub fn enable_rcon(&mut self, port: u16, password: &str) -> Result<()> {
        if port == 0 {
            return Err(AppError::invalid(
                "invalid_rcon_config",
                "RCON 端口必须在 1 到 65535 之间",
            ));
        }
        if password.trim().is_empty()
            || password
                .as_bytes()
                .iter()
                .any(|&byte| matches!(byte, b'\r' | b'\n' | 0))
        {
            return Err(AppError::invalid(
                "invalid_rcon_config",
                "RCON 密码不能为空、换行或包含 NUL 字符",
            ));
        }

        self.set("enable-rcon", "true")?;
        self.set("rcon.port", &port.to_string())?;
        self.set("rcon.password", password)?;
        Ok(())
    }

    /// 在目标目录内写入并同步临时文件，再通过重命名原子替换目标。
    /// 已存在文件的权限会复制到临时文件，替换后同步父目录。
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() || path.file_name().is_none() {
            return Err(AppError::invalid(
                "invalid_property_path",
                "properties 保存路径必须指向文件",
            ));
        }
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| {
            AppError::io(
                format!("无法创建 properties 目录 {}", parent.display()),
                error,
            )
        })?;

        let permissions = fs::metadata(path)
            .ok()
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.permissions());
        let mut temporary = NamedTempFile::new_in(parent).map_err(|error| {
            AppError::io(format!("无法在 {} 创建临时文件", parent.display()), error)
        })?;
        if let Some(permissions) = permissions {
            temporary
                .as_file()
                .set_permissions(permissions)
                .map_err(|error| AppError::io("无法保留 properties 文件权限", error))?;
        }
        self.write_to(&mut temporary)
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|error| AppError::io("无法写入 properties 临时文件", error))?;
        temporary.persist(path).map_err(|error| {
            AppError::io(format!("无法原子替换 {}", path.display()), error.error)
        })?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| AppError::io("无法同步 properties 目录", error))?;
        Ok(())
    }

    fn write_to(&self, output: &mut impl Write) -> std::io::Result<()> {
        for line in &self.lines {
            output.write_all(line.raw.as_bytes())?;
            output.write_all(line.ending.as_bytes())?;
        }
        Ok(())
    }
}

impl Display for ServerProperties {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        for line in &self.lines {
            formatter.write_str(&line.raw)?;
            formatter.write_str(&line.ending)?;
        }
        Ok(())
    }
}

fn split_lines(source: &str) -> Vec<PropertyLine> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut characters = source.char_indices().peekable();
    while let Some((index, character)) = characters.next() {
        let ending_end = match character {
            '\n' => index + 1,
            '\r' => {
                if characters.peek().is_some_and(|(_, next)| *next == '\n') {
                    characters.next();
                    index + 2
                } else {
                    index + 1
                }
            }
            _ => continue,
        };
        lines.push(PropertyLine {
            raw: source[start..index].to_owned(),
            ending: source[index..ending_end].to_owned(),
            entry: None,
        });
        start = ending_end;
    }
    if start < source.len() {
        lines.push(PropertyLine {
            raw: source[start..].to_owned(),
            ending: String::new(),
            entry: None,
        });
    }
    lines
}

fn parse_entry(raw: &str) -> Option<Entry> {
    let key_start = raw.len() - raw.trim_start().len();
    let first = raw[key_start..].chars().next()?;
    if matches!(first, '#' | '!') {
        return None;
    }

    let mut escaped = false;
    let mut separator = None;
    for (relative, character) in raw[key_start..].char_indices() {
        let index = key_start + relative;
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if matches!(character, '=' | ':') || character.is_whitespace() {
            separator = Some((index, character));
            break;
        }
    }

    let Some((key_end, first_separator)) = separator else {
        let key = raw[key_start..].to_owned();
        return (!key.is_empty()).then_some(Entry {
            key,
            value: raw.len()..raw.len(),
            has_separator: false,
        });
    };
    if key_end == key_start {
        return None;
    }

    let mut value_start = key_end;
    if first_separator.is_whitespace() {
        value_start = skip_whitespace(raw, value_start);
    }
    if raw[value_start..]
        .chars()
        .next()
        .is_some_and(|character| matches!(character, '=' | ':'))
    {
        value_start += raw[value_start..].chars().next()?.len_utf8();
    } else if !first_separator.is_whitespace() {
        // 前面的分支必然命中显式等号或冒号，此处仅作防御。
        return None;
    }
    value_start = skip_whitespace(raw, value_start);

    // 忽略值两侧的布局空白；等号、冒号和 # 均可是值的一部分。
    let value_end = raw.trim_end().len();
    let value = if value_start < value_end {
        value_start..value_end
    } else {
        value_start..value_start
    };
    Some(Entry {
        key: raw[key_start..key_end].to_owned(),
        value,
        has_separator: true,
    })
}

fn skip_whitespace(raw: &str, mut index: usize) -> usize {
    while let Some(character) = raw[index..].chars().next() {
        if !character.is_whitespace() {
            break;
        }
        index += character.len_utf8();
    }
    index
}

fn validate_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.trim() != key
        || key.starts_with(['#', '!'])
        || key
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '=' | ':' | '\0'))
    {
        return Err(AppError::invalid(
            "invalid_property",
            "properties 键不能为空、不能以 # 或 ! 开头，也不能包含空白、等号、冒号或 NUL 字符",
        ));
    }
    Ok(())
}

fn validate_value(value: &str) -> Result<()> {
    if value
        .as_bytes()
        .iter()
        .any(|&byte| matches!(byte, b'\r' | b'\n' | 0))
    {
        return Err(AppError::invalid(
            "invalid_property",
            "properties 值不能包含换行或 NUL 字符",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn parses_first_equals_unicode_and_comments_without_relayout() {
        let source = "# 中文注释\r\nmotd = 你好=A=B  \r\n! 保留注释\r\nurl=https://example.test/?a=b=c\r\n欢迎=进入:世界#1\r\nempty=\r\nstandalone\r\n";
        let properties = ServerProperties::parse(source);
        assert_eq!(properties.get("motd"), Some("你好=A=B"));
        assert_eq!(properties.get("url"), Some("https://example.test/?a=b=c"));
        assert_eq!(properties.get("欢迎"), Some("进入:世界#1"));
        assert_eq!(properties.get("empty"), Some(""));
        assert_eq!(properties.get("standalone"), Some(""));
        assert_eq!(properties.get("missing"), None);
        assert_eq!(properties.to_string(), source);
    }

    #[test]
    fn edits_values_in_place_and_preserves_order_comments_and_crlf() {
        let source = "# 顶部\r\nmotd = old  \r\nenable-rcon=false\r\n\r\n# 尾部\r\n";
        let mut properties = ServerProperties::parse(source);
        properties.set("motd", "新=A=B").unwrap();
        properties.set("view-distance", "12").unwrap();
        assert_eq!(
            properties.to_string(),
            "# 顶部\r\nmotd = 新=A=B  \r\nenable-rcon=false\r\n\r\n# 尾部\r\nview-distance=12\r\n"
        );
        assert_eq!(
            properties.ordered_keys(),
            vec!["motd", "enable-rcon", "view-distance"]
        );
    }

    #[test]
    fn duplicate_keys_use_last_value_and_only_edit_that_line() {
        let source = "motd=first\n# 中间\nmotd = second\n";
        let mut properties = ServerProperties::parse(source);
        assert_eq!(properties.get("motd"), Some("second"));
        properties.set("motd", "最后").unwrap();
        assert_eq!(properties.to_string(), "motd=first\n# 中间\nmotd = 最后\n");
        properties.set("standalone", "now=value").unwrap();
        assert!(properties.to_string().ends_with("standalone=now=value\n"));
    }

    #[test]
    fn rcon_status_distinguishes_disabled_ready_and_incomplete() {
        let mut properties = ServerProperties::parse("enable-rcon=true\nrcon.password=秘密\n");
        let ready = properties.rcon_status();
        assert_eq!(ready.port, Some(25575));
        assert_eq!(ready.credentials(), Some((25575, "秘密")));

        properties.set("rcon.port", "65536").unwrap();
        assert!(!properties.rcon_status().is_available());
        properties.set("rcon.port", "25576").unwrap();
        properties.set("enable-rcon", "false").unwrap();
        assert!(!properties.rcon_status().is_available());
        properties.set("enable-rcon", "true").unwrap();
        properties.set("rcon.password", "").unwrap();
        assert!(!properties.rcon_status().is_available());
    }

    #[test]
    fn explicit_enable_changes_memory_then_atomic_save_writes_layout() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("server.properties");
        let original = "# 保留这一行\nenable-rcon=false\nmotd=你好=A\n";
        fs::write(&path, original).unwrap();

        let mut properties = ServerProperties::load(&path).unwrap();
        assert!(!properties.rcon_status().is_available());
        properties.enable_rcon(25576, "密=码").unwrap();
        // 仅调用内存编辑 API 不应触盘。
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert_eq!(
            properties.to_string(),
            "# 保留这一行\nenable-rcon=true\nmotd=你好=A\nrcon.port=25576\nrcon.password=密=码\n"
        );

        properties.save(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), properties.to_string());
        let loaded = ServerProperties::load(&path).unwrap();
        assert_eq!(loaded.rcon_status().credentials(), Some((25576, "密=码")));
        assert_eq!(loaded.get("motd"), Some("你好=A"));
    }

    #[test]
    fn invalid_edits_do_not_partially_enable_rcon() {
        let mut properties = ServerProperties::parse("enable-rcon=false\n");
        let before = properties.to_string();
        assert_eq!(
            properties.enable_rcon(0, "password").unwrap_err().code,
            "invalid_rcon_config"
        );
        assert_eq!(
            properties
                .enable_rcon(25575, "bad\npassword")
                .unwrap_err()
                .code,
            "invalid_rcon_config"
        );
        assert_eq!(properties.to_string(), before);
        assert_eq!(
            properties.set("motd", "one\ntwo").unwrap_err().code,
            "invalid_property"
        );
        assert_eq!(properties.to_string(), before);
    }
}
