use gtk4::prelude::*;

/// Result of executing a motion: the range of text it covers.
#[derive(Debug, Clone)]
pub struct MotionRange {
    pub start: gtk4::TextIter,
    pub end: gtk4::TextIter,
    /// Whether the motion operates on full lines (for dd, yy, etc.)
    pub linewise: bool,
}

/// Check if a character is a "word" character (alphanumeric or underscore).
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Move cursor left by `count` characters.
pub fn motion_h(buf: &sourceview5::Buffer, count: u32) -> MotionRange {
    let start = buf.iter_at_mark(&buf.get_insert());
    let mut end = start;
    for _ in 0..count {
        if end.starts_line() {
            break;
        }
        end.backward_char();
    }
    MotionRange { start, end, linewise: false }
}

/// Move cursor down by `count` lines.
pub fn motion_j(buf: &sourceview5::Buffer, count: u32) -> MotionRange {
    let start = buf.iter_at_mark(&buf.get_insert());
    let mut end = start;
    for _ in 0..count {
        if !end.forward_line() {
            // Move to end of buffer
            end.forward_to_end();
            break;
        }
    }
    MotionRange { start, end, linewise: true }
}

/// Move cursor up by `count` lines.
pub fn motion_k(buf: &sourceview5::Buffer, count: u32) -> MotionRange {
    let start = buf.iter_at_mark(&buf.get_insert());
    let mut end = start;
    for _ in 0..count {
        if !end.backward_line() {
            end.set_line(0);
            break;
        }
    }
    MotionRange { start, end, linewise: true }
}

/// Move cursor right by `count` characters.
pub fn motion_l(buf: &sourceview5::Buffer, count: u32) -> MotionRange {
    let start = buf.iter_at_mark(&buf.get_insert());
    let mut end = start;
    for _ in 0..count {
        if end.ends_line() {
            break;
        }
        end.forward_char();
    }
    MotionRange { start, end, linewise: false }
}

/// Move to start of next word.
pub fn motion_w(buf: &sourceview5::Buffer, count: u32) -> MotionRange {
    let start = buf.iter_at_mark(&buf.get_insert());
    let mut end = start;
    for _ in 0..count {
        // Skip current word characters
        let at_word = is_word_char(end.char());
        if at_word {
            while !end.is_end() && is_word_char(end.char()) {
                end.forward_char();
            }
        } else {
            // Skip non-word, non-whitespace (punctuation)
            while !end.is_end()
                && !end.char().is_whitespace() && !is_word_char(end.char())
            {
                end.forward_char();
            }
        }
        // Skip whitespace
        while !end.is_end() && end.char().is_whitespace() {
            end.forward_char();
        }
    }
    MotionRange { start, end, linewise: false }
}

/// Move to start of next WORD (whitespace-delimited).
pub fn motion_big_w(buf: &sourceview5::Buffer, count: u32) -> MotionRange {
    let start = buf.iter_at_mark(&buf.get_insert());
    let mut end = start;
    for _ in 0..count {
        // Skip non-whitespace
        while !end.is_end() && !end.char().is_whitespace() {
            end.forward_char();
        }
        // Skip whitespace
        while !end.is_end() && end.char().is_whitespace() {
            end.forward_char();
        }
    }
    MotionRange { start, end, linewise: false }
}

/// Move to start of previous word.
pub fn motion_b(buf: &sourceview5::Buffer, count: u32) -> MotionRange {
    let start = buf.iter_at_mark(&buf.get_insert());
    let mut end = start;
    for _ in 0..count {
        // Move back one char to get off the boundary
        if !end.is_start() {
            end.backward_char();
        }
        // Skip whitespace
        while !end.is_start() && end.char().is_whitespace() {
            end.backward_char();
        }
        // Skip word chars (or punctuation) backwards
        let at_word = is_word_char(end.char());
        if at_word {
            while !end.is_start() {
                let mut prev = end;
                prev.backward_char();
                if !is_word_char(prev.char()) {
                    break;
                }
                end = prev;
            }
        } else {
            while !end.is_start() {
                let mut prev = end;
                prev.backward_char();
                if prev.char().is_whitespace() || is_word_char(prev.char()) {
                    break;
                }
                end = prev;
            }
        }
    }
    MotionRange { start, end, linewise: false }
}

/// Move to end of word.
pub fn motion_e(buf: &sourceview5::Buffer, count: u32) -> MotionRange {
    let start = buf.iter_at_mark(&buf.get_insert());
    let mut end = start;
    for _ in 0..count {
        // Move forward one char to get off current position
        if !end.is_end() {
            end.forward_char();
        }
        // Skip whitespace
        while !end.is_end() && end.char().is_whitespace() {
            end.forward_char();
        }
        // Move to end of word
        let at_word = is_word_char(end.char());
        if at_word {
            while !end.is_end() {
                let mut next = end;
                next.forward_char();
                if !is_word_char(next.char()) || next.is_end() {
                    break;
                }
                end = next;
            }
        } else {
            while !end.is_end() {
                let mut next = end;
                next.forward_char();
                if next.char().is_whitespace() || is_word_char(next.char())
                    || next.is_end()
                {
                    break;
                }
                end = next;
            }
        }
    }
    // For operators, the 'e' motion is inclusive (include the last char)
    if !end.is_end() {
        end.forward_char();
    }
    MotionRange { start, end, linewise: false }
}

/// Move to start of line.
pub fn motion_zero(buf: &sourceview5::Buffer) -> MotionRange {
    let start = buf.iter_at_mark(&buf.get_insert());
    let mut end = start;
    end.set_line_offset(0);
    MotionRange { start, end, linewise: false }
}

