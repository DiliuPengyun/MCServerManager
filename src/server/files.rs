use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
};

use serde::Serialize;
use tempfile::NamedTempFile;

use crate::error::{AppError, Result};

const MAX_TEXT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileEntry {
    pub name: String,
    /// 相对于文件服务根目录的路径，不暴露主机绝对路径。
    pub path: String,
    pub is_dir: bool,
    /// 目录大小固定为 0。
    pub size: u64,
    pub mod_time: String,
}

/// 同目录临时文件上的流式上传。未提交前目标原文件保持不变，Drop 会清理暂存文件。
#[derive(Debug)]
pub struct PendingUpload {
    target: PathBuf,
    temporary: NamedTempFile,
}

impl PendingUpload {
    pub fn target(&self) -> &Path {
        &self.target
    }

    pub fn write_chunk(&mut self, chunk: &[u8]) -> Result<()> {
        self.temporary
            .write_all(chunk)
            .map_err(|error| AppError::io("无法写入上传临时文件", error))
    }

    pub fn commit(self) -> Result<()> {
        self.temporary
            .as_file()
            .sync_all()
            .map_err(|error| AppError::io("无法同步上传临时文件", error))?;
        let parent = self
            .target
            .parent()
            .ok_or_else(|| invalid_path("上传目标没有有效父目录"))?
            .to_path_buf();
        self.temporary
            .persist(&self.target)
            .map_err(|error| AppError::io("无法原子替换上传目标", error.error))?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| AppError::io("无法同步上传目录", error))?;
        Ok(())
    }
}

/// 将文件操作限制在一个既存目录内，供各界面共用。
#[derive(Clone, Debug)]
pub struct FileBrowser {
    root: PathBuf,
}

impl FileBrowser {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn list(&self, relative: impl AsRef<Path>) -> Result<Vec<FileEntry>> {
        let logical_dir = normalize_relative(relative.as_ref())?;
        let root = self.canonical_root()?;
        let directory = self.resolve_normalized(&logical_dir, &root)?;
        let metadata = fs::metadata(&directory).map_err(|error| {
            map_not_found(
                error,
                || AppError::not_found("目录不存在"),
                "无法读取目录信息",
            )
        })?;
        if !metadata.is_dir() {
            return Err(AppError::not_found("目标不是目录"));
        }

        let reader = fs::read_dir(&directory).map_err(|error| {
            AppError::io(format!("无法读取目录 {}", directory.display()), error)
        })?;
        let mut entries = Vec::new();
        for item in reader {
            let item = item.map_err(|error| AppError::io("读取目录条目失败", error))?;
            let file_type = item
                .file_type()
                .map_err(|error| AppError::io("无法读取目录条目类型", error))?;
            let is_dir = file_type.is_dir();
            let size = if is_dir {
                0
            } else {
                item.metadata()
                    .map_err(|error| AppError::io("无法读取文件大小", error))?
                    .len()
            };
            let name = item.file_name().to_string_lossy().into_owned();
            let mod_time = item
                .metadata()
                .and_then(|metadata| metadata.modified())
                .map(|time| {
                    chrono::DateTime::<chrono::Local>::from(time)
                        .format("%Y-%m-%d %H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            entries.push(FileEntry {
                path: logical_dir
                    .join(item.file_name())
                    .to_string_lossy()
                    .into_owned(),
                name,
                is_dir,
                size,
                mod_time,
            });
        }
        // 目录在前，同类型按名称排序。
        entries.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(entries)
    }

    pub fn read_text(&self, relative: impl AsRef<Path>) -> Result<String> {
        let file = self.resolve(relative)?;
        read_text_file(&file)
    }

    /// 仅覆盖已经存在且仍可作为文本编辑的普通文件，不隐式创建文件。
    pub fn write_text(&self, relative: impl AsRef<Path>, content: &str) -> Result<()> {
        let file = self.resolve(relative)?;
        // 写入前沿用读取限制，避免通过直接调用接口绕过二进制和大小检查。
        read_text_file(&file)?;
        if content.len() > MAX_TEXT_BYTES {
            return Err(file_too_large());
        }
        fs::write(&file, content.as_bytes())
            .map_err(|error| AppError::io(format!("无法写入文件 {}", file.display()), error))
    }

