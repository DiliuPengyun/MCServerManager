#pragma once

#include "server/ServerProcess.h"
#include "ftxui/dom/elements.hpp"

namespace mc_manager::ui {

struct StateDisplayInfo {
    const char *status_text;
    ftxui::Color status_color;
    const char *indicator;
};

inline StateDisplayInfo get_state_display(server::ServerState state) {
    using server::ServerState;
    switch (state) {
    case ServerState::RUNNING:
        return {"运行中",   ftxui::Color::Green,   "●"};
    case ServerState::STARTING:
        return {"启动中...", ftxui::Color::Yellow,  "◐"};
    case ServerState::STOPPING:
        return {"停止中...", ftxui::Color::Orange1, "◑"};
    default:
        return {"已停止",   ftxui::Color::Red,     "○"};
    }
}

} // namespace mc_manager::ui
