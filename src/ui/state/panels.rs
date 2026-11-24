use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{config::ServerId, core::ServerState};

use super::super::input::TextInput;
use super::super::tasks::{Command, CommandKind, LifecycleAction, SimpleCommand};
use super::{
    App, ConfirmAction, ConfirmState, FileEditorState, Focus, InputAction, InputPrompt, Modal,
    PlayerState, ServerTab,
};

/// 面板按需补发加载命令；向导或全局页不触发服务器数据加载。
pub(super) fn ensure_current_panel(app: &mut App) -> Vec<Command> {
    let mut commands = Vec::new();
    if app.wizard.is_some() || app.global_selected() {
        return commands;
    }
    let Some(id) = app.selected_id() else {
        return commands;
    };
    match app.tab {
        ServerTab::Properties => {
            let properties = app.properties.entry(id).or_default();
            if !properties.loaded && !properties.loading {
                properties.loading = true;
                app.issue(
                    CommandKind::PropertiesLoad(id),
                    "加载 server.properties".to_owned(),
                    &mut commands,
                );
            }
        }
        ServerTab::Backups => {
            let backups = app.backups.entry(id).or_default();
            if !backups.loaded && !backups.loading {
                backups.loading = true;
                app.issue(
                    CommandKind::BackupsList(id),
                    "加载备份列表".to_owned(),
                    &mut commands,
                );
            }
        }
        ServerTab::Files => {
            let needs_load = app
                .files
                .get(&id)
                .is_none_or(|files| !files.loaded && !files.loading && files.editor.is_none());
            if needs_load {
                app.reload_files(id, &mut commands);
            }
        }
        _ => {}
    }
    commands
}

impl App {
    pub(super) fn handle_overview(
        &mut self,
        key: KeyEvent,
        id: ServerId,
        commands: &mut Vec<Command>,
    ) {
        match key.code {
            KeyCode::Char('s') => {
                self.issue_lifecycle(
                    id,
                    LifecycleAction::Start {
                        restart: false,
                        allow_no_rcon: false,
                    },
                    "启动服务器",
                    commands,
                );
            }
            KeyCode::Char('t') => {
                self.issue(CommandKind::Stop(id), "停止服务器".to_owned(), commands);
            }
            KeyCode::Char('r') => {
                self.issue_lifecycle(
                    id,
                    LifecycleAction::Start {
                        restart: true,
                        allow_no_rcon: false,
                    },
                    "重启服务器",
                    commands,
                );
            }
            _ => {}
        }
    }

    pub(super) fn handle_console(
        &mut self,
        key: KeyEvent,
        id: ServerId,
        commands: &mut Vec<Command>,
    ) {
        let running = self
            .selected_server()
            .is_some_and(|server| server.state == ServerState::Running);
        let active = self
            .consoles
            .get(&id)
            .is_some_and(|console| console.command_active);
        if active {
            let mut submitted = None;
            let mut not_running = false;
            let console = self.consoles.entry(id).or_default();
            match key.code {
                KeyCode::Esc => console.command_active = false,
                KeyCode::Enter => {
                    let command = console.command.value();
                    if !command.trim().is_empty() && running {
                        console.command.clear();
                        submitted = Some(command);
                    } else if !running {
                        not_running = true;
                    }
                }
                _ => {
                    console.command.handle_key(key);
                }
            }
            if let Some(command) = submitted {
                self.issue(
                    CommandKind::Simple(SimpleCommand::SendCommand { id, command }),
                    "发送控制台指令".to_owned(),
                    commands,
                );
            }
            if not_running {
                self.notice("服务器未运行，无法发送指令", true);
            }
            return;
        }
        let console = self.consoles.entry(id).or_default();
        match key.code {
            KeyCode::Enter | KeyCode::Char('i') => console.command_active = true,
            KeyCode::Up => console.scroll(-1, 1),
            KeyCode::Down => console.scroll(1, 1),
            KeyCode::PageUp => console.scroll(-1, 10),
            KeyCode::PageDown => console.scroll(1, 10),
            KeyCode::Home => console.home(),
            KeyCode::End => console.end(),
            KeyCode::Char('f') => {
                console.follow = !console.follow;
                if console.follow {
                    console.end();
                }
            }
            KeyCode::Char('c') => {
                self.issue(
                    CommandKind::Simple(SimpleCommand::ClearConsole(id)),
                    "清空控制台".to_owned(),
                    commands,
                );
            }
            _ => {}
        }
    }

