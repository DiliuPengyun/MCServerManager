#include "server/CoreInstaller.h"
#include "server/VersionManifest.h"
#include "utils/Downloader.h"
#include "utils/ProcessUtils.h"
#include <filesystem>
#include <fstream>
#include <vector>
#include <unistd.h>

namespace mc_manager::server {

struct ScopedChdir {
    std::string old_cwd;

    explicit ScopedChdir(const std::string &dir) {
        if (!dir.empty()) {
            std::array<char, 4096> cwd_buf;
            if (getcwd(cwd_buf.data(), cwd_buf.size())) {
                old_cwd = cwd_buf.data();
            }
            chdir(dir.c_str());
        }
    }

    ~ScopedChdir() {
        if (!old_cwd.empty()) {
            chdir(old_cwd.c_str());
        }
    }
};

struct JavaInstallResult {
    bool success;
    std::string error;
};

static JavaInstallResult run_java_installer(const std::string &java_path,
                                            const std::string &jar_path,
                                            const std::string &working_dir,
                                            InstallProgressCallback progress) {
    std::string abs_jar = std::filesystem::absolute(jar_path).string();
    // headless 兜底：个别安装器变体即使带 --installServer 也可能触碰 AWT
    std::string cmd = java_path + " -Djava.awt.headless=true -jar \"" + abs_jar + "\" --installServer 2>&1";
    if (progress) progress("Running installer: " + cmd);

    ScopedChdir guard(working_dir);

    std::string last_line;
    std::vector<std::string> last_lines;
    int status = utils::exec_streaming(cmd, [&](const std::string &line) {
        last_line = line;
        last_lines.push_back(line);
        if (last_lines.size() > 5) last_lines.erase(last_lines.begin());
        if (progress) progress(line);
    });

    if (status == -1) {
        if (progress) progress("Failed to execute installer");
        return {false, "Failed to launch Java process"};
    }

    if (status != 0) {
        std::string err = "Installer exited with status " + std::to_string(status);
        if (!last_lines.empty()) {
            err += ": ";
            for (size_t i = 0; i < last_lines.size(); ++i) {
                if (i > 0) err += "\n  ";
                err += last_lines[i];
            }
        }
        if (progress) progress(err);
        return {false, err};
    }

    return {true, ""};
}

static bool download_or_fail(const std::string &url,
                              const std::string &dest_path,
                              InstallResult &result) {
    auto dl = utils::download_file(url, dest_path);
    if (!dl.success) {
        result.error = "Download failed: " + dl.error;
        return false;
    }
    return true;
}

static void detect_or_default(InstallResult &result,
                               const std::string &target_dir,
                               CoreType default_type) {
    auto detection = detect_core(target_dir);
    if (detection) {
        result.detection = *detection;
    } else {
        result.detection.core_type = default_type;
    }
}

static void ensure_user_jvm_args(const std::string &target_dir,
                                  int min_memory_mb,
                                  int max_memory_mb) {
    std::filesystem::path args_path = std::filesystem::path(target_dir) / "user_jvm_args.txt";
    if (std::filesystem::exists(args_path)) return;

    std::ofstream f(args_path);
    if (!f.is_open()) return;
    f << "# Xmx and Xms set the maximum and minimum RAM usage, respectively.\n";
    f << "# They can take any number, followed by an M or a G.\n";
    f << "# M means Megabyte, G means Gigabyte.\n";
    f << "# For example, to set the maximum to 3GB: -Xmx3G\n";
    f << "# To set the minimum to 2.5GB: -Xms2500M\n";
    f << "# A good default for a modded server is 4GB.\n";
    f << "# Uncomment the next line to set it.\n";
    f << "# -Xmx4G\n";
    f.close();
}

InstallResult install_vanilla(const std::string &mc_version,
                              const std::string &target_dir,
                              const std::string &java_path,
                              InstallProgressCallback progress) {
    InstallResult result;

    if (progress) progress("Fetching server URL for " + mc_version + "...");
    std::string url = get_vanilla_server_url(mc_version);
    if (url.empty()) {
        result.error = "Failed to get download URL for Vanilla " + mc_version;
        return result;
    }

    std::filesystem::create_directories(target_dir);
    std::string jar_path = std::filesystem::path(target_dir) / ("server.jar");

    if (progress) progress("Downloading Vanilla " + mc_version + "...");
    if (!download_or_fail(url, jar_path, result)) return result;

    if (progress) progress("Verifying installation...");
    detect_or_default(result, target_dir, CoreType::VANILLA);
    if (result.detection.minecraft_version.empty()) result.detection.minecraft_version = mc_version;
    if (result.detection.server_jar_path.empty()) result.detection.server_jar_path = jar_path;

    result.success = true;
    if (progress) progress("Vanilla " + mc_version + " installed successfully!");
    return result;
}

InstallResult install_paper(const std::string &mc_version,
                            const std::string &build_number,
                            const std::string &target_dir,
                            const std::string &java_path,
                            InstallProgressCallback progress) {
    InstallResult result;

    if (progress) progress("Fetching Paper " + mc_version + " build " + build_number + "...");
    std::string url = get_paper_download_url(mc_version, build_number);
    if (url.empty()) {
        result.error = "Failed to get download URL for Paper " + mc_version + " build " + build_number;
        return result;
    }

    std::filesystem::create_directories(target_dir);
    std::string jar_path = std::filesystem::path(target_dir) / ("paper-" + mc_version + "-" + build_number + ".jar");

    if (progress) progress("Downloading Paper...");
    if (!download_or_fail(url, jar_path, result)) return result;

    detect_or_default(result, target_dir, CoreType::PAPER);
    if (result.detection.minecraft_version.empty()) result.detection.minecraft_version = mc_version;
    if (result.detection.core_version.empty()) result.detection.core_version = build_number;
    if (result.detection.server_jar_path.empty()) result.detection.server_jar_path = jar_path;

    result.success = true;
    if (progress) progress("Paper installed successfully!");
    return result;
}

static InstallResult install_forge_like(const std::string &org_domain,
                                              const std::string &org_path,
                                              const std::string &core_name,
                                              const std::string &display_name,
                                              const std::string &version,
                                              const std::string &target_dir,
                                              const std::string &java_path,
                                              InstallProgressCallback progress) {
    InstallResult result;

    std::string download_url = "https://maven." + org_domain + "/releases/net/" +
                                org_path + "/" + core_name + "/" + version + "/" +
                                core_name + "-" + version + "-installer.jar";

    std::filesystem::create_directories(target_dir);
    std::string installer_path = std::filesystem::path(target_dir) /
                                 (core_name + "-" + version + "-installer.jar");

    if (progress) progress("Downloading " + display_name + " " + version + " installer...");
    if (!download_or_fail(download_url, installer_path, result)) return result;

    if (progress) progress("Running " + display_name + " installer (this may take several minutes)...");
    auto jresult = run_java_installer(java_path, installer_path, target_dir, progress);
    if (!jresult.success) {
        result.error = jresult.error;
        return result;
    }

    if (progress) progress("Detecting installed core...");
    auto detection = detect_core(target_dir);
    if (!detection) {
        result.error = "Failed to detect " + display_name + " installation";
        return result;
    }
    result.detection = *detection;

    result.success = true;
    if (progress) progress(display_name + " " + version + " installed successfully!");
    ensure_user_jvm_args(target_dir, 1024, 4096);
    return result;
}

InstallResult install_neoforge(const std::string &mc_version,
                               const std::string &core_version,
                               const std::string &target_dir,
                               const std::string &java_path,
                               InstallProgressCallback progress) {
    return install_forge_like("neoforged.net", "neoforged", "neoforge", "NeoForge",
                              core_version, target_dir, java_path, progress);
}

InstallResult install_forge(const std::string &mc_version,
                            const std::string &core_version,
                            const std::string &target_dir,
                            const std::string &java_path,
                            InstallProgressCallback progress) {
    return install_forge_like("minecraftforge.net", "minecraftforge", "forge", "Forge",
                              mc_version, target_dir, java_path, progress);
}

InstallResult install_fabric(const std::string &mc_version,
                             const std::string &installer_version,
                             const std::string &target_dir,
                             const std::string &java_path,
                             InstallProgressCallback progress) {
    InstallResult result;

    std::string download_url = "https://maven.fabricmc.net/net/fabricmc/fabric-installer/" +
                               installer_version + "/fabric-installer-" + installer_version + ".jar";

    std::filesystem::create_directories(target_dir);
    std::string installer_path = std::filesystem::path(target_dir) / ("fabric-installer-" + installer_version + ".jar");

    if (progress) progress("Downloading Fabric installer...");
    if (!download_or_fail(download_url, installer_path, result)) return result;

    std::string abs_installer = std::filesystem::absolute(installer_path).string();
    std::string cmd = java_path + " -jar \"" + abs_installer +
                      "\" server -mcversion " + mc_version +
                      " -dir \"" + target_dir + "\" 2>&1";

    if (progress) progress("Running Fabric installer...");
    ScopedChdir guard(target_dir);

    int status = utils::exec_streaming(cmd, [&](const std::string &line) {
        if (progress) progress(line);
    });

    if (status != 0) {
        result.error = "Fabric installer exited with status " + std::to_string(status);
        return result;
    }

    // Download vanilla Minecraft server JAR (Fabric requires it to run)
    if (progress) progress("Downloading Minecraft server JAR...");
    std::string vanilla_url = get_vanilla_server_url(mc_version);
    if (vanilla_url.empty()) {
        result.error = "Failed to get vanilla server URL for version " + mc_version;
        return result;
    }

    std::string server_jar_path = std::filesystem::path(target_dir) / "server.jar";
    if (!download_or_fail(vanilla_url, server_jar_path, result)) return result;

    detect_or_default(result, target_dir, CoreType::FABRIC);
    if (result.detection.minecraft_version.empty()) result.detection.minecraft_version = mc_version;

    result.success = true;
    if (progress) progress("Fabric installed successfully!");
    return result;
}

InstallResult run_installer_jar(const std::string &installer_jar_path,
                                const std::string &target_dir,
                                const std::string &java_path,
                                InstallProgressCallback progress) {
    InstallResult result;

    std::filesystem::path source(installer_jar_path);
    std::filesystem::path dest = std::filesystem::path(target_dir) / source.filename();

    std::error_code ec;
    std::filesystem::create_directories(target_dir, ec);
    if (ec) {
        result.error = "Failed to create target directory: " + ec.message();
        return result;
    }

    if (source != dest) {
        std::filesystem::copy_file(source, dest,
                                    std::filesystem::copy_options::overwrite_existing, ec);
        if (ec) {
            result.error = "Failed to copy installer: " + ec.message();
            return result;
        }
    }

    if (progress) progress("Running installer in " + target_dir + "...");
    auto jresult = run_java_installer(java_path, dest.string(), target_dir, progress);
    if (!jresult.success) {
        result.error = jresult.error;
        return result;
    }

    auto rdetection = detect_core(target_dir);
    if (!rdetection) {
        result.error = "Failed to detect installed core";
        return result;
    }
    result.detection = *rdetection;

    result.success = true;
    if (progress) progress("Installation completed successfully!");
    ensure_user_jvm_args(target_dir, 1024, 4096);
    return result;
}

} // namespace mc_manager::server
