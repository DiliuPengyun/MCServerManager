use tempfile::TempDir;

use super::*;

fn server(temp: &TempDir) -> ServerConfig {
    ServerConfig::new("测试服务器", temp.path().join("服务器实例"))
}

fn create_world(server: &ServerConfig) {
    let world = server.world_dir();
    fs::create_dir_all(world.join("区域")).unwrap();
    fs::write(world.join("区域/中文方块.txt"), "初始世界").unwrap();
    fs::write(world.join("level.dat"), [1, 2, 3, 4]).unwrap();
}

#[test]
fn create_list_restore_and_delete_round_trip() {
    let temp = TempDir::new().unwrap();
    let config = server(&temp);
    create_world(&config);
    assert_eq!(list(&config).unwrap(), Vec::<String>::new());

    let name = create(&config).unwrap();
    assert!(name.starts_with("backup_"));
    assert_eq!(list(&config).unwrap(), vec![name.clone()]);
    assert_eq!(
        fs::read(config.backup_dir().join(&name).join("区域/中文方块.txt")).unwrap(),
        "初始世界".as_bytes()
    );

    fs::write(config.world_dir().join("区域/中文方块.txt"), "已经修改").unwrap();
    fs::write(config.world_dir().join("新增文件.txt"), "不应保留").unwrap();
    restore(&config, &name).unwrap();
    assert_eq!(
        fs::read_to_string(config.world_dir().join("区域/中文方块.txt")).unwrap(),
        "初始世界"
    );
    assert!(!config.world_dir().join("新增文件.txt").exists());

    delete(&config, &name).unwrap();
    assert!(list(&config).unwrap().is_empty());
    assert_eq!(delete(&config, &name).unwrap_err().code, "not_found");
}

#[test]
fn timestamp_names_are_unique_within_one_second() {
    let temp = TempDir::new().unwrap();
    let config = server(&temp);
    create_world(&config);
    let first = create(&config).unwrap();
    let second = create(&config).unwrap();
    assert_ne!(first, second);
    let mut names = vec![first, second];
    names.sort_by(|left, right| right.cmp(left));
    assert_eq!(list(&config).unwrap(), names);
}

#[test]
fn restore_can_recreate_a_missing_world() {
    let temp = TempDir::new().unwrap();
    let config = server(&temp);
    create_world(&config);
    let name = create(&config).unwrap();
    fs::remove_dir_all(config.world_dir()).unwrap();
    restore(&config, &name).unwrap();
    assert_eq!(
        fs::read_to_string(config.world_dir().join("区域/中文方块.txt")).unwrap(),
        "初始世界"
    );
}

#[test]
fn backup_names_cannot_escape_backup_directory() {
    let temp = TempDir::new().unwrap();
    let config = server(&temp);
    create_world(&config);
    let outside = temp.path().join("外部备份");
    fs::create_dir(&outside).unwrap();

    for name in [
        "",
        ".",
        "..",
        "../外部备份",
        "/tmp/备份",
        "目录/备份",
        "目录\\备份",
        "坏\0名",
    ] {
        assert_eq!(restore(&config, name).unwrap_err().code, "invalid_path");
        assert_eq!(delete(&config, name).unwrap_err().code, "invalid_path");
    }
    assert!(outside.is_dir());
    assert_eq!(
        fs::read_to_string(config.world_dir().join("区域/中文方块.txt")).unwrap(),
        "初始世界"
    );
}

#[test]
fn invalid_backup_does_not_replace_existing_world() {
    let temp = TempDir::new().unwrap();
    let config = server(&temp);
    create_world(&config);
    fs::create_dir_all(config.backup_dir()).unwrap();
    fs::write(config.backup_dir().join("不是目录"), "损坏备份").unwrap();

    assert_eq!(
        restore(&config, "不是目录").unwrap_err().code,
        "invalid_path"
    );
    assert_eq!(restore(&config, "不存在").unwrap_err().code, "not_found");
    assert_eq!(
        fs::read_to_string(config.world_dir().join("区域/中文方块.txt")).unwrap(),
        "初始世界"
    );
}

#[cfg(unix)]
#[test]
fn copy_failure_cleans_staging_and_preserves_world() {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let temp = TempDir::new().unwrap();
    let config = server(&temp);
    create_world(&config);
    let name = create(&config).unwrap();
    fs::write(config.world_dir().join("区域/中文方块.txt"), "原世界保持").unwrap();
    let fifo = config.backup_dir().join(&name).join("特殊文件.fifo");
    let path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // 安全性：CString 已保证 NUL 结尾，路径在测试期间有效，mkfifo 不接管其内存。
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);

    assert_eq!(restore(&config, &name).unwrap_err().code, "invalid_path");
    assert_eq!(
        fs::read_to_string(config.world_dir().join("区域/中文方块.txt")).unwrap(),
        "原世界保持"
    );
    let leftovers = fs::read_dir(config.world_dir().parent().unwrap())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains(".mcsm-"))
        .count();
    assert_eq!(leftovers, 0);
}

#[test]
fn missing_world_and_backup_inside_world_are_rejected() {
    let temp = TempDir::new().unwrap();
    let config = server(&temp);
    assert_eq!(create(&config).unwrap_err().code, "not_found");

    create_world(&config);
    let mut nested = config.clone();
    nested.backup_dir = nested.world_dir().join("backups");
    assert_eq!(create(&nested).unwrap_err().code, "invalid_path");
    assert_eq!(
        fs::read_to_string(config.world_dir().join("区域/中文方块.txt")).unwrap(),
        "初始世界"
    );
}
