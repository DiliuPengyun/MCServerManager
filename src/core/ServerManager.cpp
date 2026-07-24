#include "core/ServerManager.h"
#include "server/EulaChecker.h"
#include "server/LaunchConfig.h"
#include "utils/StringUtils.h"
#include <filesystem>
#include <chrono>
#include <ctime>
#include <sstream>
#include <iomanip>

namespace mc_manager::core {

void ServerManager::load() {
    app_config_.load();
    for (size_t i = 0; i < app_config_.servers.size(); ++i) {
        init_server_process(i);
    }
}

void ServerManager::shutdown() {
    for (auto &[idx, proc] : server_processes_) {
        proc.detach();
    }
    app_config_.save();
}

void ServerManager::init_server_process(size_t index) {
    auto &proc = server_processes_[index];
    console_buffers_[index];

    proc.set_output_callback(
        [this, index](const std::string &line) {
            console_buffers_[index].push_line(line);

            if (line.find("players online:") != std::string::npos) {
                size_t colon_pos = line.find(':');
                if (colon_pos != std::string::npos) {
                    std::string names_str = line.substr(colon_pos + 1);
                    names_str = utils::trim(names_str);

                    if (!names_str.empty()) {
                        auto names = utils::split(names_str, ',');
                        std::vector<std::string> players;
                        for (const auto &name : names) {
                            std::string trimmed = utils::trim(name);
                            if (!trimmed.empty()) {
                                players.push_back(trimmed);
                            }
                        }
                        std::lock_guard<std::mutex> lock(players_mutexes_[index]);
                        players_lists_[index] = std::move(players);
                    }
                }
            }

            notify_line_listeners(index, line);
        }
    );

    auto &sc = app_config_.servers[index];
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

StartResult ServerManager::start_server(size_t index) {
    if (index >= app_config_.servers.size()) return StartResult::INVALID_INDEX;

    auto &sc = app_config_.servers[index];

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

    auto config = server::build_launch_config(
        sc.core_type,
        sc.java_path,
        sc.server_jar_path,
        sc.min_memory_mb,
        sc.max_memory_mb,
        sc.additional_jvm_args,
        sc.launch_args_file,
        sc.working_dir);

    proc.start(config, index);
    return StartResult::STARTED;
}

StartResult ServerManager::agree_eula_and_start(size_t index) {
    if (index >= app_config_.servers.size()) return StartResult::INVALID_INDEX;
    server::agree_eula(app_config_.servers[index].working_dir);
    return start_server(index);
}

void ServerManager::stop_server(size_t index) {
    if (server_processes_.count(index) > 0) {
        server_processes_[index].stop();
    }
}

StartResult ServerManager::restart_server(size_t index) {
    stop_server(index);
    return start_server(index);
}

bool ServerManager::send_command(size_t index, const std::string &command) {
    auto *proc = process(index);
    if (!proc || !proc->is_running()) return false;
    proc->send_command(command);
    return true;
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
