"""在独立 tmux 伪终端中验收 Rust TUI，不读取真实用户配置、不启动 Minecraft。
用法：python3 tests/tui_smoke.py /absolute/path/to/MCServerManager
"""
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time
import uuid

BACKEND = Path(sys.argv[1]).resolve()


def main():
    with tempfile.TemporaryDirectory(prefix="mcsm-tui-smoke-") as temp:
        root = Path(temp).resolve()
        xdg = root / "xdg"
        config_dir = xdg / "MCServerManager"
        config_dir.mkdir(parents=True)
        home = root / "home"
        home.mkdir()
        working = root / "实例/中文 服务器"
        (working / "world").mkdir(parents=True)
        (working / "server.jar").write_bytes(b"dummy")
        (working / "world/level.dat").write_text("隔离世界", encoding="utf-8")
        properties = b"motd=terminal smoke\nenable-rcon=false\n"
        (working / "server.properties").write_bytes(properties)
        (working / "说明.txt").write_text("你好，终端", encoding="utf-8")
        config = {
            "schema_version": 2,
            "default_server_dir": str(root / "默认新建目录"),
            "global_java_path": "java",
            "web_port": 18080,
            "servers": [{
                "id": "123e4567-e89b-42d3-a456-426614174000",
                "name": "中文 服务器",
                "core_type": "custom",
                "working_dir": str(working),
                "server_jar_path": "server.jar",
                "java_path": "java",
                "rcon_port": 0,
                "rcon_password": "",
            }],
        }
        (config_dir / "config.json").write_text(
            json.dumps(config, ensure_ascii=False), encoding="utf-8"
        )
        socket_name = "mcsm-tui-harness-" + uuid.uuid4().hex
        session = "smoke"
        env = {
            **os.environ,
            "XDG_CONFIG_HOME": str(xdg),
            "XDG_DATA_HOME": str(root / "data"),
            "HOME": str(home),
            "TERM": "xterm-256color",
            "LC_ALL": "C.UTF-8",
            "RUST_LOG": "warn",
        }

        def tmux(*args, check=True):
            return subprocess.run(
                ["tmux", "-L", socket_name, *args],
                env=env,
                cwd=root,
                capture_output=True,
                text=True,
                encoding="utf-8",
                timeout=5,
                check=check,
            )

        def capture():
            result = tmux("capture-pane", "-p", "-J", "-t", session, check=False)
            return result.stdout if result.returncode == 0 else ""

        def expect(name, required):
            deadline = time.monotonic() + 10
            screen = ""
            while time.monotonic() < deadline:
                screen = capture()
                if all(text in screen for text in required):
                    (root / (name + ".txt")).write_text(screen, encoding="utf-8")
                    return screen
                time.sleep(0.1)
            raise AssertionError(f"{name} 未显示 {required!r}；最后画面：\n{screen}")

        def key(value):
            tmux("send-keys", "-t", session, value)

        try:
            command = (
                f"{shlex.quote(str(BACKEND))}; status=$?; "
                "printf '\\nTUI_EXIT:%s\\n' \"$status\"; exec /bin/sh"
            )
            tmux(
                "-f", "/dev/null", "new-session", "-d", "-s", session,
                "-x", "120", "-y", "35", command,
            )
            expect("01_initial", ["MCServerManager", "中文 服务器", "添加服务器"])

            key("a")
            expect("02_wizard", ["添加服务器", "自动安装", "导入主机目录"])
            key("Escape")
            key("g")
            expect(
                "03_global",
                ["默认服务器目录", "仅用于新建和上传", "不会修改任何已有服务器"],
            )

            key("Escape")
            key("Home")
            key("Enter")
            expect("04_overview", ["概览", "控制台", "服务器设置"])
            key("2")
            expect("05_console", ["控制台", "日志", "指令"])
            key("6")
            expect("06_files", ["server.properties", "说明.txt", "world"])
            key("7")
            expect("07_settings", ["working_dir", str(working), "F2 move", "F3 relocate"])

            key("C-c")
            expect("08_shell_restored", ["TUI_EXIT:0"])
            tmux("send-keys", "-t", session, "-l", "printf 'TERMINAL_OK\\n'")
            key("Enter")
            expect("09_shell_input", ["TERMINAL_OK"])
            assert not (working / "eula.txt").exists()
            assert (working / "server.properties").read_bytes() == properties
            assert json.loads((config_dir / "config.json").read_text())["servers"][0][
                "working_dir"
            ] == str(working)
            print(
                "真实伪终端 TUI 验收通过：向导、全局设置、概览、控制台、文件、"
                "服务器设置、中文显示及退出后终端输入恢复；未启动 Minecraft。"
            )
        finally:
            tmux("kill-server", check=False)


if __name__ == "__main__":
    main()
