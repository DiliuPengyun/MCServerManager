#pragma once

#include <functional>
#include <string>
#include "../config/AppConfig.h"
#include "ftxui/component/component.hpp"

namespace mc_manager::ui {

class SettingsPanel {
public:
    using VoidCallback = std::function<void()>;

    ftxui::Component GetComponent(config::AppConfig &app_config,
                                   VoidCallback on_save);

private:
    VoidCallback save_callback_;

    std::string parent_folder_buf_;
    std::string java_path_buf_;
    std::string save_status_;
    int save_status_ticks_{0};
    bool inited_{false};
    bool hover_folder_{false};
    bool hover_java_{false};
};

} // namespace mc_manager::ui
