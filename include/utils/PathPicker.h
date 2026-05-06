#pragma once

#include <string>

namespace mc_manager::utils {

class PathPicker {
public:
    static bool is_fzf_available();

    static std::string pick(const std::string &start_dir);

private:
    static std::string exec_command(const std::string &cmd);
};

} // namespace mc_manager::utils
