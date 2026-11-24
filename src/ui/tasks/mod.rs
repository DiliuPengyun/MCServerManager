mod command;
mod executor;
mod message;
mod operations;
mod wizard;

pub(super) use command::{Command, CommandKind, LifecycleAction, Refresh, SimpleCommand};
pub(super) use executor::TaskBook;
pub(super) use message::{Message, MessageSender};
pub(super) use wizard::{VersionChoice, WizardMethod, WizardRequest};

pub(super) type JobId = u64;
pub(super) type TaskResult<T> = std::result::Result<T, String>;
