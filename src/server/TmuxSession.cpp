#include "server/TmuxSession.h"
#include "utils/ProcessUtils.h"
#include <cstdio>
#include <cctype>
#include <sys/stat.h>
#include <unistd.h>

namespace mc_manager::server {

bool TmuxSession::is_tmux_available() {
    return exec_command_exit_code("which tmux 2>/dev/null") == 0;
}

bool TmuxSession::session_exists(const std::string &session_name) {
    std::string cmd = "tmux has-session -t " + session_name + " 2>/dev/null";
    return exec_command_exit_code(cmd) == 0;
}

std::string TmuxSession::script_path(const std::string &session_name) {
    return "/tmp/mcsm_" + session_name + ".sh";
}

bool TmuxSession::write_script(const std::string &session_name,
                                const std::string &working_dir,
                                const std::string &command) {
    std::string path = script_path(session_name);
    FILE *f = fopen(path.c_str(), "w");
    if (!f) return false;
    fprintf(f, "#!/bin/sh\n");
    if (!working_dir.empty()) {
        fprintf(f, "cd '%s'\n", working_dir.c_str());
    }
    fprintf(f, "exec %s\n", command.c_str());
    fclose(f);
    chmod(path.c_str(), 0755);
    return true;
}

void TmuxSession::remove_script(const std::string &session_name) {
    unlink(script_path(session_name).c_str());
}

bool TmuxSession::create_session(const std::string &session_name,
                                  const std::string &working_dir,
                                  const std::string &command) {
    if (!write_script(session_name, working_dir, command)) {
        return false;
    }

    std::string sp = script_path(session_name);
    std::string cmd = "tmux new-session -d -s '" + session_name +
                      "' -c '" + working_dir + "' 'sh " + sp + "'";
    bool ok = exec_command_exit_code(cmd) == 0;
    if (!ok) {
        remove_script(session_name);
    }
    return ok;
}

bool TmuxSession::kill_session(const std::string &session_name) {
    std::string cmd = "tmux kill-session -t " + session_name + " 2>/dev/null";
    bool ok = exec_command_exit_code(cmd) == 0;
    remove_script(session_name);
    return ok;
}

bool TmuxSession::send_keys(const std::string &session_name,
                             const std::string &keys) {
    std::string cmd = "tmux send-keys -t " + session_name + " '" + keys + "' Enter";
    return exec_command_exit_code(cmd) == 0;
}

bool TmuxSession::set_history_limit(const std::string &session_name,
                                     int limit) {
    std::string cmd = "tmux set-option -t " + session_name +
                      " history-limit " + std::to_string(limit) + " 2>/dev/null";
    return exec_command_exit_code(cmd) == 0;
}

std::string TmuxSession::get_pane_current_path(const std::string &session_name) {
    std::string cmd = "tmux list-panes -t " + session_name +
                      " -F '#{pane_current_path}' 2>/dev/null";
    return utils::exec_popen(cmd);
}

std::string TmuxSession::sanitize_session_name(const std::string &name) {
    std::string result;
    for (char c : name) {
        if (std::isalnum(static_cast<unsigned char>(c)) || c == '_') {
            result += c;
        } else {
            result += '_';
        }
    }
    return result;
}

int TmuxSession::exec_command_exit_code(const std::string &cmd) {
    return utils::exec_with_status(cmd).exit_code;
}

} // namespace mc_manager::server
