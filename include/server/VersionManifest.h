#pragma once

#include <string>
#include <vector>

namespace mc_manager::server {

struct VersionInfo {
    std::string id;
    std::string url;
    std::string release_type;
};

struct CoreVersionInfo {
    std::string mc_version;
    std::string core_version;
    std::string download_url;
};

std::vector<VersionInfo> fetch_vanilla_versions();

std::vector<VersionInfo> fetch_paper_versions();

std::vector<CoreVersionInfo> fetch_neoforge_versions(const std::string &mc_version);

std::vector<VersionInfo> fetch_forge_versions();

std::vector<VersionInfo> fetch_fabric_versions();

std::string get_vanilla_server_url(const std::string &version_id);

std::string get_paper_download_url(const std::string &mc_version, const std::string &build);

} // namespace mc_manager::server
