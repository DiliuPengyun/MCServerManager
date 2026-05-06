#pragma once

#include <string>
#include <vector>

namespace mc_manager::utils {

inline std::string trim(const std::string & str) {
    size_t start = str.find_first_not_of(" \t\r\n");
    if (start == std::string::npos) return "";
    size_t end = str.find_last_not_of(" \t\r\n");
    return str.substr(start, end - start + 1);
}

std::vector<std::string> split(const std::string & str, char delimiter);

bool starts_with(const std::string & str, const std::string & prefix);

int safe_stoi(const std::string & s, int default_value = 0);

template<typename C>
bool is_valid_index(int idx, const C & container) {
    return idx >= 0 && static_cast<size_t>(idx) < container.size();
}

} // namespace mc_manager::utils
