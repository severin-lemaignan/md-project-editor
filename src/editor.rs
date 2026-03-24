use gtk4::prelude::*;
use gtk4::{
    Box as GtkBox, Button, Entry, GestureClick, Label, Orientation, PolicyType, Revealer,
    RevealerTransitionType, ScrolledWindow, SearchEntry, Spinner, TextWindowType,
};
use sourceview5::prelude::*;
use sourceview5::{Buffer, LanguageManager, SearchContext, SearchSettings, View};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError};

use crate::ai::{
    request_edit_in_background, EditMode, EditRequest, EditResponse, ProviderAvailability,
    SharedAiProvider,
};
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

const AI_CONTEXT_CHARS: i32 = 1200;

#[derive(Clone)]
enum AiPromptMode {
    InsertAtCursor { offset: i32 },
    ReplaceSelection {
        start: i32,
        end: i32,
        selected_text: String,
    },
}

#[derive(Clone)]
enum PendingProposal {
    Insert {
        offset: i32,
        text: String,
    },
    Replace {
        start: i32,
        end: i32,
        original_text: String,
        replacement: String,
    },
}

struct AiAssistant {
    prompt_revealer: Revealer,
    prompt_title: Label,
    prompt_entry: Entry,
    prompt_status: Label,
    prompt_spinner: Spinner,
    prompt_send: Button,
    prompt_cancel: Button,
    review_revealer: Revealer,
    review_title: Label,
    review_status: Label,
    current_label: Label,
    current_buffer: Buffer,
    proposal_buffer: Buffer,
    accept_button: Button,
    reject_button: Button,
    prompt_mode: RefCell<Option<AiPromptMode>>,
    pending_proposal: RefCell<Option<PendingProposal>>,
    provider: Option<SharedAiProvider>,
    provider_error: Option<String>,
    request_generation: Cell<u64>,
    request_poll: RefCell<Option<glib::SourceId>>,
    buffer: Buffer,
    view: View,
}

