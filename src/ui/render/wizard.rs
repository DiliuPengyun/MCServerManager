use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use super::super::state::{AUTO_CORES, App, WizardState, WizardStep};
use super::super::tasks::WizardMethod;
use super::widgets::{ERROR, SELECTED, SUCCESS, WARNING, form_line, set_input_cursor};

pub(super) fn render_wizard(frame: &mut Frame, app: &App, wizard: &WizardState, area: Rect) {
    let block = Block::default()
        .title(format!("添加服务器 · {}", wizard.method().title()))
        .borders(Borders::ALL)
        .border_style(SELECTED);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    match wizard.step {
        WizardStep::Method => {
            let items: Vec<ListItem> = WizardMethod::ALL
                .iter()
                .map(|method| ListItem::new(method.title()))
                .collect();
            let mut state = ListState::default().with_selected(Some(wizard.method_index));
            frame.render_stateful_widget(
                List::new(items)
                    .block(
                        Block::default()
                            .title("选择方式（Enter 下一步，Esc 返回）")
                            .borders(Borders::ALL),
                    )
                    .highlight_style(SELECTED)
                    .highlight_symbol("▶ "),
                inner,
                &mut state,
            );
        }
        WizardStep::Core => {
            let items: Vec<ListItem> = AUTO_CORES
                .iter()
                .map(|core| ListItem::new(core.display_name()))
                .collect();
            let mut state = ListState::default().with_selected(Some(wizard.core_index));
            frame.render_stateful_widget(
                List::new(items)
                    .block(
                        Block::default()
                            .title("选择自动安装核心（Spigot 暂无安装 API）")
                            .borders(Borders::ALL),
                    )
                    .highlight_style(SELECTED)
                    .highlight_symbol("▶ "),
                inner,
                &mut state,
            );
        }
        WizardStep::NeoMcVersion => {
            frame.render_widget(
                Paragraph::new(format!(
                    "输入 NeoForge 对应的 Minecraft 版本：\n\n> {}\n\nEnter 后台查询核心版本，Esc 返回",
                    wizard.neo_mc_input.value()
                )),
                inner,
            );
            set_input_cursor(
                frame,
                Rect {
                    x: inner.x + 2,
                    y: inner.y + 2,
                    width: inner.width.saturating_sub(2),
                    height: 1,
                },
                &wizard.neo_mc_input,
            );
        }
        WizardStep::Versions => {
            if wizard.versions_loading {
                frame.render_widget(Paragraph::new("正在后台查询官方版本…"), inner);
            } else if let Some(error) = &wizard.versions_error {
                frame.render_widget(
                    Paragraph::new(format!("查询失败：{error}\n按 r 重试，Esc 返回"))
                        .style(ERROR)
                        .wrap(Wrap { trim: false }),
                    inner,
                );
            } else {
                let items: Vec<ListItem> = wizard
                    .versions
                    .iter()
                    .map(|version| ListItem::new(version.label.as_str()))
                    .collect();
                let mut state = ListState::default().with_selected(Some(wizard.version_selected));
                frame.render_stateful_widget(
                    List::new(items)
                        .block(
                            Block::default()
                                .title("选择版本（Enter 下一步，r 重试）")
                                .borders(Borders::ALL),
                        )
                        .highlight_style(SELECTED)
                        .highlight_symbol("▶ "),
                    inner,
                    &mut state,
                );
            }
        }
        WizardStep::Configure => {
            let mut lines = vec![
                Line::from(format!(
                    "方式：{}  默认目录：{}",
                    wizard.method().title(),
                    app.config.default_server_dir.display()
                )),
                Line::from(""),
            ];
            for (index, (name, input)) in WizardState::FIELD_NAMES
                .iter()
                .zip(wizard.fields.iter())
                .enumerate()
            {
                if index == 1 && wizard.method() == WizardMethod::Auto {
                    continue;
                }
                // 导入主机目录时工作目录固定为源目录，目标目录一栏不生效。
                let mut label = *name;
                if index == 2 && wizard.method() == WizardMethod::Import {
                    label = "目标目录（导入主机目录时固定为源目录，此处不生效）";
                }
                lines.push(form_line(wizard.field_selected == index, label, input));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(
                "↑/↓/Tab 切换；Enter/Ctrl+S 开始；已有安装器还会再次确认",
            ));
            if let Some(error) = &wizard.error {
                lines.push(Line::from(Span::styled(error.clone(), ERROR)));
            }
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        WizardStep::Installing => {
            let count = inner.height.saturating_sub(3) as usize;
            let mut lines = vec![Line::from(Span::styled(
                format!("正在后台{}，请查看进度：", wizard.method().action_name()),
                WARNING,
            ))];
            lines.extend(
                wizard
                    .progress
                    .iter()
                    .rev()
                    .take(count)
                    .rev()
                    .map(|line| Line::from(line.as_str())),
            );
            lines.push(Line::from("Esc 离开向导（任务继续）；F9 可重新查看任务"));
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        WizardStep::Done => {
            let text = if let Some(name) = &wizard.created_name {
                vec![
                    Line::from(Span::styled(
                        format!(
                            "服务器「{name}」已{}并注册。",
                            wizard.method().action_name()
                        ),
                        SUCCESS,
                    )),
                    Line::from("Enter 完成。首次启动时会分别请求 EULA / RCON 授权。"),
                ]
            } else {
                vec![
                    Line::from(Span::styled(
                        format!("{}或注册失败", wizard.method().action_name()),
                        ERROR,
                    )),
                    Line::from(wizard.error.clone().unwrap_or_default()),
                    Line::from("Esc 返回配置；不会静默同意 EULA 或启用 RCON。"),
                ]
            };
            frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), inner);
        }
    }
}
