#pragma once

#include <string>
#include <string_view>
#include <vector>
#include <cstddef>
#include <nlohmann/json.hpp>
#include "../server/CoreType.h"

namespace mc_manager::config {

struct ServerConfig {
    std::string name;
    std::string server_jar_path;
    std::string java_path{"java"};
    std::string working_dir;
    int min_memory_mb{1024};
    int max_memory_mb{4096};
    std::string additional_jvm_args;
    std::string backup_dir{"./backups"};
    std::string world_dir{"./world"};
    server::CoreType core_type{server::CoreType::CUSTOM};
    std::string minecraft_version;
    std::string core_version;
    std::string launch_args_file;

    static ServerConfig from_json(const nlohmann::json& sj);
};

class AppConfig {
public:
    static constexpr int DEFAULT_MIN_MEMORY = 1024;
    static constexpr int DEFAULT_MAX_MEMORY = 4096;
    static constexpr std::string_view DEFAULT_JAVA_PATH = "java";

    bool load();
    bool save() const;

    std::string get_effective_java_path() const;
    std::string get_effective_servers_parent_folder() const;

    std::string servers_parent_folder;
    std::string global_java_path{std::string{DEFAULT_JAVA_PATH}};
    std::vector<ServerConfig> servers;
    size_t active_server_index{0};

    // Web UI 设置（MCServerManager web 模式）
    int web_port{8080};
    std::string web_token;  // 首次启动 web 模式时自动生成

    // 配置文件所在目录（web.log / web.pid 等也放在这里）
    static std::string config_dir_path();

private:
    static std::string config_file_path();
    static std::string find_system_java();
    bool load_legacy(const nlohmann::json &j);
};

} // namespace mc_manager::config
