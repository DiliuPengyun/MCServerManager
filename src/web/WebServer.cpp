#include "web/WebServer.h"
#include "web/WebPage.h"
#include "core/ServerManager.h"
#include "server/CoreType.h"
#include "server/CoreDetector.h"
#include "server/CoreInstaller.h"
#include "server/VersionManifest.h"
#include "utils/IniParser.h"
#include "httplib.h"
#include <nlohmann/json.hpp>
#include <algorithm>
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <deque>
#include <filesystem>
#include <mutex>
#include <thread>

namespace mc_manager::web {

namespace {

std::string state_to_string(server::ServerState state) {
    switch (state) {
        case server::ServerState::STOPPED:  return "STOPPED";
        case server::ServerState::STARTING: return "STARTING";
        case server::ServerState::RUNNING:  return "RUNNING";
        case server::ServerState::STOPPING: return "STOPPING";
    }
    return "STOPPED";
}

// 服务器名会作为目录名使用，禁止路径分隔符与父目录引用
bool is_valid_server_name(const std::string &name) {
    if (name.empty() || name == "." || name == "..") return false;
    return name.find('/') == std::string::npos && name.find('\\') == std::string::npos &&
           name.find("..") == std::string::npos;
}

} // namespace

struct WebServer::Impl {
    core::ServerManager &manager;
    WebOptions options;
    std::string token;
    httplib::Server svr;

    // 保护 manager 的配置与运行态容器（httplib 工作线程 + 安装线程并发访问）。
    // 注意：stop/restart 最长可能持锁约 30 秒（等待服务器退出），期间其他请求会等待。
    std::mutex api_mutex;

    // 异步安装任务（添加服务器向导）
    struct InstallTask {
        std::mutex mutex;
        std::deque<std::string> log;
        std::string status{"running"};  // running / done / error
        std::string error;
    };
    std::mutex tasks_mutex;
    std::map<size_t, std::shared_ptr<InstallTask>> tasks;
    size_t next_task_id{1};

    // SSE 连接的逐连接行队列
    struct LineQueue {
        std::mutex mutex;
        std::condition_variable cv;
        std::deque<std::string> lines;
    };

    // 置位后 SSE 处理循环尽快退出，使 stop() 不被活动连接卡住
    std::atomic<bool> shutting_down{false};

    Impl(core::ServerManager &m, WebOptions o) : manager(m), options(std::move(o)) {}

    bool check_auth(const httplib::Request &req) const {
        if (token.empty()) return false;
        if (req.get_header_value("Authorization") == "Bearer " + token) return true;
        if (req.has_param("token") && req.get_param_value("token") == token) return true;
        return false;
    }

    static void json_reply(httplib::Response &res, const nlohmann::json &j, int status = 200) {
        res.status = status;
        res.set_content(j.dump(), "application/json; charset=utf-8");
    }

    bool valid_index(const httplib::Request &req, httplib::Response &res, size_t &idx) {
        idx = static_cast<size_t>(std::stoul(req.matches[1]));
        if (idx >= manager.server_count()) {
            json_reply(res, {{"error", "invalid_index"}}, 404);
            return false;
        }
        return true;
    }

    nlohmann::json server_to_json(size_t i) {
        const auto &sc = manager.config().servers[i];
        return {
            {"index", i},
            {"name", sc.name},
            {"core_id", server::get_core_type_info(sc.core_type).id},
            {"core_name", server::get_core_type_info(sc.core_type).display_name},
            {"mc_version", sc.minecraft_version},
            {"core_version", sc.core_version},
            {"working_dir", sc.working_dir},
            {"server_jar_path", sc.server_jar_path},
            {"java_path", sc.java_path},
            {"min_memory_mb", sc.min_memory_mb},
            {"max_memory_mb", sc.max_memory_mb},
            {"additional_jvm_args", sc.additional_jvm_args},
            {"backup_dir", sc.backup_dir},
            {"world_dir", sc.world_dir},
            {"state", state_to_string(manager.state(i))},
            {"players", manager.players(i)},
        };
    }

