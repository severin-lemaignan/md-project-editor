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
use crate::file_ops;
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
            crate::file_ops::open_path(&window, &editor.buffer, &path);
            *current_file.borrow_mut() = Some(path);
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
                crate::file_ops::save_to_path(&b_clone, path);
            }
            *debouncer_inside.borrow_mut() = None;
            glib::ControlFlow::Break
        });
        *debouncer_for_closure.borrow_mut() = Some(source_id);
    });

    // --- Sidebar ---
    let sidebar = Sidebar::new(&window, &editor.buffer, current_file.clone());
    
    let sidebar_header = HeaderBar::new();
    let btn_open_folder = Button::from_icon_name("folder-open-symbolic");
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

    let sidebar_toolbar = ToolbarView::new();
    sidebar_toolbar.add_top_bar(&sidebar_header);
    sidebar_toolbar.set_content(Some(&sidebar.container));

    // --- Main Content ---
    let content_header = HeaderBar::new();
    
    // Setup Toggle Sidebar Button
    let toggle_sidebar_btn = Button::from_icon_name("sidebar-show-symbolic");
    content_header.pack_start(&toggle_sidebar_btn);

    // Open Button
    let btn_open = Button::from_icon_name("document-open-symbolic");
    let win_clone1 = window.clone();
    let buf_clone1 = editor.buffer.clone();
    let current_file_clone1 = current_file.clone();
    btn_open.connect_clicked(move |_| {
        if let Some(path) = &*current_file_clone1.borrow() {
            crate::file_ops::save_to_path(&buf_clone1, path);
        }
        file_ops::open_file(&win_clone1, &buf_clone1, current_file_clone1.clone());
    });
    content_header.pack_start(&btn_open);

    // Save Button
    let btn_save = Button::from_icon_name("document-save-symbolic");
    let win_clone2 = window.clone();
    let buf_clone2 = editor.buffer.clone();
    let current_file_clone2 = current_file.clone();
    btn_save.connect_clicked(move |_| {
        if let Some(path) = &*current_file_clone2.borrow() {
            crate::file_ops::save_to_path(&buf_clone2, path);
        } else {
            file_ops::save_file(&win_clone2, &buf_clone2, current_file_clone2.clone());
        }
    });
    content_header.pack_start(&btn_save);

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
        "#,
    );
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("Could not get default display"),
        &css_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Set up live preview
    preview::setup_live_preview(&editor.buffer, &webview);

    // Set up synchronized scrolling
    sync_scroll::setup_sync_scroll(&editor.scrolled_window, &webview);

    // Initial Title update
    crate::file_ops::update_window_title(&window, &editor.buffer, current_file.borrow().as_deref());

    window.present();
}
