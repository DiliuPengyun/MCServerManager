use crossterm::event::{KeyEventKind, KeyEventState};
use tempfile::TempDir;

use super::*;
use crate::ui::tasks::SimpleCommand;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn ctrl(character: char) -> KeyEvent {
    KeyEvent {
        modifiers: KeyModifiers::CONTROL,
        ..key(KeyCode::Char(character))
    }
}

fn app() -> (TempDir, App, ServerId) {
    let temp = TempDir::new().unwrap();
    let directory = temp.path().join("外置目录/中文服务器");
    let server = ServerConfig::new("中文服务器", directory.clone());
    let id = server.id;
    let snapshot = ServerSnapshot {
        index: 0,
        id,
        name: server.name.clone(),
        working_dir: directory,
        state: ServerState::Stopped,
        rcon_available: false,
        players: Vec::new(),
    };
    let config = Arc::new(AppConfig::fresh(temp.path().join("默认目录")));
    let mut config_value = (*config).clone();
    config_value.servers.push(server);
    config_value.active_server_id = Some(id);
    let app = App::new(Arc::new(config_value), vec![snapshot]);
    (temp, app, id)
}

#[test]
fn sidebar_focus_and_tab_transitions_are_pure() {
    let (_temp, mut app, _) = app();
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.focus, Focus::Content);
    app.handle_key(key(KeyCode::Right));
    assert_eq!(app.tab, ServerTab::Console);
    app.handle_key(key(KeyCode::Left));
    assert_eq!(app.tab, ServerTab::Overview);
    app.handle_key(key(KeyCode::Esc));
    assert_eq!(app.focus, Focus::Sidebar);
    app.handle_key(key(KeyCode::Down));
    assert!(app.add_selected());
    app.handle_key(key(KeyCode::Enter));
    assert!(app.wizard.is_some());
}

#[test]
fn console_scroll_is_bounded_and_end_restores_follow() {
    let (_temp, mut app, id) = app();
    app.focus = Focus::Content;
    app.tab = ServerTab::Console;
    app.sync_console(id, (0..100).map(|line| format!("日志 {line}")).collect());
    app.handle_key(key(KeyCode::PageUp));
    assert_eq!(app.consoles[&id].scroll_from_end, 10);
    assert!(!app.consoles[&id].follow);
    app.handle_key(key(KeyCode::Home));
    assert_eq!(app.consoles[&id].scroll_from_end, 99);
    app.handle_key(key(KeyCode::PageUp));
    assert_eq!(app.consoles[&id].scroll_from_end, 99);
    app.handle_key(key(KeyCode::End));
    assert_eq!(app.consoles[&id].scroll_from_end, 0);
    assert!(app.consoles[&id].follow);
    app.handle_key(key(KeyCode::Enter));
    app.handle_key(key(KeyCode::Char('s')));
    assert_eq!(app.consoles[&id].command.value(), "s");
    assert!(!app.quit);
}

#[test]
fn dangerous_confirmation_defaults_to_cancel_and_only_confirmed_action_runs() {
    let (_temp, mut app, id) = app();
    let action = ConfirmAction::DeleteBackup {
        id,
        name: "backup_test".to_owned(),
    };
    app.modal = Some(Modal::Confirm(ConfirmState {
        title: "确认删除".to_owned(),
        lines: vec!["不可撤销".to_owned()],
        confirm_label: "删除".to_owned(),
        action,
        confirm_selected: false,
    }));
    let commands = app.handle_key(key(KeyCode::Enter));
    assert!(commands.is_empty());
    assert!(app.modal.is_none());

    app.modal = Some(Modal::Confirm(ConfirmState {
        title: "确认删除".to_owned(),
        lines: vec!["不可撤销".to_owned()],
        confirm_label: "删除".to_owned(),
        action: ConfirmAction::DeleteBackup {
            id,
            name: "backup_test".to_owned(),
        },
        confirm_selected: false,
    }));
    app.handle_key(key(KeyCode::Right));
    let commands = app.handle_key(key(KeyCode::Enter));
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0].kind, CommandKind::BackupDelete { .. }));
}

#[test]
fn eula_result_only_opens_confirmation_and_enter_cancel_does_not_write() {
    let (_temp, mut app, id) = app();
    app.apply_message(Message::Lifecycle {
        job: 1,
        id,
        restart: false,
        result: Ok(crate::core::StartResult::EulaRequired),
    });
    assert!(matches!(app.modal, Some(Modal::Confirm(_))));
    let commands = app.handle_key(key(KeyCode::Enter));
    assert!(commands.is_empty());

    app.apply_message(Message::Lifecycle {
        job: 2,
        id,
        restart: false,
        result: Ok(crate::core::StartResult::EulaRequired),
    });
    app.handle_key(key(KeyCode::Right));
    let commands = app.handle_key(key(KeyCode::Enter));
    assert!(matches!(
        commands[0].kind,
        CommandKind::Lifecycle {
            action: LifecycleAction::AgreeEula { restart: false },
            ..
        }
    ));
}

