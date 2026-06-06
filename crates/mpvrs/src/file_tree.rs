use std::fs;
use std::path::Path;
use std::time::Instant;

use imgui::{Condition, Ui};

use crate::rendering::{SCREEN_H, SCREEN_W};

const ROOT_PATH: &str = "ux0:/";

#[derive(Clone, Debug)]
struct FileEntry {
    name: String,
    path: String,
    is_dir: bool,
}

pub struct FileTreeView {
    current_dir: String,
    entries: Vec<FileEntry>,
    selected: Option<usize>,
    status: String,
    last_refresh: Instant,
    focus_selected: bool,
}

impl FileTreeView {
    pub fn new() -> Self {
        let mut view = Self {
            current_dir: ROOT_PATH.to_owned(),
            entries: Vec::new(),
            selected: None,
            status: String::new(),
            last_refresh: Instant::now(),
            focus_selected: true,
        };
        view.refresh();
        view
    }

    pub fn refresh(&mut self) {
        match read_one_level(&self.current_dir) {
            Ok(entries) => {
                self.entries = entries;
                self.selected = first_entry_index(&self.entries);
                self.status.clear();
                self.focus_selected = true;
            }
            Err(err) => {
                self.entries.clear();
                self.selected = None;
                self.status = format!("Failed to read {}: {err}", self.current_dir);
                self.focus_selected = false;
            }
        }
        self.last_refresh = Instant::now();
    }

    pub fn go_up(&mut self) {
        if self.current_dir == ROOT_PATH {
            self.status = format!("Already at {ROOT_PATH}");
            return;
        }

        self.current_dir = parent_dir(&self.current_dir);
        self.refresh();
    }

    pub fn draw(&mut self, ui: &Ui) {
        ui.window("File browser")
            .position([0.0, 0.0], Condition::Always)
            .size([SCREEN_W as f32, SCREEN_H as f32], Condition::Always)
            .movable(false)
            .resizable(false)
            .collapsible(false)
            .build(|| {
                ui.text(format!(
                    "{} — {} entries",
                    self.current_dir,
                    self.entries.len()
                ));
                ui.separator();

                let list_height = SCREEN_H as f32 - 112.0;
                ui.child_window("files")
                    .size([0.0, list_height])
                    .border(true)
                    .build(|| {
                        if self.current_dir != ROOT_PATH {
                            if ui.selectable_config("[UP] ..##parent").build() {
                                self.go_up();
                            }
                        }

                        let mut activated = None;
                        for (idx, entry) in self.entries.iter().enumerate() {
                            let prefix = if entry.is_dir { "[DIR]" } else { "     " };
                            let label = format!("{prefix} {}##{}", entry.name, idx);
                            let selected = self.selected == Some(idx);

                            if selected && self.focus_selected {
                                ui.set_keyboard_focus_here();
                            }

                            if ui.selectable_config(&label).selected(selected).build() {
                                activated = Some(idx);
                            }
                            if ui.is_item_focused() {
                                self.selected = Some(idx);
                                self.focus_selected = false;
                            }
                            if selected {
                                ui.set_item_default_focus();
                            }
                        }

                        if let Some(idx) = activated {
                            self.activate_entry(idx);
                        }
                    });

                ui.separator();
                ui.text("Cross: open/select  Circle: up  Triangle: refresh  Select: quit");
            });
    }

    fn activate_entry(&mut self, idx: usize) {
        let Some(entry) = self.entries.get(idx).cloned() else {
            return;
        };

        if entry.is_dir {
            self.current_dir = ensure_trailing_slash(&entry.path);
            self.refresh();
        } else {
            self.selected = Some(idx);
            self.status = format!("Selected file: {}", entry.path);
        }
    }
}

fn read_one_level(root: &str) -> Result<Vec<FileEntry>, std::io::Error> {
    let mut entries = Vec::new();

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().into_owned();

        entries.push(FileEntry {
            name,
            path: path_to_string(&path),
            is_dir: file_type.is_dir(),
        });
    }

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

fn first_entry_index(entries: &[FileEntry]) -> Option<usize> {
    if entries.is_empty() {
        None
    } else {
        Some(0)
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn ensure_trailing_slash(path: &str) -> String {
    if path.ends_with('/') {
        path.to_owned()
    } else {
        format!("{path}/")
    }
}

fn parent_dir(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.len() <= ROOT_PATH.trim_end_matches('/').len() {
        return ROOT_PATH.to_owned();
    }

    match trimmed.rfind('/') {
        Some(idx) if idx + 1 >= ROOT_PATH.len() => ensure_trailing_slash(&trimmed[..idx]),
        _ => ROOT_PATH.to_owned(),
    }
}
