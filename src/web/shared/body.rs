use std::collections::HashMap;

use axum::body::Bytes;
use serde_json::Value;

use crate::error::{AppError, Result};

/// 解析缓冲 JSON 请求体，并要求顶层是对象。
pub(crate) fn parse_body(bytes: &Bytes) -> Result<Value> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| AppError::invalid("bad_request", "请求体不是有效 JSON"))?;
    if !value.is_object() {
        return Err(AppError::invalid("bad_request", "JSON 请求体必须是对象"));
    }
    Ok(value)
}

/// 允许空请求体；一旦有内容就必须是合法 JSON 对象。
pub(crate) fn optional_body(bytes: &Bytes) -> Result<Value> {
    if bytes.is_empty() {
        Ok(serde_json::json!({}))
    } else {
        parse_body(bytes)
    }
}

pub(crate) fn required_body_string<'a>(
    body: &'a Value,
    key: &str,
    message: &str,
) -> Result<&'a str> {
    body.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::invalid("bad_request", message))
}

/// 查询参数中的必填 `path`。
pub(crate) fn required_path(query: &HashMap<String, String>) -> Result<&str> {
    query
        .get("path")
        .map(String::as_str)
        .ok_or_else(|| AppError::invalid("bad_request", "缺少 path 参数"))
}
