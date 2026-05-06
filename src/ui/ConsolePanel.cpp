#include "ui/ConsolePanel.h"
#include "ui/UiHelpers.h"
#include "server/ServerProcess.h"
#include "server/ConsoleBuffer.h"
#include "ftxui/component/event.hpp"

using namespace ftxui;

namespace mc_manager::ui {

ftxui::Component ConsolePanel::GetComponent(server::ConsoleBuffer *buffer,
                                             server::ServerProcess *process) {
    InputOption input_opt;
    input_opt.multiline = false;
    input_opt.on_enter = [this, process] {
        if (!command_buffer_.empty() && process && process->is_running()) {
            process->send_command(command_buffer_);
            command_buffer_.clear();
        }
    };
    auto command_input = Input(&command_buffer_, "输入指令...", input_opt);

    auto send_button = Button("发送", [this, process] {
        if (!command_buffer_.empty() && process && process->is_running()) {
            process->send_command(command_buffer_);
            command_buffer_.clear();
        }
    });

    auto clear_button = Button("清屏", [buffer] {
        if (buffer) buffer->clear();
    });

    auto auto_scroll_option = Checkbox("自动滚动", &auto_scroll_);

    auto output_renderer = Renderer([this, buffer] {
        Elements lines;
        if (buffer) {
            auto all_lines = buffer->get_lines();
            int total = static_cast<int>(all_lines.size());

            if (auto_scroll_) {
                scroll_offset_ = std::max(0, total - kRenderWindow_);
            }
            if (total > 0) {
                if (scroll_offset_ < 0) scroll_offset_ = 0;
                if (scroll_offset_ >= total) scroll_offset_ = total - 1;
            } else {
                scroll_offset_ = 0;
            }

            int start = scroll_offset_;
            int end = std::min(total, start + kRenderWindow_);

            for (int i = start; i < end; i++) {
                Color c = Color::White;
                const auto &line = all_lines[i];
                if (line.find("[ERROR]") != std::string::npos ||
                    line.find("ERROR") != std::string::npos ||
                    line.find("error") != std::string::npos) {
                    c = Color::Red;
                } else if (line.find("[WARN]") != std::string::npos ||
                           line.find("WARN") != std::string::npos) {
                    c = Color::Yellow;
                } else if (line.find("[STDERR]") != std::string::npos) {
                    c = Color::Orange1;
                } else if (line.find("INFO") != std::string::npos) {
                    c = Color::Grey70;
                }
                lines.push_back(paragraph(line) | color(c));
            }
        }
        if (lines.empty()) {
            lines.push_back(text(" 控制台暂无输出") | dim);
        }
        if (auto_scroll_ && !lines.empty()) {
            lines.back() = lines.back() | focus;
        }
        return vbox(std::move(lines)) | yframe | vscroll_indicator;
    });

    output_renderer |= CatchEvent([this, buffer](Event event) -> bool {
        if (!buffer) return false;
        int total = buffer->line_count();
        if (total <= 0) return false;

        if (event.is_mouse()) {
            if (event.mouse().button == Mouse::WheelUp) {
                scroll_offset_ = std::max(0, scroll_offset_ - 3);
                return true;
            }
            if (event.mouse().button == Mouse::WheelDown) {
                scroll_offset_ = std::min(total - 1, scroll_offset_ + 3);
                return true;
            }
            return false;
        }

        if (event == Event::PageUp) {
            scroll_offset_ = std::max(0, scroll_offset_ - kRenderWindow_ / 2);
            return true;
        }
        if (event == Event::PageDown) {
            scroll_offset_ = std::min(total - 1, scroll_offset_ + kRenderWindow_ / 2);
            return true;
        }
        if (event == Event::Home) {
            scroll_offset_ = 0;
            return true;
        }
        if (event == Event::End) {
            scroll_offset_ = std::max(0, total - kRenderWindow_);
            return true;
        }
        return false;
    });

    auto bottom_bar = Container::Horizontal({
        auto_scroll_option,
        clear_button,
        command_input | flex,
        send_button,
    });

    auto centered_bottom = Renderer(bottom_bar, [bottom_bar] {
        return bottom_bar->Render() | vcenter;
    });

    return Container::Vertical({
        output_renderer | flex,
        make_separator(),
        centered_bottom,
    });
}

} // namespace mc_manager::ui
