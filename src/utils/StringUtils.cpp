#include "utils/StringUtils.h"
#include <sstream>
#include <stdexcept>

namespace mc_manager::utils {

std::vector<std::string> split(const std::string & str, char delimiter) {
    std::vector<std::string> tokens;
    std::stringstream ss(str);
    std::string token;
    while (std::getline(ss, token, delimiter)) {
        tokens.push_back(token);
    }
    return tokens;
}

bool starts_with(const std::string & str, const std::string & prefix) {
    if (prefix.size() > str.size()) return false;
    return str.compare(0, prefix.size(), prefix) == 0;
}

int safe_stoi(const std::string & s, int default_value) {
    try {
        return std::stoi(s);
    } catch (const std::exception &) {
        return default_value;
    }
}

} // namespace mc_manager::utils
