use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Entry, Label, Revealer, RevealerTransitionType, ScrolledWindow, SearchEntry};
use sourceview5::prelude::*;
use sourceview5::{Buffer, LanguageManager, SearchContext, SearchSettings, View};
use std::rc::Rc;

use crate::vim::VimHandler;

pub struct SearchPanel {
    root: Revealer,
    replace_row: Revealer,
    find_entry: SearchEntry,
    replace_entry: Entry,
    status_label: Label,
    search_settings: SearchSettings,
    search_context: SearchContext,
    buffer: Buffer,
    view: View,
}

impl SearchPanel {
    fn new(buffer: &Buffer, view: &View) -> Rc<Self> {
        let search_settings = SearchSettings::new();
        search_settings.set_wrap_around(true);

        let search_context = SearchContext::new(buffer, Some(&search_settings));
        search_context.set_highlight(true);

        let root = Revealer::builder()
            .transition_type(RevealerTransitionType::SlideDown)
            .reveal_child(false)
            .build();

        let panel = GtkBox::new(gtk4::Orientation::Vertical, 6);
        panel.add_css_class("editor-search-bar");
        panel.set_margin_start(12);
        panel.set_margin_end(12);
        panel.set_margin_top(12);
        panel.set_margin_bottom(6);

        let search_row = GtkBox::new(gtk4::Orientation::Horizontal, 6);
        let find_entry = SearchEntry::new();
        find_entry.set_hexpand(true);
        find_entry.set_placeholder_text(Some("Search"));
        let prev_button = Button::from_icon_name("go-up-symbolic");
        prev_button.set_tooltip_text(Some("Previous match"));
        let next_button = Button::from_icon_name("go-down-symbolic");
        next_button.set_tooltip_text(Some("Next match"));
        let replace_toggle = Button::from_icon_name("edit-find-replace-symbolic");
        replace_toggle.set_tooltip_text(Some("Show replace"));
        let close_button = Button::from_icon_name("window-close-symbolic");
        close_button.set_tooltip_text(Some("Close search"));
        let status_label = Label::new(None);
        status_label.add_css_class("dim-label");
        search_row.append(&find_entry);
        search_row.append(&status_label);
        search_row.append(&prev_button);
        search_row.append(&next_button);
        search_row.append(&replace_toggle);
        search_row.append(&close_button);

        let replace_row = Revealer::builder()
            .transition_type(RevealerTransitionType::SlideDown)
            .reveal_child(false)
            .build();
        let replace_box = GtkBox::new(gtk4::Orientation::Horizontal, 6);
        let replace_entry = Entry::new();
        replace_entry.set_hexpand(true);
        replace_entry.set_placeholder_text(Some("Replace"));
        let replace_button = Button::with_label("Replace");
        let replace_all_button = Button::with_label("Replace All");
        replace_box.append(&replace_entry);
        replace_box.append(&replace_button);
        replace_box.append(&replace_all_button);
        replace_row.set_child(Some(&replace_box));

        panel.append(&search_row);
        panel.append(&replace_row);
        root.set_child(Some(&panel));

        let search = Rc::new(Self {
            root,
            replace_row,
            find_entry,
            replace_entry,
            status_label,
            search_settings,
            search_context,
            buffer: buffer.clone(),
            view: view.clone(),
        });

        {
            let search = search.clone();
            let find_entry = search.find_entry.clone();
            find_entry.connect_search_changed(move |entry| {
                let text = entry.text();
                let query = if text.is_empty() { None } else { Some(text.as_str()) };
                search.search_settings.set_search_text(query);
                search.update_status();
                if query.is_some() {
                    search.find_next();
                }
            });
        }

        {
            let search = search.clone();
            let key_ctrl = gtk4::EventControllerKey::new();
            let search_for_keys = search.clone();
            key_ctrl.connect_key_pressed(move |_, key, _, state| match key {
                gtk4::gdk::Key::Escape => {
                    search_for_keys.close();
                    glib::Propagation::Stop
                }
                gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter => {
                    if state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
                        search_for_keys.find_previous();
                    } else {
                        search_for_keys.find_next();
                    }
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            });
            search.find_entry.add_controller(key_ctrl);
        }

        {
            let search = search.clone();
            prev_button.connect_clicked(move |_| search.find_previous());
        }
        {
            let search = search.clone();
            next_button.connect_clicked(move |_| search.find_next());
        }
        {
            let search = search.clone();
            replace_toggle.connect_clicked(move |_| {
                let show = !search.replace_row.reveals_child();
                search.replace_row.set_reveal_child(show);
                if show {
                    search.replace_entry.grab_focus();
                }
            });
        }
        {
            let search = search.clone();
            close_button.connect_clicked(move |_| search.close());
        }
        {
            let search = search.clone();
            replace_button.connect_clicked(move |_| search.replace_current());
        }
        {
            let search = search.clone();
            replace_all_button.connect_clicked(move |_| search.replace_all());
        }
        {
            let search = search.clone();
            let search_context = search.search_context.clone();
            search_context.connect_occurrences_count_notify(move |_| search.update_status());
        }

        search
    }

