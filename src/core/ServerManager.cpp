#include "core/ServerManager.h"
#include "server/EulaChecker.h"
#include "server/LaunchConfig.h"
#include "server/RconClient.h"
#include "utils/StringUtils.h"
#include "utils/IniParser.h"
#include <filesystem>
#include <fstream>
#include <chrono>
#include <ctime>
#include <sstream>
#include <iomanip>
#include <string_view>

namespace mc_manager::core {

namespace {

// 生成 32 位十六进制 RCON 密码
std::string generate_rcon_password() {
    unsigned char buf[16] = {0};
    std::ifstream urandom("/dev/urandom", std::ios::binary);
    urandom.read(reinterpret_cast<char *>(buf), sizeof(buf));
    static const char hex[] = "0123456789abcdef";
    std::string password;
    password.reserve(sizeof(buf) * 2);
    for (unsigned char b : buf) {
        password += hex[b >> 4];
        password += hex[b & 0x0f];
    }
    return password;
}

} // namespace

ServerManager::~ServerManager() {
    stop_rcon_poller();
}

void ServerManager::load() {
    app_config_.load();
    for (size_t i = 0; i < app_config_.servers.size(); ++i) {
        init_server_process(i);
    }
    start_rcon_poller();
}

void ServerManager::shutdown() {
    stop_rcon_poller();
    for (auto &[idx, proc] : server_processes_) {
        proc.detach();
    }
    app_config_.save();
}

void ServerManager::init_server_process(size_t index) {
    auto &proc = server_processes_[index];
    console_buffers_[index];

    // 控制台输出只进缓冲区与行监听；玩家列表改由 RCON 轮询获取
    proc.set_output_callback(
        [this, index](const std::string &line) {
            console_buffers_[index].push_line(line);
            notify_line_listeners(index, line);
        }
    );

    // 配置中已有 RCON 凭据的服务器直接登记为轮询目标
    // （覆盖 attach 重连已运行服务器的场景）
    auto &sc = app_config_.servers[index];
    if (sc.rcon_port > 0 && !sc.rcon_password.empty()) {
        std::lock_guard<std::mutex> lock(rcon_mutex_);
        rcon_targets_[index] = {sc.rcon_port, sc.rcon_password};
    }

    proc.attach(index, sc.name, sc.working_dir);
}

server::ServerState ServerManager::state(size_t index) {
    auto it = server_processes_.find(index);
    if (it != server_processes_.end()) {
        return it->second.state();
    }
    return server::ServerState::STOPPED;
}

bool ServerManager::is_running(size_t index) {
    auto *proc = process(index);
    return proc && proc->is_running();
}

server::ServerProcess *ServerManager::process(size_t index) {
    auto it = server_processes_.find(index);
    return it != server_processes_.end() ? &it->second : nullptr;
}

server::ConsoleBuffer *ServerManager::console(size_t index) {
    auto it = console_buffers_.find(index);
    return it != console_buffers_.end() ? &it->second : nullptr;
}

std::vector<std::string> ServerManager::players(size_t index) {
    std::lock_guard<std::mutex> lock(players_mutexes_[index]);
    return players_lists_[index];
}

bool ServerManager::rcon_available(size_t index) {
    std::lock_guard<std::mutex> lock(rcon_mutex_);
    return rcon_targets_.count(index) > 0;
}

bool ServerManager::sync_rcon_from_properties(size_t index) {
    auto &sc = app_config_.servers[index];
    std::string path = sc.working_dir.empty()
        ? "server.properties" : sc.working_dir + "/server.properties";

    utils::IniParser parser;
    if (!parser.load(path)) return false;
    if (parser.get("enable-rcon", "false") != "true") return false;

    std::string password = parser.get("rcon.password", "");
    if (password.empty()) return false;

    int port = 25575;
    try { port = std::stoi(parser.get("rcon.port", "25575")); } catch (...) {}

    if (sc.rcon_port != port || sc.rcon_password != password) {
        sc.rcon_port = port;
        sc.rcon_password = password;
        app_config_.save();
    }
    {
        std::lock_guard<std::mutex> lock(rcon_mutex_);
        rcon_targets_[index] = {port, password};
    }
    return true;
}

StartResult ServerManager::start_server(size_t index, bool allow_no_rcon) {
    if (index >= app_config_.servers.size()) return StartResult::INVALID_INDEX;

    auto &sc = app_config_.servers[index];

    // 整合包尚未安装时优先要求运行安装器（此时连 eula.txt / server.properties
    // 都还不存在，EULA 与 RCON 检查要等安装完成后的启动流程再进行）
    if (needs_installer(index)) {
        return StartResult::INSTALLER_REQUIRED;
    }

    if (!sc.working_dir.empty() && !server::check_eula(sc.working_dir)) {
        return StartResult::EULA_REQUIRED;
    }

    if (server_processes_.count(index) == 0) {
        init_server_process(index);
    }

    auto &proc = server_processes_[index];
    if (proc.state() != server::ServerState::STOPPED) {
        return StartResult::ALREADY_RUNNING;
    }

    if (!allow_no_rcon && !sync_rcon_from_properties(index)) {
        return StartResult::RCON_REQUIRED;
    }

    auto config = server::build_launch_config(
        sc.core_type,
        sc.java_path,
        sc.server_jar_path,
        sc.min_memory_mb,
        sc.max_memory_mb,
        sc.additional_jvm_args,
        sc.launch_args_file,
        sc.working_dir);

    // console.log 在 start() 中被截断，内存缓冲区同步清空，
    // 避免控制台残留上一次会话的输出
    console_buffers_[index].clear();
    proc.start(config, index);
    return StartResult::STARTED;
}

StartResult ServerManager::agree_eula_and_start(size_t index) {
    if (index >= app_config_.servers.size()) return StartResult::INVALID_INDEX;
    server::agree_eula(app_config_.servers[index].working_dir);
    return start_server(index);
}

// 用户同意后由管理器自行启用 RCON：写入 server.properties 并保存凭据副本
StartResult ServerManager::enable_rcon_and_start(size_t index) {
    if (index >= app_config_.servers.size()) return StartResult::INVALID_INDEX;

    auto &sc = app_config_.servers[index];

    if (needs_installer(index)) {
        return StartResult::INSTALLER_REQUIRED;
    }

    if (!sc.working_dir.empty() && !server::check_eula(sc.working_dir)) {
        return StartResult::EULA_REQUIRED;
    }

    // 已在 server.properties 中启用（可能为用户手动配置）则直接使用
    if (!sync_rcon_from_properties(index)) {
        // 选端口：沿用已配置值，否则从 25575 起避开其他服务器已占用的端口
        int port = sc.rcon_port > 0 ? sc.rcon_port : 25575;
        for (;;) {
            bool conflict = false;
            for (size_t i = 0; i < app_config_.servers.size(); ++i) {
                if (i != index && app_config_.servers[i].rcon_port == port) {
                    conflict = true;
                    ++port;
                    break;
                }
            }
            if (!conflict) break;
        }

        if (sc.rcon_password.empty()) {
            sc.rcon_password = generate_rcon_password();
        }
        sc.rcon_port = port;

        std::string path = sc.working_dir.empty()
            ? "server.properties" : sc.working_dir + "/server.properties";
        utils::IniParser parser;
        parser.load(path);  // 文件不存在时从空配置开始，服务器启动时会补全其余默认值
        parser.set("enable-rcon", "true");
        parser.set("rcon.port", std::to_string(port));
        parser.set("rcon.password", sc.rcon_password);
        if (!parser.save(path)) {
            sc.rcon_port = 0;
            sc.rcon_password.clear();
            return StartResult::RCON_WRITE_FAILED;
        }
        app_config_.save();

        std::lock_guard<std::mutex> lock(rcon_mutex_);
        rcon_targets_[index] = {port, sc.rcon_password};
    }

    return start_server(index, true);
}

void ServerManager::stop_server(size_t index) {
    if (server_processes_.count(index) > 0) {
        server_processes_[index].stop();
    }
}

StartResult ServerManager::restart_server(size_t index, bool allow_no_rcon) {
    stop_server(index);
    return start_server(index, allow_no_rcon);
}

bool ServerManager::send_command(size_t index, const std::string &command) {
    auto *proc = process(index);
    if (!proc || !proc->is_running()) return false;
    proc->send_command(command);
    return true;
}

bool ServerManager::needs_installer(size_t index) {
    if (index >= app_config_.servers.size()) return false;
    const auto &sc = app_config_.servers[index];
    if (!server::get_core_type_info(sc.core_type).uses_installer) return false;
    // 启动参数文件或可启动 jar 任一存在即视为已安装；
    // 注意 jar 内容为安装器（如改名成 forge.jar 的安装器）不算可启动 jar
    if (!sc.launch_args_file.empty() && std::filesystem::exists(sc.launch_args_file)) return false;
    if (!sc.server_jar_path.empty() && std::filesystem::exists(sc.server_jar_path) &&
        !server::jar_is_installer(sc.server_jar_path)) return false;
    // 没有安装器可运行时不拦截启动（让 Java 报错原样呈现在控制台）
    return !server::find_installer_jar(sc.working_dir).empty();
}

void ServerManager::apply_installer_detection(size_t index,
                                              const server::DetectionResult &detection) {
    if (index >= app_config_.servers.size()) return;
    auto &sc = app_config_.servers[index];
    sc.core_type = detection.core_type;
    if (!detection.minecraft_version.empty()) sc.minecraft_version = detection.minecraft_version;
    if (!detection.core_version.empty()) sc.core_version = detection.core_version;
    sc.launch_args_file = detection.launch_args_file;
    if (!detection.server_jar_path.empty()) sc.server_jar_path = detection.server_jar_path;
    app_config_.save();
}

void ServerManager::append_console_line(size_t index, const std::string &line) {
    auto it = console_buffers_.find(index);
    if (it != console_buffers_.end()) {
        it->second.push_line(line);
    }
    notify_line_listeners(index, line);
}

void ServerManager::start_rcon_poller() {
    if (rcon_poller_.joinable()) return;
    rcon_poller_stop_ = false;
    rcon_poller_ = std::thread(&ServerManager::rcon_poller_loop, this);
}

void ServerManager::stop_rcon_poller() {
    rcon_poller_stop_ = true;
    if (rcon_poller_.joinable()) {
        rcon_poller_.join();
    }
}

// list 回复形如 "There are N of a max of M players online: a, b"（RCON 响应无时间戳）
std::vector<std::string> ServerManager::parse_players_from_list(const std::string &response) {
    constexpr std::string_view kPlayersMarker = "players online:";
    std::vector<std::string> players;
    size_t marker_pos = response.find(kPlayersMarker);
    if (marker_pos == std::string::npos) return players;

    std::string names_str = utils::trim(response.substr(marker_pos + kPlayersMarker.size()));
    if (!names_str.empty()) {
        auto names = utils::split(names_str, ',');
        for (const auto &name : names) {
            std::string trimmed = utils::trim(name);
            if (!trimmed.empty()) {
                players.push_back(trimmed);
            }
        }
    }
    return players;
}

void ServerManager::rcon_poller_loop() {
    // 长连接由本线程独占持有并复用：vanilla 会为每条 RCON 连接
    // 各打一行 starting/shutting down 日志，短连接高频轮询会刷屏
    std::map<size_t, server::RconClient> connections;

    while (!rcon_poller_stop_) {
        std::vector<std::pair<size_t, RconTarget>> targets;
        {
            std::lock_guard<std::mutex> lock(rcon_mutex_);
            for (const auto &[idx, target] : rcon_targets_) {
                targets.emplace_back(idx, target);
            }
        }

        // 清理目标已删除或凭据已变的连接
        for (auto it = connections.begin(); it != connections.end();) {
            bool alive = false;
            for (const auto &[idx, target] : targets) {
                if (idx == it->first && target.port == it->second.port() &&
                    target.password == it->second.password()) {
                    alive = true;
                    break;
                }
            }
            if (!alive) {
                it = connections.erase(it);
            } else {
                ++it;
            }
        }

        for (const auto &[idx, target] : targets) {
            if (rcon_poller_stop_) break;

            auto mit = players_mutexes_.find(idx);
            if (mit == players_mutexes_.end()) continue;  // 服务器可能刚被删除

            if (!is_running(idx)) {
                // 停止状态下清空残留玩家
                std::lock_guard<std::mutex> plock(mit->second);
                players_lists_[idx].clear();
                continue;
            }

            auto &conn = connections[idx];
            if (!conn.is_open()) {
                conn.open(target.port, target.password, 1500);
            }
            if (conn.is_open()) {
                auto response = conn.execute("list", 1500);
                if (response) {
                    auto players = parse_players_from_list(*response);
                    std::lock_guard<std::mutex> plock(mit->second);
                    players_lists_[idx] = std::move(players);
                }
            }
            // 建连或查询失败（启动中、密码被改等）时保留上次结果，下轮重试
        }

        // 3 秒一轮，细粒度睡眠以便及时响应退出
        for (int i = 0; i < 30 && !rcon_poller_stop_; ++i) {
            std::this_thread::sleep_for(std::chrono::milliseconds(100));
        }
    }
}

void ServerManager::add_server(const config::ServerConfig &sc) {
    app_config_.servers.push_back(sc);
    app_config_.active_server_index = app_config_.servers.size() - 1;

    if (app_config_.servers_parent_folder.empty()) {
        std::string parent_folder = app_config_.get_effective_servers_parent_folder();
        std::filesystem::create_directories(parent_folder);
        app_config_.servers_parent_folder = parent_folder;
    }

    app_config_.save();
    init_server_process(app_config_.active_server_index);
}

void ServerManager::delete_server(size_t index) {
    if (index >= app_config_.servers.size()) return;

    if (server_processes_.count(index) > 0 && server_processes_[index].is_running()) {
        server_processes_[index].stop();
    }

    server_processes_.erase(index);
    console_buffers_.erase(index);
    players_mutexes_.erase(index);
    players_lists_.erase(index);

    auto rekey_map = [&]<typename Map>(Map &m) {
        std::vector<typename Map::node_type> nodes;
        for (auto it = m.begin(); it != m.end(); ) {
            if (it->first > index) {
                auto node = m.extract(it++);
                node.key() = node.key() - 1;
                nodes.push_back(std::move(node));
            } else {
                ++it;
            }
        }
        for (auto &node : nodes) {
            m.insert(std::move(node));
        }
    };

    rekey_map(server_processes_);
    rekey_map(console_buffers_);
    rekey_map(players_mutexes_);
    rekey_map(players_lists_);
    {
        std::lock_guard<std::mutex> lock(rcon_mutex_);
        rcon_targets_.erase(index);
        rekey_map(rcon_targets_);
    }

    // 下标发生变化，通知所有行监听断开（SSE 客户端会自行重连）
    {
        std::lock_guard<std::mutex> lock(listeners_mutex_);
        line_listeners_.clear();
    }

    // 物理删除磁盘上的服务器文件
    {
        auto &sc = app_config_.servers[index];
        if (!sc.working_dir.empty()) {
            try {
                std::filesystem::remove_all(sc.working_dir);
            } catch (const std::filesystem::filesystem_error &) {
                // 忽略错误 - 服务器目录可能不存在
            }
        }
    }

    app_config_.servers.erase(app_config_.servers.begin() + index);
    if (app_config_.active_server_index >= app_config_.servers.size()) {
        app_config_.active_server_index = app_config_.servers.empty() ? 0 : app_config_.servers.size() - 1;
    }
    app_config_.save();
}

namespace {

// 备份名只允许目录名字符，杜绝路径穿越
bool is_valid_backup_name(const std::string &name) {
    if (name.empty() || name == "." || name == "..") return false;
    return name.find('/') == std::string::npos && name.find('\\') == std::string::npos;
}

} // namespace

std::vector<std::string> ServerManager::list_backups(size_t index) {
    std::vector<std::string> result;
    if (index >= app_config_.servers.size()) return result;
    try {
        for (const auto &entry :
             std::filesystem::directory_iterator(app_config_.servers[index].backup_dir)) {
            if (entry.is_directory()) {
                result.push_back(entry.path().filename().string());
            }
        }
        std::sort(result.rbegin(), result.rend());
    } catch (...) {
    }
    return result;
}

bool ServerManager::create_backup(size_t index, std::string &result_message) {
    if (index >= app_config_.servers.size()) {
        result_message = "服务器不存在";
        return false;
    }
    if (is_running(index)) {
        result_message = "创建备份前请先停止服务器";
        return false;
    }

    const auto &sc = app_config_.servers[index];
    try {
        std::filesystem::create_directories(sc.backup_dir);
    } catch (...) {
        result_message = "创建备份目录失败";
        return false;
    }

    auto now = std::chrono::system_clock::now();
    auto time_t_now = std::chrono::system_clock::to_time_t(now);
    std::tm tm_now;
    localtime_r(&time_t_now, &tm_now);

    std::ostringstream oss;
    oss << "backup_" << std::put_time(&tm_now, "%Y%m%d_%H%M%S");
    std::string backup_name = oss.str();

    try {
        std::filesystem::copy(sc.world_dir,
                              std::filesystem::path(sc.backup_dir) / backup_name,
                              std::filesystem::copy_options::recursive);
        result_message = backup_name;
        return true;
    } catch (const std::exception &e) {
        result_message = std::string("备份失败：") + e.what();
        return false;
    }
}

bool ServerManager::restore_backup(size_t index, const std::string &backup_name, std::string &error) {
    if (index >= app_config_.servers.size()) {
        error = "服务器不存在";
        return false;
    }
    if (!is_valid_backup_name(backup_name)) {
        error = "非法备份名";
        return false;
    }
    if (is_running(index)) {
        error = "恢复备份前请先停止服务器";
        return false;
    }

    const auto &sc = app_config_.servers[index];
    std::filesystem::path backup_path =
        std::filesystem::path(sc.backup_dir) / backup_name;

    try {
        if (std::filesystem::exists(sc.world_dir)) {
            std::filesystem::remove_all(sc.world_dir);
        }
        std::filesystem::copy(backup_path, sc.world_dir,
                              std::filesystem::copy_options::recursive);
        return true;
    } catch (const std::exception &e) {
        error = std::string("恢复失败：") + e.what();
        return false;
    }
}

bool ServerManager::delete_backup(size_t index, const std::string &backup_name, std::string &error) {
    if (index >= app_config_.servers.size()) {
        error = "服务器不存在";
        return false;
    }
    if (!is_valid_backup_name(backup_name)) {
        error = "非法备份名";
        return false;
    }

    const auto &sc = app_config_.servers[index];
    try {
        std::filesystem::remove_all(std::filesystem::path(sc.backup_dir) / backup_name);
        return true;
    } catch (const std::exception &e) {
        error = std::string("删除失败：") + e.what();
        return false;
    }
}

size_t ServerManager::add_line_listener(size_t index, LineListener listener) {
    std::lock_guard<std::mutex> lock(listeners_mutex_);
    size_t id = next_listener_id_++;
    line_listeners_[index][id] = std::move(listener);
    return id;
}

void ServerManager::remove_line_listener(size_t index, size_t listener_id) {
    std::lock_guard<std::mutex> lock(listeners_mutex_);
    auto it = line_listeners_.find(index);
    if (it != line_listeners_.end()) {
        it->second.erase(listener_id);
        if (it->second.empty()) {
            line_listeners_.erase(it);
        }
    }
}

void ServerManager::notify_line_listeners(size_t index, const std::string &line) {
    std::lock_guard<std::mutex> lock(listeners_mutex_);
    auto it = line_listeners_.find(index);
    if (it == line_listeners_.end()) return;
    for (const auto &[id, listener] : it->second) {
        listener(line);
    }
}

} // namespace mc_manager::core
