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

} // namespace mc_manager::server
