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
}

impl FileTreeView {
    pub fn new() -> Self {
        let mut view = Self {
            current_dir: ROOT_PATH.to_owned(),
            entries: Vec::new(),
            selected: None,
            status: String::new(),
            last_refresh: Instant::now(),
        };
        view.refresh();
        view
    }

    pub fn refresh(&mut self) {
        match read_one_level(&self.current_dir) {
            Ok(entries) => {
                self.entries = entries;
                self.selected = self.selected.filter(|&idx| idx < self.entries.len());
                self.status = format!("{} entries in {}", self.entries.len(), self.current_dir);
            }
            Err(err) => {
                self.entries.clear();
                self.selected = None;
                self.status = format!("Failed to read {}: {err}", self.current_dir);
            }
        }
        self.last_refresh = Instant::now();
    }

    pub fn open_selected(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            self.status = "No entry selected".to_owned();
            return;
        };

        if entry.is_dir {
            self.current_dir = ensure_trailing_slash(&entry.path);
            self.selected = None;
            self.refresh();
        } else {
            self.status = format!("Selected file: {}", entry.path);
        }
    }

    pub fn go_up(&mut self) {
        if self.current_dir == ROOT_PATH {
            self.status = format!("Already at {ROOT_PATH}");
            return;
        }

        self.current_dir = parent_dir(&self.current_dir);
        self.selected = None;
        self.refresh();
    }

    pub fn draw(&mut self, ui: &Ui) {
        ui.window("mpvrs")
            .position([0.0, 0.0], Condition::Always)
            .size([SCREEN_W as f32, SCREEN_H as f32], Condition::Always)
            .movable(false)
            .resizable(false)
            .collapsible(false)
            .build(|| {
                ui.text("File browser");
                ui.same_line();
                if ui.button("Refresh (Triangle)") {
                    self.refresh();
                }
                ui.same_line();
                if ui.button("Up (Circle)") {
                    self.go_up();
                }

                ui.separator();
                ui.text(format!("Path: {}", self.current_dir));
                ui.text(&self.status);

                if let Some(selected) = self.selected_entry() {
                    let action = if selected.is_dir {
                        "Cross opens"
                    } else {
                        "Cross selects"
                    };
                    ui.text(format!("{action}: {}", selected.path));
                } else {
                    ui.text("Select with touch/d-pad. Cross opens folders. Circle goes up.");
                }

                ui.separator();

                let list_height = SCREEN_H as f32 - 170.0;
                ui.child_window("files")
                    .size([0.0, list_height])
                    .border(true)
                    .build(|| {
                        if self.current_dir != ROOT_PATH {
                            if ui.selectable_config("[UP] ..##parent").build() {
                                self.go_up();
                            }
                        }

                        for (idx, entry) in self.entries.iter().enumerate() {
                            let prefix = if entry.is_dir { "[DIR]" } else { "     " };
                            let label = format!("{prefix} {}##{}", entry.name, idx);
                            let selected = self.selected == Some(idx);

                            if ui.selectable_config(&label).selected(selected).build() {
                                self.selected = Some(idx);
                                self.status = format!("Selected {}", entry.path);
                            }
                        }
                    });
            });
    }

    fn selected_entry(&self) -> Option<&FileEntry> {
        self.selected.and_then(|idx| self.entries.get(idx))
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
