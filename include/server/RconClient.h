#pragma once

#include <optional>
#include <string>

namespace mc_manager::server {

// Source RCON 协议的极简同步客户端，保持长连接复用：
// vanilla 会为每条 RCON 连接各打一行 starting/shutting down 日志，
// 短连接高频轮询会刷屏服务器控制台，因此连接建立后持续复用，
// 仅在断开（服务器停止、认证失败、读写超时）后才重建。
class RconClient {
public:
    RconClient() = default;
    ~RconClient();
    RconClient(const RconClient &) = delete;
    RconClient &operator=(const RconClient &) = delete;

    // 连接 127.0.0.1:port 并完成认证；已打开时直接返回 true。
    // 成功后会记住凭据，供调用方判断凭据是否发生变化。
    bool open(int port, const std::string &password, int timeout_ms = 2000);

    // 在已建立的连接上执行命令，返回响应文本；
    // 任何失败都会关闭连接（下次 open 重建）并返回空。
    std::optional<std::string> execute(const std::string &command, int timeout_ms = 2000);

    void close();
    bool is_open() const { return fd_ >= 0; }
    int port() const { return port_; }
    const std::string &password() const { return password_; }

private:
    int fd_{-1};
    int port_{0};
    std::string password_;
};

} // namespace mc_manager::server
