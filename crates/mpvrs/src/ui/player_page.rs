use imgui::{StyleVar, Ui};

use crate::plumbing::audio::AudioPlayer;
use crate::plumbing::rendering::{SCREEN_H, SCREEN_W};
use crate::ui::components::cover_art::{
    draw_cover_art, draw_play_pause_icon, CoverArtCache, CoverSize,
};
use crate::ui::{draw_bottom_nav, NavAction};
pub struct PlayerView {
    error: Option<String>,
    seek_focused: bool,
}

impl PlayerView {
    pub fn new() -> Self {
        Self {
            error: None,
            seek_focused: false,
        }
    }

    pub fn is_seek_focused(&self) -> bool {
        self.seek_focused
    }

    pub fn draw(
        &mut self,
        ui: &Ui,
        player: Option<&AudioPlayer>,
        window_title: &str,
        show_back: bool,
        cover_cache: &mut CoverArtCache,
    ) -> Option<NavAction> {
        let Some(player) = player else {
            return self.draw_empty(ui, window_title, show_back);
        };
        let snapshot = player.snapshot();
        let mut nav_action = None;

        ui.window(&format!("{window_title}###Now playing"))
            .position([0.0, 0.0], imgui::Condition::Always)
            .size([SCREEN_W as f32, SCREEN_H as f32], imgui::Condition::Always)
            .movable(false)
            .resizable(false)
            .collapsible(false)
            .build(|| {
                ui.text("Now playing");
                ui.separator();

                ui.set_cursor_pos([24.0, 62.0]);
                draw_cover_art(
                    ui,
                    cover_cache,
                    snapshot.metadata.art_path.as_deref(),
                    CoverSize::Large,
                    [320.0, 320.0],
                );

                ui.set_cursor_pos([370.0, 62.0]);
                ui.child_window("player-details")
                    .size([SCREEN_W as f32 - 394.0, SCREEN_H as f32 - 160.0])
                    .build(|| {
                        ui.text(&snapshot.metadata.title);
                        if let Some(artist) = &snapshot.metadata.artist {
                            if ui
                                .selectable_config(&format!("{artist}##artist-link"))
                                .build()
                            {
                                nav_action = Some(NavAction::OpenArtist(artist.clone()));
                            }
                        }
                        if let Some(album) = &snapshot.metadata.album {
                            if ui
                                .selectable_config(&format!("{album}##album-link"))
                                .build()
                            {
                                nav_action = Some(NavAction::OpenAlbum(album.clone()));
                            }
                        }

                        ui.dummy([0.0, 34.0]);
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

                        ui.dummy([0.0, 24.0]);
                        let button_padding = ui.push_style_var(StyleVar::FramePadding([0.0, 0.0]));
                        if ui.button_with_size("##play-pause", [84.0, 84.0]) {
                            player.toggle_play_pause();
                        }
                        let button_min = ui.item_rect_min();
                        draw_play_pause_icon(
                            ui,
                            snapshot.is_playing,
                            [button_min[0] + 10.0, button_min[1] + 10.0],
                            64.0,
                        );
                        button_padding.pop();

                        ui.dummy([0.0, 16.0]);
                        ui.text_disabled(&snapshot.path);

                        if snapshot.status.to_ascii_lowercase().contains("failed") {
                            ui.text(format!("Status: {}", snapshot.status));
                        }
                        if let Some(error) = &self.error {
                            ui.text(format!("Error: {error}"));
                        }
                    });

                if let Some(bottom_nav_action) = draw_bottom_nav(ui, show_back, false) {
                    nav_action = Some(bottom_nav_action);
                }
            });

        nav_action
    }

    fn draw_empty(&mut self, ui: &Ui, window_title: &str, show_back: bool) -> Option<NavAction> {
        let mut nav_action = None;

        ui.window(&format!("{window_title}###Now playing"))
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
                nav_action = draw_bottom_nav(ui, show_back, false);
            });

        nav_action
    }
}

fn format_time(seconds: f32) -> String {
    let total = seconds.max(0.0) as u32;
    format!("{}:{:02}", total / 60, total % 60)
}
