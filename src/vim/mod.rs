use gtk4::gdk;
use gtk4::prelude::*;
use sourceview5::{Buffer, View};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::editor::SearchPanel;

use async_trait::async_trait;
use nvim_rs::{create::tokio as create, Handler, Neovim, Value};
use tokio::process::Command;
use tokio::runtime::Runtime;

// ─── Neovim Event Communication ──────────────────────────────────

#[derive(Debug)]
pub enum NvimEvent {
    SyncText { full_text: String },
    ModeChanged { mode: String },
    CursorMoved { row: i64, col: i64 },
}

#[derive(Clone)]
struct NvimHandler { }

#[async_trait]
impl Handler for NvimHandler {
    type Writer = tokio_util::compat::Compat<tokio::process::ChildStdin>;
}

// ─── VimHandler ──────────────────────────────────────────────────

pub struct VimHandler {
    pub enabled: Rc<RefCell<bool>>,
    key_tx: tokio::sync::mpsc::UnboundedSender<String>,
    _runtime: Rc<Runtime>,
    sync_guard: Rc<RefCell<bool>>,
    status_label: gtk4::Label,
}

impl VimHandler {
    pub fn new(view: &View, status_label: gtk4::Label, _search: Rc<SearchPanel>) -> Self {
        let enabled = Rc::new(RefCell::new(true));

        let (key_tx, mut key_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let sync_guard = Rc::new(RefCell::new(false));
        // Initialize tokio runtime and keep it alive
        let runtime = Rc::new(Runtime::new().unwrap());

        // Default startup in vim mode:
        view.set_editable(false);
        view.set_cursor_visible(true);

        let (tx_main, mut rx_main) = tokio::sync::mpsc::unbounded_channel::<NvimEvent>();

        // Attach receiver to GTK main loop via local future
        {
            let view_clone = view.clone();
            let sync_guard_clone = sync_guard.clone();
            let status_label_clone = status_label.clone();

            gtk4::glib::spawn_future_local(async move {
                while let Some(evt) = rx_main.recv().await {
                    match evt {
                        NvimEvent::SyncText { full_text } => {
                            let buf = view_clone.buffer().downcast::<Buffer>().unwrap();
                            let start = buf.start_iter();
                            let end = buf.end_iter();
                            let current_text = buf.text(&start, &end, false).to_string();

                            if current_text != full_text {
                                *sync_guard_clone.borrow_mut() = true;
                                buf.begin_user_action();
                                buf.set_text(&full_text);
                                buf.end_user_action();
                                *sync_guard_clone.borrow_mut() = false;
                            }
                        }
                        NvimEvent::ModeChanged { mode } => {
                            if status_label_clone.text() != mode {
                                status_label_clone.set_text(&mode);
                            }
                        }
                        NvimEvent::CursorMoved { row, col } => {
                            let buf = view_clone.buffer().downcast::<Buffer>().unwrap();
                            if let Some(mut iter) = buf.iter_at_line(row as i32) {
                                iter.set_line_index(col as i32);

                                let current_mark = buf.get_insert();
                                let current_iter = buf.iter_at_mark(&current_mark);
                                
                                if current_iter.line() != iter.line() || current_iter.line_index() != iter.line_index() {
                                    buf.place_cursor(&iter);
                                    view_clone.scroll_to_iter(&mut iter, 0.0, false, 0.0, 0.0);
                                }
                            }
                        }
                    }
                }
            });
        }

        // Spawn neovim in background
        {
            let tx_main_bg = tx_main.clone();
            let view_clone2 = view.clone();

            let buf2 = view_clone2.buffer().downcast::<Buffer>().unwrap();
            let start = buf2.start_iter();
            let end = buf2.end_iter();
            let init_text = buf2.text(&start, &end, false).to_string();

            runtime.spawn(async move {
                let mut cmd = Command::new("nvim");
                cmd.args(&["--embed", "--headless", "-n"]);

                if let Ok((nvim_proc, io_handler, _child)) =
                    create::new_child_cmd(&mut cmd, NvimHandler {}).await
                {
                    tokio::spawn(io_handler);

                    let lines: Vec<Value> = init_text.split('\n').map(|s| Value::from(s)).collect();
                    let lines_val = Value::from(lines);
                    
                    let _res1: Result<Result<Value, Value>, _> = nvim_proc
                        .call(
                            "nvim_buf_set_lines",
                            vec![
                                Value::from(0),
                                Value::from(0),
                                Value::from(-1),
                                Value::from(false),
                                lines_val,
                            ],
                        )
                        .await;

                    // key input processing loop
                    let nvim_proc_input = nvim_proc.clone();
                    tokio::spawn(async move {
                        while let Some(key) = key_rx.recv().await {
                            let _res2: Result<Result<Value, Value>, _> = nvim_proc_input
                                .call(
                                    "nvim_input",
                                    vec![Value::from(key.as_str())],
                                )
                                .await;
                        }
                    });

                    // State synchronization polling loop
                    tokio::spawn(async move {
                        loop {
                            // 1. Mode
                            let mode_res: Result<Result<Value, Value>, _> =
                                nvim_proc.call("nvim_get_mode", vec![]).await;
                            
                            if let Ok(Ok(m_val)) = mode_res {
                                if let Some(m) = m_val.as_map() {
                                    for (k, v) in m {
                                        if let Some(key_str) = k.as_str() {
                                            if key_str == "mode" {
                                                if let Some(s) = v.as_str() {
                                                    let display = match s {
                                                        "n" => "-- NORMAL --",
                                                        "i" => "-- INSERT --",
                                                        "v" | "V" | "\x16" => "-- VISUAL --",
                                                        "c" => "-- COMMAND --",
                                                        "R" => "-- REPLACE --",
                                                        _ => "-- NORMAL --",
                                                    };
                                                    let _ = tx_main_bg.send(NvimEvent::ModeChanged {
                                                        mode: display.to_string(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // 2. Text
                            let lines_res: Result<Result<Value, Value>, _> = nvim_proc
                                .call(
                                    "nvim_buf_get_lines",
                                    vec![
                                        Value::from(0),
                                        Value::from(0),
                                        Value::from(-1),
                                        Value::from(false),
                                    ],
                                )
                                .await;

                            if let Ok(Ok(lines_val)) = lines_res {
                                if let Some(arr) = lines_val.as_array() {
                                    let mut text_lines = Vec::with_capacity(arr.len());
                                    for l in arr {
                                        if let Some(s) = l.as_str() {
                                            text_lines.push(s);
                                        }
                                    }
                                    let full_text = text_lines.join("\n");
                                    let _ = tx_main_bg.send(NvimEvent::SyncText { full_text });
                                }
                            }

                            // 3. Cursor
                            let win_res: Result<Result<Value, Value>, _> = nvim_proc
                                .call("nvim_get_current_win", vec![])
                                .await;
                                
                            if let Ok(Ok(win)) = win_res {
                                let cursor_res: Result<Result<Value, Value>, _> = nvim_proc
                                    .call(
                                        "nvim_win_get_cursor",
                                        vec![win],
                                    )
                                    .await;
                                
                                if let Ok(Ok(cursor_val)) = cursor_res {
                                    if let Some(cursor) = cursor_val.as_array() {
                                        if cursor.len() == 2 {
                                            let row = cursor[0].as_i64().unwrap_or(1) - 1;
                                            let col = cursor[1].as_i64().unwrap_or(0);
                                            let _ = tx_main_bg.send(NvimEvent::CursorMoved { row, col });
                                        }
                                    }
                                }
                            }

                            tokio::time::sleep(Duration::from_millis(30)).await;
                        }
                    });
                }
            });
        }

        let vim_handler = VimHandler {
            enabled,
            key_tx,
            _runtime: runtime,
            sync_guard,
            status_label,
        };

        let key_ctrl = gtk4::EventControllerKey::new();
        key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);

        let enabled_cb = vim_handler.enabled.clone();
        let key_tx_cb = vim_handler.key_tx.clone();

        key_ctrl.connect_key_pressed(move |_, key, _keycode, modifiers| {
            if !*enabled_cb.borrow() {
                return gtk4::glib::Propagation::Proceed;
            }

            if let Some(nvim_key) = gdk_key_to_nvim(key, modifiers) {
                let _ = key_tx_cb.send(nvim_key);
            }

            // Always stop propagation to prevent GTK from editing the buffer while in Vim Mode
            gtk4::glib::Propagation::Stop
        });

        view.add_controller(key_ctrl);

        vim_handler
    }

    pub fn set_enabled(&self, view: &View, enabled: bool) {
        *self.enabled.borrow_mut() = enabled;
        if enabled {
            view.set_editable(false);
            self.status_label.set_visible(true);
        } else {
            view.set_editable(true);
            self.status_label.set_visible(false);
        }
    }
}

pub fn gdk_key_to_nvim(key: gdk::Key, modifiers: gdk::ModifierType) -> Option<String> {
    let mut modifier_str = String::new();
    if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
        modifier_str.push_str("C-");
    }
    if modifiers.contains(gdk::ModifierType::ALT_MASK) {
        modifier_str.push_str("M-");
    }

    let key_str = match key {
        gdk::Key::Escape => "Esc".to_string(),
        gdk::Key::Return | gdk::Key::KP_Enter => "CR".to_string(),
        gdk::Key::Tab => "Tab".to_string(),
        gdk::Key::BackSpace => "BS".to_string(),
        gdk::Key::Up => "Up".to_string(),
        gdk::Key::Down => "Down".to_string(),
        gdk::Key::Left => "Left".to_string(),
        gdk::Key::Right => "Right".to_string(),
        gdk::Key::Home => "Home".to_string(),
        gdk::Key::End => "End".to_string(),
        gdk::Key::Page_Up => "PageUp".to_string(),
        gdk::Key::Page_Down => "PageDown".to_string(),
        gdk::Key::F1 => "F1".to_string(),
        gdk::Key::F2 => "F2".to_string(),
        gdk::Key::F3 => "F3".to_string(),
        gdk::Key::F4 => "F4".to_string(),
        gdk::Key::F5 => "F5".to_string(),
        gdk::Key::F6 => "F6".to_string(),
        gdk::Key::F7 => "F7".to_string(),
        gdk::Key::F8 => "F8".to_string(),
        gdk::Key::F9 => "F9".to_string(),
        gdk::Key::F10 => "F10".to_string(),
        gdk::Key::F11 => "F11".to_string(),
        gdk::Key::F12 => "F12".to_string(),
        gdk::Key::less => "lt".to_string(),
        gdk::Key::greater => "gt".to_string(),
        gdk::Key::space => "Space".to_string(),
        gdk::Key::slash => "/".to_string(),
        gdk::Key::question => "?".to_string(),
        gdk::Key::colon => ":".to_string(),
        k => {
            if let Some(c) = k.to_unicode() {
                if c == '<' {
                    "lt".to_string()
                } else if c == ' ' {
                    "Space".to_string()
                } else if c == '\\' {
                    "Bslash".to_string()
                } else {
                    c.to_string()
                }
            } else {
                return None;
            }
        }
    };

    if modifier_str.is_empty() && key_str.len() == 1 {
        Some(key_str)
    } else {
        Some(format!("<{}{}>", modifier_str, key_str))
    }
}
