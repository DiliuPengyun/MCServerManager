#pragma once

#include <string>
#include <vector>
#include "ftxui/component/component.hpp"

namespace mc_manager::server { class ServerProcess; }

namespace mc_manager::ui {

class BackupPanel {
public:
    void set_backup_dir(const std::string & dir);
    void set_world_dir(const std::string & dir);
    ftxui::Component GetComponent(server::ServerProcess * process);
    int backup_count() const { return static_cast<int>(existing_backups_.size()); }

private:
    std::string backup_dir_ = "./backups";
    std::string world_dir_ = "./world";
    std::string status_message_;
    std::vector<std::string> existing_backups_;
    int selected_backup_ = 0;

    void refresh_backup_list();
    void create_backup(server::ServerProcess * process);
    void restore_backup(const std::string & backup_name);
    void delete_backup(const std::string & backup_name);
};

} // namespace mc_manager::ui
