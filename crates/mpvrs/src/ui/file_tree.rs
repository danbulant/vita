use std::fs;
use std::path::Path;
use std::time::Instant;

use imgui::{Condition, Ui};

use crate::plumbing::audio::{is_supported_audio_file, PlaybackSnapshot};
use crate::plumbing::rendering::{SCREEN_H, SCREEN_W};
use crate::ui::components::scrollable_list::ScrollableList;
use crate::ui::{draw_bottom_nav, NavAction};

const ROOT_PATH: &str = "ux0:/";
const FILE_ROW_HEIGHT: f32 = 44.0;
const FILE_ICON_SIZE: f32 = 20.0;

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
    selection_visible: bool,
    files_list: ScrollableList,
}

#[derive(Clone, Debug)]
pub enum FileTreeAction {
    OpenAudio(String),
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
            selection_visible: true,
            files_list: ScrollableList::new(),
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
                self.selection_visible = true;
            }
            Err(err) => {
                self.entries.clear();
                self.selected = None;
                self.status = format!("Failed to read {}: {err}", self.current_dir);
                self.focus_selected = false;
                self.selection_visible = false;
            }
        }
        self.last_refresh = Instant::now();
    }

    pub fn set_status(&mut self, status: String) {
        self.status = status;
    }

    pub fn current_dir(&self) -> &str {
        &self.current_dir
    }

    pub fn can_go_back(&self) -> bool {
        self.current_dir != ROOT_PATH
    }

    pub fn go_up(&mut self) {
        if self.current_dir == ROOT_PATH {
            self.status = format!("Already at {ROOT_PATH}");
            return;
        }

        self.current_dir = parent_dir(&self.current_dir);
        self.refresh();
    }

    pub fn draw(
        &mut self,
        ui: &Ui,
        playback: Option<&PlaybackSnapshot>,
        controller_navigation_active: bool,
        window_title: &str,
        show_back: bool,
        show_player: bool,
    ) -> (Option<FileTreeAction>, Option<NavAction>) {
        let mut action = None;
        let mut nav_action = None;

        if controller_navigation_active && !self.selection_visible {
            self.selection_visible = true;
            self.focus_selected = true;
        }

        ui.window(&format!("{window_title}###File browser"))
            .position([0.0, 0.0], Condition::Always)
            .size([SCREEN_W as f32, SCREEN_H as f32], Condition::Always)
            .movable(false)
            .resizable(false)
            .collapsible(false)
            .build(|| {
                ui.text(format!(
                    "{} - {} entries",
                    self.current_dir,
                    self.entries.len()
                ));
                ui.separator();

                let list_height = SCREEN_H as f32 - 160.0;
                let mut files_list = std::mem::take(&mut self.files_list);
                files_list.draw(ui, "files", [0.0, list_height], true, |ui, touch| {
                    if touch.touch_started {
                        self.selection_visible = false;
                        self.focus_selected = false;
                    }

                    if self.current_dir != ROOT_PATH {
                        if draw_file_row(
                            ui,
                            "..##parent",
                            FileRowIcon::Up,
                            false,
                            touch.disable_hover,
                        ) && !touch.suppress_click
                        {
                            let keep_selection_hidden = !self.selection_visible;
                            self.go_up();
                            if keep_selection_hidden {
                                self.selection_visible = false;
                                self.focus_selected = false;
                            }
                        }
                    }

                    let mut activated = None;
                    for (idx, entry) in self.entries.iter().enumerate() {
                        let icon = if entry.is_dir {
                            FileRowIcon::Folder
                        } else if is_supported_audio_file(&entry.path) {
                            FileRowIcon::Music
                        } else {
                            FileRowIcon::File
                        };
                        let label = format!("{}##{}", entry.name, idx);
                        let selected = self.selected == Some(idx);
                        let show_selected = selected && self.selection_visible;

                        if show_selected && self.focus_selected {
                            ui.set_keyboard_focus_here();
                        }

                        if draw_file_row(ui, &label, icon, show_selected, touch.disable_hover)
                            && !touch.suppress_click
                        {
                            activated = Some(idx);
                        }
                        if self.selection_visible && ui.is_item_focused() {
                            self.selected = Some(idx);
                            self.focus_selected = false;
                        }
                        if show_selected {
                            ui.set_item_default_focus();
                        }
                    }

                    if let Some(idx) = activated {
                        let keep_selection_hidden = !self.selection_visible;
                        action = self.activate_entry(idx);
                        if keep_selection_hidden {
                            self.selection_visible = false;
                            self.focus_selected = false;
                        }
                    }
                });
                self.files_list = files_list;

                ui.separator();
                if let Some(playback) = playback {
                    let state = if playback.is_playing {
                        "playing"
                    } else {
                        "paused"
                    };
                    ui.text(format!(
                        "Now {state}: {} ({})",
                        playback.name, playback.status
                    ));
                } else if !self.status.is_empty() {
                    ui.text(&self.status);
                }

                nav_action = draw_bottom_nav(ui, show_back, show_player);
            });

        (action, nav_action)
    }

    fn activate_entry(&mut self, idx: usize) -> Option<FileTreeAction> {
        let Some(entry) = self.entries.get(idx).cloned() else {
            return None;
        };

        if entry.is_dir {
            self.current_dir = ensure_trailing_slash(&entry.path);
            self.refresh();
            None
        } else if is_supported_audio_file(&entry.path) {
            self.selected = Some(idx);
            self.status = format!("Playing: {}", entry.path);
            Some(FileTreeAction::OpenAudio(entry.path))
        } else {
            self.selected = Some(idx);
            self.status = format!("Unsupported file: {}", entry.path);
            None
        }
    }
}

