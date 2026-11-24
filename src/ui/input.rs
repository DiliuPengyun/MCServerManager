use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// 以字符而非字节记录光标，避免中文编辑时切到 UTF-8 字符内部。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextInput {
    chars: Vec<char>,
    cursor: usize,
}

impl TextInput {
    pub fn new(value: impl Into<String>) -> Self {
        let chars: Vec<char> = value.into().chars().collect();
        let cursor = chars.len();
        Self { chars, cursor }
    }

    pub fn value(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn set_value(&mut self, value: impl Into<String>) {
        self.chars = value.into().chars().collect();
        self.cursor = self.chars.len();
    }

    pub fn clear(&mut self) {
        self.chars.clear();
        self.cursor = 0;
    }

    pub fn before_cursor(&self) -> String {
        self.chars[..self.cursor].iter().collect()
    }

    /// 返回是否消费了按键。Enter、Esc 和快捷键由所属面板处理。
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.chars.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.chars.len(),
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                self.chars.remove(self.cursor);
            }
            KeyCode::Delete if self.cursor < self.chars.len() => {
                self.chars.remove(self.cursor);
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    && !character.is_control() =>
            {
                self.chars.insert(self.cursor, character);
                self.cursor += 1;
            }
            KeyCode::Backspace | KeyCode::Delete => {}
            _ => return false,
        }
        true
    }
}

/// 小型多行编辑器；业务保存仍由 FileBrowser 完成。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextEditor {
    chars: Vec<char>,
    cursor: usize,
    preferred_column: Option<usize>,
}

impl TextEditor {
    pub fn new(value: impl Into<String>) -> Self {
        let chars: Vec<char> = value.into().chars().collect();
        let cursor = chars.len();
        Self {
            chars,
            cursor,
            preferred_column: None,
        }
    }

    pub fn value(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn line_column(&self) -> (usize, usize) {
        let line = self.chars[..self.cursor]
            .iter()
            .filter(|character| **character == '\n')
            .count();
        let start = self.line_start(self.cursor);
        (line, self.cursor - start)
    }

    pub fn line_text(&self, line: usize) -> String {
        self.lines()
            .get(line)
            .map(|(start, end)| self.chars[*start..*end].iter().collect())
            .unwrap_or_default()
    }

    pub fn line_texts(&self) -> impl Iterator<Item = String> + '_ {
        self.lines()
            .into_iter()
            .map(|(start, end)| self.chars[start..end].iter().collect())
    }

    pub fn lines(&self) -> Vec<(usize, usize)> {
        let mut lines = Vec::new();
        let mut start = 0;
        for (index, character) in self.chars.iter().enumerate() {
            if *character == '\n' {
                lines.push((start, index));
                start = index + 1;
            }
        }
        lines.push((start, self.chars.len()));
        lines
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                self.preferred_column = None;
            }
            KeyCode::Right => {
                self.cursor = (self.cursor + 1).min(self.chars.len());
                self.preferred_column = None;
            }
            KeyCode::Up => self.move_vertical(-1),
            KeyCode::Down => self.move_vertical(1),
            KeyCode::Home => {
                self.cursor = self.line_start(self.cursor);
                self.preferred_column = None;
            }
            KeyCode::End => {
                self.cursor = self.line_end(self.cursor);
                self.preferred_column = None;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                self.chars.remove(self.cursor);
                self.preferred_column = None;
            }
            KeyCode::Delete if self.cursor < self.chars.len() => {
                self.chars.remove(self.cursor);
                self.preferred_column = None;
            }
            KeyCode::Enter => self.insert('\n'),
            KeyCode::Tab => self.insert('\t'),
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    && !character.is_control() =>
            {
                self.insert(character);
            }
            KeyCode::Backspace | KeyCode::Delete => {}
            _ => return false,
        }
        true
    }

    fn insert(&mut self, character: char) {
        self.chars.insert(self.cursor, character);
        self.cursor += 1;
        self.preferred_column = None;
    }

    fn line_start(&self, cursor: usize) -> usize {
        self.chars[..cursor]
            .iter()
            .rposition(|character| *character == '\n')
            .map_or(0, |index| index + 1)
    }

    fn line_end(&self, cursor: usize) -> usize {
        self.chars[cursor..]
            .iter()
            .position(|character| *character == '\n')
            .map_or(self.chars.len(), |index| cursor + index)
    }

    fn move_vertical(&mut self, delta: isize) {
        let lines = self.lines();
        let (line, column) = self.line_column();
        let preferred = *self.preferred_column.get_or_insert(column);
        let target = if delta < 0 {
            line.saturating_sub(delta.unsigned_abs())
        } else {
            (line + delta as usize).min(lines.len().saturating_sub(1))
        };
        let (start, end) = lines[target];
        self.cursor = start + preferred.min(end - start);
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyEventKind, KeyEventState};

    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn chinese_input_never_splits_utf8_and_edits_at_character_cursor() {
        let mut input = TextInput::new("服务器甲");
        input.handle_key(key(KeyCode::Left));
        input.handle_key(key(KeyCode::Char('乙')));
        assert_eq!(input.value(), "服务器乙甲");
        input.handle_key(key(KeyCode::Backspace));
        assert_eq!(input.value(), "服务器甲");
        input.handle_key(key(KeyCode::Home));
        input.handle_key(key(KeyCode::Delete));
        assert_eq!(input.value(), "务器甲");
    }

    #[test]
    fn multiline_navigation_preserves_preferred_column() {
        let mut editor = TextEditor::new("第一行\n二\n第三行");
        editor.cursor = editor.chars.len();
        editor.handle_key(key(KeyCode::Up));
        assert_eq!(editor.line_column(), (1, 1));
        editor.handle_key(key(KeyCode::Up));
        assert_eq!(editor.line_column(), (0, 3));
        editor.handle_key(key(KeyCode::Down));
        assert_eq!(editor.line_column(), (1, 1));
        editor.handle_key(key(KeyCode::Down));
        assert_eq!(editor.line_column(), (2, 3));
    }
}
