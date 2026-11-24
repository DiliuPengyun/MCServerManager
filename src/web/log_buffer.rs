const MAX_LINES: usize = 2000;

/// 任务与操作共用的有界日志；offset 是已经裁掉的绝对行数。
#[derive(Debug, Default)]
pub struct BoundedLog {
    lines: Vec<String>,
    offset: u64,
}

impl BoundedLog {
    pub fn append(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
        if self.lines.len() > MAX_LINES {
            let excess = self.lines.len() - MAX_LINES;
            self.lines.drain(..excess);
            self.offset = self.offset.saturating_add(excess as u64);
        }
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }
}
