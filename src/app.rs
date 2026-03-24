use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Paned, Orientation, Settings};
use webkit6::prelude::*;
use webkit6::WebView;

use crate::editor::Editor;
use crate::preview;
use crate::sync_scroll;

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
        .start_child(&editor.scrolled_window)
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

    window.present();
}
