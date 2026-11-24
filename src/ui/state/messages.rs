use super::super::tasks::{JobId, Message, Refresh};
use super::{App, ConfirmAction, ConfirmState, FileEditorState, Modal, RconState, WizardStep};
use crate::core::StartResult;
use crate::ui::input::TextEditor;

/// 终态消息先清理任务视图，再按消息类型穷尽分派。
pub(super) fn apply(app: &mut App, message: Message) {
    let job = message.job_id();
    if message.is_terminal() {
        app.jobs.remove(&job);
        if app.active_progress_job == Some(job) {
            app.active_progress_job = None;
        }
    }
    match message {
        Message::Progress { job, line } => push_job_log(app, job, line),
        Message::WizardProgress {
            generation, line, ..
        } => {
            if let Some(wizard) = &mut app.wizard
                && wizard.generation == generation
            {
                wizard.progress.push_back(line);
                while wizard.progress.len() > 300 {
                    wizard.progress.pop_front();
                }
            }
        }
        Message::Versions {
            generation, result, ..
        } => {
            if let Some(wizard) = &mut app.wizard
                && wizard.generation == generation
            {
                wizard.versions_loading = false;
                match result {
                    Ok(versions) => {
                        wizard.versions = versions;
                        wizard.version_selected = 0;
                    }
                    Err(error) => wizard.versions_error = Some(error),
                }
            }
        }
        Message::WizardFinished {
            generation, result, ..
        } => {
            // 结果可能晚于向导关闭或新一代向导到达，只在代次匹配时取添加方式。
            let action = app
                .wizard
                .as_ref()
                .filter(|wizard| wizard.generation == generation)
                .map_or("添加", |wizard| wizard.method().action_name());
            match *result {
                Ok(server) => {
                    let name = server.name.clone();
                    app.notice(format!("服务器「{name}」已{action}并注册"), false);
                    if let Some(wizard) = &mut app.wizard
                        && wizard.generation == generation
                    {
                        wizard.step = WizardStep::Done;
                        wizard.created_name = Some(name);
                        wizard.error = None;
                    }
                }
                Err(error) => {
                    app.notice(format!("{action}失败：{error}"), true);
                    if let Some(wizard) = &mut app.wizard
                        && wizard.generation == generation
                    {
                        wizard.step = WizardStep::Done;
                        wizard.created_name = None;
                        wizard.error = Some(error);
                    }
                }
            }
        }
        Message::Lifecycle {
            id,
            restart,
            result,
            ..
        } => match result {
            Ok(StartResult::Started) => app.notice("服务器启动命令已执行", false),
            Ok(StartResult::AlreadyRunning) => app.notice("服务器已经在运行", false),
            Ok(StartResult::InstallerRequired) => {
                app.modal = Some(Modal::Confirm(ConfirmState {
                    title: "服务器尚未安装".to_owned(),
                    lines: vec![
                        "检测到 Forge/NeoForge 安装器，但尚未生成可启动核心。".to_owned(),
                        "继续将运行安装器并写入服务器目录，完成后重新执行原启动动作。".to_owned(),
                    ],
                    confirm_label: "运行安装器".to_owned(),
                    action: ConfirmAction::RunInstaller { id, restart },
                    confirm_selected: false,
                }));
            }
            Ok(StartResult::EulaRequired) => {
                app.modal = Some(Modal::Confirm(ConfirmState {
                    title: "需要明确同意 Minecraft EULA".to_owned(),
                    lines: vec![
                        "请阅读 https://aka.ms/MinecraftEULA".to_owned(),
                        "继续会在服务器目录的 eula.txt 写入 eula=true，然后继续启动流程。"
                            .to_owned(),
                        "如不同意，请保持取消；管理器不会替你修改文件。".to_owned(),
                    ],
                    confirm_label: "同意 EULA 并继续".to_owned(),
                    action: ConfirmAction::AgreeEula { id, restart },
                    confirm_selected: false,
                }));
            }
            Ok(StartResult::RconRequired) => {
                app.modal = Some(Modal::Rcon(RconState {
                    id,
                    restart,
                    selected: 0,
                }));
            }
            Err(error) => app.notice(format!("生命周期操作失败：{error}"), true),
        },
        Message::Simple { result, .. } => match result {
            Ok((text, refresh)) => {
                app.notice(text, false);
                apply_refresh(app, refresh);
            }
            Err(error) => app.notice(error, true),
        },
        Message::Properties { id, result, .. } => {
            let properties = app.properties.entry(id).or_default();
            properties.loading = false;
            properties.loaded = true;
            match result {
                Ok((exists, entries)) => {
                    properties.exists = exists;
                    properties.entries = entries;
                    properties.selected = 0;
                    properties.dirty = false;
                }
                Err(error) => app.notice(format!("属性加载失败：{error}"), true),
            }
        }
        Message::PropertiesSaved {
            id,
            submitted,
            result,
            ..
        } => match result {
            Ok(entries) => {
                let properties = app.properties.entry(id).or_default();
                properties.exists = true;
                properties.loaded = true;
                if properties.entries == submitted {
                    properties.entries = entries;
                    properties.dirty = false;
                    app.notice("server.properties 已保存", false);
                } else {
                    // 旧保存可能在新保存之后完成；此时磁盘是旧快照，当前内容仍未保存。
                    properties.dirty = true;
                    app.notice("本次属性已保存；当前属性仍有未保存修改", false);
                }
            }
            Err(error) => app.notice(format!("属性保存失败：{error}"), true),
        },
        Message::FileList {
            id, path, result, ..
        } => {
            let files = app.files.entry(id).or_default();
            files.loading = false;
            if files.current_dir == path {
                files.loaded = true;
                match result {
                    Ok(entries) => {
                        files.entries = entries;
                        files.selected = files.selected.min(files.entries.len().saturating_sub(1));
                    }
                    Err(error) => app.notice(format!("目录读取失败：{error}"), true),
                }
            }
        }
        Message::FileRead {
            id, path, result, ..
        } => match result {
            Ok(content) => {
                let files = app.files.entry(id).or_default();
                files.editor = Some(FileEditorState {
                    path,
                    original: content.clone(),
                    editor: TextEditor::new(content),
                });
            }
            Err(error) => app.notice(format!("文本读取失败：{error}"), true),
        },
        Message::FileSaved {
            id,
            path,
            content,
            result,
            ..
        } => match result {
            Ok(()) => {
                if let Some(editor) = app
                    .files
                    .get_mut(&id)
                    .and_then(|files| files.editor.as_mut())
                    && editor.path == path
                {
                    // 保存期间可以继续输入；不能把保存完成后输入的内容也标成已写盘。
                    editor.original = content;
                }
                app.notice(format!("已保存 {}", path.display()), false);
            }
            Err(error) => app.notice(format!("文件保存失败：{error}"), true),
        },
        Message::FileChanged {
            id, path, result, ..
        } => match result {
            Ok(text) => {
                app.notice(text, false);
                if let Some(files) = app.files.get_mut(&id)
                    && files.current_dir == path
                {
                    files.loaded = false;
                    files.loading = false;
                }
            }
            Err(error) => app.notice(format!("文件操作失败：{error}"), true),
        },
        Message::Backups { id, result, .. } => {
            let backups = app.backups.entry(id).or_default();
            backups.loading = false;
            backups.loaded = true;
            match result {
                Ok(items) => {
                    backups.items = items;
                    backups.selected = backups.selected.min(backups.items.len().saturating_sub(1));
                }
                Err(error) => app.notice(format!("备份列表读取失败：{error}"), true),
            }
        }
        Message::BackupCreated { id, result, .. } => match result {
            Ok((name, items)) => {
                let backups = app.backups.entry(id).or_default();
                backups.items = items;
                backups.loaded = true;
                app.notice(format!("备份已创建：{name}"), false);
            }
            Err(error) => app.notice(format!("备份创建失败：{error}"), true),
        },
        Message::BackupChanged { id, result, .. } => match result {
            Ok((text, items)) => {
                let backups = app.backups.entry(id).or_default();
                backups.items = items;
                backups.selected = backups.selected.min(backups.items.len().saturating_sub(1));
                app.notice(text, false);
            }
            Err(error) => app.notice(format!("备份操作失败：{error}"), true),
        },
    }
}

fn apply_refresh(app: &mut App, refresh: Refresh) {
    match refresh {
        Refresh::None => {}
        Refresh::ServerData(id) => {
            app.properties.remove(&id);
            app.backups.remove(&id);
            app.files.remove(&id);
            app.server_form.dirty = false;
            app.global_form.dirty = false;
        }
        Refresh::ServerRemoved(id) => {
            app.consoles.remove(&id);
            app.players.remove(&id);
            app.properties.remove(&id);
            app.backups.remove(&id);
            app.files.remove(&id);
            app.server_form.id = None;
        }
    }
}

fn push_job_log(app: &mut App, job: JobId, line: String) {
    if let Some(view) = app.jobs.get_mut(&job) {
        view.log.push_back(line);
        while view.log.len() > 300 {
            view.log.pop_front();
        }
    }
}
