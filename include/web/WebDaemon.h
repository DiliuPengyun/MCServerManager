#pragma once

namespace mc_manager::web {

// 以静默后台（守护进程）方式运行 Web UI：fork 后父进程打印访问信息并退出，
// 服务在子进程中持续运行，不阻塞终端，也不启动 TUI。
// port_override > 0 时覆盖配置中的 web_port。
int run_web_daemon(int port_override);

} // namespace mc_manager::web