    pub(super) fn handle_players(
        &mut self,
        key: KeyEvent,
        id: ServerId,
        commands: &mut Vec<Command>,
    ) {
        let Some(snapshot) = self.selected_server() else {
            return;
        };
        if !snapshot.rcon_available {
            return;
        }
        let players = snapshot.players.clone();
        let state = self
            .players
            .entry(id)
            .or_insert(PlayerState { selected: 0 });
        match key.code {
            KeyCode::Up => state.selected = state.selected.saturating_sub(1),
            KeyCode::Down => {
                state.selected = (state.selected + 1).min(players.len().saturating_sub(1));
            }
            KeyCode::Char('k') => {
                if let Some(player) = players.get(state.selected) {
                    self.issue(
                        CommandKind::Simple(SimpleCommand::KickPlayer {
                            id,
                            player: player.clone(),
                        }),
                        format!("踢出玩家 {player}"),
                        commands,
                    );
                }
            }
            KeyCode::Char('b') => {
                if let Some(player) = players.get(state.selected) {
                    self.modal = Some(Modal::Confirm(ConfirmState {
                        title: "确认封禁玩家".to_owned(),
                        lines: vec![
                            format!("将封禁玩家「{player}」。"),
                            "该操作会修改服务器的封禁列表。".to_owned(),
                        ],
                        confirm_label: "确认封禁".to_owned(),
                        action: ConfirmAction::BanPlayer {
                            id,
                            player: player.clone(),
                        },
                        confirm_selected: false,
                    }));
                }
            }
            _ => {}
        }
    }

