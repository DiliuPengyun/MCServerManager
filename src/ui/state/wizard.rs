use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::CoreType;

use super::super::tasks::{Command, CommandKind, WizardMethod, WizardRequest};
use super::{App, ConfirmAction, ConfirmState, Modal, WizardState, WizardStep};

impl App {
    pub(super) fn open_wizard(&mut self) {
        let generation = self.next_generation;
        self.next_generation += 1;
        self.sidebar_index = self.snapshots.len();
        self.wizard = Some(WizardState::new(generation, &self.config));
        self.focus = super::Focus::Content;
    }

    pub(super) fn handle_wizard(&mut self, key: KeyEvent, commands: &mut Vec<Command>) {
        if key.code == KeyCode::F(9) {
            if let Some((job, _)) = self.jobs.last_key_value() {
                self.active_progress_job = Some(*job);
            }
            return;
        }
        let step = self.wizard.as_ref().map(|wizard| wizard.step);
        match step {
            Some(WizardStep::Method) => self.wizard_method(key),
            Some(WizardStep::Core) => self.wizard_core(key, commands),
            Some(WizardStep::NeoMcVersion) => self.wizard_neo_mc(key, commands),
            Some(WizardStep::Versions) => self.wizard_versions(key, commands),
            Some(WizardStep::Configure) => self.wizard_configure(key, commands),
            Some(WizardStep::Installing) => {
                if key.code == KeyCode::Esc {
                    self.wizard = None;
                    self.sidebar_index = 0;
                    self.notice(
                        "任务仍在后台执行；F9 可查看任务，完成后服务器会出现在列表中",
                        false,
                    );
                }
            }
            Some(WizardStep::Done) => {
                let success = self
                    .wizard
                    .as_ref()
                    .is_some_and(|wizard| wizard.created_name.is_some());
                match key.code {
                    KeyCode::Enter if success => {
                        self.wizard = None;
                        self.sidebar_index = self
                            .config
                            .active_server_id
                            .and_then(|id| self.config.index_of(id))
                            .unwrap_or(0);
                        self.focus = super::Focus::Content;
                    }
                    KeyCode::Esc if !success => {
                        if let Some(wizard) = &mut self.wizard {
                            wizard.step = WizardStep::Configure;
                        }
                    }
                    KeyCode::Esc => {
                        self.wizard = None;
                        self.focus = super::Focus::Sidebar;
                    }
                    _ => {}
                }
            }
            None => {}
        }
    }

