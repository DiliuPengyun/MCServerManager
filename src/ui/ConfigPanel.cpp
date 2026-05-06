#include "ui/ConfigPanel.h"
#include "ui/MenuHelpers.h"
#include "ui/UiHelpers.h"
#include "utils/StringUtils.h"
#include "ftxui/component/event.hpp"

using namespace ftxui;

namespace mc_manager::ui {

void ConfigPanel::set_config_path(const std::string & path) {
    config_path_ = path;
    loaded_ = parser_.load(path);
    if (loaded_) {
        status_message_ = "已加载 " + path;
        key_names_ = parser_.ordered_keys();
        if (!key_names_.empty()) {
            edit_value_ = parser_.get(key_names_[0]);
        }
    } else {
        status_message_ = "加载失败 " + path + "（文件可能尚不存在）";
        key_names_.clear();
    }
    selected_key_ = 0;
}

ftxui::Component ConfigPanel::GetComponent() {
    auto reload_button = Button("重新加载", [this] {
        loaded_ = parser_.load(config_path_);
        if (loaded_) {
            status_message_ = "重新加载成功。";
            key_names_ = parser_.ordered_keys();
            if (!key_names_.empty() && selected_key_ >= static_cast<int>(key_names_.size())) {
                selected_key_ = 0;
            }
        } else {
            status_message_ = "重新加载失败。";
        }
        selected_key_ = 0;
    });

    auto save_button = Button("保存", [this] {
        if (utils::is_valid_index(selected_key_, key_names_)) {
            parser_.set(key_names_[selected_key_], edit_value_);
        }
        if (parser_.save(config_path_)) {
            status_message_ = "保存成功。";
        } else {
            status_message_ = "保存失败！";
        }
    });

    MenuOption menu_opt;
    menu_opt.entries = &key_names_;
    menu_opt.selected = &selected_key_;
    menu_opt.on_change = [this] {
        if (utils::is_valid_index(selected_key_, key_names_)) {
            edit_value_ = parser_.get(key_names_[selected_key_]);
        }
    };
    auto vertical_ref_for_enter = std::make_shared<ftxui::Component>();
    menu_opt.on_enter = [this, vertical_ref_for_enter] {
        if (utils::is_valid_index(selected_key_, key_names_)) {
            edit_value_ = parser_.get(key_names_[selected_key_]);
        }
        if (*vertical_ref_for_enter) {
            (*vertical_ref_for_enter)->ChildAt(4)->TakeFocus();
        }
    };
    menu_opt.entries_option.transform = [this](const EntryState &state) -> Element {
        const auto &key = state.label;
        std::string val;
        if (state.index >= 0 && static_cast<size_t>(state.index) < key_names_.size()) {
            val = parser_.get(key_names_[state.index]);
        }
        return menu_entry_with_indicator(state, {
            text(key) | size(WIDTH, EQUAL, 26) | (state.active ? bold : nothing),
            text(" = "),
            text(val) | (state.active ? color(Color::Cyan) | bold : color(Color::Grey70)),
        }) | (state.focused ? focus : nothing);
    };

    auto property_menu = Menu(menu_opt);

    auto property_section = Renderer(property_menu, [this, property_menu] {
        Elements header_items;
        header_items.push_back(text(" 文件：" + config_path_));
        if (!status_message_.empty()) {
            header_items.push_back(make_status_message(status_message_));
        }
        header_items.push_back(separator());
        header_items.push_back(hbox({
            text(" 属性（共 " + std::to_string(key_names_.size()) + " 项）："),
        }));
        header_items.push_back(separator());
        auto header = vbox(std::move(header_items));

        if (!loaded_ || key_names_.empty()) {
            return vbox({
                header,
                text(" 未加载配置。请先启动服务器以生成 server.properties。") | dim,
            });
        }

        return vbox({
            header,
            property_menu->Render() | frame | vscroll_indicator | flex,
        });
    });

    auto vertical_ref = std::make_shared<ftxui::Component>();

    InputOption edit_opt;
    edit_opt.multiline = false;
    edit_opt.on_enter = [this, vertical_ref] {
        if (utils::is_valid_index(selected_key_, key_names_)) {
            parser_.set(key_names_[selected_key_], edit_value_);
        }
        if (parser_.save(config_path_)) {
            status_message_ = "保存成功。";
        } else {
            status_message_ = "保存失败！";
        }
        if (*vertical_ref) {
            (*vertical_ref)->ChildAt(2)->TakeFocus();
        }
    };
    auto edit_input = Input(&edit_value_, "编辑值...", edit_opt);

    auto revert_button = Button("恢复原值", [this] {
        if (utils::is_valid_index(selected_key_, key_names_)) {
            edit_value_ = parser_.get(key_names_[selected_key_]);
            status_message_ = "已恢复 " + key_names_[selected_key_] + " 的值。";
        }
    });

    auto edit_view = Renderer([this] {
        if (!loaded_ || key_names_.empty()) return text("");
        if (utils::is_valid_index(selected_key_, key_names_)) {
            return hbox({
                text(" 编辑 " + key_names_[selected_key_] + "："),
            });
        }
        return text("");
    });

    auto vertical = Container::Vertical({
        reload_button,
        save_button,
        property_section,
        edit_view,
        edit_input,
        revert_button,
    });
    *vertical_ref = vertical;
    *vertical_ref_for_enter = vertical;

    auto component = Renderer(vertical, [vertical] {
        return vbox({
            hbox({
                vertical->ChildAt(0)->Render(),
                vertical->ChildAt(1)->Render(),
            }),
            separator(),
            vertical->ChildAt(2)->Render() | flex,
            separator(),
            vertical->ChildAt(3)->Render(),
            hbox({
                vertical->ChildAt(4)->Render() | flex,
                vertical->ChildAt(5)->Render(),
            }),
        });
    });

    component |= CatchEvent([this, vertical](Event event) -> bool {
        if (!loaded_ || key_names_.empty()) return false;

        bool edit_focused = vertical->ChildAt(4)->Focused();

        if (event.is_mouse()) {
            return false;
        }

        if (event == Event::Escape) {
            if (edit_focused) {
                if (utils::is_valid_index(selected_key_, key_names_)) {
                    edit_value_ = parser_.get(key_names_[selected_key_]);
                }
                vertical->ChildAt(2)->TakeFocus();
                return true;
            }
            return false;
        }

        bool reload_focused = vertical->ChildAt(0)->Focused();
        bool save_focused = vertical->ChildAt(1)->Focused();

        if (edit_focused) return false;

        if (reload_focused && event == Event::ArrowRight) {
            vertical->ChildAt(1)->TakeFocus();
            return true;
        }
        if (save_focused && event == Event::ArrowLeft) {
            vertical->ChildAt(0)->TakeFocus();
            return true;
        }

        return false;
    });

    return component;
}

} // namespace mc_manager::ui