    /// 新建空文件或目录；父目录必须已经存在，目标不能重名。
    pub fn create(&self, relative: impl AsRef<Path>, is_dir: bool) -> Result<()> {
        let normalized = normalize_relative(relative.as_ref())?;
        if normalized.as_os_str().is_empty() {
            return Err(invalid_path("不能创建文件服务根目录"));
        }
        let root = self.canonical_root()?;
        let target = self.resolve_normalized(&normalized, &root)?;
        if target == root {
            return Err(invalid_path("目标名称无效"));
        }
        match fs::symlink_metadata(&target) {
            Ok(_) => return Err(AppError::already_exists("同名文件或目录已存在")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(AppError::io(
                    format!("无法检查目标 {}", target.display()),
                    error,
                ));
            }
        }

        let parent = target
            .parent()
            .ok_or_else(|| invalid_path("目标没有有效的父目录"))?;
        let parent_metadata = fs::metadata(parent).map_err(|error| {
            map_not_found(
                error,
                || AppError::not_found("父目录不存在"),
                "无法读取父目录信息",
            )
        })?;
        if !parent_metadata.is_dir() {
            return Err(AppError::not_found("父目录不存在或不是目录"));
        }

        let result = if is_dir {
            fs::create_dir(&target)
        } else {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map(|_| ())
        };
        result.map_err(|error| match error.kind() {
            io::ErrorKind::AlreadyExists => AppError::already_exists("同名文件或目录已存在"),
            io::ErrorKind::NotFound => AppError::not_found("父目录不存在"),
            _ => AppError::io(format!("无法创建目标 {}", target.display()), error),
        })
    }

