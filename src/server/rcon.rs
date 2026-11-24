use std::{future::Future, io, time::Duration};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{Instant, timeout_at},
};

use crate::error::{AppError, Result};

/// 单个 RCON 包体（不含长度字段）的最大尺寸，避免异常端点造成无界分配。
pub const MAX_PACKET_SIZE: usize = 1024 * 1024;

const SERVERDATA_RESPONSE_VALUE: i32 = 0;
const SERVERDATA_EXECCOMMAND: i32 = 2;
const SERVERDATA_AUTH_RESPONSE: i32 = 2;
const SERVERDATA_AUTH: i32 = 3;
const AUTH_REQUEST_ID: i32 = 1;
const MAX_SKIPPED_PACKETS: usize = 64;

/// 复用同一条 TCP 连接的 Source RCON 客户端。
///
/// `&mut self` 同时保证命令串行发送，响应只与本次请求 ID 关联。
#[derive(Debug)]
pub struct RconClient {
    stream: Option<TcpStream>,
    next_request_id: i32,
    io_timeout: Duration,
}

impl RconClient {
    /// 在 `timeout` 限定的总时间内完成地址解析、建连和认证。
    /// 后续每条命令也使用该时长作为自己的绝对截止时间。
    pub async fn connect(host: &str, port: u16, password: &str, timeout: Duration) -> Result<Self> {
        if host.trim().is_empty() || port == 0 {
            return Err(unavailable("RCON 主机或端口无效"));
        }
        if timeout.is_zero() {
            return Err(unavailable("RCON 超时时间必须大于零"));
        }
        if password.is_empty() || password.as_bytes().contains(&0) {
            return Err(auth_failed("RCON 密码为空或包含 NUL 字符"));
        }
        validate_payload_size(password.len())
            .map_err(|_| auth_failed("RCON 密码超过 1 MiB 包上限"))?;

        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| unavailable("RCON 超时时间超出可表示范围"))?;
        let mut stream =
            io_with_deadline(deadline, "连接 RCON", TcpStream::connect((host, port))).await?;

        let auth_packet = encode_packet(AUTH_REQUEST_ID, SERVERDATA_AUTH, password.as_bytes())?;
        io_with_deadline(deadline, "发送 RCON 认证", stream.write_all(&auth_packet)).await?;

        // 某些实现会在 AUTH_RESPONSE 前发送空 RESPONSE_VALUE，只接受匹配的认证响应。
        for _ in 0..MAX_SKIPPED_PACKETS {
            let response = read_packet(&mut stream, deadline).await?;
            if response.kind != SERVERDATA_AUTH_RESPONSE {
                continue;
            }
            if response.id == -1 {
                return Err(auth_failed("RCON 密码认证失败"));
            }
            if response.id == AUTH_REQUEST_ID {
                return Ok(Self {
                    stream: Some(stream),
                    next_request_id: AUTH_REQUEST_ID + 1,
                    io_timeout: timeout,
                });
            }
        }

        Err(unavailable("RCON 认证期间收到过多无关数据包"))
    }

    /// 在已认证的长连接上执行一条命令，并返回该请求 ID 对应的响应。
    /// 连接或协议失败后连接会被丢弃，调用方需显式重新 `connect`。
    pub async fn command(&mut self, command: &str) -> Result<String> {
        validate_command(command)?;
        let deadline = Instant::now()
            .checked_add(self.io_timeout)
            .ok_or_else(|| unavailable("RCON 超时时间超出可表示范围"))?;
        let result = self.command_with_deadline(command, deadline).await;
        if result.is_err() {
            // 读写失败、包损坏或响应错位后，流中可能仍有残余字节，不能继续复用。
            self.stream.take();
        }
        result
    }

    /// 查询 vanilla 玩家列表。无人时返回空向量；响应不可识别时返回错误。
    pub async fn list_players(&mut self) -> Result<Vec<String>> {
        let response = self.command("list").await?;
        parse_vanilla_list(&response)
            .ok_or_else(|| unavailable("无法识别 vanilla list 的 RCON 响应"))
    }

    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    async fn command_with_deadline(&mut self, command: &str, deadline: Instant) -> Result<String> {
        let request_id = self.allocate_request_id();
        let packet = encode_packet(request_id, SERVERDATA_EXECCOMMAND, command.as_bytes())?;
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| unavailable("RCON 连接已关闭，请重新连接"))?;

        io_with_deadline(deadline, "发送 RCON 命令", stream.write_all(&packet)).await?;

        for _ in 0..MAX_SKIPPED_PACKETS {
            let response = read_packet(stream, deadline).await?;
            if response.id == -1 && response.kind == SERVERDATA_AUTH_RESPONSE {
                return Err(auth_failed("RCON 会话认证已失效"));
            }
            if response.id != request_id {
                continue;
            }
            if response.kind != SERVERDATA_RESPONSE_VALUE {
                return Err(unavailable("RCON 命令收到了类型不匹配的响应"));
            }
            return String::from_utf8(response.payload)
                .map_err(|_| unavailable("RCON 响应不是有效的 UTF-8"));
        }

        Err(unavailable("RCON 命令期间收到过多无关数据包"))
    }

    fn allocate_request_id(&mut self) -> i32 {
        let id = self.next_request_id;
        self.next_request_id = if id == i32::MAX { 1 } else { id + 1 };
        id
    }
}

