use std::path::PathBuf;

use crate::config::{CoreType, ServerConfig, ServerId};

use super::JobId;
use super::wizard::WizardRequest;

#[derive(Clone, Debug)]
pub(in crate::ui) struct Command {
    pub id: JobId,
    pub kind: CommandKind,
}

#[derive(Clone, Debug)]
pub(in crate::ui) enum CommandKind {
    LoadVersions {
        generation: u64,
        core_type: CoreType,
        mc_version: String,
    },
    WizardInstall(WizardRequest),
    Lifecycle {
        id: ServerId,
        action: LifecycleAction,
    },
    Stop(ServerId),
    Simple(SimpleCommand),
    PropertiesLoad(ServerId),
    PropertiesSave {
        id: ServerId,
        entries: Vec<(String, String)>,
    },
    FileList {
        id: ServerId,
        path: PathBuf,
    },
    FileRead {
        id: ServerId,
        path: PathBuf,
    },
    FileWrite {
        id: ServerId,
        path: PathBuf,
        content: String,
    },
    FileCreate {
        id: ServerId,
        path: PathBuf,
        is_dir: bool,
    },
    FileRename {
        id: ServerId,
        path: PathBuf,
        new_name: String,
    },
    FileDelete {
        id: ServerId,
        path: PathBuf,
    },
    BackupsList(ServerId),
    BackupCreate(ServerId),
    BackupRestore {
        id: ServerId,
        name: String,
    },
    BackupDelete {
        id: ServerId,
        name: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum LifecycleAction {
    Start { restart: bool, allow_no_rcon: bool },
    AgreeEula { restart: bool },
    EnableRcon { restart: bool },
    RunInstaller { restart: bool },
}

impl LifecycleAction {
    pub(super) fn restart(&self) -> bool {
        match self {
            Self::Start { restart, .. }
            | Self::AgreeEula { restart }
            | Self::EnableRcon { restart }
            | Self::RunInstaller { restart } => *restart,
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::ui) enum SimpleCommand {
    SaveGlobal {
        default_dir: PathBuf,
        java_path: String,
    },
    SaveServer {
        id: ServerId,
        config: Box<ServerConfig>,
    },
    MoveServer {
        id: ServerId,
        destination: PathBuf,
        relocate: bool,
    },
    RemoveServer {
        id: ServerId,
        delete_files: bool,
    },
    SendCommand {
        id: ServerId,
        command: String,
    },
    KickPlayer {
        id: ServerId,
        player: String,
    },
    BanPlayer {
        id: ServerId,
        player: String,
    },
    ClearConsole(ServerId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum Refresh {
    None,
    ServerData(ServerId),
    ServerRemoved(ServerId),
}
