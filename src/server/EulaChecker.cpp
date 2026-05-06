#include "server/EulaChecker.h"
#include "utils/StringUtils.h"
#include <filesystem>
#include <fstream>
#include <sstream>

namespace mc_manager::server {

bool check_eula(const std::string &working_dir) {
    if (working_dir.empty()) return false;

    std::filesystem::path eula_path = std::filesystem::path(working_dir) / "eula.txt";
    if (!std::filesystem::exists(eula_path)) return false;

    std::ifstream file(eula_path);
    if (!file.is_open()) return false;

    std::string line;
    while (std::getline(file, line)) {
        if (line.empty() || line[0] == '#') continue;

        size_t eq_pos = line.find('=');
        if (eq_pos == std::string::npos) continue;

        std::string key = line.substr(0, eq_pos);
        std::string value = line.substr(eq_pos + 1);

        key = utils::trim(key);
        value = utils::trim(value);

        if (key == "eula") {
            return value == "true" || value == "TRUE" || value == "True";
        }
    }

    return false;
}

bool agree_eula(const std::string &working_dir) {
    if (working_dir.empty()) return false;

    std::filesystem::path eula_path = std::filesystem::path(working_dir) / "eula.txt";
    std::filesystem::create_directories(working_dir);

    std::string content;
    bool found_eula_line = false;

    if (std::filesystem::exists(eula_path)) {
        std::ifstream in(eula_path);
        if (in.is_open()) {
            std::ostringstream oss;
            std::string line;
            while (std::getline(in, line)) {
                size_t eq_pos = line.find("eula=");
                if (eq_pos != std::string::npos) {
                    for (size_t i = 0; i < eq_pos; ++i) {
                        if (line[i] != ' ' && line[i] != '\t') {
                            eq_pos = std::string::npos;
                            break;
                        }
                    }
                }
                if (eq_pos != std::string::npos) {
                    oss << "eula=true\n";
                    found_eula_line = true;
                } else {
                    oss << line << "\n";
                }
            }
            content = oss.str();
            in.close();
        }
    }

    if (!found_eula_line) {
        if (!content.empty() && content.back() != '\n') {
            content += "\n";
        }
        content += "eula=true\n";
    }

    std::ofstream out(eula_path, std::ios::trunc);
    if (!out.is_open()) return false;

    out << content;
    out.close();

    return true;
}

}
