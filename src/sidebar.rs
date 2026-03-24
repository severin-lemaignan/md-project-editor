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
        for i in 0..sel.n_items() {
            if let Some(obj) = sel.item(i) {
                if let Ok(info) = obj.downcast::<gio::FileInfo>() {
                    let mut file_path = None;
                    if let Some(abs) = info.attribute_string("agentic::absolute_path") {
                        file_path = Some(std::path::PathBuf::from(abs.as_str()));
                    } else if let Some(root) = dl.file() {
                        let child_file = root.child(info.name());
                        file_path = child_file.path();
                    }
                    if file_path.as_deref() == Some(path) {
                        sel.set_selected(i);
                        return;
                    }
                }
            }
        }
    }
}

impl Sidebar {
    pub fn new(
        window: &ApplicationWindow,
        buffer: &Buffer,
        current_file: Rc<RefCell<Option<PathBuf>>>,
        refresh_preview: crate::preview::PreviewRefresh,
    ) -> Self {
        let container = GtkBox::new(gtk4::Orientation::Vertical, 0);
        let settings = gio::Settings::new("com.agentic.md");
        let folder_str = settings.string("last-folder");
        let current_dir = if folder_str.is_empty() {
            gio::File::for_path(".")
        } else {
            gio::File::for_path(folder_str.as_str())
        };
        
        let dir_list = DirectoryList::new(Some("standard::name,standard::icon,standard::type,time::modified"), Some(&current_dir));
        
        // Sorting dropdown setup
        let sort_drop = gtk4::DropDown::from_strings(&["A -> Z", "Z -> A", "Recently Changed"]);
        sort_drop.set_tooltip_text(Some("Sort Files"));
        let sort_method = Rc::new(RefCell::new(0)); // 0: A-Z, 1: Z-A, 2: Recent
        let sort_method_c = sort_method.clone();
        
        let sorter = gtk4::CustomSorter::new(move |obj1, obj2| {
            let info1 = obj1.downcast_ref::<gio::FileInfo>().unwrap();
            let info2 = obj2.downcast_ref::<gio::FileInfo>().unwrap();
            
            let is_dir1 = info1.file_type() == gio::FileType::Directory;
            let is_dir2 = info2.file_type() == gio::FileType::Directory;
            if is_dir1 && !is_dir2 { return gtk4::Ordering::Smaller; }
            if !is_dir1 && is_dir2 { return gtk4::Ordering::Larger; }
            
            let name1 = info1.name().to_string_lossy().to_lowercase();
            let name2 = info2.name().to_string_lossy().to_lowercase();
            
            match *sort_method_c.borrow() {
                0 => name1.cmp(&name2).into(),
                1 => name2.cmp(&name1).into(),
                2 => {
                    let m1 = info1.modification_date_time().map(|d| d.to_unix()).unwrap_or(0);
                    let m2 = info2.modification_date_time().map(|d| d.to_unix()).unwrap_or(0);
                    m2.cmp(&m1).into()
                }
                _ => gtk4::Ordering::Equal,
            }
        });
        
        let sorter_for_drop = sorter.clone();
        let sort_method_for_drop = sort_method.clone();
        sort_drop.connect_selected_notify(move |drop| {
            *sort_method_for_drop.borrow_mut() = drop.selected() as i32;
            sorter_for_drop.changed(gtk4::SorterChange::Different);
        });
        
        let sort_model_dirs = gtk4::SortListModel::new(Some(dir_list.clone()), Some(sorter.clone()));
        
        let search_entry = gtk4::SearchEntry::new();
        search_entry.set_hexpand(true);
        search_entry.set_placeholder_text(Some("Search files..."));
        
        let search_store = gio::ListStore::new::<gio::FileInfo>();
        let sort_model_search = gtk4::SortListModel::new(Some(search_store.clone()), Some(sorter.clone()));
        
        // factory
        let factory = SignalListItemFactory::new();
        let dl_setup = dir_list.clone();
        let buf_setup = buffer.clone();
        let win_setup = window.clone();
        let current_setup = current_file.clone();
        let refresh_setup = refresh_preview.clone();
        
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
                        if let Some(abs) = info.attribute_string("agentic::absolute_path") {
                            return Some(std::path::PathBuf::from(abs.as_str()));
                        }
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
            let refresh_del = refresh_setup.clone();
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
                        refresh_del();
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
            let refresh_ren = refresh_setup.clone();
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
                    let refresh_c = refresh_ren.clone();
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
                                refresh_c();
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
                       && !crate::file_ops::supports_preview(std::path::Path::new(&name))
                       && crate::file_ops::document_kind_for_name(&name) == crate::file_ops::DocumentKind::PlainText {
                        child.add_css_class("dim-label");
                    } else {
                        child.remove_css_class("dim-label");
                    }
                }
            }
        });
        
        // We wrap the sort_model_dirs in SingleSelection natively
        let sel = SingleSelection::new(Some(sort_model_dirs.clone()));
        let list_view = ListView::new(Some(sel.clone()), Some(factory));
        list_view.set_single_click_activate(true);
        
        let sel_for_search = sel.clone();
        let dir_list_for_search = dir_list.clone();
        let search_store_for_search = search_store.clone();
        let sort_model_dirs_for_search = sort_model_dirs.clone();
        let sort_model_search_for_search = sort_model_search.clone();
        
        search_entry.connect_search_changed(move |entry| {
            let text = entry.text().to_string().to_lowercase();
            if text.is_empty() {
                sel_for_search.set_model(Some(&sort_model_dirs_for_search));
            } else {
                search_store_for_search.remove_all();
                if let Some(root_file) = dir_list_for_search.file() {
                    if let Some(root_path) = root_file.path() {
                        for e in walkdir::WalkDir::new(&root_path).into_iter().filter_map(|e| e.ok()) {
                            let path = e.path();
                            if path == root_path { continue; }
                            
                            let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                            if file_name.contains(&text) {
                                let info = gio::FileInfo::new();
                                let relative_path = path.strip_prefix(&root_path).unwrap_or(path).to_path_buf();
                                info.set_name(&relative_path);
                                
                                let icon_name = if path.is_dir() {
                                    info.set_file_type(gio::FileType::Directory);
                                    "folder"
                                } else {
                                    info.set_file_type(gio::FileType::Regular);
                                    "text-x-generic"
                                };
                                let icon = gio::ThemedIcon::new(icon_name);
                                info.set_icon(&icon);
                                
                                if let Ok(metadata) = std::fs::metadata(path) {
                                    if let Ok(sys_time) = metadata.modified() {
                                        if let Ok(duration) = sys_time.duration_since(std::time::UNIX_EPOCH) {
                                            if let Ok(dt) = gtk4::glib::DateTime::from_unix_utc(duration.as_secs() as i64) {
                                                info.set_modification_date_time(&dt);
                                            }
                                        }
                                    }
                                }
                                
                                info.set_attribute_string("agentic::absolute_path", path.to_string_lossy().as_ref());
                                search_store_for_search.append(&info);
                            }
                        }
                    }
                }
                sel_for_search.set_model(Some(&sort_model_search_for_search));
            }
        });
        
        let sel_sync = sel.clone();
        let dl_sync = dir_list.clone();
        let current_sync = current_file.clone();
        dir_list.connect_items_changed(move |_, _, _, _| {
            let c = current_sync.borrow();
            sync_selection(&sel_sync, &dl_sync, c.as_deref());
        });
        
        let win_clone = window.clone();
        let buf_clone = buffer.clone();
        let dl_clone = dir_list.clone();
        let current_file_clone = current_file.clone();
        let sel_for_activate = sel.clone();
        let search_entry_for_activate = search_entry.clone();
        let refresh_preview_activate = refresh_preview.clone();
        list_view.connect_activate(move |_lv, position| {
            if let Some(obj) = sel_for_activate.item(position) {
                if let Ok(info) = obj.downcast::<gio::FileInfo>() {
                    let is_dir = info.file_type() == gio::FileType::Directory;
                    let mut file_path = None;
                    
                    let is_search_result = info.attribute_string("agentic::absolute_path").is_some();
                    if let Some(abs) = info.attribute_string("agentic::absolute_path") {
                        file_path = Some(std::path::PathBuf::from(abs.as_str()));
                    } else if let Some(root) = dl_clone.file() {
                        let child_file = root.child(info.name());
                        file_path = child_file.path();
                    }
                    
                    if let Some(path) = file_path {
                        if is_dir {
                            dl_clone.set_file(Some(&gio::File::for_path(&path)));
                            let _ = gio::Settings::new("com.agentic.md").set_string("last-folder", &path.to_string_lossy());
                            search_entry_for_activate.set_text("");
                        } else {
                            if let Some(current_path) = current_file_clone.borrow().as_deref() {
                                if crate::file_ops::is_editable_in_buffer(current_path) {
                                    crate::file_ops::save_to_path(&buf_clone, current_path);
                                }
                            }
                            
                            // Navigate to the file's folder if opened from search
                            if is_search_result {
                                if let Some(parent) = path.parent() {
                                    dl_clone.set_file(Some(&gio::File::for_path(parent)));
                                    let _ = gio::Settings::new("com.agentic.md").set_string("last-folder", &parent.to_string_lossy());
                                    search_entry_for_activate.set_text("");
                                }
                            }
                            
                            let _ = gio::Settings::new("com.agentic.md").set_string("last-file", &path.to_string_lossy());
                            *current_file_clone.borrow_mut() = Some(path.clone());
                            crate::file_ops::open_path(&win_clone, &buf_clone, &path);
                            refresh_preview_activate();
                        }
                    }
                }
            }
        });
        
        let filter_box = GtkBox::new(gtk4::Orientation::Horizontal, 6);
        filter_box.set_margin_start(4);
        filter_box.set_margin_end(4);
        filter_box.set_margin_top(4);
        filter_box.set_margin_bottom(4);
        filter_box.append(&search_entry);
        filter_box.append(&sort_drop);
        
        let scroll = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .child(&list_view)
            .vexpand(true)
            .build();
            
        container.append(&filter_box);
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
