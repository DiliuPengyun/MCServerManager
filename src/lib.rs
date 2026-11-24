pub mod config;
pub mod core;
pub mod error;
pub mod server;
pub mod ui;
pub mod web;

pub use error::{AppError, ErrorKind, Result};

/// 原生 Web 单页由源码 partials 在编译时拼接并内嵌到单个二进制中。
pub use web::assets::INDEX_HTML;
