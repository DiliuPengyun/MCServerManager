#pragma once

#include <string>
#include <fstream>
#include <filesystem>
#include <optional>
#include <sstream>
#include <nlohmann/json.hpp>

namespace mc_manager::utils {

inline std::optional<std::string> read_file_string(const std::string &path) {
    std::ifstream file(path, std::ios::binary);
    if (!file.is_open()) {
        return std::nullopt;
    }
    std::ostringstream buf;
    buf << file.rdbuf();
    return buf.str();
}

inline bool write_file_string(const std::string &path, const std::string &content) {
    std::ofstream file(path, std::ios::trunc);
    if (!file.is_open()) {
        return false;
    }
    file << content;
    return true;
}

inline std::optional<nlohmann::json> read_json_file(const std::string &path) {
    auto content = read_file_string(path);
    if (!content.has_value()) {
        return std::nullopt;
    }
    try {
        return nlohmann::json::parse(*content);
    } catch (const nlohmann::json::parse_error &) {
        return std::nullopt;
    }
}

inline bool write_json_file(const std::string &path, const nlohmann::json &j, int indent = 4) {
    return write_file_string(path, j.dump(indent));
}

} // namespace mc_manager::utils
