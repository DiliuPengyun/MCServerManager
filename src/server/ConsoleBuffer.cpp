#include "server/ConsoleBuffer.h"

namespace mc_manager::server {

void ConsoleBuffer::push_line(const std::string & line) {
    std::lock_guard<std::mutex> lock(mutex_);
    lines_.push_back(line);
    while (static_cast<int>(lines_.size()) > MAX_LINES) {
        lines_.pop_front();
    }
}

std::vector<std::string> ConsoleBuffer::get_lines() const {
    std::lock_guard<std::mutex> lock(mutex_);
    return std::vector<std::string>(lines_.begin(), lines_.end());
}

void ConsoleBuffer::clear() {
    std::lock_guard<std::mutex> lock(mutex_);
    lines_.clear();
}

int ConsoleBuffer::line_count() const {
    std::lock_guard<std::mutex> lock(mutex_);
    return static_cast<int>(lines_.size());
}

} // namespace mc_manager::server
