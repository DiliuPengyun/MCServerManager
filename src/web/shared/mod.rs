//! 按 `ServerId` 组织的 Web 用例。
//!
//! 协议适配层负责 UUID、operation、幂等和创建/上传任务，具体业务实现集中在此；
//! 依赖方向固定为 `api/global/uploads -> shared`，不允许反向依赖。

pub(crate) mod backups;
pub(crate) mod body;
pub(crate) mod config;
pub(crate) mod console;
pub(crate) mod errors;
pub(crate) mod files;
pub(crate) mod json;
pub(crate) mod lifecycle;
pub(crate) mod properties;
pub(crate) mod upload;
