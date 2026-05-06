#pragma once

#include <string>
#include <functional>
#include <optional>
#include <nlohmann/json.hpp>

namespace mc_manager::utils {

struct DownloadProgress {
    std::string url;
    size_t downloaded_bytes{0};
    size_t total_bytes{0};
    bool finished{false};
    bool success{false};
    std::string error;
};

using ProgressCallback = std::function<void(const DownloadProgress &)>;

struct DownloadResult {
    bool success{false};
    std::string error;
    size_t downloaded_bytes{0};
};

DownloadResult download_file(const std::string &url,
                             const std::string &dest_path,
                             ProgressCallback progress = nullptr);

std::string http_get(const std::string &url);

std::string http_get_json(const std::string &url);

std::optional<nlohmann::json> http_get_json_parsed(const std::string &url);

} // namespace mc_manager::utils
