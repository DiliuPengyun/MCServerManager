#pragma once

#include <string>

namespace mc_manager::server {

bool check_eula(const std::string &working_dir);
bool agree_eula(const std::string &working_dir);

}
