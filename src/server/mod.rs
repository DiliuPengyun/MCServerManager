pub mod backup;
pub mod console;
pub mod console_buffer;
pub mod detect;
pub mod eula;
pub mod files;
pub(crate) mod fs_copy;
pub mod install;
pub mod launch;
pub mod process;
pub mod properties;
pub mod rcon;
pub mod tmux;

pub use crate::config::CoreType;
