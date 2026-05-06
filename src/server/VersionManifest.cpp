#include "server/VersionManifest.h"
#include "utils/Downloader.h"
#include <nlohmann/json.hpp>
#include <algorithm>
#include <map>
#include <sstream>

namespace mc_manager::server {

std::vector<VersionInfo> fetch_vanilla_versions() {
    std::vector<VersionInfo> result;
    auto j_opt = utils::http_get_json_parsed("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json");
    if (!j_opt) return result;
    auto &j = *j_opt;

    if (!j.contains("versions") || !j["versions"].is_array()) return result;

    for (const auto &v : j["versions"]) {
        VersionInfo info;
        info.id = v.value("id", "");
        info.url = v.value("url", "");
        info.release_type = v.value("type", "");
        if (!info.id.empty() && (info.release_type == "release" || info.release_type == "snapshot")) {
            result.push_back(std::move(info));
        }
    }

    return result;
}

std::string get_vanilla_server_url(const std::string &version_id) {
    auto j_opt = utils::http_get_json_parsed(
        "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json");
    if (!j_opt) return "";
    auto &j = *j_opt;

    if (!j.contains("versions")) return "";

    for (const auto &v : j["versions"]) {
        if (v.value("id", "") == version_id) {
            std::string version_url = v.value("url", "");
            if (version_url.empty()) return "";

            auto vj_opt = utils::http_get_json_parsed(version_url);
            if (!vj_opt) return "";
            auto &vj = *vj_opt;

            if (vj.contains("downloads") && vj["downloads"].contains("server")) {
                return vj["downloads"]["server"].value("url", "");
            }
            return "";
        }
    }

    return "";
}

std::vector<VersionInfo> fetch_paper_versions() {
    std::vector<VersionInfo> result;
    auto j_opt = utils::http_get_json_parsed("https://api.papermc.io/v2/projects/paper");
    if (!j_opt) return result;
    auto &j = *j_opt;

    if (!j.contains("versions")) return result;

    for (const auto &v : j["versions"]) {
        if (v.is_string()) {
            VersionInfo info;
            info.id = v.get<std::string>();
            info.release_type = "release";
            result.push_back(std::move(info));
        }
    }

    return result;
}

std::string get_paper_download_url(const std::string &mc_version, const std::string &build) {
    std::string url = "https://api.papermc.io/v2/projects/paper/versions/" + mc_version + "/builds/" + build;
    auto j_opt = utils::http_get_json_parsed(url);
    if (!j_opt) return "";
    auto &j = *j_opt;

    if (j.contains("downloads") && j["downloads"].contains("application")) {
        auto filename = j["downloads"]["application"].value("name", "");
        return "https://api.papermc.io/v2/projects/paper/versions/" +
               mc_version + "/builds/" + build + "/downloads/" + filename;
    }

    return "";
}

static std::string mc_version_to_neoforge_prefix(const std::string &mc_version) {
    std::string ver = mc_version;
    if (ver.find("1.") == 0) {
        ver = ver.substr(2);
    }
    size_t dots = std::count(ver.begin(), ver.end(), '.');
    if (dots == 0) {
        return ver + ".0.";
    } else {
        auto second_dot = ver.find('.', ver.find('.') + 1);
        return ver.substr(0, second_dot) + ".";
    }
}

std::vector<CoreVersionInfo> fetch_neoforge_versions(const std::string &mc_version) {
    std::vector<CoreVersionInfo> result;
    auto j_opt = utils::http_get_json_parsed(
        "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge");
    if (!j_opt) return result;
    auto &j = *j_opt;

    std::string neo_prefix = mc_version_to_neoforge_prefix(mc_version);

    if (!j.contains("versions")) return result;

    for (const auto &v : j["versions"]) {
        if (!v.is_string()) continue;
        std::string ver = v.get<std::string>();

        if (ver.find(neo_prefix) == 0) {
            CoreVersionInfo info;
            info.mc_version = mc_version;
            info.core_version = ver;
            info.download_url = "https://maven.neoforged.net/releases/net/neoforged/neoforge/" +
                                ver + "/neoforge-" + ver + "-installer.jar";
            result.push_back(std::move(info));
        }
    }

    return result;
}

static int parse_segment(const std::string &s) {
    try { return std::stoi(s); } catch (...) { return 0; }
}

static bool is_newer_version(const std::string &a, const std::string &b) {
    std::stringstream ss_a(a), ss_b(b);
    std::string seg_a, seg_b;

    while (true) {
        bool has_a = static_cast<bool>(std::getline(ss_a, seg_a, '.'));
        bool has_b = static_cast<bool>(std::getline(ss_b, seg_b, '.'));

        if (!has_a && !has_b) return false;
        if (!has_a) return false;
        if (!has_b) return true;

        int num_a = parse_segment(seg_a);
        int num_b = parse_segment(seg_b);
        if (num_a != num_b) return num_a > num_b;
    }
}

std::vector<VersionInfo> fetch_forge_versions() {
    std::vector<VersionInfo> result;

    auto body = utils::http_get("https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml");
    if (body.empty()) return result;

    const std::string tag_open = "<version>";
    const std::string tag_close = "</version>";

    std::map<std::string, std::string> latest_by_mc;
    size_t pos = 0;

    while ((pos = body.find(tag_open, pos)) != std::string::npos) {
        pos += tag_open.size();
        size_t end = body.find(tag_close, pos);
        if (end == std::string::npos) break;

        std::string full_version = body.substr(pos, end - pos);
        if (full_version.empty()) continue;

        auto dash_pos = full_version.find('-');
        if (dash_pos == std::string::npos) continue;

        std::string mc_version = full_version.substr(0, dash_pos);

        if (mc_version.size() < 4) continue;

        latest_by_mc[mc_version] = full_version;

        pos = end + tag_close.size();
    }

    result.reserve(latest_by_mc.size());
    for (const auto &[mc_version, full_version] : latest_by_mc) {
        VersionInfo info;
        info.id = full_version;
        info.url = mc_version;
        info.release_type = "release";
        result.push_back(std::move(info));
    }

    std::sort(result.begin(), result.end(),
              [](const VersionInfo &a, const VersionInfo &b) {
                  return is_newer_version(a.url, b.url);
              });

    return result;
}

std::vector<VersionInfo> fetch_fabric_versions() {
    std::vector<VersionInfo> result;
    auto j_opt = utils::http_get_json_parsed("https://meta.fabricmc.net/v2/versions/game");
    if (!j_opt) return result;
    auto &j = *j_opt;

    for (const auto &v : j) {
        VersionInfo info;
        info.id = v.value("version", "");
        info.release_type = v.value("stable", false) ? "release" : "snapshot";
        if (!info.id.empty()) {
            result.push_back(std::move(info));
        }
    }

    return result;
}

} // namespace mc_manager::server
