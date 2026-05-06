#pragma once

#include <string>
#include <string_view>

namespace mc_manager::server {

enum class CoreType {
    VANILLA,
    PAPER,
    SPIGOT,
    FORGE,
    NEOFORGE,
    FABRIC,
    CUSTOM
};

struct CoreTypeInfo {
    CoreType type;
    std::string_view id;
    std::string_view display_name;
    bool uses_installer;
    bool uses_launch_args_file;
};

inline constexpr CoreTypeInfo CORE_TYPES[] = {
    {CoreType::VANILLA,  "vanilla",  "Vanilla",  false, false},
    {CoreType::PAPER,    "paper",    "Paper",    false, false},
    {CoreType::SPIGOT,   "spigot",   "Spigot",   false, false},
    {CoreType::FORGE,    "forge",    "Forge",    true,  true },
    {CoreType::NEOFORGE, "neoforge", "NeoForge", true,  true },
    {CoreType::FABRIC,   "fabric",   "Fabric",   false, false},
    {CoreType::CUSTOM,   "custom",   "自定义",    false, false},
};

const CoreTypeInfo &get_core_type_info(CoreType type);
CoreType core_type_from_id(std::string_view id);

} // namespace mc_manager::server
