use gtk4::prelude::*;
use libadwaita as adw;
use gtk4::{gio, Box as GtkBox, Label, ListView, ScrolledWindow, SignalListItemFactory, SingleSelection, DirectoryList};
use adw::ApplicationWindow;
use sourceview5::Buffer;
use std::rc::Rc;
use std::cell::RefCell;
use std::path::PathBuf;

pub struct Sidebar {
    pub container: GtkBox,
    pub dir_list: DirectoryList,
}

impl Sidebar {
    pub fn new(window: &ApplicationWindow, buffer: &Buffer, current_file: Rc<RefCell<Option<PathBuf>>>) -> Self {
        let container = GtkBox::new(gtk4::Orientation::Vertical, 0);
        let settings = gio::Settings::new("com.agentic.md");
        let folder_str = settings.string("last-folder");
        let current_dir = if folder_str.is_empty() {
            gio::File::for_path(".")
        } else {
            gio::File::for_path(folder_str.as_str())
        };
        
        let dir_list = DirectoryList::new(Some("standard::name,standard::icon,standard::type"), Some(&current_dir));
        
        // factory
        let factory = SignalListItemFactory::new();
        factory.connect_setup(move |_, list_item| {
            let item = list_item.downcast_ref::<gtk4::ListItem>().unwrap();
            let bx = GtkBox::new(gtk4::Orientation::Horizontal, 6);
            let img = gtk4::Image::new();
            let label = Label::new(None);
            bx.append(&img);
            bx.append(&label);
            item.set_child(Some(&bx));
        });
        
        factory.connect_bind(move |_, list_item| {
            let item = list_item.downcast_ref::<gtk4::ListItem>().unwrap();
            let child = item.child().unwrap().downcast::<GtkBox>().unwrap();
            let img = child.first_child().unwrap().downcast::<gtk4::Image>().unwrap();
            let label = child.last_child().unwrap().downcast::<Label>().unwrap();
            
            if let Some(obj) = item.item() {
                if let Ok(info) = obj.downcast::<gio::FileInfo>() {
                    label.set_text(&info.name().to_string_lossy());
                    if let Some(icon) = info.icon() {
                        img.set_from_gicon(&icon);
                    }
                }
            }
        });
        
        let list_model = dir_list.clone();
        
        // Filter out non-files and non-markdown/directories if possible? For now show all.
        // We'll wrap in a SingleSelection
        let sel = SingleSelection::new(Some(list_model.clone()));
        let list_view = ListView::new(Some(sel), Some(factory));
        list_view.set_single_click_activate(true);
        
        let win_clone = window.clone();
        let buf_clone = buffer.clone();
        let dl_clone = list_model.clone();
        let current_file_clone = current_file.clone();
        list_view.connect_activate(move |_lv, position| {
            if let Some(obj) = dl_clone.item(position) {
                if let Ok(info) = obj.downcast::<gio::FileInfo>() {
                    // Only open if it is a regular file
                    if info.file_type() == gio::FileType::Regular {
                        if let Some(root) = dl_clone.file() {
                            let child_file = root.child(info.name());
                            if let Some(path) = child_file.path() {
                                // Auto-save existing file first
                                if let Some(current_path) = current_file_clone.borrow().as_deref() {
                                    crate::file_ops::save_to_path(&buf_clone, current_path);
                                }
                                
                                let _ = gio::Settings::new("com.agentic.md").set_string("last-file", &path.to_string_lossy());
                                *current_file_clone.borrow_mut() = Some(path.clone());
                                crate::file_ops::open_path(&win_clone, &buf_clone, &path);
                            }
                        }
                    }
                }
            }
        });
        
        let scroll = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .child(&list_view)
            .vexpand(true)
            .build();
            
        container.append(&scroll);
        
        Sidebar {
            container,
            dir_list,
        }
    }
}
