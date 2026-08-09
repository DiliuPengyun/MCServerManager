# AGENTS.md

> 本文件面向 AI 编码代理，介绍 MCServerManager 项目的结构、构建方式与开发约定。

## 项目概述

MCServerManager 是一个基于 [FTXUI](https://github.com/ArthurSonzogni/FTXUI) 的 Minecraft 服务器管理器，以全屏 TUI（终端用户界面）形式运行在 **Linux** 终端中。当前版本 0.2（见 `xmake.lua` 的 `set_version`）。

主要功能：

- 通过 **tmux 会话** 启动 / 停止 / 重启 / 重连 Minecraft 服务器进程
- 多核心支持：Vanilla、Paper（README 注明目前有问题）、Spigot、Forge、NeoForge、Fabric、自定义
- 自动安装：从各官方 API 查询版本并下载安装服务器核心
- 控制台：可滚动日志输出（环形缓冲 5000 行）与指令输入
- 玩家管理：通过 **RCON** 周期执行 `list` 获取在线玩家、踢出、封禁。启动时若服务器未启用 RCON，会弹窗询问：同意后管理器自动在 `server.properties` 写入 `enable-rcon=true` 与自动生成的 `rcon.port`/`rcon.password`（凭据副本存于 `config.json` 的 `rcon_port`/`rcon_password`）；拒绝则直接启动，玩家页灰显不可用
- 备份 / 恢复：世界目录的带时间戳快照
- 文件管理：浏览服务器工作目录、查看 / 编辑文本文件、新建文件 / 文件夹、重命名、删除（TUI 与 Web 均有「文件」标签页，共用 `server/FileBrowser` 操作层）
- `server.properties` 图形化编辑器、EULA 检查与同意、向导式创建服务器

注意：README 声明本项目代码 100% 由 AI 生成。

## 技术栈

- **语言**：C++20（`set_languages("c++20")`）
- **构建系统**：xmake（配置在根目录 `xmake.lua`）
- **依赖**（通过 xmake 包管理拉取）：`ftxui`（UI，锁定 v6.1.9——v7 起 `ComponentBase::children_` 改为私有，代码暂不兼容）、`nlohmann_json`（配置文件与版本清单 JSON）
- **内置第三方**：`third_party/httplib.h` —— 单头文件 cpp-httplib，用于 HTTP 下载与 API 请求
- **系统运行时依赖**：
  - `tmux`（**必需**，入口 `main()` 启动时检查，不存在则退出）
  - `java`（运行服务器；自动探测 `which java` 与 `JAVA_HOME`）
  - `fzf`（可选，`PathPicker` 用于界面内选择路径）
- 平台：仅针对 Linux（`main.cpp` 使用 `execinfo.h`，`ExePath` 读 `/proc/self/exe`，xmake 仅在 Linux 下链接 `pthread`）

## 构建与运行

```bash
xmake build              # 构建（默认 release；另有 mode.debug / mode.release）
xmake run MCServerManager # 构建并运行
```

- 产物：`build/linux/<arch>/<mode>/MCServerManager`（单一可执行文件）
- **没有测试框架、测试目录和 CI 配置**。验证方式 = 编译通过 + 手动运行。
- **环境说明**：系统 `/usr/bin/xmake` 为 v2.8.7（过旧，无法解析当前 xmake-repo 依赖）；已通过官方脚本安装 v3.0.9 到用户级 `~/.local/bin/xmake`（PATH 优先级更高，直接生效），可正常拉取依赖并全量构建。首次构建需加 `-y` 自动确认安装依赖包。

## 目录结构与模块划分

所有代码位于命名空间 `mc_manager` 下，头文件在 `include/`，实现文件在 `src/`，两者路径一一对应（如 `include/server/TmuxSession.h` ↔ `src/server/TmuxSession.cpp`）。共约 4300 行实现代码。

```
src/main.cpp            入口：安装 terminate 处理器（打印 backtrace），检查 tmux。
                        无参数 → 创建 Application 运行 TUI（Esc 键退出）；
                        `web [端口]` → 以守护进程方式后台运行 Web UI（不启动 TUI）
src/Application.cpp     创建 FTXUI 全屏 ScreenInteractive，开启 100ms 定时
                        投递 Event::Custom 的刷新线程，进入事件循环
```

- **`core/`** —— `ServerManager`：UI 无关的控制层（从 MainWindow 抽离）。持有 `AppConfig`、`std::map<size_t, ServerProcess>`、`ConsoleBuffer`、玩家列表等全部运行态，提供 `start/stop/restart/send_command/state/players` 与 EULA 处理（`StartResult::EULA_REQUIRED`），并支持注册控制台行监听（Web SSE 用）。**TUI 与 Web UI 都通过它操作服务器。**
  - 启动时的 **RCON 检查**：`start_server(index, allow_no_rcon)` 在 EULA 之后检查 `server.properties` 是否已启用 RCON 且有密码，未启用返回 `StartResult::RCON_REQUIRED`；`enable_rcon_and_start(index)` 负责写入 RCON 配置（端口从 25575 起避开其他服务器已占用项）并启动。`rcon_available(index)` 供 UI 决定玩家页是否灰显。
  - 启动时的 **安装器检查**（先于 EULA）：Forge/NeoForge 服务器的启动参数文件与可启动 jar 均缺失且目录中存在安装器 jar 时，`needs_installer()` 为真、`start_server` 返回 `StartResult::INSTALLER_REQUIRED`；用户同意后由 UI 层在后台线程运行安装器（`server::run_installer_jar`），成功后 `apply_installer_detection()` 更新配置，再重新走启动流程（EULA → RCON → 启动）。`append_console_line()` 用于把安装器输出注入控制台缓冲区与行监听。TUI 暂无安装引导，只在控制台写提示；Web UI 有完整流程。
  - **RCON 玩家轮询**：`load()` 启动后台线程，每 3 秒对运行中且已配置 RCON 的服务器执行 `list`（目标凭据存于 `rcon_targets_`，随删除服务器一并清理），结果缓存进 `players_lists_` 供 `players()` 读取。
- **`web/`** —— Web UI（`MCServerManager web [端口]` 模式）：
  - `WebServer`：基于内置 `third_party/httplib.h` 服务端的完整 REST + SSE 接口（服务器 CRUD 与启停、EULA、RCON 启用、控制台快照/SSE 流、发指令、server.properties 读写、备份增删恢复、文件管理 `/files` 系列、全局设置、核心类型/版本清单、异步安装任务轮询），令牌认证（`Authorization: Bearer` 或 `?token=`），线程池限为 4；配置与容器访问由 `api_mutex` 串行化（stop 最长持锁约 30 秒）。未启用 RCON 时 `/start` 与 `/restart` 返回 409 `rcon_required`，前端弹窗选择 `POST /rcon`（启用并启动）或携带 `allow_no_rcon` 重发原动作（直接启动）；`/restart` 在停止前同步预检 RCON。整合包未安装时 `/start` 与 `/restart` 返回 409 `installer_required`，前端弹窗确认后 `POST /install` 后台运行安装器（进度写入安装任务，同时经 `append_console_line` 注入控制台 SSE），前端轮询 `/api/tasks/<id>`，成功后自动重发原启动动作（可能继续链式弹出 EULA / RCON 框）。文件管理路由：`GET /files?path=`（列目录）、`GET|PUT /files/content`（读/写文本）、`POST /files`（新建，`is_dir` 区分目录）、`PUT /files/rename`、`DELETE /files?path=`；业务经 `server::FileBrowser`，错误描述由 `file_error_reply` 映射为 400/404/409/500 与错误码。
  - `WebPage.h`：内嵌的功能完整单页应用（约 70 KB，原生 JS + EventSource，无外部依赖），含概览/控制台/玩家/配置/备份/文件/服务器设置标签页与添加服务器向导、全局设置弹窗；「文件」页为目录表格 + 内嵌文本编辑器，新建/重命名经 `modal-file` 通用弹窗、删除走原生 `confirm()`（与备份页一致）。
  - `WebDaemon`：fork 守护进程——父进程通过管道确认子进程绑定端口后打印访问地址/令牌并退出（不阻塞终端）；子进程 setsid、重定向 stdio 到 `<配置目录>/web.log`、写 `web.pid`，sigwait 线程处理 SIGTERM/SIGINT 优雅退出。退出时与 TUI 一样 `detach` 而非停止服务器。**单实例限制**：httplib 在 Linux 上默认 `SO_REUSEPORT`，重复启动不会报端口占用而是多进程分摊连接，因此启动时通过 `web.pid` 检查并拒绝第二个实例。
  - 配置项：`web_port`（默认 8080）、`web_token`（首次启动自动生成 32 位十六进制），均存于 `config.json`。

- **`config/`** —— `AppConfig` / `ServerConfig`：JSON 配置读写。配置文件路径为 `$XDG_CONFIG_HOME/MCServerManager/config.json`（无 XDG 时为 `~/.config/MCServerManager/config.json`）。管理全局 Java 路径、服务器存放父目录（默认 `<可执行文件目录>/server`）、服务器列表与当前选中项。
- **`server/`** —— 服务器进程与核心管理：
  - `ServerProcess`：核心类。拼接 Java 启动命令，在 tmux 会话中运行并把输出 `tee` 到 `<working_dir>/.mcsm/console.log`；后台线程尾随该日志文件并按行分发给回调。支持 `attach()` 重连已存在的会话。tmux 会话名格式为 `mcsm_<index>_<净化后的目录名>`。
  - `TmuxSession`：全部为静态方法，封装 tmux 命令（创建/销毁会话、send_keys、写启动脚本等）。
  - `LaunchConfig`：按核心类型构建 JVM 参数；Forge/NeoForge 使用 launch args 文件而非 `-jar`。
  - `CoreType`：`CoreType` 枚举与 `CORE_TYPES` 常量表（id、显示名、是否用安装器）。
  - `VersionManifest` + `CoreInstaller`：从 Mojang / Paper / NeoForge / Forge / Fabric 官方 API 查询版本并下载安装核心（Forge/NeoForge 需运行 installer jar）。
  - `CoreDetector`：探测已有目录中的服务端核心类型与版本。优先按 `libraries/net/{neoforged,minecraftforge,fabricmc}` 目录判定；无 libraries 的整合包（未运行安装器或旧版打包）按顶层 jar 文件名中的加载器标记识别（neoforge 先于 forge 匹配），只发现安装器 jar 时判定类型但留空 jar 路径。安装器识别基于**内容**（扫描 jar 尾部 zip 中央目录中的 `minecraftforge/installer`、`fml/installer`、`neoforged/installer` 条目名），整合包把安装器改名成 `forge.jar` 也能识别；`find_installer_jar()` 与 `jar_is_installer()` 供未安装整合包定位/判定安装器。jar 兜底递归扫描跳过 `mods` 目录。
  - `EulaChecker`：检查 / 同意 `eula.txt`。
  - `RconClient`：Source RCON 协议的极简同步客户端（auth + 单命令往返），**保持长连接复用**（vanilla 会为每条 RCON 连接各打一行 starting/shutting down 日志，短连接高频轮询会刷屏），仅在断开后重建；用于玩家轮询线程向本机服务器周期发送 `list`。
  - `FileBrowser`：服务器目录文件操作层（浏览、读写文本、新建 / 重命名 / 删除），纯 `std::filesystem` 实现（不拼 shell）。所有相对路径经 `resolve()` 做 `weakly_canonical` 前缀校验，拒绝 `..` 越界与符号链接逃逸；文本读取限 1 MiB 并拒绝二进制文件；禁止删除 / 重命名根目录本身。**TUI 的 FilePanel 与 Web 的 /files 接口共用它。**
  - `ConsoleBuffer`：带互斥锁的 5000 行环形日志缓冲。
- **`ui/`** —— FTXUI 界面层：
  - `MainWindow`：顶层组件，侧栏含「仪表盘 / 设置」；**本身不持有服务器运行态**，通过 `core::ServerManager` 操作（`app_config_` 是 `manager_.config()` 的引用），只保留弹窗、列表选中、面板切换等纯 UI 状态。启动遇 `EULA_REQUIRED` / `RCON_REQUIRED` 时分别弹 EULA / RCON 确认框（EULA 同意后会链式弹出 RCON 框）。
  - `DashboardPanel`：服务器详情页，子标签为 概览 / 控制台 / 玩家 / 配置 / 备份 / 文件，内嵌下列各面板。
  - `ConsolePanel`（日志滚动与命令输入）、`PlayerPanel`（在线玩家/踢出/封禁，数据来自控制层 RCON 轮询；RCON 不可用时整页灰显）、`ConfigPanel`（server.properties 编辑器）、`BackupPanel`（备份/恢复）、`FilePanel`（目录浏览 + 多行文本编辑 + 新建/重命名/删除，`set_root_dir` 注入工作目录，操作走 `server::FileBrowser`）、`SettingsPanel`（全局设置）、`AddServerPanel`（向导式创建服务器）、`ServerPanel`（单个服务器的配置编辑与删除）。
  - 辅助组件：`DynamicSlot`（动态切换子组件）、`UiHelpers`、`MenuHelpers`、`NavigationHelpers`（Tab 焦点导航）、`StateDisplayInfo`。
- **`utils/`** —— 通用工具：
  - `Downloader`：httplib 封装（`http_get` / `http_get_json_parsed` / `download_file` 带进度回调）。
  - `ProcessUtils`：shell 执行（`exec_popen` / `exec_with_status` / `exec_streaming`）。
  - `FileUtils`：头文件内联的文件读写、`IniParser`（`key=value` 解析，用于 server.properties）、`StringUtils`（trim/split，部分内联在头文件）、`ExePath`、`PathPicker`（fzf 路径选择）。

## 代码风格约定

- 命名空间：`mc_manager::{config, core, server, ui, utils, web}`，与目录一一对应。
- 头文件一律 `#pragma once`；源文件内用相对路径包含项目头（如 `#include "config/AppConfig.h"`、`#include "../server/CoreType.h"`）。
- 命名：类用 PascalCase；函数与变量用 snake_case；成员变量带尾部下划线（如 `output_thread_`、`state_`）。
- 缩进 4 空格，K&R 大括号风格。
- 注释、UI 文本、提交信息均使用**中文**——新增代码请保持中文注释与中文界面文案。
- 无 clang-format / editorconfig 配置文件，请模仿周边代码风格。
- 修改仅限必要范围；头文件中简单工具函数可直接 `inline` 实现（参考 `StringUtils.h`、`FileUtils.h`）。

## 测试说明

项目没有自动化测试。改动后的验证流程：

1. `xmake build` 编译通过（注意上文提到的 xmake 版本问题）。
2. `xmake run MCServerManager` 手动运行，确认 TUI 行为（需要装有 tmux 的 Linux 终端）。

## 部署说明

无打包 / 安装流程：直接在 `build/` 目录运行二进制即可。服务器实例文件默认放在 `<可执行文件目录>/server/` 下（可在设置中修改）；每个服务器工作目录下的 `.mcsm/console.log` 是本程序维护的控制台日志。

## 安全注意事项

- `ProcessUtils` 与 `TmuxSession` 大量通过**字符串拼接构造 shell 命令**（popen / system）。修改涉及路径、会话名、玩家名等外部输入的代码时，必须注意 shell 转义与注入风险；会话名必须经过 `TmuxSession::sanitize_session_name()` 净化。
- `agree_eula()` 会修改服务器目录下的 `eula.txt`（代表用户同意 Mojang EULA），不要在未授权的流程中静默调用。
- `enable_rcon_and_start()` 会修改服务器目录下的 `server.properties`（启用 RCON 并写入端口/密码），凭据副本以明文存于 `config.json` 与 `server.properties`——同属需用户明确同意的操作，不要在未授权流程中静默调用。
- 网络下载均指向各官方 API 域名（Mojang、Paper 等）；更换下载源时需在代码中明确。
- 配置文件存储在用户配置目录（`~/.config/MCServerManager/`），不要写入仓库目录。
- `ServerProcess::stop()` 先发送 `stop` 命令等待最多 30 秒，超时后强制 `kill_session`——调整超时逻辑时注意数据损坏风险。
- `FileBrowser` 直接读写服务器目录：**任何新增调用方都必须走它的 `resolve()` 校验**（`weakly_canonical` 前缀比较），不要把未校验的外部路径拼进文件操作；`remove()` 递归删除目录，调用前需有用户确认。