    pub fn widget(&self) -> &Revealer {
        &self.root
    }

    pub fn open_find(&self) {
        self.open(false);
    }

    pub fn open_replace(&self) {
        self.open(true);
    }

    pub fn set_query_text(&self, query: &str) {
        self.find_entry.set_text(query);
        self.search_settings
            .set_search_text((!query.is_empty()).then_some(query));
        self.update_status();
    }

    fn open(&self, show_replace: bool) {
        self.root.set_reveal_child(true);
        self.replace_row
            .set_reveal_child(show_replace && self.view.is_editable());

        if let Some((start, end)) = self.buffer.selection_bounds() {
            let selected = self.buffer.text(&start, &end, false).to_string();
            if !selected.is_empty() && !selected.contains('\n') {
                self.set_query_text(&selected);
            }
        }

        self.update_status();
        self.find_entry.grab_focus();
        self.find_entry.select_region(0, -1);
    }

    pub fn close(&self) {
        self.root.set_reveal_child(false);
        self.replace_row.set_reveal_child(false);
        self.search_settings.set_search_text(None);
        self.status_label.set_text("");
        self.view.grab_focus();
    }

    pub fn has_query(&self) -> bool {
        self.search_settings
            .search_text()
            .map(|text| !text.is_empty())
            .unwrap_or(false)
    }

    fn select_match(&self, start: &gtk4::TextIter, end: &gtk4::TextIter) {
        let mut start = *start;
        let end = *end;
        self.buffer.select_range(&start, &end);
        self.buffer.place_cursor(&start);
        self.view.scroll_to_iter(&mut start, 0.2, false, 0.0, 0.0);
        self.update_status();
    }

    pub fn find_next(&self) {
        if !self.has_query() {
            return;
        }

        let iter = if let Some((_, end)) = self.buffer.selection_bounds() {
            end
        } else {
            self.buffer.iter_at_mark(&self.buffer.get_insert())
        };

        if let Some((start, end, _)) = self.search_context.forward(&iter) {
            self.select_match(&start, &end);
        } else {
            self.update_status();
        }
    }

    pub fn find_previous(&self) {
        if !self.has_query() {
            return;
        }

        let iter = if let Some((start, _)) = self.buffer.selection_bounds() {
            start
        } else {
            self.buffer.iter_at_mark(&self.buffer.get_insert())
        };

        if let Some((start, end, _)) = self.search_context.backward(&iter) {
            self.select_match(&start, &end);
        } else {
            self.update_status();
        }
    }

    fn current_match_bounds(&self) -> Option<(gtk4::TextIter, gtk4::TextIter)> {
        let (start, end) = self.buffer.selection_bounds()?;
        let selected = self.buffer.text(&start, &end, false).to_string();
        let search_text = self.search_settings.search_text()?.to_string();
        if selected == search_text {
            Some((start, end))
        } else {
            None
        }
    }

    pub fn replace_current(&self) {
        if !self.has_query() || !self.view.is_editable() {
            return;
        }

        if let Some((mut start, mut end)) = self.current_match_bounds() {
            let replacement = self.replace_entry.text().to_string();
            if self.search_context.replace(&mut start, &mut end, &replacement).is_ok() {
                self.find_next();
            }
        } else {
            self.find_next();
        }
    }

    pub fn replace_all(&self) {
        if !self.has_query() || !self.view.is_editable() {
            return;
        }

        let replacement = self.replace_entry.text().to_string();
        let _ = self.search_context.replace_all(&replacement);
        self.update_status();
    }

