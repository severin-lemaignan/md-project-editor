use gtk4::prelude::*;
use gtk4::ScrolledWindow;
use sourceview5::prelude::*;
use sourceview5::{Buffer, LanguageManager, View};

use crate::vim::VimHandler;

/// Holds the editor widget and its state.
pub struct Editor {
    pub scrolled_window: ScrolledWindow,
    pub view: View,
    pub buffer: Buffer,
    pub container: gtk4::Box,
    pub vim_handler: VimHandler,
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
            .hexpand(true)
            .vexpand(true)
            .build();

        // Status bar for vim mode indicator
        let status_label = gtk4::Label::new(Some("-- NORMAL --"));
        status_label.set_halign(gtk4::Align::Start);
        status_label.set_margin_start(12);
        status_label.set_margin_end(12);
        status_label.set_margin_top(4);
        status_label.set_margin_bottom(4);
        status_label.add_css_class("vim-status");

        // Container: scrolled editor + status bar
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.append(&scrolled_window);
        container.append(&status_label);

        // Install vim handler
        let vim_handler = VimHandler::new(&view, status_label);

        // Seed buffer with example content
        buffer.set_text(
            r#"# Welcome to Agentic MD

A **fast**, *sleek* markdown editor with agentic capabilities.

## Vim Bindings

This editor supports vim keybindings! Try these:

- `h` `j` `k` `l` — move cursor
- `w` / `b` / `e` — word motions
- `0` / `$` / `^` — line motions
- `gg` / `G` — jump to start/end of file
- `i` / `a` / `o` / `O` — enter Insert mode
- `Esc` — back to Normal mode
- `dd` / `yy` / `p` — delete/yank/paste lines
- `dw` / `cw` — delete/change word
- `v` / `V` — visual mode
- `u` — undo
- `:q` — quit

## Features

- ✏️ Live preview as you type
- 🔄 Synchronized scrolling
- 🎯 Full vim keybindings
- 🌙 Dark theme

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
| Vim mode | ✅    |

---

*Happy editing!*
"#,
        );

        Editor {
            scrolled_window,
            view,
            buffer,
            container,
            vim_handler,
        }
    }
}
