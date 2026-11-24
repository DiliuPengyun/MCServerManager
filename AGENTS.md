# AGENTS.md

> 本文件面向 AI 编码代理，介绍 MCServerManager 当前 Rust 实现、构建方式与开发约定。

## 项目概述

MCServerManager 是轻量的 **Linux 单机** Minecraft 服务器管理器。主实现为 Rust 2024，版本 0.3.0（根目录 `Cargo.toml`），默认提供 ratatui 全屏 TUI，`web [端口]` 提供 REST/SSE 与内嵌单页 Web UI。Minecraft 进程通过 **tmux** 持久运行，不使用数据库，也不扩展为多租户或集群平台。

主要功能：

- 启动、停止、重启和重连服务器；管理器退出只 detach，不停止 Minecraft。
- 稳定 `ServerId(UUID)`、每服务器独立 `working_dir`、默认新建目录、目录移动和手动搬迁后的重新定位。
- Vanilla、Paper、Forge、NeoForge、Fabric 自动安装；Spigot 和自定义核心可识别/导入，Spigot 暂无自动安装 API。
- 5000 行控制台缓冲、SSE、指令输入、ANSI/jline 清理、重连尾部回放和日志轮转。
- RCON 长连接玩家轮询、踢出和封禁。
- 文件浏览、文本编辑、新建、重命名、删除和二进制流式上传。
- 世界备份、校验恢复和失败回滚。
- 保留布局的 `server.properties` 编辑、显式 EULA/RCON 授权及创建服务器向导。

README 声明项目代码 100% 由 AI 生成。

## 技术栈与构建

- Rust 2024、Cargo；主要依赖 Tokio、Axum、Reqwest、Serde、ratatui、crossterm。
- 单包、单二进制；lib 名为 `mcsm`，入口二进制名为 `MCServerManager`。
- Linux 运行时必需 `tmux`，运行服务器需要 `java`。
- Web 前端源码位于 `assets/index/` 的 HTML/CSS/classic JS partials，由 `web/assets.rs` 通过 `concat!(include_str!(...))` 在编译期拼成同一页面并内嵌，无 npm 或运行时资源目录。

```bash
cargo build                 # 开发构建
cargo build --release       # 发布构建
cargo run                   # 默认 TUI
cargo run -- web 8080       # Web 守护进程
cargo run -- web 8080 --foreground
```

产物为 `target/debug/MCServerManager` 或 `target/release/MCServerManager`。

## Rust 目录结构与职责

```text
src/main.rs             clap 入口、tmux 检查；默认 TUI，web 子命令
src/lib.rs              公共模块、错误与内嵌页面导出
src/config/             稳定 ID、配置模型和原子保存
src/core/               ServerManager、RCON 轮询、tmux 会话名、安装和目录事务
src/server/install/     版本源、下载、Java 安装器、安装服务及内部测试
src/server/             tmux、进程、控制台、RCON、检测、文件和备份
src/web/shared/         /api 的 UUID 领域用例与 HTTP 辅助
src/web/                协议适配、SSE、上传、操作注册表和守护进程
src/ui/state/           页面状态、消息 reducer、弹窗、向导和面板输入
src/ui/tasks/           命令/消息模型与各领域后台操作
src/ui/render/          shell、widgets、向导、弹窗和面板绘制
assets/index/           编译期组装的 HTML/CSS/classic JS partials
```

### `config/`

- `ServerId` 是永久 UUID；名称和数组下标不是内部身份。
- `AppConfig.schema_version = 2`。配置中的服务器必须带有 `id` 和绝对的 `working_dir`，不满足时按 `config_invalid` 拒绝加载且不改写文件；读取失败不得退化成空配置，否则下一次保存会覆盖全部服务器。
- 每个 `ServerConfig` 有独立、绝对的 `working_dir`。JAR、launch args、world、backup 的相对路径都通过它解析，不能依赖进程当前目录。
- `default_server_dir` 只决定新建/上传落点；修改它不能移动或改写已有服务器。
- 新安装默认目录在用户数据目录下；配置中相对的 `default_server_dir` 以用户数据目录为基准，不依赖二进制位置。
- `ConfigStore::save` 使用同目录临时文件、同步、原子替换和 `.bak`。保存配置不属于仓库目录写入。

### `core/`

