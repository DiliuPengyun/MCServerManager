#pragma once

#include <string>
#include <functional>

namespace mc_manager::utils {

struct ExecResult {
    std::string output;
    int exit_code;
};

using LineCallback = std::function<void(const std::string&)>;

std::string exec_popen(const std::string &cmd);
ExecResult exec_with_status(const std::string &cmd);
int exec_streaming(const std::string &cmd, LineCallback on_line);

} // namespace mc_manager::utils