    /// 幂等创建目录及所有缺失父目录，供本地目录上传保留空目录。
    pub fn create_dirs(&self, relative: impl AsRef<Path>) -> Result<()> {
        let normalized = normalize_relative(relative.as_ref())?;
        let root = self.canonical_root()?;
        let mut current = PathBuf::new();
        for component in normalized.components() {
            let Component::Normal(name) = component else {
                continue;
            };
            current.push(name);
            let target = self.resolve_normalized(&current, &root)?;
            match fs::symlink_metadata(&target) {
                Ok(metadata) if metadata.is_dir() => {}
                Ok(_) => {
                    return Err(AppError::already_exists("路径中的同名目标不是目录"));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    fs::create_dir(&target).map_err(|error| {
                        AppError::io(format!("无法创建目录 {}", target.display()), error)
                    })?;
                }
                Err(error) => {
                    return Err(AppError::io(
                        format!("无法检查目录 {}", target.display()),
                        error,
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn begin_upload(
        &self,
        relative: impl AsRef<Path>,
        overwrite: bool,
    ) -> Result<PendingUpload> {
        let normalized = normalize_relative(relative.as_ref())?;
        validate_entry_name(
            normalized
                .file_name()
                .map(Path::new)
                .ok_or_else(|| AppError::invalid("invalid_name", "上传路径必须包含文件名"))?,
        )?;
        let root = self.canonical_root()?;
        let target = self.resolve_normalized(&normalized, &root)?;
        if target == root {
            return Err(AppError::invalid("invalid_name", "不能覆盖文件服务根目录"));
        }
        let existing = match fs::metadata(&target) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(AppError::io(
                    format!("无法检查上传目标 {}", target.display()),
                    error,
                ));
            }
        };
        if let Some(metadata) = &existing {
            if !overwrite {
                return Err(AppError::already_exists("同名文件或目录已存在"));
            }
            if !metadata.is_file() {
                return Err(invalid_path("目标是目录，不能覆盖写入"));
            }
        }
        let parent = target
            .parent()
            .ok_or_else(|| invalid_path("上传目标没有有效父目录"))?;
        if !parent.is_dir() {
            return Err(AppError::not_found("父目录不存在"));
        }
        let temporary = NamedTempFile::new_in(parent)
            .map_err(|error| AppError::io("无法创建上传临时文件", error))?;
        let permissions = existing
            .map(|metadata| metadata.permissions())
            .unwrap_or_else(|| fs::Permissions::from_mode(0o644));
        temporary
            .as_file()
            .set_permissions(permissions)
            .map_err(|error| AppError::io("无法设置上传文件权限", error))?;
        Ok(PendingUpload { target, temporary })
    }

    pub fn upload_file(
        &self,
        relative: impl AsRef<Path>,
        content: &[u8],
        overwrite: bool,
    ) -> Result<()> {
        let mut upload = self.begin_upload(relative, overwrite)?;
        upload.write_chunk(content)?;
        upload.commit()
    }

    /// 仅在原目录内修改条目名称，不能通过名称移动条目。
    pub fn rename(&self, relative: impl AsRef<Path>, new_name: impl AsRef<Path>) -> Result<()> {
        let new_name = new_name.as_ref();
        validate_entry_name(new_name)?;
        let normalized = normalize_relative(relative.as_ref())?;
        let root = self.canonical_root()?;
        let source = self.resolve_normalized(&normalized, &root)?;
        if source == root {
            return Err(invalid_path("不能重命名文件服务根目录"));
        }
        fs::symlink_metadata(&source).map_err(|error| {
            map_not_found(
                error,
                || AppError::not_found("目标不存在"),
                "无法读取目标信息",
            )
        })?;
        let parent = source
            .parent()
            .ok_or_else(|| invalid_path("目标没有有效的父目录"))?;
        let destination = parent.join(new_name);
        match fs::symlink_metadata(&destination) {
            Ok(_) => return Err(AppError::already_exists("同名文件或目录已存在")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(AppError::io(
                    format!("无法检查目标 {}", destination.display()),
                    error,
                ));
            }
        }
        fs::rename(&source, &destination).map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => AppError::not_found("目标不存在"),
            io::ErrorKind::AlreadyExists => AppError::already_exists("同名文件或目录已存在"),
            _ => AppError::io("重命名失败", error),
        })
    }

    pub fn remove(&self, relative: impl AsRef<Path>) -> Result<()> {
        let normalized = normalize_relative(relative.as_ref())?;
        let root = self.canonical_root()?;
        let target = self.resolve_normalized(&normalized, &root)?;
        // 同时阻止 remove("")、remove(".") 和经内部符号链接指向根目录的写法。
        if target == root {
            return Err(invalid_path("不能删除文件服务根目录"));
        }
        let metadata = fs::symlink_metadata(&target).map_err(|error| {
            map_not_found(
                error,
                || AppError::not_found("目标不存在"),
                "无法读取目标信息",
            )
        })?;
        let result = if metadata.file_type().is_dir() {
            // std::fs::remove_dir_all 不会跟随目录内的符号链接。
            fs::remove_dir_all(&target)
        } else {
            fs::remove_file(&target)
        };
        result.map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => AppError::not_found("目标不存在"),
            _ => AppError::io(format!("无法删除目标 {}", target.display()), error),
        })
    }

    /// 解析已存在部分并规范化未存在尾部；任何越界都在文件操作前被拒绝。
    fn resolve(&self, relative: impl AsRef<Path>) -> Result<PathBuf> {
        let normalized = normalize_relative(relative.as_ref())?;
        let root = self.canonical_root()?;
        self.resolve_normalized(&normalized, &root)
    }

    fn canonical_root(&self) -> Result<PathBuf> {
        let root = self.root.canonicalize().map_err(|error| {
            map_not_found(
                error,
                || AppError::not_found("文件服务根目录不存在"),
                "无法解析文件服务根目录",
            )
        })?;
        if !root.is_dir() {
            return Err(invalid_path("文件服务根路径不是目录"));
        }
        Ok(root)
    }

