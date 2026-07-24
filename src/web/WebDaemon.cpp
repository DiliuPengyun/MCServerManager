#include "web/WebDaemon.h"
#include "web/WebServer.h"
#include "core/ServerManager.h"
#include "config/AppConfig.h"
#include <iostream>
#include <fstream>
#include <string>
#include <thread>
#include <unistd.h>
#include <fcntl.h>
#include <csignal>
#include <pthread.h>
#include <sys/wait.h>

namespace mc_manager::web {

namespace {

// 生成 32 位十六进制访问令牌
std::string generate_token() {
    unsigned char buf[16] = {0};
    std::ifstream urandom("/dev/urandom", std::ios::binary);
    urandom.read(reinterpret_cast<char *>(buf), sizeof(buf));
    static const char hex[] = "0123456789abcdef";
    std::string token;
    token.reserve(sizeof(buf) * 2);
    for (unsigned char b : buf) {
        token += hex[b >> 4];
        token += hex[b & 0x0f];
    }
    return token;
}

} // namespace

int run_web_daemon(int port_override) {
    // 1) 预读配置（此阶段不创建任何线程，之后 fork 才安全）
    config::AppConfig cfg;
    cfg.load();
    if (port_override > 0) cfg.web_port = port_override;
    if (cfg.web_token.empty()) cfg.web_token = generate_token();
    cfg.save();

    const int port = cfg.web_port;
    const std::string token = cfg.web_token;
    const std::string config_dir = config::AppConfig::config_dir_path();
    const std::string log_path = config_dir + "/web.log";
    const std::string pid_path = config_dir + "/web.pid";

    // 单实例检查：内置 httplib 在 Linux 上会设置 SO_REUSEPORT，
    // 重复启动不会报端口占用，而是两个进程分摊连接（状态各自为政），
    // 因此通过 pid 文件阻止第二个实例
    {
        std::ifstream pidfile(pid_path);
        pid_t old_pid = 0;
        if ((pidfile >> old_pid) && old_pid > 0 && kill(old_pid, 0) == 0) {
            std::cerr << "MCServerManager: Web 服务已在运行 (PID " << old_pid
                      << ")。如需重启，请先执行 kill " << old_pid << std::endl;
            return 1;
        }
    }

    // 2) 父子进程同步管道：子进程绑定端口成功后通知父进程
    int pipefd[2];
    if (pipe(pipefd) != 0) {
        std::cerr << "MCServerManager: pipe() 失败" << std::endl;
        return 1;
    }

    pid_t pid = fork();
    if (pid < 0) {
        std::cerr << "MCServerManager: fork() 失败" << std::endl;
        return 1;
    }

    if (pid > 0) {
        // 父进程：等待子进程绑定结果，打印访问信息后退出，不阻塞终端
        close(pipefd[1]);
        char result = 0;
        ssize_t n = read(pipefd[0], &result, 1);
        close(pipefd[0]);
        if (n > 0 && result == 'O') {
            std::cout << "MCServerManager Web UI 已在后台启动 (PID " << pid << ")\n"
                      << "本机访问:   http://127.0.0.1:" << port << "/?token=" << token << "\n"
                      << "局域网访问: http://<本机IP>:" << port << "/?token=" << token << "\n"
                      << "日志文件:   " << log_path << "\n"
                      << "停止服务:   kill " << pid << std::endl;
            return 0;
        }
        std::cerr << "MCServerManager: Web 服务启动失败，端口 " << port
                  << " 可能被占用（详见 " << log_path << "）" << std::endl;
        int status = 0;
        waitpid(pid, &status, 0);
        return 1;
    }

    // 3) 子进程：脱离终端，静默运行
    close(pipefd[0]);
    setsid();

    int devnull = open("/dev/null", O_RDONLY);
    if (devnull >= 0) {
        dup2(devnull, STDIN_FILENO);
        if (devnull > STDERR_FILENO) close(devnull);
    }
    int logfd = open(log_path.c_str(), O_WRONLY | O_CREAT | O_APPEND, 0644);
    if (logfd >= 0) {
        dup2(logfd, STDOUT_FILENO);
        dup2(logfd, STDERR_FILENO);
        if (logfd > STDERR_FILENO) close(logfd);
    }

    {
        std::ofstream pidfile(pid_path);
        pidfile << getpid() << "\n";
    }

    // 先阻塞终止信号，保证后续创建的工作线程都继承该掩码，
    // 再由专用线程 sigwait 处理，避免在信号处理函数里调用非异步安全函数
    sigset_t term_set;
    sigemptyset(&term_set);
    sigaddset(&term_set, SIGTERM);
    sigaddset(&term_set, SIGINT);
    pthread_sigmask(SIG_BLOCK, &term_set, nullptr);

    // fork 之后才创建线程（attach 服务器会话会起日志尾随线程）
    core::ServerManager manager;
    manager.load();

    WebServer server(manager, WebOptions{"0.0.0.0", port});
    server.set_token(token);

    if (server.bind() < 0) {
        char fail = 'E';
        ssize_t unused = write(pipefd[1], &fail, 1);
        (void)unused;
        _exit(1);
    }
    char ok = 'O';
    ssize_t unused = write(pipefd[1], &ok, 1);
    (void)unused;
    close(pipefd[1]);

    std::thread([&server, &term_set] {
        int sig = 0;
        sigwait(&term_set, &sig);
        server.stop();
    }).detach();

    server.run();

    // 与 TUI 退出一致：detach 而不是停止，让服务器继续运行
    manager.shutdown();
    unlink(pid_path.c_str());
    _exit(0);
}

} // namespace mc_manager::web