    void task_log(const std::shared_ptr<InstallTask> &task, const std::string &line) {
        std::lock_guard<std::mutex> lock(task->mutex);
        if (task->log.size() >= 2000) task->log.pop_front();
        task->log.push_back(line);
    }

    // 后台执行安装并在成功后注册服务器（语义与 AddServerPanel 一致）
    void run_install_task(std::shared_ptr<InstallTask> task, nlohmann::json body) {
        const std::string method = body["method"];
        const std::string name = body["name"];
        const std::string java = body.value("java_path", "").empty() ? "java" : body.value("java_path", "");
        const int min_mem = body.value("min_memory_mb", 1024);
        const int max_mem = body.value("max_memory_mb", 4096);
        const std::string jvm_args = body.value("jvm_args", "");

        auto progress = [this, task](const std::string &line) { task_log(task, line); };

        std::string parent;
        {
            std::lock_guard<std::mutex> lock(api_mutex);
            parent = manager.config().get_effective_servers_parent_folder();
        }
        const std::string target_dir = (std::filesystem::path(parent) / name).string();

        server::InstallResult result;
        try {
            if (method == "auto") {
                const auto core_type = server::core_type_from_id(body.value("core_type", "custom"));
                const std::string version = body.value("mc_version", "");
                switch (core_type) {
                case server::CoreType::VANILLA:
                    result = server::install_vanilla(version, target_dir, java, progress);
                    break;
                case server::CoreType::PAPER:
                    result = server::install_paper(version, "latest", target_dir, java, progress);
                    break;
                case server::CoreType::NEOFORGE: {
                    auto neo_versions = server::fetch_neoforge_versions(version);
                    if (!neo_versions.empty()) {
                        result = server::install_neoforge(version, neo_versions.back().core_version,
                                                          target_dir, java, progress);
                    } else {
                        result.error = "No NeoForge version found for " + version;
                    }
                    break;
                }
                case server::CoreType::FORGE:
                    result = server::install_forge(version, version, target_dir, java, progress);
                    break;
                case server::CoreType::FABRIC:
                    result = server::install_fabric(version, "1.0.1", target_dir, java, progress);
                    break;
                default:
                    result.error = "该核心类型不支持自动安装";
                    break;
                }
            } else if (method == "installer") {
                result = server::run_installer_jar(body["path"], target_dir, java, progress);
            } else if (method == "import") {
                auto detection = server::detect_core(body["path"]);
                if (detection) {
                    result.success = true;
                    result.detection = *detection;
                } else {
                    result.error = "无法在该目录中识别服务端核心: " + body["path"].get<std::string>();
                }
            } else if (method == "jar") {
                std::filesystem::create_directories(target_dir);
                std::filesystem::path source(body["path"].get<std::string>());
                auto dest = std::filesystem::path(target_dir) / source.filename();
                std::error_code ec;
                std::filesystem::copy_file(source, dest,
                                           std::filesystem::copy_options::overwrite_existing, ec);
                if (ec) {
                    result.error = "复制 JAR 失败: " + ec.message();
                } else {
                    auto detection = server::detect_core(target_dir);
                    if (detection) {
                        result.success = true;
                        result.detection = *detection;
                    } else {
                        result.success = true;
                        result.detection.core_type = server::CoreType::CUSTOM;
                        result.detection.server_jar_path = dest.string();
                    }
                }
            } else {
                result.error = "未知的添加方式";
            }
        } catch (const std::exception &e) {
            result.success = false;
            result.error = std::string("安装过程异常: ") + e.what();
        }

        if (!result.success) {
            task_log(task, "错误: " + result.error);
            std::lock_guard<std::mutex> lock(task->mutex);
            task->status = "error";
            task->error = result.error;
            return;
        }

        // 组装 ServerConfig（与 AddServerPanel::build_server_config 一致）
        config::ServerConfig sc;
        sc.name = name;
        sc.java_path = java;
        sc.min_memory_mb = min_mem;
        sc.max_memory_mb = max_mem;
        sc.additional_jvm_args = jvm_args;
        sc.core_type = result.detection.core_type;
        sc.minecraft_version = result.detection.minecraft_version;
        sc.core_version = result.detection.core_version;
        sc.launch_args_file = result.detection.launch_args_file;

        if (method == "import") {
            // 规范化导入路径（解析符号链接、去掉尾部分隔符）
            std::error_code ec;
            auto canonical = std::filesystem::weakly_canonical(body["path"].get<std::string>(), ec);
            std::string dir = (ec ? std::filesystem::absolute(body["path"].get<std::string>())
                                  : canonical).string();
            sc.server_jar_path = result.detection.server_jar_path;
            sc.working_dir = dir;
            sc.backup_dir = (std::filesystem::path(dir) / "backups").string();
            sc.world_dir = (std::filesystem::path(dir) / "world").string();
        } else {
            std::filesystem::path server_dir = std::filesystem::absolute(target_dir);
            if (!result.detection.server_jar_path.empty()) {
                sc.server_jar_path = result.detection.server_jar_path;
            } else if (!server::get_core_type_info(sc.core_type).uses_launch_args_file) {
                auto detection = server::detect_core(server_dir.string());
                if (detection && !detection->server_jar_path.empty()) {
                    sc.server_jar_path = detection->server_jar_path;
                }
            }
            sc.working_dir = server_dir.string();
            sc.backup_dir = (server_dir / "backups").string();
            sc.world_dir = (server_dir / "world").string();
        }

        {
            std::lock_guard<std::mutex> lock(api_mutex);
            manager.add_server(sc);
        }
        task_log(task, "服务器已添加: " + name);
        std::lock_guard<std::mutex> lock(task->mutex);
        task->status = "done";
    }

