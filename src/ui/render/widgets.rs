use std::path::Path;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::core::ServerState;

use super::super::input::TextInput;

pub(super) const SELECTED: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
pub(super) const MUTED: Style = Style::new().fg(Color::DarkGray);
pub(super) const ERROR: Style = Style::new().fg(Color::Red).add_modifier(Modifier::BOLD);
pub(super) const WARNING: Style = Style::new().fg(Color::Yellow);
pub(super) const SUCCESS: Style = Style::new().fg(Color::Green);

pub(super) fn form_line(selected: bool, name: &str, input: &TextInput) -> Line<'static> {
    Line::from(vec![
        Span::styled(if selected { "▶ " } else { "  " }, SELECTED),
        Span::styled(
            format!("{name}："),
            if selected { SELECTED } else { Style::new() },
        ),
        Span::styled(input.value(), Style::new().fg(Color::White)),
    ])
}

pub(super) fn set_input_cursor(frame: &mut Frame, area: Rect, input: &TextInput) {
    let width = Line::from(input.before_cursor()).width() as u16;
    if area.height > 0 && width < area.width {
        frame.set_cursor_position((area.x + width, area.y));
    }
}

pub(super) fn centered(full: Rect, preferred_width: u16, preferred_height: u16) -> Rect {
    let width = preferred_width.min(full.width);
    let height = preferred_height.min(full.height);
    Rect {
        x: full.x + full.width.saturating_sub(width) / 2,
        y: full.y + full.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

pub(super) fn state_style(state: ServerState) -> Style {
    match state {
        ServerState::Running => SUCCESS,
        ServerState::Stopped => MUTED,
        ServerState::Starting | ServerState::Stopping => WARNING,
        ServerState::Unknown => ERROR,
    }
}

pub(super) fn empty_as<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}

pub(super) fn path_or_empty(path: &Path) -> String {
    if path.as_os_str().is_empty() {
        "未配置".to_owned()
    } else {
        path.display().to_string()
    }
}
