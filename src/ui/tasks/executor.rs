use std::{collections::HashMap, sync::Arc};

use tokio::{runtime::Runtime, task::JoinHandle};

use crate::core::ServerManager;

use super::{
    Command, CommandKind, JobId, Message, MessageSender,
    operations::{self, backups, files, install, lifecycle, properties, simple, versions},
};

#[derive(Default)]
pub(in crate::ui) struct TaskBook {
    handles: HashMap<JobId, JoinHandle<()>>,
}

impl TaskBook {
    pub(in crate::ui) fn dispatch(
        &mut self,
        runtime: &Runtime,
        manager: Arc<ServerManager>,
        sender: MessageSender,
        command: Command,
    ) {
        let id = command.id;
        let handle = runtime.spawn(execute(manager, sender, command));
        self.handles.insert(id, handle);
    }

    pub(in crate::ui) fn reap(&mut self) {
        self.handles.retain(|_, handle| !handle.is_finished());
    }

    pub(in crate::ui) fn abort_all(&mut self) {
        for handle in self.handles.values() {
            handle.abort();
        }
        self.handles.clear();
    }
}

/// 瘦分派器：只选择领域操作并回发对应消息，具体 I/O 在 operations 中完成。
async fn execute(manager: Arc<ServerManager>, sender: MessageSender, command: Command) {
    let job = command.id;
    match command.kind {
        CommandKind::LoadVersions {
            generation,
            core_type,
            mc_version,
        } => {
            let result = versions::load(core_type, &mc_version).await;
            let _ = sender.send(Message::Versions {
                job,
                generation,
                result,
            });
        }
        CommandKind::WizardInstall(request) => {
            let generation = request.generation;
            let result = install::run_wizard(manager, request, job, sender.clone()).await;
            let _ = sender.send(Message::WizardFinished {
                job,
                generation,
                result: Box::new(result),
            });
        }
        CommandKind::Lifecycle { id, action } => {
            let restart = action.restart();
            let result = lifecycle::run(manager, id, action, job, sender.clone()).await;
            let _ = sender.send(Message::Lifecycle {
                job,
                id,
                restart,
                result,
            });
        }
        CommandKind::Stop(id) => {
            let result = lifecycle::stop(&manager, id).await;
            let _ = sender.send(Message::Simple { job, result });
        }
        CommandKind::Simple(command) => {
            let result = simple::run(manager, command).await;
            let _ = sender.send(Message::Simple { job, result });
        }
        CommandKind::PropertiesLoad(id) => {
            let result = properties::load(&manager, id).await;
            let _ = sender.send(Message::Properties { job, id, result });
        }
        CommandKind::PropertiesSave { id, entries } => {
            let submitted = entries.clone();
            let result = properties::save(&manager, id, entries).await;
            let _ = sender.send(Message::PropertiesSaved {
                job,
                id,
                submitted,
                result,
            });
        }
        CommandKind::FileList { id, path } => {
            let result = files::list(&manager, id, path.clone()).await;
            let _ = sender.send(Message::FileList {
                job,
                id,
                path,
                result,
            });
        }
        CommandKind::FileRead { id, path } => {
            let result = files::read(&manager, id, path.clone()).await;
            let _ = sender.send(Message::FileRead {
                job,
                id,
                path,
                result,
            });
        }
        CommandKind::FileWrite { id, path, content } => {
            let submitted = content.clone();
            let result = files::write(&manager, id, path.clone(), content).await;
            let _ = sender.send(Message::FileSaved {
                job,
                id,
                path,
                content: submitted,
                result,
            });
        }
        CommandKind::FileCreate { id, path, is_dir } => {
            let parent = operations::parent_path(&path);
            let result = files::create(&manager, id, path, is_dir).await;
            let _ = sender.send(Message::FileChanged {
                job,
                id,
                path: parent,
                result,
            });
        }
        CommandKind::FileRename { id, path, new_name } => {
            let parent = operations::parent_path(&path);
            let result = files::rename(&manager, id, path, new_name).await;
            let _ = sender.send(Message::FileChanged {
                job,
                id,
                path: parent,
                result,
            });
        }
        CommandKind::FileDelete { id, path } => {
            let parent = operations::parent_path(&path);
            let result = files::delete(&manager, id, path).await;
            let _ = sender.send(Message::FileChanged {
                job,
                id,
                path: parent,
                result,
            });
        }
        CommandKind::BackupsList(id) => {
            let result = backups::list(&manager, id).await;
            let _ = sender.send(Message::Backups { job, id, result });
        }
        CommandKind::BackupCreate(id) => {
            let result = backups::create(&manager, id).await;
            let _ = sender.send(Message::BackupCreated { job, id, result });
        }
        CommandKind::BackupRestore { id, name } => {
            let result = backups::change(manager.clone(), id, name, true).await;
            let _ = sender.send(Message::BackupChanged { job, id, result });
        }
        CommandKind::BackupDelete { id, name } => {
            let result = backups::change(manager, id, name, false).await;
            let _ = sender.send(Message::BackupChanged { job, id, result });
        }
    }
}
