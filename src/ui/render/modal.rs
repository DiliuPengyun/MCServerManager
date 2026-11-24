use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::super::state::{ConfirmState, InputPrompt, Modal, RconState};
use super::widgets::{MUTED, SELECTED, WARNING, centered, set_input_cursor};

pub(super) fn render_modal(frame: &mut Frame, modal: &Modal, full: Rect) {
    match modal {
        Modal::Confirm(confirm) => render_confirm(frame, confirm, full),
        Modal::Input(prompt) => render_prompt(frame, prompt, full),
        Modal::Rcon(rcon) => render_rcon(frame, rcon, full),
    }
}

fn render_confirm(frame: &mut Frame, confirm: &ConfirmState, full: Rect) {
    let height = (confirm.lines.len() as u16 + 7).clamp(7, 16);
    let area = centered(full, 76, height);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(confirm.title.as_str())
        .borders(Borders::ALL)
        .border_style(WARNING);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines: Vec<Line> = confirm
        .lines
        .iter()
        .map(|line| Line::from(line.as_str()))
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            if confirm.confirm_selected {
                "  取消  "
            } else {
                "[ 取消 ]"
            },
            if confirm.confirm_selected {
                MUTED
            } else {
                SELECTED
            },
        ),
        Span::raw("    "),
        Span::styled(
            if confirm.confirm_selected {
                format!("[ {} ]", confirm.confirm_label)
            } else {
                format!("  {}  ", confirm.confirm_label)
            },
            if confirm.confirm_selected {
                WARNING
            } else {
                MUTED
            },
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "←/→ 选择，Enter 执行；默认取消，Esc 关闭",
        MUTED,
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_prompt(frame: &mut Frame, prompt: &InputPrompt, full: Rect) {
    let area = centered(full, 72, 9);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(prompt.title.as_str())
        .borders(Borders::ALL)
        .border_style(SELECTED);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(prompt.description.as_str()),
            Line::from(""),
            Line::from(format!("> {}", prompt.input.value())),
            Line::from(""),
            Line::from(Span::styled("Enter 继续，Esc 取消", MUTED)),
        ])
        .wrap(Wrap { trim: false }),
        inner,
    );
    if inner.height > 2 {
        set_input_cursor(
            frame,
            Rect {
                x: inner.x + 2,
                y: inner.y + 2,
                width: inner.width.saturating_sub(2),
                height: 1,
            },
            &prompt.input,
        );
    }
}

fn render_rcon(frame: &mut Frame, rcon: &RconState, full: Rect) {
    let area = centered(full, 78, 12);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title("是否启用 RCON 以使用玩家管理？")
        .borders(Borders::ALL)
        .border_style(WARNING);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let labels = ["取消", "启用 RCON 并继续", "本次不启用（再确认）"];
    let choices = labels
        .iter()
        .enumerate()
        .map(|(index, label)| {
            Span::styled(
                format!(" {label} "),
                if rcon.selected == index {
                    SELECTED
                } else {
                    MUTED
                },
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(vec![
            Line::from("启用会明确写入 enable-rcon=true、选定或复用的端口和生成或复用的密码。"),
            Line::from("凭据副本存于 config.json 与 server.properties。"),
            Line::from("不启用则直接启动，玩家页灰显。"),
            Line::from(""),
            Line::from(choices),
            Line::from("←/→ 选择，Enter 执行，Esc 取消"),
        ])
        .wrap(Wrap { trim: false }),
        inner,
    );
}
