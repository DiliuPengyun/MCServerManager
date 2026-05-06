#include "ui/BackupPanel.h"
#include "ui/MenuHelpers.h"
#include "ui/UiHelpers.h"
#include "utils/StringUtils.h"
#include "server/ServerProcess.h"
#include <filesystem>
#include <chrono>
#include <ctime>
#include <sstream>
#include <iomanip>

using namespace ftxui;

namespace mc_manager::ui {

void BackupPanel::set_backup_dir(const std::string & dir) {
    backup_dir_ = dir;
    refresh_backup_list();
}

void BackupPanel::set_world_dir(const std::string & dir) {
    world_dir_ = dir;
}

void BackupPanel::refresh_backup_list() {
    existing_backups_.clear();
    try {
        for (const auto &entry : std::filesystem::directory_iterator(backup_dir_)) {
            if (entry.is_directory()) {
                existing_backups_.push_back(entry.path().filename().string());
            }
        }
        std::sort(existing_backups_.rbegin(), existing_backups_.rend());
    } catch (...) {
    }
    selected_backup_ = 0;
}

void BackupPanel::create_backup(server::ServerProcess *process) {
    if (!process || process->is_running()) {
        status_message_ = "创建备份前请先停止服务器。";
        return;
    }

    try {
        std::filesystem::create_directories(backup_dir_);
    } catch (...) {
        status_message_ = "创建备份目录失败。";
        return;
    }

    auto now = std::chrono::system_clock::now();
    auto time_t_now = std::chrono::system_clock::to_time_t(now);
    std::tm tm_now;
    localtime_r(&time_t_now, &tm_now);

    std::ostringstream oss;
    oss << "backup_"
        << std::put_time(&tm_now, "%Y%m%d_%H%M%S");

    std::string backup_name = oss.str();
    std::string backup_path = backup_dir_ + "/" + backup_name;

    try {
        std::filesystem::copy(world_dir_, backup_path,
                              std::filesystem::copy_options::recursive);
        status_message_ = "备份已创建：" + backup_name;
        refresh_backup_list();
    } catch (const std::exception & e) {
        status_message_ = std::string("备份失败：") + e.what();
    }
}

void BackupPanel::restore_backup(const std::string & backup_name) {
    std::string backup_path = backup_dir_ + "/" + backup_name;

    try {
        if (std::filesystem::exists(world_dir_)) {
            std::filesystem::remove_all(world_dir_);
        }
        std::filesystem::copy(backup_path, world_dir_,
                              std::filesystem::copy_options::recursive);
        status_message_ = "已从备份恢复：" + backup_name;
    } catch (const std::exception & e) {
        status_message_ = std::string("恢复失败：") + e.what();
    }
}

void BackupPanel::delete_backup(const std::string & backup_name) {
    std::string backup_path = backup_dir_ + "/" + backup_name;

    try {
        std::filesystem::remove_all(backup_path);
        status_message_ = "已删除：" + backup_name;
        refresh_backup_list();
    } catch (const std::exception & e) {
        status_message_ = std::string("删除失败：") + e.what();
    }
}

ftxui::Component BackupPanel::GetComponent(server::ServerProcess *process) {
    auto refresh_button = Button("刷新列表", [this] { refresh_backup_list(); });

    auto create_button = Button("创建备份", [this, process] {
        create_backup(process);
    });

    auto restore_button = Button("恢复选中", [this] {
        if (utils::is_valid_index(selected_backup_, existing_backups_)) {
            restore_backup(existing_backups_[selected_backup_]);
        }
    });

    auto delete_button = Button("删除选中", [this] {
        if (utils::is_valid_index(selected_backup_, existing_backups_)) {
            delete_backup(existing_backups_[selected_backup_]);
        }
    });

    auto backup_menu = MakeStyledMenu(&existing_backups_, &selected_backup_);

    auto content_section = Renderer(backup_menu, [this, process, backup_menu] {
        Elements items;

        items.push_back(text(" 备份目录：" + backup_dir_));
        items.push_back(text(" 世界目录：" + world_dir_));

        if (!status_message_.empty()) {
            items.push_back(make_status_message(status_message_));
        }
        items.push_back(separator());

        bool running = process && process->is_running();
        if (running) {
            items.push_back(text(" ⚠ 创建备份前需要停止服务器。") | color(Color::Yellow));
        }

        items.push_back(separator());
        items.push_back(text(" 已有备份（共 " + std::to_string(existing_backups_.size()) + " 个）："));

        if (existing_backups_.empty()) {
            items.push_back(text(" 暂无备份。") | dim);
        } else {
            items.push_back(backup_menu->Render() | flex);
        }

        return vbox(std::move(items));
    });

    auto component = Container::Vertical({
        content_section | flex,
        Container::Horizontal({
            create_button,
            restore_button,
            delete_button,
            refresh_button,
        }),
    });

    return component;
}

} // namespace mc_manager::ui
