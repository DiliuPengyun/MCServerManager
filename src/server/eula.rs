use std::{fs, path::Path};

use tempfile::NamedTempFile;

use crate::error::{AppError, Result};

/// 只读取 EULA，不创建目录或修改文件。
pub fn eula_accepted(working_dir: &Path) -> bool {
    let Ok(source) = fs::read_to_string(working_dir.join("eula.txt")) else {
        return false;
    };
    source.lines().any(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return false;
        }
        let Some((key, value)) = line.split_once('=') else {
            return false;
        };
        key.trim() == "eula" && value.trim().eq_ignore_ascii_case("true")
    })
}

/// 用户明确同意后写入 `eula=true`。调用方必须把此函数绑定到单独的授权动作。
pub fn agree_eula(working_dir: &Path) -> Result<()> {
    if working_dir.as_os_str().is_empty() {
        return Err(AppError::invalid("invalid_path", "服务器工作目录不能为空"));
    }
    fs::create_dir_all(working_dir)
        .map_err(|error| AppError::io("无法创建服务器工作目录", error))?;
    let path = working_dir.join("eula.txt");
    let source = fs::read_to_string(&path).unwrap_or_default();
    let mut output = String::with_capacity(source.len() + 16);
    let mut replaced = false;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("eula=") {
            output.push_str("eula=true\n");
            replaced = true;
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    if !replaced {
        output.push_str("eula=true\n");
    }

    let mut temporary = NamedTempFile::new_in(working_dir)
        .map_err(|error| AppError::io("无法创建 EULA 临时文件", error))?;
    std::io::Write::write_all(&mut temporary, output.as_bytes())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| AppError::io("无法写入 EULA", error))?;
    temporary
        .persist(&path)
        .map_err(|error| AppError::io("无法保存 EULA", error.error))?;
    fs::File::open(working_dir)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| AppError::io("无法同步 EULA 目录", error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn checking_never_writes_and_agreement_is_explicit() {
        let temp = TempDir::new().unwrap();
        let directory = temp.path().join("server");
        assert!(!eula_accepted(&directory));
        assert!(!directory.exists());

        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("eula.txt"), "# 说明\neula=false\n").unwrap();
        assert!(!eula_accepted(&directory));
        agree_eula(&directory).unwrap();
        assert!(eula_accepted(&directory));
        assert_eq!(
            fs::read_to_string(directory.join("eula.txt")).unwrap(),
            "# 说明\neula=true\n"
        );
    }
}
