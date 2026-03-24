use gtk4::prelude::*;
use libadwaita as adw;
use gtk4::{FileDialog, gio};
use adw::ApplicationWindow;
use sourceview5::Buffer;
use sourceview5::prelude::*;
use std::process::Command;
use std::rc::Rc;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentKind {
    PlainText,
    Image,
    Svg,
    EditablePandoc { format: &'static str },
    ReadOnlyPandoc { format: &'static str },
}

fn extension_lower(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

fn document_kind_from_extension(ext: Option<&str>) -> DocumentKind {
    match ext {
        Some("svg") => DocumentKind::Svg,
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "avif" | "ico") => {
            DocumentKind::Image
        }
        Some("md" | "markdown" | "mkd" | "mkdn" | "mdown") => {
            DocumentKind::EditablePandoc { format: "markdown" }
        }
        Some("rst") => DocumentKind::EditablePandoc { format: "rst" },
        Some("tex" | "ltx" | "latex") => DocumentKind::EditablePandoc { format: "latex" },
        Some("html" | "htm" | "xhtml") => DocumentKind::EditablePandoc { format: "html" },
        Some("org") => DocumentKind::EditablePandoc { format: "org" },
        Some("textile") => DocumentKind::EditablePandoc { format: "textile" },
        Some("typ") => DocumentKind::EditablePandoc { format: "typst" },
        Some("csv") => DocumentKind::EditablePandoc { format: "csv" },
        Some("tsv") => DocumentKind::EditablePandoc { format: "tsv" },
        Some("docx") => DocumentKind::ReadOnlyPandoc { format: "docx" },
        Some("odt") => DocumentKind::ReadOnlyPandoc { format: "odt" },
        Some("epub") => DocumentKind::ReadOnlyPandoc { format: "epub" },
        Some("rtf") => DocumentKind::ReadOnlyPandoc { format: "rtf" },
        Some("ipynb") => DocumentKind::ReadOnlyPandoc { format: "ipynb" },
        Some("fb2") => DocumentKind::ReadOnlyPandoc { format: "fb2" },
        _ => DocumentKind::PlainText,
    }
}

pub fn document_kind(path: &Path) -> DocumentKind {
    document_kind_from_extension(extension_lower(path).as_deref())
}

pub fn document_kind_for_name(name: &str) -> DocumentKind {
    let ext = Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());
    document_kind_from_extension(ext.as_deref())
}

pub fn supports_preview(path: &Path) -> bool {
    !matches!(document_kind(path), DocumentKind::PlainText)
}

pub fn is_editable_in_buffer(path: &Path) -> bool {
    !matches!(document_kind(path), DocumentKind::Image | DocumentKind::ReadOnlyPandoc { .. })
}

fn pandoc_extract_plain(path: &Path, format: &str) -> Result<String, String> {
    let mut command = Command::new("pandoc");
    command
        .arg(format!("--from={format}"))
        .arg("--to=plain")
        .arg(path);

    if let Some(parent) = path.parent() {
        command.current_dir(parent);
    }

    let output = command
        .output()
        .map_err(|err| format!("Failed to run pandoc: {err}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

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

pub fn open_path(window: &ApplicationWindow, buffer: &Buffer, path: &std::path::Path) {
    match document_kind(path) {
        DocumentKind::Image => {
            buffer.set_text("");
            buffer.set_modified(false);
            buffer.set_language(None::<&sourceview5::Language>);
            update_window_title(window, buffer, Some(path));
        }
        DocumentKind::ReadOnlyPandoc { format } => {
            let content = match pandoc_extract_plain(path, format) {
                Ok(content) => content,
                Err(err) => format!("Unable to extract text from this document with pandoc.\n\n{err}"),
            };
            buffer.set_text(&content);
            buffer.set_modified(false);
            buffer.set_language(None::<&sourceview5::Language>);
            update_window_title(window, buffer, Some(path));
        }
        DocumentKind::PlainText | DocumentKind::Svg | DocumentKind::EditablePandoc { .. } => {
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    buffer.set_text(&content);
                    buffer.set_modified(false);

                    let lang_manager = sourceview5::LanguageManager::default();
                    if let Some(lang) = lang_manager.guess_language(Some(path), None) {
                        buffer.set_language(Some(&lang));
                    } else {
                        buffer.set_language(None::<&sourceview5::Language>);
                    }

                    update_window_title(window, buffer, Some(path));
                }
                Err(err) => {
                    eprintln!("Failed to read file: {err}");
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_supported_documents() {
        assert_eq!(
            document_kind(Path::new("paper.md")),
            DocumentKind::EditablePandoc { format: "markdown" }
        );
        assert_eq!(
            document_kind(Path::new("notes.rst")),
            DocumentKind::EditablePandoc { format: "rst" }
        );
        assert_eq!(
            document_kind(Path::new("draft.tex")),
            DocumentKind::EditablePandoc { format: "latex" }
        );
        assert_eq!(
            document_kind(Path::new("article.docx")),
            DocumentKind::ReadOnlyPandoc { format: "docx" }
        );
        assert_eq!(document_kind(Path::new("figure.svg")), DocumentKind::Svg);
        assert_eq!(document_kind(Path::new("photo.jpg")), DocumentKind::Image);
        assert_eq!(document_kind(Path::new("scratch.txt")), DocumentKind::PlainText);
    }

    #[test]
    fn reports_editability() {
        assert!(is_editable_in_buffer(Path::new("paper.md")));
        assert!(is_editable_in_buffer(Path::new("diagram.svg")));
        assert!(!is_editable_in_buffer(Path::new("article.docx")));
        assert!(!is_editable_in_buffer(Path::new("photo.png")));
    }
}
