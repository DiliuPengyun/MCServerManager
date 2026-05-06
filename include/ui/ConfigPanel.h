#pragma once

#include <string>
#include <vector>
#include "../utils/IniParser.h"
#include "ftxui/component/component.hpp"

namespace mc_manager::ui {

class ConfigPanel {
public:
    void set_config_path(const std::string &path);
    ftxui::Component GetComponent();

private:
    std::string config_path_;
    utils::IniParser parser_;
    bool loaded_ = false;
    std::string status_message_;
    std::vector<std::string> key_names_;
    int selected_key_ = 0;
    std::string edit_value_;
};

} // namespace mc_manager::ui