    pub(super) fn handle_properties(
        &mut self,
        key: KeyEvent,
        id: ServerId,
        commands: &mut Vec<Command>,
    ) {
        let properties = self.properties.entry(id).or_default();
        if let Some((index, input)) = &mut properties.editing {
            match key.code {
                KeyCode::Esc => properties.editing = None,
                KeyCode::Enter => {
                    if let Some(entry) = properties.entries.get_mut(*index) {
                        entry.1 = input.value();
                        properties.dirty = true;
                    }
                    properties.editing = None;
                }
                _ => {
                    input.handle_key(key);
                }
            }
            return;
        }
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL)
            || key.code == KeyCode::F(5)
        {
            if properties.entries.is_empty() && !properties.exists {
                self.notice("文件不存在；按 n 建立默认属性后再保存", true);
                return;
            }
            let entries = properties.entries.clone();
            self.issue(
                CommandKind::PropertiesSave { id, entries },
                "保存 server.properties".to_owned(),
                commands,
            );
            return;
        }
        match key.code {
            KeyCode::Up => properties.selected = properties.selected.saturating_sub(1),
            KeyCode::Down => {
                properties.selected =
                    (properties.selected + 1).min(properties.entries.len().saturating_sub(1));
            }
            KeyCode::PageUp => properties.selected = properties.selected.saturating_sub(10),
            KeyCode::PageDown => {
                properties.selected =
                    (properties.selected + 10).min(properties.entries.len().saturating_sub(1));
            }
            KeyCode::Enter => {
                if let Some((_, value)) = properties.entries.get(properties.selected) {
                    properties.editing =
                        Some((properties.selected, TextInput::new(value.as_str())));
                }
            }
            KeyCode::Char('r') => {
                properties.loading = true;
                self.issue(
                    CommandKind::PropertiesLoad(id),
                    "重新加载 server.properties".to_owned(),
                    commands,
                );
            }
            KeyCode::Char('n') if !properties.exists => {
                properties.entries = [
                    ("server-port", "25565"),
                    ("online-mode", "true"),
                    ("max-players", "20"),
                    ("view-distance", "10"),
                    ("motd", "A Minecraft Server"),
                    ("enable-rcon", "false"),
                ]
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value.to_owned()))
                .collect();
                properties.dirty = true;
                properties.selected = 0;
                self.notice("默认属性仅在内存中；Ctrl+S 明确保存后才会写盘", false);
            }
            _ => {}
        }
    }

    pub(super) fn handle_backups(
        &mut self,
        key: KeyEvent,
        id: ServerId,
        commands: &mut Vec<Command>,
    ) {
        let stopped = self.server_stopped(id);
        let backups = self.backups.entry(id).or_default();
        match key.code {
            KeyCode::Up => backups.selected = backups.selected.saturating_sub(1),
            KeyCode::Down => {
                backups.selected =
                    (backups.selected + 1).min(backups.items.len().saturating_sub(1));
            }
            KeyCode::Char('r') => {
                backups.loading = true;
                self.issue(
                    CommandKind::BackupsList(id),
                    "刷新备份列表".to_owned(),
                    commands,
                );
            }
            KeyCode::Char('c') => {
                if stopped {
                    self.issue(
                        CommandKind::BackupCreate(id),
                        "创建世界备份".to_owned(),
                        commands,
                    );
                } else {
                    self.notice("创建备份前必须停止服务器", true);
                }
            }
            KeyCode::Enter => {
                if !stopped {
                    self.notice("恢复备份前必须停止服务器", true);
                } else if let Some(name) = backups.items.get(backups.selected).cloned() {
                    self.modal = Some(Modal::Confirm(ConfirmState {
                        title: "确认恢复世界备份".to_owned(),
                        lines: vec![
                            format!("将使用备份「{name}」替换当前世界目录。"),
                            "当前世界中不属于该备份的内容将丢失，此操作不可撤销。".to_owned(),
                        ],
                        confirm_label: "确认恢复".to_owned(),
                        action: ConfirmAction::RestoreBackup { id, name },
                        confirm_selected: false,
                    }));
                }
            }
            KeyCode::Char('d') => {
                if let Some(name) = backups.items.get(backups.selected).cloned() {
                    self.modal = Some(Modal::Confirm(ConfirmState {
                        title: "确认删除备份".to_owned(),
                        lines: vec![format!("备份「{name}」将被递归删除，不可撤销。")],
                        confirm_label: "确认删除".to_owned(),
                        action: ConfirmAction::DeleteBackup { id, name },
                        confirm_selected: false,
                    }));
                }
            }
            _ => {}
        }
    }

    pub(super) fn handle_files(
        &mut self,
        key: KeyEvent,
        id: ServerId,
        commands: &mut Vec<Command>,
    ) {
        if self
            .files
            .get(&id)
            .is_some_and(|files| files.editor.is_some())
        {
            self.handle_file_editor(key, id, commands);
            return;
        }
        let files = self.files.entry(id).or_default();
        match key.code {
            KeyCode::Up => files.selected = files.selected.saturating_sub(1),
            KeyCode::Down => {
                files.selected = (files.selected + 1).min(files.entries.len().saturating_sub(1));
            }
            KeyCode::PageUp => files.selected = files.selected.saturating_sub(10),
            KeyCode::PageDown => {
                files.selected = (files.selected + 10).min(files.entries.len().saturating_sub(1));
            }
            KeyCode::Backspace | KeyCode::Char('u') => {
                if files.current_dir.parent().is_some() || !files.current_dir.as_os_str().is_empty()
                {
                    files.current_dir = files
                        .current_dir
                        .parent()
                        .unwrap_or_else(|| Path::new(""))
                        .to_path_buf();
                    files.loaded = false;
                    files.loading = false;
                    files.selected = 0;
                }
            }
            KeyCode::Char('r') => self.reload_files(id, commands),
            KeyCode::Enter => {
                if let Some(entry) = files.entries.get(files.selected).cloned() {
                    if entry.is_dir {
                        files.current_dir = PathBuf::from(entry.path);
                        files.loaded = false;
                        files.loading = false;
                        files.selected = 0;
                    } else {
                        self.issue(
                            CommandKind::FileRead {
                                id,
                                path: PathBuf::from(entry.path),
                            },
                            "读取文本文件".to_owned(),
                            commands,
                        );
                    }
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                let is_dir = key.code == KeyCode::Char('N');
                self.modal = Some(Modal::Input(InputPrompt {
                    title: if is_dir {
                        "新建目录"
                    } else {
                        "新建文件"
                    }
                    .to_owned(),
                    description: "输入当前目录下的纯名称，不能包含路径分隔符".to_owned(),
                    input: TextInput::default(),
                    action: InputAction::CreateFile { id, is_dir },
                }));
            }
            KeyCode::F(2) => {
                if let Some(entry) = files.entries.get(files.selected) {
                    self.modal = Some(Modal::Input(InputPrompt {
                        title: "重命名".to_owned(),
                        description: "只能在原目录内改名，不能用此操作移动条目".to_owned(),
                        input: TextInput::new(&entry.name),
                        action: InputAction::RenameFile {
                            id,
                            path: PathBuf::from(&entry.path),
                        },
                    }));
                }
            }
            KeyCode::Char('d') => {
                if let Some(entry) = files.entries.get(files.selected) {
                    self.modal = Some(Modal::Confirm(ConfirmState {
                        title: "确认删除文件条目".to_owned(),
                        lines: vec![
                            format!("将删除「{}」。", entry.name),
                            if entry.is_dir {
                                "这是目录，其全部内容会被递归删除，不可撤销。"
                            } else {
                                "文件删除后不可撤销。"
                            }
                            .to_owned(),
                        ],
                        confirm_label: "确认删除".to_owned(),
                        action: ConfirmAction::DeleteFile {
                            id,
                            path: PathBuf::from(&entry.path),
                        },
                        confirm_selected: false,
                    }));
                }
            }
            _ => {}
        }
    }

    fn handle_file_editor(&mut self, key: KeyEvent, id: ServerId, commands: &mut Vec<Command>) {
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL)
            || key.code == KeyCode::F(5)
        {
            let files = self.files.entry(id).or_default();
            if let Some(editor) = &files.editor {
                let path = editor.path.clone();
                let content = editor.editor.value();
                self.issue(
                    CommandKind::FileWrite { id, path, content },
                    "保存文本文件".to_owned(),
                    commands,
                );
            }
            return;
        }
        if key.code == KeyCode::Esc {
            let dirty = self
                .files
                .get(&id)
                .and_then(|files| files.editor.as_ref())
                .is_some_and(FileEditorState::dirty);
            if dirty {
                self.modal = Some(Modal::Confirm(ConfirmState {
                    title: "放弃未保存修改".to_owned(),
                    lines: vec!["关闭编辑器将丢弃尚未保存的内容。".to_owned()],
                    confirm_label: "放弃修改".to_owned(),
                    action: ConfirmAction::CloseFileEditor { id },
                    confirm_selected: false,
                }));
            } else if let Some(files) = self.files.get_mut(&id) {
                files.editor = None;
            }
            return;
        }
        let files = self.files.entry(id).or_default();
        if let Some(editor) = &mut files.editor {
            match key.code {
                KeyCode::PageUp => {
                    for _ in 0..20 {
                        editor.editor.handle_key(with_code(key, KeyCode::Up));
                    }
                }
                KeyCode::PageDown => {
                    for _ in 0..20 {
                        editor.editor.handle_key(with_code(key, KeyCode::Down));
                    }
                }
                _ => {
                    editor.editor.handle_key(key);
                }
            }
        }
    }

    pub(super) fn handle_global(&mut self, key: KeyEvent, commands: &mut Vec<Command>) {
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL)
            || key.code == KeyCode::F(5)
        {
            let default_dir = PathBuf::from(self.global_form.default_dir.value());
            let java_path = self.global_form.java_path.value();
            self.issue(
                CommandKind::Simple(SimpleCommand::SaveGlobal {
                    default_dir,
                    java_path,
                }),
                "保存全局设置".to_owned(),
                commands,
            );
            return;
        }
        match key.code {
            KeyCode::Esc => self.focus = Focus::Sidebar,
            KeyCode::Up | KeyCode::BackTab => {
                self.global_form.selected = self.global_form.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Tab => {
                self.global_form.selected = (self.global_form.selected + 1).min(1);
            }
            _ => {
                let field = if self.global_form.selected == 0 {
                    &mut self.global_form.default_dir
                } else {
                    &mut self.global_form.java_path
                };
                if field.handle_key(key) {
                    self.global_form.dirty = true;
                }
            }
        }
    }

    pub(super) fn handle_server_settings(
        &mut self,
        key: KeyEvent,
        id: ServerId,
        commands: &mut Vec<Command>,
    ) {
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL)
            || key.code == KeyCode::F(5)
        {
            self.save_server(id, commands);
            return;
        }
        match key.code {
            KeyCode::Esc => self.focus = Focus::Sidebar,
            KeyCode::Up | KeyCode::BackTab => {
                self.server_form.selected = self.server_form.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Tab => {
                self.server_form.selected =
                    (self.server_form.selected + 1).min(self.server_form.fields.len() - 1);
            }
            KeyCode::F(2) => self.prompt_location(id, false),
            KeyCode::F(3) => self.prompt_location(id, true),
            KeyCode::Delete => self.confirm_remove_server(id, false),
            KeyCode::F(8) => self.confirm_remove_server(id, true),
            _ => {
                if let Some(field) = self.server_form.fields.get_mut(self.server_form.selected)
                    && field.handle_key(key)
                {
                    self.server_form.dirty = true;
                }
            }
        }
    }

    fn save_server(&mut self, id: ServerId, commands: &mut Vec<Command>) {
        let Some(mut config) = self.selected_config().cloned() else {
            return;
        };
        if self.server_form.fields.len() != 7 {
            return;
        }
        let min_memory_mb = match self.server_form.fields[2].value().parse::<i32>() {
            Ok(value) if value >= 0 => value,
            _ => {
                self.notice("最小内存必须是非负整数", true);
                return;
            }
        };
        let max_memory_mb = match self.server_form.fields[3].value().parse::<i32>() {
            Ok(value) if value >= 0 => value,
            _ => {
                self.notice("最大内存必须是非负整数", true);
                return;
            }
        };
        if min_memory_mb > max_memory_mb {
            self.notice("最小内存不能大于最大内存", true);
            return;
        }
        config.name = self.server_form.fields[0].value();
        config.java_path = self.server_form.fields[1].value();
        config.min_memory_mb = min_memory_mb;
        config.max_memory_mb = max_memory_mb;
        config.additional_jvm_args = self.server_form.fields[4].value();
        config.world_dir = PathBuf::from(self.server_form.fields[5].value());
        config.backup_dir = PathBuf::from(self.server_form.fields[6].value());
        self.issue(
            CommandKind::Simple(SimpleCommand::SaveServer {
                id,
                config: Box::new(config),
            }),
            "保存服务器设置".to_owned(),
            commands,
        );
    }

    fn prompt_location(&mut self, id: ServerId, relocate: bool) {
        self.modal = Some(Modal::Input(InputPrompt {
            title: if relocate {
                "重新定位到手动搬迁后的目录"
            } else {
                "由管理器移动服务器目录"
            }
            .to_owned(),
            description: if relocate {
                "仅修改注册路径，不复制或删除文件；目标必须已存在且包含可启动核心"
            } else {
                "目标必须是不存在的绝对路径；管理器将移动全部服务器文件"
            }
            .to_owned(),
            input: TextInput::default(),
            action: InputAction::ChangeLocation { id, relocate },
        }));
    }

    fn confirm_remove_server(&mut self, id: ServerId, delete_files: bool) {
        let name = self
            .selected_config()
            .map(|server| server.name.clone())
            .unwrap_or_else(|| "?".to_owned());
        self.modal = Some(Modal::Confirm(ConfirmState {
            title: if delete_files {
                "确认删除服务器及全部文件"
            } else {
                "确认移除服务器注册"
            }
            .to_owned(),
            lines: vec![
                format!("服务器：{name}"),
                if delete_files {
                    "配置和整个 working_dir 将被递归删除，不可撤销；运行中服务器会先停止。"
                } else {
                    "只移除配置，保留 working_dir 中的全部文件；运行中服务器会先停止。"
                }
                .to_owned(),
            ],
            confirm_label: if delete_files {
                "删除配置和文件"
            } else {
                "仅移除注册"
            }
            .to_owned(),
            action: ConfirmAction::RemoveServer { id, delete_files },
            confirm_selected: false,
        }));
    }

    fn reload_files(&mut self, id: ServerId, commands: &mut Vec<Command>) {
        let path = self.files.entry(id).or_default().current_dir.clone();
        if let Some(files) = self.files.get_mut(&id) {
            files.loading = true;
        }
        self.issue(
            CommandKind::FileList { id, path },
            "浏览服务器目录".to_owned(),
            commands,
        );
    }
}

fn with_code(key: KeyEvent, code: KeyCode) -> KeyEvent {
    KeyEvent { code, ..key }
}
