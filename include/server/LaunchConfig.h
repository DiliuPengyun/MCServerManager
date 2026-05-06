#pragma once

#include <string>
#include <vector>
#include "CoreType.h"

namespace mc_manager::server {

struct LaunchConfig {
    std::string java_path;
    std::vector<std::string> args;
    std::string working_dir;
};

LaunchConfig build_launch_config(
    CoreType core_type,
    const std::string &java_path,
    const std::string &jar_path,
    int min_memory_mb,
    int max_memory_mb,
    const std::string &additional_args,
    const std::string &launch_args_file,
    const std::string &working_dir);

} // namespace mc_manager::server
