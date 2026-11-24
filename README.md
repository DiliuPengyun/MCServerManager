# MCServerManager

轻量的 Linux 单机 Minecraft 服务器管理器，使用 **Rust 2024** 编写，提供 ratatui 全屏 TUI 与内嵌单页 Web UI；服务器由 **tmux** 持久运行，不使用数据库。

## 功能

- **服务器管理**：启动、停止、重启、重连已有 tmux 会话；退出管理器只 detach，不停止 Minecraft。
- **独立工作目录**：每台服务器都有稳定的 UUID 和任意位置的 `working_dir`。
- **目录迁移**：停服后可由管理器移动整个目录；跨文件系统时先复制并校验。用户已手动搬迁时，可只重新定位注册路径。
- **默认服务器目录**：仅决定以后新建和上传服务器的落点；修改它不会移动或改写已有服务器路径。
- **核心与安装**：支持 Vanilla、Paper、Forge、NeoForge、Fabric、自定义核心及已有 Spigot 核心识别；自动安装使用各项目官方 API，其中 Paper 使用官方 Fill v3 下载服务。Spigot 暂不提供自动安装接口，可导入已有目录或 JAR。
- **控制台**：5000 行有界缓冲、SSE、滚动日志、指令输入、ANSI/jline 清理、重连尾部回放和日志轮转。
- **玩家管理**：通过复用长连接的 RCON 周期查询在线玩家，支持踢出和封禁。
- **文件管理**：浏览、文本编辑、新建、重命名、删除及二进制流式上传；统一校验服务器根目录，拒绝 `..` 和符号链接逃逸。
- **备份/恢复**：世界快照使用暂存复制与内容校验；恢复失败保留或回滚旧世界。创建和恢复需要停服，并与启动、安装和目录移动串行执行；删除备份不需要停服。
- **配置编辑**：保留 `server.properties` 注释、顺序、重复键和换行布局。
- **创建向导**：自动安装、导入主机目录、运行现有安装器、自定义 JAR；Web 还支持上传浏览器所在电脑的整个目录。

## 构建

需要支持 Rust 2024 的近期 stable Rust 工具链和 Cargo：

```bash
cargo build --release
```

产物：

```text
target/release/MCServerManager
```

开发模式可直接使用：

```bash
cargo build
```

## 源码结构

- `src/core/`：共享服务器注册表、安装编排、目录移动及删除事务；`provisioning.rs` 统一 Web/TUI 的核心安装分派，RCON 轮询、tmux 会话匹配和配置校验分别位于独立内部模块。
- `src/server/`：tmux、进程、控制台、RCON、文件、备份和核心检测；`fs_copy.rs` 提供 crate 内通用目录复制校验，较大的世界备份测试位于 `backup/tests.rs`。
- `src/server/install/`：按版本源、下载、Java 安装器和安装服务划分，对外入口为 `InstallService`。
- `src/web/`：`api.rs` 提供以 UUID 标识服务器的 `/api` 协议，`global.rs` 提供设置、版本和路径浏览；相同配置、控制台、文件、备份和上传用例放在 `web/shared/`，`uploads/` 按创建安装、目录上传、注册和暂存清理划分。
- `src/ui/`：`state/`、`tasks/`、`render/` 分别负责状态转换、后台命令和 ratatui 绘制，按键与绘制不直接重写业务逻辑。
- `assets/index/`：按页面领域划分的 HTML/CSS/classic JavaScript partials，由 `web/assets.rs` 在编译期拼接为同一个内嵌页面；不需要 npm，运行时也不请求额外资源。

## 运行

默认启动 TUI：

```bash
./target/release/MCServerManager
```

启动 Web 守护进程：

```bash
./target/release/MCServerManager web 8080
```

前台运行 Web，便于开发或调试：

```bash
./target/release/MCServerManager web 8080 --foreground
```

Web 固定监听 `0.0.0.0`（所有网卡），且不提供身份认证；只应在可信网络中运行，并用防火墙限制访问。运行时还需要：

- `tmux`：必需，启动时检查。
- `java`：运行 Minecraft；可在全局或单服务器设置中指定路径。

## 配置与目录语义

配置文件：

```text
$XDG_CONFIG_HOME/MCServerManager/config.json
```

未设置 XDG 时为：

```text
~/.config/MCServerManager/config.json
```

新安装的默认服务器目录位于 `$XDG_DATA_HOME/MCServerManager/servers`，未设置 XDG 时为 `~/.local/share/MCServerManager/servers`。

JAR、launch args、world 和 backup 的相对路径统一相对于该服务器的 `working_dir` 解析。

## HTTP API 与 CLI

- `/api` 下的服务器资源都以稳定 UUID 标识。`GET /api/capabilities` 返回 `schema_version: 2` 和协议能力。
- 生命周期、已有安装器、目录 `move`/`relocate` 和删除使用可查询 operation，并支持指定写操作的幂等键；安装器运行日志通过有界 `log` / `log_offset` 增量读取。
- 添加服务器和目录上传使用 `/api/tasks` 与 `/api/uploads` 的任务会话：任务日志可轮询查看，目录上传任务可以取消，失败后可复用未过期的暂存任务重试，闲置暂存目录会被定期清理；任务日志同样通过 `log_offset` 报告已裁掉的绝对行数。
- `DELETE /api/servers/{id}` 默认只移除注册，不触碰磁盘文件。删除磁盘文件必须同时提供 `delete_files=true` 和与完整 `working_dir` 一致的 `confirm`。两种删除都要求服务器已停止且没有对应 tmux 会话，管理器不会为了删除而自动停服。
- 配套 [`MCSMCli`](../MCSMCli) 会先请求 `/api/capabilities`；能力缺失、网络错误或非法响应都会直接失败。

## EULA 与 RCON 授权

检查启动条件不会修改服务器文件：

- 只有用户明确同意 Minecraft EULA 后，才会写入 `eula.txt`。
- 只有用户明确选择启用 RCON 后，才会写入 `server.properties`、选定端口、生成或复用密码，并保存凭据副本。
- 用户也可以选择不启用 RCON 直接启动，此时玩家管理不可用。

RCON 凭据以明文保存在 `config.json` 和 `server.properties` 中，应注意本机文件权限和配置分发。

## 验证

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
node tests/frontend_state.js
```

隔离 Web 压力测试（使用临时 XDG、随机端口和临时目录，包含大于 256 MiB 的流式上传）：

```bash
python3 tests/web_resilience.py "$PWD/target/debug/MCServerManager"
```

真实伪终端 TUI 验收（独立 tmux socket，不启动 Minecraft）：

```bash
python3 tests/tui_smoke.py "$PWD/target/debug/MCServerManager"
```

真实浏览器 GUI 回归（临时 XDG、HOME 和随机端口，不启动 Minecraft）。需要 `playwright-core` 和对应版本的 Chromium，位置分别由环境变量 `PLAYWRIGHT_CORE` 和 `PLAYWRIGHT_BROWSERS_PATH` 指定。截图和页面文字写入 `artifacts/gui-<时间戳>/`，也可以用第二个参数指定一个不存在或为空的目录：

```bash
node tests/gui_regression.js "$PWD/target/debug/MCServerManager"
```

截图里的中文依赖系统可用的中文字体；断言只检查页面文字和状态，不受字体影响。

配套 CLI 与真实后端的联调：

```bash
python3 ../MCSMCli/tests/backend_smoke.py "$PWD/target/debug/MCServerManager"
```

常规测试不访问官方下载服务，不操作真实用户配置、真实世界、EULA 或 RCON。

## 免责声明

本项目代码 100% 由 AI 生成。
