#include "Application.h"
#include "server/TmuxSession.h"
#include "web/WebDaemon.h"
#include <iostream>
#include <exception>
#include <execinfo.h>
#include <cstdlib>
#include <cstring>

static void terminate_handler() {
    std::cerr << "\n*** TERMINATE called ***" << std::endl;
    void *buffer[64];
    int nptrs = backtrace(buffer, 64);
    char **symbols = backtrace_symbols(buffer, nptrs);
    if (symbols) {
        for (int i = 0; i < nptrs; ++i) {
            std::cerr << "  " << symbols[i] << std::endl;
        }
        free(symbols);
    }
    std::abort();
}

static void print_usage() {
    std::cout << "用法:\n"
              << "  MCServerManager             启动 TUI（终端界面）\n"
              << "  MCServerManager web [端口]  以静默后台方式启动 Web UI，不阻塞终端\n"
              << "                              （端口缺省使用配置值，初始为 8080）"
              << std::endl;
}

int main(int argc, char **argv) {
    std::set_terminate(terminate_handler);

    try {
        if (argc >= 2 && (std::strcmp(argv[1], "-h") == 0 || std::strcmp(argv[1], "--help") == 0)) {
            print_usage();
            return 0;
        }

        if (!mc_manager::server::TmuxSession::is_tmux_available()) {
            std::cerr << "MCServerManager: tmux not found. Please install tmux first." << std::endl;
            return 1;
        }

        // web 模式：静默后台运行 Web 服务，不启动 TUI
        if (argc >= 2 && std::strcmp(argv[1], "web") == 0) {
            int port = 0;
            if (argc >= 3) {
                port = std::atoi(argv[2]);
                if (port <= 0 || port > 65535) {
                    std::cerr << "MCServerManager: 无效端口: " << argv[2] << std::endl;
                    return 1;
                }
            }
            return mc_manager::web::run_web_daemon(port);
        }

        mc_manager::Application app;
        if (!app.init()) {
            std::cerr << "MCServerManager: Failed to initialize." << std::endl;
            return 1;
        }

        app.run();
        app.shutdown();

        return 0;
    } catch (const std::exception &e) {
        std::cerr << "MCServerManager: Fatal error: " << e.what() << std::endl;
        return 1;
    } catch (...) {
        std::cerr << "MCServerManager: Fatal unknown error." << std::endl;
        return 1;
    }
}
