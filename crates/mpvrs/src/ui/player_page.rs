use imgui::{StyleVar, Ui};

use crate::plumbing::audio::AudioPlayer;
use crate::plumbing::rendering::{SCREEN_H, SCREEN_W};
use crate::ui::file_tree::FileTreeView;

pub struct PlayerView {
    browser: FileTreeView,
    error: Option<String>,
    seek_focused: bool,
}

impl PlayerView {
    pub fn new(browser: FileTreeView) -> Self {
        Self {
            browser,
            error: None,
            seek_focused: false,
        }
    }

    pub fn refresh_browser(&mut self) {
        self.browser.refresh();
    }

    pub fn into_browser(self) -> FileTreeView {
        self.browser
    }

    pub fn is_seek_focused(&self) -> bool {
        self.seek_focused
    }

    pub fn draw(&mut self, ui: &Ui, player: Option<&AudioPlayer>) {
        let Some(player) = player else {
            self.draw_empty(ui);
            return;
        };
        let snapshot = player.snapshot();

        ui.window("Now playing")
            .position([0.0, 0.0], imgui::Condition::Always)
            .size([SCREEN_W as f32, SCREEN_H as f32], imgui::Condition::Always)
            .movable(false)
            .resizable(false)
            .collapsible(false)
            .build(|| {
                ui.text("Now playing");
                ui.separator();
                ui.text(&snapshot.name);
                ui.text(&snapshot.path);
                ui.spacing();

                let mut progress = if let Some(duration) = snapshot.duration_seconds {
                    if duration > 0.0 {
                        (snapshot.position_seconds / duration).clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

                let label = if let Some(duration) = snapshot.duration_seconds {
                    format!(
                        "{} / {}",
                        format_time(snapshot.position_seconds),
                        format_time(duration)
                    )
                } else {
                    format!("{} / --:--", format_time(snapshot.position_seconds))
                };

                let seek_padding = ui.push_style_var(StyleVar::FramePadding([0.0, 10.0]));
                if ui
                    .slider_config("##seek", 0.0_f32, 1.0_f32)
                    .display_format("")
                    .build(&mut progress)
                {
                    player.seek_percent(progress);
                }
                self.seek_focused = ui.is_item_focused();
                seek_padding.pop();
                ui.text(label);

                ui.spacing();
                let button_padding = ui.push_style_var(StyleVar::FramePadding([16.0, 12.0]));
                let button = if snapshot.is_playing { "Pause" } else { "Play" };
                if ui.button_with_size(button, [136.0, 48.0]) {
                    player.toggle_play_pause();
                }
                button_padding.pop();

                if !snapshot.status.is_empty() {
                    ui.same_line();
                    ui.text(snapshot.status);
                }
                if let Some(error) = &self.error {
                    ui.text(format!("Error: {error}"));
                }

                ui.separator();
                ui.text("Cross: play/pause/seek  Circle: file browser  Select: quit");
            });
    }

    fn draw_empty(&mut self, ui: &Ui) {
        ui.window("Now playing")
            .position([0.0, 0.0], imgui::Condition::Always)
            .size([SCREEN_W as f32, SCREEN_H as f32], imgui::Condition::Always)
            .movable(false)
            .resizable(false)
            .collapsible(false)
            .build(|| {
                ui.text("No track loaded");
                if let Some(error) = &self.error {
                    ui.text(format!("Error: {error}"));
                }
                ui.separator();
                ui.text("Circle: file browser  Select: quit");
            });
    }
}

fn format_time(seconds: f32) -> String {
    let total = seconds.max(0.0) as u32;
    format!("{}:{:02}", total / 60, total % 60)
}
