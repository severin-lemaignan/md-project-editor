use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use gtk4::{gio, Box as GtkBox, Label, ListView, ScrolledWindow, SignalListItemFactory, SingleSelection, DirectoryList};
use adw::ApplicationWindow;
use sourceview5::Buffer;
use std::rc::Rc;
use std::cell::RefCell;
use std::path::PathBuf;

pub struct Sidebar {
    pub container: GtkBox,
    pub dir_list: DirectoryList,
    pub sel: SingleSelection,
}

pub fn sync_selection(sel: &SingleSelection, dl: &DirectoryList, current_path: Option<&std::path::Path>) {
    if let Some(path) = current_path {
        for i in 0..dl.n_items() {
            if let Some(obj) = dl.item(i) {
                if let Ok(info) = obj.downcast::<gio::FileInfo>() {
                    if let Some(root) = dl.file() {
                        let child_file = root.child(info.name());
                        if let Some(p) = child_file.path() {
                            if p == path {
                                sel.set_selected(i);
                                return;
                            }
                        }
                    }
                }
            }
        }
    }
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
        let dl_setup = dir_list.clone();
        let buf_setup = buffer.clone();
        let win_setup = window.clone();
        let current_setup = current_file.clone();
        
        factory.connect_setup(move |_, list_item| {
            let item = list_item.downcast_ref::<gtk4::ListItem>().unwrap();
            let bx = GtkBox::new(gtk4::Orientation::Horizontal, 6);
            let img = gtk4::Image::new();
            let label = Label::new(None);
            bx.append(&img);
            bx.append(&label);
            
            let popover = gtk4::Popover::new();
            let vbox = GtkBox::new(gtk4::Orientation::Vertical, 0);
            let btn_rename = gtk4::Button::with_label("Rename");
            let btn_dup = gtk4::Button::with_label("Duplicate");
            let btn_del = gtk4::Button::with_label("Delete");
            
            btn_del.add_css_class("destructive-action");
            btn_rename.add_css_class("flat");
            btn_dup.add_css_class("flat");
            btn_del.add_css_class("flat");
            
            vbox.append(&btn_rename);
            vbox.append(&btn_dup);
            vbox.append(&btn_del);
            popover.set_child(Some(&vbox));
            popover.set_has_arrow(true);
            bx.append(&popover); 
            
            let gesture = gtk4::GestureClick::new();
            gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
            let popover_click = popover.clone();
            gesture.connect_pressed(move |_g, _, x, y| {
                popover_click.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                popover_click.popup();
            });
            bx.add_controller(gesture);
            
            let item_for_path = item.clone();
            let dl_path = dl_setup.clone();
            let get_path = move || -> Option<PathBuf> {
                if let Some(obj) = item_for_path.item() {
                    if let Ok(info) = obj.downcast::<gio::FileInfo>() {
                        if let Some(root) = dl_path.file() {
                            return root.child(info.name()).path();
                        }
                    }
                }
                None
            };
            
            let get_path_del = get_path.clone();
            let popover_del = popover.clone();
            let current_del = current_setup.clone();
            let buf_del = buf_setup.clone();
            btn_del.connect_clicked(move |_| {
                if let Some(path) = get_path_del() {
                    let _ = std::fs::remove_file(&path);
                    let is_current = {
                        if let Some(current) = current_del.borrow().as_deref() {
                            current == path
                        } else {
                            false
                        }
                    };
                    if is_current {
                        *current_del.borrow_mut() = None;
                        buf_del.set_text("");
                    }
                }
                popover_del.popdown();
            });
            
            let get_path_dup = get_path.clone();
            let popover_dup = popover.clone();
            btn_dup.connect_clicked(move |_| {
                if let Some(path) = get_path_dup() {
                    if let Some(filename) = path.file_name() {
                        let mut new_name = filename.to_string_lossy().into_owned();
                        if let Some(pos) = new_name.rfind('.') {
                            new_name.insert_str(pos, "_copy");
                        } else {
                            new_name.push_str("_copy");
                        }
                        let new_path = path.with_file_name(new_name);
                        let _ = std::fs::copy(&path, &new_path);
                    }
                }
                popover_dup.popdown();
            });
            
            let get_path_ren = get_path.clone();
            let popover_ren = popover.clone();
            let win_ren = win_setup.clone();
            let current_ren = current_setup.clone();
            let buf_ren = buf_setup.clone();
            btn_rename.connect_clicked(move |_| {
                if let Some(path) = get_path_ren() {
                    let dialog = libadwaita::MessageDialog::builder()
                        .heading("Rename File")
                        .body("Enter new name:")
                        .build();
                    let entry = gtk4::Entry::builder()
                        .text(path.file_name().unwrap_or_default().to_string_lossy().as_ref())
                        .activates_default(true)
                        .build();
                    dialog.set_extra_child(Some(&entry));
                    dialog.add_response("cancel", "Cancel");
                    dialog.add_response("rename", "Rename");
                    dialog.set_response_appearance("rename", libadwaita::ResponseAppearance::Suggested);
                    dialog.set_default_response(Some("rename"));
                    
                    let path_clone = path.clone();
                    let current_c = current_ren.clone();
                    let buf_c = buf_ren.clone();
                    let win_c = win_ren.clone();
                    dialog.connect_response(Some("rename"), move |d, _| {
                        let new_name = entry.text();
                        let new_path = path_clone.with_file_name(new_name.as_str());
                        if std::fs::rename(&path_clone, &new_path).is_ok() {
                            let is_current = {
                                if let Some(current) = current_c.borrow().as_deref() {
                                    current == path_clone
                                } else {
                                    false
                                }
                            };
                            if is_current {
                                *current_c.borrow_mut() = Some(new_path.clone());
                                crate::file_ops::update_window_title(&win_c, &buf_c, Some(&new_path));
                            }
                        }
                        d.close();
                    });
                    
                    dialog.connect_response(Some("cancel"), move |d, _| d.close());
                    
                    dialog.set_transient_for(Some(&win_ren));
                    dialog.present();
                }
                popover_ren.popdown();
            });
            
            item.set_child(Some(&bx));
        });
        
        factory.connect_bind(move |_, list_item| {
            let item = list_item.downcast_ref::<gtk4::ListItem>().unwrap();
            let child = item.child().unwrap().downcast::<GtkBox>().unwrap();
            let img = child.first_child().unwrap().downcast::<gtk4::Image>().unwrap();
            let label = img.next_sibling().unwrap().downcast::<Label>().unwrap();
            
            if let Some(obj) = item.item() {
                if let Ok(info) = obj.downcast::<gio::FileInfo>() {
                    let name = info.name().to_string_lossy().into_owned();
                    label.set_text(&name);
                    if let Some(icon) = info.icon() {
                        img.set_from_gicon(&icon);
                    }
                    
                    if info.file_type() != gio::FileType::Directory 
                       && !name.ends_with(".md") && !name.ends_with(".markdown") {
                        child.add_css_class("dim-label");
                    } else {
                        child.remove_css_class("dim-label");
                    }
                }
            }
        });
        
        let list_model = dir_list.clone();
        
        // Filter out non-files and non-markdown/directories if possible? For now show all.
        // We'll wrap in a SingleSelection
        let sel = SingleSelection::new(Some(list_model.clone()));
        let list_view = ListView::new(Some(sel.clone()), Some(factory));
        list_view.set_single_click_activate(true);
        
        let sel_sync = sel.clone();
        let dl_sync = dir_list.clone();
        let current_sync = current_file.clone();
        dir_list.connect_items_changed(move |_, _, _, _| {
            let c = current_sync.borrow();
            sync_selection(&sel_sync, &dl_sync, c.as_deref());
        });
        
        let win_clone = window.clone();
        let buf_clone = buffer.clone();
        let dl_clone = list_model.clone();
        let current_file_clone = current_file.clone();
        list_view.connect_activate(move |_lv, position| {
            if let Some(obj) = dl_clone.item(position) {
                if let Ok(info) = obj.downcast::<gio::FileInfo>() {
                    if let Some(root) = dl_clone.file() {
                        let child_file = root.child(info.name());
                        if info.file_type() == gio::FileType::Directory {
                            dl_clone.set_file(Some(&child_file));
                            if let Some(path) = child_file.path() {
                                let _ = gio::Settings::new("com.agentic.md").set_string("last-folder", &path.to_string_lossy());
                            }
                        } else if info.file_type() == gio::FileType::Regular {
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
            sel,
        }
    }

    pub fn sync_selection(&self, current: Option<&std::path::Path>) {
        sync_selection(&self.sel, &self.dir_list, current);
    }
}
