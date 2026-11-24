use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::core::ServerState;

use super::super::state::{App, FilesState, Focus, SERVER_TABS, ServerForm, ServerTab};
use super::widgets::{
    MUTED, SELECTED, WARNING, empty_as, form_line, path_or_empty, set_input_cursor, state_style,
};

pub(super) fn render_server(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);
    let titles = SERVER_TABS
        .iter()
        .enumerate()
        .map(|(index, tab)| {
            let style = if *tab == app.tab { SELECTED } else { MUTED };
            Span::styled(format!(" {}:{} ", index + 1, tab.title()), style)
        })
        .collect::<Vec<_>>();
    let name = app
        .selected_server()
        .map(|server| server.name.as_str())
        .unwrap_or("?");
    frame.render_widget(
        Paragraph::new(Line::from(titles))
            .block(
                Block::default()
                    .title(format!("{name}（←/→ 或数字切换）"))
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        chunks[0],
    );
    let content = Block::default()
        .title(app.tab.title())
        .borders(Borders::ALL)
        .border_style(if app.focus == Focus::Content {
            SELECTED
        } else {
            Style::new()
        });
    let inner = content.inner(chunks[1]);
    frame.render_widget(content, chunks[1]);
    match app.tab {
        ServerTab::Overview => render_overview(frame, app, inner),
        ServerTab::Console => render_console(frame, app, inner),
        ServerTab::Players => render_players(frame, app, inner),
        ServerTab::Properties => render_properties(frame, app, inner),
        ServerTab::Backups => render_backups(frame, app, inner),
        ServerTab::Files => render_files(frame, app, inner),
        ServerTab::Settings => render_server_settings(frame, app, inner),
    }
}

fn render_overview(frame: &mut Frame, app: &App, area: Rect) {
    let Some(snapshot) = app.selected_server() else {
        return;
    };
    let Some(server) = app.selected_config() else {
        return;
    };
    let console_lines = app
        .consoles
        .get(&snapshot.id)
        .map_or(0, |console| console.lines.len());
    let lines = vec![
        Line::from(vec![
            Span::raw("状态："),
            Span::styled(snapshot.state.as_str(), state_style(snapshot.state)),
            Span::raw(format!("    在线玩家：{}", snapshot.players.len())),
            Span::raw(format!("    控制台：{console_lines} 行")),
        ]),
        Line::from(""),
        Line::from(format!("核心：{}", server.core_type.display_name())),
        Line::from(format!(
            "Minecraft 版本：{}",
            empty_as(&server.minecraft_version, "未知")
        )),
        Line::from(format!(
            "核心版本：{}",
            empty_as(&server.core_version, "未知")
        )),
        Line::from(format!("JAR：{}", path_or_empty(&server.server_jar_path))),
        Line::from(format!(
            "启动参数：{}",
            path_or_empty(&server.launch_args_file)
        )),
        Line::from(format!("Java：{}", app.config.effective_java_path(server))),
        Line::from(format!(
            "内存：{} MiB - {} MiB",
            server.min_memory_mb, server.max_memory_mb
        )),
        Line::from(format!("working_dir：{}", server.working_dir.display())),
        Line::from(format!("世界：{}", server.world_dir().display())),
        Line::from(format!("备份：{}", server.backup_dir().display())),
        Line::from(format!(
            "RCON：{}",
            if snapshot.rcon_available {
                "已配置"
            } else {
                "未配置"
            }
        )),
        Line::from(""),
        Line::from(Span::styled(
            "s 启动   t 停止   r 重启；安装器 → EULA → RCON 会依次明确确认",
            WARNING,
        )),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn render_console(frame: &mut Frame, app: &App, area: Rect) {
    let Some(id) = app.selected_id() else {
        return;
    };
    let console = app.consoles.get(&id);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);
    let height = chunks[0].height.saturating_sub(2) as usize;
    let (lines, label) = if let Some(console) = console {
        let start = console
            .lines
            .len()
            .saturating_sub(height.saturating_add(console.scroll_from_end));
        let end = (start + height).min(console.lines.len());
        (
            console.lines[start..end]
                .iter()
                .map(|line| Line::from(line.as_str()))
                .collect::<Vec<_>>(),
            format!(
                "日志 {}/{}  从末尾回退 {} 行{}",
                end,
                console.lines.len(),
                console.scroll_from_end,
                if console.follow { "  自动跟随" } else { "" }
            ),
        )
    } else {
        (vec![Line::from("暂无控制台输出")], "日志".to_owned())
    };
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().title(label).borders(Borders::ALL)),
        chunks[0],
    );
    let active = console.is_some_and(|console| console.command_active);
    let command = console
        .map(|console| console.command.value())
        .unwrap_or_default();
    let command_block = Block::default()
        .title(if active {
            "指令（Enter 发送，Esc 返回滚动）"
        } else {
            "按 i 或 Enter 输入指令"
        })
        .borders(Borders::ALL)
        .border_style(if active { SELECTED } else { MUTED });
    let inner = command_block.inner(chunks[1]);
    frame.render_widget(command_block, chunks[1]);
    frame.render_widget(Paragraph::new(command), inner);
    if active && let Some(console) = console {
        set_input_cursor(frame, inner, &console.command);
    }
}

