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

// 在顶层目录查找文件名（小写）包含指定标记的 jar。
// exclude_installer 为 true 时跳过安装器 jar（不能直接作为服务端启动）。
static std::string find_top_level_jar_with_marker(const std::filesystem::path &base,
                                                  const std::string &marker,
                                                  bool exclude_installer) {
    try {
        for (const auto &entry : std::filesystem::directory_iterator(base)) {
            if (!entry.is_regular_file()) continue;
            std::string name = entry.path().filename().string();
            for (auto &c : name) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
            if (!name.ends_with(".jar")) continue;
            if (exclude_installer && name.find("installer") != std::string::npos) continue;
            if (name.find(marker) != std::string::npos) return entry.path().string();
        }
    } catch (...) {}
    return "";
}

// 扫描 jar 尾部字节判断其中是否存在指定条目名前缀。
// zip 中央目录位于文件末尾且条目名不压缩，可直接字节扫描，无需解压。
static bool jar_contains_entry(const std::filesystem::path &jar, const std::string &entry_prefix) {
    try {
        std::ifstream f(jar, std::ios::binary | std::ios::ate);
        if (!f.is_open()) return false;
        auto size = f.tellg();
        if (size <= 0) return false;
        constexpr std::streamoff kTail = 4 * 1024 * 1024;  // 尾部 4MB 足以覆盖中央目录
        std::streamoff read_size = std::min<std::streamoff>(size, kTail);
        std::string buf(static_cast<size_t>(read_size), '\0');
        f.seekg(size - read_size);
        f.read(buf.data(), read_size);
        return buf.find(entry_prefix) != std::string::npos;
    } catch (...) {}
    return false;
}

// 判断 jar 是否为 Forge/NeoForge 安装器（按内容而非文件名）：
// 整合包常把安装器改名成 forge.jar 之类，仅靠文件名识别不出来
static bool jar_looks_like_installer(const std::filesystem::path &jar) {
    return jar_contains_entry(jar, "net/minecraftforge/installer/") ||
           jar_contains_entry(jar, "cpw/mods/fml/installer/") ||
           jar_contains_entry(jar, "net/neoforged/installer/");
}

// 在顶层目录按文件名标记查找加载器 jar，并按内容区分角色：
// 安装器（文件名含 installer，或 jar 内含安装器条目）与可启动 jar。
// 任一输出参数为 nullptr 表示不收集该角色。
static void find_loader_jars(const std::filesystem::path &base,
                             const std::string &marker,
                             std::string *server_jar_out,
                             std::string *installer_out) {
    try {
        for (const auto &entry : std::filesystem::directory_iterator(base)) {
            if (!entry.is_regular_file()) continue;
            std::string name = entry.path().filename().string();
            for (auto &c : name) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
            if (!name.ends_with(".jar")) continue;
            if (name.find(marker) == std::string::npos) continue;
            bool is_installer = name.find("installer") != std::string::npos ||
                                jar_looks_like_installer(entry.path());
            if (is_installer) {
                if (installer_out && installer_out->empty()) *installer_out = entry.path().string();
            } else {
                if (server_jar_out && server_jar_out->empty()) *server_jar_out = entry.path().string();
            }
        }
    } catch (...) {}
}

// 从 "forge-<mc版本>-<加载器版本>[-universal|-installer].jar" 形式的文件名解析版本号。
// core_version 采用与 libraries 目录名一致的组合形式（如 1.12.2-14.23.5.2860）。
static void parse_loader_jar_versions(const std::string &jar_path,
                                      const std::string &prefix,
                                      std::string &mc_version,
                                      std::string &core_version) {
    std::string name = std::filesystem::path(jar_path).filename().string();
    if (!name.starts_with(prefix + "-") || !name.ends_with(".jar")) return;
    std::string ver = name.substr(prefix.size() + 1, name.size() - prefix.size() - 1 - 4);
    for (const auto &suffix : {std::string("-universal"), std::string("-installer")}) {
        if (ver.ends_with(suffix)) {
            ver = ver.substr(0, ver.size() - suffix.size());
            break;
        }
    }
    if (ver.empty()) return;
    core_version = ver;
    auto dash = ver.find('-');
    if (dash != std::string::npos) {
        mc_version = ver.substr(0, dash);
    }
}

// 判断顶层候选 jar 是否可能是可启动服务端：文件名含 installer 的直接排除；
// forge 系 jar（含 neoforge）按内容再判一次，避免改名成 forge.jar 的安装器被当作服务端
static bool top_jar_is_launchable(const std::string &lower_name, const std::filesystem::path &path) {
    if (lower_name.find("installer") != std::string::npos) return false;
    if (lower_name.find("forge") != std::string::npos && jar_looks_like_installer(path)) return false;
    return true;
}

