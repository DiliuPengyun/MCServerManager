use std::{collections::VecDeque, sync::Arc};

use parking_lot::RwLock;
use serde::Serialize;
use tokio::sync::broadcast;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ConsoleEvent {
    pub sequence: u64,
    pub line: String,
}

#[derive(Default)]
struct BufferState {
    lines: VecDeque<ConsoleEvent>,
    next_sequence: u64,
}

/// 有界日志快照与广播订阅。广播慢消费者只会收到 Lagged，不会阻塞日志尾随。
pub struct ConsoleBuffer {
    state: RwLock<BufferState>,
    sender: broadcast::Sender<ConsoleEvent>,
    capacity: usize,
}

impl ConsoleBuffer {
    pub fn new(capacity: usize) -> Arc<Self> {
        let (sender, _) = broadcast::channel(1024);
        Arc::new(Self {
            state: RwLock::new(BufferState::default()),
            sender,
            capacity: capacity.max(1),
        })
    }

    pub fn push(&self, line: impl Into<String>) -> ConsoleEvent {
        let event = {
            let mut state = self.state.write();
            state.next_sequence = state.next_sequence.wrapping_add(1).max(1);
            let event = ConsoleEvent {
                sequence: state.next_sequence,
                line: line.into(),
            };
            state.lines.push_back(event.clone());
            while state.lines.len() > self.capacity {
                state.lines.pop_front();
            }
            event
        };
        let _ = self.sender.send(event.clone());
        event
    }

    pub fn clear(&self) {
        self.state.write().lines.clear();
    }

    pub fn lines(&self) -> Vec<String> {
        self.state
            .read()
            .lines
            .iter()
            .map(|event| event.line.clone())
            .collect()
    }

    pub fn snapshot(&self) -> (u64, Vec<ConsoleEvent>) {
        let state = self.state.read();
        (state.next_sequence, state.lines.iter().cloned().collect())
    }

    pub fn events_since(&self, sequence: u64, limit: usize) -> Option<Vec<ConsoleEvent>> {
        let state = self.state.read();
        let oldest = state.lines.front().map(|event| event.sequence).unwrap_or(1);
        if sequence.saturating_add(1) < oldest {
            return None;
        }
        Some(
            state
                .lines
                .iter()
                .filter(|event| event.sequence > sequence)
                .take(limit)
                .cloned()
                .collect(),
        )
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ConsoleEvent> {
        self.sender.subscribe()
    }

    pub fn len(&self) -> usize {
        self.state.read().lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_capacity_and_monotonic_cursors() {
        let console = ConsoleBuffer::new(2);
        console.push("一");
        let second = console.push("二");
        let third = console.push("三");
        assert_eq!(console.lines(), vec!["二", "三"]);
        assert!(console.events_since(0, 10).is_none());
        assert_eq!(
            console.events_since(second.sequence, 10).unwrap(),
            vec![third]
        );
        console.clear();
        let fourth = console.push("四");
        assert_eq!(fourth.sequence, 4);
    }
}
