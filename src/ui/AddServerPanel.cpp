#include "ui/AddServerPanel.h"
#include "server/VersionManifest.h"
#include "server/CoreInstaller.h"
#include "server/CoreDetector.h"
#include "utils/StringUtils.h"
#include "utils/ExePath.h"
#include "ui/UiHelpers.h"
#include "ftxui/component/component.hpp"
#include "ftxui/component/event.hpp"
#include <filesystem>
#include <fstream>

using namespace ftxui;

namespace {

void restart_jthread(std::jthread &thread) {
    if (thread.joinable()) {
        thread.request_stop();
        thread.join();
    }
}

} // namespace

namespace mc_manager::ui {

void AddServerPanel::reset() {
    restart_jthread(version_fetch_thread_);
    restart_jthread(install_thread_);

    step_ = WizardStep::SELECT_METHOD;
    built_step_ = WizardStep::SELECT_METHOD;
    method_index_ = 0;
    core_type_index_ = 0;
    version_index_ = 0;
    server_name_.clear();
    java_path_ = "java";
    min_mem_ = "1024";
    max_mem_ = "4096";
    jvm_args_.clear();
    import_dir_path_.clear();
    installer_jar_path_.clear();
    jar_path_.clear();
    version_list_.clear();
    version_ids_.clear();
    versions_loading_ = false;
    versions_loaded_ = false;
    installing_ = false;
    install_output_.clear();
    install_success_ = false;
    install_error_.clear();
    configure_error_.clear();
    install_target_dir_.clear();
    log_export_message_.clear();
    if (slot_) {
        slot_->set_child(make_method_step());
    }
}

void AddServerPanel::start_version_fetch() {
    if (versions_loading_) return;

    restart_jthread(version_fetch_thread_);

    versions_loading_ = true;
    versions_loaded_ = false;
    version_list_.clear();
    version_ids_.clear();

    auto core_type = server::CORE_TYPES[core_type_index_].type;

    version_fetch_thread_ = std::jthread([this, core_type](std::stop_token token) {
        try {
        std::vector<std::string> names;
        std::vector<std::string> ids;

        switch (core_type) {
        case server::CoreType::VANILLA: {
            auto versions = server::fetch_vanilla_versions();
            for (auto &v : versions) {
                if (token.stop_requested()) return;
                if (v.release_type == "release") {
                    names.push_back(v.id + " (release)");
                    ids.push_back(v.id);
                }
            }
            break;
        }
        case server::CoreType::PAPER: {
            auto versions = server::fetch_paper_versions();
            for (auto &v : versions) {
                if (token.stop_requested()) return;
                names.push_back(v.id);
                ids.push_back(v.id);
            }
            break;
        }
        case server::CoreType::NEOFORGE: {
            names.push_back("1.21.1");
            ids.push_back("1.21.1");
            names.push_back("1.21");
            ids.push_back("1.21");
            names.push_back("1.20.6");
            ids.push_back("1.20.6");
            names.push_back("1.20.4");
            ids.push_back("1.20.4");
            names.push_back("1.20.1");
            ids.push_back("1.20.1");
            break;
        }
        case server::CoreType::FORGE: {
            auto versions = server::fetch_forge_versions();
            for (auto &v : versions) {
                if (token.stop_requested()) return;
                names.push_back(v.id);
                ids.push_back(v.id);
            }
            break;
        }
        case server::CoreType::FABRIC: {
            auto versions = server::fetch_fabric_versions();
            for (auto &v : versions) {
                if (token.stop_requested()) return;
                if (v.release_type == "release") {
                    names.push_back(v.id);
                    ids.push_back(v.id);
                }
            }
            break;
        }
        default:
            break;
        }

        if (token.stop_requested()) return;
        version_list_ = std::move(names);
        version_ids_ = std::move(ids);
        versions_loading_ = false;
        versions_loaded_ = true;
        } catch (...) {}
    });
}

void AddServerPanel::start_install() {
    restart_jthread(install_thread_);

    installing_ = true;
    install_output_.clear();
    install_success_ = false;
    install_error_.clear();
    log_export_message_.clear();

    auto core_type = server::CORE_TYPES[core_type_index_].type;
    std::string parent = servers_parent_folder_.empty() ? utils::get_exe_dir() + "/server" : servers_parent_folder_;
    std::string target_dir = std::filesystem::path(parent) / server_name_;
    install_target_dir_ = target_dir;
    std::string java = java_path_.empty() ? "java" : java_path_;

    install_thread_ = std::jthread([this, core_type, target_dir, java](std::stop_token token) {
        try {
        server::InstallResult result;
        auto progress = [this](const std::string &line) {
            std::lock_guard<std::mutex> lock(install_output_mutex_);
            install_output_ += line + "\n";
        };

        switch (method_) {
        case AddMethod::AUTO_DOWNLOAD: {
            std::string version_id;
            if (version_index_ >= 0 && version_index_ < static_cast<int>(version_ids_.size())) {
                version_id = version_ids_[version_index_];
            }

            switch (core_type) {
            case server::CoreType::VANILLA:
                if (token.stop_requested()) return;
                result = server::install_vanilla(version_id, target_dir, java, progress);
                break;
            case server::CoreType::PAPER:
                if (token.stop_requested()) return;
                result = server::install_paper(version_id, "latest", target_dir, java, progress);
                break;
            case server::CoreType::NEOFORGE: {
                if (token.stop_requested()) return;
                auto neo_versions = server::fetch_neoforge_versions(version_id);
                if (!neo_versions.empty()) {
                    result = server::install_neoforge(version_id, neo_versions.back().core_version,
                                                      target_dir, java, progress);
                } else {
                    result.error = "No NeoForge version found for " + version_id;
                }
                break;
            }
            case server::CoreType::FORGE:
                if (token.stop_requested()) return;
                result = server::install_forge(version_id, version_id, target_dir, java, progress);
                break;
            case server::CoreType::FABRIC:
                if (token.stop_requested()) return;
                result = server::install_fabric(version_id, "1.0.1", target_dir, java, progress);
                break;
            default:
                result.error = "Unsupported core type for auto download";
                break;
            }
            break;
        }
        case AddMethod::PROVIDE_INSTALLER: {
            if (token.stop_requested()) return;
            result = server::run_installer_jar(installer_jar_path_, target_dir, java, progress);
            break;
        }
        case AddMethod::IMPORT_DIRECTORY: {
            if (token.stop_requested()) return;
            auto detection = server::detect_core(import_dir_path_);
            if (detection) {
                result.success = true;
                result.detection = *detection;
            } else {
                result.error = "Failed to detect core type in " + import_dir_path_;
            }
            break;
        }
        case AddMethod::PROVIDE_JAR: {
            if (token.stop_requested()) return;
            std::filesystem::create_directories(target_dir);
            std::filesystem::path source(jar_path_);
            auto dest = std::filesystem::path(target_dir) / source.filename();
            std::error_code ec;
            std::filesystem::copy_file(source, dest,
                                        std::filesystem::copy_options::overwrite_existing, ec);
            if (ec) {
                result.error = "Failed to copy JAR: " + ec.message();
            } else {
                auto detection = server::detect_core(target_dir);
                if (detection) {
                    result.success = true;
                    result.detection = *detection;
                } else {
                    result.success = true;
                    result.detection.core_type = core_type;
                    result.detection.server_jar_path = dest.string();
                }
            }
            break;
        }
        }

        if (token.stop_requested()) return;
        install_success_ = result.success;
        install_error_ = result.error;
        detection_result_ = result.detection;
        installing_ = false;
        step_ = WizardStep::DONE;
        } catch (...) {
            install_error_ = "Installation thread crashed";
            install_success_ = false;
            installing_ = false;
            step_ = WizardStep::DONE;
        }
    });
}

void AddServerPanel::finish_wizard() {
    if (!on_complete_) return;

    config::ServerConfig sc;
    sc.name = server_name_;
    sc.java_path = java_path_.empty() ? "java" : java_path_;
    sc.core_type = detection_result_.core_type;
    sc.minecraft_version = detection_result_.minecraft_version;
    sc.core_version = detection_result_.core_version;
    sc.launch_args_file = detection_result_.launch_args_file;
    sc.additional_jvm_args = jvm_args_;

    sc.min_memory_mb = utils::safe_stoi(min_mem_, 1024);
    sc.max_memory_mb = utils::safe_stoi(max_mem_, 4096);

    if (method_ == AddMethod::IMPORT_DIRECTORY) {
        // 规范化导入路径（解析符号链接、去掉尾部分隔符与相对成分），
        // 保证重连时与 tmux 汇报的 pane 路径一致
        std::error_code ec;
        std::filesystem::path canonical_dir = std::filesystem::weakly_canonical(import_dir_path_, ec);
        std::string dir = (ec ? std::filesystem::absolute(import_dir_path_) : canonical_dir).string();
        sc.server_jar_path = detection_result_.server_jar_path;
        sc.working_dir = dir;
        sc.backup_dir = (std::filesystem::path(dir) / "backups").string();
        sc.world_dir = (std::filesystem::path(dir) / "world").string();
    } else {
        std::string parent = servers_parent_folder_.empty() ? utils::get_exe_dir() + "/server" : servers_parent_folder_;
        std::filesystem::path server_dir = std::filesystem::absolute(std::filesystem::path(parent) / server_name_);

        auto &info = server::get_core_type_info(detection_result_.core_type);
        if (!detection_result_.server_jar_path.empty()) {
            sc.server_jar_path = detection_result_.server_jar_path;
        } else if (!info.uses_launch_args_file) {
            // For types that launch with -jar, try to find the actual jar
            auto detection = server::detect_core(server_dir.string());
            if (detection && !detection->server_jar_path.empty()) {
                sc.server_jar_path = detection->server_jar_path;
            }
        }

        sc.working_dir = server_dir.string();
        sc.backup_dir = (server_dir / "backups").string();
        sc.world_dir = (server_dir / "world").string();
    }

    on_complete_(sc);
}

void AddServerPanel::export_install_log() {
    if (install_target_dir_.empty()) {
        log_export_message_ = "错误：未找到安装目录";
        return;
    }
    std::filesystem::path log_path = std::filesystem::path(install_target_dir_) / "install_log.txt";
    std::error_code ec;
    std::filesystem::create_directories(install_target_dir_, ec);
    if (ec) {
        log_export_message_ = "错误：无法创建目录 - " + ec.message();
        return;
    }
    std::ofstream file(log_path);
    if (!file.is_open()) {
        log_export_message_ = "错误：无法写入文件 " + log_path.string();
        return;
    }
    std::string output;
    {
        std::lock_guard<std::mutex> lock(install_output_mutex_);
        output = install_output_;
    }
    file << output;
    file.close();
    log_export_message_ = "日志已保存到: " + log_path.string();
}

void AddServerPanel::rebuild_if_needed() {
    if (built_step_ == step_) return;
    built_step_ = step_;
    if (!slot_) return;
    switch (step_) {
    case WizardStep::SELECT_METHOD:  slot_->set_child(make_method_step()); break;
    case WizardStep::SELECT_CORE:    slot_->set_child(make_core_step()); break;
    case WizardStep::SELECT_VERSION: slot_->set_child(make_version_step()); break;
    case WizardStep::CONFIGURE:      slot_->set_child(make_configure_step()); break;
    case WizardStep::INSTALLING:     slot_->set_child(make_installing_step()); break;
    case WizardStep::DONE:           slot_->set_child(make_done_step()); break;
    }
}

Component AddServerPanel::make_cancel_button() {
    return Button("取消", [this] { if (on_cancel_) on_cancel_(); });
}

Component AddServerPanel::make_navigation_buttons(
    bool has_back, bool has_next,
    std::function<void()> next_action,
    std::function<void()> back_action)
{
    Components buttons;
    if (has_next && next_action) {
        buttons.push_back(Button("下一步", next_action));
    }
    if (has_back && back_action) {
        buttons.push_back(Button("上一步", back_action));
    }
    buttons.push_back(make_cancel_button());
    return Container::Horizontal(std::move(buttons));
}

Component AddServerPanel::wrap_wizard_step(Element title, Component body, bool needs_border) {
    return Renderer(body, [body, title = std::move(title), needs_border] {
        auto content = vbox({
            title,
            separator(),
            body ? body->Render() : text(""),
        });
        return needs_border ? (content | border) : content;
    });
}

Component AddServerPanel::wrap_wizard_step(const std::string& title, Component body, bool needs_border) {
    return wrap_wizard_step(text(title) | bold, body, needs_border);
}

Component AddServerPanel::make_method_step() {
    auto toggle = Toggle(&method_names_, &method_index_);
    auto buttons = make_navigation_buttons(
        false, true,
        [this] {
            method_ = static_cast<AddMethod>(method_index_);
            if (method_ == AddMethod::IMPORT_DIRECTORY ||
                method_ == AddMethod::PROVIDE_INSTALLER ||
                method_ == AddMethod::PROVIDE_JAR) {
                step_ = WizardStep::CONFIGURE;
            } else {
                step_ = WizardStep::SELECT_CORE;
            }
            rebuild_if_needed();
        },
        nullptr);

    auto container = Container::Vertical({toggle, buttons});
    container->SetActiveChild(toggle);

    return wrap_wizard_step(" 添加服务器 - 选择方式",
        Renderer(container, [container] {
            return vbox({
                text(" ↑↓切换, Tab到按钮, Enter确认"),
                text(""),
                text(" ● 自动下载：从网络下载并安装服务端核心") | dim,
                text(" ● 导入目录：导入已有的服务器目录") | dim,
                text(" ● 安装器 JAR：提供安装器 JAR，自动运行安装") | dim,
                text(" ● 核心 JAR：直接提供服务端核心 JAR 文件") | dim,
                separator(),
                container->Render(),
            });
        }));
}

Component AddServerPanel::make_core_step() {
    if (core_type_names_.empty()) {
        for (const auto &ct : server::CORE_TYPES) {
            if (ct.type != server::CoreType::CUSTOM) {
                core_type_names_.emplace_back(ct.display_name);
            }
        }
    }

    auto toggle = Toggle(&core_type_names_, &core_type_index_);
    auto buttons = make_navigation_buttons(
        true, true,
        [this] {
            step_ = WizardStep::SELECT_VERSION;
            version_index_ = 0;
            start_version_fetch();
            rebuild_if_needed();
        },
        [this] {
            step_ = WizardStep::SELECT_METHOD;
            rebuild_if_needed();
        });

    auto container = Container::Vertical({toggle, buttons});
    container->SetActiveChild(toggle);

    return wrap_wizard_step(" 添加服务器 - 选择核心类型",
        Renderer(container, [container] {
            return vbox({
                text(" ↑↓切换, Tab到按钮"),
                text(""),
                container->Render(),
            });
        }));
}

Component AddServerPanel::make_version_step() {
    auto buttons = make_navigation_buttons(
        true, true,
        [this] { step_ = WizardStep::CONFIGURE; rebuild_if_needed(); },
        [this] { step_ = WizardStep::SELECT_CORE; rebuild_if_needed(); });

    auto version_list_comp = Renderer([this] {
        Elements items;
        for (int i = 0; i < static_cast<int>(version_list_.size()); i++) {
            auto row = hbox({
                text(i == version_index_ ? " \u25b6 " : "   "),
                text(version_list_[i]),
            });
            if (i == version_index_) {
                row = row | inverted | focus;
            }
            items.push_back(row);
        }
        if (items.empty()) {
            items.push_back(text(" 无版本") | dim);
        }
        return vbox(items) | frame | vscroll_indicator;
    });

    version_list_comp |= CatchEvent([this](Event event) -> bool {
        if (!versions_loaded_ || version_list_.empty()) return false;
        if (event == Event::ArrowUp) {
            if (version_index_ > 0) version_index_--;
            return true;
        }
        if (event == Event::ArrowDown) {
            if (version_index_ < static_cast<int>(version_list_.size()) - 1)
                version_index_++;
            return true;
        }
        if (event == Event::Return && !version_list_.empty()) {
            step_ = WizardStep::CONFIGURE;
            rebuild_if_needed();
            return true;
        }
        return false;
    });

    auto container = Container::Vertical({
        version_list_comp | flex,
        buttons,
    });
    container->SetActiveChild(container->ChildAt(0));

    return wrap_wizard_step(" 添加服务器 - 选择版本",
        Renderer(container, [container, this] {
            Elements items;
            if (versions_loading_) {
                items.push_back(text(" 正在获取版本列表...") | color(Color::Yellow));
                items.push_back(container->Render());
            } else if (versions_loaded_ && version_list_.empty()) {
                items.push_back(text(" 未找到可用版本") | color(Color::Red));
                items.push_back(container->Render());
            } else if (versions_loaded_) {
                items.push_back(text(" \u2191\u2193选择, Tab到按钮, Enter直接下一步"));
                items.push_back(container->Render());
            }
            return vbox(std::move(items));
        }));
}

Component AddServerPanel::make_configure_step() {
    auto name_input = Input(&server_name_, "服务器名称");
    auto java_input = Input(&java_path_, "Java 路径");
    auto min_mem_input = Input(&min_mem_, "最小内存 (MB)");
    auto max_mem_input = Input(&max_mem_, "最大内存 (MB)");
    auto jvm_input = Input(&jvm_args_, "额外 JVM 参数 (可选)");

    bool needs_path_input = (method_ == AddMethod::IMPORT_DIRECTORY ||
                             method_ == AddMethod::PROVIDE_INSTALLER ||
                             method_ == AddMethod::PROVIDE_JAR);

    std::string path_label;
    switch (method_) {
    case AddMethod::IMPORT_DIRECTORY:
        path_label = "服务器目录路径";
        break;
    case AddMethod::PROVIDE_INSTALLER:
        path_label = "安装器 JAR 文件路径";
        break;
    case AddMethod::PROVIDE_JAR:
        path_label = "核心 JAR 文件路径";
        break;
    default:
        break;
    }

    Component dir_or_jar_input;
    if (needs_path_input) {
        dir_or_jar_input = (method_ == AddMethod::IMPORT_DIRECTORY)
            ? Input(&import_dir_path_, path_label)
            : (method_ == AddMethod::PROVIDE_INSTALLER
                ? Input(&installer_jar_path_, path_label)
                : Input(&jar_path_, path_label));
    }

    auto install_btn = Button("开始安装", [this] {
        if (server_name_.empty()) { configure_error_ = "请输入服务器名称"; return; }
        if (method_ == AddMethod::PROVIDE_INSTALLER && installer_jar_path_.empty()) { configure_error_ = "请输入安装器 JAR 文件路径"; return; }
        if (method_ == AddMethod::PROVIDE_JAR && jar_path_.empty()) { configure_error_ = "请输入核心 JAR 文件路径"; return; }
        if (method_ == AddMethod::IMPORT_DIRECTORY && import_dir_path_.empty()) { configure_error_ = "请输入服务器目录路径"; return; }
        if (java_path_.empty()) java_path_ = "java";
        configure_error_.clear();
        step_ = WizardStep::INSTALLING;
        start_install();
        rebuild_if_needed();
    });
    auto back_btn = Button("上一步", [this] {
        if (method_ == AddMethod::AUTO_DOWNLOAD) {
            step_ = WizardStep::SELECT_VERSION;
        } else {
            step_ = WizardStep::SELECT_METHOD;
        }
        rebuild_if_needed();
    });
    auto cancel_btn = make_cancel_button();

    auto button_row = Container::Horizontal({install_btn, back_btn, cancel_btn});

    Components container_children;
    container_children.push_back(name_input);
    if (needs_path_input && dir_or_jar_input) {
        container_children.push_back(dir_or_jar_input);
    }
    container_children.push_back(java_input);
    container_children.push_back(min_mem_input);
    container_children.push_back(max_mem_input);
    container_children.push_back(jvm_input);
    container_children.push_back(button_row);

    auto container = Container::Vertical(std::move(container_children));
    container->SetActiveChild(name_input);

    auto inner = container;
    container = CatchEvent(inner, [this, inner, button_row](Event event) -> bool {
        if (event == Event::Return) {
            if (inner->ActiveChild() == button_row) return false;
            if (server_name_.empty()) {
                configure_error_ = "请输入服务器名称";
                return true;
            }
            if (method_ == AddMethod::PROVIDE_INSTALLER && installer_jar_path_.empty()) {
                configure_error_ = "请输入安装器 JAR 文件路径";
                return true;
            }
            if (method_ == AddMethod::PROVIDE_JAR && jar_path_.empty()) {
                configure_error_ = "请输入核心 JAR 文件路径";
                return true;
            }
            if (method_ == AddMethod::IMPORT_DIRECTORY && import_dir_path_.empty()) {
                configure_error_ = "请输入服务器目录路径";
                return true;
            }
            if (java_path_.empty()) java_path_ = "java";
            configure_error_.clear();
            step_ = WizardStep::INSTALLING;
            start_install();
            rebuild_if_needed();
            return true;
        }
        if (event.is_character()) {
            configure_error_.clear();
        }
        return false;
    });

    return wrap_wizard_step(" 添加服务器 - 配置",
        Renderer(container, [container, this] {
            auto &info = server::CORE_TYPES[core_type_index_];

            Elements items;
            if (method_ == AddMethod::AUTO_DOWNLOAD) {
                std::string ver = (version_index_ >= 0 && version_index_ < static_cast<int>(version_ids_.size()))
                    ? version_ids_[version_index_] : "?";
                items.push_back(text(" 核心类型: " + std::string(info.display_name) + "  版本: " + ver) | color(Color::Cyan));
            }

            if (!configure_error_.empty()) {
                items.push_back(text(" " + configure_error_) | color(Color::Red) | bold);
            }

            items.push_back(container->Render());
            return vbox(std::move(items));
        }));
}

Component AddServerPanel::make_installing_step() {
    return wrap_wizard_step(text(" 安装中...") | bold | color(Color::Yellow),
        Renderer([this] {
            std::string output;
            {
                std::lock_guard<std::mutex> lock(install_output_mutex_);
                output = install_output_;
            }

            Elements items;
            auto lines = utils::split(output, '\n');
            int start = static_cast<int>(lines.size()) > 15 ? static_cast<int>(lines.size()) - 15 : 0;
            for (int i = start; i < static_cast<int>(lines.size()); ++i) {
                items.push_back(text(" " + lines[i]) | dim);
            }

            return vbox(std::move(items));
        }));
}

Component AddServerPanel::make_done_step() {
    auto complete_btn = Button("完成", [this] { finish_wizard(); });
    auto back_btn = Button("上一步", [this] {
        step_ = WizardStep::CONFIGURE;
        rebuild_if_needed();
    });
    auto cancel_btn = make_cancel_button();
    auto export_btn = Button("导出日志", [this] {
        export_install_log();
        rebuild_if_needed();
    });

    auto success_buttons = Container::Horizontal({complete_btn, cancel_btn});
    auto fail_buttons = Container::Horizontal({export_btn, back_btn, cancel_btn});

    Component active_buttons = install_success_
        ? std::move(success_buttons)
        : std::move(fail_buttons);

    return wrap_wizard_step(" 安装结果",
        Renderer(active_buttons, [active_buttons, this] {
            Elements items;
            if (install_success_) {
                items.push_back(text(" 安装成功！") | bold | color(Color::Green));
                items.push_back(text(" 核心类型: " + std::string(server::get_core_type_info(detection_result_.core_type).display_name)));
                if (!detection_result_.minecraft_version.empty()) {
                    items.push_back(text(" MC 版本: " + detection_result_.minecraft_version));
                }
                if (!detection_result_.core_version.empty()) {
                    items.push_back(text(" 核心版本: " + detection_result_.core_version));
                }
                if (!detection_result_.launch_args_file.empty()) {
                    items.push_back(text(" 启动参数文件: " + detection_result_.launch_args_file) | dim);
                }
            } else {
                items.push_back(text(" 安装失败！") | bold | color(Color::Red));
                items.push_back(text(" 错误: " + install_error_) | color(Color::Red));
            }

            if (!log_export_message_.empty()) {
                items.push_back(text(" " + log_export_message_) | color(Color::Cyan));
            }

            items.push_back(separator());
            items.push_back(active_buttons->Render());
            return vbox(std::move(items));
        }));
}
Component AddServerPanel::GetComponent() {
    slot_ = std::make_shared<DynamicSlot>();
    ftxui::Component slot_comp = slot_;
    slot_->set_child(make_method_step());
    built_step_ = WizardStep::SELECT_METHOD;

    auto wrapper = CatchEvent(slot_comp, [this](Event event) {
        if (built_step_ != step_) {
            rebuild_if_needed();
        }
        return false;
    });

    return wrapper;
}

} // namespace mc_manager::ui
