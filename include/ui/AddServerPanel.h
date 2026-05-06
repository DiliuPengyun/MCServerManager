#pragma once

#include <string>
#include <vector>
#include <functional>
#include <atomic>
#include <thread>
#include <mutex>
#include "server/CoreType.h"
#include "server/CoreDetector.h"
#include "config/AppConfig.h"
#include "ftxui/component/component.hpp"
#include "ui/DynamicSlot.h"

namespace mc_manager::ui {

class AddServerPanel {
public:
    enum class AddMethod {
        AUTO_DOWNLOAD,
        IMPORT_DIRECTORY,
        PROVIDE_INSTALLER,
        PROVIDE_JAR
    };

    enum class WizardStep {
        SELECT_METHOD,
        SELECT_CORE,
        SELECT_VERSION,
        CONFIGURE,
        INSTALLING,
        DONE
    };

    using OnCompleteCallback = std::function<void(const config::ServerConfig &)>;
    using OnCancelCallback = std::function<void()>;

    ftxui::Component GetComponent();
    void reset();

    void set_on_complete(OnCompleteCallback callback) { on_complete_ = std::move(callback); }
    void set_on_cancel(OnCancelCallback callback) { on_cancel_ = std::move(callback); }
    void set_servers_parent_folder(const std::string &folder) { servers_parent_folder_ = folder; }
    void set_global_java_path(const std::string &path) { java_path_ = path; }

private:
    std::atomic<WizardStep> step_{WizardStep::SELECT_METHOD};
    AddMethod method_{AddMethod::AUTO_DOWNLOAD};

    int method_index_{0};
    int core_type_index_{0};
    int version_index_{0};

    std::string server_name_;
    std::string java_path_{"java"};
    std::string min_mem_{"1024"};
    std::string max_mem_{"4096"};
    std::string jvm_args_;

    std::string import_dir_path_;
    std::string installer_jar_path_;
    std::string jar_path_;

    std::vector<std::string> version_list_;
    std::vector<std::string> version_ids_;
    std::atomic<bool> versions_loading_{false};
    std::atomic<bool> versions_loaded_{false};

    std::atomic<bool> installing_{false};
    std::string install_output_;
    std::mutex install_output_mutex_;
    std::atomic<bool> install_success_{false};
    std::string install_error_;
    std::string configure_error_;
    std::string install_target_dir_;
    std::string log_export_message_;
    server::DetectionResult detection_result_;
    std::string servers_parent_folder_;

    std::jthread version_fetch_thread_;
    std::jthread install_thread_;

    OnCompleteCallback on_complete_;
    OnCancelCallback on_cancel_;

    std::vector<std::string> method_names_{"自动下载", "导入已有目录", "提供安装器 JAR", "提供核心 JAR"};
    std::vector<std::string> core_type_names_;

    std::shared_ptr<DynamicSlot> slot_;
    WizardStep built_step_{WizardStep::SELECT_METHOD};

    void start_version_fetch();
    void start_install();
    void finish_wizard();
    void export_install_log();
    void rebuild_if_needed();

    ftxui::Component make_cancel_button();
    ftxui::Component make_navigation_buttons(
        bool has_back, bool has_next,
        std::function<void()> next_action,
        std::function<void()> back_action);
    ftxui::Component wrap_wizard_step(
        ftxui::Element title, ftxui::Component body,
        bool needs_border = true);
    ftxui::Component wrap_wizard_step(
        const std::string& title, ftxui::Component body,
        bool needs_border = true);
    ftxui::Component make_method_step();
    ftxui::Component make_core_step();
    ftxui::Component make_version_step();
    ftxui::Component make_configure_step();
    ftxui::Component make_installing_step();
    ftxui::Component make_done_step();
};

} // namespace mc_manager::ui
