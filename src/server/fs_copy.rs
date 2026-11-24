use std::{
    fs::{self, DirEntry},
    io::{self, BufRead, BufReader},
    path::Path,
};

use crate::error::{AppError, Result};

pub(crate) fn copy_directory_contents(source: &Path, destination: &Path) -> Result<()> {
    let reader = fs::read_dir(source)
        .map_err(|error| AppError::io(format!("无法读取目录 {}", source.display()), error))?;
    for entry in reader {
        let entry = entry.map_err(|error| AppError::io("读取目录条目失败", error))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|error| AppError::io("读取目录条目类型失败", error))?;
        if file_type.is_dir() {
            fs::create_dir(&destination_path).map_err(|error| {
                AppError::io(
                    format!("无法创建目录 {}", destination_path.display()),
                    error,
                )
            })?;
            copy_directory_contents(&source_path, &destination_path)?;
            // 最后恢复权限，避免只读目录导致子文件无法复制。
            let permissions = entry
                .metadata()
                .map_err(|error| AppError::io("读取目录权限失败", error))?
                .permissions();
            fs::set_permissions(&destination_path, permissions).map_err(|error| {
                AppError::io(
                    format!("无法保留目录权限 {}", destination_path.display()),
                    error,
                )
            })?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                AppError::io(
                    format!(
                        "无法复制文件 {} 到 {}",
                        source_path.display(),
                        destination_path.display()
                    ),
                    error,
                )
            })?;
        } else if file_type.is_symlink() {
            copy_symlink(&source_path, &destination_path)?;
        } else {
            return Err(invalid_path(format!(
                "目录包含不支持的特殊文件: {}",
                source_path.display()
            )));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn copy_symlink(source: &Path, destination: &Path) -> Result<()> {
    let target = fs::read_link(source)
        .map_err(|error| AppError::io(format!("无法读取符号链接 {}", source.display()), error))?;
    std::os::unix::fs::symlink(&target, destination)
        .map_err(|error| AppError::io(format!("无法复制符号链接 {}", destination.display()), error))
}

#[cfg(not(unix))]
fn copy_symlink(source: &Path, _destination: &Path) -> Result<()> {
    Err(invalid_path(format!(
        "当前平台不支持复制符号链接: {}",
        source.display()
    )))
}

/// 校验目录结构、类型、符号链接目标与文件内容，防止不完整副本替换世界。
pub(crate) fn verify_directory_copy(expected: &Path, actual: &Path) -> Result<()> {
    let expected_entries = sorted_entries(expected)?;
    let actual_entries = sorted_entries(actual)?;
    if expected_entries.len() != actual_entries.len() {
        return Err(verification_failed(actual));
    }
    for (expected_entry, actual_entry) in expected_entries.iter().zip(actual_entries.iter()) {
        if expected_entry.file_name() != actual_entry.file_name() {
            return Err(verification_failed(actual));
        }
        let expected_type = expected_entry
            .file_type()
            .map_err(|error| AppError::io("校验源文件类型失败", error))?;
        let actual_type = actual_entry
            .file_type()
            .map_err(|error| AppError::io("校验暂存文件类型失败", error))?;
        if expected_type != actual_type {
            return Err(verification_failed(&actual_entry.path()));
        }
        if expected_type.is_dir() {
            verify_directory_copy(&expected_entry.path(), &actual_entry.path())?;
        } else if expected_type.is_file() {
            let expected_size = expected_entry
                .metadata()
                .map_err(|error| AppError::io("读取源文件大小失败", error))?
                .len();
            let actual_size = actual_entry
                .metadata()
                .map_err(|error| AppError::io("读取暂存文件大小失败", error))?
                .len();
            if expected_size != actual_size
                || !files_equal(&expected_entry.path(), &actual_entry.path())?
            {
                return Err(verification_failed(&actual_entry.path()));
            }
        } else if expected_type.is_symlink()
            && fs::read_link(expected_entry.path())
                .map_err(|error| AppError::io("校验源符号链接失败", error))?
                != fs::read_link(actual_entry.path())
                    .map_err(|error| AppError::io("校验暂存符号链接失败", error))?
        {
            return Err(verification_failed(&actual_entry.path()));
        }
    }
    Ok(())
}

fn sorted_entries(directory: &Path) -> Result<Vec<DirEntry>> {
    let reader = fs::read_dir(directory)
        .map_err(|error| AppError::io(format!("校验时无法读取 {}", directory.display()), error))?;
    let mut entries = reader
        .collect::<io::Result<Vec<_>>>()
        .map_err(|error| AppError::io("校验时读取目录条目失败", error))?;
    entries.sort_by_key(|entry| entry.file_name());
    Ok(entries)
}

fn files_equal(expected: &Path, actual: &Path) -> Result<bool> {
    let mut expected_reader = BufReader::new(
        fs::File::open(expected)
            .map_err(|error| AppError::io(format!("无法打开 {}", expected.display()), error))?,
    );
    let mut actual_reader = BufReader::new(
        fs::File::open(actual)
            .map_err(|error| AppError::io(format!("无法打开 {}", actual.display()), error))?,
    );
    loop {
        let expected_bytes = expected_reader
            .fill_buf()
            .map_err(|error| AppError::io("校验源文件内容失败", error))?;
        let actual_bytes = actual_reader
            .fill_buf()
            .map_err(|error| AppError::io("校验暂存文件内容失败", error))?;
        if expected_bytes.is_empty() || actual_bytes.is_empty() {
            return Ok(expected_bytes.is_empty() && actual_bytes.is_empty());
        }
        let compared = expected_bytes.len().min(actual_bytes.len());
        if expected_bytes[..compared] != actual_bytes[..compared] {
            return Ok(false);
        }
        expected_reader.consume(compared);
        actual_reader.consume(compared);
    }
}

fn verification_failed(path: &Path) -> AppError {
    AppError::internal(format!("目录暂存副本校验失败: {}", path.display()))
}

fn invalid_path(message: impl Into<String>) -> AppError {
    AppError::invalid("invalid_path", message)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn verification_detects_changed_file_contents() {
        let temp = TempDir::new().unwrap();
        let expected = temp.path().join("源");
        let actual = temp.path().join("副本");
        fs::create_dir(&expected).unwrap();
        fs::create_dir(&actual).unwrap();
        fs::write(expected.join("数据.txt"), "AAAA").unwrap();
        fs::write(actual.join("数据.txt"), "BBBB").unwrap();
        assert!(verify_directory_copy(&expected, &actual).is_err());
    }
}