- `ServerManager` 同时服务 TUI 与 Web，持有配置、每服务器 `ServerProcess`、玩家、RCON 连接和后台任务。
- 生命周期状态为 `Stopped / Starting / Running / Stopping / Unknown`。状态未知时不能贸然启动第二个 JVM。
- 每服务器 `operation` 异步锁串行化启动、停止、重连、停止态目录事务、备份创建/恢复、安装器执行和删除提交；`work` 租约串行化安装、全部备份修改、目录工作和服务器移除。
- `provisioning.rs`：统一 Web/TUI 的自动安装分派、安装器定位和检测结果回写；版本默认选择、工作租约的获取时机和任务日志仍由各调用方处理；`run_registered_installer` 在内部取得生命周期锁。
- `rcon.rs`：按服务器复用 RCON 连接并执行后台玩家轮询；Source RCON 协议编解码仍由 `server/rcon.rs` 负责。
- `session.rs`：由服务器 ID 生成 tmux 会话名；重连、启动前检查和目录操作共用同一个名字。
- `validation.rs`：集中服务器/玩家校验、工作目录唯一性检查和内部路径规范化。
- `location.rs`：
  - `move` 在同文件系统使用 rename；跨文件系统使用隐藏暂存副本、结构/内容/符号链接校验，再切换配置。
  - 配置保存失败时，同文件系统移动回滚 rename，跨文件系统删除新副本并保留旧目录。
  - 配置成功后才清理旧目录；清理失败作为警告返回。
  - `relocate` 只更新同一 ID 的注册路径，不复制或删除文件，并验证原配置的核心/launch args 仍存在。
- `removal.rs`：磁盘删除先改名为暂存目录，配置保存失败时回滚，提交成功后才清理运行态和暂存目录。
- 备份创建/恢复必须经 `ServerManager::create_backup` / `restore_backup`，不能只在 UI 做一次停服状态检查后绕过生命周期锁；备份删除必须经 `delete_backup` 持有同一工作租约，但不要求停服。
- TUI 的显式删除流程可使用 `remove_server`；Web API 使用 `remove_registered_server`，默认仅移除注册，磁盘删除需要显式确认参数；无论是否删除磁盘文件，Web 移除都要求服务器已停止且没有 tmux 会话，不得隐式停服。

### `server/`

- `tmux.rs`：外部命令使用参数化 `tokio::process::Command`，精确 target pane，私有运行目录和独立测试 socket。
- `process.rs`：启动脚本、日志尾随、重连、30 秒优雅停止及最多 5 秒强杀确认。单行上限 64 KiB，重连回放尾部 256 KiB，日志约 64 MiB 轮转。
- `console.rs` / `console_buffer.rs`：清理 jline/ANSI，5000 行有界缓冲、单调 sequence 和 broadcast；慢消费者不能阻塞日志尾随。
- `rcon.rs`：Source RCON 编解码、认证、请求 ID 关联、包上限和绝对 I/O deadline；连接按服务器复用。
- `properties.rs`：保留注释、空白、顺序、重复键与 LF/CRLF/CR；`enable_rcon` 只改内存，显式 `save` 才写盘。
- `eula.rs`：检查只读；只有明确授权动作才能调用 `agree_eula`。
- `files.rs`：所有外部相对路径经统一根目录和逐组件规范化（`resolve_normalized`）校验，拒绝绝对路径、`..` 与符号链接逃逸。读取文本限 1 MiB 并拒绝二进制，写入同样限 1 MiB 且只覆盖已存在的文本文件；上传使用同目录临时文件和原子替换。
- `backup.rs`：世界备份列表、创建、恢复和删除；恢复使用暂存复制、内容校验和原子换入，失败保留或回滚旧世界。较大测试位于 `backup/tests.rs`。
- `fs_copy.rs`：crate 内通用的递归目录复制与内容校验，保留符号链接和目录权限，供世界备份与跨文件系统目录移动复用。
- `install/`：`version.rs` 处理官方 API 与解析，`download.rs` 负责流式下载和原子落盘，`installer.rs` 负责参数化 Java 与输出，`service.rs` 组合安装流程；对外入口是 `InstallService`，`compat.rs` 的模块级自由函数目前 crate 内没有调用方。常规测试使用内部本地 mock，不访问官方服务。
- `detect.rs`：按 libraries、ZIP 中央目录和确定排序的 JAR 扫描识别核心及改名安装器。

### `web/`

