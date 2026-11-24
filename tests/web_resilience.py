"""隔离验证 Web 资源限制；不启动 Minecraft、不读取用户配置。
用法：python3 tests/web_resilience.py /absolute/path/to/MCServerManager
"""
import concurrent.futures
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time
from urllib.request import urlopen

BACKEND = Path(sys.argv[1]).resolve()
MAX_SSE = 3


def read_headers(sock):
    data = bytearray()
    while b"\r\n\r\n" not in data:
        chunk = sock.recv(4096)
        if not chunk:
            raise RuntimeError("响应头未完成")
        data.extend(chunk)
        if len(data) > 65536:
            raise RuntimeError("响应头异常增大")
    head = bytes(data).split(b"\r\n\r\n", 1)[0].decode("iso-8859-1")
    status = int(head.splitlines()[0].split()[1])
    return status


def raw_request(port, request):
    sock = socket.create_connection(("127.0.0.1", port), timeout=5)
    sock.settimeout(5)
    try:
        sock.sendall(request)
        return sock, read_headers(sock)
    except Exception:
        sock.close()
        raise


def request_line(method, path):
    return f"{method} {path} HTTP/1.1\r\n".encode()


def open_sse(port, server_id):
    sock = socket.create_connection(("127.0.0.1", port), timeout=5)
    sock.settimeout(5)
    try:
        sock.sendall(
            request_line("GET", f"/api/servers/{server_id}/console/stream")
            + f"Host: 127.0.0.1:{port}\r\n".encode()
            + b"Accept: text/event-stream\r\nConnection: keep-alive\r\n\r\n"
        )
        return sock, open_status(sock)
    except Exception:
        sock.close()
        raise


def open_status(sock):
    return read_headers(sock)


