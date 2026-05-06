add_rules("mode.debug", "mode.release")

add_requires("ftxui")
add_requires("nlohmann_json")

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