    pub fn search_from_cursor(&self, query: &str, forward: bool) {
        self.set_query_text(query);
        if forward {
            let iter = self.buffer.iter_at_mark(&self.buffer.get_insert());
            if let Some((start, end, _)) = self.search_context.forward(&iter) {
                self.select_match(&start, &end);
            }
        } else {
            let iter = self.buffer.iter_at_mark(&self.buffer.get_insert());
            if let Some((start, end, _)) = self.search_context.backward(&iter) {
                self.select_match(&start, &end);
            }
        }
    }

    pub fn replace_whole_file_query(&self, query: &str, replacement: &str, global: bool) {
        if query.is_empty() || !self.view.is_editable() {
            return;
        }

        let start = self.buffer.start_iter();
        let end = self.buffer.end_iter();
        let text = self.buffer.text(&start, &end, false).to_string();
        let replaced = replace_text_by_line(&text, query, replacement, global);

        if replaced != text {
            let mut delete_start = self.buffer.start_iter();
            let mut delete_end = self.buffer.end_iter();
            self.buffer.begin_user_action();
            self.buffer.delete(&mut delete_start, &mut delete_end);
            self.buffer.insert(&mut delete_start, &replaced);
            self.buffer.end_user_action();
        }

        self.set_query_text(query);
    }

    pub fn replace_current_line_query(&self, query: &str, replacement: &str, global: bool) {
        if query.is_empty() || !self.view.is_editable() {
            return;
        }

        let mut start = self.buffer.iter_at_mark(&self.buffer.get_insert());
        start.set_line_offset(0);
        let mut end = start;
        end.forward_to_line_end();

        let line = self.buffer.text(&start, &end, false).to_string();
        let replaced = if global {
            line.replace(query, replacement)
        } else {
            line.replacen(query, replacement, 1)
        };

        if replaced != line {
            let mut delete_start = start;
            let mut delete_end = end;
            self.buffer.begin_user_action();
            self.buffer.delete(&mut delete_start, &mut delete_end);
            self.buffer.insert(&mut delete_start, &replaced);
            self.buffer.end_user_action();
        }

        self.set_query_text(query);
    }

    fn update_status(&self) {
        let Some(search_text) = self.search_settings.search_text() else {
            self.status_label.set_text("");
            return;
        };
        if search_text.is_empty() {
            self.status_label.set_text("");
            return;
        }

        let total = self.search_context.occurrences_count();
        if total <= 0 {
            self.status_label.set_text("No matches");
            return;
        }

        if let Some((start, end)) = self.current_match_bounds() {
            let current = self.search_context.occurrence_position(&start, &end);
            if current > 0 {
                self.status_label.set_text(&format!("{current}/{total}"));
                return;
            }
        }

        self.status_label.set_text(&format!("{total} matches"));
    }
}

fn replace_text_by_line(text: &str, query: &str, replacement: &str, global: bool) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            if global {
                line.replace(query, replacement)
            } else {
                line.replacen(query, replacement, 1)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::replace_text_by_line;

    #[test]
    fn replaces_first_match_per_line() {
        let text = "foo foo\nfoo\nbar foo";
        let replaced = replace_text_by_line(text, "foo", "x", false);
        assert_eq!(replaced, "x foo\nx\nbar x");
    }

    #[test]
    fn replaces_all_matches_per_line() {
        let text = "foo foo\nfoo\nbar foo";
        let replaced = replace_text_by_line(text, "foo", "x", true);
        assert_eq!(replaced, "x x\nx\nbar x");
    }
}

/// Holds the editor widget and its state.
pub struct Editor {
    pub scrolled_window: ScrolledWindow,
    pub view: View,
    pub buffer: Buffer,
    pub container: gtk4::Box,
    pub vim_handler: VimHandler,
    pub search: Rc<SearchPanel>,
}

impl Editor {
    pub fn new() -> Self {
        // Create a new default GtkSourceView buffer with markdown highlighting
        let buffer = Buffer::new(None);
        buffer.set_enable_undo(true);
        buffer.set_max_undo_levels(1024);

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

        let search = SearchPanel::new(&buffer, &view);

        // Container: search bar + scrolled editor + status bar
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.append(search.widget());
        container.append(&scrolled_window);
        container.append(&status_label);

        // Install vim handler
        let vim_handler = VimHandler::new(&view, status_label, search.clone());

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
            search,
        }
    }
}
