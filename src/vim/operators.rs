use gtk4::prelude::*;

use super::motions::MotionRange;

/// Execute a delete operation on the given range.
pub fn op_delete(buf: &sourceview5::Buffer, range: &MotionRange) -> String {
    let mut start = range.start;
    let mut end = range.end;

    // For linewise operations, ensure we select full lines
    if range.linewise {
        start.set_line_offset(0);
        if !end.ends_line() && !end.is_end() {
            end.forward_to_line_end();
        }
        // Include the trailing newline if present
        if !end.is_end() {
            end.forward_char();
        }
    }

    // Ensure start <= end
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }

    let deleted = buf.text(&start, &end, false).to_string();
    buf.delete(&mut start, &mut end);
    deleted
}

/// Execute a yank (copy) operation on the given range.
pub fn op_yank(buf: &sourceview5::Buffer, range: &MotionRange) -> String {
    let mut start = range.start;
    let mut end = range.end;

    if range.linewise {
        start.set_line_offset(0);
        if !end.ends_line() && !end.is_end() {
            end.forward_to_line_end();
        }
        if !end.is_end() {
            end.forward_char();
        }
    }

    if start > end {
        std::mem::swap(&mut start, &mut end);
    }

    buf.text(&start, &end, false).to_string()
}

/// Paste text after cursor.
pub fn op_paste_after(buf: &sourceview5::Buffer, text: &str, linewise: bool) {
    if linewise {
        // Paste on a new line below
        let mut iter = buf.iter_at_mark(&buf.get_insert());
        if !iter.ends_line() {
            iter.forward_to_line_end();
        }
        let insert_text = if text.ends_with('\n') {
            format!("\n{}", &text[..text.len() - 1])
        } else {
            format!("\n{text}")
        };
        buf.insert(&mut iter, &insert_text);
    } else {
        let mut iter = buf.iter_at_mark(&buf.get_insert());
        if !iter.ends_line() {
            iter.forward_char();
        }
        buf.insert(&mut iter, text);
    }
}

/// Paste text before cursor.
pub fn op_paste_before(buf: &sourceview5::Buffer, text: &str, linewise: bool) {
    if linewise {
        let mut iter = buf.iter_at_mark(&buf.get_insert());
        iter.set_line_offset(0);
        let insert_text = if text.ends_with('\n') {
            text.to_string()
        } else {
            format!("{text}\n")
        };
        buf.insert(&mut iter, &insert_text);
    } else {
        let mut iter = buf.iter_at_mark(&buf.get_insert());
        buf.insert(&mut iter, text);
    }
}

/// Join the current line with the next line.
pub fn op_join_lines(buf: &sourceview5::Buffer) {
    let mut iter = buf.iter_at_mark(&buf.get_insert());
    if !iter.ends_line() {
        iter.forward_to_line_end();
    }
    if iter.is_end() {
        return;
    }
    let mut end = iter;
    end.forward_char(); // Move past the newline
    // Skip leading whitespace on the next line
    while !end.is_end() && (end.char() == ' ' || end.char() == '\t') {
        end.forward_char();
    }
    buf.delete(&mut iter, &mut end);
    buf.insert(&mut iter, " ");
}

/// Toggle case of character under cursor.
pub fn op_toggle_case(buf: &sourceview5::Buffer) {
    let mut start = buf.iter_at_mark(&buf.get_insert());
    if start.is_end() || start.ends_line() {
        return;
    }
    let mut end = start;
    end.forward_char();
    let c = start.char();
    if c == '\0' {
        return;
    }
    let toggled: String = if c.is_uppercase() {
        c.to_lowercase().to_string()
    } else {
        c.to_uppercase().to_string()
    };
    buf.delete(&mut start, &mut end);
    buf.insert(&mut start, &toggled);
}

/// Replace character under cursor.
pub fn op_replace_char(buf: &sourceview5::Buffer, replacement: char) {
    let mut start = buf.iter_at_mark(&buf.get_insert());
    if start.is_end() || start.ends_line() {
        return;
    }
    let mut end = start;
    end.forward_char();
    buf.delete(&mut start, &mut end);
    buf.insert(&mut start, &replacement.to_string());
    // Move cursor back to the replaced position
    let mut cursor = buf.iter_at_mark(&buf.get_insert());
    cursor.backward_char();
    buf.place_cursor(&cursor);
}

/// Delete character under cursor (like 'x').
pub fn op_delete_char(buf: &sourceview5::Buffer, count: u32) -> String {
    let mut start = buf.iter_at_mark(&buf.get_insert());
    let mut end = start;
    for _ in 0..count {
        if end.ends_line() || end.is_end() {
            break;
        }
        end.forward_char();
    }
    let deleted = buf.text(&start, &end, false).to_string();
    buf.delete(&mut start, &mut end);
    deleted
}

/// Open a new line below and return the position to insert at.
pub fn op_open_line_below(buf: &sourceview5::Buffer) {
    let mut iter = buf.iter_at_mark(&buf.get_insert());
    if !iter.ends_line() {
        iter.forward_to_line_end();
    }
    buf.insert(&mut iter, "\n");
    buf.place_cursor(&iter);
}

/// Open a new line above and return the position to insert at.
pub fn op_open_line_above(buf: &sourceview5::Buffer) {
    let mut iter = buf.iter_at_mark(&buf.get_insert());
    iter.set_line_offset(0);
    buf.insert(&mut iter, "\n");
    iter.backward_char();
    buf.place_cursor(&iter);
}

/// Indent lines in range.
pub fn op_indent(buf: &sourceview5::Buffer, range: &MotionRange) {
    let start_line = range.start.line().min(range.end.line());
    let end_line = range.start.line().max(range.end.line());
    for line in start_line..=end_line {
        if let Some(mut iter) = buf.iter_at_line(line) {
            buf.insert(&mut iter, "    ");
        }
    }
}

/// Unindent lines in range.
pub fn op_unindent(buf: &sourceview5::Buffer, range: &MotionRange) {
    let start_line = range.start.line().min(range.end.line());
    let end_line = range.start.line().max(range.end.line());
    for line in start_line..=end_line {
        if let Some(iter) = buf.iter_at_line(line) {
            let mut end = iter;
            let mut removed = 0;
            while removed < 4 && !end.ends_line() && end.char() == ' ' {
                end.forward_char();
                removed += 1;
            }
            if removed > 0 {
                let mut start = iter;
                buf.delete(&mut start, &mut end);
            }
        }
    }
}
