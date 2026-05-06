#pragma once

#include <string>
#include <vector>
#include <unordered_map>

namespace mc_manager::utils {

class IniParser {
public:
    bool load(const std::string & file_path);
    bool save(const std::string & file_path) const;

    std::string get(const std::string & key, const std::string & default_value = "") const;
    void set(const std::string & key, const std::string & value);
    bool has_key(const std::string & key) const;

    std::unordered_map<std::string, std::string> entries() const;
    std::vector<std::string> ordered_keys() const;

private:
    struct Entry {
        std::string key;
        std::string value;
        std::vector<std::string> leading_comments;
    };

    std::vector<Entry> entries_;
    std::unordered_map<std::string, size_t> key_index_;
};

} // namespace mc_manager::utils