#[test]
fn installer_wizard_requires_second_explicit_confirmation() {
    let (_temp, mut app, _) = app();
    app.open_wizard();
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    let wizard = app.wizard.as_mut().unwrap();
    wizard.fields[0].set_value("整合包");
    wizard.fields[1].set_value("/tmp/installer.jar");
    app.handle_key(key(KeyCode::Enter));
    assert!(matches!(app.modal, Some(Modal::Confirm(_))));
    assert!(
        app.wizard
            .as_ref()
            .is_some_and(|wizard| wizard.step != WizardStep::Installing)
    );
    app.handle_key(key(KeyCode::Right));
    let commands = app.handle_key(key(KeyCode::Enter));
    assert!(matches!(commands[0].kind, CommandKind::WizardInstall(_)));
}

#[test]
fn terminal_messages_clear_job_and_active_progress_before_dispatch() {
    let (_temp, mut app, _) = app();
    app.jobs.insert(
        5,
        JobView {
            label: "测试任务".to_owned(),
            log: VecDeque::new(),
        },
    );
    app.active_progress_job = Some(5);
    app.apply_message(Message::Progress {
        job: 5,
        line: "进行中".to_owned(),
    });
    assert!(app.jobs.contains_key(&5));
    assert_eq!(app.jobs[&5].log.len(), 1);

    app.apply_message(Message::Simple {
        job: 5,
        result: Err("故意失败".to_owned()),
    });
    assert!(!app.jobs.contains_key(&5));
    assert!(app.active_progress_job.is_none());
    assert!(app.notice.is_error);
}

#[test]
fn stale_wizard_generation_is_ignored_and_current_generation_applies() {
    let (_temp, mut app, _) = app();
    app.open_wizard();
    let generation = app.wizard.as_ref().unwrap().generation;
    app.wizard.as_mut().unwrap().versions_loading = true;
    let choice = VersionChoice {
        label: "1.21.1".to_owned(),
        version: "1.21.1".to_owned(),
        mc_version: "1.21.1".to_owned(),
    };
    app.apply_message(Message::Versions {
        job: 1,
        generation: generation + 1,
        result: Ok(vec![choice.clone()]),
    });
    assert!(app.wizard.as_ref().unwrap().versions.is_empty());
    assert!(app.wizard.as_ref().unwrap().versions_loading);

    app.apply_message(Message::Versions {
        job: 2,
        generation,
        result: Ok(vec![choice]),
    });
    let wizard = app.wizard.as_ref().unwrap();
    assert_eq!(wizard.versions.len(), 1);
    assert!(!wizard.versions_loading);
    assert_eq!(wizard.step, WizardStep::Method);

    let server = ServerConfig::new("旧请求", PathBuf::from("/tmp/旧请求"));
    app.apply_message(Message::WizardFinished {
        job: 3,
        generation: generation + 7,
        result: Box::new(Ok(server)),
    });
    assert_eq!(app.wizard.as_ref().unwrap().step, WizardStep::Method);
    assert!(app.wizard.as_ref().unwrap().created_name.is_none());

    let server = ServerConfig::new("新请求", PathBuf::from("/tmp/新请求"));
    app.apply_message(Message::WizardFinished {
        job: 4,
        generation,
        result: Box::new(Ok(server)),
    });
    let wizard = app.wizard.as_ref().unwrap();
    assert_eq!(wizard.step, WizardStep::Done);
    assert_eq!(wizard.created_name.as_deref(), Some("新请求"));
    assert!(wizard.error.is_none());
}

