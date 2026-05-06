#pragma once

#include "ftxui/dom/elements.hpp"
#include "ftxui/component/component.hpp"
#include <functional>
#include <string>
#include <vector>

namespace mc_manager::ui {

/// Build a standard menu entry row with active indicator and focus styling.
/// @param state  The FTXUI EntryState from menu transform callback
/// @param content  The content elements to display after the indicator
/// @return  A styled hbox element
inline ftxui::Element menu_entry_with_indicator(const ftxui::EntryState &state,
                                                 ftxui::Elements content) {
    content.insert(content.begin(), ftxui::text(state.active ? " ▶ " : "   "));
    auto row = ftxui::hbox(std::move(content));
    if (state.focused) {
        row = row | ftxui::inverted;
    }
    return row;
}

/// Create a styled menu with the default indicator transform.
/// @param entries  Pointer to the vector of menu entry labels
/// @param selected  Pointer to the selected index
/// @return  An ftxui::Component wrapping the menu
inline ftxui::Component MakeStyledMenu(std::vector<std::string>* entries, int* selected) {
    ftxui::MenuOption menu_opt;
    menu_opt.entries = entries;
    menu_opt.selected = selected;
    menu_opt.entries_option.transform = [](const ftxui::EntryState& state) -> ftxui::Element {
        return menu_entry_with_indicator(state, {ftxui::text(state.label)});
    };
    return ftxui::Menu(menu_opt);
}

/// Create a styled menu with a custom entry transform.
/// @param entries  Pointer to the vector of menu entry labels
/// @param selected  Pointer to the selected index
/// @param transform  Custom transform function for menu entries
/// @return  An ftxui::Component wrapping the menu
inline ftxui::Component MakeStyledMenu(std::vector<std::string>* entries, int* selected,
                                        std::function<ftxui::Element(const ftxui::EntryState&)> transform) {
    ftxui::MenuOption menu_opt;
    menu_opt.entries = entries;
    menu_opt.selected = selected;
    menu_opt.entries_option.transform = std::move(transform);
    return ftxui::Menu(menu_opt);
}

} // namespace mc_manager::ui
