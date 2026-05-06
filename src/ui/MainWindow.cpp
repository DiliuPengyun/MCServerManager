#include "ui/MainWindow.h"
#include "ui/DynamicSlot.h"
#include "ui/StateDisplayInfo.h"
#include "ui/UiHelpers.h"
#include "ftxui/component/component.hpp"
#include "ftxui/component/event.hpp"
#include "ftxui/component/screen_interactive.hpp"
#include "utils/StringUtils.h"
#include "server/EulaChecker.h"
#include <filesystem>
#include <thread>

using namespace ftxui;

namespace mc_manager::ui {

namespace {

class ModalComponent : public ComponentBase {
public:
    ModalComponent(Component main, Component modal, bool &show)
        : show_(show) {
        Add(main);
        Add(modal);
    }

    Element OnRender() override {
        auto main_el = children_[0]->Render();
        if (show_) {
            return dbox({
                main_el | dim,
                children_[1]->Render() | clear_under | center,
            });
        }
        return main_el;
    }

    bool OnEvent(Event event) override {
        if (show_) {
            if (event == Event::Escape) {
                show_ = false;
                return true;
            }
            return children_[1]->OnEvent(event);
        }
        return children_[0]->OnEvent(event);
    }

    Component ActiveChild() override {
        return show_ ? children_[1] : children_[0];
    }

