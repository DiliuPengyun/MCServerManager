#pragma once

#include <map>
#include <mutex>
#include <vector>
#include <string>
#include <functional>
#include <atomic>
#include <thread>
#include "config/AppConfig.h"
#include "server/ConsoleBuffer.h"
#include "server/CoreDetector.h"
#include "server/ServerProcess.h"

namespace mc_manager::core {

// start_server 的结果；EULA_REQUIRED 表示需要先同意 EULA，
// RCON_REQUIRED 表示服务器未启用 RCON（玩家管理需要），需用户确认后决定
// enable_rcon_and_start 或以 allow_no_rcon=true 直接启动；
// INSTALLER_REQUIRED 表示整合包尚未运行安装器（Forge/NeoForge 无启动参数文件
// 与可启动 jar），需用户同意后由 run_server_installer 安装再重新启动
enum class StartResult {
    STARTED,
    ALREADY_RUNNING,
    EULA_REQUIRED,
    RCON_REQUIRED,
    RCON_WRITE_FAILED,
    INSTALLER_REQUIRED,
    INVALID_INDEX,
};

// UI 无关的服务器控制层：持有配置与所有服务器运行态，
// TUI（MainWindow）与 Web UI（WebServer）都通过它操作服务器。
class ServerManager {
public:
    ServerManager() = default;
    ~ServerManager();
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

    // RCON 是否已配置可用（玩家管理功能的前提）
    bool rcon_available(size_t index);

    // allow_no_rcon=false 时，若服务器未启用 RCON 返回 RCON_REQUIRED 而不启动
    StartResult start_server(size_t index, bool allow_no_rcon = false);
    StartResult agree_eula_and_start(size_t index);
    void stop_server(size_t index);
    StartResult restart_server(size_t index, bool allow_no_rcon = false);
    // 用户同意后由管理器自行在 server.properties 中启用 RCON（生成端口与密码），然后启动
    StartResult enable_rcon_and_start(size_t index);
    bool send_command(size_t index, const std::string &command);

    // 整合包是否尚未安装：Forge/NeoForge 核心的启动参数文件与可启动 jar 均缺失，
    // 且工作目录中存在安装器 jar（此时 start_server 返回 INSTALLER_REQUIRED）
    bool needs_installer(size_t index);
    // 应用安装后的核心检测结果（更新 jar 路径 / 启动参数文件 / 版本并保存配置）
    void apply_installer_detection(size_t index, const server::DetectionResult &detection);
    // 向控制台缓冲区与行监听追加一行（安装器输出注入 Web 控制台用）
    void append_console_line(size_t index, const std::string &line);

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

    // 读取 server.properties 检查 RCON 是否已启用且有密码；
    // 已启用则把端口/密码同步进配置与 rcon_targets_ 并返回 true
    bool sync_rcon_from_properties(size_t index);

    // RCON 玩家轮询线程：周期性地对运行中且已配置 RCON 的服务器执行 list
    void start_rcon_poller();
    void stop_rcon_poller();
    void rcon_poller_loop();
    static std::vector<std::string> parse_players_from_list(const std::string &response);

    config::AppConfig app_config_;

    std::map<size_t, server::ServerProcess> server_processes_;
    std::map<size_t, server::ConsoleBuffer> console_buffers_;
    std::map<size_t, std::mutex> players_mutexes_;
    std::map<size_t, std::vector<std::string>> players_lists_;

    struct RconTarget {
        int port{0};
        std::string password;
    };
    std::mutex rcon_mutex_;
    std::map<size_t, RconTarget> rcon_targets_;
    std::thread rcon_poller_;
    std::atomic<bool> rcon_poller_stop_{false};

    std::mutex listeners_mutex_;
    std::map<size_t, std::map<size_t, LineListener>> line_listeners_;
    size_t next_listener_id_{1};
};

} // namespace mc_manager::core