fn render_players(frame: &mut Frame, app: &App, area: Rect) {
    let Some(snapshot) = app.selected_server() else {
        return;
    };
    if !snapshot.rcon_available {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from("玩家管理不可用"),
                Line::from(""),
                Line::from("该服务器未启用 RCON，无法获取在线玩家。"),
                Line::from("下次启动时明确选择「启用 RCON」后才可使用本页。"),
            ])
            .style(MUTED)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let selected = app
        .players
        .get(&snapshot.id)
        .map_or(0, |state| state.selected);
    let items: Vec<ListItem> = snapshot
        .players
        .iter()
        .map(|player| ListItem::new(format!("{player}  （在线）")))
        .collect();
    let mut state = ListState::default().with_selected((!items.is_empty()).then_some(selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(if snapshot.state == ServerState::Running {
                        "在线玩家（k 踢出，b 封禁需确认）"
                    } else {
                        "服务器未运行"
                    })
                    .borders(Borders::ALL),
            )
            .highlight_style(SELECTED)
            .highlight_symbol("▶ "),
        area,
        &mut state,
    );
}

fn render_properties(frame: &mut Frame, app: &App, area: Rect) {
    let Some(id) = app.selected_id() else {
        return;
    };
    let Some(properties) = app.properties.get(&id) else {
        return;
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(area);
    if properties.loading {
        frame.render_widget(Paragraph::new("正在后台加载…"), chunks[0]);
    } else if properties.entries.is_empty() {
        frame.render_widget(
            Paragraph::new(if properties.exists {
                "文件中没有属性。按 r 重新加载。"
            } else {
                "server.properties 尚不存在。按 n 建立内存默认项，Ctrl+S 才会写盘。"
            })
            .wrap(Wrap { trim: false }),
            chunks[0],
        );
    } else {
        let items: Vec<ListItem> = properties
            .entries
            .iter()
            .map(|(key, value)| ListItem::new(format!("{key} = {value}")))
            .collect();
        let mut state = ListState::default().with_selected(Some(properties.selected));
        frame.render_stateful_widget(
            List::new(items)
                .block(
                    Block::default()
                        .title("属性（Enter 编辑，r 重载，Ctrl+S 保存）")
                        .borders(Borders::ALL),
                )
                .highlight_style(SELECTED)
                .highlight_symbol("▶ "),
            chunks[0],
            &mut state,
        );
    }
    let editing = properties.editing.as_ref();
    let block = Block::default()
        .title(if editing.is_some() {
            "编辑值（Enter 应用，Esc 取消）"
        } else {
            "修改尚未写盘；保存会保留原文件注释和顺序"
        })
        .borders(Borders::ALL)
        .border_style(if editing.is_some() { SELECTED } else { MUTED });
    let inner = block.inner(chunks[1]);
    frame.render_widget(block, chunks[1]);
    if let Some((index, input)) = editing {
        let key = properties
            .entries
            .get(*index)
            .map(|entry| entry.0.as_str())
            .unwrap_or("?");
        frame.render_widget(Paragraph::new(format!("{key} = {}", input.value())), inner);
    } else if properties.dirty {
        frame.render_widget(Paragraph::new("有未保存修改").style(WARNING), inner);
    }
}

fn render_backups(frame: &mut Frame, app: &App, area: Rect) {
    let Some(id) = app.selected_id() else {
        return;
    };
    let Some(backups) = app.backups.get(&id) else {
        return;
    };
    let server = app.selected_config();
    let lines = vec![
        Line::from(format!(
            "世界：{}",
            server
                .map(|server| server.world_dir().display().to_string())
                .unwrap_or_default()
        )),
        Line::from(format!(
            "备份：{}",
            server
                .map(|server| server.backup_dir().display().to_string())
                .unwrap_or_default()
        )),
        Line::from(Span::styled(
            "c 创建（需停止）  Enter 恢复（需停止且确认）  d 删除（确认）  r 刷新",
            WARNING,
        )),
        Line::from(""),
    ];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(1)])
        .split(area);
    frame.render_widget(Paragraph::new(lines), chunks[0]);
    let items: Vec<ListItem> = backups
        .items
        .iter()
        .map(|name| ListItem::new(name.as_str()))
        .collect();
    let mut state =
        ListState::default().with_selected((!items.is_empty()).then_some(backups.selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(if backups.loading {
                        "正在加载…"
                    } else {
                        "已有备份"
                    })
                    .borders(Borders::ALL),
            )
            .highlight_style(SELECTED)
            .highlight_symbol("▶ "),
        chunks[1],
        &mut state,
    );
}

