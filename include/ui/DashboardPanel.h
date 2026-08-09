#pragma once

#include <functional>
#include <vector>
#include <string>
#include <chrono>
#include "../config/AppConfig.h"
#include "ftxui/component/component.hpp"

namespace mc_manager::server { class ServerProcess; class ConsoleBuffer; }

namespace mc_manager::ui {

class ConsolePanel;
class PlayerPanel;
class ConfigPanel;
class BackupPanel;

class DashboardPanel {
public:
    using VoidCallback = std::function<void()>;

    enum class SubTab { OVERVIEW, CONSOLE, PLAYERS, CONFIG, BACKUP };

    // players_provider：玩家列表实时来源（控制层 RCON 轮询的缓存）；
    // rcon_available：RCON 是否可用（玩家页灰态依据）
    ftxui::Component GetComponent(config::ServerConfig &server_config,
                                   server::ServerProcess *process,
                                   server::ConsoleBuffer *buffer,
                                   std::function<std::vector<std::string>()> players_provider,
                                   bool rcon_available,
                                   int backup_count,
                                   ConsolePanel *console_panel,
                                   PlayerPanel *player_panel,
                                   ConfigPanel *config_panel,
                                   BackupPanel *backup_panel);

    void set_start_callback(VoidCallback callback) { start_callback_ = std::move(callback); }
    void set_stop_callback(VoidCallback callback) { stop_callback_ = std::move(callback); }
    void set_restart_callback(VoidCallback callback) { restart_callback_ = std::move(callback); }
    void set_config_save_callback(VoidCallback callback) { config_save_callback_ = std::move(callback); }
    void set_edit_callback(VoidCallback callback) { edit_callback_ = std::move(callback); }

    SubTab current_sub_tab() const { return current_sub_tab_; }
    void set_sub_tab(SubTab tab) { current_sub_tab_ = tab; }

private:
    SubTab current_sub_tab_{SubTab::OVERVIEW};
    int sub_tab_int_{0};
    std::chrono::steady_clock::time_point start_time_;
    bool has_start_time_{false};
    VoidCallback start_callback_;
    VoidCallback stop_callback_;
    VoidCallback restart_callback_;
    VoidCallback config_save_callback_;
    VoidCallback edit_callback_;


};

} // namespace mc_manager::ui
