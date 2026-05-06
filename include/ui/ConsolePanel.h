#pragma once

#include <string>
#include "ftxui/component/component.hpp"

namespace mc_manager::server { class ServerProcess; class ConsoleBuffer; }

namespace mc_manager::ui {

class ConsolePanel {
public:
    ftxui::Component GetComponent(server::ConsoleBuffer *buffer,
                                   server::ServerProcess *process);

private:
    std::string command_buffer_;
    bool auto_scroll_ = true;
    int scroll_offset_ = 0;
    int last_line_count_ = 0;
    static constexpr int kRenderWindow_ = 200;
};

} // namespace mc_manager::ui