impl AiAssistant {
    fn new(buffer: &Buffer, view: &View, availability: ProviderAvailability) -> Rc<Self> {
        let prompt_revealer = Revealer::builder()
            .transition_type(RevealerTransitionType::SlideDown)
            .reveal_child(false)
            .build();

        let prompt_box = GtkBox::new(Orientation::Vertical, 6);
        prompt_box.add_css_class("editor-ai-bar");
        prompt_box.set_margin_start(12);
        prompt_box.set_margin_end(12);
        prompt_box.set_margin_top(12);
        prompt_box.set_margin_bottom(6);

        let prompt_row = GtkBox::new(Orientation::Horizontal, 6);
        let prompt_title = Label::new(Some("Ask AI"));
        prompt_title.add_css_class("heading");
        let prompt_entry = Entry::new();
        prompt_entry.set_hexpand(true);
        prompt_entry.set_placeholder_text(Some("Describe the change you want"));
        let prompt_spinner = Spinner::new();
        let prompt_status = Label::new(None);
        prompt_status.add_css_class("dim-label");
        prompt_status.set_hexpand(false);
        let prompt_send = Button::with_label("Send");
        let prompt_cancel = Button::with_label("Cancel");
        prompt_cancel.add_css_class("flat");

        prompt_row.append(&prompt_title);
        prompt_row.append(&prompt_entry);
        prompt_row.append(&prompt_spinner);
        prompt_row.append(&prompt_status);
        prompt_row.append(&prompt_send);
        prompt_row.append(&prompt_cancel);
        prompt_box.append(&prompt_row);
        prompt_revealer.set_child(Some(&prompt_box));

        let review_revealer = Revealer::builder()
            .transition_type(RevealerTransitionType::SlideDown)
            .reveal_child(false)
            .build();

        let review_box = GtkBox::new(Orientation::Vertical, 6);
        review_box.add_css_class("editor-ai-review");
        review_box.set_margin_start(12);
        review_box.set_margin_end(12);
        review_box.set_margin_top(6);
        review_box.set_margin_bottom(6);

        let review_header = GtkBox::new(Orientation::Horizontal, 6);
        let review_title = Label::new(Some("AI Proposal"));
        review_title.add_css_class("heading");
        review_title.set_xalign(0.0);
        review_title.set_hexpand(true);
        let review_status = Label::new(None);
        review_status.add_css_class("dim-label");
        let accept_button = Button::with_label("Accept");
        let reject_button = Button::with_label("Reject");
        reject_button.add_css_class("flat");
        review_header.append(&review_title);
        review_header.append(&review_status);
        review_header.append(&accept_button);
        review_header.append(&reject_button);

        let review_content = GtkBox::new(Orientation::Horizontal, 12);
        let current_column = GtkBox::new(Orientation::Vertical, 4);
        let current_label = Label::new(Some("Current"));
        current_label.add_css_class("dim-label");
        current_label.set_xalign(0.0);
        let current_buffer = Buffer::new(None);
        let current_view = View::with_buffer(&current_buffer);
        configure_review_view(&current_view);
        let current_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .min_content_height(140)
            .child(&current_view)
            .hexpand(true)
            .build();
        current_column.append(&current_label);
        current_column.append(&current_scroll);

        let proposal_column = GtkBox::new(Orientation::Vertical, 4);
        let proposal_label = Label::new(Some("Proposal"));
        proposal_label.add_css_class("dim-label");
        proposal_label.set_xalign(0.0);
        let proposal_buffer = Buffer::new(None);
        let proposal_view = View::with_buffer(&proposal_buffer);
        configure_review_view(&proposal_view);
        let proposal_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .min_content_height(140)
            .child(&proposal_view)
            .hexpand(true)
            .build();
        proposal_column.append(&proposal_label);
        proposal_column.append(&proposal_scroll);

        review_content.append(&current_column);
        review_content.append(&proposal_column);
        review_box.append(&review_header);
        review_box.append(&review_content);
        review_revealer.set_child(Some(&review_box));

        let assistant = Rc::new(Self {
            prompt_revealer,
            prompt_title,
            prompt_entry,
            prompt_status,
            prompt_spinner,
            prompt_send,
            prompt_cancel,
            review_revealer,
            review_title,
            review_status,
            current_label,
            current_buffer,
            proposal_buffer,
            accept_button,
            reject_button,
            prompt_mode: RefCell::new(None),
            pending_proposal: RefCell::new(None),
            provider: availability.provider(),
            provider_error: availability.error_message().map(|msg| msg.to_string()),
            request_generation: Cell::new(0),
            request_poll: RefCell::new(None),
            buffer: buffer.clone(),
            view: view.clone(),
        });

        assistant.install_prompt_actions();
        assistant.install_review_actions();
        assistant.install_context_menu();

        assistant
    }

    fn prompt_widget(&self) -> &Revealer {
        &self.prompt_revealer
    }

    fn review_widget(&self) -> &Revealer {
        &self.review_revealer
    }

    fn install_prompt_actions(self: &Rc<Self>) {
        {
            let assistant = self.clone();
            self.prompt_send.connect_clicked(move |_| assistant.send_request());
        }
        {
            let assistant = self.clone();
            self.prompt_cancel.connect_clicked(move |_| assistant.close_prompt());
        }
        {
            let assistant = self.clone();
            self.prompt_entry.connect_activate(move |_| assistant.send_request());
        }
    }

    fn install_review_actions(self: &Rc<Self>) {
        {
            let assistant = self.clone();
            self.accept_button.connect_clicked(move |_| assistant.accept_proposal());
        }
        {
            let assistant = self.clone();
            self.reject_button.connect_clicked(move |_| assistant.reject_proposal());
        }
    }

