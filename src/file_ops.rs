use gtk4::prelude::*;
use libadwaita as adw;
use gtk4::{FileDialog, gio};
use adw::ApplicationWindow;
use sourceview5::Buffer;

/// Opens a file dialog, reads the selected file, and updates the buffer.
pub fn open_file(window: &ApplicationWindow, buffer: &Buffer) {
    let dialog = FileDialog::builder()
        .title("Open Markdown File")
        .modal(true)
        .build();

    // Add a filter for markdown files
    let filter = gtk4::FileFilter::new();
    filter.set_name(Some("Markdown files"));
    filter.add_pattern("*.md");
    filter.add_pattern("*.markdown");
    
    let filter_list = gio::ListStore::new::<gtk4::FileFilter>();
    filter_list.append(&filter);
    dialog.set_filters(Some(&filter_list));

    let window_clone = window.clone();
    let buffer_clone = buffer.clone();

    dialog.open(Some(window), gio::Cancellable::NONE, move |result| {
        if let Ok(file) = result {
            if let Some(path) = file.path() {
                open_path(&window_clone, &buffer_clone, &path);
            }
        }
    });
}

/// Opens a specific path and updates the buffer
pub fn open_path(window: &ApplicationWindow, buffer: &Buffer, path: &std::path::Path) {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            buffer.set_text(&content);
            if let Some(filename) = path.file_name() {
                let fname = filename.to_string_lossy();
                window.set_title(Some(&format!("{} - Agentic MD", fname)));
            }
        }
        Err(err) => {
            eprintln!("Failed to read file: {err}");
        }
    }
}

/// Opens a save dialog, gets the buffer text, and writes it to the selected file.
pub fn save_file(window: &ApplicationWindow, buffer: &Buffer) {
    let dialog = FileDialog::builder()
        .title("Save Markdown File")
        .modal(true)
        .build();

    let window_clone = window.clone();
    let buffer_clone = buffer.clone();

    dialog.save(Some(window), gio::Cancellable::NONE, move |result| {
        if let Ok(file) = result {
            let path = file.path().expect("Expected a valid path");
            
            // Get text from buffer
            let start = buffer_clone.start_iter();
            let end = buffer_clone.end_iter();
            let text = buffer_clone.text(&start, &end, false);

            match std::fs::write(&path, text.as_str()) {
                Ok(_) => {
                    if let Some(filename) = path.file_name() {
                        window_clone.set_title(Some(&format!("{} - Agentic MD", filename.to_string_lossy())));
                    }
                }
                Err(err) => {
                    eprintln!("Failed to save file: {err}");
                }
            }
        }
    });
}
