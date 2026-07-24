#include "config/AppConfig.h"
#include "utils/ExePath.h"
#include "utils/ProcessUtils.h"
#include <nlohmann/json.hpp>
#include <fstream>
#include <filesystem>
#include <cstdlib>

namespace mc_manager::config {

std::string AppConfig::find_system_java() {
    std::string result = utils::exec_popen("which java 2>/dev/null");
    if (result.empty()) {
        const char *java_home = std::getenv("JAVA_HOME");
        if (java_home && java_home[0] != '\0') {
            result = std::string(java_home) + "/bin/java";
        }
    }
    if (result.empty()) {
        result = "java";
    }
    return result;
}

std::string AppConfig::get_effective_java_path() const {
    if (global_java_path.empty() || global_java_path == "java") {
        std::string sys_java = find_system_java();
        if (!sys_java.empty() && sys_java != "java") {
            return sys_java;
        }
    }
    return global_java_path.empty() ? "java" : global_java_path;
}

std::string AppConfig::get_effective_servers_parent_folder() const {
    if (servers_parent_folder.empty()) {
        return utils::get_exe_dir() + "/server";
    }
    if (std::filesystem::path(servers_parent_folder).is_absolute()) {
        return servers_parent_folder;
    }
    return (std::filesystem::path(utils::get_exe_dir()) / servers_parent_folder).string();
}

std::string AppConfig::config_dir_path() {
    const char *xdg_config = std::getenv("XDG_CONFIG_HOME");
    std::string config_dir;
    if (xdg_config && xdg_config[0] != '\0') {
        config_dir = std::string(xdg_config) + "/MCServerManager";
    } else {
        const char *home = std::getenv("HOME");
        if (home && home[0] != '\0') {
            config_dir = std::string(home) + "/.config/MCServerManager";
        } else {
            return ".";
        }
    }
    try {
        std::filesystem::create_directories(config_dir);
    } catch (const std::filesystem::filesystem_error &) {
        return ".";
    }
    return config_dir;
}

std::string AppConfig::config_file_path() {
    return config_dir_path() + "/config.json";
}

ServerConfig ServerConfig::from_json(const nlohmann::json& sj) {
    ServerConfig sc;
    sc.name = sj.value("name", std::string{});
    sc.server_jar_path = sj.value("server_jar_path", "");
    sc.java_path = sj.value("java_path", "java");
    sc.working_dir = sj.value("working_dir", "");
    sc.min_memory_mb = sj.value("min_memory_mb", 1024);
    sc.max_memory_mb = sj.value("max_memory_mb", 4096);
    sc.additional_jvm_args = sj.value("additional_jvm_args", "");
    sc.backup_dir = sj.value("backup_dir", "./backups");
    sc.world_dir = sj.value("world_dir", "./world");
    sc.core_type = server::core_type_from_id(sj.value("core_type", "custom"));
    sc.minecraft_version = sj.value("minecraft_version", "");
    sc.core_version = sj.value("core_version", "");
    sc.launch_args_file = sj.value("launch_args_file", "");
    return sc;
}

bool AppConfig::load_legacy(const nlohmann::json &j) {
    auto jar_path = j.value("server_jar_path", "");
    if (jar_path.empty() || !std::filesystem::exists(jar_path)) {
        return true;
    }

    auto working_dir = j.value("server_working_dir", "");

    ServerConfig sc;
    auto wd_path = std::filesystem::path(working_dir);
    sc.name = working_dir.empty()
        ? std::filesystem::path(jar_path).stem().string()
        : wd_path.filename().string();

    sc.server_jar_path = jar_path;
    sc.java_path = j.value("java_path", std::string{DEFAULT_JAVA_PATH});
    sc.working_dir = working_dir;
    sc.min_memory_mb = j.value("min_memory_mb", DEFAULT_MIN_MEMORY);
    sc.max_memory_mb = j.value("max_memory_mb", DEFAULT_MAX_MEMORY);
    sc.additional_jvm_args = j.value("additional_jvm_args", "");
    sc.backup_dir = j.value("backup_dir", "./backups");
    sc.world_dir = j.value("world_dir", "./world");
    sc.core_type = server::CoreType::CUSTOM;
    servers.push_back(std::move(sc));

    if (!working_dir.empty()) {
        servers_parent_folder = std::filesystem::path(working_dir).parent_path().string();
    }
    return true;
}

bool AppConfig::load() {
    std::ifstream file(config_file_path());
    if (!file.is_open()) return false;

    try {
        nlohmann::json j;
        file >> j;

        if (j.contains("servers") && j["servers"].is_array()) {
            servers_parent_folder = j.value("servers_parent_folder", "");
            global_java_path = j.value("global_java_path", std::string{DEFAULT_JAVA_PATH});
            active_server_index = j.value("active_server_index", static_cast<size_t>(0));
            web_port = j.value("web_port", 8080);
            web_token = j.value("web_token", "");

            for (const auto &sj : j["servers"]) {
                servers.push_back(ServerConfig::from_json(sj));
            }
        } else {
            load_legacy(j);
        }

        return true;
    } catch (...) {
        return false;
    }
}

bool AppConfig::save() const {
    try {
        nlohmann::json j;

        j["servers_parent_folder"] = servers_parent_folder;
        j["global_java_path"] = global_java_path;
        j["active_server_index"] = active_server_index;
        j["web_port"] = web_port;
        j["web_token"] = web_token;

        auto servers_array = nlohmann::json::array();
        for (const auto &s : servers) {
            nlohmann::json sj;
            sj["name"] = s.name;
            sj["server_jar_path"] = s.server_jar_path;
            sj["java_path"] = s.java_path;
            sj["working_dir"] = s.working_dir;
            sj["min_memory_mb"] = s.min_memory_mb;
            sj["max_memory_mb"] = s.max_memory_mb;
            sj["additional_jvm_args"] = s.additional_jvm_args;
            sj["backup_dir"] = s.backup_dir;
            sj["world_dir"] = s.world_dir;
            sj["core_type"] = std::string(server::get_core_type_info(s.core_type).id);
            sj["minecraft_version"] = s.minecraft_version;
            sj["core_version"] = s.core_version;
            sj["launch_args_file"] = s.launch_args_file;
            servers_array.push_back(std::move(sj));
        }
        j["servers"] = std::move(servers_array);

        std::ofstream file(config_file_path());
        if (!file.is_open()) return false;
        file << j.dump(4);
        return true;
    } catch (...) {
        return false;
    }
}

} // namespace mc_manager::config
