#pragma once

#include <string>
#include <functional>
#include "server/CoreType.h"
#include "server/CoreDetector.h"

namespace mc_manager::server {

struct InstallResult {
    bool success{false};
    std::string error;
    DetectionResult detection;
};

using InstallProgressCallback = std::function<void(const std::string &)>;

InstallResult install_vanilla(const std::string &mc_version,
                              const std::string &target_dir,
                              const std::string &java_path,
                              InstallProgressCallback progress = nullptr);

InstallResult install_paper(const std::string &mc_version,
                            const std::string &build_number,
                            const std::string &target_dir,
                            const std::string &java_path,
                            InstallProgressCallback progress = nullptr);

InstallResult install_neoforge(const std::string &mc_version,
                               const std::string &core_version,
                               const std::string &target_dir,
                               const std::string &java_path,
                               InstallProgressCallback progress = nullptr);

InstallResult install_forge(const std::string &mc_version,
                            const std::string &core_version,
                            const std::string &target_dir,
                            const std::string &java_path,
                            InstallProgressCallback progress = nullptr);

InstallResult install_fabric(const std::string &mc_version,
                             const std::string &installer_version,
                             const std::string &target_dir,
                             const std::string &java_path,
                             InstallProgressCallback progress = nullptr);

InstallResult run_installer_jar(const std::string &installer_jar_path,
                                const std::string &target_dir,
                                const std::string &java_path,
                                InstallProgressCallback progress = nullptr);

} // namespace mc_manager::server
