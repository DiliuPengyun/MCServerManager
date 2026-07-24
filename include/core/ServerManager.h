#pragma once

#include <map>
#include <mutex>
#include <vector>
#include <string>
#include <functional>
#include "config/AppConfig.h"
#include "server/ConsoleBuffer.h"
#include "server/ServerProcess.h"

namespace mc_manager::core {

// start_server 的结果；EULA_REQUIRED 表示需要先同意 EULA
enum class StartResult {
    STARTED,
    ALREADY_RUNNING,
    EULA_REQUIRED,
    INVALID_INDEX,
};

// UI 无关的服务器控制层：持有配置与所有服务器运行态，
// TUI（MainWindow）与 Web UI（WebServer）都通过它操作服务器。
class ServerManager {
public:
    ServerManager() = default;
    ~ServerManager() = default;
    ServerManager(const ServerManager &) = delete;
    ServerManager &operator=(const ServerManager &) = delete;

    // 读取配置并尝试重连（attach）所有已存在的 tmux 会话
    void load();
    // 分离所有服务器进程（不杀服务器）并保存配置
    void shutdown();

    config::AppConfig &config() { return app_config_; }
    bool save_config() { return app_config_.save(); }

    size_t server_count() const { return app_config_.servers.size(); }
    server::ServerState state(size_t index);
    bool is_running(size_t index);

    // 供现有 UI 面板直接使用；index 无效时返回 nullptr
    server::ServerProcess *process(size_t index);
    server::ConsoleBuffer *console(size_t index);
    std::vector<std::string> players(size_t index);

    StartResult start_server(size_t index);
    StartResult agree_eula_and_start(size_t index);
    void stop_server(size_t index);
    StartResult restart_server(size_t index);
    bool send_command(size_t index, const std::string &command);

    // 添加 / 删除服务器（删除会停止进程并物理删除工作目录）
    void add_server(const config::ServerConfig &sc);
    void delete_server(size_t index);

    // 备份操作（基于服务器配置中的 backup_dir / world_dir）。
    // create / restore 要求服务器处于停止状态；name 不允许含路径分隔符。
    std::vector<std::string> list_backups(size_t index);
    bool create_backup(size_t index, std::string &result_message);
    bool restore_backup(size_t index, const std::string &backup_name, std::string &error);
    bool delete_backup(size_t index, const std::string &backup_name, std::string &error);

    // 控制台行监听（Web SSE 推送用）；返回的 id 用于注销
    using LineListener = std::function<void(const std::string &)>;
    size_t add_line_listener(size_t index, LineListener listener);
    void remove_line_listener(size_t index, size_t listener_id);

private:
    void init_server_process(size_t index);
    void notify_line_listeners(size_t index, const std::string &line);

    config::AppConfig app_config_;

    std::map<size_t, server::ServerProcess> server_processes_;
    std::map<size_t, server::ConsoleBuffer> console_buffers_;
    std::map<size_t, std::mutex> players_mutexes_;
    std::map<size_t, std::vector<std::string>> players_lists_;

    std::mutex listeners_mutex_;
    std::map<size_t, std::map<size_t, LineListener>> line_listeners_;
    size_t next_listener_id_{1};
};

} // namespace mc_manager::core
