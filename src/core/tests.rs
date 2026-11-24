use std::{fs, path::Path, sync::Arc, time::Duration};

use tempfile::TempDir;

use super::*;

fn manager(temp: &TempDir) -> Arc<ServerManager> {
    let root = temp.path();
    let store = ConfigStore::with_paths(root.join("config/config.json"), root.join("data"));
    ServerManager::load(store).unwrap()
}

#[test]
fn deleting_order_does_not_define_server_identity() {
    let temp = TempDir::new().unwrap();
    let manager = manager(&temp);
    let first = manager
        .add_server(ServerConfig::new("甲", temp.path().join("a")))
        .unwrap();
    let second = manager
        .add_server(ServerConfig::new("乙", temp.path().join("b")))
        .unwrap();
    assert_eq!(manager.server_config_at(0).unwrap().id, first.id);
    assert_eq!(manager.server_config_at(1).unwrap().id, second.id);
    assert_ne!(first.id, second.id);
    assert_eq!(manager.server_config(second.id).unwrap().name, "乙");
}

#[test]
fn default_directory_does_not_move_existing_servers() {
    let temp = TempDir::new().unwrap();
    let manager = manager(&temp);
    let server = manager
        .add_server(ServerConfig::new(
            "外置服",
            temp.path().join("outside/server"),
        ))
        .unwrap();
    manager
        .update_default_server_dir(Path::new("new-default"))
        .unwrap();
    assert_eq!(
        manager.server_config(server.id).unwrap().working_dir,
        temp.path().join("outside/server")
    );
    assert_eq!(
        manager.config().default_server_dir,
        temp.path().join("data/new-default")
    );
}

#[test]
fn duplicate_directories_are_rejected() {
    let temp = TempDir::new().unwrap();
    let manager = manager(&temp);
    manager
        .add_server(ServerConfig::new("甲", temp.path().join("same")))
        .unwrap();
    let error = manager
        .add_server(ServerConfig::new("乙", temp.path().join("same")))
        .unwrap_err();
    assert_eq!(error.code, "already_exists");
}

#[tokio::test]
async fn startup_checks_do_not_agree_eula_or_enable_rcon() {
    let temp = TempDir::new().unwrap();
    let manager = manager(&temp);
    let directory = temp.path().join("server");
    fs::create_dir_all(&directory).unwrap();
    let server = manager
        .add_server(ServerConfig::new("测试", directory.clone()))
        .unwrap();
    assert_eq!(
        manager.start_server(server.id, false).await.unwrap(),
        StartResult::EulaRequired
    );
    assert!(!directory.join("eula.txt").exists());

    fs::write(directory.join("eula.txt"), "eula=true\n").unwrap();
    fs::write(directory.join("server.properties"), "motd=保留\n").unwrap();
    assert_eq!(
        manager.start_server(server.id, false).await.unwrap(),
        StartResult::RconRequired
    );
    assert_eq!(
        fs::read_to_string(directory.join("server.properties")).unwrap(),
        "motd=保留\n"
    );
    assert_eq!(
        manager.start_server(server.id, false).await.unwrap(),
        StartResult::RconRequired
    );
}

#[tokio::test]
async fn backup_mutations_share_work_and_lifecycle_locks() {
    let temp = TempDir::new().unwrap();
    let manager = manager(&temp);
    let directory = temp.path().join("backup-server");
    fs::create_dir_all(directory.join("world")).unwrap();
    fs::write(directory.join("world/level.dat"), "原始世界").unwrap();
    let server = manager
        .add_server(ServerConfig::new("备份", directory.clone()))
        .unwrap();

    let process = manager.process(server.id).unwrap();
    let lifecycle = process.lock_stopped().await.unwrap();
    let pending = tokio::spawn({
        let manager = manager.clone();
        async move { manager.create_backup(server.id).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!pending.is_finished(), "备份不应越过生命周期锁");
    assert!(manager.has_server_work(server.id));
    assert!(manager.begin_server_work(server.id).is_err());
    drop(lifecycle);
    let name = pending.await.unwrap().unwrap();
    assert!(!manager.has_server_work(server.id));

    let lifecycle = process.lock_stopped().await.unwrap();
    let pending = tokio::spawn({
        let manager = manager.clone();
        async move { manager.create_backup(server.id).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(manager.delete_backup(server.id, &name).await.is_err());
    assert!(directory.join("backups").join(&name).is_dir());
    drop(lifecycle);
    pending.await.unwrap().unwrap();

    fs::write(directory.join("world/level.dat"), "已修改").unwrap();
    manager.restore_backup(server.id, &name).await.unwrap();
    assert_eq!(
        fs::read_to_string(directory.join("world/level.dat")).unwrap(),
        "原始世界"
    );
    manager.delete_backup(server.id, &name).await.unwrap();
    assert!(!directory.join("backups").join(&name).exists());
    assert!(!manager.has_server_work(server.id));
}

#[tokio::test]
async fn registered_removal_waits_for_in_flight_lifecycle_operation() {
    let temp = TempDir::new().unwrap();
    let manager = manager(&temp);
    let directory = temp.path().join("等待删除");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("server.jar"), "保留到锁释放").unwrap();
    let server = manager
        .add_server(ServerConfig::new("等待删除", directory.clone()))
        .unwrap();
    let process = manager.process(server.id).unwrap();
    let lifecycle = process.lock_stopped().await.unwrap();

    let pending = tokio::spawn({
        let manager = manager.clone();
        async move { manager.remove_registered_server(server.id, true).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!pending.is_finished(), "删除不应越过进程操作锁");
    assert!(directory.join("server.jar").is_file());

    drop(lifecycle);
    pending.await.unwrap().unwrap();
    assert!(!directory.exists());
}

#[tokio::test]
async fn registered_removal_separates_unregister_from_disk_delete() {
    let temp = TempDir::new().unwrap();
    let manager = manager(&temp);
    let kept = temp.path().join("保留目录");
    let deleted = temp.path().join("删除目录");
    fs::create_dir_all(&kept).unwrap();
    fs::create_dir_all(&deleted).unwrap();
    fs::write(kept.join("server.jar"), "保留").unwrap();
    fs::write(deleted.join("server.jar"), "删除").unwrap();
    let kept_server = manager
        .add_server(ServerConfig::new("仅注销", kept.clone()))
        .unwrap();
    let deleted_server = manager
        .add_server(ServerConfig::new("删除文件", deleted.clone()))
        .unwrap();

    manager
        .remove_registered_server(kept_server.id, false)
        .await
        .unwrap();
    assert!(kept.join("server.jar").exists());
    assert!(manager.process(kept_server.id).is_none());

    manager
        .remove_registered_server(deleted_server.id, true)
        .await
        .unwrap();
    assert!(!deleted.exists());
    assert!(manager.process(deleted_server.id).is_none());
    assert!(manager.server_config(deleted_server.id).is_err());
}
