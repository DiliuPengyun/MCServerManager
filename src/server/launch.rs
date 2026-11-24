use std::{ffi::OsString, path::PathBuf};

use crate::{
    config::{AppConfig, ServerConfig},
    error::{AppError, Result},
};

#[derive(Clone, Debug)]
pub struct LaunchConfig {
    pub program: String,
    pub args: Vec<OsString>,
    pub working_dir: PathBuf,
}

pub fn build_launch_config(config: &AppConfig, server: &ServerConfig) -> Result<LaunchConfig> {
    let mut args = vec![
        OsString::from(format!("-Xms{}M", server.min_memory_mb)),
        OsString::from(format!("-Xmx{}M", server.max_memory_mb)),
    ];
    // 与现有配置语义一致：附加 JVM 参数按空格切分，不额外引入 shell 引号规则。
    args.extend(
        server
            .additional_jvm_args
            .split_whitespace()
            .map(OsString::from),
    );

    if server.core_type.uses_launch_args_file() {
        if let Some(path) = server.launch_args_path() {
            let mut argument = OsString::from("@");
            argument.push(path.as_os_str());
            args.push(argument);
        } else {
            args.extend(required_jar_arguments(server)?);
        }
    } else {
        args.extend(required_jar_arguments(server)?);
    }
    args.push(OsString::from("nogui"));

    Ok(LaunchConfig {
        program: config.effective_java_path(server).to_owned(),
        args,
        working_dir: server.working_dir.clone(),
    })
}

fn required_jar_arguments(server: &ServerConfig) -> Result<Vec<OsString>> {
    let jar = server.jar_path().ok_or_else(|| {
        AppError::invalid("jar_required", "该服务器尚未配置可启动 JAR 或启动参数文件")
    })?;
    Ok(vec![OsString::from("-jar"), jar.into_os_string()])
}

#[cfg(test)]
mod tests {
    use crate::config::CoreType;

    use super::*;

    #[test]
    fn launch_uses_arguments_without_shell_rendering() {
        let mut config = AppConfig::fresh("/tmp/servers".into());
        config.global_java_path = "/usr/lib/jvm/java".into();
        let mut server = ServerConfig::new("测试", "/srv/mc".into());
        server.server_jar_path = "server.jar".into();
        server.additional_jvm_args = "-Dfoo=a b  -Dempty=".into();
        let launch = build_launch_config(&config, &server).unwrap();
        assert_eq!(launch.program, "/usr/lib/jvm/java");
        assert!(launch.args.contains(&OsString::from("-Dfoo=a")));
        assert!(launch.args.contains(&OsString::from("b")));
        assert!(launch.args.contains(&OsString::from("/srv/mc/server.jar")));
        assert_eq!(launch.args.last(), Some(&OsString::from("nogui")));
    }

    #[test]
    fn forge_uses_launch_args_instead_of_installer_jar() {
        let config = AppConfig::fresh("/tmp/servers".into());
        let mut server = ServerConfig::new("Forge", "/srv/forge".into());
        server.core_type = CoreType::Forge;
        server.launch_args_file = "libraries/net/minecraftforge/forge/unix_args.txt".into();
        let launch = build_launch_config(&config, &server).unwrap();
        assert!(launch.args.contains(&OsString::from(
            "@/srv/forge/libraries/net/minecraftforge/forge/unix_args.txt"
        )));
        assert!(!launch.args.contains(&OsString::from("-jar")));
    }
}