static std::string find_server_jar(const std::filesystem::path &base) {
    try {
        // Pass 1: top-level files containing "server" in name
        for (const auto &entry : std::filesystem::directory_iterator(base)) {
            if (!entry.is_regular_file()) continue;
            auto name = entry.path().filename().string();
            std::string lower_name = name;
            for (auto &c : lower_name) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
            if (lower_name.find("server") != std::string::npos && lower_name.ends_with(".jar") &&
                top_jar_is_launchable(lower_name, entry.path())) {
                return entry.path().string();
            }
        }
        // Pass 2: any top-level .jar not containing "installer"
        for (const auto &entry : std::filesystem::directory_iterator(base)) {
            if (!entry.is_regular_file()) continue;
            if (entry.path().extension() == ".jar") {
                auto name = entry.path().filename().string();
                std::string lower_name = name;
                for (auto &c : lower_name) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
                if (top_jar_is_launchable(lower_name, entry.path())) {
                    return entry.path().string();
                }
            }
        }
        // Pass 3: recursive search for any .jar as last resort
        // （跳过 mods 目录：mod jar 永远不是可启动的服务端核心，
        //   否则会污染 Forge/NeoForge 安装结果的 server_jar_path）
        for (const auto &entry : std::filesystem::recursive_directory_iterator(base)) {
            if (!entry.is_regular_file()) continue;
            bool in_mods_dir = false;
            for (const auto &part : entry.path()) {
                if (part.filename() == "mods") { in_mods_dir = true; break; }
            }
            if (in_mods_dir) continue;
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
    // 仅发现安装器 jar 时置 true：核心类型可确定但服务端尚未安装，
    // 不做 jar 兜底，避免整合包被当作原版启动
    bool server_not_installed = false;

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

        // 部分整合包没有 libraries 目录（尚未运行安装器或为旧版打包方式），
        // 此时根据顶层 jar 文件名中的加载器标记识别核心类型，并按内容区分
        // 可启动 jar 与安装器（整合包常把安装器改名成 forge.jar）。
        // 注意 neoforge 必须先于 forge 匹配（"neoforge" 包含 "forge" 子串）。
        std::string neoforge_server, neoforge_installer;
        find_loader_jars(base, "neoforge", &neoforge_server, &neoforge_installer);
        std::string forge_server, forge_installer;
        find_loader_jars(base, "forge", &forge_server, &forge_installer);
        std::string fabric_jar = find_top_level_jar_with_marker(base, "fabric", true);

        if (!neoforge_server.empty()) {
            result.core_type = CoreType::NEOFORGE;
            result.server_jar_path = neoforge_server;
            parse_loader_jar_versions(neoforge_server, "neoforge",
                                      result.minecraft_version, result.core_version);
        } else if (!forge_server.empty()) {
            result.core_type = CoreType::FORGE;
            result.server_jar_path = forge_server;
            parse_loader_jar_versions(forge_server, "forge",
                                      result.minecraft_version, result.core_version);
        } else if (!fabric_jar.empty()) {
            result.core_type = CoreType::FABRIC;
            result.server_jar_path = fabric_jar;
        } else {
            // 只有安装器 jar 时也能确定核心类型，但服务端尚未安装，
            // 不提供可启动的 jar，避免整合包被当作原版启动
            std::string installer_jar = neoforge_installer;
            std::string installer_prefix = "neoforge";
            CoreType installer_type = CoreType::NEOFORGE;
            if (installer_jar.empty()) {
                installer_jar = forge_installer;
                installer_prefix = "forge";
                installer_type = CoreType::FORGE;
            }
            if (!installer_jar.empty()) {
                result.core_type = installer_type;
                result.server_jar_path.clear();
                server_not_installed = true;
                parse_loader_jar_versions(installer_jar, installer_prefix,
                                          result.minecraft_version, result.core_version);
            } else {
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
            }
        }

        if (result.minecraft_version.empty()) {
            result.minecraft_version = extract_mc_version_from_version_json(base);
        }
    }

    if (!server_not_installed && result.server_jar_path.empty()) {
        result.server_jar_path = find_server_jar(base);
    }

    return result;
}

std::string find_installer_jar(const std::string &directory) {
    std::filesystem::path base = std::filesystem::absolute(directory);
    std::error_code ec;
    if (!std::filesystem::exists(base, ec) || !std::filesystem::is_directory(base, ec)) {
        return "";
    }
    // neoforge 必须先于 forge 匹配（"neoforge" 包含 "forge" 子串）；
    // 改名成 forge.jar 的安装器由 find_loader_jars 按内容识别
    std::string installer;
    find_loader_jars(base, "neoforge", nullptr, &installer);
    if (!installer.empty()) return installer;
    find_loader_jars(base, "forge", nullptr, &installer);
    return installer;
}

bool jar_is_installer(const std::string &jar_path) {
    std::string name = std::filesystem::path(jar_path).filename().string();
    for (auto &c : name) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
    if (name.find("installer") != std::string::npos) return true;
    return jar_looks_like_installer(jar_path);
}

} // namespace mc_manager::server
