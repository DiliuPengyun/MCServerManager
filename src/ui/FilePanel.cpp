#include "ui/FilePanel.h"
#include "ui/MenuHelpers.h"
#include "ui/UiHelpers.h"
#include "ftxui/component/event.hpp"
#include <cstdio>

using namespace ftxui;

namespace {

// 文件大小格式化为 B / KiB / MiB
std::string format_size(std::uintmax_t size) {
    char buf[32];
    if (size < 1024) {
        std::snprintf(buf, sizeof(buf), "%llu B", static_cast<unsigned long long>(size));
    } else if (size < 1024 * 1024) {
        std::snprintf(buf, sizeof(buf), "%.1f KiB", static_cast<double>(size) / 1024.0);
    } else {
        std::snprintf(buf, sizeof(buf), "%.1f MiB", static_cast<double>(size) / (1024.0 * 1024.0));
    }
    return buf;
}

// 在 rel 目录下拼接名字（rel 为 "" 时即名字本身）
std::string join_rel(const std::string &rel, const std::string &name) {
    return rel.empty() ? name : rel + "/" + name;
}

} // namespace

namespace mc_manager::ui {

FilePanel::FilePanel() = default;

void FilePanel::set_root_dir(const std::string &working_dir) {
    if (working_dir.empty()) {
        browser_.reset();
    } else {
        browser_ = std::make_unique<server::FileBrowser>(working_dir);
    }
    current_rel_.clear();
    mode_ = Mode::BROWSE;
    pending_action_ = PendingAction::NONE;
    name_input_.clear();
    edit_content_.clear();
    edit_rel_.clear();
    pending_rel_.clear();
    pending_name_.clear();
    status_message_ = browser_valid() ? "" : "服务器目录不存在";
    refresh_entries();
}

const server::FileEntry *FilePanel::selected_entry() const {
    // 非根目录时菜单索引 0 固定为 [..]，真实条目从 1 起
    int base = current_rel_.empty() ? 0 : 1;
    int idx = selected_ - base;
    if (idx < 0 || idx >= static_cast<int>(entries_.size())) return nullptr;
    return &entries_[idx];
}

void FilePanel::refresh_entries() {
    entries_.clear();
    entry_names_.clear();
    selected_ = 0;
    if (!browser_valid()) return;

    if (!current_rel_.empty()) {
        entry_names_.push_back("[..]");
    }
    std::string err;
    if (!browser_->list(current_rel_, entries_, err)) {
        entries_.clear();
        status_message_ = err;
        return;
    }
    for (const auto &entry : entries_) {
        if (entry.is_dir) {
            entry_names_.push_back(entry.name + "/");
        } else {
            entry_names_.push_back(entry.name + "  (" + format_size(entry.size) + ")");
        }
    }
}

void FilePanel::go_parent() {
    auto pos = current_rel_.find_last_of('/');
    current_rel_ = (pos == std::string::npos) ? "" : current_rel_.substr(0, pos);
    refresh_entries();
}

void FilePanel::open_selected() {
    if (!browser_valid()) return;
    if (!current_rel_.empty() && selected_ == 0) {
        go_parent();
        return;
    }
    const auto *entry = selected_entry();
    if (!entry) return;
    if (entry->is_dir) {
        current_rel_ = join_rel(current_rel_, entry->name);
        refresh_entries();
        return;
    }
    // 文件：读入编辑器
    std::string rel = join_rel(current_rel_, entry->name);
    std::string err;
    if (browser_->read_text(rel, edit_content_, err)) {
        edit_rel_ = rel;
        status_message_.clear();
        mode_ = Mode::EDIT;
    } else {
        status_message_ = err;
    }
}

void FilePanel::begin_name_input(PendingAction action) {
    if (!browser_valid()) return;
    pending_action_ = action;
    switch (action) {
        case PendingAction::CREATE_FILE:
            name_input_label_ = "新文件名：";
            name_input_.clear();
            break;
        case PendingAction::CREATE_DIR:
            name_input_label_ = "新文件夹名：";
            name_input_.clear();
            break;
        case PendingAction::RENAME: {
            const auto *entry = selected_entry();
            if (!entry) return;
            pending_rel_ = join_rel(current_rel_, entry->name);
            pending_name_ = entry->name;
            name_input_label_ = "重命名为：";
            name_input_ = entry->name;  // 预填当前名
            break;
        }
        default:
            return;
    }
    mode_ = Mode::NAME_INPUT;
}

void FilePanel::confirm_name_input() {
    if (!browser_valid() || name_input_.empty()) {
        cancel_to_browse();
        return;
    }
    std::string err;
    bool ok = false;
    std::string done_msg;
    switch (pending_action_) {
        case PendingAction::CREATE_FILE:
            ok = browser_->create_file(join_rel(current_rel_, name_input_), err);
            done_msg = "已新建文件：" + name_input_;
            break;
        case PendingAction::CREATE_DIR:
            ok = browser_->create_dir(join_rel(current_rel_, name_input_), err);
            done_msg = "已新建文件夹：" + name_input_;
            break;
        case PendingAction::RENAME:
            ok = browser_->rename(pending_rel_, name_input_, err);
            done_msg = "已重命名为：" + name_input_;
            break;
        default:
            break;
    }
    status_message_ = ok ? done_msg : err;
    cancel_to_browse();
    refresh_entries();
}

void FilePanel::cancel_to_browse() {
    mode_ = Mode::BROWSE;
    pending_action_ = PendingAction::NONE;
    name_input_.clear();
}

void FilePanel::save_edit() {
    if (!browser_valid()) return;
    std::string err;
    if (browser_->write_text(edit_rel_, edit_content_, err)) {
        status_message_ = "已保存：" + edit_rel_;
    } else {
        status_message_ = err;
    }
}

void FilePanel::begin_delete() {
    if (!browser_valid()) return;
    const auto *entry = selected_entry();
    if (!entry) return;
    pending_rel_ = join_rel(current_rel_, entry->name);
    pending_name_ = entry->name;
    mode_ = Mode::CONFIRM_DELETE;
}

void FilePanel::confirm_delete() {
    if (!browser_valid()) {
        cancel_to_browse();
        return;
    }
    std::string err;
    if (browser_->remove(pending_rel_, err)) {
        status_message_ = "已删除：" + pending_name_;
    } else {
        status_message_ = err;
    }
    cancel_to_browse();
    refresh_entries();
}

ftxui::Component FilePanel::GetComponent() {
    // root 先以引用占位，模式切换后需要把焦点移到对应模式的可见子组件
    auto root_ref = std::make_shared<ftxui::Component>();

    // 各模式子组件统一挂在 root 容器里，渲染时按 mode_ 选取；
    // 焦点索引：BROWSE→0（菜单）、EDIT→2（编辑框）、NAME_INPUT→4（输入框）、
    // CONFIRM_DELETE→5（按钮行）
    auto focus_current_mode = [this, root_ref] {
        int index = 0;
        switch (mode_) {
            case Mode::BROWSE: index = 0; break;
            case Mode::EDIT: index = 2; break;
            case Mode::NAME_INPUT: index = 4; break;
            case Mode::CONFIRM_DELETE: index = 5; break;
        }
        if (*root_ref) {
            (*root_ref)->ChildAt(index)->TakeFocus();
        }
    };

    // 执行一个动作，若动作导致模式切换则同步焦点；
    // 模式不变时保持原焦点（避免点击按钮后焦点被抢回菜单）
    auto act = [this, focus_current_mode](const std::function<void()> &fn) {
        Mode before = mode_;
        fn();
        if (mode_ != before) focus_current_mode();
    };

    // 条目菜单（[..] 返回上级 / 目录进入 / 文件进入编辑），
    // Enter 打开的逻辑在下方 CatchEvent 中实现
    MenuOption menu_opt;
    menu_opt.entries = &entry_names_;
    menu_opt.selected = &selected_;
    menu_opt.entries_option.transform = [](const EntryState &state) -> Element {
        return menu_entry_with_indicator(state, {text(state.label)});
    };
    auto file_menu = Menu(menu_opt);

    auto open_button = Button("打开", [this, act] { act([this] { open_selected(); }); });
    auto create_file_button = Button("新建文件", [this, act] {
        act([this] { begin_name_input(PendingAction::CREATE_FILE); });
    });
    auto create_dir_button = Button("新建文件夹", [this, act] {
        act([this] { begin_name_input(PendingAction::CREATE_DIR); });
    });
    auto rename_button = Button("重命名", [this, act] {
        act([this] { begin_name_input(PendingAction::RENAME); });
    });
    auto delete_button = Button("删除", [this, act] { act([this] { begin_delete(); }); });
    auto refresh_button = Button("刷新", [this] {
        status_message_.clear();
        refresh_entries();
    });
    auto browse_buttons = Container::Horizontal({
        open_button,
        create_file_button,
        create_dir_button,
        rename_button,
        delete_button,
        refresh_button,
    });

    // 多行编辑框：Enter 插入换行，保存走按钮
    InputOption edit_opt;
    edit_opt.multiline = true;
    auto edit_input = Input(&edit_content_, "", edit_opt);
    auto save_button = Button("保存", [this] { save_edit(); });
    auto edit_cancel_button = Button("取消", [this, act] { act([this] { cancel_to_browse(); }); });
    auto edit_buttons = Container::Horizontal({save_button, edit_cancel_button});

    InputOption name_opt;
    name_opt.multiline = false;
    auto name_input = Input(&name_input_, "输入名称...", name_opt);

    auto confirm_delete_button = Button("确认删除", [this, act] { act([this] { confirm_delete(); }); });
    auto delete_cancel_button = Button("取消", [this, act] { act([this] { cancel_to_browse(); }); });
    auto delete_buttons = Container::Horizontal({confirm_delete_button, delete_cancel_button});

    auto root = Container::Vertical({
        file_menu,        // 0
        browse_buttons,   // 1
        edit_input,       // 2
        edit_buttons,     // 3
        name_input,       // 4
        delete_buttons,   // 5
    });
    *root_ref = root;

    auto component = Renderer(root, [this, root] {
        if (mode_ == Mode::EDIT) {
            Elements items;
            items.push_back(text(" 编辑：" + edit_rel_));
            if (!status_message_.empty()) {
                items.push_back(make_status_message(status_message_));
            }
            items.push_back(separator());
            items.push_back(root->ChildAt(2)->Render() | frame | flex);
            items.push_back(separator());
            items.push_back(root->ChildAt(3)->Render());
            return vbox(std::move(items));
        }

        // BROWSE / NAME_INPUT / CONFIRM_DELETE 共用骨架：
        // 路径行 + 状态行 + 菜单 + 底部行（按模式切换）
        Elements items;
        std::string path_display = current_rel_.empty() ? "/" : "/" + current_rel_;
        items.push_back(text(" 文件：" + path_display));
        if (mode_ == Mode::CONFIRM_DELETE) {
            items.push_back(text(" 确认删除 " + pending_name_ + " ？（目录将递归删除）") | color(Color::Yellow));
        } else if (!status_message_.empty()) {
            items.push_back(make_status_message(status_message_));
        }
        items.push_back(separator());

        if (entry_names_.empty()) {
            items.push_back(text(browser_valid() ? " （空目录）" : " 服务器目录不存在") | dim | flex);
        } else {
            items.push_back(root->ChildAt(0)->Render() | frame | vscroll_indicator | flex);
        }
        items.push_back(separator());

        if (mode_ == Mode::NAME_INPUT) {
            items.push_back(hbox({
                text(" " + name_input_label_),
                root->ChildAt(4)->Render() | flex,
            }));
        } else if (mode_ == Mode::CONFIRM_DELETE) {
            items.push_back(root->ChildAt(5)->Render());
        } else {
            items.push_back(root->ChildAt(1)->Render());
        }
        return vbox(std::move(items));
    });

    component |= CatchEvent([this, root, focus_current_mode](Event event) -> bool {
        // Esc：非浏览模式下取消当前操作回浏览模式；浏览模式下不拦截（留给全局退出）
        if (event == Event::Escape) {
            if (mode_ == Mode::BROWSE) return false;
            cancel_to_browse();
            focus_current_mode();
            return true;
        }
        if (event.is_mouse()) return false;

        // 名称输入 / 删除确认时输入行与按钮行是唯一可见控件，
        // 吞掉上下方向键，防止焦点移到未渲染的子组件
        if (mode_ == Mode::NAME_INPUT || mode_ == Mode::CONFIRM_DELETE) {
            if (event == Event::ArrowUp || event == Event::ArrowDown) return true;
        }
        // 浏览模式与编辑模式的按钮行均在最底部，同理拦截向下
        if (mode_ == Mode::BROWSE && root->ChildAt(1)->Focused() && event == Event::ArrowDown) return true;
        if (mode_ == Mode::EDIT && root->ChildAt(3)->Focused() && event == Event::ArrowDown) return true;

        // 浏览模式下菜单 Enter 打开选中项（[..] / 目录 / 文件）
        if (mode_ == Mode::BROWSE && event == Event::Return && root->ChildAt(0)->Focused()) {
            open_selected();
            focus_current_mode();
            return true;
        }
        // 名称输入框 Enter 确认
        if (mode_ == Mode::NAME_INPUT && event == Event::Return && root->ChildAt(4)->Focused()) {
            confirm_name_input();
            focus_current_mode();
            return true;
        }
        return false;
    });

    return component;
}

} // namespace mc_manager::ui
