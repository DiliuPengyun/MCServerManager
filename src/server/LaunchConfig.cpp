#include "server/LaunchConfig.h"
#include "utils/StringUtils.h"

namespace mc_manager::server {

LaunchConfig build_launch_config(
    CoreType core_type,
    const std::string &java_path,
    const std::string &jar_path,
    int min_memory_mb,
    int max_memory_mb,
    const std::string &additional_args,
    const std::string &launch_args_file,
    const std::string &working_dir) {

    LaunchConfig config;
    config.java_path = java_path;
    config.working_dir = working_dir;

    config.args.push_back("-Xms" + std::to_string(min_memory_mb) + "M");
    config.args.push_back("-Xmx" + std::to_string(max_memory_mb) + "M");

    if (!additional_args.empty()) {
        auto tokens = utils::split(additional_args, ' ');
        for (auto &token : tokens) {
            token = utils::trim(token);
            if (!token.empty()) {
                config.args.push_back(token);
            }
        }
    }

    const auto &info = get_core_type_info(core_type);
    if (info.uses_launch_args_file && !launch_args_file.empty()) {
        config.args.push_back("@" + launch_args_file);
    } else {
        config.args.push_back("-jar");
        config.args.push_back(jar_path);
    }

    config.args.push_back("nogui");

    return config;
}

} // namespace mc_manager::server