- `shared/`：按 `ServerId` 实现配置、控制台、properties、备份、文件和上传用例；协议适配层不得反向依赖私有 helper。
- `api.rs`：`/api` 下的服务器资源只按稳定 UUID 路由，其他 ID 形式一律拒绝；生命周期、安装、目录和删除使用可查询 operation，同步接口直接返回结果。
- `global.rs`：提供 `coretypes`、`versions`、`settings` 和 `browse` 等不依赖服务器 ID 的全局接口。
- `uploads/`：负责添加服务器的创建任务（数字任务 ID），涵盖安装、导入和目录上传；`install.rs` 负责安装/导入，`directory.rs` 负责目录上传状态机，`registration.rs` 负责注册配置，`cleanup.rs` 负责暂存清理。任务与 operation 的有界日志都通过 `log_offset` 报告已裁掉的绝对行数，前端不得把当前数组下标当作全局进度。
- `tests.rs`：`/api` 的 HTTP 契约测试，并验证无效的服务器 ID 和未知路由不能修改服务器。
- `assets.rs`：按固定顺序拼接 `assets/index/` partials；保持 classic JS 全局依赖和初始化顺序，不新增运行时静态资源请求。
- `operations.rs`：单个互斥状态保护操作表与幂等索引。`Idempotency-Key` 绑定服务器、动作和请求内容 SHA-256；同键不同请求返回 409。操作历史有界，panic 也必须落为结构化错误。`log_buffer.rs` 让 operation 与任务共享 2000 行有界日志和绝对偏移。
- 持续 SSE 最多 3 条；普通缓冲请求体上限 256 MiB，原始文件上传例外。缓冲 JSON 拒绝 chunked，原始上传支持 chunked 并周期检查磁盘空间。
- 普通上传保留 512 MiB 空间，未知长度时每 16 MiB 检查；空间不足返回 507 `no_space`。
- 上传暂存目录 30 分钟无活动后清理由受管任务执行；活动和 finalizing 任务必须受到保护。
- `daemon.rs`：在 Tokio 运行时创建前 fork，使用 PID 文件和 flock 保证单实例；父进程等绑定成功通知。退出时 detach Minecraft。
- Web 固定监听 `0.0.0.0` 且无身份认证，仅适合可信网络。

### `ui/`

- `state/`：显式页面、焦点、表单、弹窗和向导状态；`messages.rs` 按领域处理后台结果，`panels.rs` 处理面板按键，按键转换与业务执行分离；大型状态测试放在 `tests.rs`。
- `tasks/`：独立 Tokio runtime 中的受管任务；`executor.rs` 只做分派，`operations/` 调用共享 core/server 服务，不能在绘制函数中执行副作用。
- `render/`：ratatui 的 shell、widgets、向导、弹窗和面板绘制；注意中文宽度和窄终端边界。
- `input.rs`：UTF-8 安全的单行/多行编辑。
- 安装器、EULA、RCON、恢复、删除等危险动作必须明确确认，默认选择取消。
- 正常退出、错误和 panic 路径均应尽力恢复终端；退出只 detach。

## 代码风格

- Rust 遵循 `cargo fmt` 和标准 Rust 命名：类型 PascalCase，函数/变量 snake_case，模块 snake_case。
- 使用 `AppError` / `Result` 保留结构化错误码；不要把真实 I/O、权限或网络错误误报为“不存在”。
- 避免跨 `.await` 持有 `parking_lot` guard；长磁盘/安装工作使用受控 `spawn_blocking` 或受管任务。
- 新注释、UI 文案和提交信息使用中文；不要把提交视为已授权操作。
- 修改保持必要范围，沿用周边模块的错误处理和并发方式。

## 验证流程

Rust 主实现：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build
python3 tests/web_resilience.py "$PWD/target/debug/MCServerManager"
python3 tests/tui_smoke.py "$PWD/target/debug/MCServerManager"
node tests/gui_regression.js "$PWD/target/debug/MCServerManager"
node tests/frontend_state.js
```

GUI 回归需要 `playwright-core` 和对应版本的 Chromium，位置用 `PLAYWRIGHT_CORE`、`PLAYWRIGHT_BROWSERS_PATH` 指定；证据默认写入 `artifacts/gui-<时间戳>/`，用第二个参数指定目录时该目录必须不存在或为空。缺少浏览器环境时如实报告该项未运行，不得记为通过。

配套 CLI（位于仓库同级的 `../MCSMCli`）：

```bash
cargo test --manifest-path ../MCSMCli/Cargo.toml --locked
python3 ../MCSMCli/tests/contract.py
python3 ../MCSMCli/tests/backend_smoke.py "$PWD/target/debug/MCServerManager"
```

测试必须使用临时 XDG、随机端口、临时服务器目录、独立 tmux socket、本地 HTTP mock 和模拟 Java/RCON。不得读取、改写或删除真实用户配置、真实服务器和真实世界。后台任务完成通知不是用户授权。

## 安全注意事项

- 不得在未授权流程中调用 EULA 同意或 RCON 启用；普通检查和测试不得静默写这些文件。
- 不要为了测试或迁移清理当前 dirty worktree，不要执行 Git commit/push，除非用户另行明确要求。
- 外部命令保持参数数组；不要为了图方便改成 `sh -c` 或拼接未校验输入。
- `FileBrowser` 的所有新增调用都必须使用其安全方法，不能把未校验路径直接拼进文件操作。
- 删除目录、恢复备份和服务器删除具有破坏性，必须由明确用户动作触发；Web API 不得为了删除而隐式停服。
- 配置原子保存成功之前不得删除目录移动的旧数据；恢复世界前必须完成暂存副本校验。
- 下载只使用 Mojang、Paper、Forge、NeoForge、Fabric 等官方源；更换下载源必须在代码和文档中明确。
- RCON 凭据以明文保存，Web 又无认证，不能把它描述为适合不可信公网。
