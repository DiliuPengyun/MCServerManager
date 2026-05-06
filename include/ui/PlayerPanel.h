#pragma once

#include <vector>
#include <string>
#include <chrono>
#include "ftxui/component/component.hpp"

namespace mc_manager::server { class ServerProcess; }

namespace mc_manager::ui {

class PlayerPanel {
public:
    ftxui::Component GetComponent(server::ServerProcess *process,
                                   const std::vector<std::string> &players);

private:
    std::chrono::steady_clock::time_point last_refresh_;
    std::vector<std::string> players_;
    int selected_{0};
    void request_player_list(server::ServerProcess *process);
};

} // namespace mc_manager::ui
