#include "utils/Downloader.h"
#include "utils/ProcessUtils.h"
#include <fstream>
#include <filesystem>
#include <sstream>

namespace mc_manager::utils {

static std::string exec_curl(const std::string &url, const std::string &output_path, bool is_download) {
    std::string cmd = "curl -sS -L --connect-timeout 30 --max-time 600";
    if (is_download) {
        cmd += " -o " + output_path;
    }
    cmd += " \"" + url + "\" 2>&1";

    auto result = exec_with_status(cmd);
    if (result.exit_code != 0) {
        return "curl exited with status " + std::to_string(result.exit_code) + ": " + result.output;
    }
    return "";
}

DownloadResult download_file(const std::string &url,
                             const std::string &dest_path,
                             ProgressCallback progress) {
    DownloadResult result;

    std::filesystem::path p(dest_path);
    if (p.has_parent_path()) {
        std::filesystem::create_directories(p.parent_path());
    }

    if (progress) {
        DownloadProgress prog;
        prog.url = url;
        prog.finished = false;
        progress(prog);
    }

    std::string error = exec_curl(url, dest_path, true);
    if (!error.empty()) {
        result.error = error;
        if (progress) {
            DownloadProgress prog;
            prog.url = url;
            prog.finished = true;
            prog.success = false;
            prog.error = error;
            progress(prog);
        }
        return result;
    }

    if (std::filesystem::exists(dest_path)) {
        result.success = true;
        result.downloaded_bytes = std::filesystem::file_size(dest_path);
    } else {
        result.error = "File not found after download";
    }

    if (progress) {
        DownloadProgress prog;
        prog.url = url;
        prog.finished = true;
        prog.success = result.success;
        prog.error = result.error;
        prog.downloaded_bytes = result.downloaded_bytes;
        progress(prog);
    }

    return result;
}

std::string http_get(const std::string &url) {
    std::string tmp_path = std::filesystem::temp_directory_path() / "mc_manager_tmp_response";
    std::string error = exec_curl(url, tmp_path, true);
    if (!error.empty()) return "";

    std::ifstream file(tmp_path);
    if (!file.is_open()) return "";

    std::ostringstream ss;
    ss << file.rdbuf();
    file.close();
    std::filesystem::remove(tmp_path);
    return ss.str();
}

std::string http_get_json(const std::string &url) {
    return http_get(url);
}

std::optional<nlohmann::json> http_get_json_parsed(const std::string &url) {
    auto body = http_get(url);
    if (body.empty()) return std::nullopt;
    try {
        return nlohmann::json::parse(body);
    } catch (...) {
        return std::nullopt;
    }
}

} // namespace mc_manager::utils
