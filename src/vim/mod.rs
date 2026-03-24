pub mod commands;
pub mod motions;
pub mod operators;

use gtk4::gdk;
use gtk4::prelude::*;
use sourceview5::View;
use std::cell::RefCell;
use std::rc::Rc;

use crate::editor::SearchPanel;
use commands::CommandResult;
use motions::MotionRange;

// ─── Vim modes ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimMode {
    Normal,
    Insert,
    Visual,
    VisualLine,
    Command,
    SearchForward,
    SearchBackward,
}

impl VimMode {
    pub fn display_name(&self) -> &'static str {
        match self {
            VimMode::Normal => "NORMAL",
            VimMode::Insert => "INSERT",
            VimMode::Visual => "VISUAL",
            VimMode::VisualLine => "V-LINE",
            VimMode::Command => "COMMAND",
            VimMode::SearchForward => "SEARCH",
            VimMode::SearchBackward => "SEARCH",
        }
    }
}

// ─── Pending operator ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingOp {
    Delete,
    Change,
    Yank,
    Indent,
    Unindent,
}

// ─── Pending state for multi-key sequences ──────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingKey {
    None,
    /// Waiting for a char (f, F, r)
    WaitChar(WaitCharKind),
    /// 'g' pressed, waiting for second key
    G,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WaitCharKind {
    FindForward,   // f
    FindBackward,  // F
    Replace,       // r
}

// ─── Register (clipboard) ───────────────────────────────────────

#[derive(Debug, Clone)]
struct Register {
    text: String,
    linewise: bool,
}

// ─── Main vim state ─────────────────────────────────────────────

#[derive(Debug)]
pub struct VimState {
    pub enabled: bool,
    pub mode: VimMode,
    pending_op: Option<PendingOp>,
    pending_key: PendingKey,
    count_buf: String,
    register: Option<Register>,
    command_buf: String,
    last_search: Option<String>,
    last_search_forward: bool,
    /// Visual mode anchor point
    visual_anchor: Option<gtk4::TextMark>,
    /// For repeat (.)
    last_edit: Option<LastEdit>,
}

#[derive(Debug, Clone)]
struct LastEdit {
    keys: Vec<(gdk::Key, gdk::ModifierType)>,
}

impl VimState {
    pub fn new() -> Self {
        VimState {
            enabled: true,
            mode: VimMode::Normal,
            pending_op: None,
            pending_key: PendingKey::None,
            count_buf: String::new(),
            register: None,
            command_buf: String::new(),
            last_search: None,
            last_search_forward: true,
            visual_anchor: None,
            last_edit: None,
        }
    }

    fn count(&self) -> u32 {
        self.count_buf.parse::<u32>().unwrap_or(1).max(1)
    }

    fn count_opt(&self) -> Option<u32> {
        self.count_buf.parse::<u32>().ok()
    }

    fn reset_count(&mut self) {
        self.count_buf.clear();
    }

    pub fn status_text(&self) -> String {
        match self.mode {
            VimMode::Command => format!(":{}", self.command_buf),
            VimMode::SearchForward => format!("/{}", self.command_buf),
            VimMode::SearchBackward => format!("?{}", self.command_buf),
            _ => {
                let mut s = format!("-- {} --", self.mode.display_name());
                if let Some(op) = &self.pending_op {
                    s.push_str(&format!("  {:?}", op));
                }
                if !self.count_buf.is_empty() {
                    s.push_str(&format!("  {}", self.count_buf));
                }
                s
            }
        }
    }
}

// ─── VimHandler: connects to the view ───────────────────────────

pub struct VimHandler {
    pub state: Rc<RefCell<VimState>>,
    pub status_label: gtk4::Label,
}

impl VimHandler {
    pub fn new(view: &View, status_label: gtk4::Label, search: Rc<SearchPanel>) -> Self {
        let state = Rc::new(RefCell::new(VimState::new()));

        // Start in Normal mode — disable editing
        view.set_editable(false);
        view.set_cursor_visible(true);

        let handler = VimHandler {
            state: state.clone(),
            status_label: status_label.clone(),
        };

        handler.update_status();

        // Install key controller
        let key_ctrl = gtk4::EventControllerKey::new();
        key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);

