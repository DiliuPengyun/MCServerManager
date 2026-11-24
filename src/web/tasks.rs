use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use parking_lot::{Mutex, RwLock};
use serde_json::{Value, json};

use super::log_buffer::BoundedLog;
use crate::error::{AppError, Result};

#[derive(Debug)]
pub struct WebTask {
    pub id: u64,
    pub status: String,
    pub log: BoundedLog,
    pub error: String,
    pub directory_upload: bool,
    pub finalizing: bool,
    pub cancelled: bool,
    pub active_uploads: usize,
    pub staging_dir: PathBuf,
    pub target_dir: PathBuf,
    pub request_body: Value,
    pub last_activity: Instant,
}

impl WebTask {
    pub fn json(&self) -> Value {
        json!({
            "status": self.status,
            "log": self.log.lines(),
            "log_offset": self.log.offset(),
            "error": self.error,
            "directory_upload": self.directory_upload,
            "finalizing": self.finalizing,
        })
    }
}

pub type TaskRef = Arc<Mutex<WebTask>>;

#[derive(Default)]
pub struct WebTasks {
    next_id: AtomicU64,
    tasks: RwLock<HashMap<u64, TaskRef>>,
}

impl WebTasks {
    pub fn create(&self) -> TaskRef {
        self.prune();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let task = Arc::new(Mutex::new(WebTask {
            id,
            status: "running".into(),
            log: BoundedLog::default(),
            error: String::new(),
            directory_upload: false,
            finalizing: false,
            cancelled: false,
            active_uploads: 0,
            staging_dir: PathBuf::new(),
            target_dir: PathBuf::new(),
            request_body: Value::Null,
            last_activity: Instant::now(),
        }));
        self.tasks.write().insert(id, task.clone());
        task
    }

    pub fn find(&self, id: u64) -> Result<TaskRef> {
        self.tasks.read().get(&id).cloned().ok_or_else(|| {
            AppError::new(
                crate::error::ErrorKind::NotFound,
                "task_not_found",
                "任务不存在",
            )
        })
    }

    pub fn append(task: &TaskRef, line: impl Into<String>) {
        let mut task = task.lock();
        task.log.append(line);
        task.last_activity = Instant::now();
    }

    pub fn finish(task: &TaskRef, result: Result<()>) {
        let mut task = task.lock();
        match result {
            Ok(()) => {
                task.status = "done".into();
                task.error.clear();
            }
            Err(error) => {
                task.status = "error".into();
                task.error = error.message;
            }
        }
        task.finalizing = false;
        task.last_activity = Instant::now();
    }

    pub fn finish_activity(task: &TaskRef) {
        let mut task = task.lock();
        task.active_uploads = task.active_uploads.saturating_sub(1);
        task.last_activity = Instant::now();
    }

    /// 取出过期任务暂存路径；活动上传和正在收尾的任务永远不会进入清理集合。
    pub fn take_stale_staging(&self, max_idle: std::time::Duration) -> Vec<PathBuf> {
        let tasks: Vec<_> = self.tasks.read().values().cloned().collect();
        let mut stale = Vec::new();
        for task in tasks {
            let mut guard = task.lock();
            if guard.directory_upload
                && guard.status != "done"
                && !guard.finalizing
                && guard.active_uploads == 0
                && guard.last_activity.elapsed() >= max_idle
                && !guard.staging_dir.as_os_str().is_empty()
            {
                guard.cancelled = true;
                guard.status = "error".into();
                guard.error = "上传任务因长时间无活动而过期".into();
                stale.push(std::mem::take(&mut guard.staging_dir));
            }
        }
        stale
    }

    /// 扫描守护进程遗留目录时，保留注册表中所有任务的暂存路径，尤其是活动上传。
    pub fn staging_paths(&self) -> std::collections::HashSet<PathBuf> {
        self.tasks
            .read()
            .values()
            .filter_map(|task| {
                let path = task.lock().staging_dir.clone();
                (!path.as_os_str().is_empty()).then_some(path)
            })
            .collect()
    }

    fn prune(&self) {
        let mut tasks = self.tasks.write();
        while tasks.len() >= 64 {
            let candidate = tasks.iter().find_map(|(id, task)| {
                let task = task.lock();
                (task.status != "running" && !task.finalizing && task.active_uploads == 0)
                    .then_some(*id)
            });
            match candidate {
                Some(id) => {
                    tasks.remove(&id);
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_task_log_reports_its_absolute_offset() {
        let tasks = WebTasks::default();
        let task = tasks.create();
        for line in 0..2005 {
            WebTasks::append(&task, format!("行 {line}"));
        }
        let json = task.lock().json();
        assert_eq!(json["log_offset"], 5);
        assert_eq!(json["log"].as_array().unwrap().len(), 2000);
        assert_eq!(json["log"][0], "行 5");
        assert_eq!(json["log"][1999], "行 2004");
    }
}
