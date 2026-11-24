use std::{
    collections::HashMap,
    sync::Arc,
    time::{Instant, SystemTime},
};

use parking_lot::Mutex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::log_buffer::BoundedLog;
use crate::{
    config::ServerId,
    error::{AppError, Result},
};

#[derive(Debug)]
pub struct Operation {
    pub id: String,
    pub server_id: Option<ServerId>,
    pub action: String,
    pub status: String,
    pub result: Value,
    pub error: Option<AppError>,
    pub log: BoundedLog,
    fingerprint: String,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
    pub started: Instant,
}

pub type OperationRef = Arc<Mutex<Operation>>;

impl Operation {
    pub fn json(&self) -> Value {
        json!({
            "operation_id": self.id,
            "server_id": self.server_id.map(|id| id.to_string()),
            "action": self.action,
            "status": self.status,
            "result": self.result,
            "error": self.error.as_ref().map(|error| json!({
                "code": error.code,
                "message": error.message,
            })),
            "log": self.log.lines(),
            "log_offset": self.log.offset(),
            "created_at": self.created_at,
            "updated_at": self.updated_at,
        })
    }
}

#[derive(Default)]
pub struct Operations {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    operations: HashMap<String, OperationRef>,
    idempotency: HashMap<String, String>,
}

impl Operations {
    pub fn begin(
        &self,
        server_id: Option<ServerId>,
        action: &str,
        idempotency_key: Option<&str>,
        request: &Value,
    ) -> Result<(OperationRef, bool)> {
        self.begin_inner(server_id, action, idempotency_key, request, false)
    }

    /// 安装等同类操作复用运行中的操作，但必须先校验已有幂等键的请求指纹。
    pub fn begin_coalesced(
        &self,
        server_id: Option<ServerId>,
        action: &str,
        idempotency_key: Option<&str>,
        request: &Value,
    ) -> Result<(OperationRef, bool)> {
        self.begin_inner(server_id, action, idempotency_key, request, true)
    }

    fn begin_inner(
        &self,
        server_id: Option<ServerId>,
        action: &str,
        idempotency_key: Option<&str>,
        request: &Value,
        coalesce_running: bool,
    ) -> Result<(OperationRef, bool)> {
        if let Some(key) = idempotency_key
            && (key.is_empty()
                || key.len() > 128
                || key.chars().any(|character| character.is_control()))
        {
            return Err(AppError::invalid(
                "invalid_idempotency_key",
                "Idempotency-Key 必须为 1–128 字节的非控制字符",
            ));
        }
        let fingerprint = fingerprint(request)?;
        let scope = idempotency_key.map(|key| {
            format!(
                "{}:{}:{key}",
                server_id.map(|id| id.to_string()).unwrap_or_default(),
                action
            )
        });
        let mut state = self.state.lock();
        if let Some(scope) = &scope
            && let Some(id) = state.idempotency.get(scope).cloned()
        {
            let operation = state
                .operations
                .get(&id)
                .cloned()
                .ok_or_else(|| AppError::internal("幂等操作索引失效"))?;
            if operation.lock().fingerprint != fingerprint {
                return Err(AppError::conflict(
                    "idempotency_conflict",
                    "同一 Idempotency-Key 不能用于不同请求内容",
                ));
            }
            return Ok((operation, true));
        }

        if coalesce_running
            && let Some(operation) = state.operations.values().find_map(|operation| {
                let guard = operation.lock();
                (guard.server_id == server_id
                    && guard.action == action
                    && guard.status == "running")
                    .then(|| operation.clone())
            })
        {
            return Ok((operation, true));
        }

        Self::prune(&mut state);
        if state.operations.len() >= 128 {
            return Err(AppError::busy("操作历史已满，请稍后重试"));
        }
        let id = Uuid::new_v4().to_string();
        let now = SystemTime::now();
        let operation = Arc::new(Mutex::new(Operation {
            id: id.clone(),
            server_id,
            action: action.to_owned(),
            status: "running".into(),
            result: Value::Null,
            error: None,
            log: BoundedLog::default(),
            fingerprint,
            created_at: now,
            updated_at: now,
            started: Instant::now(),
        }));
        state.operations.insert(id.clone(), operation.clone());
        if let Some(scope) = scope {
            state.idempotency.insert(scope, id);
        }
        Ok((operation, false))
    }

    pub fn get(&self, id: &str) -> Result<OperationRef> {
        self.state
            .lock()
            .operations
            .get(id)
            .cloned()
            .ok_or_else(|| {
                AppError::new(
                    crate::ErrorKind::NotFound,
                    "operation_not_found",
                    "操作不存在或已被回收",
                )
            })
    }

    pub fn append(operation: &OperationRef, line: impl Into<String>) {
        let mut operation = operation.lock();
        operation.log.append(line);
        operation.updated_at = SystemTime::now();
    }

    pub fn finish(operation: &OperationRef, result: Result<Value>) {
        let mut operation = operation.lock();
        match result {
            Ok(result) => {
                operation.status = "done".into();
                operation.result = result;
                operation.error = None;
            }
            Err(error) => {
                operation.status = "error".into();
                operation.result = Value::Null;
                operation.error = Some(error);
            }
        }
        operation.updated_at = SystemTime::now();
    }