fn render_files(frame: &mut Frame, app: &App, area: Rect) {
    let Some(id) = app.selected_id() else {
        return;
    };
    let Some(files) = app.files.get(&id) else {
        return;
    };
    if files.editor.is_some() {
        render_file_editor(frame, files, area);
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!("路径：/{}", files.current_dir.display())),
            Line::from("Enter 打开  Backspace 上级  n 文件  N 目录  F2 改名  d 删除  r 刷新"),
        ])
        .wrap(Wrap { trim: false }),
        chunks[0],
    );
    let items: Vec<ListItem> = files
        .entries
        .iter()
        .map(|entry| {
            if entry.is_dir {
                ListItem::new(format!("[目录] {}/  {}", entry.name, entry.mod_time))
            } else {
                ListItem::new(format!(
                    "[文件] {}  {} B  {}",
                    entry.name, entry.size, entry.mod_time
                ))
            }
        })
        .collect();
    let mut state =
        ListState::default().with_selected((!items.is_empty()).then_some(files.selected));
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(if files.loading {
                        "正在后台读取…"
                    } else {
                        "目录内容"
                    })
                    .borders(Borders::ALL),
            )
            .highlight_style(SELECTED)
            .highlight_symbol("▶ "),
        chunks[1],
        &mut state,
    );
}

fn render_file_editor(frame: &mut Frame, files: &FilesState, area: Rect) {
    let Some(editor) = &files.editor else {
        return;
    };
    let block = Block::default()
        .title(format!(
            "编辑 {}{}（Ctrl+S 保存，Esc 关闭）",
            editor.path.display(),
            if editor.dirty() { " *未保存" } else { "" }
        ))
        .borders(Borders::ALL)
        .border_style(if editor.dirty() { WARNING } else { SELECTED });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let (cursor_line, cursor_column) = editor.editor.line_column();
    let top = cursor_line.saturating_sub(inner.height.saturating_sub(1) as usize);
    let visible: Vec<Line> = editor
        .editor
        .line_texts()
        .skip(top)
        .take(inner.height as usize)
        .map(Line::from)
        .collect();
    frame.render_widget(Paragraph::new(visible), inner);
    let row = cursor_line.saturating_sub(top) as u16;
    let prefix = editor.editor.line_text(cursor_line);
    let prefix: String = prefix.chars().take(cursor_column).collect();
    let width = Line::from(prefix).width() as u16;
    if row < inner.height && width < inner.width {
        frame.set_cursor_position((inner.x + width, inner.y + row));
    }
}

pub(super) fn render_global(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().title("全局设置").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = vec![
        Line::from(Span::styled(
            "默认服务器目录仅用于新建和上传，不会修改任何已有服务器的 working_dir",
            WARNING,
        )),
        Line::from(""),
        form_line(
            app.global_form.selected == 0,
            "默认服务器目录",
            &app.global_form.default_dir,
        ),
        form_line(
            app.global_form.selected == 1,
            "全局 Java 路径",
            &app.global_form.java_path,
        ),
        Line::from(""),
        Line::from("↑/↓/Tab 切换，直接键入编辑，Ctrl+S 或 F5 保存，Esc 返回侧栏"),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    if app.focus == Focus::Content {
        let input = if app.global_form.selected == 0 {
            &app.global_form.default_dir
        } else {
            &app.global_form.java_path
        };
        let y = inner.y + 2 + app.global_form.selected as u16;
        if y < inner.bottom() {
            set_input_cursor(
                frame,
                Rect {
                    x: inner.x + 22,
                    y,
                    width: inner.width.saturating_sub(22),
                    height: 1,
                },
                input,
            );
        }
    }
}

fn render_server_settings(frame: &mut Frame, app: &App, area: Rect) {
    let Some(server) = app.selected_config() else {
        return;
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(3)])
        .split(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!(
                "working_dir（任意绝对路径，只读显示）：{}",
                server.working_dir.display()
            )),
            Line::from(Span::styled(
                "F2 move：管理器移动文件并改路径；F3 relocate：手动搬迁后仅重新指向",
                WARNING,
            )),
            Line::from("Delete 仅移除注册（确认）；F8 删除注册和全部文件（明确确认）"),
        ])
        .wrap(Wrap { trim: false }),
        chunks[0],
    );
    let mut lines = Vec::new();
    for (index, (name, input)) in ServerForm::FIELD_NAMES
        .iter()
        .zip(app.server_form.fields.iter())
        .enumerate()
    {
        lines.push(form_line(app.server_form.selected == index, name, input));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("↑/↓/Tab 切换，Ctrl+S/F5 保存。"));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), chunks[1]);
}