    fn wizard_method(&mut self, key: KeyEvent) {
        let Some(wizard) = &mut self.wizard else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.wizard = None;
                self.focus = super::Focus::Sidebar;
            }
            KeyCode::Up => wizard.method_index = wizard.method_index.saturating_sub(1),
            KeyCode::Down => {
                wizard.method_index = (wizard.method_index + 1).min(WizardMethod::ALL.len() - 1);
            }
            KeyCode::Enter => {
                wizard.step = if wizard.method() == WizardMethod::Auto {
                    WizardStep::Core
                } else {
                    WizardStep::Configure
                };
                wizard.error = None;
            }
            _ => {}
        }
    }

    fn wizard_core(&mut self, key: KeyEvent, commands: &mut Vec<Command>) {
        let Some(wizard) = &mut self.wizard else {
            return;
        };
        match key.code {
            KeyCode::Esc => wizard.step = WizardStep::Method,
            KeyCode::Up => wizard.core_index = wizard.core_index.saturating_sub(1),
            KeyCode::Down => {
                wizard.core_index = (wizard.core_index + 1).min(super::AUTO_CORES.len() - 1)
            }
            KeyCode::Enter => {
                if wizard.core_type() == CoreType::Neoforge {
                    wizard.step = WizardStep::NeoMcVersion;
                } else {
                    wizard.step = WizardStep::Versions;
                    self.load_wizard_versions(commands);
                }
            }
            _ => {}
        }
    }

    fn wizard_neo_mc(&mut self, key: KeyEvent, commands: &mut Vec<Command>) {
        match key.code {
            KeyCode::Esc => {
                if let Some(wizard) = &mut self.wizard {
                    wizard.step = WizardStep::Core;
                }
            }
            KeyCode::Enter => {
                if let Some(wizard) = &mut self.wizard {
                    wizard.step = WizardStep::Versions;
                }
                self.load_wizard_versions(commands);
            }
            _ => {
                if let Some(wizard) = &mut self.wizard {
                    wizard.neo_mc_input.handle_key(key);
                }
            }
        }
    }

    fn load_wizard_versions(&mut self, commands: &mut Vec<Command>) {
        let Some(wizard) = &mut self.wizard else {
            return;
        };
        let generation = wizard.generation;
        let core_type = wizard.core_type();
        let mc_version = wizard.neo_mc_input.value();
        wizard.versions_loading = true;
        wizard.versions_error = None;
        wizard.versions.clear();
        wizard.version_selected = 0;
        self.issue(
            CommandKind::LoadVersions {
                generation,
                core_type,
                mc_version,
            },
            "查询官方核心版本".to_owned(),
            commands,
        );
    }

    fn wizard_versions(&mut self, key: KeyEvent, commands: &mut Vec<Command>) {
        let Some(wizard) = &mut self.wizard else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                wizard.step = if wizard.core_type() == CoreType::Neoforge {
                    WizardStep::NeoMcVersion
                } else {
                    WizardStep::Core
                };
            }
            KeyCode::Up => wizard.version_selected = wizard.version_selected.saturating_sub(1),
            KeyCode::Down => {
                wizard.version_selected =
                    (wizard.version_selected + 1).min(wizard.versions.len().saturating_sub(1));
            }
            KeyCode::PageUp => {
                wizard.version_selected = wizard.version_selected.saturating_sub(10);
            }
            KeyCode::PageDown => {
                wizard.version_selected =
                    (wizard.version_selected + 10).min(wizard.versions.len().saturating_sub(1));
            }
            KeyCode::Char('r') => self.load_wizard_versions(commands),
            KeyCode::Enter => {
                if let Some(choice) = wizard.versions.get(wizard.version_selected).cloned() {
                    wizard.selected_version = Some(choice);
                    wizard.step = WizardStep::Configure;
                    wizard.error = None;
                } else if !wizard.versions_loading {
                    wizard.error = Some("没有可用版本，请返回或按 r 重试".to_owned());
                }
            }
            _ => {}
        }
    }

    fn wizard_configure(&mut self, key: KeyEvent, commands: &mut Vec<Command>) {
        if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL)
            || key.code == KeyCode::F(5)
            || key.code == KeyCode::Enter
        {
            self.prepare_wizard_install(commands);
            return;
        }
        let Some(wizard) = &mut self.wizard else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                wizard.step = match wizard.method() {
                    WizardMethod::Auto => WizardStep::Versions,
                    _ => WizardStep::Method,
                };
            }
            KeyCode::Up | KeyCode::BackTab => {
                wizard.field_selected = wizard.field_selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Tab => {
                wizard.field_selected = (wizard.field_selected + 1).min(wizard.fields.len() - 1);
            }
            _ => {
                if let Some(field) = wizard.fields.get_mut(wizard.field_selected) {
                    field.handle_key(key);
                }
            }
        }
    }

    fn prepare_wizard_install(&mut self, commands: &mut Vec<Command>) {
        let request = match self.build_wizard_request() {
            Ok(request) => request,
            Err(error) => {
                if let Some(wizard) = &mut self.wizard {
                    wizard.error = Some(error);
                }
                return;
            }
        };
        if request.method == WizardMethod::Installer {
            if let Some(wizard) = &mut self.wizard {
                wizard.pending_request = Some(request);
            }
            self.modal = Some(Modal::Confirm(ConfirmState {
                title: "确认运行已有安装器".to_owned(),
                lines: vec![
                    "将使用配置的 Java 运行安装器，并在目标目录写入核心文件。".to_owned(),
                    "该过程可能持续数分钟；只有继续后才会调用安装器。".to_owned(),
                ],
                confirm_label: "运行安装器".to_owned(),
                action: ConfirmAction::RunWizardInstaller,
                confirm_selected: false,
            }));
        } else {
            if let Some(wizard) = &mut self.wizard {
                wizard.pending_request = Some(request);
            }
            self.start_validated_wizard(commands);
        }
    }

    pub(super) fn build_wizard_request(&mut self) -> std::result::Result<WizardRequest, String> {
        let wizard = self
            .wizard
            .as_ref()
            .ok_or_else(|| "向导已关闭".to_owned())?;
        let name = wizard.fields[0].value();
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.contains(['/', '\\'])
            || name.contains("..")
            || name.chars().any(char::is_control)
        {
            return Err("服务器名称不能为空，也不能包含 /、\\、.. 或控制字符".to_owned());
        }
        let min_memory_mb = wizard.fields[4]
            .value()
            .parse::<i32>()
            .ok()
            .filter(|value| *value >= 0)
            .ok_or_else(|| "最小内存必须是非负整数".to_owned())?;
        let max_memory_mb = wizard.fields[5]
            .value()
            .parse::<i32>()
            .ok()
            .filter(|value| *value >= 0)
            .ok_or_else(|| "最大内存必须是非负整数".to_owned())?;
        if min_memory_mb > max_memory_mb {
            return Err("最小内存不能大于最大内存".to_owned());
        }
        let method = wizard.method();
        let source_path = PathBuf::from(wizard.fields[1].value());
        if method != WizardMethod::Auto && !source_path.is_absolute() {
            return Err("源目录或 JAR 必须使用绝对路径".to_owned());
        }
        let target_input = wizard.fields[2].value();
        let target_dir = if method == WizardMethod::Import {
            source_path.clone()
        } else if target_input.trim().is_empty() {
            self.config.default_server_dir.join(&name)
        } else {
            PathBuf::from(target_input)
        };
        if !target_dir.is_absolute() {
            return Err("目标目录必须是绝对路径，或留空使用默认服务器目录".to_owned());
        }
        let selected_version = wizard.selected_version.clone();
        if method == WizardMethod::Auto && selected_version.is_none() {
            return Err("请先选择要安装的版本".to_owned());
        }
        let java_path = {
            let value = wizard.fields[3].value();
            if value.trim().is_empty() {
                "java".to_owned()
            } else {
                value
            }
        };
        Ok(WizardRequest {
            generation: wizard.generation,
            method,
            core_type: wizard.core_type(),
            name,
            source_path,
            target_dir,
            java_path,
            min_memory_mb,
            max_memory_mb,
            jvm_args: wizard.fields[6].value(),
            version: selected_version
                .as_ref()
                .map(|version| version.version.clone())
                .unwrap_or_default(),
            mc_version: selected_version
                .as_ref()
                .map(|version| version.mc_version.clone())
                .unwrap_or_default(),
        })
    }

    pub(super) fn start_validated_wizard(&mut self, commands: &mut Vec<Command>) {
        let Some(wizard) = &mut self.wizard else {
            return;
        };
        let Some(request) = wizard.pending_request.take() else {
            return;
        };
        wizard.step = WizardStep::Installing;
        wizard.progress.clear();
        wizard.error = None;
        let label = format!("{}新服务器", request.method.action_name());
        self.issue(CommandKind::WizardInstall(request), label, commands);
    }
}
