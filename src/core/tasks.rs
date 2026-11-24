use std::{collections::HashMap, sync::Arc, time::SystemTime};

use parking_lot::RwLock;
use serde::Serialize;
use uuid::Uuid;

/// 任务快照；当前没有任何调用方使用本类型，Web 的添加与上传任务实际由
/// `web::tasks::WebTasks` 管理，任务 ID 为数字。
#[derive(Clone, Debug, Serialize)]
pub struct TaskSnapshot {
    pub task_id: String,
    pub status: TaskStatus,
    pub log: Vec<String>,
    pub error: Option<String>,
    #[serde(skip)]
    pub created_at: SystemTime,
    #[serde(skip)]
    pub updated_at: SystemTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Running,
    Done,
    Error,
}

#[derive(Default)]
pub struct TaskRegistry {
    tasks: RwLock<HashMap<String, Arc<RwLock<TaskSnapshot>>>>,
}

impl TaskRegistry {
    pub fn create(&self) -> String {
        let id = Uuid::new_v4().to_string();
        let now = SystemTime::now();
        let snapshot = TaskSnapshot {
            task_id: id.clone(),
            status: TaskStatus::Running,
            log: Vec::new(),
            error: None,
            created_at: now,
            updated_at: now,
        };
        self.tasks
            .write()
            .insert(id.clone(), Arc::new(RwLock::new(snapshot)));
        id
    }

    pub fn get(&self, id: &str) -> Option<TaskSnapshot> {
        self.tasks.read().get(id).map(|task| task.read().clone())
    }

    pub fn append(&self, id: &str, line: impl Into<String>) {
        if let Some(task) = self.tasks.read().get(id) {
            let mut task = task.write();
            task.log.push(line.into());
            if task.log.len() > 2000 {
                let excess = task.log.len() - 2000;
                task.log.drain(..excess);
            }
            task.updated_at = SystemTime::now();
        }
    }

    pub fn finish(&self, id: &str, result: crate::Result<()>) {
        if let Some(task) = self.tasks.read().get(id) {
            let mut task = task.write();
            match result {
                Ok(()) => {
                    task.status = TaskStatus::Done;
                    task.error = None;
                }
                Err(error) => {
                    task.status = TaskStatus::Error;
                    task.error = Some(error.message);
                }
            }
            task.updated_at = SystemTime::now();
        }
    }
}
