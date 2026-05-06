#pragma once

#include <string>
#include <vector>
#include <deque>
#include <mutex>

namespace mc_manager::server {

class ConsoleBuffer {
public:
    static constexpr int MAX_LINES = 5000;

    void push_line(const std::string & line);
    std::vector<std::string> get_lines() const;
    void clear();
    int line_count() const;

private:
    mutable std::mutex mutex_;
    std::deque<std::string> lines_;
};

} // namespace mc_manager::server
