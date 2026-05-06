#include "server/ServerProcess.h"
#include "server/TmuxSession.h"
#include <filesystem>
#include <fcntl.h>
#include <unistd.h>
#include <cerrno>

namespace mc_manager::server {

ServerProcess::ServerProcess() = default;

ServerProcess::~ServerProcess() {
    if (output_thread_.joinable()) {
        if (state_ == ServerState::RUNNING || state_ == ServerState::STARTING) {
            state_ = ServerState::STOPPED;
        }
        output_thread_.join();
    }
    close_log_fd();
}

void ServerProcess::ensure_log_dir(const std::string &working_dir) {
    std::filesystem::path dir = std::filesystem::path(working_dir) / ".mcsm";
    std::filesystem::create_directories(dir);
}

std::string ServerProcess::build_full_command(const LaunchConfig &config) {
    std::string cmd = config.java_path;
    for (const auto &arg : config.args) {
        cmd += " " + arg;
    }
    return cmd;
}

void ServerProcess::start(const LaunchConfig &config, size_t index) {
    if (state_ != ServerState::STOPPED) return;

    if (output_thread_.joinable()) {
        output_thread_.join();
    }

    close_log_fd();

    std::string sanitized = TmuxSession::sanitize_session_name(
        config.working_dir.empty() ? "server" : std::filesystem::path(config.working_dir).filename().string());
    session_name_ = "mcsm_" + std::to_string(index) + "_" + sanitized;

    if (TmuxSession::session_exists(session_name_)) {
        TmuxSession::kill_session(session_name_);
    }

    if (!config.working_dir.empty()) {
        ensure_log_dir(config.working_dir);
        log_file_path_ = (std::filesystem::path(config.working_dir) / ".mcsm" / "console.log").string();
    } else {
        std::filesystem::create_directories(".mcsm");
        log_file_path_ = ".mcsm/console.log";
    }

    int create_fd = open(log_file_path_.c_str(), O_CREAT | O_WRONLY | O_TRUNC, 0644);
    if (create_fd >= 0) {
        close(create_fd);
    }

    std::string java_cmd = build_full_command(config);
    std::string full_cmd = java_cmd + " 2>&1 | tee -a '" + log_file_path_ + "'";

    if (!TmuxSession::create_session(session_name_, config.working_dir, full_cmd)) {
        state_ = ServerState::STOPPED;
        return;
    }

    TmuxSession::set_history_limit(session_name_, 50000);

    log_fd_ = -1;
    log_read_offset_ = 0;
    is_reconnected_ = false;
    state_ = ServerState::STARTING;

    output_thread_ = std::thread(&ServerProcess::read_output_loop, this);
}

void ServerProcess::stop() {
    if (state_ == ServerState::STOPPING) return;

    ServerState prev = state_;
    state_ = ServerState::STOPPING;

    if (prev != ServerState::STOPPED && !session_name_.empty()) {
        TmuxSession::send_keys(session_name_, "stop");

        for (int i = 0; i < 300; ++i) {
            if (!TmuxSession::session_exists(session_name_)) {
                break;
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(100));
        }

        if (TmuxSession::session_exists(session_name_)) {
            TmuxSession::kill_session(session_name_);
        }
    }

    if (output_thread_.joinable()) {
        output_thread_.join();
    }

    close_log_fd();
    state_ = ServerState::STOPPED;
}

void ServerProcess::detach() {
    if (state_ == ServerState::RUNNING || state_ == ServerState::STARTING) {
        state_ = ServerState::STOPPED;
    }

    if (output_thread_.joinable()) {
        output_thread_.join();
    }

    close_log_fd();
}

bool ServerProcess::attach(size_t index, const std::string &server_name, const std::string &working_dir) {
    if (state_ != ServerState::STOPPED) return false;

    if (output_thread_.joinable()) {
        output_thread_.join();
    }

    close_log_fd();

    std::string sanitized = TmuxSession::sanitize_session_name(
        working_dir.empty() ? "server" : std::filesystem::path(working_dir).filename().string());
    session_name_ = "mcsm_" + std::to_string(index) + "_" + sanitized;

    if (!TmuxSession::session_exists(session_name_)) {
        session_name_.clear();
        return false;
    }

    if (!working_dir.empty()) {
        std::string pane_path = TmuxSession::get_pane_current_path(session_name_);
        if (!pane_path.empty() && pane_path != working_dir) {
            TmuxSession::kill_session(session_name_);
            session_name_.clear();
            return false;
        }
    }

    if (!working_dir.empty()) {
        ensure_log_dir(working_dir);
        log_file_path_ = (std::filesystem::path(working_dir) / ".mcsm" / "console.log").string();
    } else {
        log_file_path_ = ".mcsm/console.log";
    }

    if (log_fd_ >= 0) {
        close_log_fd();
    }

    log_fd_ = open(log_file_path_.c_str(), O_RDONLY | O_NONBLOCK);
    if (log_fd_ >= 0) {
        constexpr size_t BUF_SIZE = 4096;
        char buf[BUF_SIZE];
        std::string line_buffer;
        ssize_t bytes;
        while ((bytes = read(log_fd_, buf, sizeof(buf) - 1)) > 0) {
            dispatch_lines(std::string(buf, bytes), line_buffer,
                           [this](const std::string& line) {
                               if (output_callback_) {
                                   output_callback_(line);
                               }
                           });
        }
        log_read_offset_ = lseek(log_fd_, 0, SEEK_CUR);
    } else {
        log_read_offset_ = 0;
    }

    is_reconnected_ = true;
    state_ = ServerState::RUNNING;

    output_thread_ = std::thread(&ServerProcess::read_output_loop, this);
    return true;
}

void ServerProcess::send_command(const std::string &command) {
    if (session_name_.empty()) return;
    if (state_ != ServerState::RUNNING && state_ != ServerState::STARTING) return;

    TmuxSession::send_keys(session_name_, command);
}

ServerState ServerProcess::state() const {
    return state_.load();
}

bool ServerProcess::is_running() const {
    return state_ == ServerState::RUNNING || state_ == ServerState::STARTING;
}

void ServerProcess::set_output_callback(OutputCallback callback) {
    output_callback_ = std::move(callback);
}

void ServerProcess::read_output_loop() {
    try {
        constexpr size_t BUFFER_SIZE = 4096;
        char buffer[BUFFER_SIZE];
        std::string line_buffer;
        bool detected_ready = is_reconnected_;
        int check_counter = 0;

        while (state_ == ServerState::STARTING || state_ == ServerState::RUNNING ||
               state_ == ServerState::STOPPING) {

            if (log_fd_ < 0 && !log_file_path_.empty()) {
                log_fd_ = open(log_file_path_.c_str(), O_RDONLY | O_NONBLOCK);
                if (log_fd_ >= 0) {
                    lseek(log_fd_, log_read_offset_, SEEK_SET);
                }
            }

            if (log_fd_ >= 0) {
                ssize_t bytes_read = read(log_fd_, buffer, sizeof(buffer) - 1);
                if (bytes_read > 0) {
                    log_read_offset_ += bytes_read;
                    dispatch_lines(std::string(buffer, bytes_read), line_buffer,
                                   [this, &detected_ready](const std::string& line) {
                                       if (output_callback_) {
                                           output_callback_(line);
                                       }
                                       if (!detected_ready) {
                                           if (line.find("Done (") != std::string::npos ||
                                               (line.find("Done!") != std::string::npos &&
                                                line.find("For help") == std::string::npos)) {
                                               detected_ready = true;
                                               state_ = ServerState::RUNNING;
                                           }
                                       }
                                   });
                }
            }

            ++check_counter;
            if (check_counter >= 10) {
                check_counter = 0;
                if (!session_name_.empty() && !TmuxSession::session_exists(session_name_)) {
                    if (output_callback_) {
                        output_callback_("Server process exited");
                    }
                    state_ = ServerState::STOPPED;
                    break;
                }
            }

            std::this_thread::sleep_for(std::chrono::milliseconds(50));
        }
    } catch (const std::exception &e) {
        if (output_callback_) {
            output_callback_(std::string("Output thread exception: ") + e.what());
        }
    } catch (...) {
        if (output_callback_) {
            output_callback_("Output thread unknown exception");
        }
    }
    state_ = ServerState::STOPPED;
}

void ServerProcess::close_log_fd() {
    if (log_fd_ >= 0) {
        close(log_fd_);
        log_fd_ = -1;
    }
}

void ServerProcess::dispatch_lines(const std::string& chunk, std::string& line_buffer,
                                   const OutputCallback& on_line) {
    size_t pos = 0;
    while (pos < chunk.size()) {
        size_t newline = chunk.find('\n', pos);
        if (newline == std::string::npos) {
            line_buffer += chunk.substr(pos);
            break;
        }
        line_buffer += chunk.substr(pos, newline - pos);
        if (!line_buffer.empty() && line_buffer.back() == '\r') {
            line_buffer.pop_back();
        }
        if (on_line && !line_buffer.empty()) {
            on_line(line_buffer);
        }
        line_buffer.clear();
        pos = newline + 1;
    }
}

} // namespace mc_manager::server
