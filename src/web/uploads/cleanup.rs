use std::{fs, path::Path};

use super::super::tasks::WebTasks;

/// 清理超时任务和守护进程重启前遗留的暂存目录；活动任务路径始终受到保护。
pub(in crate::web) fn cleanup_expired_uploads(tasks: &WebTasks, parent: &Path) {
    const MAX_IDLE: std::time::Duration = std::time::Duration::from_secs(30 * 60);
    for path in tasks.take_stale_staging(MAX_IDLE) {
        if let Err(error) = fs::remove_dir_all(&path) {
            tracing::warn!(%error, path = %path.display(), "无法清理过期上传暂存目录");
        }
    }
    let protected = tasks.staging_paths();
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!(%error, path = %parent.display(), "无法扫描上传暂存目录");
            return;
        }
    };
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(".mcsm-upload-") {
            continue;
        }
        let path = entry.path();
        if protected.contains(&path) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_dir()
            || metadata
                .modified()
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_none_or(|age| age < MAX_IDLE)
        {
            continue;
        }
        if let Err(error) = fs::remove_dir_all(&path) {
            tracing::warn!(%error, path = %path.display(), "无法清理遗留上传暂存目录");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant, SystemTime};

    use tempfile::TempDir;

    use super::*;

    fn make_old(path: &Path) {
        fs::write(path.join("上传内容.bin"), [0, 1, 2, 255]).unwrap();
        let directory = fs::File::open(path).unwrap();
        directory
            .set_times(
                fs::FileTimes::new()
                    .set_modified(SystemTime::now() - Duration::from_secs(2 * 60 * 60)),
            )
            .unwrap();
    }

    #[test]
    fn cleanup_removes_only_expired_unprotected_upload_staging() {
        let temp = TempDir::new().unwrap();
        let parent = temp.path();
        let tasks = WebTasks::default();
        let expired_path = parent.join(".mcsm-upload-expired");
        let active_path = parent.join(".mcsm-upload-active");
        let finalizing_path = parent.join(".mcsm-upload-finalizing");
        let orphan_path = parent.join(".mcsm-upload-orphan");
        let fresh_path = parent.join(".mcsm-upload-fresh");
        let unrelated_path = parent.join("普通目录");
        for path in [
            &expired_path,
            &active_path,
            &finalizing_path,
            &orphan_path,
            &fresh_path,
            &unrelated_path,
        ] {
            fs::create_dir(path).unwrap();
        }
        for path in [&expired_path, &active_path, &finalizing_path, &orphan_path] {
            make_old(path);
        }
        make_old(&unrelated_path);

        let expired = tasks.create();
        {
            let mut task = expired.lock();
            task.directory_upload = true;
            task.staging_dir = expired_path.clone();
            task.last_activity = Instant::now() - Duration::from_secs(31 * 60);
        }
        let active = tasks.create();
        {
            let mut task = active.lock();
            task.directory_upload = true;
            task.staging_dir = active_path.clone();
            task.active_uploads = 1;
            task.last_activity = Instant::now() - Duration::from_secs(31 * 60);
        }
        let finalizing = tasks.create();
        {
            let mut task = finalizing.lock();
            task.directory_upload = true;
            task.staging_dir = finalizing_path.clone();
            task.finalizing = true;
            task.last_activity = Instant::now() - Duration::from_secs(31 * 60);
        }

        cleanup_expired_uploads(&tasks, parent);

        assert!(!expired_path.exists());
        assert!(!orphan_path.exists());
        assert!(active_path.exists());
        assert!(finalizing_path.exists());
        assert!(fresh_path.exists());
        assert!(unrelated_path.exists());
        assert!(expired.lock().cancelled);
        assert_eq!(expired.lock().status, "error");
        assert_eq!(active.lock().status, "running");
        assert!(finalizing.lock().finalizing);
    }
}
