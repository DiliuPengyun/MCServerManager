use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use tempfile::TempDir;

use super::render;
use crate::{
    config::{AppConfig, ServerConfig},
    core::{ServerSnapshot, ServerState},
    ui::{
        state::{
            App, ConfirmAction, ConfirmState, FilesState, Focus, Modal, ServerTab, WizardStep,
        },
        tasks::VersionChoice,
    },
};

fn fixture() -> (TempDir, App) {
    let temp = TempDir::new().unwrap();
    let mut server = ServerConfig::new("中文服务器甲", temp.path().join("任意/外部目录"));
    let id = server.id;
    server.server_jar_path = "核心.jar".into();
    let snapshot = ServerSnapshot {
        index: 0,
        id,
        name: server.name.clone(),
        working_dir: server.working_dir.clone(),
        state: ServerState::Stopped,
        rcon_available: false,
        players: Vec::new(),
    };
    let mut config = AppConfig::fresh(temp.path().join("默认"));
    config.servers.push(server);
    config.active_server_id = Some(id);
    let mut app = App::new(Arc::new(config), vec![snapshot]);
    app.files.insert(id, FilesState::default());
    (temp, app)
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn test_backend_renders_chinese_width_without_corruption() {
    let (_temp, mut app) = fixture();
    app.focus = Focus::Content;
    let backend = TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, &app)).unwrap();
    let text = buffer_text(&terminal);
    // TestBackend 的宽字符后续单元以空白占位，去掉占位空格检查中文没有错位或损坏。
    let compact = text.replace(' ', "");
    assert!(compact.contains("中文服务器甲"), "{text}");
    assert!(compact.contains("任意/外部目录"), "{text}");
    assert!(compact.contains("概览"), "{text}");
    assert_eq!(terminal.backend().buffer().area.width, 100);
}

#[test]
fn narrow_confirmation_stays_inside_backend_and_defaults_to_cancel() {
    let (_temp, mut app) = fixture();
    let id = app.selected_id().unwrap();
    app.modal = Some(Modal::Confirm(ConfirmState {
        title: "确认删除世界备份".to_owned(),
        lines: vec!["中文内容很长，会在弹窗内换行而不会越出终端边界".to_owned()],
        confirm_label: "确认删除".to_owned(),
        action: ConfirmAction::DeleteBackup {
            id,
            name: "备份".to_owned(),
        },
        confirm_selected: false,
    }));
    let mut terminal = Terminal::new(TestBackend::new(30, 9)).unwrap();
    terminal.draw(|frame| render(frame, &app)).unwrap();
    let text = buffer_text(&terminal);
    assert!(text.replace(' ', "").contains("[取消]"), "{text}");
    assert_eq!(terminal.backend().buffer().area.width, 30);
    assert_eq!(terminal.backend().buffer().area.height, 9);
}

#[test]
fn wizard_renders_key_copy_for_all_seven_steps() {
    let (_temp, mut app) = fixture();
    app.handle_key(key(KeyCode::Down));
    app.handle_key(key(KeyCode::Enter));
    assert!(app.wizard.is_some());
    let checks = [
        (WizardStep::Method, "选择方式"),
        (WizardStep::Core, "选择自动安装核心"),
        (
            WizardStep::NeoMcVersion,
            "输入 NeoForge 对应的 Minecraft 版本",
        ),
        (WizardStep::Versions, "选择版本"),
        (WizardStep::Configure, "服务器名称"),
        (WizardStep::Installing, "正在后台安装，请查看进度"),
        (WizardStep::Done, "安装或注册失败"),
    ];
    for (step, expected) in checks {
        let wizard = app.wizard.as_mut().unwrap();
        wizard.step = step;
        wizard.versions_loading = false;
        wizard.versions_error = None;
        wizard.versions = vec![VersionChoice {
            label: "1.21.1".to_owned(),
            version: "1.21.1".to_owned(),
            mc_version: "1.21.1".to_owned(),
        }];
        wizard.version_selected = 0;
        wizard.created_name = None;
        wizard.error = Some("解析失败".to_owned());
        wizard.progress.clear();
        wizard.progress.push_back("下载中".to_owned());
        // 每步使用全新 TestBackend，避免宽字符与上一帧标题残留互相覆盖。
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let text = buffer_text(&terminal).replace(' ', "");
        let expected = expected.replace(' ', "");
        assert!(
            text.contains(&expected),
            "步骤 {step:?} 缺少文案 {expected}：{text}"
        );
    }

    app.wizard.as_mut().unwrap().created_name = Some("测试服".to_owned());
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal.draw(|frame| render(frame, &app)).unwrap();
    let text = buffer_text(&terminal).replace(' ', "");
    assert!(text.contains("已安装并注册"), "{text}");
}

#[test]
fn console_home_renders_oldest_lines_and_end_renders_latest() {
    let (_temp, mut app) = fixture();
    let id = app.selected_id().unwrap();
    app.focus = Focus::Content;
    app.tab = ServerTab::Console;
    app.sync_console(id, (0..100).map(|line| format!("日志 {line}")).collect());
    app.handle_key(key(KeyCode::Home));
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal.draw(|frame| render(frame, &app)).unwrap();
    let text = buffer_text(&terminal);
    assert!(text.replace(' ', "").contains("日志0"), "{text}");
    app.handle_key(key(KeyCode::End));
    terminal.draw(|frame| render(frame, &app)).unwrap();
    let text = buffer_text(&terminal);
    assert!(text.replace(' ', "").contains("日志99"), "{text}");
}
