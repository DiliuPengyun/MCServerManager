#pragma once

#include <functional>
#include <string>
#include "../config/AppConfig.h"
#include "../server/ServerProcess.h"
#include "ftxui/component/component.hpp"

namespace mc_manager::ui {

class ServerPanel {
public:
    ftxui::Component GetComponent(config::AppConfig &app_config,
                                   std::function<server::ServerState(size_t)> state_provider,
                                   std::function<void()> on_save,
                                   std::function<void(size_t)> on_delete_server);

    void load_from_server(const config::ServerConfig &sc);

private:
    void write_to_server(config::ServerConfig &sc);

    std::string name_buf_;
    std::string java_path_buf_;
    std::string jvm_args_buf_;
    std::string backup_dir_buf_;
    std::string world_dir_buf_;
    std::string min_mem_str_{"1024"};
    std::string max_mem_str_{"4096"};
    int min_mem_{1024};
    int max_mem_{4096};
    bool show_delete_confirm_{false};
    int main_tab_{0};
};

} // namespace mc_manager::ui