def main():
    with tempfile.TemporaryDirectory(prefix="mcsm-web-resilience-") as temp:
        root = Path(temp).resolve()
        xdg = root / "xdg"
        config_dir = xdg / "MCServerManager"
        config_dir.mkdir(parents=True)
        working = root / "dummy-server"
        working.mkdir()
        (working / "server.jar").write_bytes(b"dummy")
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        config = {
            "schema_version": 2, "default_server_dir": str(root), "web_port": port,
            "global_java_path": "java",
            "servers": [{
                "id": "123e4567-e89b-42d3-a456-426614174000",
                "name": "隔离测试服", "core_type": "vanilla",
                "server_jar_path": str(working / "server.jar"),
                "working_dir": str(working), "java_path": "java",
                "rcon_port": 0, "rcon_password": "",
            }],
        }
        (config_dir / "config.json").write_text(json.dumps(config), encoding="utf-8")
        env = {**os.environ, "XDG_CONFIG_HOME": str(xdg), "XDG_DATA_HOME": str(root / "data")}
        started = subprocess.run([str(BACKEND), "web", str(port)], env=env, cwd=root,
                                 capture_output=True, text=True, timeout=15)
        if started.returncode:
            raise RuntimeError(started.stdout + started.stderr)
        pid = int((config_dir / "web.pid").read_text())
        sockets = []
        host = f"http://127.0.0.1:{port}"

        def healthy():
            with urlopen(host + "/api/servers", timeout=3) as response:
                return response.status == 200 and len(json.load(response)) == 1

        try:
            deadline = time.monotonic() + 10
            while not healthy() and time.monotonic() < deadline:
                time.sleep(0.1)

            assert healthy(), "Web 服务启动后不可用"
            with urlopen(host + "/api/servers", timeout=3) as response:
                server_id = json.load(response)[0]["id"]
            unknown, status = raw_request(
                port,
                request_line("GET", "/api/unknown")
                + f"Host: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n".encode(),
            )
            sockets.append(unknown)
            assert status == 404, "未知的 /api 路由应返回 404"
            invalid_id, status = open_sse(port, "0")
            sockets.append(invalid_id)
            assert status == 400, "无效的服务器 ID 不应被接受"
            time.sleep(11)
            assert healthy(), "Web 服务持续运行超过 10 秒后退出"
            with urlopen(host + "/favicon.svg", timeout=3) as response:
                assert response.status == 200
                assert response.headers.get_content_type() == "image/svg+xml"
            print("PASS: Web 持续运行超过 10 秒，favicon 可用", flush=True)

            with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
                opened = list(pool.map(lambda _: open_sse(port, server_id), range(8)))
            sockets = [item[0] for item in opened]
            statuses = [item[1] for item in opened]
            assert statuses.count(200) == MAX_SSE, statuses
            assert statuses.count(503) == 8 - MAX_SSE, statuses
            print("SSE statuses:", statuses, flush=True)
            try:
                assert healthy(), "SSE 饱和后 REST 不可用"
            except Exception:
                try:
                    with urlopen(host + "/", timeout=3) as response:
                        print("DIAG static root:", response.status, flush=True)
                except Exception as root_error:
                    print("DIAG static root failed:", repr(root_error), flush=True)
                time.sleep(6)
                try:
                    print("DIAG REST after 6s:", healthy(), flush=True)
                except Exception as retry_error:
                    print("DIAG REST after 6s failed:", repr(retry_error), flush=True)
                raise

            # 超过协议层上限：只发送请求头，服务应立即拒绝而不是等待/缓冲请求体。
            large = socket.create_connection(("127.0.0.1", port), timeout=5)
            large.settimeout(5)
            sockets.append(large)
            large.sendall(
                request_line("POST", f"/api/servers/{server_id}/command")
                + f"Host: 127.0.0.1:{port}\r\n".encode()
                + b"Content-Type: application/json\r\n"
                + b"Content-Length: 268435457\r\nConnection: close\r\n\r\n"
            )
            assert read_headers(large) == 413, "超大请求体未被协议层拒绝"

            cases = [
                (
                    "chunked 块早期拒绝",
                    413,
                    request_line("POST", f"/api/servers/{server_id}/command")
                    + f"Host: 127.0.0.1:{port}\r\n".encode()
                    + b"Content-Type: application/json\r\n"
                    + b"Transfer-Encoding: chunked\r\n\r\n10000001\r\n",
                ),
                (
                    "磁盘无法容纳时拒绝",
                    507,
                    request_line("POST", f"/api/servers/{server_id}/files/upload?path=too-big.bin")
                    + f"Host: 127.0.0.1:{port}\r\n".encode()
                    + b"Content-Type: application/octet-stream\r\n"
                    + b"Content-Length: 9223372036854775807\r\n\r\n",
                ),
            ]
            for name, expected, request in cases:
                sock, status = raw_request(port, request)
                sockets.append(sock)
                assert status == expected, f"{name}失败：HTTP {status}"
                assert healthy(), f"{name}后 REST 不可用"

            # 大于旧 256 MiB 固定限制的文件，只要磁盘能容纳就应成功。
            upload_size = 256 * 1024 * 1024 + 1
            upload = socket.create_connection(("127.0.0.1", port), timeout=5)
            upload.settimeout(30)
            sockets.append(upload)
            upload.sendall(
                request_line("POST", f"/api/servers/{server_id}/files/upload?path=big.bin")
                + f"Host: 127.0.0.1:{port}\r\n".encode()
                + b"Content-Type: application/octet-stream\r\n"
                + f"Content-Length: {upload_size}\r\n".encode()
                + b"Connection: close\r\n\r\n"
            )
            block = b"x" * (1024 * 1024)
            for _ in range(256):
                upload.sendall(block)
            upload.sendall(b"x")
            assert read_headers(upload) == 200, "磁盘充足时大文件上传失败"
            assert (working / "big.bin").stat().st_size == upload_size
            assert healthy(), "大文件上传后 REST 不可用"
            print("PASS: SSE、普通请求内存上限、大文件流式上传、磁盘空间保护")
        finally:
            os.kill(pid, signal.SIGTERM)
            deadline = time.monotonic() + 15
            while (config_dir / "web.pid").exists() and time.monotonic() < deadline:
                time.sleep(0.1)
            if (config_dir / "web.pid").exists():
                os.kill(pid, signal.SIGKILL)
                raise RuntimeError(f"带活动 SSE 的后台未按时退出，PID={pid}")
            for sock in sockets:
                try:
                    sock.close()
                except OSError:
                    pass
            print("PASS: 活动 SSE 存在时优雅退出")


if __name__ == "__main__":
    main()
