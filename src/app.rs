use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Paned, Orientation, Settings, HeaderBar, Button, MenuButton, Popover, Switch, Box as GtkBox, Label, Align};
use webkit6::prelude::*;
use webkit6::WebView;

use crate::editor::Editor;
use crate::preview;
use crate::sync_scroll;
use crate::file_ops;

/// Build the main application UI.
pub fn build_ui(app: &Application) {
    // Initialize GtkSourceView (must happen after GTK init)
    sourceview5::init();

    // Prefer dark theme
    if let Some(settings) = Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(true);
    }

    // Create the editor
    let editor = Editor::new();

    // Create the WebView for preview
    let webview = WebView::new();
    webview.set_hexpand(true);
    webview.set_vexpand(true);

    // Set WebView background to match the dark theme
    let bg = gtk4::gdk::RGBA::new(0.118, 0.118, 0.180, 1.0); // #1e1e2e
    webview.set_background_color(&bg);

    // Set up the paned container
    let paned = Paned::builder()
        .orientation(Orientation::Horizontal)
        .wide_handle(true)
        .start_child(&editor.container)
        .end_child(&webview)
        .build();

    // Set initial position to 50%
    paned.set_position(600);

    // Apply custom CSS for the overall look
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

    // Create the window
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Agentic MD")
        .default_width(1200)
        .default_height(800)
        .child(&paned)
        .build();

    // Set up Header Bar
    let header_bar = HeaderBar::new();
    
    // Open Button
    let btn_open = Button::from_icon_name("document-open-symbolic");
    let win_clone1 = window.clone();
    let buf_clone1 = editor.buffer.clone();
    btn_open.connect_clicked(move |_| {
        file_ops::open_file(&win_clone1, &buf_clone1);
    });
    header_bar.pack_start(&btn_open);

    // Save Button
    let btn_save = Button::from_icon_name("document-save-symbolic");
    let win_clone2 = window.clone();
    let buf_clone2 = editor.buffer.clone();
    btn_save.connect_clicked(move |_| {
        file_ops::save_file(&win_clone2, &buf_clone2);
    });
    header_bar.pack_start(&btn_save);

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
    let vim_handler = editor.vim_handler; // Can move since we don't need it elsewhere
    vim_switch.connect_active_notify(move |switch| {
        vim_handler.set_enabled(&editor_view, switch.is_active());
    });

    popover_box.append(&vim_label);
    popover_box.append(&vim_switch);
    popover.set_child(Some(&popover_box));
    settings_btn.set_popover(Some(&popover));
    
    header_bar.pack_end(&settings_btn);

    window.set_titlebar(Some(&header_bar));

    window.present();
}