/// Move to end of line.
pub fn motion_dollar(buf: &sourceview5::Buffer) -> MotionRange {
    let start = buf.iter_at_mark(&buf.get_insert());
    let mut end = start;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    MotionRange { start, end, linewise: false }
}

/// Move to first non-blank character on line.
pub fn motion_caret(buf: &sourceview5::Buffer) -> MotionRange {
    let start = buf.iter_at_mark(&buf.get_insert());
    let mut end = start;
    end.set_line_offset(0);
    while !end.ends_line() && end.char().is_whitespace() {
        end.forward_char();
    }
    MotionRange { start, end, linewise: false }
}

/// Move to start of file (or line N if count given).
pub fn motion_gg(buf: &sourceview5::Buffer, count: Option<u32>) -> MotionRange {
    let start = buf.iter_at_mark(&buf.get_insert());
    let end = match count {
        Some(n) => {
            let line = (n as i32 - 1).max(0);
            buf.iter_at_line(line).unwrap_or(buf.start_iter())
        }
        None => buf.start_iter(),
    };
    MotionRange { start, end, linewise: true }
}

/// Move to end of file (or line N if count given).
pub fn motion_big_g(buf: &sourceview5::Buffer, count: Option<u32>) -> MotionRange {
    let start = buf.iter_at_mark(&buf.get_insert());
    let end = match count {
        Some(n) => {
            let line = (n as i32 - 1).max(0);
            buf.iter_at_line(line).unwrap_or(buf.end_iter())
        }
        None => buf.end_iter(),
    };
    MotionRange { start, end, linewise: true }
}

/// Find character forward on current line.
pub fn motion_f(buf: &sourceview5::Buffer, target: char, count: u32) -> Option<MotionRange> {
    let start = buf.iter_at_mark(&buf.get_insert());
    let mut end = start;
    let mut found = 0;
    loop {
        end.forward_char();
        if end.ends_line() || end.is_end() {
            break;
        }
        if end.char() == target {
            found += 1;
            if found >= count {
                // Include the found char for operators
                let mut inclusive_end = end;
                inclusive_end.forward_char();
                return Some(MotionRange { start, end: inclusive_end, linewise: false });
            }
        }
    }
    None
}

/// Find character backward on current line.
pub fn motion_big_f(buf: &sourceview5::Buffer, target: char, count: u32) -> Option<MotionRange> {
    let start = buf.iter_at_mark(&buf.get_insert());
    let mut end = start;
    let mut found = 0;
    loop {
        if end.starts_line() {
            break;
        }
        end.backward_char();
        if end.char() == target {
            found += 1;
            if found >= count {
                return Some(MotionRange { start, end, linewise: false });
            }
        }
    }
    None
}

/// Move to matching bracket.
pub fn motion_percent(buf: &sourceview5::Buffer) -> Option<MotionRange> {
    let start = buf.iter_at_mark(&buf.get_insert());
    let c = start.char();
    let (target, forward) = match c {
        '(' => (')', true),
        ')' => ('(', false),
        '[' => (']', true),
        ']' => ('[', false),
        '{' => ('}', true),
        '}' => ('{', false),
        _ => return None,
    };
    let mut end = start;
    let mut depth = 1i32;
    loop {
        if forward {
            end.forward_char();
        } else {
            if end.is_start() { return None; }
            end.backward_char();
        }
        if end.is_end() || (end.is_start() && !forward) {
            return None;
        }
        let ch = end.char();
        if ch != '\0' {
            if ch == c {
                depth += 1;
            } else if ch == target {
                depth -= 1;
                if depth == 0 {
                    return Some(MotionRange { start, end, linewise: false });
                }
            }
        }
    }
}

/// "Inner word" text object — selects the word under/near the cursor.
pub fn text_object_inner_word(buf: &sourceview5::Buffer) -> MotionRange {
    let cursor = buf.iter_at_mark(&buf.get_insert());
    let mut start = cursor;
    let mut end = cursor;

    if is_word_char(cursor.char()) {
        // Go backward to start of word
        while !start.is_start() {
            let mut prev = start;
            prev.backward_char();
            if !is_word_char(prev.char()) {
                break;
            }
            start = prev;
        }
        // Go forward to end of word
        while !end.is_end() && is_word_char(end.char()) {
            end.forward_char();
        }
    } else {
        // Non-word: select the non-word, non-space run
        while !start.is_start() {
            let mut prev = start;
            prev.backward_char();
            if prev.char().is_whitespace() || is_word_char(prev.char()) {
                break;
            }
            start = prev;
        }
        while !end.is_end() && !end.char().is_whitespace() && !is_word_char(end.char()) {
            end.forward_char();
        }
    }
    MotionRange { start, end, linewise: false }
}

/// "A word" text object — selects the word plus surrounding whitespace.
pub fn text_object_a_word(buf: &sourceview5::Buffer) -> MotionRange {
    let mut range = text_object_inner_word(buf);
    // Include trailing whitespace
    while !range.end.is_end() && range.end.char().is_whitespace() && range.end.char() != '\n' {
        range.end.forward_char();
    }
    range
}

/// Select the full current line(s) as a range.
pub fn motion_current_line(buf: &sourceview5::Buffer, count: u32) -> MotionRange {
    let cursor = buf.iter_at_mark(&buf.get_insert());
    let mut start = cursor;
    start.set_line_offset(0);
    let mut end = start;
    for _ in 0..count {
        if !end.forward_line() {
            end.forward_to_end();
            // Ensure we include the last line even without trailing newline
            if !end.ends_line() || end == start {
                end.forward_to_end();
            }
            break;
        }
    }
    MotionRange { start, end, linewise: true }
}
