# MCServerManager

基于 FTXUI 的 Minecraft 服务器管理器，运行在 Linux 终端中。

## 功能

- **服务器管理**：通过 tmux 会话启动/停止/重启 Minecraft 服务器
- **多核心支持**：Vanilla、Paper(目前有问题)、Forge、NeoForge、Fabric
- **自动安装**：从网络下载并安装服务器核心，支持版本选择
- **控制台**：可滚动日志输出和指令输入
- **玩家管理**：查看在线玩家、踢出、封禁
- **备份/恢复**：世界备份与恢复，带时间戳快照
- **配置编辑**：server.properties 图形化编辑器
- **向导式创建**：逐步引导创建新服务器

## 构建

```bash
xmake build
```

依赖：[FTXUI](https://github.com/ArthurSonzogni/FTXUI)、[nlohmann/json](https://github.com/nlohmann/json)、tmux、Java。

## 运行

```bash
xmake run MCServerManager
```

## 免责声明

本项目代码 100% 由 AI 生成。
