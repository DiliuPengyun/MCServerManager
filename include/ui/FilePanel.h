#pragma once

#include <memory>
#include <string>
#include <vector>
#include "../server/FileBrowser.h"
#include "ftxui/component/component.hpp"

namespace mc_manager::ui {

// 服务器文件管理面板：浏览目录、查看/编辑文本文件、
// 新建/重命名/删除文件与文件夹。所有操作经 server::FileBrowser
// 限制在服务器工作目录之内。
class FilePanel {
public:
    FilePanel();

    // 切换服务器时调用：置根目录、刷新列表、回到浏览模式；
    // 传空串表示未选服务器，面板显示空态提示
    void set_root_dir(const std::string &working_dir);

    ftxui::Component GetComponent();

private:
    enum class Mode { BROWSE, NAME_INPUT, EDIT, CONFIRM_DELETE };
    enum class PendingAction { NONE, CREATE_FILE, CREATE_DIR, RENAME };

    std::unique_ptr<server::FileBrowser> browser_;  // 无默认构造，set_root_dir 时重建
    std::string current_rel_;                        // 当前相对路径，"" 为根目录
    std::vector<server::FileEntry> entries_;         // 当前目录真实条目
    std::vector<std::string> entry_names_;           // 菜单显示名（含首行 [..]）
    int selected_ = 0;
    std::string status_message_;
    Mode mode_ = Mode::BROWSE;
    PendingAction pending_action_ = PendingAction::NONE;
    std::string name_input_;                         // 新建/重命名输入
    std::string name_input_label_;                   // 输入行标签
    std::string edit_content_;                       // 编辑器内容
    std::string edit_rel_;                           // 正在编辑的文件相对路径
    std::string pending_rel_;                        // 待重命名/删除的目标相对路径
    std::string pending_name_;                       // 目标纯文件名（提示与预填用）

    bool browser_valid() const { return browser_ && browser_->is_valid(); }
    // 菜单索引映射到真实条目（[..] 占索引 0 时返回 nullptr）
    const server::FileEntry *selected_entry() const;

    void refresh_entries();
    void go_parent();
    void open_selected();
    void begin_name_input(PendingAction action);
    void confirm_name_input();
    void cancel_to_browse();
    void save_edit();
    void begin_delete();
    void confirm_delete();
};

} // namespace mc_manager::ui
