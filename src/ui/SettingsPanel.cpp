#include "ui/SettingsPanel.h"
#include "ftxui/component/event.hpp"

using namespace ftxui;

namespace mc_manager::ui {

ftxui::Component SettingsPanel::GetComponent(config::AppConfig &app_config,
                                               VoidCallback on_save) {
    save_callback_ = std::move(on_save);

    std::string effective_java = app_config.get_effective_java_path();

    if (!inited_) {
        parent_folder_buf_ = app_config.servers_parent_folder;
        java_path_buf_ = app_config.global_java_path;
        inited_ = true;
    }

    auto do_save = [this, &app_config] {
        app_config.servers_parent_folder = parent_folder_buf_;
        app_config.global_java_path = java_path_buf_;
        if (save_callback_) save_callback_();
        save_status_ = "已保存";
        save_status_ticks_ = 10;
    };

    InputOption folder_opt;
    folder_opt.transform = [this](InputState state) -> Element {
        if (state.focused) {
            return state.element | inverted;
        }
        auto str = parent_folder_buf_.empty()
            ? "管理目录 (留空自动检测)" : parent_folder_buf_;
        Element elem = text(str);
        if (hover_folder_) {
            elem = elem | inverted;
        } else if (parent_folder_buf_.empty()) {
            elem = elem | dim;
        }
        return elem;
    };

    InputOption java_opt;
    java_opt.transform = [this, effective_java](InputState state) -> Element {
        if (state.focused) {
            return state.element | inverted;
        }
        auto str = java_path_buf_.empty() || java_path_buf_ == "java"
            ? effective_java : java_path_buf_;
        Element elem = text(str);
        if (hover_java_) {
            elem = elem | inverted;
        } else if (java_path_buf_.empty() || java_path_buf_ == "java") {
            elem = elem | dim;
        }
        return elem;
    };

    auto parent_folder_input = Input(&parent_folder_buf_, "管理目录 (留空自动检测)", folder_opt);
    auto java_path_input = Input(&java_path_buf_, "Java 路径 (留空使用默认 java)", java_opt);

    parent_folder_input |= Hoverable(&hover_folder_);
    java_path_input |= Hoverable(&hover_java_);

    auto container = Container::Vertical({
        parent_folder_input,
        java_path_input,
    });

    container->SetActiveChild(Component(nullptr));

    container |= CatchEvent([do_save](Event event) -> bool {
        if (event == Event::Return) {
            do_save();
            return true;
        }
        return false;
    });

    auto main_renderer = Renderer(container, [this,
                                              parent_folder_input,
                                              java_path_input,
                                              effective_java] {
        bool folder_focused = parent_folder_input->Focused();
        bool java_focused = java_path_input->Focused();

        if (save_status_ticks_ > 0) save_status_ticks_--;

        Elements items;

        items.push_back(text(" 设置") | bold);
        items.push_back(separator());

        if (folder_focused) {
            items.push_back(text(" ▸ 正在编辑管理目录，Enter 保存 · Tab 切换") |
                            color(Color::Cyan));
        } else if (java_focused) {
            items.push_back(text(" ▸ 正在编辑 Java 路径，Enter 保存 · Tab 切换") |
                            color(Color::Cyan));
        }

        auto folder_input_el = hbox({
            text(" 管理目录: ") | (folder_focused ? bold : nothing),
            parent_folder_input->Render(),
        });
        std::string java_label = " Java 路径: ";
        if (java_path_buf_.empty() || java_path_buf_ == "java") {
            java_label += "(默认: " + effective_java + ") ";
        }
        auto java_input_el = hbox({
            text(java_label) | (java_focused ? bold : nothing),
            java_path_input->Render(),
        });

        items.push_back(folder_input_el);
        items.push_back(java_input_el);

        if (save_status_ticks_ > 0) {
            items.push_back(text(""));
            items.push_back(hbox({text(" ✓ "),
                                  text(save_status_) | bold | color(Color::Green)}));
        }

        items.push_back(separator());
        items.push_back(
            text(" Tab 切换焦点 · 直接键入编辑 · Enter 保存") | dim);

        return vbox(std::move(items)) | border;
    });

    return main_renderer;
}

} // namespace mc_manager::ui
