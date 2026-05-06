#include "server/CoreDetector.h"
#include <filesystem>
#include <fstream>
#include <nlohmann/json.hpp>

namespace mc_manager::server {

static bool dir_contains_segment(const std::filesystem::path &base, const std::string &segment) {
    for (const auto &entry : std::filesystem::recursive_directory_iterator(base)) {
        auto path_str = entry.path().string();
        if (path_str.find(segment) != std::string::npos) {
            return true;
        }
    }
    return false;
}

static std::string find_unix_args_file(const std::filesystem::path &base, const std::string &marker) {
    try {
        for (const auto &entry : std::filesystem::recursive_directory_iterator(base)) {
            if (!entry.is_regular_file()) continue;
            auto name = entry.path().filename().string();
            if (name == "unix_args.txt") {
                auto parent = entry.path().parent_path().string();
                if (parent.find(marker) != std::string::npos) {
                    return entry.path().string();
                }
            }
        }
    } catch (...) {}
    return "";
}

static std::string extract_neoforge_version(const std::filesystem::path &base) {
    try {
        std::filesystem::path lib_path = base / "libraries" / "net" / "neoforged" / "neoforge";
        if (std::filesystem::exists(lib_path)) {
            for (const auto &entry : std::filesystem::directory_iterator(lib_path)) {
                if (entry.is_directory()) {
                    return entry.path().filename().string();
                }
            }
        }
    } catch (...) {}
    return "";
}

static std::string extract_forge_version(const std::filesystem::path &base) {
    try {
        std::filesystem::path lib_path = base / "libraries" / "net" / "minecraftforge" / "forge";
        if (std::filesystem::exists(lib_path)) {
            for (const auto &entry : std::filesystem::directory_iterator(lib_path)) {
                if (entry.is_directory()) {
                    return entry.path().filename().string();
                }
            }
        }
    } catch (...) {}
    return "";
}

static std::string extract_mc_version_from_version_json(const std::filesystem::path &base) {
    try {
        std::filesystem::path vj_path = base / "version.json";
        if (std::filesystem::exists(vj_path)) {
            std::ifstream f(vj_path);
            if (f.is_open()) {
                nlohmann::json j;
                f >> j;
                return j.value("id", "");
            }
        }
    } catch (...) {}

    try {
        for (const auto &entry : std::filesystem::directory_iterator(base / "libraries")) {
            auto name = entry.path().filename().string();
            if (name.find("minecraft_server") != std::string::npos) {
                auto stem = entry.path().stem().string();
                auto pos = stem.rfind('-');
                if (pos != std::string::npos) {
                    return stem.substr(pos + 1);
                }
            }
        }
    } catch (...) {}

    return "";
}

static std::string find_universal_jar(const std::filesystem::path &base,
                                       const std::string &lib_org,
                                       const std::string &lib_core,
                                       const std::string &version) {
    try {
        std::filesystem::path lib_ver_dir = base / "libraries" / "net" / lib_org / lib_core / version;
        if (!std::filesystem::exists(lib_ver_dir)) {
            lib_ver_dir = base / "libraries" / "net" / lib_org / lib_core;
            if (std::filesystem::exists(lib_ver_dir)) {
                for (const auto &v : std::filesystem::directory_iterator(lib_ver_dir)) {
                    if (v.is_directory()) { lib_ver_dir = v.path(); break; }
                }
            }
        }
        if (std::filesystem::exists(lib_ver_dir)) {
            std::string prefix = lib_core + "-" + lib_ver_dir.filename().string() + "-universal";
            for (const auto &entry : std::filesystem::directory_iterator(lib_ver_dir)) {
                if (!entry.is_regular_file()) continue;
                auto name = entry.path().filename().string();
                if (name.ends_with(".jar") && name.find("installer") == std::string::npos) {
                    if (name.find("universal") != std::string::npos) return entry.path().string();
                }
            }
            // fallback: any jar
            for (const auto &entry : std::filesystem::directory_iterator(lib_ver_dir)) {
                if (!entry.is_regular_file()) continue;
                auto name = entry.path().filename().string();
                if (name.ends_with(".jar") && name.find("installer") == std::string::npos &&
                    name.find("sources") == std::string::npos) {
                    return entry.path().string();
                }
            }
        }
    } catch (...) {}
    return "";
}

static std::string find_server_jar(const std::filesystem::path &base) {
    try {
        // Pass 1: top-level files containing "server" in name
        for (const auto &entry : std::filesystem::directory_iterator(base)) {
            if (!entry.is_regular_file()) continue;
            auto name = entry.path().filename().string();
            if (name.find("server") != std::string::npos && name.ends_with(".jar")) {
                if (name.find("installer") == std::string::npos) {
                    return entry.path().string();
                }
            }
        }
        // Pass 2: any top-level .jar not containing "installer"
        for (const auto &entry : std::filesystem::directory_iterator(base)) {
            if (!entry.is_regular_file()) continue;
            if (entry.path().extension() == ".jar") {
                auto name = entry.path().filename().string();
                if (name.find("installer") == std::string::npos) {
                    return entry.path().string();
                }
            }
        }
        // Pass 3: recursive search for any .jar as last resort
        for (const auto &entry : std::filesystem::recursive_directory_iterator(base)) {
            if (!entry.is_regular_file()) continue;
            auto name = entry.path().filename().string();
            if (name.ends_with(".jar") && name.find("installer") == std::string::npos &&
                name.find("sources") == std::string::npos) {
                return entry.path().string();
            }
        }
    } catch (...) {}
    return "";
}

std::optional<DetectionResult> detect_core(const std::string &directory) {
    std::filesystem::path base = std::filesystem::absolute(directory);
    if (!std::filesystem::exists(base) || !std::filesystem::is_directory(base)) {
        return std::nullopt;
    }

    DetectionResult result;

    bool has_neoforged = std::filesystem::exists(base / "libraries" / "net" / "neoforged");
    bool has_minecraftforge = std::filesystem::exists(base / "libraries" / "net" / "minecraftforge");
    bool has_fabricmc = std::filesystem::exists(base / "libraries" / "net" / "fabricmc");

    if (has_neoforged) {
        result.core_type = CoreType::NEOFORGE;
        result.core_version = extract_neoforge_version(base);
        result.launch_args_file = find_unix_args_file(base, "neoforged");
        result.minecraft_version = extract_mc_version_from_version_json(base);
        result.server_jar_path = find_universal_jar(base, "neoforged", "neoforge", result.core_version);
    } else if (has_minecraftforge) {
        result.core_type = CoreType::FORGE;
        result.core_version = extract_forge_version(base);
        result.launch_args_file = find_unix_args_file(base, "minecraftforge");
        result.minecraft_version = extract_mc_version_from_version_json(base);
        result.server_jar_path = find_universal_jar(base, "minecraftforge", "forge", result.core_version);
    } else if (has_fabricmc) {
        result.core_type = CoreType::FABRIC;
        result.minecraft_version = extract_mc_version_from_version_json(base);
        result.server_jar_path = find_server_jar(base);
    } else {
        result.server_jar_path = find_server_jar(base);
        if (result.server_jar_path.empty()) {
            return std::nullopt;
        }

        auto jar_name = std::filesystem::path(result.server_jar_path).filename().string();
        std::string lower_name = jar_name;
        for (auto &c : lower_name) c = std::tolower(c);

        if (lower_name.find("paper") != std::string::npos) {
            result.core_type = CoreType::PAPER;
        } else if (lower_name.find("spigot") != std::string::npos) {
            result.core_type = CoreType::SPIGOT;
        } else {
            result.core_type = CoreType::VANILLA;
        }

        result.minecraft_version = extract_mc_version_from_version_json(base);
    }

    if (result.server_jar_path.empty()) {
        result.server_jar_path = find_server_jar(base);
    }

    return result;
}

} // namespace mc_manager::server
