use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    config::{AppConfig, CoreType, ServerConfig, ServerId},
    core::{ServerSnapshot, ServerState},
    server::files::FileEntry,
};

use super::{
    input::{TextEditor, TextInput},
    tasks::{
        Command, CommandKind, JobId, LifecycleAction, Message, VersionChoice, WizardMethod,
        WizardRequest,
    },
};

mod messages;
mod modal;
mod panels;
mod wizard;

pub(super) const SERVER_TABS: [ServerTab; 7] = [
    ServerTab::Overview,
    ServerTab::Console,
    ServerTab::Players,
    ServerTab::Properties,
    ServerTab::Backups,
    ServerTab::Files,
    ServerTab::Settings,
];
pub(super) const AUTO_CORES: [CoreType; 5] = [
    CoreType::Vanilla,
    CoreType::Paper,
    CoreType::Forge,
    CoreType::Neoforge,
    CoreType::Fabric,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Focus {
    Sidebar,
    Content,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ServerTab {
    Overview,
    Console,
    Players,
    Properties,
    Backups,
    Files,
    Settings,
}

impl ServerTab {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Overview => "概览",
            Self::Console => "控制台",
            Self::Players => "玩家",
            Self::Properties => "属性",
            Self::Backups => "备份",
            Self::Files => "文件",
            Self::Settings => "服务器设置",
        }
    }

    fn index(self) -> usize {
        SERVER_TABS.iter().position(|tab| *tab == self).unwrap_or(0)
    }
}

#[derive(Clone, Debug)]
pub(super) struct Notice {
    pub(super) text: String,
    pub(super) is_error: bool,
}

impl Default for Notice {
    fn default() -> Self {
        Self {
            text: "退出只断开管理器，不会停止 Minecraft".to_owned(),
            is_error: false,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct JobView {
    pub(super) label: String,
    pub(super) log: VecDeque<String>,
}

#[derive(Clone, Debug)]
pub(super) struct ConsoleState {
    pub(super) lines: Vec<String>,
    pub(super) scroll_from_end: usize,
    pub(super) follow: bool,
    pub(super) command: TextInput,
    pub(super) command_active: bool,
}

impl Default for ConsoleState {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            scroll_from_end: 0,
            follow: true,
            command: TextInput::default(),
            command_active: false,
        }
    }
}

impl ConsoleState {
    pub(super) fn scroll(&mut self, delta: isize, page: usize) {
        let amount = page.max(1);
        if delta < 0 {
            self.scroll_from_end = self
                .scroll_from_end
                .saturating_add(amount)
                .min(self.lines.len().saturating_sub(1));
            self.follow = false;
        } else {
            self.scroll_from_end = self.scroll_from_end.saturating_sub(amount);
            if self.scroll_from_end == 0 {
                self.follow = true;
            }
        }
    }

    pub(super) fn home(&mut self) {
        self.scroll_from_end = self.lines.len().saturating_sub(1);
        self.follow = false;
    }

    pub(super) fn end(&mut self) {
        self.scroll_from_end = 0;
        self.follow = true;
    }

