#pragma once

#include "ftxui/component/component.hpp"
#include "ftxui/component/event.hpp"

namespace mc_manager::ui {

/// Wraps children in a vertical container with Tab/Shift+Tab navigation.
/// Tab cycles forward through focusable children, Shift+Tab cycles backward.
inline ftxui::Component MakeTabNavigateVertical(ftxui::Components children) {
    auto vertical = ftxui::Container::Vertical(std::move(children));

    auto vertical_ref = std::make_shared<ftxui::Component>();
    *vertical_ref = vertical;

    auto navigate_vertical = [vertical_ref](ftxui::Event event) -> bool {
        if (event != ftxui::Event::Tab && event != ftxui::Event::TabReverse) {
            return false;
        }

        auto& vert = *vertical_ref;
        if (!vert) return false;

        int count = static_cast<int>(vert->ChildCount());
        if (count == 0) return false;

        int current = -1;
        auto* active_ptr = vert->ActiveChild().get();
        for (int i = 0; i < count; i++) {
            if (vert->ChildAt(i).get() == active_ptr) {
                current = i;
                break;
            }
        }
        if (current < 0) current = 0;

        int step = (event == ftxui::Event::TabReverse) ? -1 : 1;
        for (int i = 1; i <= count; i++) {
            int next = (current + step * i) % count;
            if (next < 0) next += count;
            if (vert->ChildAt(next)->Focusable()) {
                vert->ChildAt(next)->TakeFocus();
                return true;
            }
        }
        return true;
    };

    return vertical | ftxui::CatchEvent(std::move(navigate_vertical));
}

} // namespace mc_manager::ui
