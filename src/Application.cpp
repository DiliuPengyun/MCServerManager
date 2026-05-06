#include "Application.h"
#include "ftxui/component/component.hpp"
#include "ftxui/component/event.hpp"
#include "ftxui/component/screen_interactive.hpp"
#include <thread>
#include <atomic>

using namespace ftxui;

namespace mc_manager {

bool Application::init() {
    main_window_.init();
    return true;
}

void Application::run() {
    auto screen = ScreenInteractive::Fullscreen();
    screen.TrackMouse(true);

    auto component = main_window_.GetComponent(screen);

    std::atomic<bool> refresh_running{true};
    std::jthread refresh_thread([&]() {
        try {
            while (refresh_running) {
                std::this_thread::sleep_for(std::chrono::milliseconds(100));
                screen.PostEvent(Event::Custom);
            }
        } catch (...) {}
    });

    auto component_with_quit = CatchEvent(component, [&](Event event) -> bool {
        if (event == Event::Escape) {
            refresh_running = false;
            screen.ExitLoopClosure()();
            return true;
        }
        return false;
    });

    screen.Loop(component_with_quit);

    refresh_running = false;
}

void Application::shutdown() {
    main_window_.shutdown();
}

} // namespace mc_manager