    fn resolve_normalized(&self, normalized: &Path, root: &Path) -> Result<PathBuf> {
        let mut resolved = root.to_path_buf();
        let mut missing = false;
        for component in normalized.components() {
            let Component::Normal(name) = component else {
                continue;
            };
            resolved.push(name);
            if missing {
                continue;
            }
            match fs::symlink_metadata(&resolved) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    resolved = fs::canonicalize(&resolved).map_err(|error| {
                        map_not_found(
                            error,
                            || AppError::not_found("符号链接目标不存在"),
                            "无法解析符号链接",
                        )
                    })?;
                    ensure_inside(root, &resolved)?;
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    // 后续组件必然位于这个不存在的尾部之下，只保留词法路径。
                    missing = true;
                }
                Err(error) => {
                    return Err(AppError::io(
                        format!("无法检查路径 {}", resolved.display()),
                        error,
                    ));
                }
            }
        }
        ensure_inside(root, &resolved)?;
        Ok(resolved)
    }
}

fn normalize_relative(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().as_encoded_bytes().contains(&0) {
        return Err(invalid_path("路径不能包含 NUL 字节"));
    }
    if path.is_absolute() {
        return Err(invalid_path("只接受相对路径"));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) => normalized.push(name),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(invalid_path("路径不能包含绝对路径或 .. 分量"));
            }
        }
    }
    Ok(normalized)
}

fn validate_entry_name(name: &Path) -> Result<()> {
    if name.as_os_str().as_encoded_bytes().contains(&0)
        || name.as_os_str().as_encoded_bytes().contains(&b'\\')
    {
        return Err(AppError::invalid("invalid_name", "名称包含无效字符"));
    }
    let mut components = name.components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(AppError::invalid(
            "invalid_name",
            "新名称必须是非空的纯文件或目录名",
        ));
    }
    Ok(())
}

fn ensure_inside(root: &Path, target: &Path) -> Result<()> {
    if target == root || target.starts_with(root) {
        Ok(())
    } else {
        Err(invalid_path("路径越出文件服务根目录"))
    }
}

fn read_text_file(file: &Path) -> Result<String> {
    let metadata = fs::metadata(file).map_err(|error| {
        map_not_found(
            error,
            || AppError::not_found("文件不存在"),
            "无法读取文件信息",
        )
    })?;
    if !metadata.is_file() {
        return Err(AppError::not_found("目标不是普通文件"));
    }
    if metadata.len() > MAX_TEXT_BYTES as u64 {
        return Err(file_too_large());
    }

    // 即使文件在读取期间增长，也只读取上限加一个字节，避免大文件占满内存。
    let handle = fs::File::open(file)
        .map_err(|error| AppError::io(format!("无法打开文件 {}", file.display()), error))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    handle
        .take(MAX_TEXT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::io(format!("无法读取文件 {}", file.display()), error))?;
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(file_too_large());
    }
    if bytes.contains(&0) {
        return Err(AppError::invalid("binary_file", "二进制文件不支持编辑"));
    }
    String::from_utf8(bytes)
        .map_err(|_| AppError::invalid("binary_file", "文件不是有效的 UTF-8 文本"))
}

fn invalid_path(message: impl Into<String>) -> AppError {
    AppError::invalid("invalid_path", message)
}

fn file_too_large() -> AppError {
    AppError::invalid("file_too_large", "文件过大（超过 1 MiB），不支持在线编辑")
}

