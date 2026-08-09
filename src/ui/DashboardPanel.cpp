#include "ui/DashboardPanel.h"
#include "ui/ConsolePanel.h"
#include "ui/PlayerPanel.h"
#include "ui/ConfigPanel.h"
#include "ui/BackupPanel.h"
#include "ui/FilePanel.h"
#include "ui/StateDisplayInfo.h"
#include "ui/NavigationHelpers.h"
#include "ui/UiHelpers.h"
#include "server/ServerProcess.h"
#include "server/ConsoleBuffer.h"
#include "ftxui/component/event.hpp"
#include <sstream>

using namespace ftxui;

namespace mc_manager::ui {

ftxui::Component DashboardPanel::GetComponent(
        config::ServerConfig &server_config,
        server::ServerProcess *process,
        server::ConsoleBuffer *buffer,
        std::function<std::vector<std::string>()> players_provider,
        bool rcon_available,
        int backup_count,
        ConsolePanel *console_panel,
        PlayerPanel *player_panel,
        ConfigPanel *config_panel,
        BackupPanel *backup_panel,
        FilePanel *file_panel) {

    sub_tab_int_ = static_cast<int>(current_sub_tab_);
    std::vector<std::string> tab_labels = {"概览", "控制台", "玩家", "配置编辑", "备份", "文件"};

    auto tab_toggle = ftxui::Toggle(tab_labels, &sub_tab_int_);

    auto start_btn = Button("启动", [this] {
        if (start_callback_) start_callback_();
    });
    auto stop_btn = Button("停止", [this] {
        if (stop_callback_) stop_callback_();
    });
    auto restart_btn = Button("重启", [this] {
        if (restart_callback_) restart_callback_();
    });
    auto edit_btn = Button("编辑", [this] {
        if (edit_callback_) edit_callback_();
    });

    // ---- Overview tab ----
    auto overview_renderer = Renderer([=, this, &server_config] {
        auto state = process ? process->state() : server::ServerState::STOPPED;
        int player_count = players_provider
            ? static_cast<int>(players_provider().size()) : 0;
        int console_line_count = buffer ? buffer->line_count() : 0;

        auto disp = get_state_display(state);

        Elements items;

        // Status section
        {
            if (state == server::ServerState::RUNNING && !has_start_time_) {
                start_time_ = std::chrono::steady_clock::now();
                has_start_time_ = true;
            }
            if (state == server::ServerState::STOPPED) {
                has_start_time_ = false;
            }

            items.push_back(hbox({
                text(" ● ") | color(disp.status_color),
                text("服务器状态："),
                text(disp.status_text) | color(disp.status_color) | bold,
            }));

            if (state == server::ServerState::RUNNING && has_start_time_) {
                auto now = std::chrono::steady_clock::now();
                auto uptime = std::chrono::duration_cast<std::chrono::seconds>(now - start_time_).count();
                int hours = static_cast<int>(uptime) / 3600;
                int minutes = (static_cast<int>(uptime) % 3600) / 60;
                int seconds = static_cast<int>(uptime) % 60;
                std::ostringstream oss;
                oss << "运行时间：";
                if (hours < 10) oss << "0";
                oss << hours << ":";
                if (minutes < 10) oss << "0";
                oss << minutes << ":";
                if (seconds < 10) oss << "0";
                oss << seconds;
                items.push_back(text(" " + oss.str()) | color(Color::Cyan));
            }
        }

        items.push_back(separator());

        // Cards section
        {
            add_section_title(items, " 配置概览", Color::Cyan);
            items.push_back(text(" 核心类型: " + std::string(server::get_core_type_info(server_config.core_type).display_name)) | color(Color::Grey70));
            if (!server_config.minecraft_version.empty()) {
                items.push_back(text(" MC 版本: " + server_config.minecraft_version) | color(Color::Grey70));
            }
            if (!server_config.core_version.empty()) {
                items.push_back(text(" 核心版本: " + server_config.core_version) | color(Color::Grey70));
            }
            items.push_back(text(" JAR: " + (server_config.server_jar_path.empty() ? "未配置" : server_config.server_jar_path)) | color(Color::Grey70));
            items.push_back(text(" Java: " + server_config.java_path) | color(Color::Grey70));
            {
                std::ostringstream oss;
                oss << " 内存: " << server_config.min_memory_mb << " MB - " << server_config.max_memory_mb << " MB";
                items.push_back(text(oss.str()) | color(Color::Grey70));
            }
            if (!server_config.working_dir.empty()) {
                items.push_back(text(" 工作目录: " + server_config.working_dir) | color(Color::Grey70));
            }
        }

        // Stats
        {
            add_section_title(items, " 统计信息", Color::Green);
            items.push_back(text(" 控制台日志：" + std::to_string(console_line_count) + " 行"));
            items.push_back(text(" 在线玩家：" + std::to_string(player_count) + " 人"));
        }

        // Backup status
        {
            add_section_title(items, " 备份状态", Color::Yellow);
            items.push_back(text(" 现有备份：" + std::to_string(backup_count) + " 个"));
            if (backup_count > 0) {
                items.push_back(text(" 上次备份可用") | color(Color::Green));
            } else {
                items.push_back(text(" 暂无备份") | dim);
            }
        }

        // Quick guide
        add_section_title(items, " 快速指南");
        items.push_back(text(" 1. 在「服务器」页面管理服务器配置") | dim);
        items.push_back(text(" 2. 在「控制台」查看服务器输出并发送指令") | dim);
        items.push_back(text(" 3. 在「玩家」管理在线玩家") | dim);
        items.push_back(text(" 4. 在「配置编辑」编辑 server.properties") | dim);
        items.push_back(text(" 5. 在「备份」创建和恢复世界备份") | dim);
        items.push_back(text(" 6. 在「文件」管理服务器目录内容") | dim);

        return vbox(std::move(items));
    });

    // ---- Console tab ----
    ftxui::Component console_tab;
    if (console_panel && buffer && process) {
        console_tab = console_panel->GetComponent(buffer, process);
    } else {
        console_tab = make_empty_state_component("请先选择并启动一个服务器。");
    }

    // ---- Players tab ----
    ftxui::Component players_tab;
    if (player_panel && process) {
        players_tab = player_panel->GetComponent(process, players_provider, rcon_available);
    } else {
        players_tab = make_empty_state_component("请先选择并启动一个服务器。");
    }

    // ---- Config tab ----
    ftxui::Component config_tab;
    if (config_panel) {
        config_tab = config_panel->GetComponent();
    } else {
        config_tab = Renderer([] {
            return text(" 未加载配置。") | dim | center;
        });
    }

    // ---- Backup tab ----
    ftxui::Component backup_tab;
    if (backup_panel && process) {
        backup_tab = backup_panel->GetComponent(process);
    } else {
        backup_tab = make_empty_state_component("请先选择并启动一个服务器。");
    }

    // ---- Files tab ----
    ftxui::Component files_tab;
    if (file_panel) {
        files_tab = file_panel->GetComponent();
    } else {
        files_tab = make_empty_state_component("请先选择一个服务器。");
    }

    auto tab_content = Container::Tab({
        overview_renderer,
        console_tab,
        players_tab,
        config_tab,
        backup_tab,
        files_tab,
    }, &sub_tab_int_);

    tab_content |= CatchEvent([this](Event event) -> bool {
        auto new_tab = static_cast<SubTab>(sub_tab_int_);
        if (new_tab != current_sub_tab_) {
            current_sub_tab_ = new_tab;
        }
        return false;
    });

    return MakeTabNavigateVertical({
        tab_toggle,
        make_separator(),
        tab_content | flex,
        Container::Horizontal({
            start_btn,
            stop_btn,
            restart_btn,
            edit_btn,
        }),
    });
}

} // namespace mc_manager::ui