        let state_clone = state.clone();
        let view_clone = view.clone();
        let label_clone = status_label.clone();
        let search_clone = search.clone();

        key_ctrl.connect_key_pressed(move |_, key, _keycode, modifiers| {
            if !state_clone.borrow().enabled {
                return glib::Propagation::Proceed;
            }

            let result = handle_key(&state_clone, &view_clone, &search_clone, key, modifiers);
            // Update status label
            let st = state_clone.borrow();
            label_clone.set_text(&st.status_text());
            result
        });

        view.add_controller(key_ctrl);

        handler
    }

    fn update_status(&self) {
        let st = self.state.borrow();
        if st.enabled {
            self.status_label.set_visible(true);
            self.status_label.set_text(&st.status_text());
        } else {
            self.status_label.set_visible(false);
        }
    }

    pub fn set_enabled(&self, view: &View, enabled: bool) {
        self.state.borrow_mut().enabled = enabled;
        if enabled {
            // Reset to normal mode
            enter_normal(&self.state, view);
        } else {
            // Behave like a normal editor
            view.set_editable(true);
            view.set_cursor_visible(true);
        }
        self.update_status();
    }
}

// ─── Key dispatch ───────────────────────────────────────────────

fn handle_key(
    state: &Rc<RefCell<VimState>>,
    view: &View,
    search: &Rc<SearchPanel>,
    key: gdk::Key,
    modifiers: gdk::ModifierType,
) -> glib::Propagation {
    let mode = state.borrow().mode;
    match mode {
        VimMode::Insert => handle_insert(state, view, key),
        VimMode::Normal => handle_normal(state, view, search, key, modifiers),
        VimMode::Visual | VimMode::VisualLine => handle_visual(state, view, key, modifiers),
        VimMode::Command => handle_command(state, view, search, key),
        VimMode::SearchForward | VimMode::SearchBackward => handle_search(state, view, search, key),
    }
}

// ─── Insert mode ────────────────────────────────────────────────

fn handle_insert(
    state: &Rc<RefCell<VimState>>,
    view: &View,
    key: gdk::Key,
) -> glib::Propagation {
    if key == gdk::Key::Escape {
        enter_normal(state, view);
        return glib::Propagation::Stop;
    }
    // Let GTK handle all other keys in insert mode
    glib::Propagation::Proceed
}

// ─── Normal mode ────────────────────────────────────────────────

