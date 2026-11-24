use ratatui::Frame;

use super::state::App;

mod modal;
mod panels;
mod shell;
mod widgets;
mod wizard;

pub(super) fn render(frame: &mut Frame, app: &App) {
    shell::render(frame, app);
}

#[cfg(test)]
mod tests;