#[test]
fn wizard_request_validation_rejects_unsafe_input_but_accepts_valid_config() {
    let (_temp, mut app, _) = app();
    app.open_wizard();
    assert!(
        app.build_wizard_request()
            .unwrap_err()
            .contains("服务器名称不能为空")
    );

    {
        let wizard = app.wizard.as_mut().unwrap();
        wizard.fields[0].set_value("测试服");
    }
    assert_eq!(
        app.build_wizard_request().unwrap_err(),
        "请先选择要安装的版本"
    );

    {
        let wizard = app.wizard.as_mut().unwrap();
        wizard.fields[4].set_value("2048");
        wizard.fields[5].set_value("1024");
    }
    assert_eq!(
        app.build_wizard_request().unwrap_err(),
        "最小内存不能大于最大内存"
    );

    {
        let wizard = app.wizard.as_mut().unwrap();
        wizard.fields[5].set_value("4096");
        wizard.selected_version = Some(VersionChoice {
            label: "1.21.1".to_owned(),
            version: "1.21.1".to_owned(),
            mc_version: "1.21.1".to_owned(),
        });
        wizard.fields[2].set_value("相对目录");
    }
    assert_eq!(
        app.build_wizard_request().unwrap_err(),
        "目标目录必须是绝对路径，或留空使用默认服务器目录"
    );

    {
        let wizard = app.wizard.as_mut().unwrap();
        wizard.method_index = 1;
        wizard.fields[1].set_value("相对源目录");
        wizard.fields[2].clear();
    }
    assert_eq!(
        app.build_wizard_request().unwrap_err(),
        "源目录或 JAR 必须使用绝对路径"
    );

    {
        let wizard = app.wizard.as_mut().unwrap();
        wizard.fields[1].set_value("/tmp/导入目录");
    }
    let request = app.build_wizard_request().unwrap();
    assert_eq!(request.method, WizardMethod::Import);
    assert_eq!(request.name, "测试服");
    assert_eq!(request.source_path, PathBuf::from("/tmp/导入目录"));
    assert_eq!(request.target_dir, PathBuf::from("/tmp/导入目录"));
    assert_eq!(request.min_memory_mb, 2048);
    assert_eq!(request.max_memory_mb, 4096);
}

#[test]
fn global_java_input_accepts_q_and_ctrl_s_saves() {
    let (_temp, mut app, _) = app();
    app.sidebar_index = app.snapshots.len() + 1;
    app.focus = Focus::Content;
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Char('q')));
    assert!(app.global_form.java_path.value().ends_with('q'));
    assert!(!app.quit);
    let commands = app.handle_key(ctrl('s'));
    assert!(matches!(
        commands[0].kind,
        CommandKind::Simple(SimpleCommand::SaveGlobal { .. })
    ));
}

#[test]
fn file_save_acknowledges_only_the_submitted_snapshot() {
    let (_temp, mut app, id) = app();
    let path = PathBuf::from("配置.txt");
    app.files.entry(id).or_default().editor = Some(FileEditorState {
        path: path.clone(),
        original: "旧内容".to_owned(),
        editor: crate::ui::input::TextEditor::new("第一版"),
    });
    app.files
        .get_mut(&id)
        .unwrap()
        .editor
        .as_mut()
        .unwrap()
        .editor = crate::ui::input::TextEditor::new("保存后的第二版");

    app.apply_message(Message::FileSaved {
        job: 1,
        id,
        path: path.clone(),
        content: "第一版".to_owned(),
        result: Ok(()),
    });
    let editor = app.files[&id].editor.as_ref().unwrap();
    assert_eq!(editor.original, "第一版");
    assert_eq!(editor.editor.value(), "保存后的第二版");
    assert!(editor.dirty());

    app.apply_message(Message::FileSaved {
        job: 2,
        id,
        path,
        content: "保存后的第二版".to_owned(),
        result: Ok(()),
    });
    assert!(!app.files[&id].editor.as_ref().unwrap().dirty());
}

#[test]
fn properties_save_does_not_overwrite_later_edits() {
    let (_temp, mut app, id) = app();
    let first = vec![("motd".to_owned(), "第一版".to_owned())];
    let second = vec![("motd".to_owned(), "第二版".to_owned())];
    let properties = app.properties.entry(id).or_default();
    properties.entries = second.clone();
    properties.dirty = true;

    app.apply_message(Message::PropertiesSaved {
        job: 1,
        id,
        submitted: first.clone(),
        result: Ok(first.clone()),
    });
    let properties = &app.properties[&id];
    assert_eq!(properties.entries, second);
    assert!(properties.dirty);
    assert!(app.notice.text.contains("当前属性仍有未保存修改"));

    app.apply_message(Message::PropertiesSaved {
        job: 2,
        id,
        submitted: second.clone(),
        result: Ok(second.clone()),
    });
    assert!(!app.properties[&id].dirty);

    // 较旧的保存最后完成时，磁盘已回到第一版，不能沿用先前的干净状态。
    app.apply_message(Message::PropertiesSaved {
        job: 3,
        id,
        submitted: first.clone(),
        result: Ok(first.clone()),
    });
    assert_eq!(app.properties[&id].entries, second);
    assert!(app.properties[&id].dirty);
}
