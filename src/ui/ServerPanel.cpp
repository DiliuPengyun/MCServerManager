#include "ui/ServerPanel.h"
#include "ui/StateDisplayInfo.h"
#include "utils/StringUtils.h"

using namespace ftxui;

namespace mc_manager::ui {

void ServerPanel::load_from_server(const config::ServerConfig &sc) {
    name_buf_ = sc.name;
    java_path_buf_ = sc.java_path;
    jvm_args_buf_ = sc.additional_jvm_args;
    backup_dir_buf_ = sc.backup_dir;
    world_dir_buf_ = sc.world_dir;
    min_mem_str_ = std::to_string(sc.min_memory_mb);
    max_mem_str_ = std::to_string(sc.max_memory_mb);
    min_mem_ = sc.min_memory_mb;
    max_mem_ = sc.max_memory_mb;
    show_delete_confirm_ = false;
    main_tab_ = 0;
}

void ServerPanel::write_to_server(config::ServerConfig &sc) {
    sc.name = name_buf_;
    sc.java_path = java_path_buf_;
    sc.additional_jvm_args = jvm_args_buf_;
    sc.backup_dir = backup_dir_buf_;
    sc.world_dir = world_dir_buf_;
    sc.min_memory_mb = min_mem_;
    sc.max_memory_mb = max_mem_;
}

ftxui::Component ServerPanel::GetComponent(
        config::AppConfig &app_config,
        std::function<server::ServerState(size_t)> state_provider,
        std::function<void()> on_save,
        std::function<void(size_t)> on_delete_server) {

    auto name_input = Input(&name_buf_, "服务器名称");
    auto java_input = Input(&java_path_buf_, "Java 路径");
    auto jvm_input = Input(&jvm_args_buf_, "额外 JVM 参数");
    auto backup_input = Input(&backup_dir_buf_, "备份目录");
    auto world_input = Input(&world_dir_buf_, "世界目录");

    auto min_mem_input = Input(&min_mem_str_, "最小内存 (MB)");
    auto max_mem_input = Input(&max_mem_str_, "最大内存 (MB)");

    auto save_button = Button("保存配置", [this, &app_config, on_save] {
        min_mem_ = utils::safe_stoi(min_mem_str_, min_mem_);
        max_mem_ = utils::safe_stoi(max_mem_str_, max_mem_);

        if (app_config.active_server_index < app_config.servers.size()) {
            write_to_server(app_config.servers[app_config.active_server_index]);
        }
        if (on_save) on_save();
    });

    auto delete_button = Button("删除此服务器", [this] {
        show_delete_confirm_ = true;
        main_tab_ = 1;
    });

    auto confirm_yes = Button("确定删除", [this, &app_config, on_delete_server] {
        auto idx = app_config.active_server_index;
        if (idx < app_config.servers.size()) {
            if (on_delete_server) on_delete_server(idx);
        }
        show_delete_confirm_ = false;
        main_tab_ = 0;
    });

    auto confirm_no = Button("取消", [this] {
        show_delete_confirm_ = false;
        main_tab_ = 0;
    });

    auto info_renderer = Renderer([this, &app_config, state_provider] {
        Elements items;
        if (app_config.servers.empty()) {
            items.push_back(text(" 暂无服务器配置。") | dim);
            items.push_back(text(" 请在左侧服务器列表中点击「+ 添加」来添加一个服务器。"));
            return vbox(std::move(items));
        }

        auto idx = app_config.active_server_index;
        if (idx >= app_config.servers.size()) idx = 0;

        auto &sc = app_config.servers[idx];
        auto state = state_provider(idx);

        auto disp = get_state_display(state);

        items.push_back(hbox({
            text(" 服务器状态："),
            text(disp.status_text) | color(disp.status_color) | bold,
        }));
        items.push_back(separator());
        items.push_back(text(" JAR 路径（只读）：" + sc.server_jar_path) | dim);
        items.push_back(text(" 工作目录（只读）：" + sc.working_dir) | dim);

        return vbox(std::move(items));
    });

    auto normal_section = Container::Vertical({
        info_renderer,
        name_input,
        java_input,
        jvm_input,
        min_mem_input,
        max_mem_input,
        backup_input,
        world_input,
        Container::Horizontal({
            save_button,
            delete_button,
        }),
    });

    auto confirm_renderer = Renderer([this, &app_config] {
        auto sc_ptr = app_config.active_server_index < app_config.servers.size()
            ? &app_config.servers[app_config.active_server_index] : nullptr;
        return vbox({
            text(""),
            text(" ⚠ 确定要删除服务器「" + (sc_ptr ? sc_ptr->name : std::string("?")) + "」吗？") | color(Color::Yellow) | bold,
            text(" 此操作将删除服务器配置和磁盘上的所有文件，不可撤销。"),
            text(""),
        });
    });

    auto confirm_section = Container::Vertical({
        confirm_renderer,
        Container::Horizontal({
            confirm_yes,
            confirm_no,
        }),
    });

    main_tab_ = show_delete_confirm_ ? 1 : 0;
    auto content = Container::Tab({
        normal_section,
        confirm_section,
    }, &main_tab_);

    return content;
}

} // namespace mc_manager::ui
