use libadwaita as adw;
use gtk4::prelude::*;
use adw::prelude::*;
use adw::{Application, ApplicationWindow, OverlaySplitView, ToolbarView, HeaderBar};
use gtk4::{Paned, Orientation, Button, MenuButton, Popover, Switch, Box as GtkBox, Label, Align};
use webkit6::prelude::*;
use webkit6::WebView;

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
        .title("Agentic MD")
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

    // --- Sidebar ---
    let sidebar = Sidebar::new(&window, &editor.buffer);
    
    let sidebar_header = HeaderBar::new();
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
    btn_open.connect_clicked(move |_| {
        file_ops::open_file(&win_clone1, &buf_clone1);
    });
    content_header.pack_start(&btn_open);

    // Save Button
    let btn_save = Button::from_icon_name("document-save-symbolic");
    let win_clone2 = window.clone();
    let buf_clone2 = editor.buffer.clone();
    btn_save.connect_clicked(move |_| {
        file_ops::save_file(&win_clone2, &buf_clone2);
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
    vim_switch.set_active(true); // Default enabled
    vim_switch.set_valign(Align::Center);
    
    let editor_view = editor.view.clone();
    let vim_handler = editor.vim_handler;
    vim_switch.connect_active_notify(move |switch| {
        vim_handler.set_enabled(&editor_view, switch.is_active());
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

    window.present();
}
