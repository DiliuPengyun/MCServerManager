#pragma once

#include <cstdint>
#include <filesystem>
#include <optional>
#include <string>
#include <vector>

namespace mc_manager::server {

// 目录条目（文件或子目录）
struct FileEntry {
    std::string name;
    bool is_dir = false;
    std::uintmax_t size = 0;  // 目录为 0
    std::string mod_time;     // "YYYY-MM-DD HH:MM" 格式
};

// 服务器工作目录的文件浏览器：所有操作限制在根目录之内，
// 供 TUI 文件面板与 Web 文件接口共用。
class FileBrowser {
public:
    explicit FileBrowser(std::string root_dir);

    // 根目录存在且是目录
    bool is_valid() const;
    const std::string &root() const { return root_dir_; }

    // rel 为相对路径，"" 或 "." 表示根目录；err 输出中文错误信息
    bool list(const std::string &rel, std::vector<FileEntry> &out, std::string &err) const;
    // 读取文本文件；超过 max_bytes 报「文件过大」，含 NUL 字节报「二进制文件」
    bool read_text(const std::string &rel, std::string &content, std::string &err,
                   std::uintmax_t max_bytes = 1024 * 1024) const;
    // 覆盖写入文本文件（文件必须已存在）
    bool write_text(const std::string &rel, const std::string &content, std::string &err) const;
    // 新建空文件（父目录必须已存在，目标不得已存在）
    bool create_file(const std::string &rel, std::string &err) const;
    // 上传文件：以二进制写入（允许二进制内容）；目标已存在时须 overwrite 为真，
    // 目录不能被覆盖
    bool upload_file(const std::string &rel, const std::string &content, bool overwrite,
                     std::string &err) const;
    // 新建目录（父目录必须已存在，目标不得已存在）
    bool create_dir(const std::string &rel, std::string &err) const;
    // 重命名（new_name 为纯文件名，不得含路径分隔符）
    bool rename(const std::string &rel, const std::string &new_name, std::string &err) const;
    // 删除文件或目录（目录递归删除；不得删除根目录本身）
    bool remove(const std::string &rel, std::string &err) const;

private:
    // 核心安全校验：解析 root_/rel（weakly_canonical）后必须仍位于根目录内，
    // 否则返回 nullopt（拒绝 ".." 越界与符号链接逃逸）
    std::optional<std::filesystem::path> resolve(const std::string &rel) const;

    std::string root_dir_;
    std::filesystem::path root_;
};

} // namespace mc_manager::server
