#pragma once

#include "ftxui/dom/elements.hpp"
#include "ftxui/component/component.hpp"

namespace mc_manager::ui {

/// Build an empty state element with dimmed centered text.
inline ftxui::Element make_empty_state(const std::string& message) {
    return ftxui::text(" " + message) | ftxui::dim | ftxui::center;
}

/// Build a standalone separator component.
inline ftxui::Component make_separator() {
    return ftxui::Renderer([] { return ftxui::separator(); });
}

/// Add a section title followed by a separator to the given element list.
inline void add_section_title(ftxui::Elements& items,
                              const std::string& title,
                              ftxui::Color color = ftxui::Color::White) {
    items.push_back(ftxui::text(title) | ftxui::bold | ftxui::color(color));
    items.push_back(ftxui::separator());
}

/// Build a dimmed status message element.
inline ftxui::Element make_status_message(const std::string& msg) {
    return ftxui::text(" " + msg) | ftxui::dim;
}

/// Build a Renderer component displaying an empty state message.
inline ftxui::Component make_empty_state_component(const std::string& message) {
    return ftxui::Renderer([message] {
        return make_empty_state(message);
    });
}

} // namespace mc_manager::ui
