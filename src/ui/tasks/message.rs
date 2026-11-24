use std::path::PathBuf;

use tokio::sync::mpsc;

use crate::{
    config::{ServerConfig, ServerId},
    core::StartResult,
    server::files::FileEntry,
};

use super::wizard::VersionChoice;
use super::{JobId, TaskResult};

pub(in crate::ui) type MessageSender = mpsc::UnboundedSender<Message>;

#[derive(Debug)]
pub(in crate::ui) enum Message {
    Progress {
        job: JobId,
        line: String,
    },
    Versions {
        job: JobId,
        generation: u64,
        result: TaskResult<Vec<VersionChoice>>,
    },
    WizardProgress {
        job: JobId,
        generation: u64,
        line: String,
    },
    WizardFinished {
        job: JobId,
        generation: u64,
        result: Box<TaskResult<ServerConfig>>,
    },
    Lifecycle {
        job: JobId,
        id: ServerId,
        restart: bool,
        result: TaskResult<StartResult>,
    },
    Simple {
        job: JobId,
        result: TaskResult<(String, super::Refresh)>,
    },
    Properties {
        job: JobId,
        id: ServerId,
        result: TaskResult<(bool, Vec<(String, String)>)>,
    },
    PropertiesSaved {
        job: JobId,
        id: ServerId,
        submitted: Vec<(String, String)>,
        result: TaskResult<Vec<(String, String)>>,
    },
    FileList {
        job: JobId,
        id: ServerId,
        path: PathBuf,
        result: TaskResult<Vec<FileEntry>>,
    },
    FileRead {
        job: JobId,
        id: ServerId,
        path: PathBuf,
        result: TaskResult<String>,
    },
    FileSaved {
        job: JobId,
        id: ServerId,
        path: PathBuf,
        content: String,
        result: TaskResult<()>,
    },
    FileChanged {
        job: JobId,
        id: ServerId,
        path: PathBuf,
        result: TaskResult<String>,
    },
    Backups {
        job: JobId,
        id: ServerId,
        result: TaskResult<Vec<String>>,
    },
    BackupCreated {
        job: JobId,
        id: ServerId,
        result: TaskResult<(String, Vec<String>)>,
    },
    BackupChanged {
        job: JobId,
        id: ServerId,
        result: TaskResult<(String, Vec<String>)>,
    },
}

impl Message {
    pub(in crate::ui) fn job_id(&self) -> JobId {
        match self {
            Self::Progress { job, .. }
            | Self::Versions { job, .. }
            | Self::WizardProgress { job, .. }
            | Self::WizardFinished { job, .. }
            | Self::Lifecycle { job, .. }
            | Self::Simple { job, .. }
            | Self::Properties { job, .. }
            | Self::PropertiesSaved { job, .. }
            | Self::FileList { job, .. }
            | Self::FileRead { job, .. }
            | Self::FileSaved { job, .. }
            | Self::FileChanged { job, .. }
            | Self::Backups { job, .. }
            | Self::BackupCreated { job, .. }
            | Self::BackupChanged { job, .. } => *job,
        }
    }

    pub(in crate::ui) fn is_terminal(&self) -> bool {
        !matches!(self, Self::Progress { .. } | Self::WizardProgress { .. })
    }
}
