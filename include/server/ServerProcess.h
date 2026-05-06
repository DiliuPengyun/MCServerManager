#pragma once

#include <string>
#include <thread>
#include <atomic>
#include <functional>
#include "LaunchConfig.h"

namespace mc_manager::server {

enum class ServerState {
    STOPPED,
    STARTING,
    RUNNING,
    STOPPING
};

class ServerProcess {
public:
    using OutputCallback = std::function<void(const std::string &)>;

    ServerProcess();
    ~ServerProcess();

    ServerProcess(const ServerProcess &) = delete;
    ServerProcess &operator=(const ServerProcess &) = delete;

    void start(const LaunchConfig &config, size_t index);

    void stop();
    void detach();
    ServerState state() const;
    bool is_running() const;
    void send_command(const std::string &command);
    void set_output_callback(OutputCallback callback);

    bool attach(size_t index, const std::string &server_name, const std::string &working_dir);

private:
    std::string session_name_;
    std::string log_file_path_;
    int log_fd_ = -1;
    off_t log_read_offset_ = 0;
    std::thread output_thread_;
    std::atomic<ServerState> state_{ServerState::STOPPED};
    OutputCallback output_callback_;
    bool is_reconnected_ = false;

    void read_output_loop();
    void ensure_log_dir(const std::string &working_dir);
    std::string build_full_command(const LaunchConfig &config);
    void close_log_fd();
    void dispatch_lines(const std::string& chunk, std::string& line_buffer,
                        const OutputCallback& on_line);
};

} // namespace mc_manager::server
