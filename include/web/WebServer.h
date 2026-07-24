#pragma once

#include <string>

namespace mc_manager::core { class ServerManager; }

namespace mc_manager::web {

struct WebOptions {
    std::string host{"0.0.0.0"};
    int port{8080};
};

// 基于内置 cpp-httplib 的 Web UI 服务：
// REST 接口（状态 / 启停 / 指令）+ SSE 控制台推送 + 内嵌单页
class WebServer {
public:
    WebServer(core::ServerManager &manager, WebOptions options);
    ~WebServer();
    WebServer(const WebServer &) = delete;
    WebServer &operator=(const WebServer &) = delete;

    void set_token(std::string token);
    const std::string &token() const;

    // 绑定端口（不开始监听），成功返回实际端口，失败返回 -1。
    // 用于守护进程：先绑定成功再通知父进程，避免端口占用时静默失败。
    int bind();
    // 阻塞运行，直到 stop() 被调用（如 SIGTERM 处理函数中）
    bool run();
    void stop();

private:
    struct Impl;
    Impl *impl_;
};

} // namespace mc_manager::web
