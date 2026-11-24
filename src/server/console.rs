/// 清理 tmux 伪终端中 jline 产生的提示符重绘、终端模式和配色序列。
pub fn sanitize_console_line(line: &str) -> String {
    let mut result = line.as_bytes().to_vec();
    const REDRAW_TAIL: &[u8] = b"\r\x1b[K";

    loop {
        let mut position = 0;
        while position < result.len() {
            let Some(next) = skip_escape_sequence(&result, position) else {
                break;
            };
            position = next;
        }
        let mut erase_len = position;
        if result.get(position..position + 2) == Some(b"> ")
            && let Some(relative) = find_bytes(&result[position + 2..], REDRAW_TAIL)
        {
            erase_len = position + 2 + relative + REDRAW_TAIL.len();
        }
        if erase_len == 0 {
            break;
        }
        result.drain(..erase_len);
    }

    let mut clean = Vec::with_capacity(result.len());
    let mut index = 0;
    while index < result.len() {
        if let Some(next) = skip_escape_sequence(&result, index) {
            index = next;
            continue;
        }
        if matches!(result[index], b'\r' | 0x1b) {
            index += 1;
            continue;
        }
        clean.push(result[index]);
        index += 1;
    }
    String::from_utf8_lossy(&clean).into_owned()
}

fn skip_escape_sequence(value: &[u8], index: usize) -> Option<usize> {
    if value.get(index) != Some(&0x1b) || index + 1 >= value.len() {
        return None;
    }
    if value[index + 1] == b'[' {
        let mut cursor = index + 2;
        while cursor < value.len()
            && cursor - index < 32
            && value[cursor] != 0x1b
            && !(0x40..=0x7e).contains(&value[cursor])
        {
            cursor += 1;
        }
        return Some(if cursor < value.len() && value[cursor] != 0x1b {
            cursor + 1
        } else {
            cursor
        });
    }
    let mut cursor = index + 1;
    while cursor < value.len() && (0x20..=0x2f).contains(&value[cursor]) {
        cursor += 1;
    }
    (cursor < value.len() && (0x30..=0x7e).contains(&value[cursor])).then_some(cursor + 1)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_colors_modes_and_redraw_without_duplicating_echo() {
        assert_eq!(
            sanitize_console_line("\x1b[32mINFO\x1b[m 正常"),
            "INFO 正常"
        );
        assert_eq!(
            sanitize_console_line("\x1b[?2004h\x1b=\x1b[m> say 你好\r"),
            "> say 你好"
        );
        assert_eq!(
            sanitize_console_line("\x1b[m> old\r\x1b[K\x1b[m> new"),
            "> new"
        );
    }
}
