use gtk4::prelude::*;
use libadwaita as adw;
use gtk4::{FileDialog, gio};
use adw::ApplicationWindow;
use sourceview5::Buffer;
use std::rc::Rc;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// Updates the window title based on the filename and buffer modified state.
pub fn update_window_title(window: &ApplicationWindow, buffer: &Buffer, path: Option<&Path>) {
    let filename = match path {
        Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Unknown".to_string()),
        None => "[No Name]".to_string(),
    };
    let star = if buffer.is_modified() { "*" } else { "" };
    window.set_title(Some(&format!("{}{}", star, filename)));
}

/// Opens a file dialog, reads the selected file, and updates the buffer.
pub fn open_file(window: &ApplicationWindow, buffer: &Buffer, current_file: Rc<RefCell<Option<PathBuf>>>) {
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
                let _ = gio::Settings::new("com.agentic.md").set_string("last-file", &path.to_string_lossy());
                *current_file.borrow_mut() = Some(path.clone());
                open_path(&window_clone, &buffer_clone, &path);
            }
        }
    });
}

/// Helper to save text buffer into a specific path.
pub fn save_to_path(buffer: &Buffer, path: &Path) {
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    let text = buffer.text(&start, &end, false);
    if let Err(err) = std::fs::write(path, text.as_str()) {
        eprintln!("Failed to save file: {err}");
    } else {
        buffer.set_modified(false);
    }
}

/// Opens a specific path and updates the buffer
pub fn open_path(window: &ApplicationWindow, buffer: &Buffer, path: &std::path::Path) {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            buffer.set_text(&content);
            buffer.set_modified(false);
            update_window_title(window, buffer, Some(path));
        }
        Err(err) => {
            eprintln!("Failed to read file: {err}");
        }
    }
}

/// Opens a save dialog, gets the buffer text, and writes it to the selected file.
pub fn save_file(window: &ApplicationWindow, buffer: &Buffer, current_file: Rc<RefCell<Option<PathBuf>>>) {
    let dialog = FileDialog::builder()
        .title("Save Markdown File")
        .modal(true)
        .build();

    let window_clone = window.clone();
    let buffer_clone = buffer.clone();

    dialog.save(Some(window), gio::Cancellable::NONE, move |result| {
        if let Ok(file) = result {
            let path = file.path().expect("Expected a valid path");
            let _ = gio::Settings::new("com.agentic.md").set_string("last-file", &path.to_string_lossy());
            save_to_path(&buffer_clone, &path);
            *current_file.borrow_mut() = Some(path.clone());
            update_window_title(&window_clone, &buffer_clone, Some(&path));
        }
    });
}
