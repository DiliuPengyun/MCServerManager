#include "server/FileBrowser.h"

#include "utils/FileUtils.h"

#include <algorithm>
#include <chrono>
#include <fstream>
#include <system_error>

namespace mc_manager::server {

namespace {

// 文件修改时间格式化为 "YYYY-MM-DD HH:MM"（本地时间）
std::string format_mod_time(const std::filesystem::path &path) {
    std::error_code ec;
    auto ftime = std::filesystem::last_write_time(path, ec);
    if (ec) {
        return "";
    }
    auto sctp = std::chrono::time_point_cast<std::chrono::system_clock::duration>(
        ftime - std::filesystem::file_time_type::clock::now() + std::chrono::system_clock::now());
    std::time_t tt = std::chrono::system_clock::to_time_t(sctp);
    std::tm tm{};
    localtime_r(&tt, &tm);
    char buf[20];
    std::strftime(buf, sizeof(buf), "%Y-%m-%d %H:%M", &tm);
    return buf;
}

// 纯文件名校验：非空、不为 . / ..、不含路径分隔符
bool is_valid_entry_name(const std::string &name) {
    return !name.empty() && name != "." && name != ".." &&
           name.find('/') == std::string::npos && name.find('\\') == std::string::npos;
}

} // namespace

FileBrowser::FileBrowser(std::string root_dir) : root_dir_(std::move(root_dir)), root_(root_dir_) {}

bool FileBrowser::is_valid() const {
    std::error_code ec;
    return !root_.empty() && std::filesystem::is_directory(root_, ec);
}

std::optional<std::filesystem::path> FileBrowser::resolve(const std::string &rel) const {
    if (!is_valid()) {
        return std::nullopt;
    }
    std::error_code ec;
    auto canonical_root = std::filesystem::weakly_canonical(root_, ec);
    if (ec) {
        return std::nullopt;
    }
    std::filesystem::path target = root_;
    if (!rel.empty() && rel != ".") {
        target /= std::filesystem::path(rel);
    }
    // weakly_canonical 解析已存在的部分（含符号链接），未存在的尾部按规则化处理
    auto canonical_target = std::filesystem::weakly_canonical(target, ec);
    if (ec) {
        return std::nullopt;
    }
    // 目标必须等于根或位于根之下
    if (canonical_target != canonical_root) {
        auto root_it = canonical_root.begin();
        auto target_it = canonical_target.begin();
        for (; root_it != canonical_root.end(); ++root_it, ++target_it) {
            if (target_it == canonical_target.end() || *target_it != *root_it) {
                return std::nullopt;
            }
        }
    }
    return canonical_target;
}

bool FileBrowser::list(const std::string &rel, std::vector<FileEntry> &out, std::string &err) const {
    auto dir = resolve(rel);
    if (!dir) {
        err = "路径无效或越出服务器目录";
        return false;
    }
    std::error_code ec;
    if (!std::filesystem::is_directory(*dir, ec)) {
        err = "目标不是目录";
        return false;
    }
    out.clear();
    for (const auto &item : std::filesystem::directory_iterator(*dir, ec)) {
        FileEntry entry;
        entry.name = item.path().filename().string();
        std::error_code type_ec;
        entry.is_dir = item.is_directory(type_ec);
        if (!entry.is_dir) {
            std::error_code size_ec;
            entry.size = item.file_size(size_ec);
            if (size_ec) {
                entry.size = 0;
            }
        }
        entry.mod_time = format_mod_time(item.path());
        out.push_back(std::move(entry));
    }
    if (ec) {
        err = "读取目录失败";
        return false;
    }
    // 目录在前，各自按名称排序
    std::sort(out.begin(), out.end(), [](const FileEntry &a, const FileEntry &b) {
        if (a.is_dir != b.is_dir) {
            return a.is_dir;
        }
        return a.name < b.name;
    });
    return true;
}

bool FileBrowser::read_text(const std::string &rel, std::string &content, std::string &err,
                            std::uintmax_t max_bytes) const {
    auto file = resolve(rel);
    if (!file) {
        err = "路径无效或越出服务器目录";
        return false;
    }
    std::error_code ec;
    if (!std::filesystem::is_regular_file(*file, ec)) {
        err = "目标不是文件";
        return false;
    }
    auto size = std::filesystem::file_size(*file, ec);
    if (!ec && size > max_bytes) {
        err = "文件过大（超过 1 MiB），不支持在线编辑";
        return false;
    }
    auto data = utils::read_file_string(file->string());
    if (!data) {
        err = "读取文件失败";
        return false;
    }
    if (data->find('\0') != std::string::npos) {
        err = "二进制文件不支持编辑";
        return false;
    }
    content = std::move(*data);
    return true;
}

bool FileBrowser::write_text(const std::string &rel, const std::string &content,
                             std::string &err) const {
    auto file = resolve(rel);
    if (!file) {
        err = "路径无效或越出服务器目录";
        return false;
    }
    std::error_code ec;
    if (!std::filesystem::is_regular_file(*file, ec)) {
        err = "目标不是已存在的文件";
        return false;
    }
    if (!utils::write_file_string(file->string(), content)) {
        err = "写入文件失败";
        return false;
    }
    return true;
}

bool FileBrowser::create_file(const std::string &rel, std::string &err) const {
    auto file = resolve(rel);
    if (!file) {
        err = "路径无效或越出服务器目录";
        return false;
    }
    if (!is_valid_entry_name(file->filename().string())) {
        err = "文件名无效";
        return false;
    }
    std::error_code ec;
    if (std::filesystem::exists(*file, ec)) {
        err = "同名文件或目录已存在";
        return false;
    }
    if (!std::filesystem::is_directory(file->parent_path(), ec)) {
        err = "父目录不存在";
        return false;
    }
    std::ofstream ofs(*file);
    if (!ofs.is_open()) {
        err = "创建文件失败";
        return false;
    }
    return true;
}

bool FileBrowser::create_dir(const std::string &rel, std::string &err) const {
    auto dir = resolve(rel);
    if (!dir) {
        err = "路径无效或越出服务器目录";
        return false;
    }
    if (!is_valid_entry_name(dir->filename().string())) {
        err = "目录名无效";
        return false;
    }
    std::error_code ec;
    if (std::filesystem::exists(*dir, ec)) {
        err = "同名文件或目录已存在";
        return false;
    }
    if (!std::filesystem::create_directory(*dir, ec)) {
        err = "创建目录失败";
        return false;
    }
    return true;
}

bool FileBrowser::rename(const std::string &rel, const std::string &new_name,
                         std::string &err) const {
    if (!is_valid_entry_name(new_name)) {
        err = "名称无效";
        return false;
    }
    auto src = resolve(rel);
    if (!src) {
        err = "路径无效或越出服务器目录";
        return false;
    }
    auto root_path = resolve(".");
    if (!root_path || *src == *root_path) {
        err = "不能重命名根目录";
        return false;
    }
    std::error_code ec;
    if (!std::filesystem::exists(*src, ec)) {
        err = "目标不存在";
        return false;
    }
    auto dst = src->parent_path() / new_name;
    if (std::filesystem::exists(dst, ec)) {
        err = "同名文件或目录已存在";
        return false;
    }
    std::filesystem::rename(*src, dst, ec);
    if (ec) {
        err = "重命名失败";
        return false;
    }
    return true;
}

bool FileBrowser::remove(const std::string &rel, std::string &err) const {
    auto target = resolve(rel);
    if (!target) {
        err = "路径无效或越出服务器目录";
        return false;
    }
    auto root_path = resolve(".");
    if (!root_path || *target == *root_path) {
        err = "不能删除服务器根目录";
        return false;
    }
    std::error_code ec;
    if (!std::filesystem::exists(*target, ec)) {
        err = "目标不存在";
        return false;
    }
    std::filesystem::remove_all(*target, ec);
    if (ec) {
        err = "删除失败";
        return false;
    }
    return true;
}

} // namespace mc_manager::server
