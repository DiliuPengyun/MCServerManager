#pragma once

#include <vector>
#include <string>
#include <chrono>
#include <functional>
#include "ftxui/component/component.hpp"

namespace mc_manager::server { class ServerProcess; }

namespace mc_manager::ui {

class PlayerPanel {
public:
    // 玩家列表数据来源（由控制层 RCON 轮询维护）
    using PlayersProvider = std::function<std::vector<std::string>()>;

    // rcon_available 为 false 时渲染灰态（玩家管理不可用）
    ftxui::Component GetComponent(server::ServerProcess *process,
                                   PlayersProvider players_provider,
                                   bool rcon_available);

private:
    std::chrono::steady_clock::time_point last_refresh_;
    std::vector<std::string> players_;
    PlayersProvider players_provider_;
    int selected_{0};
    void refresh_players();
};

} // namespace mc_manager::ui
