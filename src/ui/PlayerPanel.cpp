#include "ui/PlayerPanel.h"
#include "ui/MenuHelpers.h"
#include "ui/UiHelpers.h"
#include "utils/StringUtils.h"
#include "server/ServerProcess.h"
#include <chrono>

using namespace ftxui;

namespace mc_manager::ui {

void PlayerPanel::refresh_players() {
    auto now = std::chrono::steady_clock::now();
    if (now - last_refresh_ < std::chrono::seconds(1)) return;
    last_refresh_ = now;
    if (players_provider_) {
        players_ = players_provider_();
    }
}

ftxui::Component PlayerPanel::GetComponent(server::ServerProcess *process,
                                            PlayersProvider players_provider,
                                            bool rcon_available) {
    players_provider_ = std::move(players_provider);
    players_.clear();
    selected_ = 0;
    last_refresh_ = {};  // 置为纪元，保证首次渲染立即拉取

    // RCON 未启用：整页灰态，不提供任何操作
    if (!rcon_available) {
        return Renderer([] {
            return vbox({
                text(""),
                text(" 玩家管理不可用") | bold | dim,
                text(""),
                text(" 该服务器未启用 RCON，无法获取在线玩家。") | dim,
                text(" 下次启动服务器时选择「启用 RCON 并启动」即可使用本页。") | dim,
                text(""),
            }) | center;
        });
    }

    auto refresh_button = Button("刷新列表", [this] {
        last_refresh_ = {};
        refresh_players();
    });

    auto kick_button = Button("踢出", [this, process] {
        if (process && utils::is_valid_index(selected_, players_)) {
            process->send_command("kick " + players_[selected_]);
        }
    });

    auto ban_button = Button("封禁", [this, process] {
        if (process && utils::is_valid_index(selected_, players_)) {
            process->send_command("ban " + players_[selected_]);
        }
    });

    auto player_menu = MakeStyledMenu(&players_, &selected_, [](const EntryState &state) -> Element {
        return menu_entry_with_indicator(state, {
            text(state.label),
            text("  (在线)") | color(Color::Green) | dim,
        });
    });

    auto player_section = Renderer(player_menu, [this, player_menu] {
        refresh_players();
        if (players_.empty()) {
            return vbox({
                make_empty_state("暂无在线玩家。"),
            });
        }
        return player_menu->Render() | flex;
    });

    auto header_renderer = Renderer([this, process] {
        Elements items;
        if (!process) {
            items.push_back(text(" 服务器进程不可用。") | dim);
        } else if (!process->is_running()) {
            items.push_back(text(" 服务器未运行。") | dim);
        } else {
            refresh_players();
            items.push_back(text(" 在线玩家：" + std::to_string(players_.size())));
            items.push_back(separator());
            items.push_back(text(" 提示：↑↓ 选择玩家，点击踢出/封禁按钮操作。"));
        }
        return vbox(std::move(items));
    });

    return Container::Vertical({
        header_renderer,
        player_section | flex,
        Container::Horizontal({
            refresh_button,
            kick_button,
            ban_button,
        }),
    });
}

} // namespace mc_manager::ui