    bool Focusable() const override {
        return true;
    }

private:
    bool &show_;
};

Component Modal(Component main, Component modal, bool &show) {
    return std::make_shared<ModalComponent>(main, modal, show);
}

} // namespace

MainWindow::MainWindow() = default;
MainWindow::~MainWindow() = default;

void MainWindow::init() {
    app_config_.load();

    for (size_t i = 0; i < app_config_.servers.size(); ++i) {
        init_server_process(i);
    }

    if (!app_config_.servers.empty() && app_config_.active_server_index < app_config_.servers.size()) {
        auto idx = app_config_.active_server_index;
        backup_panel_.set_backup_dir(app_config_.servers[idx].backup_dir);
        backup_panel_.set_world_dir(app_config_.servers[idx].world_dir);

        apply_server_properties_path(idx);
    }

    refresh_server_list_entries();
}

void MainWindow::init_server_process(size_t index) {
    auto &proc = server_processes_[index];
    console_buffers_[index];

    proc.set_output_callback(
        [this, index](const std::string &line) {
            console_buffers_[index].push_line(line);

            if (line.find("players online:") != std::string::npos) {
                size_t colon_pos = line.find(':');
                if (colon_pos != std::string::npos) {
                    std::string names_str = line.substr(colon_pos + 1);
                    names_str = utils::trim(names_str);

                    if (!names_str.empty()) {
                        auto names = utils::split(names_str, ',');
                        std::vector<std::string> players;
                        for (const auto &name : names) {
                            std::string trimmed = utils::trim(name);
                            if (!trimmed.empty()) {
                                players.push_back(trimmed);
                            }
                        }
                        std::lock_guard<std::mutex> lock(players_mutexes_[index]);
                        players_lists_[index] = std::move(players);
                    }
                }
            }
        }
    );

    auto &sc = app_config_.servers[index];
    proc.attach(index, sc.name, sc.working_dir);
}

void MainWindow::refresh_server_list_entries() {
    server_list_entries_.clear();
    for (size_t i = 0; i < app_config_.servers.size(); ++i) {
        auto state = server::ServerState::STOPPED;
        auto it = server_processes_.find(i);
        if (it != server_processes_.end()) {
            state = it->second.state();
        }

        auto disp = get_state_display(state);

        server_list_entries_.push_back(
            std::string(disp.indicator) + " " + app_config_.servers[i].name +
            " [" + std::string(server::get_core_type_info(app_config_.servers[i].core_type).display_name) + "]"
        );
    }
}

void MainWindow::setup_dashboard_callbacks(ScreenInteractive &screen) {
    dashboard_panel_.set_start_callback([this, &screen]() {
        start_server(app_config_.active_server_index);
        screen.PostEvent(Event::Custom);
    });
    dashboard_panel_.set_stop_callback([this, &screen]() {
        size_t idx = app_config_.active_server_index;
        std::thread([this, idx, &screen]() {
            if (server_processes_.count(idx) > 0) {
                server_processes_[idx].stop();
            }
            screen.PostEvent(Event::Custom);
        }).detach();
    });
    dashboard_panel_.set_restart_callback([this, &screen]() {
        restart_server(app_config_.active_server_index);
        screen.PostEvent(Event::Custom);
    });
    dashboard_panel_.set_config_save_callback([this]() {
        app_config_.save();
        on_server_switched();
    });
    dashboard_panel_.set_edit_callback([this]() {
        if (app_config_.active_server_index < app_config_.servers.size()) {
            server_panel_.load_from_server(app_config_.servers[app_config_.active_server_index]);
        }
        show_server_config_popup_ = true;
    });
}

ftxui::Component MainWindow::GetComponent(ScreenInteractive &screen) {
    setup_dashboard_callbacks(screen);

    // ---- Sidebar ----
    sidebar_menu_sel_ = (sidebar_tab_ == SidebarTab::DASHBOARD) ? 0 : 1;
    content_tab_index_ = sidebar_menu_sel_;

    auto sidebar_menu = Menu(&sidebar_entries_, &sidebar_menu_sel_);

    auto sidebar_renderer = Renderer(sidebar_menu, [sidebar_menu, this] {
        return vbox({
            text("  导航") | bold,
            separator(),
            sidebar_menu->Render() | vscroll_indicator,
        });
    });

    // ---- Server List ----
    server_list_menu_sel_ = static_cast<int>(app_config_.active_server_index);

    auto server_list_menu = Menu(&server_list_entries_, &server_list_menu_sel_);

    ButtonOption add_btn_opt;
    add_btn_opt.label = " + ";
    add_btn_opt.on_click = [&] {
        add_server_panel_.reset();
        {
            add_server_panel_.set_servers_parent_folder(
                app_config_.get_effective_servers_parent_folder());
        }
        add_server_panel_.set_global_java_path(app_config_.get_effective_java_path());
        add_server_panel_.set_on_complete([this, &screen](const config::ServerConfig &sc) {
            add_server_from_config(sc);
            show_add_server_popup_ = false;
            refresh_server_list_entries();
            on_server_switched();
            content_needs_rebuild_ = true;
            screen.PostEvent(Event::Custom);
        });
        add_server_panel_.set_on_cancel([this, &screen] {
            show_add_server_popup_ = false;
            screen.PostEvent(Event::Custom);
        });
        show_add_server_popup_ = true;
        screen.PostEvent(Event::Custom);
    };
    add_btn_opt.transform = [this](const EntryState& s) -> Element {
        return (hover_add_btn_ || s.focused)
            ? text(" + ") | color(Color::GreenLight) | inverted
            : text(" + ") | color(Color::Green);
    };
    auto add_button = Button(add_btn_opt);

    add_button |= Hoverable(&hover_add_btn_);

    auto server_list_header = Renderer(add_button, [add_button] {
        return hbox({
            text(" 服务器列表") | bold,
            filler(),
            add_button->Render(),
        });
    });

    auto server_list_body = Renderer(server_list_menu, [server_list_menu, this] {
        if (app_config_.servers.empty()) {
            return text(" 暂无服务器") | dim;
        } else {
            return server_list_menu->Render() | vscroll_indicator;
        }
    });

    server_list_body |= CatchEvent([&](Event event) -> bool {
        auto new_sel = static_cast<size_t>(server_list_menu_sel_);
        bool changed = false;

        if (event.is_character() && event.character() == "\n") {
            changed = (new_sel != app_config_.active_server_index);
        } else if (server_list_menu_sel_ >= 0 && app_config_.servers.size() > 0) {
            changed = (static_cast<size_t>(server_list_menu_sel_) != app_config_.active_server_index);
        }

        if (changed && new_sel < app_config_.servers.size()) {
            app_config_.active_server_index = new_sel;
            app_config_.save();
            on_server_switched();
            content_needs_rebuild_ = true;
            screen.PostEvent(Event::Custom);
        }
        return false;
    });

    auto server_list_section = Container::Vertical({
        server_list_header,
        make_separator(),
        server_list_body | flex,
    });

    // ---- Content Area ----
    auto make_content = [&]() -> ftxui::Component {
        auto settings_comp = settings_panel_.GetComponent(app_config_, [this]() {
            app_config_.save();
        });

        if (app_config_.servers.empty()) {
            auto empty_comp = Renderer([] {
                return text(" 暂无服务器。请点击「+」添加一个服务器。") | dim | center;
            });
            if (content_tab_index_ == 1) return settings_comp;
            return empty_comp;
        }

        auto idx = app_config_.active_server_index;
        if (idx >= app_config_.servers.size()) idx = 0;

        auto &sc = app_config_.servers[idx];
        auto *proc = (server_processes_.count(idx) > 0)
                         ? &server_processes_[idx] : nullptr;
        auto *buf = (console_buffers_.count(idx) > 0)
                        ? &console_buffers_[idx] : nullptr;

        std::vector<std::string> players;
        {
            std::lock_guard<std::mutex> lock(players_mutexes_[idx]);
            players = players_lists_[idx];
        }

        int bk_count = backup_panel_.backup_count();

        auto dashboard_comp = dashboard_panel_.GetComponent(
            sc, proc, buf, players, bk_count,
            &console_panel_, &player_panel_,
            &config_panel_, &backup_panel_);

        switch (content_tab_index_) {
            case 0: return dashboard_comp;
            case 1: return settings_comp;
            default: return dashboard_comp;
        }
    };

    auto content_slot_ptr = std::make_shared<DynamicSlot>();
    ftxui::Component content_slot = content_slot_ptr;
    content_slot_ptr->set_child(make_content());
    content_needs_rebuild_ = false;

    auto content_wrapper = Renderer(content_slot, [=, this] {
        if (content_needs_rebuild_) {
            content_slot_ptr->set_child(make_content());
            content_needs_rebuild_ = false;
        }
        return content_slot_ptr->Render();
    });

    // ---- Layout ----
    auto sized_server_list = server_list_section | size(WIDTH, EQUAL, 22);
    auto maybe_server_list = Maybe(sized_server_list, &show_server_list_);

    auto conditional_server_sep = Renderer([this] {
        if (sidebar_tab_ == SidebarTab::DASHBOARD) {
            return separator();
        }
        return text("") | size(WIDTH, EQUAL, 0);
    });

    auto main_layout = Container::Horizontal({
        sidebar_renderer | size(WIDTH, EQUAL, 14),
        make_separator(),
        maybe_server_list,
        conditional_server_sep,
        content_wrapper | flex,
    });

    auto add_panel_component = add_server_panel_.GetComponent();

    auto server_config_component = server_panel_.GetComponent(
        app_config_,
        [this](size_t idx) -> server::ServerState {
            auto it = server_processes_.find(idx);
            if (it != server_processes_.end()) return it->second.state();
            return server::ServerState::STOPPED;
        },
        [this, &screen]() {
            app_config_.save();
            on_server_switched();
            content_needs_rebuild_ = true;
            show_server_config_popup_ = false;
            screen.PostEvent(Event::Custom);
        },
        [this](size_t idx) {
            delete_server(idx);
        });

    auto agree_btn = Button("同意 EULA 并启动", [this, &screen] {
        if (eula_pending_server_index_ < app_config_.servers.size()) {
            auto &sc = app_config_.servers[eula_pending_server_index_];
            server::agree_eula(sc.working_dir);
            show_eula_popup_ = false;
            start_server(eula_pending_server_index_);
            screen.PostEvent(Event::Custom);
        }
    });
    auto cancel_eula_btn = Button("取消", [this, &screen] {
        show_eula_popup_ = false;
        screen.PostEvent(Event::Custom);
    });
    auto eula_popup_content = Container::Vertical({
        Renderer([] {
            return vbox({
                text(""),
                text(" 需要同意 Minecraft EULA") | bold | color(Color::Yellow),
                text(""),
                text(" 您需要同意 Minecraft 最终用户许可协议 (EULA)"),
                text(" 才能启动服务器。"),
                text(""),
                text(" EULA 链接: https://aka.ms/MinecraftEULA") | color(Color::Cyan),
                text(""),
            });
        }),
        Container::Horizontal({agree_btn, cancel_eula_btn}),
    });
    auto eula_popup = Renderer(eula_popup_content, [eula_popup_content] {
        return vbox({
            eula_popup_content->Render(),
        }) | border | size(WIDTH, LESS_THAN, 60);
    });

    auto root = Modal(main_layout, add_panel_component, show_add_server_popup_);
    auto root_with_server = Modal(root, server_config_component, show_server_config_popup_);
    auto root_with_eula = Modal(root_with_server, eula_popup, show_eula_popup_);

    auto root_with_esc = CatchEvent(root_with_eula, [this](Event event) {
        if (event == Event::Escape) {
            if (show_eula_popup_) {
                show_eula_popup_ = false;
                return true;
            }
            if (show_server_config_popup_) {
                show_server_config_popup_ = false;
                return true;
            }
        }
        return false;
    });

    auto root_with_tab = Renderer(root_with_esc, [=, this] {
        content_tab_index_ = sidebar_menu_sel_;
        auto prev_tab = sidebar_tab_;
        if (sidebar_menu_sel_ == 0) {
            sidebar_tab_ = SidebarTab::DASHBOARD;
        } else {
            sidebar_tab_ = SidebarTab::SETTINGS;
        }
        show_server_list_ = (sidebar_tab_ == SidebarTab::DASHBOARD);
        if (sidebar_tab_ != prev_tab) {
            content_needs_rebuild_ = true;
        }
        return root_with_esc->Render();
    });

    return root_with_tab;
}

void MainWindow::add_server_from_config(const config::ServerConfig &sc) {
    app_config_.servers.push_back(sc);
    app_config_.active_server_index = app_config_.servers.size() - 1;

    std::string parent_folder = app_config_.servers_parent_folder;
    if (parent_folder.empty()) {
        parent_folder = app_config_.get_effective_servers_parent_folder();
        std::filesystem::create_directories(parent_folder);
        app_config_.servers_parent_folder = parent_folder;
    }

    app_config_.save();
    init_server_process(app_config_.active_server_index);
    on_server_switched();
}

void MainWindow::delete_server(size_t index) {
    if (index >= app_config_.servers.size()) return;

    show_server_config_popup_ = false;

    if (server_processes_.count(index) > 0 && server_processes_[index].is_running()) {
        server_processes_[index].stop();
    }

    server_processes_.erase(index);
    console_buffers_.erase(index);
    players_mutexes_.erase(index);
    players_lists_.erase(index);

    auto rekey_map = [&]<typename Map>(Map &m) {
        std::vector<typename Map::node_type> nodes;
        for (auto it = m.begin(); it != m.end(); ) {
            if (it->first > index) {
                auto node = m.extract(it++);
                node.key() = node.key() - 1;
                nodes.push_back(std::move(node));
            } else {
                ++it;
            }
        }
        for (auto &node : nodes) {
            m.insert(std::move(node));
        }
    };

    rekey_map(server_processes_);
    rekey_map(console_buffers_);
    rekey_map(players_mutexes_);
    rekey_map(players_lists_);

    // 物理删除磁盘上的服务器文件
    {
        auto &sc = app_config_.servers[index];
        if (!sc.working_dir.empty()) {
            try {
                std::filesystem::remove_all(sc.working_dir);
            } catch (const std::filesystem::filesystem_error &) {
                // 忽略错误 - 服务器目录可能不存在
            }
        }
    }

    app_config_.servers.erase(app_config_.servers.begin() + index);
    if (app_config_.active_server_index >= app_config_.servers.size()) {
        app_config_.active_server_index = app_config_.servers.empty() ? 0 : app_config_.servers.size() - 1;
    }
    app_config_.save();
    refresh_server_list_entries();
    on_server_switched();
    content_needs_rebuild_ = true;
}

void MainWindow::start_server(size_t index) {
    if (index >= app_config_.servers.size()) return;

    auto &sc = app_config_.servers[index];

    if (!sc.working_dir.empty() && !server::check_eula(sc.working_dir)) {
        eula_pending_server_index_ = index;
        show_eula_popup_ = true;
        return;
    }

    if (server_processes_.count(index) == 0) {
        init_server_process(index);
    }

    auto config = server::build_launch_config(
        sc.core_type,
        sc.java_path,
        sc.server_jar_path,
        sc.min_memory_mb,
        sc.max_memory_mb,
        sc.additional_jvm_args,
        sc.launch_args_file,
        sc.working_dir);

    server_processes_[index].start(config, index);
}

void MainWindow::stop_server(size_t index) {
    if (server_processes_.count(index) > 0) {
        server_processes_[index].stop();
    }
}

void MainWindow::restart_server(size_t index) {
    if (server_processes_.count(index) > 0) {
        server_processes_[index].stop();
        start_server(index);
    }
}

void MainWindow::apply_server_properties_path(size_t idx) {
    if (idx >= app_config_.servers.size()) return;
    auto &sc = app_config_.servers[idx];
    std::string props_path = sc.working_dir.empty()
        ? "server.properties"
        : sc.working_dir + "/server.properties";
    config_panel_.set_config_path(props_path);
}

void MainWindow::on_server_switched() {
    auto idx = app_config_.active_server_index;
    if (idx >= app_config_.servers.size()) return;

    auto &sc = app_config_.servers[idx];
    backup_panel_.set_backup_dir(sc.backup_dir);
    backup_panel_.set_world_dir(sc.world_dir);

    apply_server_properties_path(idx);
}

void MainWindow::shutdown() {
    for (auto &[idx, proc] : server_processes_) {
        proc.detach();
    }
    app_config_.save();
}

} // namespace mc_manager::ui
