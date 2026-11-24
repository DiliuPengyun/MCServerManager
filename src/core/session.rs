use crate::config::ServerId;

/// 会话名只由服务器 ID 决定；重连、启动前检查和目录操作都用同一个名字。
pub(super) fn stable_session_name(id: ServerId) -> String {
    format!("mcsm_{}", id.simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_name_is_derived_from_server_id_only() {
        let id = ServerId::new();
        assert_eq!(stable_session_name(id), format!("mcsm_{}", id.simple()));
        assert_ne!(
            stable_session_name(id),
            stable_session_name(ServerId::new())
        );
    }
}
