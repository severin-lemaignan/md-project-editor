use libadwaita as adw;
use gtk4::prelude::*;
use gtk4::gio;
use adw::prelude::*;
use adw::{Application, ApplicationWindow, OverlaySplitView, ToolbarView, HeaderBar};
use gtk4::{Paned, Orientation, Button, MenuButton, Popover, Switch, Box as GtkBox, Label, Align};
use webkit6::prelude::*;
use webkit6::WebView;
use std::rc::Rc;
use std::cell::RefCell;
use std::path::PathBuf;

use crate::editor::Editor;
use crate::preview;
use crate::sync_scroll;
use crate::sidebar::Sidebar;

/// Build the main application UI.
pub fn build_ui(app: &Application) {
    // Initialize GtkSourceView (must happen after GTK init)
    sourceview5::init();

    // Prefer dark theme (adwaita style)
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);

    // Create the window first so we can pass it to other components
    let window = ApplicationWindow::builder()
        .application(app)
        .default_width(1200)
        .default_height(800)
        .build();

    // Create the editor
    let editor = Editor::new();

    // Create the WebView for preview
    let webview = WebView::new();
    webview.set_hexpand(true);
    webview.set_vexpand(true);
    let bg = gtk4::gdk::RGBA::new(0.118, 0.118, 0.180, 1.0); // #1e1e2e
    webview.set_background_color(&bg);

    // Set up the paned container (side-by-side editor/preview)
    let paned = Paned::builder()
        .orientation(Orientation::Horizontal)
        .wide_handle(true)
        .start_child(&editor.container)
        .end_child(&webview)
        .build();
    paned.set_position(600);

    let current_file: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    let settings = gio::Settings::new("com.agentic.md");

    // Load initial file if it exists
    let last_file = settings.string("last-file");
    if !last_file.is_empty() {
        let path = std::path::PathBuf::from(last_file.as_str());
        if path.exists() {
            *current_file.borrow_mut() = Some(path.clone());
            crate::file_ops::open_path(&window, &editor.buffer, &path);
        }
    }

    // --- Auto-Save on Type setup ---
    let debouncer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let current_file_deb = current_file.clone();
    
    // --- Title Updater ---
    let title_win = window.clone();
    let title_file = current_file.clone();
    editor.buffer.connect_modified_changed(move |b| {
        let buf = b.downcast_ref::<sourceview5::Buffer>().unwrap();
        crate::file_ops::update_window_title(&title_win, buf, title_file.borrow().as_deref());
    });

    let debouncer_for_closure = debouncer.clone();
    editor.buffer.connect_changed(move |b| {
        if let Some(source_id) = debouncer_for_closure.borrow_mut().take() {
            source_id.remove();
        }
        let path_clone = current_file_deb.clone();
        let b_clone = b.clone();
        let debouncer_inside = debouncer_for_closure.clone();
        let source_id = glib::timeout_add_local(std::time::Duration::from_millis(1500), move || {
            if let Some(path) = &*path_clone.borrow() {
                if crate::file_ops::is_editable_in_buffer(path) {
                    crate::file_ops::save_to_path(&b_clone, path);
                }
            }
            *debouncer_inside.borrow_mut() = None;
            glib::ControlFlow::Break
        });
        *debouncer_for_closure.borrow_mut() = Some(source_id);
    });

    let refresh_preview = preview::setup_live_preview(
        &editor.buffer,
        &webview,
        &editor.view,
        &editor.container,
        current_file.clone(),
    );

    // --- Sidebar ---
    let sidebar = Sidebar::new(
        &window,
        &editor.buffer,
        current_file.clone(),
        refresh_preview.clone(),
    );
    
    let sidebar_header = HeaderBar::new();
    let empty_title = adw::WindowTitle::new("", "");
    sidebar_header.set_title_widget(Some(&empty_title));
    
    // Up Folder button
    let btn_up_folder = Button::from_icon_name("go-up-symbolic");
    btn_up_folder.set_tooltip_text(Some("Up a Directory"));
    let dl_up = sidebar.dir_list.clone();
    btn_up_folder.connect_clicked(move |_| {
        if let Some(file) = dl_up.file() {
            if let Some(parent) = file.parent() {
                dl_up.set_file(Some(&parent));
                if let Some(p) = parent.path() {
                    let _ = gio::Settings::new("com.agentic.md").set_string("last-folder", &p.to_string_lossy());
                }
            }
        }
    });

    sidebar_header.pack_start(&btn_up_folder);

    // open folder button
    let btn_open_folder = Button::from_icon_name("folder-open-symbolic");
    btn_open_folder.set_tooltip_text(Some("Open Folder"));
    let win_folder_clone = window.clone();
    let dir_list_clone = sidebar.dir_list.clone();
    btn_open_folder.connect_clicked(move |_| {
        let dialog = gtk4::FileDialog::builder()
            .title("Open Folder")
            .modal(true)
            .build();
        let dl = dir_list_clone.clone();
        dialog.select_folder(Some(&win_folder_clone), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    let _ = gio::Settings::new("com.agentic.md").set_string("last-folder", &path.to_string_lossy());
                    dl.set_file(Some(&gio::File::for_path(&path)));
                }
            }
        });
    });
    sidebar_header.pack_start(&btn_open_folder);

    // New File Button
    let btn_new_file = MenuButton::new();
    btn_new_file.set_icon_name("list-add-symbolic");
    btn_new_file.set_tooltip_text(Some("New File"));
    let popover_new = Popover::new();
    let popover_new_box = GtkBox::new(Orientation::Horizontal, 6);
    popover_new_box.set_margin_start(6);
    popover_new_box.set_margin_end(6);
    popover_new_box.set_margin_top(6);
    popover_new_box.set_margin_bottom(6);
    
    let new_entry = gtk4::Entry::builder().placeholder_text("filename.md").build();
    let btn_create = Button::builder().label("Create").build();
    popover_new_box.append(&new_entry);
    popover_new_box.append(&btn_create);
    popover_new.set_child(Some(&popover_new_box));
    btn_new_file.set_popover(Some(&popover_new));

    let new_entry_clone = new_entry.clone();
    let popover_new_clone = popover_new.clone();
    let win_new_clone = window.clone();
    let buf_new_clone = editor.buffer.clone();
    let current_file_new_clone = current_file.clone();
    let refresh_preview_new = refresh_preview.clone();
    let create_action = Rc::new(move || {
        let filename = new_entry_clone.text().to_string();
        if filename.is_empty() { return; }
        
        let s = gio::Settings::new("com.agentic.md");
        let folder_str = s.string("last-folder");
        let root = if folder_str.is_empty() { PathBuf::from(".") } else { PathBuf::from(folder_str.as_str()) };
        let file_path = root.join(filename);
        
        if !file_path.exists() {
            if let Err(e) = std::fs::write(&file_path, "") {
                eprintln!("Failed to create file: {}", e);
                return;
            }
        }
        
        if let Some(current_path) = current_file_new_clone.borrow().as_deref() {
            if crate::file_ops::is_editable_in_buffer(current_path) {
                crate::file_ops::save_to_path(&buf_new_clone, current_path);
            }
        }
        
        let _ = gio::Settings::new("com.agentic.md").set_string("last-file", &file_path.to_string_lossy());
        *current_file_new_clone.borrow_mut() = Some(file_path.clone());
        crate::file_ops::open_path(&win_new_clone, &buf_new_clone, &file_path);
        refresh_preview_new();
        
        popover_new_clone.popdown();
        new_entry_clone.set_text("");
    });

    let create1 = create_action.clone();
    btn_create.connect_clicked(move |_| create1());
    let create2 = create_action.clone();
    new_entry.connect_activate(move |_| create2());

    sidebar_header.pack_start(&btn_new_file);

    let sidebar_toolbar = ToolbarView::new();
    sidebar_toolbar.add_top_bar(&sidebar_header);
    sidebar_toolbar.set_content(Some(&sidebar.container));

    // --- Main Content ---
    let content_header = HeaderBar::new();
    
    // Setup Toggle Sidebar Button
    let toggle_sidebar_btn = Button::from_icon_name("sidebar-show-symbolic");
    content_header.pack_start(&toggle_sidebar_btn);

    // Ctrl+S shortcut for saving
    let key_ctrl = gtk4::EventControllerKey::new();
    key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let save_file_clone = current_file.clone();
    let save_buffer_clone = editor.buffer.clone();
    let undo_buffer_clone = editor.buffer.clone();
    let search_panel = editor.search.clone();
    let editor_view_for_keys = editor.view.clone();
    let vim_state_for_keys = editor.vim_handler.state.clone();
    key_ctrl.connect_key_pressed(move |_, keyval, _keycode, state| {
        if state.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
            && (keyval == gtk4::gdk::Key::f || keyval == gtk4::gdk::Key::F)
        {
            search_panel.open_find();
            return glib::Propagation::Stop;
        }
        if state.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
            && (keyval == gtk4::gdk::Key::h || keyval == gtk4::gdk::Key::H)
        {
            search_panel.open_replace();
            return glib::Propagation::Stop;
        }
        if editor_view_for_keys.has_focus()
            && state.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
            && (keyval == gtk4::gdk::Key::z || keyval == gtk4::gdk::Key::Z)
        {
            if state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
                if undo_buffer_clone.can_redo() {
                    undo_buffer_clone.redo();
                }
            } else if undo_buffer_clone.can_undo() {
                undo_buffer_clone.undo();
            }
            return glib::Propagation::Stop;
        }
        if editor_view_for_keys.has_focus()
            && state.contains(gtk4::gdk::ModifierType::CONTROL_MASK)
            && (keyval == gtk4::gdk::Key::y || keyval == gtk4::gdk::Key::Y)
        {
            if undo_buffer_clone.can_redo() {
                undo_buffer_clone.redo();
            }
            return glib::Propagation::Stop;
        }
        if state.contains(gtk4::gdk::ModifierType::CONTROL_MASK) && 
           (keyval == gtk4::gdk::Key::s || keyval == gtk4::gdk::Key::S) {
            if let Some(path) = &*save_file_clone.borrow() {
                if crate::file_ops::is_editable_in_buffer(path) {
                    crate::file_ops::save_to_path(&save_buffer_clone, path);
                }
            }
            return glib::Propagation::Stop;
        }
        if keyval == gtk4::gdk::Key::F3 {
            if state.contains(gtk4::gdk::ModifierType::SHIFT_MASK) {
                search_panel.find_previous();
            } else {
                search_panel.find_next();
            }
            return glib::Propagation::Stop;
        }
        if state.is_empty()
            && keyval == gtk4::gdk::Key::slash
            && editor_view_for_keys.has_focus()
            && !editor_view_for_keys.is_editable()
            && !vim_state_for_keys.borrow().enabled
        {
            search_panel.open_find();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(key_ctrl);

    // Settings Menu
    let settings_btn = MenuButton::new();
    settings_btn.set_icon_name("open-menu-symbolic");
    
    let popover = Popover::new();
    let popover_box = GtkBox::new(Orientation::Horizontal, 12);
    popover_box.set_margin_start(12);
    popover_box.set_margin_end(12);
    popover_box.set_margin_top(12);
    popover_box.set_margin_bottom(12);
    
    let vim_label = Label::new(Some("Vim Mode"));
    let vim_switch = Switch::new();
    let is_vim_mode = settings.boolean("vim-mode");
    vim_switch.set_active(is_vim_mode);
    vim_switch.set_valign(Align::Center);
    
    let editor_view = editor.view.clone();
    let vim_handler = editor.vim_handler;
    vim_handler.set_enabled(&editor_view, is_vim_mode);
    
    vim_switch.connect_active_notify(move |switch| {
        let active = switch.is_active();
        vim_handler.set_enabled(&editor_view, active);
        let s = gio::Settings::new("com.agentic.md");
        let _ = s.set_boolean("vim-mode", active);
    });

    popover_box.append(&vim_label);
    popover_box.append(&vim_switch);
    popover.set_child(Some(&popover_box));
    settings_btn.set_popover(Some(&popover));
    
    content_header.pack_end(&settings_btn);

    let content_toolbar = ToolbarView::new();
    content_toolbar.add_top_bar(&content_header);
    content_toolbar.set_content(Some(&paned));

    // --- Overlay Split View ---
    let split_view = OverlaySplitView::builder()
        .sidebar(&sidebar_toolbar)
        .content(&content_toolbar)
        .show_sidebar(true)
        .build();

    let split_view_clone = split_view.clone();
    toggle_sidebar_btn.connect_clicked(move |_| {
        let current = split_view_clone.shows_sidebar();
        split_view_clone.set_show_sidebar(!current);
    });

    window.set_content(Some(&split_view));

    // Apply custom CSS
    let css_provider = gtk4::CssProvider::new();
    css_provider.load_from_string(
        r#"
        window {
            background-color: #1e1e2e;
        }
        paned > separator {
            min-width: 3px;
            background-color: #313244;
        }
        paned > separator:hover {
            background-color: #89b4fa;
        }
        .editor-view {
            font-size: 14px;
        }
        .vim-status {
            font-family: monospace;
            font-size: 12px;
            color: #a6adc8;
            background-color: #181825;
            padding: 4px 12px;
            border-top: 1px solid #313244;
        }
        .editor-search-bar {
            background-color: #181825;
            border: 1px solid #313244;
            border-radius: 10px;
            padding: 8px;
        }
        .dim-label {
            opacity: 0.4;
        }
        .destructive-action {
            color: #f38ba8;
        }
        "#,
    );
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("Could not get default display"),
        &css_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Set up citation completion
    crate::citation_completion::setup_citation_completion(
        &editor.view,
        &editor.buffer,
        current_file.clone(),
    );

    // Set up synchronized scrolling
    sync_scroll::setup_sync_scroll(&editor.scrolled_window, &webview);

    // Initial Title update
    crate::file_ops::update_window_title(&window, &editor.buffer, current_file.borrow().as_deref());

    window.present();
}
