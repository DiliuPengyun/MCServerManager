use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};

use super::super::tasks::{Command, CommandKind, LifecycleAction, SimpleCommand};
use super::{App, ConfirmAction, ConfirmState, InputAction, InputPrompt, Modal};

impl App {
    pub(super) fn handle_modal(&mut self, key: KeyEvent, commands: &mut Vec<Command>) {
        let Some(modal) = self.modal.take() else {
            return;
        };
        match modal {
            Modal::Confirm(mut confirm) => match key.code {
                KeyCode::Esc => {}
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                    confirm.confirm_selected = !confirm.confirm_selected;
                    self.modal = Some(Modal::Confirm(confirm));
                }
                KeyCode::Enter if !confirm.confirm_selected => {}
                KeyCode::Enter => self.execute_confirmed(confirm.action, commands),
                _ => self.modal = Some(Modal::Confirm(confirm)),
            },
            Modal::Input(mut prompt) => match key.code {
                KeyCode::Esc => {}
                KeyCode::Enter => self.submit_prompt(prompt, commands),
                _ => {
                    prompt.input.handle_key(key);
                    self.modal = Some(Modal::Input(prompt));
                }
            },
            Modal::Rcon(mut rcon) => match key.code {
                KeyCode::Esc => {}
                KeyCode::Left | KeyCode::BackTab => {
                    rcon.selected = rcon.selected.saturating_sub(1);
                    self.modal = Some(Modal::Rcon(rcon));
                }
                KeyCode::Right | KeyCode::Tab => {
                    rcon.selected = (rcon.selected + 1).min(2);
                    self.modal = Some(Modal::Rcon(rcon));
                }
                KeyCode::Enter => match rcon.selected {
                    1 => {
                        self.issue_lifecycle(
                            rcon.id,
                            LifecycleAction::EnableRcon {
                                restart: rcon.restart,
                            },
                            "启用 RCON 并启动",
                            commands,
                        );
                    }
                    2 => {
                        self.modal = Some(Modal::Confirm(ConfirmState {
                            title: "确认本次不启用 RCON".to_owned(),
                            lines: vec!["服务器将直接启动，玩家管理页保持不可用。".to_owned()],
                            confirm_label: "直接启动".to_owned(),
                            action: ConfirmAction::StartWithoutRcon {
                                id: rcon.id,
                                restart: rcon.restart,
                            },
                            confirm_selected: false,
                        }))
                    }
                    _ => {}
                },
                _ => self.modal = Some(Modal::Rcon(rcon)),
            },
        }
    }

    fn submit_prompt(&mut self, prompt: InputPrompt, commands: &mut Vec<Command>) {
        let value = prompt.input.value();
        if value.trim().is_empty() {
            self.notice("输入不能为空", true);
            self.modal = Some(Modal::Input(prompt));
            return;
        }
        match prompt.action {
            InputAction::CreateFile { id, is_dir } => {
                let Some(files) = self.files.get(&id) else {
                    return;
                };
                let path = files.current_dir.join(&value);
                self.issue(
                    CommandKind::FileCreate { id, path, is_dir },
                    if is_dir {
                        "新建目录"
                    } else {
                        "新建文件"
                    }
                    .to_owned(),
                    commands,
                );
            }
            InputAction::RenameFile { id, path } => {
                self.issue(
                    CommandKind::FileRename {
                        id,
                        path,
                        new_name: value,
                    },
                    "重命名文件条目".to_owned(),
                    commands,
                );
            }
            InputAction::ChangeLocation { id, relocate } => {
                let destination = PathBuf::from(&value);
                if !destination.is_absolute() {
                    self.notice("目标必须是绝对路径", true);
                    self.modal = Some(Modal::Input(prompt));
                    return;
                }
                self.modal = Some(Modal::Confirm(ConfirmState {
                    title: if relocate {
                        "确认重新定位服务器"
                    } else {
                        "确认由管理器移动服务器"
                    }
                    .to_owned(),
                    lines: vec![
                        format!("目标：{}", destination.display()),
                        if relocate {
                            "只修改配置指向，不复制或删除任何文件。"
                        } else {
                            "将移动整个服务器目录；目标存在时不会覆盖。"
                        }
                        .to_owned(),
                        "操作前必须停止服务器，且不能存在对应 tmux 会话。".to_owned(),
                    ],
                    confirm_label: if relocate {
                        "确认重新定位"
                    } else {
                        "确认移动"
                    }
                    .to_owned(),
                    action: ConfirmAction::MoveServer {
                        id,
                        destination,
                        relocate,
                    },
                    confirm_selected: false,
                }));
            }
        }
    }

    fn execute_confirmed(&mut self, action: ConfirmAction, commands: &mut Vec<Command>) {
        match action {
            ConfirmAction::AgreeEula { id, restart } => {
                self.issue_lifecycle(
                    id,
                    LifecycleAction::AgreeEula { restart },
                    "同意 EULA 并启动",
                    commands,
                );
            }
            ConfirmAction::RunInstaller { id, restart } => {
                let job = self.issue_lifecycle(
                    id,
                    LifecycleAction::RunInstaller { restart },
                    "运行安装器",
                    commands,
                );
                self.active_progress_job = Some(job);
            }
            ConfirmAction::StartWithoutRcon { id, restart } => {
                self.issue_lifecycle(
                    id,
                    LifecycleAction::Start {
                        restart,
                        allow_no_rcon: true,
                    },
                    "不启用 RCON 直接启动",
                    commands,
                );
            }
            ConfirmAction::RestoreBackup { id, name } => {
                self.issue(
                    CommandKind::BackupRestore { id, name },
                    "恢复世界备份".to_owned(),
                    commands,
                );
            }
            ConfirmAction::DeleteBackup { id, name } => {
                self.issue(
                    CommandKind::BackupDelete { id, name },
                    "删除备份".to_owned(),
                    commands,
                );
            }
            ConfirmAction::DeleteFile { id, path, .. } => {
                self.issue(
                    CommandKind::FileDelete { id, path },
                    "删除文件条目".to_owned(),
                    commands,
                );
            }
            ConfirmAction::BanPlayer { id, player } => {
                self.issue(
                    CommandKind::Simple(SimpleCommand::BanPlayer { id, player }),
                    "封禁玩家".to_owned(),
                    commands,
                );
            }
            ConfirmAction::MoveServer {
                id,
                destination,
                relocate,
            } => {
                self.issue(
                    CommandKind::Simple(SimpleCommand::MoveServer {
                        id,
                        destination,
                        relocate,
                    }),
                    if relocate {
                        "重新定位服务器"
                    } else {
                        "移动服务器目录"
                    }
                    .to_owned(),
                    commands,
                );
            }
            ConfirmAction::RemoveServer { id, delete_files } => {
                self.issue(
                    CommandKind::Simple(SimpleCommand::RemoveServer { id, delete_files }),
                    "删除服务器".to_owned(),
                    commands,
                );
            }
            ConfirmAction::CloseFileEditor { id } => {
                if let Some(files) = self.files.get_mut(&id) {
                    files.editor = None;
                }
            }
            ConfirmAction::RunWizardInstaller => self.start_validated_wizard(commands),
        }
    }
}
