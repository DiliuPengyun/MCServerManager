#include "utils/IniParser.h"
#include "utils/StringUtils.h"
#include <fstream>

namespace mc_manager::utils {

bool IniParser::load(const std::string & file_path) {
    std::ifstream file(file_path);
    if (!file.is_open()) return false;

    entries_.clear();
    key_index_.clear();

    std::vector<std::string> current_comments;
    std::string line;

    while (std::getline(file, line)) {
        std::string trimmed = trim(line);
        if (trimmed.empty()) {
            current_comments.clear();
            continue;
        }
        if (trimmed[0] == '#') {
            current_comments.push_back(line);
            continue;
        }

        size_t eq_pos = trimmed.find('=');
        if (eq_pos != std::string::npos) {
            Entry entry;
            entry.key = trim(trimmed.substr(0, eq_pos));
            entry.value = trim(trimmed.substr(eq_pos + 1));
            entry.leading_comments = std::move(current_comments);
            current_comments.clear();

            key_index_[entry.key] = entries_.size();
            entries_.push_back(std::move(entry));
        }
    }

    return true;
}

bool IniParser::save(const std::string & file_path) const {
    std::ofstream file(file_path);
    if (!file.is_open()) return false;

    for (const auto & entry : entries_) {
        for (const auto & comment : entry.leading_comments) {
            file << comment << '\n';
        }
        file << entry.key << '=' << entry.value << '\n';
    }

    return true;
}

std::string IniParser::get(const std::string & key, const std::string & default_value) const {
    auto it = key_index_.find(key);
    if (it != key_index_.end()) {
        return entries_[it->second].value;
    }
    return default_value;
}

void IniParser::set(const std::string & key, const std::string & value) {
    auto it = key_index_.find(key);
    if (it != key_index_.end()) {
        entries_[it->second].value = value;
    } else {
        Entry entry;
        entry.key = key;
        entry.value = value;
        key_index_[key] = entries_.size();
        entries_.push_back(std::move(entry));
    }
}

bool IniParser::has_key(const std::string & key) const {
    return key_index_.find(key) != key_index_.end();
}

std::unordered_map<std::string, std::string> IniParser::entries() const {
    std::unordered_map<std::string, std::string> result;
    for (const auto & entry : entries_) {
        result[entry.key] = entry.value;
    }
    return result;
}

std::vector<std::string> IniParser::ordered_keys() const {
    std::vector<std::string> keys;
    for (const auto & entry : entries_) {
        keys.push_back(entry.key);
    }
    return keys;
}

} // namespace mc_manager::utils
