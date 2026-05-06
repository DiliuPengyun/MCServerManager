#include "Application.h"
#include "server/TmuxSession.h"
#include <iostream>
#include <exception>
#include <execinfo.h>
#include <cstdlib>

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

int main() {
    std::set_terminate(terminate_handler);

    try {
        if (!mc_manager::server::TmuxSession::is_tmux_available()) {
            std::cerr << "MCServerManager: tmux not found. Please install tmux first." << std::endl;
            return 1;
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
