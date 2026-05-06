#pragma once

#include <string>

namespace mc_manager::server {

class TmuxSession {
public:
    static bool is_tmux_available();

    static bool session_exists(const std::string &session_name);

    static bool create_session(const std::string &session_name,
                               const std::string &working_dir,
                               const std::string &command);

    static bool kill_session(const std::string &session_name);

    static bool send_keys(const std::string &session_name,
                          const std::string &keys);

    static bool set_history_limit(const std::string &session_name,
                                  int limit);

    static std::string get_pane_current_path(const std::string &session_name);

    static std::string sanitize_session_name(const std::string &name);

    static std::string script_path(const std::string &session_name);

    static bool write_script(const std::string &session_name,
                             const std::string &working_dir,
                             const std::string &command);

    static void remove_script(const std::string &session_name);

private:
    static int exec_command_exit_code(const std::string &cmd);
};

} // namespace mc_manager::server