fn map_not_found(
    error: io::Error,
    not_found: impl FnOnce() -> AppError,
    context: impl std::fmt::Display,
) -> AppError {
    if error.kind() == io::ErrorKind::NotFound {
        not_found()
    } else {
        AppError::io(context, error)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn browser() -> (TempDir, FileBrowser) {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("服务器目录");
        fs::create_dir(&root).unwrap();
        let root_for_browser = root.clone();
        (temp, FileBrowser::new(root_for_browser))
    }

    #[test]
    fn rejects_absolute_parent_and_nul_paths() {
        let (temp, browser) = browser();
        let outside = temp.path().join("外部.txt");
        fs::write(&outside, "不可修改").unwrap();

        for path in [
            outside.to_str().unwrap(),
            "../外部.txt",
            "目录/../../外部.txt",
            "含有\0空字节.txt",
        ] {
            let error = browser.read_text(path).unwrap_err();
            assert_eq!(error.code, "invalid_path", "路径 {path:?} 未被拒绝");
            assert_eq!(
                browser.create(path, false).unwrap_err().code,
                "invalid_path"
            );
        }
        assert_eq!(fs::read_to_string(outside).unwrap(), "不可修改");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_file_and_directory_symlink_escapes() {
        use std::os::unix::fs::symlink;

        let (temp, browser) = browser();
        let outside = temp.path().join("外部");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("秘密.txt"), "原文").unwrap();
        symlink(&outside, temp.path().join("服务器目录/外部链接")).unwrap();
        symlink(
            outside.join("秘密.txt"),
            temp.path().join("服务器目录/文件链接.txt"),
        )
        .unwrap();

        for path in ["外部链接", "外部链接/秘密.txt", "文件链接.txt"] {
            assert_eq!(browser.read_text(path).unwrap_err().code, "invalid_path");
        }
        assert_eq!(
            browser
                .write_text("外部链接/秘密.txt", "篡改")
                .unwrap_err()
                .code,
            "invalid_path"
        );
        assert_eq!(
            browser.create("外部链接/逃逸.txt", false).unwrap_err().code,
            "invalid_path"
        );
        assert_eq!(
            fs::read_to_string(outside.join("秘密.txt")).unwrap(),
            "原文"
        );
        assert!(!outside.join("逃逸.txt").exists());
    }

    #[test]
    fn chinese_entries_support_full_text_workflow() {
        let (temp, browser) = browser();
        browser.create("世界", true).unwrap();
        browser.create("世界/你好.txt", false).unwrap();
        browser
            .write_text("世界/你好.txt", "第一行：你好\n第二行：世界\n")
            .unwrap();
        assert_eq!(
            browser.read_text("世界/你好.txt").unwrap(),
            "第一行：你好\n第二行：世界\n"
        );

        let root_entries = browser.list("").unwrap();
        assert_eq!(root_entries.len(), 1);
        assert_eq!(root_entries[0].name, "世界");
        assert_eq!(root_entries[0].path, "世界");
        assert!(root_entries[0].is_dir);
        assert_eq!(root_entries[0].size, 0);

        let entries = browser.list("世界").unwrap();
        assert_eq!(entries[0].path, "世界/你好.txt");
        assert!(!entries[0].is_dir);
        assert_eq!(entries[0].size, "第一行：你好\n第二行：世界\n".len() as u64);

        browser.rename("世界/你好.txt", "新名字.txt").unwrap();
        assert_eq!(
            browser.read_text("世界/新名字.txt").unwrap(),
            "第一行：你好\n第二行：世界\n"
        );
        assert_eq!(
            browser
                .rename("世界/新名字.txt", "其他/名字.txt")
                .unwrap_err()
                .code,
            "invalid_name"
        );
        browser.remove("世界").unwrap();
        assert!(
            temp.path()
                .join("服务器目录/世界")
                .symlink_metadata()
                .is_err()
        );
    }

    #[test]
    fn text_limits_and_existing_file_rule_are_enforced() {
        let (_temp, browser) = browser();
        fs::write(browser.root.join("二进制.dat"), [0xff, 0x00, 0xfe]).unwrap();
        fs::write(browser.root.join("非UTF8.dat"), [0xff, 0xfe]).unwrap();
        fs::write(
            browser.root.join("过大.txt"),
            vec![b'a'; MAX_TEXT_BYTES + 1],
        )
        .unwrap();
        fs::write(browser.root.join("边界.txt"), vec![b'a'; MAX_TEXT_BYTES]).unwrap();

        assert_eq!(
            browser.read_text("二进制.dat").unwrap_err().code,
            "binary_file"
        );
        assert_eq!(
            browser.read_text("非UTF8.dat").unwrap_err().code,
            "binary_file"
        );
        assert_eq!(
            browser.read_text("过大.txt").unwrap_err().code,
            "file_too_large"
        );
        assert_eq!(browser.read_text("边界.txt").unwrap().len(), MAX_TEXT_BYTES);
        assert_eq!(
            browser
                .write_text("边界.txt", &"超".repeat(MAX_TEXT_BYTES))
                .unwrap_err()
                .code,
            "file_too_large"
        );
        assert_eq!(browser.read_text("边界.txt").unwrap().len(), MAX_TEXT_BYTES);
        assert_eq!(
            browser.write_text("二进制.dat", "文本").unwrap_err().code,
            "binary_file"
        );
        assert_eq!(
            browser.write_text("尚不存在.txt", "文本").unwrap_err().code,
            "not_found"
        );
        assert!(!browser.root.join("尚不存在.txt").exists());
    }

    #[test]
    fn create_rename_and_remove_report_conflicts_and_protect_root() {
        let (_temp, browser) = browser();
        browser.create("目录", true).unwrap();
        browser.create("目录/甲.txt", false).unwrap();
        browser.create("目录/乙.txt", false).unwrap();

        assert_eq!(
            browser.create("目录", true).unwrap_err().code,
            "already_exists"
        );
        assert_eq!(
            browser.create("缺失/文件.txt", false).unwrap_err().code,
            "not_found"
        );
        assert_eq!(
            browser.rename("目录/甲.txt", "乙.txt").unwrap_err().code,
            "already_exists"
        );
        assert_eq!(browser.remove("").unwrap_err().code, "invalid_path");
        assert_eq!(browser.remove(".").unwrap_err().code, "invalid_path");
        assert_eq!(
            browser.rename(".", "新根").unwrap_err().code,
            "invalid_path"
        );
        assert!(browser.root.is_dir());

        browser.remove("目录/乙.txt").unwrap();
        assert_eq!(
            browser.read_text("目录/乙.txt").unwrap_err().code,
            "not_found"
        );
        assert_eq!(browser.remove("目录/乙.txt").unwrap_err().code, "not_found");
    }

    #[test]
    fn streaming_upload_is_binary_safe_atomic_and_overwrite_checked() {
        let (_temp, browser) = browser();
        browser.create_dirs("空目录/嵌套").unwrap();
        browser.create_dirs("空目录/嵌套").unwrap();
        assert!(browser.root.join("空目录/嵌套").is_dir());

        let content = [0, 0xff, b'M', b'C', 0, 1];
        let mut upload = browser.begin_upload("空目录/嵌套/核心.jar", false).unwrap();
        upload.write_chunk(&content[..3]).unwrap();
        upload.write_chunk(&content[3..]).unwrap();
        assert!(!browser.root.join("空目录/嵌套/核心.jar").exists());
        upload.commit().unwrap();
        assert_eq!(
            fs::read(browser.root.join("空目录/嵌套/核心.jar")).unwrap(),
            content
        );
        assert_eq!(
            browser
                .begin_upload("空目录/嵌套/核心.jar", false)
                .unwrap_err()
                .code,
            "already_exists"
        );
        browser
            .upload_file("空目录/嵌套/核心.jar", b"new", true)
            .unwrap();
        assert_eq!(
            fs::read(browser.root.join("空目录/嵌套/核心.jar")).unwrap(),
            b"new"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_to_root_cannot_bypass_root_removal_guard() {
        use std::os::unix::fs::symlink;

        let (_temp, browser) = browser();
        symlink(".", browser.root.join("根链接")).unwrap();
        assert_eq!(browser.remove("根链接").unwrap_err().code, "invalid_path");
        assert!(browser.root.is_dir());
    }
}
