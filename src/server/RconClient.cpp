#include "server/RconClient.h"
#include <chrono>
#include <cstdint>
#include <cstring>
#include <vector>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <fcntl.h>
#include <poll.h>
#include <unistd.h>

namespace mc_manager::server {

namespace {

using Clock = std::chrono::steady_clock;

// 距截止时间还剩多少毫秒（<=0 表示已超时）
int remaining_ms(Clock::time_point deadline) {
    return static_cast<int>(std::max<int64_t>(
        0, std::chrono::duration_cast<std::chrono::milliseconds>(deadline - Clock::now()).count()));
}

void write_i32_le(std::string &out, int32_t v) {
    out.push_back(static_cast<char>(v & 0xff));
    out.push_back(static_cast<char>((v >> 8) & 0xff));
    out.push_back(static_cast<char>((v >> 16) & 0xff));
    out.push_back(static_cast<char>((v >> 24) & 0xff));
}

int32_t read_i32_le(const char *p) {
    return static_cast<int32_t>(
        (static_cast<uint32_t>(static_cast<unsigned char>(p[0]))      ) |
        (static_cast<uint32_t>(static_cast<unsigned char>(p[1])) <<  8) |
        (static_cast<uint32_t>(static_cast<unsigned char>(p[2])) << 16) |
        (static_cast<uint32_t>(static_cast<unsigned char>(p[3])) << 24));
}

std::string build_packet(int32_t id, int32_t type, const std::string &payload) {
    std::string pkt;
    pkt.reserve(payload.size() + 14);
    write_i32_le(pkt, static_cast<int32_t>(payload.size()) + 10);  // 长度不含自身 4 字节
    write_i32_le(pkt, id);
    write_i32_le(pkt, type);
    pkt += payload;
    pkt.push_back('\0');
    pkt.push_back('\0');
    return pkt;
}

bool send_all(int fd, const std::string &data) {
    size_t sent = 0;
    while (sent < data.size()) {
        ssize_t n = send(fd, data.data() + sent, data.size() - sent, 0);
        if (n <= 0) return false;
        sent += static_cast<size_t>(n);
    }
    return true;
}

bool recv_exact(int fd, char *buf, size_t n) {
    size_t got = 0;
    while (got < n) {
        ssize_t r = recv(fd, buf + got, n - got, MSG_WAITALL);
        if (r <= 0) return false;
        got += static_cast<size_t>(r);
    }
    return true;
}

// 读取一个 RCON 数据包，返回 false 表示连接断开或超时
bool recv_packet(int fd, int32_t &id, int32_t &type, std::string &payload) {
    char len_buf[4];
    if (!recv_exact(fd, len_buf, 4)) return false;
    int32_t len = read_i32_le(len_buf);
    if (len < 10 || len > 1024 * 1024) return false;  // 防御性上限
    std::vector<char> body(static_cast<size_t>(len));
    if (!recv_exact(fd, body.data(), body.size())) return false;
    id = read_i32_le(body.data());
    type = read_i32_le(body.data() + 4);
    payload.assign(body.data() + 8, static_cast<size_t>(len - 10));  // 去掉尾部两个 \0
    return true;
}

// 为套接字设置收发超时
void set_io_timeout(int fd, int timeout_ms) {
    timeval tv{timeout_ms / 1000, (timeout_ms % 1000) * 1000};
    setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
    setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &tv, sizeof(tv));
}

} // namespace

RconClient::~RconClient() {
    close();
}

void RconClient::close() {
    if (fd_ >= 0) {
        ::close(fd_);
        fd_ = -1;
    }
}

bool RconClient::open(int port, const std::string &password, int timeout_ms) {
    if (fd_ >= 0) {
        // 凭据变化时重建连接
        if (port == port_ && password == password_) return true;
        close();
    }
    if (port <= 0 || port > 65535 || password.empty()) return false;

    auto deadline = Clock::now() + std::chrono::milliseconds(timeout_ms);

    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) return false;

    // 非阻塞 connect + poll，保证连接阶段也受超时约束
    int flags = fcntl(fd, F_GETFL, 0);
    fcntl(fd, F_SETFL, flags | O_NONBLOCK);

    sockaddr_in addr{};
    addr.sin_family = AF_INET;
    addr.sin_port = htons(static_cast<uint16_t>(port));
    inet_pton(AF_INET, "127.0.0.1", &addr.sin_addr);

    int rc = connect(fd, reinterpret_cast<sockaddr *>(&addr), sizeof(addr));
    if (rc < 0 && errno != EINPROGRESS) {
        ::close(fd);
        return false;
    }
    if (rc < 0) {
        pollfd pfd{fd, POLLOUT, 0};
        int pr = poll(&pfd, 1, std::max(1, remaining_ms(deadline)));
        int err = 0;
        socklen_t elen = sizeof(err);
        if (pr <= 0 || getsockopt(fd, SOL_SOCKET, SO_ERROR, &err, &elen) < 0 || err != 0) {
            ::close(fd);
            return false;
        }
    }

    // 恢复阻塞模式，收发统一用 SO_RCVTIMEO/SO_SNDTIMEO 限时
    fcntl(fd, F_SETFL, flags & ~O_NONBLOCK);
    set_io_timeout(fd, std::max(1, remaining_ms(deadline)));

    // 认证：id=1, type=3(SERVERDATA_AUTH)
    bool authed = false;
    if (send_all(fd, build_packet(1, 3, password))) {
        // 服务器可能先发一个空 RESPONSE_VALUE 再发 AUTH_RESPONSE，逐个读取
        for (int i = 0; i < 4; ++i) {
            int32_t id = 0, type = 0;
            std::string payload;
            if (!recv_packet(fd, id, type, payload)) break;
            if (type == 2) {  // AUTH_RESPONSE：id 为 -1 表示认证失败
                authed = (id == 1);
                break;
            }
        }
    }

    if (!authed) {
        ::close(fd);
        return false;
    }

    fd_ = fd;
    port_ = port;
    password_ = password;
    return true;
}

std::optional<std::string> RconClient::execute(const std::string &command, int timeout_ms) {
    if (fd_ < 0) return std::nullopt;

    set_io_timeout(fd_, timeout_ms);

    std::optional<std::string> result;
    // 执行命令：id=2, type=2(SERVERDATA_EXECCOMMAND)
    if (send_all(fd_, build_packet(2, 2, command))) {
        for (int i = 0; i < 4; ++i) {
            int32_t rid = 0, rtype = 0;
            std::string rpayload;
            if (!recv_packet(fd_, rid, rtype, rpayload)) break;
            if (rid == 2 && rtype == 0) {
                result = std::move(rpayload);
                break;
            }
        }
    }

    if (!result) {
        close();  // 读写失败/超时：关闭连接，等待下一轮重建
    }
    return result;
}

} // namespace mc_manager::server