    fn prune(state: &mut State) {
        while state.operations.len() >= 128 {
            let mut candidate: Option<(String, Instant)> = None;
            for (id, operation) in &state.operations {
                let operation = operation.lock();
                if operation.status == "running" {
                    continue;
                }
                if candidate
                    .as_ref()
                    .is_none_or(|(_, started)| operation.started < *started)
                {
                    candidate = Some((id.clone(), operation.started));
                }
            }
            let Some((id, _)) = candidate else { break };
            state.operations.remove(&id);
            state.idempotency.retain(|_, value| value != &id);
        }
    }
}

fn fingerprint(request: &Value) -> Result<String> {
    let bytes = serde_json::to_vec(request)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotency_replays_only_identical_requests() {
        let operations = Operations::default();
        let server = ServerId::new();
        let (first, reused) = operations
            .begin(
                Some(server),
                "move",
                Some("request-1"),
                &json!({"path":"/a"}),
            )
            .unwrap();
        assert!(!reused);
        let (second, reused) = operations
            .begin(
                Some(server),
                "move",
                Some("request-1"),
                &json!({"path":"/a"}),
            )
            .unwrap();
        assert!(reused);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(
            operations
                .begin(
                    Some(server),
                    "move",
                    Some("request-1"),
                    &json!({"path":"/b"})
                )
                .unwrap_err()
                .code,
            "idempotency_conflict"
        );
        assert_eq!(
            operations
                .begin(Some(server), "move", Some("bad\nkey"), &json!({}))
                .unwrap_err()
                .code,
            "invalid_idempotency_key"
        );
    }

    #[test]
    fn coalescing_never_bypasses_idempotency_conflicts() {
        let operations = Operations::default();
        let server = ServerId::new();
        let (first, reused) = operations
            .begin_coalesced(
                Some(server),
                "install",
                Some("install-request"),
                &json!({"source":"a"}),
            )
            .unwrap();
        assert!(!reused);
        let (running, reused) = operations
            .begin_coalesced(
                Some(server),
                "install",
                Some("another-request"),
                &json!({"source":"b"}),
            )
            .unwrap();
        assert!(reused);
        assert!(Arc::ptr_eq(&first, &running));
        assert_eq!(
            operations
                .begin_coalesced(
                    Some(server),
                    "install",
                    Some("install-request"),
                    &json!({"source":"b"}),
                )
                .unwrap_err()
                .code,
            "idempotency_conflict"
        );
    }

    #[test]
    fn concurrent_identical_keys_create_only_one_operation() {
        let operations = Arc::new(Operations::default());
        let server = ServerId::new();
        let barrier = Arc::new(std::sync::Barrier::new(16));
        let threads: Vec<_> = (0..16)
            .map(|_| {
                let operations = operations.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    operations
                        .begin(
                            Some(server),
                            "move",
                            Some("shared-request"),
                            &json!({"path":"/服务器"}),
                        )
                        .unwrap()
                })
            })
            .collect();
        let created: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        assert_eq!(created.iter().filter(|(_, reused)| !reused).count(), 1);
        assert!(
            created
                .iter()
                .all(|(operation, _)| Arc::ptr_eq(operation, &created[0].0))
        );
        assert_eq!(operations.state.lock().operations.len(), 1);
    }

    #[test]
    fn operation_log_is_bounded_and_replayed_with_absolute_offset() {
        let operations = Operations::default();
        let server = ServerId::new();
        let (operation, reused) = operations
            .begin_coalesced(Some(server), "install", Some("install-log"), &json!({}))
            .unwrap();
        assert!(!reused);
        for line in 0..2005 {
            Operations::append(&operation, format!("行 {line}"));
        }
        let snapshot = operation.lock().json();
        assert_eq!(snapshot["log_offset"], 5);
        assert_eq!(snapshot["log"].as_array().unwrap().len(), 2000);
        assert_eq!(snapshot["log"][0], "行 5");
        assert_eq!(snapshot["log"][1999], "行 2004");

        let (replayed, reused) = operations
            .begin_coalesced(Some(server), "install", Some("install-log"), &json!({}))
            .unwrap();
        assert!(reused);
        assert!(Arc::ptr_eq(&operation, &replayed));
        assert_eq!(replayed.lock().json()["log_offset"], 5);
    }

    #[test]
    fn completion_keeps_structured_result_and_error() {
        let operations = Operations::default();
        let (operation, _) = operations.begin(None, "test", None, &json!({})).unwrap();
        Operations::finish(&operation, Ok(json!({"ok":true})));
        assert_eq!(operation.lock().json()["status"], "done");
        assert_eq!(operation.lock().json()["result"]["ok"], true);
        Operations::finish(
            &operation,
            Err(AppError::conflict("eula_required", "需要同意")),
        );
        let snapshot = operation.lock().json();
        assert_eq!(snapshot["status"], "error");
        assert_eq!(snapshot["error"]["code"], "eula_required");
    }
}