    void setup_routes() {
        svr.set_pre_routing_handler([this](const httplib::Request &req, httplib::Response &res) {
            if (req.path.rfind("/api/", 0) == 0 && !check_auth(req)) {
                json_reply(res, {{"error", "unauthorized"}}, 401);
                return httplib::Server::HandlerResponse::Handled;
            }
            return httplib::Server::HandlerResponse::Unhandled;
        });

        svr.Get("/", [](const httplib::Request &, httplib::Response &res) {
            res.set_content(kIndexHtml, "text/html; charset=utf-8");
        });

        // ---- 服务器列表 / 状态 ----
        svr.Get("/api/servers", [this](const httplib::Request &, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            auto arr = nlohmann::json::array();
            for (size_t i = 0; i < manager.server_count(); ++i) {
                arr.push_back(server_to_json(i));
            }
            json_reply(res, arr);
        });

        // ---- 核心类型与版本清单（添加服务器向导）----
        svr.Get("/api/coretypes", [](const httplib::Request &, httplib::Response &res) {
            auto arr = nlohmann::json::array();
            for (const auto &info : server::CORE_TYPES) {
                arr.push_back({
                    {"id", info.id},
                    {"name", info.display_name},
                    {"uses_installer", info.uses_installer},
                });
            }
            json_reply(res, arr);
        });

        svr.Get("/api/versions", [this](const httplib::Request &req, httplib::Response &res) {
            const auto core_type = server::core_type_from_id(
                req.has_param("core") ? req.get_param_value("core") : "");
            auto arr = nlohmann::json::array();
            try {
                switch (core_type) {
                case server::CoreType::VANILLA:
                    for (const auto &v : server::fetch_vanilla_versions()) {
                        if (v.release_type == "release") {
                            arr.push_back({{"id", v.id}, {"label", v.id + " (release)"}});
                        }
                    }
                    break;
                case server::CoreType::PAPER:
                    for (const auto &v : server::fetch_paper_versions()) {
                        arr.push_back({{"id", v.id}, {"label", v.id}});
                    }
                    break;
                case server::CoreType::NEOFORGE:
                    for (const char *v : {"1.21.1", "1.21", "1.20.6", "1.20.4", "1.20.1"}) {
                        arr.push_back({{"id", v}, {"label", v}});
                    }
                    break;
                case server::CoreType::FORGE:
                    for (const auto &v : server::fetch_forge_versions()) {
                        arr.push_back({{"id", v.id}, {"label", v.id}});
                    }
                    break;
                case server::CoreType::FABRIC:
                    for (const auto &v : server::fetch_fabric_versions()) {
                        if (v.release_type == "release") {
                            arr.push_back({{"id", v.id}, {"label", v.id}});
                        }
                    }
                    break;
                default:
                    break;
                }
            } catch (const std::exception &e) {
                json_reply(res, {{"error", e.what()}}, 502);
                return;
            }
            json_reply(res, {{"versions", arr}});
        });

        // ---- 添加服务器（异步安装任务）----
        svr.Post("/api/servers", [this](const httplib::Request &req, httplib::Response &res) {
            auto body = nlohmann::json::parse(req.body, nullptr, false);
            if (body.is_discarded()) {
                json_reply(res, {{"error", "bad_request"}}, 400);
                return;
            }
            const std::string method = body.value("method", "");
            const std::string name = body.value("name", "");
            static const std::vector<std::string> kMethods = {"auto", "import", "installer", "jar"};
            if (std::find(kMethods.begin(), kMethods.end(), method) == kMethods.end()) {
                json_reply(res, {{"error", "未知方法 method"}}, 400);
                return;
            }
            if (!is_valid_server_name(name)) {
                json_reply(res, {{"error", "服务器名称无效（不能为空或包含 / \\ ..）"}}, 400);
                return;
            }
            if (method == "auto" && body.value("mc_version", "").empty()) {
                json_reply(res, {{"error", "缺少 mc_version"}}, 400);
                return;
            }
            if (method != "auto" && body.value("path", "").empty()) {
                json_reply(res, {{"error", "缺少 path"}}, 400);
                return;
            }

            auto task = std::make_shared<InstallTask>();
            size_t task_id;
            {
                std::lock_guard<std::mutex> lock(tasks_mutex);
                task_id = next_task_id++;
                tasks[task_id] = task;
            }
            task_log(task, "开始安装: " + name);
            std::thread([this, task, body]() mutable { run_install_task(task, std::move(body)); }).detach();
            json_reply(res, {{"ok", true}, {"task_id", task_id}});
        });

        svr.Get(R"(/api/tasks/(\d+))", [this](const httplib::Request &req, httplib::Response &res) {
            size_t id = static_cast<size_t>(std::stoul(req.matches[1]));
            std::shared_ptr<InstallTask> task;
            {
                std::lock_guard<std::mutex> lock(tasks_mutex);
                auto it = tasks.find(id);
                if (it != tasks.end()) task = it->second;
            }
            if (!task) {
                json_reply(res, {{"error", "task_not_found"}}, 404);
                return;
            }
            std::lock_guard<std::mutex> lock(task->mutex);
            json_reply(res, {
                {"status", task->status},
                {"log", task->log},
                {"error", task->error},
            });
        });

        // ---- 服务器配置（编辑 / 删除）----
        svr.Get(R"(/api/servers/(\d+)/config)", [this](const httplib::Request &req, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            size_t idx;
            if (!valid_index(req, res, idx)) return;
            json_reply(res, server_to_json(idx));
        });

        svr.Put(R"(/api/servers/(\d+)/config)", [this](const httplib::Request &req, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            size_t idx;
            if (!valid_index(req, res, idx)) return;
            auto body = nlohmann::json::parse(req.body, nullptr, false);
            if (body.is_discarded()) {
                json_reply(res, {{"error", "bad_request"}}, 400);
                return;
            }
            auto &sc = manager.config().servers[idx];
            if (body.contains("name")) {
                std::string name = body["name"];
                if (!is_valid_server_name(name)) {
                    json_reply(res, {{"error", "服务器名称无效"}}, 400);
                    return;
                }
                sc.name = name;
            }
            if (body.contains("java_path")) sc.java_path = body["java_path"].get<std::string>();
            if (body.contains("additional_jvm_args")) sc.additional_jvm_args = body["additional_jvm_args"].get<std::string>();
            if (body.contains("backup_dir")) sc.backup_dir = body["backup_dir"].get<std::string>();
            if (body.contains("world_dir")) sc.world_dir = body["world_dir"].get<std::string>();
            if (body.contains("min_memory_mb")) sc.min_memory_mb = std::max(0, body["min_memory_mb"].get<int>());
            if (body.contains("max_memory_mb")) sc.max_memory_mb = std::max(0, body["max_memory_mb"].get<int>());
            manager.save_config();
            json_reply(res, {{"ok", true}});
        });

        svr.Delete(R"(/api/servers/(\d+))", [this](const httplib::Request &req, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            size_t idx;
            if (!valid_index(req, res, idx)) return;
            manager.delete_server(idx);
            json_reply(res, {{"ok", true}});
        });

        // ---- 启停控制 ----
        svr.Post(R"(/api/servers/(\d+)/start)", [this](const httplib::Request &req, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            size_t idx;
            if (!valid_index(req, res, idx)) return;
            switch (manager.start_server(idx)) {
                case core::StartResult::STARTED:         json_reply(res, {{"ok", true}}); break;
                case core::StartResult::ALREADY_RUNNING: json_reply(res, {{"ok", true}, {"note", "already_running"}}); break;
                case core::StartResult::EULA_REQUIRED:   json_reply(res, {{"error", "eula_required"}}, 409); break;
                default:                                 json_reply(res, {{"error", "invalid_index"}}, 404); break;
            }
        });

        svr.Post(R"(/api/servers/(\d+)/eula)", [this](const httplib::Request &req, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            size_t idx;
            if (!valid_index(req, res, idx)) return;
            auto result = manager.agree_eula_and_start(idx);
            json_reply(res, {{"ok", result == core::StartResult::STARTED || result == core::StartResult::ALREADY_RUNNING}});
        });

        // stop / restart 可能阻塞数十秒（等待服务器退出），放到后台线程执行
        svr.Post(R"(/api/servers/(\d+)/stop)", [this](const httplib::Request &req, httplib::Response &res) {
            size_t idx;
            {
                std::lock_guard<std::mutex> lock(api_mutex);
                if (!valid_index(req, res, idx)) return;
            }
            std::thread([this, idx] {
                std::lock_guard<std::mutex> lock(api_mutex);
                manager.stop_server(idx);
            }).detach();
            json_reply(res, {{"ok", true}});
        });

        svr.Post(R"(/api/servers/(\d+)/restart)", [this](const httplib::Request &req, httplib::Response &res) {
            size_t idx;
            {
                std::lock_guard<std::mutex> lock(api_mutex);
                if (!valid_index(req, res, idx)) return;
            }
            std::thread([this, idx] {
                std::lock_guard<std::mutex> lock(api_mutex);
                manager.restart_server(idx);
            }).detach();
            json_reply(res, {{"ok", true}});
        });

        svr.Post(R"(/api/servers/(\d+)/command)", [this](const httplib::Request &req, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            size_t idx;
            if (!valid_index(req, res, idx)) return;
            auto body = nlohmann::json::parse(req.body, nullptr, false);
            if (body.is_discarded() || !body.value("command", "").size()) {
                json_reply(res, {{"error", "bad_request"}}, 400);
                return;
            }
            if (!manager.send_command(idx, body["command"].get<std::string>())) {
                json_reply(res, {{"error", "not_running"}}, 409);
                return;
            }
            json_reply(res, {{"ok", true}});
        });

        // ---- 控制台 ----
        svr.Get(R"(/api/servers/(\d+)/console)", [this](const httplib::Request &req, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            size_t idx;
            if (!valid_index(req, res, idx)) return;
            int tail = 300;
            if (req.has_param("tail")) {
                try { tail = std::stoi(req.get_param_value("tail")); } catch (...) {}
                tail = std::max(1, std::min(tail, server::ConsoleBuffer::MAX_LINES));
            }
            auto *buf = manager.console(idx);
            auto lines = buf ? buf->get_lines() : std::vector<std::string>{};
            if (static_cast<size_t>(tail) < lines.size()) {
                lines.erase(lines.begin(), lines.end() - tail);
            }
            json_reply(res, {{"lines", lines}});
        });

        svr.Get(R"(/api/servers/(\d+)/console/stream)", [this](const httplib::Request &req, httplib::Response &res) {
            size_t idx;
            {
                std::lock_guard<std::mutex> lock(api_mutex);
                if (!valid_index(req, res, idx)) return;
            }
            res.set_header("Cache-Control", "no-cache");
            res.set_header("X-Accel-Buffering", "no");
            res.set_chunked_content_provider("text/event-stream",
                [this, idx](size_t /*offset*/, httplib::DataSink &sink) {
                    auto queue = std::make_shared<LineQueue>();
                    size_t listener_id = manager.add_line_listener(idx,
                        [queue](const std::string &line) {
                            {
                                std::lock_guard<std::mutex> lock(queue->mutex);
                                // 客户端读取过慢时丢弃最旧的行，避免内存膨胀
                                if (queue->lines.size() >= 1000) queue->lines.pop_front();
                                queue->lines.push_back(line);
                            }
                            queue->cv.notify_one();
                        });

                    while (!shutting_down.load() && sink.is_writable()) {
                        std::string line;
                        {
                            std::unique_lock<std::mutex> lock(queue->mutex);
                            if (!queue->cv.wait_for(lock, std::chrono::seconds(3),
                                                    [&] { return !queue->lines.empty(); })) {
                                // SSE 保活注释，防止代理/浏览器断连
                                static const char kPing[] = ": ping\n\n";
                                if (!sink.write(kPing, sizeof(kPing) - 1)) break;
                                continue;
                            }
                            line = std::move(queue->lines.front());
                            queue->lines.pop_front();
                        }
                        std::string payload = "data: " + nlohmann::json({{"line", line}}).dump() + "\n\n";
                        if (!sink.write(payload.data(), payload.size())) break;
                    }

                    manager.remove_line_listener(idx, listener_id);
                    return false;
                });
        });

        // ---- server.properties ----
        svr.Get(R"(/api/servers/(\d+)/properties)", [this](const httplib::Request &req, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            size_t idx;
            if (!valid_index(req, res, idx)) return;
            const auto &sc = manager.config().servers[idx];
            std::string path = sc.working_dir.empty()
                ? "server.properties" : sc.working_dir + "/server.properties";
            utils::IniParser parser;
            bool loaded = parser.load(path);
            nlohmann::json entries = nlohmann::json::object();
            for (const auto &[k, v] : parser.entries()) {
                entries[k] = v;
            }
            json_reply(res, {
                {"exists", loaded},
                {"entries", entries},
                {"order", parser.ordered_keys()},
            });
        });

        svr.Put(R"(/api/servers/(\d+)/properties)", [this](const httplib::Request &req, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            size_t idx;
            if (!valid_index(req, res, idx)) return;
            auto body = nlohmann::json::parse(req.body, nullptr, false);
            if (body.is_discarded() || !body.contains("entries") || !body["entries"].is_object()) {
                json_reply(res, {{"error", "bad_request"}}, 400);
                return;
            }
            const auto &sc = manager.config().servers[idx];
            std::string path = sc.working_dir.empty()
                ? "server.properties" : sc.working_dir + "/server.properties";
            utils::IniParser parser;
            parser.load(path);
            for (const auto &[k, v] : body["entries"].items()) {
                if (v.is_string()) parser.set(k, v.get<std::string>());
            }
            if (!parser.save(path)) {
                json_reply(res, {{"error", "save_failed"}}, 500);
                return;
            }
            json_reply(res, {{"ok", true}});
        });

        // ---- 备份 ----
        svr.Get(R"(/api/servers/(\d+)/backups)", [this](const httplib::Request &req, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            size_t idx;
            if (!valid_index(req, res, idx)) return;
            json_reply(res, {{"backups", manager.list_backups(idx)}});
        });

        svr.Post(R"(/api/servers/(\d+)/backups)", [this](const httplib::Request &req, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            size_t idx;
            if (!valid_index(req, res, idx)) return;
            std::string message;
            if (!manager.create_backup(idx, message)) {
                json_reply(res, {{"error", message}}, 409);
                return;
            }
            json_reply(res, {{"ok", true}, {"name", message}});
        });

        svr.Post(R"(/api/servers/(\d+)/backups/([^/]+)/restore)", [this](const httplib::Request &req, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            size_t idx;
            if (!valid_index(req, res, idx)) return;
            std::string error;
            if (!manager.restore_backup(idx, req.matches[2], error)) {
                json_reply(res, {{"error", error}}, 409);
                return;
            }
            json_reply(res, {{"ok", true}});
        });

        svr.Delete(R"(/api/servers/(\d+)/backups/([^/]+))", [this](const httplib::Request &req, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            size_t idx;
            if (!valid_index(req, res, idx)) return;
            std::string error;
            if (!manager.delete_backup(idx, req.matches[2], error)) {
                json_reply(res, {{"error", error}}, 409);
                return;
            }
            json_reply(res, {{"ok", true}});
        });

        // ---- 全局设置 ----
        svr.Get("/api/settings", [this](const httplib::Request &, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            const auto &cfg = manager.config();
            json_reply(res, {
                {"servers_parent_folder", cfg.servers_parent_folder},
                {"effective_parent_folder", cfg.get_effective_servers_parent_folder()},
                {"global_java_path", cfg.global_java_path},
                {"effective_java_path", cfg.get_effective_java_path()},
                {"web_port", cfg.web_port},
            });
        });

        svr.Put("/api/settings", [this](const httplib::Request &req, httplib::Response &res) {
            std::lock_guard<std::mutex> lock(api_mutex);
            auto body = nlohmann::json::parse(req.body, nullptr, false);
            if (body.is_discarded()) {
                json_reply(res, {{"error", "bad_request"}}, 400);
                return;
            }
            auto &cfg = manager.config();
            if (body.contains("servers_parent_folder")) cfg.servers_parent_folder = body["servers_parent_folder"].get<std::string>();
            if (body.contains("global_java_path")) cfg.global_java_path = body["global_java_path"].get<std::string>();
            if (body.contains("web_port")) {
                int port = body["web_port"];
                if (port <= 0 || port > 65535) {
                    json_reply(res, {{"error", "端口无效"}}, 400);
                    return;
                }
                cfg.web_port = port;
            }
            manager.save_config();
            json_reply(res, {{"ok", true}, {"note", "web_port 需重启 web 服务后生效"}});
        });
    }
};

WebServer::WebServer(core::ServerManager &manager, WebOptions options)
    : impl_(new Impl(manager, std::move(options))) {
    impl_->svr.new_task_queue = [] { return new httplib::ThreadPool(4); };
    impl_->setup_routes();
}

WebServer::~WebServer() {
    delete impl_;
}

void WebServer::set_token(std::string token) {
    impl_->token = std::move(token);
}

const std::string &WebServer::token() const {
    return impl_->token;
}

int WebServer::bind() {
    return impl_->svr.bind_to_port(impl_->options.host, impl_->options.port)
               ? impl_->options.port : -1;
}

bool WebServer::run() {
    return impl_->svr.listen_after_bind();
}

void WebServer::stop() {
    impl_->shutting_down.store(true);
    impl_->svr.stop();
}

} // namespace mc_manager::web
