#pragma once

#include <string>
#include <optional>
#include "server/CoreType.h"

namespace mc_manager::server {

struct DetectionResult {
    CoreType core_type{CoreType::CUSTOM};
    std::string minecraft_version;
    std::string core_version;
    std::string launch_args_file;
    std::string server_jar_path;
};

std::optional<DetectionResult> detect_core(const std::string &directory);

// 在目录顶层查找 Forge/NeoForge 安装器 jar（文件名含 forge/neoforge 与 installer，
// 或内容含安装器条目——整合包常把安装器改名成 forge.jar），
// 供尚未安装的整合包运行安装器；找不到返回空串
std::string find_installer_jar(const std::string &directory);

// 按文件名与内容判断 jar 是否为 Forge/NeoForge 安装器
bool jar_is_installer(const std::string &jar_path);

} // namespace mc_manager::server