#[derive(Clone, Copy)]
enum FileRowIcon {
    Up,
    Folder,
    Music,
    File,
}

fn draw_file_row(
    ui: &Ui,
    label: &str,
    icon: FileRowIcon,
    selected: bool,
    disable_hover: bool,
) -> bool {
    let (visible_label, id) = split_imgui_label(label);
    let hidden_label = format!("##{id}");
    let clicked = ui
        .selectable_config(&hidden_label)
        .selected(selected)
        .disabled(disable_hover)
        .size([0.0, FILE_ROW_HEIGHT])
        .build();

    let min = ui.item_rect_min();
    let max = ui.item_rect_max();
    let row_height = max[1] - min[1];
    let icon_y = min[1] + (row_height - FILE_ICON_SIZE) * 0.5;
    let icon_x = min[0] + 8.0;
    draw_file_icon(ui, icon, [icon_x, icon_y], FILE_ICON_SIZE);

    let text_size = ui.calc_text_size(visible_label);
    let text_x = icon_x + FILE_ICON_SIZE + 12.0;
    let text_y = min[1] + (row_height - text_size[1]) * 0.5;
    ui.get_window_draw_list()
        .add_text([text_x, text_y], [0.92, 0.94, 0.98, 1.0], visible_label);

    clicked
}

fn split_imgui_label(label: &str) -> (&str, &str) {
    match label.split_once("##") {
        Some((visible, id)) => (visible, id),
        None => (label, label),
    }
}

fn draw_file_icon(ui: &Ui, icon: FileRowIcon, pos: [f32; 2], size: f32) {
    let draw_list = ui.get_window_draw_list();
    let color = [0.86, 0.90, 0.96, 1.0];
    let accent = [0.42, 0.72, 1.0, 1.0];
    let x = pos[0];
    let y = pos[1];
    let s = size;

    match icon {
        FileRowIcon::Up => {
            draw_list
                .add_triangle(
                    [x + s * 0.5, y + s * 0.15],
                    [x + s * 0.12, y + s * 0.55],
                    [x + s * 0.88, y + s * 0.55],
                    accent,
                )
                .filled(true)
                .build();
            draw_list
                .add_rect(
                    [x + s * 0.38, y + s * 0.50],
                    [x + s * 0.62, y + s * 0.90],
                    accent,
                )
                .filled(true)
                .build();
        }
        FileRowIcon::Folder => {
            draw_list
                .add_rect(
                    [x + s * 0.08, y + s * 0.30],
                    [x + s * 0.44, y + s * 0.48],
                    accent,
                )
                .filled(true)
                .build();
            draw_list
                .add_rect(
                    [x + s * 0.08, y + s * 0.42],
                    [x + s * 0.92, y + s * 0.86],
                    color,
                )
                .rounding(2.0)
                .filled(true)
                .build();
        }
        FileRowIcon::Music => {
            draw_list
                .add_line(
                    [x + s * 0.58, y + s * 0.18],
                    [x + s * 0.58, y + s * 0.70],
                    accent,
                )
                .thickness(3.0)
                .build();
            draw_list
                .add_line(
                    [x + s * 0.58, y + s * 0.18],
                    [x + s * 0.84, y + s * 0.28],
                    accent,
                )
                .thickness(3.0)
                .build();
            draw_list
                .add_circle([x + s * 0.42, y + s * 0.74], s * 0.18, color)
                .filled(true)
                .build();
        }
        FileRowIcon::File => {
            draw_list
                .add_rect(
                    [x + s * 0.24, y + s * 0.10],
                    [x + s * 0.78, y + s * 0.90],
                    color,
                )
                .rounding(1.5)
                .build();
            draw_list
                .add_line(
                    [x + s * 0.36, y + s * 0.40],
                    [x + s * 0.66, y + s * 0.40],
                    accent,
                )
                .thickness(2.0)
                .build();
            draw_list
                .add_line(
                    [x + s * 0.36, y + s * 0.58],
                    [x + s * 0.66, y + s * 0.58],
                    accent,
                )
                .thickness(2.0)
                .build();
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
