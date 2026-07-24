#pragma once

#include <vector>
#include <string>
#include "../core/ServerManager.h"
#include "../config/AppConfig.h"
#include "DashboardPanel.h"
#include "ConsolePanel.h"
#include "PlayerPanel.h"
#include "ConfigPanel.h"
#include "BackupPanel.h"
#include "SettingsPanel.h"
#include "AddServerPanel.h"
#include "ServerPanel.h"
#include "ftxui/component/screen_interactive.hpp"

namespace mc_manager::ui {

class MainWindow {
public:
    MainWindow();
    ~MainWindow();
    MainWindow(const MainWindow &) = delete;
    MainWindow &operator=(const MainWindow &) = delete;

    void init();
    ftxui::Component GetComponent(ftxui::ScreenInteractive &screen);
    void shutdown();

private:
    enum class SidebarTab { DASHBOARD, SETTINGS };

    // 服务器运行态全部在控制层；app_config_ 是 manager_ 配置的引用
    core::ServerManager manager_;
    config::AppConfig &app_config_;
    SidebarTab sidebar_tab_{SidebarTab::DASHBOARD};

    DashboardPanel dashboard_panel_;
    ConsolePanel console_panel_;
    PlayerPanel player_panel_;
    ConfigPanel config_panel_;
    BackupPanel backup_panel_;
    SettingsPanel settings_panel_;
    AddServerPanel add_server_panel_;
    ServerPanel server_panel_;

    bool show_add_server_popup_{false};
    bool show_server_config_popup_{false};
    bool show_eula_popup_{false};
    size_t eula_pending_server_index_{0};

    int add_core_type_index_{0};
    std::vector<std::string> core_type_names_;

    std::vector<std::string> server_list_entries_;

    std::vector<std::string> sidebar_entries_{"仪表盘", "设置"};
    int sidebar_menu_sel_{0};
    int server_list_menu_sel_{0};
    int content_tab_index_{0};
    bool content_needs_rebuild_{true};
    bool hover_add_btn_{false};
    bool show_server_list_{true};

    void add_server_from_config(const config::ServerConfig &sc);
    void delete_server(size_t index);
    void start_server(size_t index);
    void stop_server(size_t index);
    void restart_server(size_t index);
    void on_server_switched();
    void apply_server_properties_path(size_t idx);
    void setup_dashboard_callbacks(ftxui::ScreenInteractive &screen);
    void refresh_server_list_entries();
};

} // namespace mc_manager::ui
