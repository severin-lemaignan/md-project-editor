use gtk4::prelude::*;
use gtk4::{EventControllerKey, ScrolledWindow};
use sourceview5::prelude::*;
use sourceview5::{Buffer, LanguageManager, View};
use std::cell::Cell;
use std::rc::Rc;

/// Editor modes for future vim-style bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    Normal,
    Insert,
}

/// Holds the editor widget and its state.
pub struct Editor {
    pub scrolled_window: ScrolledWindow,
    pub view: View,
    pub buffer: Buffer,
    pub mode: Rc<Cell<EditorMode>>,
}

impl Editor {
    pub fn new() -> Self {
        // Create a GtkSourceView buffer with markdown highlighting
        let buffer = Buffer::new(None);

        // Try to set markdown language for syntax highlighting
        let lang_manager = LanguageManager::default();
        if let Some(lang) = lang_manager.language("markdown") {
            buffer.set_language(Some(&lang));
        }

        // Use a dark source style scheme
        let scheme_manager = sourceview5::StyleSchemeManager::default();
        if let Some(scheme) = scheme_manager.scheme("Adwaita-dark") {
            buffer.set_style_scheme(Some(&scheme));
        }

        // Create the source view
        let view = View::with_buffer(&buffer);
        view.set_monospace(true);
        view.set_show_line_numbers(true);
        view.set_tab_width(4);
        view.set_auto_indent(true);
        view.set_indent_width(4);
        view.set_highlight_current_line(true);
        view.set_wrap_mode(gtk4::WrapMode::Word);
        view.set_top_margin(12);
        view.set_bottom_margin(12);
        view.set_left_margin(12);
        view.set_right_margin(12);

        // Larger, comfortable font — apply via CSS class
        view.add_css_class("editor-view");

        // Wrap in a ScrolledWindow
        let scrolled_window = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .child(&view)
            .build();

        // Mode state
        let mode = Rc::new(Cell::new(EditorMode::Insert));

        // Install key controller on the view for modal editing
        let key_ctrl = EventControllerKey::new();
        let mode_clone = mode.clone();
        let view_clone = view.clone();
        key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
        key_ctrl.connect_key_pressed(move |_, key, _keycode, _modifiers| {
            let current_mode = mode_clone.get();
            match current_mode {
                EditorMode::Normal => {
                    // In Normal mode, consume most keys
                    match key {
                        gtk4::gdk::Key::i => {
                            // Switch to Insert mode
                            mode_clone.set(EditorMode::Insert);
                            view_clone.set_editable(true);
                            // Update cursor style
                            view_clone.set_cursor_visible(true);
                            glib::Propagation::Stop
                        }
                        gtk4::gdk::Key::a => {
                            // Append: move cursor forward one, enter insert
                            mode_clone.set(EditorMode::Insert);
                            view_clone.set_editable(true);
                            view_clone.set_cursor_visible(true);
                            // Move cursor forward
                            let buf = view_clone.buffer();
                            let mut iter = buf.iter_at_mark(&buf.get_insert());
                            iter.forward_char();
                            buf.place_cursor(&iter);
                            glib::Propagation::Stop
                        }
                        _ => {
                            // Suppress all other key input in Normal mode
                            glib::Propagation::Stop
                        }
                    }
                }
                EditorMode::Insert => {
                    match key {
                        gtk4::gdk::Key::Escape => {
                            // Switch to Normal mode
                            mode_clone.set(EditorMode::Normal);
                            view_clone.set_editable(false);
                            view_clone.set_cursor_visible(true);
                            glib::Propagation::Stop
                        }
                        _ => {
                            // Normal typing in Insert mode
                            glib::Propagation::Proceed
                        }
                    }
                }
            }
        });
        view.add_controller(key_ctrl);

        // Seed buffer with example content
        buffer.set_text(
            r#"# Welcome to Agentic MD

A **fast**, *sleek* markdown editor with agentic capabilities.

## Features

- ✏️ Live preview as you type
- 🔄 Synchronized scrolling
- 🎯 Modal editing (press `Esc` for Normal mode, `i` for Insert)
- 🌙 Dark theme

## Getting Started

Start typing markdown here and see it rendered on the right!

```rust
fn main() {
    println!("Hello, Agentic MD!");
}
```

> "The best way to predict the future is to invent it." — Alan Kay

### Lists

1. First item
2. Second item
3. Third item

- Bullet one
- Bullet two
  - Nested bullet

### Table

| Feature | Status |
|---------|--------|
| Editor  | ✅     |
| Preview | ✅     |
| Scroll sync | ✅ |
| Vim mode | 🔜    |

---

*Happy editing!*
"#,
        );

        Editor {
            scrolled_window,
            view,
            buffer,
            mode,
        }
    }
}