    fn clamp(&mut self) {
        self.scroll_from_end = self.scroll_from_end.min(self.lines.len().saturating_sub(1));
        if self.follow {
            self.scroll_from_end = 0;
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct PlayerState {
    pub(super) selected: usize,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PropertiesState {
    pub(super) loaded: bool,
    pub(super) loading: bool,
    pub(super) exists: bool,
    pub(super) entries: Vec<(String, String)>,
    pub(super) selected: usize,
    pub(super) editing: Option<(usize, TextInput)>,
    pub(super) dirty: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct BackupState {
    pub(super) loaded: bool,
    pub(super) loading: bool,
    pub(super) items: Vec<String>,
    pub(super) selected: usize,
}

#[derive(Clone, Debug)]
pub(super) struct FileEditorState {
    pub(super) path: PathBuf,
    pub(super) editor: TextEditor,
    pub(super) original: String,
}

impl FileEditorState {
    pub(super) fn dirty(&self) -> bool {
        self.editor.value() != self.original
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct FilesState {
    pub(super) current_dir: PathBuf,
    pub(super) entries: Vec<FileEntry>,
    pub(super) selected: usize,
    pub(super) loaded: bool,
    pub(super) loading: bool,
    pub(super) editor: Option<FileEditorState>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct GlobalForm {
    pub(super) default_dir: TextInput,
    pub(super) java_path: TextInput,
    pub(super) selected: usize,
    pub(super) dirty: bool,
    pub(super) initialized: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ServerForm {
    pub(super) id: Option<ServerId>,
    pub(super) fields: Vec<TextInput>,
    pub(super) selected: usize,
    pub(super) dirty: bool,
}

impl ServerForm {
    pub(super) const FIELD_NAMES: [&'static str; 7] = [
        "名称",
        "Java 路径（java 表示使用全局设置）",
        "最小内存 MB",
        "最大内存 MB",
        "额外 JVM 参数",
        "世界目录",
        "备份目录",
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WizardStep {
    Method,
    Core,
    NeoMcVersion,
    Versions,
    Configure,
    Installing,
    Done,
}

#[derive(Clone, Debug)]
pub(super) struct WizardState {
    pub(super) generation: u64,
    pub(super) step: WizardStep,
    pub(super) method_index: usize,
    pub(super) core_index: usize,
    pub(super) versions: Vec<VersionChoice>,
    pub(super) version_selected: usize,
    pub(super) versions_loading: bool,
    pub(super) versions_error: Option<String>,
    pub(super) neo_mc_input: TextInput,
    pub(super) selected_version: Option<VersionChoice>,
    pub(super) fields: Vec<TextInput>,
    pub(super) field_selected: usize,
    pub(super) progress: VecDeque<String>,
    pub(super) error: Option<String>,
    pub(super) created_name: Option<String>,
    pub(super) pending_request: Option<WizardRequest>,
}

impl WizardState {
    pub(super) const FIELD_NAMES: [&'static str; 7] = [
        "服务器名称",
        "源目录 / 安装器 / JAR 路径",
        "目标目录（留空使用默认目录/名称）",
        "Java 路径",
        "最小内存 MB",
        "最大内存 MB",
        "额外 JVM 参数",
    ];

    fn new(generation: u64, config: &AppConfig) -> Self {
        Self {
            generation,
            step: WizardStep::Method,
            method_index: 0,
            core_index: 0,
            versions: Vec::new(),
            version_selected: 0,
            versions_loading: false,
            versions_error: None,
            neo_mc_input: TextInput::new("1.21.1"),
            selected_version: None,
            fields: vec![
                TextInput::default(),
                TextInput::default(),
                TextInput::default(),
                TextInput::new(if config.global_java_path.is_empty() {
                    "java"
                } else {
                    &config.global_java_path
                }),
                TextInput::new("1024"),
                TextInput::new("4096"),
                TextInput::default(),
            ],
            field_selected: 0,
            progress: VecDeque::new(),
            error: None,
            created_name: None,
            pending_request: None,
        }
    }

    pub(super) fn method(&self) -> WizardMethod {
        WizardMethod::ALL[self.method_index.min(WizardMethod::ALL.len() - 1)]
    }

    pub(super) fn core_type(&self) -> CoreType {
        AUTO_CORES[self.core_index.min(AUTO_CORES.len() - 1)]
    }
}

#[derive(Clone, Debug)]
pub(super) struct ConfirmState {
    pub(super) title: String,
    pub(super) lines: Vec<String>,
    pub(super) confirm_label: String,
    pub(super) action: ConfirmAction,
    /// 危险操作默认选中取消，必须显式移到确认项。
    pub(super) confirm_selected: bool,
}

#[derive(Clone, Debug)]
pub(super) struct RconState {
    pub(super) id: ServerId,
    pub(super) restart: bool,
    /// 0 取消、1 启用 RCON、2 本次不启用。
    pub(super) selected: usize,
}

#[derive(Clone, Debug)]
pub(super) enum Modal {
    Confirm(ConfirmState),
    Input(InputPrompt),
    Rcon(RconState),
}

#[derive(Clone, Debug)]
pub(super) struct InputPrompt {
    pub(super) title: String,
    pub(super) description: String,
    pub(super) input: TextInput,
    pub(super) action: InputAction,
}

#[derive(Clone, Debug)]
pub(super) enum InputAction {
    CreateFile { id: ServerId, is_dir: bool },
    RenameFile { id: ServerId, path: PathBuf },
    ChangeLocation { id: ServerId, relocate: bool },
}

#[derive(Clone, Debug)]
pub(super) enum ConfirmAction {
    AgreeEula {
        id: ServerId,
        restart: bool,
    },
    RunInstaller {
        id: ServerId,
        restart: bool,
    },
    StartWithoutRcon {
        id: ServerId,
        restart: bool,
    },
    RestoreBackup {
        id: ServerId,
        name: String,
    },
    DeleteBackup {
        id: ServerId,
        name: String,
    },
    DeleteFile {
        id: ServerId,
        path: PathBuf,
    },
    BanPlayer {
        id: ServerId,
        player: String,
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
    CloseFileEditor {
        id: ServerId,
    },
    RunWizardInstaller,
}

pub(super) struct App {
    pub(super) config: Arc<AppConfig>,
    pub(super) snapshots: Vec<ServerSnapshot>,
    pub(super) sidebar_index: usize,
    pub(super) focus: Focus,
    pub(super) tab: ServerTab,
    pub(super) consoles: HashMap<ServerId, ConsoleState>,
    pub(super) players: HashMap<ServerId, PlayerState>,
    pub(super) properties: HashMap<ServerId, PropertiesState>,
    pub(super) backups: HashMap<ServerId, BackupState>,
    pub(super) files: HashMap<ServerId, FilesState>,
    pub(super) global_form: GlobalForm,
    pub(super) server_form: ServerForm,
    pub(super) wizard: Option<WizardState>,
    pub(super) modal: Option<Modal>,
    pub(super) jobs: BTreeMap<JobId, JobView>,
    pub(super) active_progress_job: Option<JobId>,
    pub(super) notice: Notice,
    pub(super) quit: bool,
    next_job: JobId,
    next_generation: u64,
}

impl App {
    pub(super) fn new(config: Arc<AppConfig>, snapshots: Vec<ServerSnapshot>) -> Self {
        let sidebar_index = config
            .active_server_id
            .and_then(|id| config.index_of(id))
            .unwrap_or(0)
            .min(snapshots.len().saturating_add(1));
        let mut app = Self {
            config,
            snapshots,
            sidebar_index,
            focus: Focus::Sidebar,
            tab: ServerTab::Overview,
            consoles: HashMap::new(),
            players: HashMap::new(),
            properties: HashMap::new(),
            backups: HashMap::new(),
            files: HashMap::new(),
            global_form: GlobalForm::default(),
            server_form: ServerForm::default(),
            wizard: None,
            modal: None,
            jobs: BTreeMap::new(),
            active_progress_job: None,
            notice: Notice::default(),
            quit: false,
            next_job: 1,
            next_generation: 1,
        };
        app.sync_global_form();
        app.sync_server_form();
        app
    }

    pub(super) fn sync_manager(&mut self, config: Arc<AppConfig>, snapshots: Vec<ServerSnapshot>) {
        let selected_id = self.selected_server().map(|server| server.id);
        self.config = config;
        self.snapshots = snapshots;
        if let Some(id) = selected_id
            && let Some(index) = self.config.index_of(id)
        {
            self.sidebar_index = index;
        }
        let maximum = self.snapshots.len().saturating_add(1);
        self.sidebar_index = self.sidebar_index.min(maximum);
        for snapshot in &self.snapshots {
            let console = self.consoles.entry(snapshot.id).or_default();
            console.clamp();
            let players = self
                .players
                .entry(snapshot.id)
                .or_insert(PlayerState { selected: 0 });
            players.selected = players
                .selected
                .min(snapshot.players.len().saturating_sub(1));
        }
        self.sync_global_form();
        self.sync_server_form();
    }

    /// 每次刷新只同步 core 快照；控制台内容由事件循环从 ConsoleBuffer 注入。
    pub(super) fn sync_console(&mut self, id: ServerId, lines: Vec<String>) {
        let console = self.consoles.entry(id).or_default();
        console.lines = lines;
        console.clamp();
    }

    fn sync_global_form(&mut self) {
        if !self.global_form.initialized || !self.global_form.dirty {
            self.global_form
                .default_dir
                .set_value(self.config.default_server_dir.to_string_lossy());
            self.global_form
                .java_path
                .set_value(&self.config.global_java_path);
            self.global_form.initialized = true;
        }
    }

    fn sync_server_form(&mut self) {
        let Some(server) = self.selected_config().cloned() else {
            self.server_form.id = None;
            return;
        };
        if self.server_form.id == Some(server.id) && self.server_form.dirty {
            return;
        }
        self.server_form.id = Some(server.id);
        self.server_form.fields = vec![
            TextInput::new(&server.name),
            TextInput::new(&server.java_path),
            TextInput::new(server.min_memory_mb.to_string()),
            TextInput::new(server.max_memory_mb.to_string()),
            TextInput::new(&server.additional_jvm_args),
            TextInput::new(server.world_dir.to_string_lossy()),
            TextInput::new(server.backup_dir.to_string_lossy()),
        ];
        self.server_form.dirty = false;
    }

    pub(super) fn selected_server(&self) -> Option<&ServerSnapshot> {
        self.snapshots.get(self.sidebar_index)
    }

    pub(super) fn selected_config(&self) -> Option<&ServerConfig> {
        self.config.servers.get(self.sidebar_index)
    }

    pub(super) fn selected_id(&self) -> Option<ServerId> {
        self.selected_server().map(|server| server.id)
    }

    pub(super) fn add_selected(&self) -> bool {
        self.sidebar_index == self.snapshots.len()
    }

    pub(super) fn global_selected(&self) -> bool {
        self.sidebar_index == self.snapshots.len() + 1
    }

    pub(super) fn text_input_active(&self) -> bool {
        if self.modal.is_some() || self.wizard.is_some() {
            return true;
        }
        if self.global_selected() || self.tab == ServerTab::Settings {
            return self.focus == Focus::Content;
        }
        let Some(id) = self.selected_id() else {
            return false;
        };
        match self.tab {
            ServerTab::Console => self
                .consoles
                .get(&id)
                .is_some_and(|console| console.command_active),
            ServerTab::Properties => self
                .properties
                .get(&id)
                .is_some_and(|properties| properties.editing.is_some()),
            ServerTab::Files => self
                .files
                .get(&id)
                .is_some_and(|files| files.editor.is_some()),
            _ => false,
        }
    }

    pub(super) fn handle_key(&mut self, key: KeyEvent) -> Vec<Command> {
        let mut commands = Vec::new();
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return commands;
        }
        if self.active_progress_job.is_some() && matches!(key.code, KeyCode::Esc | KeyCode::F(9)) {
            self.active_progress_job = None;
            return commands;
        }
        if self.modal.is_some() {
            self.handle_modal(key, &mut commands);
            return commands;
        }
        if self.wizard.is_some() {
            self.handle_wizard(key, &mut commands);
            return commands;
        }
        if self.focus == Focus::Sidebar {
            self.handle_sidebar(key);
            return commands;
        }
        if self.global_selected() {
            self.handle_global(key, &mut commands);
            return commands;
        }
        let Some(id) = self.selected_id() else {
            if matches!(key.code, KeyCode::Esc | KeyCode::Tab | KeyCode::Left) {
                self.focus = Focus::Sidebar;
            }
            return commands;
        };
        if self.handle_form_shortcuts(key, id, &mut commands) {
            return commands;
        }
        match self.tab {
            ServerTab::Overview => self.handle_overview(key, id, &mut commands),
            ServerTab::Console => self.handle_console(key, id, &mut commands),
            ServerTab::Players => self.handle_players(key, id, &mut commands),
            ServerTab::Properties => self.handle_properties(key, id, &mut commands),
            ServerTab::Backups => self.handle_backups(key, id, &mut commands),
            ServerTab::Files => self.handle_files(key, id, &mut commands),
            ServerTab::Settings => self.handle_server_settings(key, id, &mut commands),
        }
        commands
    }

    fn handle_sidebar(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                self.sidebar_index = self.sidebar_index.saturating_sub(1);
                self.selection_changed();
            }
            KeyCode::Down => {
                self.sidebar_index = (self.sidebar_index + 1).min(self.snapshots.len() + 1);
                self.selection_changed();
            }
            KeyCode::Home => {
                self.sidebar_index = 0;
                self.selection_changed();
            }
            KeyCode::End => {
                self.sidebar_index = self.snapshots.len() + 1;
                self.selection_changed();
            }
            KeyCode::Right | KeyCode::Tab | KeyCode::Enter => {
                if self.add_selected() {
                    self.open_wizard();
                } else {
                    self.focus = Focus::Content;
                    self.sync_server_form();
                }
            }
            KeyCode::Char('a') => self.open_wizard(),
            KeyCode::Char('g') => {
                self.sidebar_index = self.snapshots.len() + 1;
                self.wizard = None;
                self.focus = Focus::Content;
            }
            KeyCode::Esc | KeyCode::Char('q') => self.quit = true,
            _ => {}
        }
    }

    fn selection_changed(&mut self) {
        self.wizard = None;
        self.server_form.id = None;
        self.sync_server_form();
    }

    fn handle_form_shortcuts(
        &mut self,
        key: KeyEvent,
        id: ServerId,
        commands: &mut [Command],
    ) -> bool {
        if self.text_input_active()
            && !(self.tab == ServerTab::Console
                && !self
                    .consoles
                    .get(&id)
                    .is_some_and(|console| console.command_active))
        {
            return false;
        }
        match key.code {
            KeyCode::Left => {
                self.change_tab(-1, commands);
                true
            }
            KeyCode::Right => {
                self.change_tab(1, commands);
                true
            }
            KeyCode::Char(character @ '1'..='7') => {
                self.tab = SERVER_TABS[(character as usize) - ('1' as usize)];
                true
            }
            KeyCode::Tab | KeyCode::Esc => {
                self.focus = Focus::Sidebar;
                true
            }
            KeyCode::Char('q') => {
                self.quit = true;
                true
            }
            KeyCode::F(9) => {
                if let Some((job, _)) = self.jobs.last_key_value() {
                    self.active_progress_job = Some(*job);
                } else {
                    self.notice("当前没有后台任务", false);
                }
                true
            }
            _ => false,
        }
    }

    fn change_tab(&mut self, delta: isize, _commands: &mut [Command]) {
        let current = self.tab.index() as isize;
        let next = (current + delta).clamp(0, SERVER_TABS.len() as isize - 1);
        self.tab = SERVER_TABS[next as usize];
    }

    /// 事件循环每轮调用：由面板实现决定是否需要补发加载命令。
    pub(super) fn ensure_current_panel(&mut self) -> Vec<Command> {
        panels::ensure_current_panel(self)
    }

    fn issue_lifecycle(
        &mut self,
        id: ServerId,
        action: LifecycleAction,
        label: &str,
        commands: &mut Vec<Command>,
    ) -> JobId {
        self.issue(
            CommandKind::Lifecycle { id, action },
            label.to_owned(),
            commands,
        )
    }

    fn issue(&mut self, kind: CommandKind, label: String, commands: &mut Vec<Command>) -> JobId {
        let id = self.next_job;
        self.next_job += 1;
        self.jobs.insert(
            id,
            JobView {
                label: label.clone(),
                log: VecDeque::new(),
            },
        );
        commands.push(Command { id, kind });
        id
    }

    /// 先由 messages 清理终态任务，再做穷尽分派；调用方无需关心顺序。
    pub(super) fn apply_message(&mut self, message: Message) {
        messages::apply(self, message);
    }

    fn server_stopped(&self, id: ServerId) -> bool {
        self.snapshots
            .iter()
            .find(|server| server.id == id)
            .is_some_and(|server| server.state == ServerState::Stopped)
    }

    fn notice(&mut self, text: impl Into<String>, is_error: bool) {
        self.notice = Notice {
            text: text.into(),
            is_error,
        };
    }
}

#[cfg(test)]
mod tests;
