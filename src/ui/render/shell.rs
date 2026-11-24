use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::core::ServerState;

use super::super::state::{App, Focus, ServerTab};
use super::{
    modal::render_modal,
    panels::{render_global, render_server},
    widgets::{ERROR, MUTED, SELECTED, SUCCESS, WARNING, centered},
    wizard::render_wizard,
};

pub(super) fn render(frame: &mut Frame, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(frame.area());
    let title = format!(
        " MCServerManager  |  后台任务 {}  |  退出仅 detach，不停止 Minecraft",
        app.jobs.len()
    );
    frame.render_widget(
        Paragraph::new(title).style(Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        outer[0],
    );

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(30), Constraint::Min(20)])
        .split(outer[1]);
    render_sidebar(frame, app, body[0]);
    if let Some(wizard) = &app.wizard {
        render_wizard(frame, app, wizard, body[1]);
    } else if app.global_selected() {
        render_global(frame, app, body[1]);
    } else if app.selected_server().is_some() {
        render_server(frame, app, body[1]);
    } else {
        frame.render_widget(
            Paragraph::new("暂无服务器。选择左侧「添加服务器」开始。")
                .block(Block::default().title("欢迎").borders(Borders::ALL))
                .wrap(Wrap { trim: false }),
            body[1],
        );
    }

    let notice_style = if app.notice.is_error { ERROR } else { MUTED };
    frame.render_widget(
        Paragraph::new(app.notice.text.as_str())
            .style(notice_style)
            .wrap(Wrap { trim: false }),
        outer[2],
    );
    frame.render_widget(
        Paragraph::new(help_text(app)).style(MUTED),
        Rect {
            y: outer[2].y.saturating_add(1),
            height: outer[2].height.saturating_sub(1),
            ..outer[2]
        },
    );

    if let Some(modal) = &app.modal {
        render_modal(frame, modal, frame.area());
    }
    if let Some(job) = app.active_progress_job
        && let Some(view) = app.jobs.get(&job)
    {
        let area = centered(frame.area(), 76, 14);
        let lines: Vec<Line> = view
            .log
            .iter()
            .rev()
            .take(area.height.saturating_sub(5) as usize)
            .rev()
            .map(|line| Line::from(line.as_str()))
            .collect();
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::default()
                        .title(format!("后台进度：{}（Esc 关闭，任务继续）", view.label))
                        .borders(Borders::ALL)
                        .border_style(WARNING),
                )
                .wrap(Wrap { trim: false }),
            area,
        );
    }
}

fn render_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    let mut items: Vec<ListItem> = app
        .snapshots
        .iter()
        .map(|server| {
            let marker = match server.state {
                ServerState::Running => "●",
                ServerState::Starting | ServerState::Stopping => "◐",
                ServerState::Stopped => "○",
                ServerState::Unknown => "?",
            };
            let style = match server.state {
                ServerState::Running => SUCCESS,
                ServerState::Starting | ServerState::Stopping => WARNING,
                _ => Style::new(),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{marker} "), style),
                Span::raw(server.name.clone()),
                Span::styled(format!("  {}", server.state.as_str()), MUTED),
            ]))
        })
        .collect();
    items.push(ListItem::new("＋ 添加服务器"));
    items.push(ListItem::new("⚙ 全局设置"));
    let mut state = ListState::default().with_selected(Some(app.sidebar_index));
    let list = List::new(items)
        .block(
            Block::default()
                .title("服务器 / 导航")
                .borders(Borders::ALL)
                .border_style(if app.focus == Focus::Sidebar {
                    SELECTED
                } else {
                    Style::new()
                }),
        )
        .highlight_style(SELECTED)
        .highlight_symbol("▶ ");
    frame.render_stateful_widget(list, area, &mut state);
}

fn help_text(app: &App) -> &'static str {
    if app.modal.is_some() {
        "弹窗捕获按键；危险操作默认取消"
    } else if app.wizard.is_some() {
        "向导：Enter 下一步/开始，Esc 返回；后台任务不阻塞界面"
    } else if app.focus == Focus::Sidebar {
        "↑/↓ 导航  Enter/→ 打开  a 添加  g 全局设置  q/Esc 退出（仅 detach）"
    } else if app.global_selected() {
        "编辑全局设置  Ctrl+S 保存  Esc 返回"
    } else {
        match app.tab {
            ServerTab::Overview => "s 启动  t 停止  r 重启  ←/→ 标签  Esc 侧栏  F9 后台任务",
            ServerTab::Console => "↑/↓ 滚动  PgUp/PgDn 翻页  Home/End  i 输入  f 跟随  c 清屏",
            ServerTab::Players => "↑/↓ 选择  k 踢出  b 封禁确认  Esc 侧栏",
            ServerTab::Properties => "Enter 编辑值  Ctrl+S 保存  r 重载  n 缺失时新建默认项",
            ServerTab::Backups => "c 创建  Enter 恢复确认  d 删除确认  r 刷新",
            ServerTab::Files => "Enter 打开  n/N 新建  F2 改名  d 删除确认  Backspace 上级",
            ServerTab::Settings => "Ctrl+S 保存  F2 move  F3 relocate  Delete 移除  F8 删除文件",
        }
    }
}