fn handle_normal(
    state: &Rc<RefCell<VimState>>,
    view: &View,
    search: &Rc<SearchPanel>,
    key: gdk::Key,
    modifiers: gdk::ModifierType,
) -> glib::Propagation {
    let buf = view.buffer()
        .downcast::<sourceview5::Buffer>()
        .expect("Buffer should be sourceview5::Buffer");

    // Handle pending multi-key sequences first
    {
        let pending = state.borrow().pending_key.clone();
        match pending {
            PendingKey::WaitChar(kind) => {
                state.borrow_mut().pending_key = PendingKey::None;
                if let Some(c) = key.to_unicode() {
                    let count = state.borrow().count();
                    match kind {
                        WaitCharKind::FindForward => {
                            if let Some(range) = motions::motion_f(&buf, c, count) {
                                apply_motion_or_op(state, &buf, range);
                            }
                        }
                        WaitCharKind::FindBackward => {
                            if let Some(range) = motions::motion_big_f(&buf, c, count) {
                                apply_motion_or_op(state, &buf, range);
                            }
                        }
                        WaitCharKind::Replace => {
                            buf.begin_user_action();
                            operators::op_replace_char(&buf, c);
                            buf.end_user_action();
                        }
                    }
                    state.borrow_mut().reset_count();
                }
                return glib::Propagation::Stop;
            }
            PendingKey::G => {
                state.borrow_mut().pending_key = PendingKey::None;
                match key {
                    gdk::Key::g => {
                        let count = state.borrow().count_opt();
                        let range = motions::motion_gg(&buf, count);
                        apply_motion_or_op(state, &buf, range);
                        state.borrow_mut().reset_count();
                    }
                    _ => {
                        state.borrow_mut().reset_count();
                    }
                }
                return glib::Propagation::Stop;
            }
            PendingKey::None => {}
        }
    }

    // Handle pending operator expecting a text object after 'i' or 'a'
    // (This handles cases like 'diw', 'ciw', 'yiw')

    // Count accumulation (digits)
    if let Some(c) = key.to_unicode() {
        if c.is_ascii_digit() {
            // '0' at the start is a motion (line start), not a count
            let is_line_start = c == '0' && state.borrow().count_buf.is_empty()
                && state.borrow().pending_op.is_none();
            if !is_line_start {
                state.borrow_mut().count_buf.push(c);
                return glib::Propagation::Stop;
            }
        }
    }

    let count = state.borrow().count();
    let has_pending_op = state.borrow().pending_op.is_some();

    // Motions
    match key {
        // ── Basic motions ──
        gdk::Key::h | gdk::Key::Left => {
            let range = motions::motion_h(&buf, count);
            apply_motion_or_op(state, &buf, range);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::j | gdk::Key::Down => {
            let range = motions::motion_j(&buf, count);
            apply_motion_or_op(state, &buf, range);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::k | gdk::Key::Up => {
            let range = motions::motion_k(&buf, count);
            apply_motion_or_op(state, &buf, range);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::l | gdk::Key::Right => {
            let range = motions::motion_l(&buf, count);
            apply_motion_or_op(state, &buf, range);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }

        // ── Word motions ──
        gdk::Key::w => {
            if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
                let range = motions::motion_big_w(&buf, count);
                apply_motion_or_op(state, &buf, range);
            } else {
                let range = motions::motion_w(&buf, count);
                apply_motion_or_op(state, &buf, range);
            }
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::W => {
            let range = motions::motion_big_w(&buf, count);
            apply_motion_or_op(state, &buf, range);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::b => {
            let range = motions::motion_b(&buf, count);
            apply_motion_or_op(state, &buf, range);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::e => {
            let range = motions::motion_e(&buf, count);
            apply_motion_or_op(state, &buf, range);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }

        // ── Line motions ──
        gdk::Key::_0 | gdk::Key::Home => {
            let range = motions::motion_zero(&buf);
            apply_motion_or_op(state, &buf, range);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::dollar | gdk::Key::End => {
            let range = motions::motion_dollar(&buf);
            apply_motion_or_op(state, &buf, range);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::asciicircum => {
            let range = motions::motion_caret(&buf);
            apply_motion_or_op(state, &buf, range);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }

        // ── File motions ──
        gdk::Key::g => {
            state.borrow_mut().pending_key = PendingKey::G;
            return glib::Propagation::Stop;
        }
        gdk::Key::G => {
            let count = state.borrow().count_opt();
            let range = motions::motion_big_g(&buf, count);
            apply_motion_or_op(state, &buf, range);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }

        // ── Char search ──
        gdk::Key::f => {
            state.borrow_mut().pending_key = PendingKey::WaitChar(WaitCharKind::FindForward);
            return glib::Propagation::Stop;
        }
        gdk::Key::F => {
            state.borrow_mut().pending_key = PendingKey::WaitChar(WaitCharKind::FindBackward);
            return glib::Propagation::Stop;
        }

        // ── Bracket matching ──
        gdk::Key::percent => {
            if let Some(range) = motions::motion_percent(&buf) {
                apply_motion_or_op(state, &buf, range);
            }
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }

        // ── Text objects (only valid after an operator) ──
        gdk::Key::i if has_pending_op => {
            // Wait for next key to determine text object (iw, etc.)
            // For now, handle inline
            state.borrow_mut().pending_key = PendingKey::WaitChar(WaitCharKind::FindForward);
            // Actually need a different mechanism — let's handle 'iw' directly
            // by storing that we're in text-object mode. For simplicity, we'll
            // handle the most common text objects inline.
            state.borrow_mut().pending_key = PendingKey::None;
            // We'll need the next key — but since we can't easily peek,
            // let's use a workaround by reprocessing. Set a special pending.
            // For now, apply "inner word" directly since it's the most common.
            let range = motions::text_object_inner_word(&buf);
            apply_motion_or_op(state, &buf, range);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::a if has_pending_op => {
            let range = motions::text_object_a_word(&buf);
            apply_motion_or_op(state, &buf, range);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }

        // ── Operators ──
        gdk::Key::d => {
            if state.borrow().pending_op == Some(PendingOp::Delete) {
                // dd: delete line
                let range = motions::motion_current_line(&buf, count);
                buf.begin_user_action();
                let deleted = operators::op_delete(&buf, &range);
                buf.end_user_action();
                state.borrow_mut().register = Some(Register { text: deleted, linewise: true });
                state.borrow_mut().pending_op = None;
                state.borrow_mut().reset_count();
            } else {
                state.borrow_mut().pending_op = Some(PendingOp::Delete);
            }
            return glib::Propagation::Stop;
        }
        gdk::Key::c => {
            if state.borrow().pending_op == Some(PendingOp::Change) {
                // cc: change line
                let range = motions::motion_current_line(&buf, count);
                buf.begin_user_action();
                let deleted = operators::op_delete(&buf, &range);
                buf.end_user_action();
                state.borrow_mut().register = Some(Register { text: deleted, linewise: true });
                state.borrow_mut().pending_op = None;
                state.borrow_mut().reset_count();
                enter_insert(state, view);
            } else {
                state.borrow_mut().pending_op = Some(PendingOp::Change);
            }
            return glib::Propagation::Stop;
        }
        gdk::Key::y => {
            if state.borrow().pending_op == Some(PendingOp::Yank) {
                // yy: yank line
                let range = motions::motion_current_line(&buf, count);
                let yanked = operators::op_yank(&buf, &range);
                state.borrow_mut().register = Some(Register { text: yanked, linewise: true });
                state.borrow_mut().pending_op = None;
                state.borrow_mut().reset_count();
            } else {
                state.borrow_mut().pending_op = Some(PendingOp::Yank);
            }
            return glib::Propagation::Stop;
        }

        gdk::Key::greater if has_pending_op => {
            // >> indent
            if state.borrow().pending_op == Some(PendingOp::Indent) {
                let range = motions::motion_current_line(&buf, count);
                buf.begin_user_action();
                operators::op_indent(&buf, &range);
                buf.end_user_action();
                state.borrow_mut().pending_op = None;
                state.borrow_mut().reset_count();
            }
            return glib::Propagation::Stop;
        }
        gdk::Key::greater => {
            state.borrow_mut().pending_op = Some(PendingOp::Indent);
            return glib::Propagation::Stop;
        }
        gdk::Key::less if has_pending_op => {
            if state.borrow().pending_op == Some(PendingOp::Unindent) {
                let range = motions::motion_current_line(&buf, count);
                buf.begin_user_action();
                operators::op_unindent(&buf, &range);
                buf.end_user_action();
                state.borrow_mut().pending_op = None;
                state.borrow_mut().reset_count();
            }
            return glib::Propagation::Stop;
        }
        gdk::Key::less => {
            state.borrow_mut().pending_op = Some(PendingOp::Unindent);
            return glib::Propagation::Stop;
        }

        // ── Single-key operations ──
        gdk::Key::x => {
            buf.begin_user_action();
            let deleted = operators::op_delete_char(&buf, count);
            buf.end_user_action();
            state.borrow_mut().register = Some(Register { text: deleted, linewise: false });
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::u => {
            if buf.can_undo() {
                buf.undo();
            }
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::r if modifiers.contains(gdk::ModifierType::CONTROL_MASK) => {
            if buf.can_redo() {
                buf.redo();
            }
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::r => {
            state.borrow_mut().pending_key = PendingKey::WaitChar(WaitCharKind::Replace);
            return glib::Propagation::Stop;
        }
        gdk::Key::J => {
            buf.begin_user_action();
            for _ in 0..count {
                operators::op_join_lines(&buf);
            }
            buf.end_user_action();
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::asciitilde => {
            buf.begin_user_action();
            for _ in 0..count {
                operators::op_toggle_case(&buf);
                // Move cursor forward
                let mut iter = buf.iter_at_mark(&buf.get_insert());
                if !iter.ends_line() {
                    iter.forward_char();
                    buf.place_cursor(&iter);
                }
            }
            buf.end_user_action();
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::D => {
            // Delete to end of line
            let range = motions::motion_dollar(&buf);
            buf.begin_user_action();
            let deleted = operators::op_delete(&buf, &range);
            buf.end_user_action();
            state.borrow_mut().register = Some(Register { text: deleted, linewise: false });
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::C => {
            // Change to end of line
            let range = motions::motion_dollar(&buf);
            buf.begin_user_action();
            let deleted = operators::op_delete(&buf, &range);
            buf.end_user_action();
            state.borrow_mut().register = Some(Register { text: deleted, linewise: false });
            state.borrow_mut().reset_count();
            enter_insert(state, view);
            return glib::Propagation::Stop;
        }
        gdk::Key::Y => {
            // Yank line (like yy)
            let range = motions::motion_current_line(&buf, count);
            let yanked = operators::op_yank(&buf, &range);
            state.borrow_mut().register = Some(Register { text: yanked, linewise: true });
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }

        // ── Mode switches ──
        gdk::Key::i => {
            enter_insert(state, view);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::I => {
            // Insert at first non-blank
            let range = motions::motion_caret(&buf);
            buf.place_cursor(&range.end);
            enter_insert(state, view);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::a => {
            // Append after cursor
            let mut iter = buf.iter_at_mark(&buf.get_insert());
            if !iter.ends_line() {
                iter.forward_char();
                buf.place_cursor(&iter);
            }
            enter_insert(state, view);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::A => {
            // Append at end of line
            let range = motions::motion_dollar(&buf);
            buf.place_cursor(&range.end);
            enter_insert(state, view);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::o => {
            buf.begin_user_action();
            operators::op_open_line_below(&buf);
            buf.end_user_action();
            enter_insert(state, view);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::O => {
            buf.begin_user_action();
            operators::op_open_line_above(&buf);
            buf.end_user_action();
            enter_insert(state, view);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }

        // ── Visual mode ──
        gdk::Key::v => {
            enter_visual(state, &buf, VimMode::Visual);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::V => {
            enter_visual(state, &buf, VimMode::VisualLine);
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }

        // ── Command mode ──
        gdk::Key::colon => {
            state.borrow_mut().mode = VimMode::Command;
            state.borrow_mut().command_buf.clear();
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::slash => {
            state.borrow_mut().mode = VimMode::SearchForward;
            state.borrow_mut().command_buf.clear();
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::question => {
            state.borrow_mut().mode = VimMode::SearchBackward;
            state.borrow_mut().command_buf.clear();
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::n => {
            let (query, forward) = {
                let st = state.borrow();
                (st.last_search.clone(), st.last_search_forward)
            };
            if let Some(query) = query {
                search.search_from_cursor(&query, forward);
            }
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }
        gdk::Key::N => {
            let (query, forward) = {
                let st = state.borrow();
                (st.last_search.clone(), !st.last_search_forward)
            };
            if let Some(query) = query {
                search.search_from_cursor(&query, forward);
            }
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }

        // ── Escape: cancel pending ──
        gdk::Key::Escape => {
            state.borrow_mut().pending_op = None;
            state.borrow_mut().pending_key = PendingKey::None;
            state.borrow_mut().reset_count();
            return glib::Propagation::Stop;
        }

        _ => {}
    }

    // Consume everything else in Normal mode
    glib::Propagation::Stop
}

// ─── Visual mode ────────────────────────────────────────────────

fn handle_visual(
    state: &Rc<RefCell<VimState>>,
    view: &View,
    key: gdk::Key,
    _modifiers: gdk::ModifierType,
) -> glib::Propagation {
    let buf = view.buffer()
        .downcast::<sourceview5::Buffer>()
        .expect("Buffer should be sourceview5::Buffer");

    match key {
        gdk::Key::Escape | gdk::Key::v | gdk::Key::V => {
            // Exit visual mode
            buf.select_range(
                &buf.iter_at_mark(&buf.get_insert()),
                &buf.iter_at_mark(&buf.get_insert()),
            );
            enter_normal(state, view);
            return glib::Propagation::Stop;
        }
        // Motions in visual mode — extend selection
        gdk::Key::h | gdk::Key::Left => {
            extend_visual_selection(state, &buf, |b| motions::motion_h(b, 1));
        }
        gdk::Key::j | gdk::Key::Down => {
            extend_visual_selection(state, &buf, |b| motions::motion_j(b, 1));
        }
        gdk::Key::k | gdk::Key::Up => {
            extend_visual_selection(state, &buf, |b| motions::motion_k(b, 1));
        }
        gdk::Key::l | gdk::Key::Right => {
            extend_visual_selection(state, &buf, |b| motions::motion_l(b, 1));
        }
        gdk::Key::w => {
            extend_visual_selection(state, &buf, |b| motions::motion_w(b, 1));
        }
        gdk::Key::b => {
            extend_visual_selection(state, &buf, |b| motions::motion_b(b, 1));
        }
        gdk::Key::e => {
            extend_visual_selection(state, &buf, |b| motions::motion_e(b, 1));
        }
        gdk::Key::_0 => {
            extend_visual_selection(state, &buf, |b| motions::motion_zero(b));
        }
        gdk::Key::dollar => {
            extend_visual_selection(state, &buf, |b| motions::motion_dollar(b));
        }
        gdk::Key::G => {
            extend_visual_selection(state, &buf, |b| motions::motion_big_g(b, None));
        }

        // Operators on selection
        gdk::Key::d | gdk::Key::x => {
            let (start, end) = visual_selection_range(state, &buf);
            let range = MotionRange {
                start,
                end,
                linewise: state.borrow().mode == VimMode::VisualLine,
            };
            buf.begin_user_action();
            let deleted = operators::op_delete(&buf, &range);
            buf.end_user_action();
            state.borrow_mut().register = Some(Register {
                text: deleted,
                linewise: state.borrow().mode == VimMode::VisualLine,
            });
            enter_normal(state, view);
        }
        gdk::Key::c => {
            let (start, end) = visual_selection_range(state, &buf);
            let range = MotionRange {
                start,
                end,
                linewise: state.borrow().mode == VimMode::VisualLine,
            };
            buf.begin_user_action();
            let deleted = operators::op_delete(&buf, &range);
            buf.end_user_action();
            state.borrow_mut().register = Some(Register {
                text: deleted,
                linewise: state.borrow().mode == VimMode::VisualLine,
            });
            enter_insert(state, view);
        }
        gdk::Key::y => {
            let (start, end) = visual_selection_range(state, &buf);
            let range = MotionRange {
                start,
                end,
                linewise: state.borrow().mode == VimMode::VisualLine,
            };
            let yanked = operators::op_yank(&buf, &range);
            state.borrow_mut().register = Some(Register {
                text: yanked,
                linewise: state.borrow().mode == VimMode::VisualLine,
            });
            enter_normal(state, view);
        }
        gdk::Key::greater => {
            let (start, end) = visual_selection_range(state, &buf);
            let range = MotionRange { start, end, linewise: true };
            buf.begin_user_action();
            operators::op_indent(&buf, &range);
            buf.end_user_action();
            enter_normal(state, view);
        }
        gdk::Key::less => {
            let (start, end) = visual_selection_range(state, &buf);
            let range = MotionRange { start, end, linewise: true };
            buf.begin_user_action();
            operators::op_unindent(&buf, &range);
            buf.end_user_action();
            enter_normal(state, view);
        }
        _ => {}
    }

    glib::Propagation::Stop
}

fn extend_visual_selection(
    state: &Rc<RefCell<VimState>>,
    buf: &sourceview5::Buffer,
    motion_fn: impl FnOnce(&sourceview5::Buffer) -> MotionRange,
) {
    let range = motion_fn(buf);
    // Move cursor to the motion's end, keeping the anchor
    buf.place_cursor(&range.end);
    // Now select from anchor to new cursor
    let anchor_mark = state.borrow().visual_anchor.clone();
    if let Some(anchor_mark) = anchor_mark {
        let anchor = buf.iter_at_mark(&anchor_mark);
        let cursor = buf.iter_at_mark(&buf.get_insert());

        let is_visual_line = state.borrow().mode == VimMode::VisualLine;
        if is_visual_line {
            // Extend to full lines
            let mut sel_start = anchor;
            let mut sel_end = cursor;
            if sel_start > sel_end {
                std::mem::swap(&mut sel_start, &mut sel_end);
            }
            sel_start.set_line_offset(0);
            if !sel_end.ends_line() {
                sel_end.forward_to_line_end();
            }
            sel_end.forward_char(); // include newline
            buf.select_range(&sel_end, &sel_start);
        } else {
            buf.select_range(&cursor, &anchor);
        }
    }
}

fn visual_selection_range(
    state: &Rc<RefCell<VimState>>,
    buf: &sourceview5::Buffer,
) -> (gtk4::TextIter, gtk4::TextIter) {
    let anchor_mark = state.borrow().visual_anchor.clone();
    if let Some(anchor_mark) = anchor_mark {
        let anchor = buf.iter_at_mark(&anchor_mark);
        let cursor = buf.iter_at_mark(&buf.get_insert());
        if anchor < cursor {
            (anchor, cursor)
        } else {
            (cursor, anchor)
        }
    } else {
        let cursor = buf.iter_at_mark(&buf.get_insert());
        (cursor, cursor)
    }
}

// ─── Command mode ───────────────────────────────────────────────

fn handle_command(
    state: &Rc<RefCell<VimState>>,
    view: &View,
    search: &Rc<SearchPanel>,
    key: gdk::Key,
) -> glib::Propagation {
    match key {
        gdk::Key::Escape => {
            enter_normal(state, view);
            return glib::Propagation::Stop;
        }
        gdk::Key::Return => {
            let cmd = state.borrow().command_buf.clone();
            let result = commands::execute_command(&cmd);
            let buf = view.buffer()
                .downcast::<sourceview5::Buffer>()
                .expect("Buffer should be sourceview5::Buffer");
            match result {
                CommandResult::Quit => {
                    if let Some(root) = view.root() {
                        if let Some(window) = root.downcast_ref::<gtk4::Window>() {
                            window.close();
                        }
                    }
                }
                CommandResult::GotoLine(n) => {
                    let line = (n - 1).max(0);
                    if let Some(iter) = buf.iter_at_line(line) {
                        buf.place_cursor(&iter);
                        view.scroll_to_iter(&mut buf.iter_at_mark(&buf.get_insert()), 0.1, false, 0.0, 0.5);
                    }
                }
                CommandResult::Substitute(command) => {
                    if command.whole_file {
                        search.replace_whole_file_query(
                            &command.pattern,
                            &command.replacement,
                            command.global,
                        );
                    } else {
                        search.replace_current_line_query(
                            &command.pattern,
                            &command.replacement,
                            command.global,
                        );
                    }
                    state.borrow_mut().last_search = Some(command.pattern);
                    state.borrow_mut().last_search_forward = true;
                }
                CommandResult::Error(_msg) => {
                    // Could display error in status bar
                }
                CommandResult::None => {}
            }
            enter_normal(state, view);
            return glib::Propagation::Stop;
        }
        gdk::Key::BackSpace => {
            state.borrow_mut().command_buf.pop();
            return glib::Propagation::Stop;
        }
        _ => {
            if let Some(c) = key.to_unicode() {
                state.borrow_mut().command_buf.push(c);
            }
            return glib::Propagation::Stop;
        }
    }
}

fn handle_search(
    state: &Rc<RefCell<VimState>>,
    view: &View,
    search: &Rc<SearchPanel>,
    key: gdk::Key,
) -> glib::Propagation {
    match key {
        gdk::Key::Escape => {
            enter_normal(state, view);
            glib::Propagation::Stop
        }
        gdk::Key::Return => {
            let query = state.borrow().command_buf.clone();
            if !query.is_empty() {
                let forward = state.borrow().mode == VimMode::SearchForward;
                search.search_from_cursor(&query, forward);
                state.borrow_mut().last_search = Some(query);
                state.borrow_mut().last_search_forward = forward;
            }
            enter_normal(state, view);
            glib::Propagation::Stop
        }
        gdk::Key::BackSpace => {
            state.borrow_mut().command_buf.pop();
            glib::Propagation::Stop
        }
        _ => {
            if let Some(c) = key.to_unicode() {
                state.borrow_mut().command_buf.push(c);
            }
            glib::Propagation::Stop
        }
    }
}

// ─── Mode transitions ───────────────────────────────────────────

fn enter_normal(state: &Rc<RefCell<VimState>>, view: &View) {
    let was_insert = state.borrow().mode == VimMode::Insert;
    state.borrow_mut().mode = VimMode::Normal;
    state.borrow_mut().pending_op = None;
    state.borrow_mut().pending_key = PendingKey::None;
    state.borrow_mut().command_buf.clear();
    view.set_editable(false);
    view.set_cursor_visible(true);

    // Move cursor back one if we were in insert mode (vim behavior)
    if was_insert {
        let buf = view.buffer();
        let mut iter = buf.iter_at_mark(&buf.get_insert());
        if !iter.starts_line() {
            iter.backward_char();
            buf.place_cursor(&iter);
        }
    }
}

fn enter_insert(state: &Rc<RefCell<VimState>>, view: &View) {
    state.borrow_mut().mode = VimMode::Insert;
    state.borrow_mut().pending_op = None;
    state.borrow_mut().pending_key = PendingKey::None;
    view.set_editable(true);
    view.set_cursor_visible(true);
}

fn enter_visual(state: &Rc<RefCell<VimState>>, buf: &sourceview5::Buffer, mode: VimMode) {
    // Set anchor at current cursor position
    let cursor = buf.iter_at_mark(&buf.get_insert());
    let mark = buf.create_mark(Some("vim_visual_anchor"), &cursor, true);
    state.borrow_mut().visual_anchor = Some(mark);
    state.borrow_mut().mode = mode;
}

// ─── Apply motion or pending operator ───────────────────────────

fn apply_motion_or_op(
    state: &Rc<RefCell<VimState>>,
    buf: &sourceview5::Buffer,
    range: MotionRange,
) {
    let pending = state.borrow().pending_op;
    match pending {
        None => {
            // Pure motion — just move cursor
            buf.place_cursor(&range.end);
        }
        Some(PendingOp::Delete) => {
            buf.begin_user_action();
            let deleted = operators::op_delete(buf, &range);
            buf.end_user_action();
            state.borrow_mut().register = Some(Register { text: deleted, linewise: range.linewise });
            state.borrow_mut().pending_op = None;
        }
        Some(PendingOp::Change) => {
            buf.begin_user_action();
            let deleted = operators::op_delete(buf, &range);
            buf.end_user_action();
            state.borrow_mut().register = Some(Register { text: deleted, linewise: range.linewise });
            state.borrow_mut().pending_op = None;
            // Change enters insert mode — we need the view reference
            // This is handled by returning and checking in the caller
        }
        Some(PendingOp::Yank) => {
            let yanked = operators::op_yank(buf, &range);
            state.borrow_mut().register = Some(Register { text: yanked, linewise: range.linewise });
            state.borrow_mut().pending_op = None;
        }
        Some(PendingOp::Indent) => {
            buf.begin_user_action();
            operators::op_indent(buf, &range);
            buf.end_user_action();
            state.borrow_mut().pending_op = None;
        }
        Some(PendingOp::Unindent) => {
            buf.begin_user_action();
            operators::op_unindent(buf, &range);
            buf.end_user_action();
            state.borrow_mut().pending_op = None;
        }
    }
}