/// 解析 vanilla `list` 响应。
///
/// `Some(vec![])` 明确表示服务器在线且当前无人；`None` 表示响应不可用或格式不认识。
pub fn parse_vanilla_list(response: &str) -> Option<Vec<String>> {
    const MARKER: &str = "players online:";

    let marker = response.find(MARKER)?;
    let prefix = response[..marker].trim();
    if !prefix.starts_with("There are ") || !prefix.contains(" of a max of ") {
        return None;
    }

    let names = response[marker + MARKER.len()..].trim();
    if names.is_empty() {
        return Some(Vec::new());
    }
    Some(
        names
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

#[derive(Debug, PartialEq, Eq)]
struct Packet {
    id: i32,
    kind: i32,
    payload: Vec<u8>,
}

fn encode_packet(id: i32, kind: i32, payload: &[u8]) -> Result<Vec<u8>> {
    let body_len = validate_payload_size(payload.len())?;
    let mut output = Vec::with_capacity(body_len + 4);
    output.extend_from_slice(&(body_len as i32).to_le_bytes());
    output.extend_from_slice(&id.to_le_bytes());
    output.extend_from_slice(&kind.to_le_bytes());
    output.extend_from_slice(payload);
    output.extend_from_slice(&[0, 0]);
    Ok(output)
}

#[cfg(test)]
fn decode_packet(frame: &[u8]) -> Result<Packet> {
    if frame.len() < 4 {
        return Err(unavailable("RCON 包缺少长度字段"));
    }
    let body_len = decode_body_len(&frame[..4])?;
    if frame.len() != body_len + 4 {
        return Err(unavailable("RCON 包声明长度与实际长度不一致"));
    }
    decode_packet_body(&frame[4..])
}

fn decode_packet_body(body: &[u8]) -> Result<Packet> {
    if body.len() < 10 {
        return Err(unavailable("RCON 包体小于协议最小长度"));
    }
    if body[body.len() - 2..] != [0, 0] {
        return Err(unavailable("RCON 包缺少两个 NUL 终止字节"));
    }
    Ok(Packet {
        id: i32::from_le_bytes(body[..4].try_into().expect("已检查包体长度")),
        kind: i32::from_le_bytes(body[4..8].try_into().expect("已检查包体长度")),
        payload: body[8..body.len() - 2].to_vec(),
    })
}

async fn read_packet(stream: &mut TcpStream, deadline: Instant) -> Result<Packet> {
    let mut header = [0_u8; 4];
    io_with_deadline(deadline, "读取 RCON 包头", stream.read_exact(&mut header)).await?;
    let body_len = decode_body_len(&header)?;
    let mut body = vec![0_u8; body_len];
    io_with_deadline(deadline, "读取 RCON 包体", stream.read_exact(&mut body)).await?;
    decode_packet_body(&body)
}

fn decode_body_len(header: &[u8]) -> Result<usize> {
    let raw = i32::from_le_bytes(
        header[..4]
            .try_into()
            .map_err(|_| unavailable("RCON 包头不完整"))?,
    );
    if raw < 10 {
        return Err(unavailable("RCON 包声明了非法长度"));
    }
    let length = raw as usize;
    if length > MAX_PACKET_SIZE {
        return Err(unavailable("RCON 包超过 1 MiB 上限"));
    }
    Ok(length)
}

fn validate_payload_size(payload_len: usize) -> Result<usize> {
    let body_len = payload_len
        .checked_add(10)
        .ok_or_else(|| unavailable("RCON 包长度溢出"))?;
    if body_len > MAX_PACKET_SIZE {
        return Err(unavailable("RCON 包超过 1 MiB 上限"));
    }
    Ok(body_len)
}

fn validate_command(command: &str) -> Result<()> {
    if command.trim().is_empty() {
        return Err(invalid_command("RCON 命令不能为空"));
    }
    if command
        .as_bytes()
        .iter()
        .any(|&byte| matches!(byte, b'\r' | b'\n' | 0))
    {
        return Err(invalid_command("RCON 命令必须是单行且不能包含 NUL 字符"));
    }
    if validate_payload_size(command.len()).is_err() {
        return Err(invalid_command("RCON 命令超过 1 MiB 包上限"));
    }
    Ok(())
}

async fn io_with_deadline<T, F>(deadline: Instant, action: &str, operation: F) -> Result<T>
where
    F: Future<Output = io::Result<T>>,
{
    match timeout_at(deadline, operation).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(unavailable(format!("{action}失败: {error}"))),
        Err(_) => Err(unavailable(format!("{action}超时"))),
    }
}

fn unavailable(message: impl Into<String>) -> AppError {
    AppError::unavailable("rcon_unavailable", message)
}

fn auth_failed(message: impl Into<String>) -> AppError {
    AppError::invalid("rcon_auth_failed", message)
}

fn invalid_command(message: impl Into<String>) -> AppError {
    AppError::invalid("invalid_command", message)
}

#[cfg(test)]
mod tests {
    use tokio::net::TcpListener;

    use super::*;

    #[test]
    fn packet_codec_is_little_endian_and_preserves_unicode() {
        let payload = "你好=RCON".as_bytes();
        let encoded = encode_packet(0x0102_0304, SERVERDATA_AUTH, payload).unwrap();
        let body_len = payload.len() + 10;
        assert_eq!(&encoded[..4], &(body_len as i32).to_le_bytes());
        assert_eq!(&encoded[4..8], &[0x04, 0x03, 0x02, 0x01]);
        assert_eq!(&encoded[8..12], &SERVERDATA_AUTH.to_le_bytes());
        assert_eq!(&encoded[encoded.len() - 2..], &[0, 0]);

        let decoded = decode_packet(&encoded).unwrap();
        assert_eq!(decoded.id, 0x0102_0304);
        assert_eq!(decoded.kind, SERVERDATA_AUTH);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn packet_codec_rejects_bad_terminators_lengths_and_oversize() {
        let mut bad_terminator = encode_packet(7, SERVERDATA_EXECCOMMAND, b"list").unwrap();
        let last = bad_terminator.len() - 1;
        bad_terminator[last] = 1;
        assert_eq!(
            decode_packet(&bad_terminator).unwrap_err().code,
            "rcon_unavailable"
        );

        let mut bad_length = encode_packet(7, SERVERDATA_EXECCOMMAND, b"list").unwrap();
        bad_length.push(0);
        assert_eq!(
            decode_packet(&bad_length).unwrap_err().code,
            "rcon_unavailable"
        );
        assert_eq!(
            decode_body_len(&((MAX_PACKET_SIZE + 1) as i32).to_le_bytes())
                .unwrap_err()
                .code,
            "rcon_unavailable"
        );
        assert!(encode_packet(1, SERVERDATA_AUTH, &vec![b'x'; MAX_PACKET_SIZE]).is_err());
    }

    #[test]
    fn vanilla_list_distinguishes_empty_players_from_unavailable_response() {
        assert_eq!(
            parse_vanilla_list("There are 0 of a max of 20 players online:"),
            Some(Vec::new())
        );
        assert_eq!(
            parse_vanilla_list("There are 3 of a max of 20 players online: Alice, 玩家甲, Bob"),
            Some(vec!["Alice".into(), "玩家甲".into(), "Bob".into()])
        );
        assert_eq!(parse_vanilla_list("Unknown command"), None);
        assert_eq!(parse_vanilla_list("players online: Alice"), None);
    }

    #[tokio::test]
    async fn authenticates_correlates_ids_and_reuses_one_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let auth = read_test_packet(&mut stream).await;
            assert_eq!(auth.id, AUTH_REQUEST_ID);
            assert_eq!(auth.kind, SERVERDATA_AUTH);
            assert_eq!(auth.payload, "秘密".as_bytes());

            // 认证前的空响应和错误 ID 均不应被误认为目标响应。
            write_test_packet(&mut stream, 99, SERVERDATA_RESPONSE_VALUE, b"").await;
            write_test_packet(&mut stream, AUTH_REQUEST_ID, SERVERDATA_AUTH_RESPONSE, b"").await;

            for (command, response) in [
                ("list", "There are 0 of a max of 20 players online:"),
                ("say 你好", "已广播"),
            ] {
                let request = read_test_packet(&mut stream).await;
                assert_eq!(request.kind, SERVERDATA_EXECCOMMAND);
                assert_eq!(request.payload, command.as_bytes());
                write_test_packet(
                    &mut stream,
                    request.id + 100,
                    SERVERDATA_RESPONSE_VALUE,
                    "旧响应".as_bytes(),
                )
                .await;
                write_test_packet(
                    &mut stream,
                    request.id,
                    SERVERDATA_RESPONSE_VALUE,
                    response.as_bytes(),
                )
                .await;
            }
        });

        let mut client = RconClient::connect("127.0.0.1", port, "秘密", Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(
            client.command("list").await.unwrap(),
            "There are 0 of a max of 20 players online:"
        );
        assert_eq!(client.command("say 你好").await.unwrap(), "已广播");
        assert!(client.is_connected());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn invalid_commands_do_not_write_or_close_the_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            assert_eq!(read_test_packet(&mut stream).await.kind, SERVERDATA_AUTH);
            write_test_packet(&mut stream, AUTH_REQUEST_ID, SERVERDATA_AUTH_RESPONSE, b"").await;
            let request = read_test_packet(&mut stream).await;
            assert_eq!(request.payload, b"list");
            write_test_packet(
                &mut stream,
                request.id,
                SERVERDATA_RESPONSE_VALUE,
                b"There are 0 of a max of 20 players online:",
            )
            .await;
        });

        let mut client = RconClient::connect("127.0.0.1", port, "password", Duration::from_secs(1))
            .await
            .unwrap();
        for command in ["", " \t", "say one\nsay two", "say one\0hidden"] {
            assert_eq!(
                client.command(command).await.unwrap_err().code,
                "invalid_command"
            );
        }
        assert_eq!(
            client
                .command(&"x".repeat(MAX_PACKET_SIZE))
                .await
                .unwrap_err()
                .code,
            "invalid_command"
        );
        assert!(client.is_connected());
        assert_eq!(client.list_players().await.unwrap(), Vec::<String>::new());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn negative_auth_id_has_a_distinct_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_test_packet(&mut stream).await;
            write_test_packet(&mut stream, -1, SERVERDATA_AUTH_RESPONSE, b"").await;
        });

        let error = RconClient::connect("127.0.0.1", port, "wrong", Duration::from_secs(1))
            .await
            .unwrap_err();
        assert_eq!(error.code, "rcon_auth_failed");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn connect_deadline_covers_authentication_io() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
        });

        let error = RconClient::connect("127.0.0.1", port, "password", Duration::from_millis(20))
            .await
            .unwrap_err();
        assert_eq!(error.code, "rcon_unavailable");
        server.abort();
    }

    async fn read_test_packet(stream: &mut TcpStream) -> Packet {
        let mut header = [0_u8; 4];
        stream.read_exact(&mut header).await.unwrap();
        let length = decode_body_len(&header).unwrap();
        let mut body = vec![0_u8; length];
        stream.read_exact(&mut body).await.unwrap();
        decode_packet_body(&body).unwrap()
    }

    async fn write_test_packet(stream: &mut TcpStream, id: i32, kind: i32, payload: &[u8]) {
        let packet = encode_packet(id, kind, payload).unwrap();
        stream.write_all(&packet).await.unwrap();
    }
}
