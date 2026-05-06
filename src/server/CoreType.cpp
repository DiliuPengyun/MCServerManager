#include "server/CoreType.h"

namespace mc_manager::server {

const CoreTypeInfo &get_core_type_info(CoreType type) {
    for (const auto &info : CORE_TYPES) {
        if (info.type == type) return info;
    }
    return CORE_TYPES[6]; // CUSTOM
}

CoreType core_type_from_id(std::string_view id) {
    for (const auto &info : CORE_TYPES) {
        if (info.id == id) return info.type;
    }
    return CoreType::CUSTOM;
}

} // namespace mc_manager::server
