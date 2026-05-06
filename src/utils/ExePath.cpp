#include "utils/ExePath.h"
#include <filesystem>
#include <unistd.h>
#include <climits>

namespace mc_manager::utils {

std::string get_exe_dir() {
    char buf[PATH_MAX];
    ssize_t len = readlink("/proc/self/exe", buf, sizeof(buf) - 1);
    if (len != -1) {
        buf[len] = '\0';
        return std::filesystem::path(buf).parent_path().string();
    }
    return ".";
}

} // namespace mc_manager::utils
