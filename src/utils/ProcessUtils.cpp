#include "utils/ProcessUtils.h"
#include <cstdio>
#include <array>
#include <sys/wait.h>

namespace mc_manager::utils {

std::string exec_popen(const std::string &cmd) {
    std::string result;
    std::array<char, 256> buffer;
    FILE *pipe = popen(cmd.c_str(), "r");
    if (!pipe) return result;

    while (fgets(buffer.data(), buffer.size(), pipe) != nullptr) {
        result += buffer.data();
    }
    pclose(pipe);

    while (!result.empty() && (result.back() == '\n' || result.back() == '\r')) {
        result.pop_back();
    }
    return result;
}

ExecResult exec_with_status(const std::string &cmd) {
    ExecResult ret;
    std::array<char, 256> buffer;
    FILE *pipe = popen(cmd.c_str(), "r");
    if (!pipe) {
        ret.exit_code = -1;
        return ret;
    }

    while (fgets(buffer.data(), buffer.size(), pipe) != nullptr) {
        ret.output += buffer.data();
    }
    int status = pclose(pipe);

    while (!ret.output.empty() && (ret.output.back() == '\n' || ret.output.back() == '\r')) {
        ret.output.pop_back();
    }
    ret.exit_code = WEXITSTATUS(status);
    return ret;
}

int exec_streaming(const std::string &cmd, LineCallback on_line) {
    std::array<char, 256> buffer;
    FILE *pipe = popen(cmd.c_str(), "r");
    if (!pipe) return -1;

    while (fgets(buffer.data(), buffer.size(), pipe) != nullptr) {
        std::string line(buffer.data());
        while (!line.empty() && (line.back() == '\n' || line.back() == '\r')) {
            line.pop_back();
        }
        on_line(line);
    }
    int status = pclose(pipe);
    return WEXITSTATUS(status);
}

} // namespace mc_manager::utils
