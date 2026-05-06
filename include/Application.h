#pragma once

#include "ui/MainWindow.h"

namespace mc_manager {

class Application {
public:
    bool init();
    void run();
    void shutdown();

private:
    ui::MainWindow main_window_;
};

} // namespace mc_manager