    fn install_context_menu(self: &Rc<Self>) {
        let popover = gtk4::Popover::new();
        popover.set_parent(&self.view);

        let menu_box = GtkBox::new(Orientation::Vertical, 0);
        let ask_button = Button::with_label("Ask AI Here");
        ask_button.add_css_class("flat");
        let rewrite_button = Button::with_label("Rewrite Selection");
        rewrite_button.add_css_class("flat");
        menu_box.append(&ask_button);
        menu_box.append(&rewrite_button);
        popover.set_child(Some(&menu_box));
        popover.set_has_arrow(true);

        {
            let assistant = self.clone();
            let popover = popover.clone();
            ask_button.connect_clicked(move |_| {
                popover.popdown();
                assistant.open_insert_prompt();
            });
        }
        {
            let assistant = self.clone();
            let popover = popover.clone();
            rewrite_button.connect_clicked(move |_| {
                popover.popdown();
                assistant.open_replace_prompt();
            });
        }

        let gesture = GestureClick::new();
        gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
        let assistant = self.clone();
        gesture.connect_pressed(move |_, _, x, y| {
            assistant.prepare_context_menu(&popover, &rewrite_button, x, y);
        });
        self.view.add_controller(gesture);
    }

    fn prepare_context_menu(
        &self,
        popover: &gtk4::Popover,
        rewrite_button: &Button,
        x: f64,
        y: f64,
    ) {
        let (buffer_x, buffer_y) =
            self.view
                .window_to_buffer_coords(TextWindowType::Widget, x as i32, y as i32);

        if let Some(iter) = self.view.iter_at_location(buffer_x, buffer_y) {
            let clicked_offset = iter.offset();
            if let Some((start, end)) = self.buffer.selection_bounds() {
                let start_offset = start.offset();
                let end_offset = end.offset();
                if clicked_offset < start_offset || clicked_offset > end_offset {
                    self.buffer.place_cursor(&iter);
                }
            } else {
                self.buffer.place_cursor(&iter);
            }
        }

        rewrite_button.set_visible(self.has_single_line_or_block_selection());
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
            x as i32,
            y as i32,
            1,
            1,
        )));
        popover.popup();
    }

    fn has_single_line_or_block_selection(&self) -> bool {
        self.buffer
            .selection_bounds()
            .map(|(start, end)| start.offset() != end.offset())
            .unwrap_or(false)
    }

    fn open_insert_prompt(&self) {
        let offset = self.buffer.iter_at_mark(&self.buffer.get_insert()).offset();
        self.prompt_mode
            .borrow_mut()
            .replace(AiPromptMode::InsertAtCursor { offset });
        self.prompt_title.set_text("Ask AI Here");
        self.prompt_status.set_text("");
        self.prompt_entry.set_text("");
        self.prompt_entry
            .set_placeholder_text(Some("Describe what should be inserted at the cursor"));
        self.prompt_spinner.stop();
        self.prompt_send.set_sensitive(true);
        self.prompt_revealer.set_reveal_child(true);
        self.prompt_entry.grab_focus();
    }

    fn open_replace_prompt(&self) {
        let Some((start, end)) = self.buffer.selection_bounds() else {
            return;
        };
        let selected_text = self.buffer.text(&start, &end, false).to_string();
        if selected_text.is_empty() {
            return;
        }

        self.prompt_mode.borrow_mut().replace(AiPromptMode::ReplaceSelection {
            start: start.offset(),
            end: end.offset(),
            selected_text,
        });
        self.prompt_title.set_text("Rewrite Selection");
        self.prompt_status.set_text("");
        self.prompt_entry.set_text("");
        self.prompt_entry
            .set_placeholder_text(Some("Describe how the selection should be changed"));
        self.prompt_spinner.stop();
        self.prompt_send.set_sensitive(true);
        self.prompt_revealer.set_reveal_child(true);
        self.prompt_entry.grab_focus();
    }

    fn close_prompt(&self) {
        self.bump_request_generation();
        self.prompt_mode.borrow_mut().take();
        self.prompt_spinner.stop();
        self.prompt_send.set_sensitive(true);
        self.prompt_status.set_text("");
        self.prompt_revealer.set_reveal_child(false);
        self.view.grab_focus();
    }

    fn reject_proposal(&self) {
        self.pending_proposal.borrow_mut().take();
        self.review_revealer.set_reveal_child(false);
        self.review_status.set_text("");
        self.view.grab_focus();
    }

    fn send_request(self: &Rc<Self>) {
        let instruction = self.prompt_entry.text().trim().to_string();
        if instruction.is_empty() {
            self.prompt_status.set_text("Enter a prompt.");
            return;
        }

        let Some(prompt_mode) = self.prompt_mode.borrow().clone() else {
            self.prompt_status.set_text("No AI action is active.");
            return;
        };

        let Some(provider) = self.provider.clone() else {
            self.prompt_status.set_text(
                self.provider_error
                    .as_deref()
                    .unwrap_or("AI provider is not configured."),
            );
            return;
        };

        let request = self.build_request(&instruction, prompt_mode);
        let receiver = request_edit_in_background(provider, request);
        let generation = self.bump_request_generation();

        self.prompt_spinner.start();
        self.prompt_send.set_sensitive(false);
        self.prompt_status.set_text("Waiting for AI...");

        self.install_request_poll(receiver, generation);
    }

    fn build_request(&self, instruction: &str, prompt_mode: AiPromptMode) -> EditRequest {
        match prompt_mode {
            AiPromptMode::InsertAtCursor { offset } => {
                let iter = self.buffer.iter_at_offset(offset);
                let context_before = excerpt_before(&self.buffer, &iter, AI_CONTEXT_CHARS);
                let context_after = excerpt_after(&self.buffer, &iter, AI_CONTEXT_CHARS);
                EditRequest {
                    instruction: instruction.to_string(),
                    document_name: None,
                    mode: EditMode::InsertAtCursor,
                    selected_text: None,
                    context_before,
                    context_after,
                }
            }
            AiPromptMode::ReplaceSelection {
                start,
                end,
                selected_text,
            } => {
                let start_iter = self.buffer.iter_at_offset(start);
                let end_iter = self.buffer.iter_at_offset(end);
                let context_before = excerpt_before(&self.buffer, &start_iter, AI_CONTEXT_CHARS);
                let context_after = excerpt_after(&self.buffer, &end_iter, AI_CONTEXT_CHARS);
                EditRequest {
                    instruction: instruction.to_string(),
                    document_name: None,
                    mode: EditMode::ReplaceSelection,
                    selected_text: Some(selected_text),
                    context_before,
                    context_after,
                }
            }
        }
    }

    fn install_request_poll(
        self: &Rc<Self>,
        receiver: Receiver<Result<EditResponse, String>>,
        generation: u64,
    ) {
        if let Some(source_id) = self.request_poll.borrow_mut().take() {
            source_id.remove();
        }

        let assistant = self.clone();
        let source_id = glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            let keep_polling = match receiver.try_recv() {
                Ok(result) => {
                    assistant.handle_request_result(generation, result);
                    false
                }
                Err(TryRecvError::Empty) => true,
                Err(TryRecvError::Disconnected) => {
                    assistant.handle_request_result(
                        generation,
                        Err("AI request thread disconnected.".to_string()),
                    );
                    false
                }
            };

            if keep_polling {
                glib::ControlFlow::Continue
            } else {
                assistant.request_poll.borrow_mut().take();
                glib::ControlFlow::Break
            }
        });

        *self.request_poll.borrow_mut() = Some(source_id);
    }

    fn handle_request_result(&self, generation: u64, result: Result<EditResponse, String>) {
        if generation != self.request_generation.get() {
            return;
        }

        self.prompt_spinner.stop();
        self.prompt_send.set_sensitive(true);

        match result {
            Ok(response) => {
                let Some(prompt_mode) = self.prompt_mode.borrow().clone() else {
                    self.prompt_status.set_text("AI result arrived without an active action.");
                    return;
                };
                self.prompt_status.set_text("");
                self.prompt_revealer.set_reveal_child(false);
                self.show_proposal(prompt_mode, response);
            }
            Err(err) => {
                self.prompt_status.set_text(&err);
            }
        }
    }

    fn show_proposal(&self, prompt_mode: AiPromptMode, response: EditResponse) {
        match prompt_mode {
            AiPromptMode::InsertAtCursor { offset } => {
                self.pending_proposal.borrow_mut().replace(PendingProposal::Insert {
                    offset,
                    text: response.text.clone(),
                });
                self.current_label.set_text("Context");
                let iter = self.buffer.iter_at_offset(offset);
                let before = excerpt_before(&self.buffer, &iter, 300);
                let after = excerpt_after(&self.buffer, &iter, 300);
                self.current_buffer.set_text(&format!("{before}|{after}"));
                self.proposal_buffer.set_text(&response.text);
                self.review_title.set_text(
                    response
                        .summary
                        .as_deref()
                        .unwrap_or("AI proposes an insertion at the cursor"),
                );
                self.review_status.set_text("Review the proposed insertion.");
            }
            AiPromptMode::ReplaceSelection {
                start,
                end,
                selected_text,
            } => {
                self.pending_proposal.borrow_mut().replace(PendingProposal::Replace {
                    start,
                    end,
                    original_text: selected_text.clone(),
                    replacement: response.text.clone(),
                });
                self.current_label.set_text("Current");
                self.current_buffer.set_text(&selected_text);
                self.proposal_buffer.set_text(&response.text);
                self.review_title.set_text(
                    response
                        .summary
                        .as_deref()
                        .unwrap_or("AI proposes a rewrite for the selection"),
                );
                self.review_status.set_text("Review the proposed replacement.");
            }
        }

        self.review_revealer.set_reveal_child(true);
    }

    fn accept_proposal(&self) {
        let Some(proposal) = self.pending_proposal.borrow().clone() else {
            self.review_status.set_text("No proposal to apply.");
            return;
        };

        match proposal {
            PendingProposal::Insert { offset, text, .. } => {
                let mut iter = self.buffer.iter_at_offset(offset);
                self.buffer.begin_user_action();
                self.buffer.insert(&mut iter, &text);
                self.buffer.end_user_action();
            }
            PendingProposal::Replace {
                start,
                end,
                original_text,
                replacement,
                ..
            } => {
                let mut start_iter = self.buffer.iter_at_offset(start);
                let mut end_iter = self.buffer.iter_at_offset(end);
                let current = self.buffer.text(&start_iter, &end_iter, false).to_string();
                if current != original_text {
                    self.review_status
                        .set_text("The buffer changed since the proposal was generated. Reject and retry.");
                    return;
                }
                self.buffer.begin_user_action();
                self.buffer.delete(&mut start_iter, &mut end_iter);
                self.buffer.insert(&mut start_iter, &replacement);
                self.buffer.end_user_action();
            }
        }

        self.pending_proposal.borrow_mut().take();
        self.review_revealer.set_reveal_child(false);
        self.review_status.set_text("");
        self.view.grab_focus();
    }

    fn bump_request_generation(&self) -> u64 {
        let next = self.request_generation.get().wrapping_add(1);
        self.request_generation.set(next);
        next
    }
}

fn configure_review_view(view: &View) {
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_wrap_mode(gtk4::WrapMode::Word);
    view.set_monospace(true);
    view.set_show_line_numbers(false);
    view.set_top_margin(8);
    view.set_bottom_margin(8);
    view.set_left_margin(8);
    view.set_right_margin(8);
}

fn excerpt_before(buffer: &Buffer, iter: &gtk4::TextIter, max_chars: i32) -> String {
    let mut start = iter.clone();
    start.backward_chars(max_chars);
    buffer.text(&start, iter, false).to_string()
}

fn excerpt_after(buffer: &Buffer, iter: &gtk4::TextIter, max_chars: i32) -> String {
    let mut end = iter.clone();
    end.forward_chars(max_chars);
    buffer.text(iter, &end, false).to_string()
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
    _ai: Rc<AiAssistant>,
}

impl Editor {
    pub fn new(ai_provider: ProviderAvailability) -> Self {
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
        let ai = AiAssistant::new(&buffer, &view, ai_provider);

        // Container: AI review + AI prompt + search bar + scrolled editor + status bar
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.append(ai.review_widget());
        container.append(ai.prompt_widget());
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
            _ai: ai,
        }
    }
}
