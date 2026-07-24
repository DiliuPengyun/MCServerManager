add_rules("mode.debug", "mode.release")

-- 锁定 v6.1.9：ftxui v7 将 ComponentBase::children_ 改为私有，代码暂不兼容
add_requires("ftxui v6.1.9")
add_requires("nlohmann_json")

set_version("0.2")

target("MCServerManager")
    set_kind("binary")
    add_files("src/**.cpp")
    add_includedirs("include", "third_party")
    add_packages("ftxui", "nlohmann_json")
    if is_plat("linux") then
        add_syslinks("pthread")
    end
    set_languages("c++20")
    if is_mode("debug") then
        set_symbols("debug")
        set_optimize("none")
    end
