use imgui::{StyleVar, Ui};

use crate::plumbing::audio::AudioPlayer;
use crate::plumbing::rendering::{
    create_rgba_texture, delete_texture, GlTexture, SCREEN_H, SCREEN_W,
};
use crate::queue::RepeatMode;
use crate::ui::components::cover_art::{draw_cover_art, CoverArtCache, CoverSize};
use crate::ui::{draw_bottom_nav, NavAction};
#[derive(Clone, Debug)]
pub struct PlayerQueueInfo {
    pub shuffle_enabled: bool,
    pub repeat_mode: RepeatMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerAction {
    Previous,
    Next,
    ToggleShuffle,
    CycleRepeat,
}

pub struct PlayerView {
    error: Option<String>,
    seek_focused: bool,
    icons: PlayerIconTextures,
}

impl PlayerView {
    pub fn new() -> Self {
        Self {
            error: None,
            seek_focused: false,
            icons: PlayerIconTextures::new(),
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
        queue: Option<PlayerQueueInfo>,
    ) -> (Option<NavAction>, Option<PlayerAction>) {
        let Some(player) = player else {
            return (self.draw_empty(ui, window_title, show_back), None);
        };
        let snapshot = player.snapshot();
        let mut nav_action = None;
        let mut player_action = None;

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

                        ui.dummy([0.0, 20.0]);
                        let button_padding = ui.push_style_var(StyleVar::FramePadding([0.0, 0.0]));
                        if icon_button(
                            ui,
                            &mut self.icons,
                            "##previous",
                            [52.0, 52.0],
                            IconKind::Previous,
                        ) {
                            player_action = Some(PlayerAction::Previous);
                        }
                        ui.same_line();
                        if icon_button(
                            ui,
                            &mut self.icons,
                            "##play-pause",
                            [84.0, 84.0],
                            IconKind::PlayPause(snapshot.is_playing),
                        ) {
                            player.toggle_play_pause();
                        }
                        ui.same_line();
                        if icon_button(ui, &mut self.icons, "##next", [52.0, 52.0], IconKind::Next)
                        {
                            player_action = Some(PlayerAction::Next);
                        }
                        ui.same_line();
                        let shuffle_active =
                            queue.as_ref().is_some_and(|queue| queue.shuffle_enabled);
                        if icon_button(
                            ui,
                            &mut self.icons,
                            "##shuffle",
                            [52.0, 52.0],
                            IconKind::Shuffle(shuffle_active),
                        ) {
                            player_action = Some(PlayerAction::ToggleShuffle);
                        }
                        ui.same_line();
                        let repeat_mode = queue
                            .as_ref()
                            .map(|queue| queue.repeat_mode)
                            .unwrap_or(RepeatMode::Off);
                        if icon_button(
                            ui,
                            &mut self.icons,
                            "##repeat",
                            [52.0, 52.0],
                            IconKind::Repeat(repeat_mode),
                        ) {
                            player_action = Some(PlayerAction::CycleRepeat);
                        }
                        button_padding.pop();

                        ui.dummy([0.0, 12.0]);
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

        (nav_action, player_action)
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

#[derive(Clone, Copy)]
enum IconKind {
    Previous,
    Next,
    PlayPause(bool),
    Shuffle(bool),
    Repeat(RepeatMode),
}

struct PlayerIconTextures {
    play: Option<GlTexture>,
    pause: Option<GlTexture>,
    previous: Option<GlTexture>,
    next: Option<GlTexture>,
    shuffle: Option<GlTexture>,
    shuffle_glow: Option<GlTexture>,
    repeat: Option<GlTexture>,
    repeat_glow: Option<GlTexture>,
    repeat_glow_one: Option<GlTexture>,
}

impl PlayerIconTextures {
    fn new() -> Self {
        Self {
            play: None,
            pause: None,
            previous: None,
            next: None,
            shuffle: None,
            shuffle_glow: None,
            repeat: None,
            repeat_glow: None,
            repeat_glow_one: None,
        }
    }

    fn texture_for(&mut self, icon: IconKind) -> Option<&GlTexture> {
        match icon {
            IconKind::Previous => load_icon(&mut self.previous, PLAY_ICON_W, PLAY_ICON_H, REW_RGBA),
            IconKind::Next => load_icon(&mut self.next, PLAY_ICON_W, PLAY_ICON_H, FF_RGBA),
            IconKind::PlayPause(true) => {
                load_icon(&mut self.pause, PLAY_ICON_W, PLAY_ICON_H, PAUSE_RGBA)
            }
            IconKind::PlayPause(false) => {
                load_icon(&mut self.play, PLAY_ICON_W, PLAY_ICON_H, PLAY_RGBA)
            }
            IconKind::Shuffle(true) => load_icon(
                &mut self.shuffle_glow,
                SMALL_ICON_SIZE,
                SMALL_ICON_SIZE,
                SHUFFLE_GLOW_RGBA,
            ),
            IconKind::Shuffle(false) => load_icon(
                &mut self.shuffle,
                SMALL_ICON_SIZE,
                SMALL_ICON_SIZE,
                SHUFFLE_RGBA,
            ),
            IconKind::Repeat(RepeatMode::Off) => load_icon(
                &mut self.repeat,
                SMALL_ICON_SIZE,
                SMALL_ICON_SIZE,
                REPEAT_RGBA,
            ),
            IconKind::Repeat(RepeatMode::One) => load_icon(
                &mut self.repeat_glow_one,
                SMALL_ICON_SIZE,
                SMALL_ICON_SIZE,
                REPEAT_GLOW_ONE_RGBA,
            ),
            IconKind::Repeat(RepeatMode::Queue) => load_icon(
                &mut self.repeat_glow,
                SMALL_ICON_SIZE,
                SMALL_ICON_SIZE,
                REPEAT_GLOW_RGBA,
            ),
        }
    }
}

impl Drop for PlayerIconTextures {
    fn drop(&mut self) {
        for texture in [
            self.play.take(),
            self.pause.take(),
            self.previous.take(),
            self.next.take(),
            self.shuffle.take(),
            self.shuffle_glow.take(),
            self.repeat.take(),
            self.repeat_glow.take(),
            self.repeat_glow_one.take(),
        ]
        .into_iter()
        .flatten()
        {
            delete_texture(texture.gl_id);
        }
    }
}

fn icon_button(
    ui: &Ui,
    icons: &mut PlayerIconTextures,
    id: &str,
    size: [f32; 2],
    icon: IconKind,
) -> bool {
    let clicked = ui.button_with_size(id, size);
    let min = ui.item_rect_min();
    let max = ui.item_rect_max();
    let Some(texture) = icons.texture_for(icon) else {
        return clicked;
    };

    let icon_size = fit_size(
        [texture_width(icon), texture_height(icon)],
        [max[0] - min[0] - 8.0, max[1] - min[1] - 8.0],
    );
    let pos = [
        min[0] + (max[0] - min[0] - icon_size[0]) * 0.5,
        min[1] + (max[1] - min[1] - icon_size[1]) * 0.5,
    ];
    ui.get_window_draw_list()
        .add_image(
            texture.texture_id,
            pos,
            [pos[0] + icon_size[0], pos[1] + icon_size[1]],
        )
        .build();

    clicked
}

fn load_icon<'a>(
    slot: &'a mut Option<GlTexture>,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Option<&'a GlTexture> {
    if slot.is_none() {
        *slot = create_rgba_texture(width, height, rgba);
    }
    slot.as_ref()
}

fn texture_width(icon: IconKind) -> f32 {
    match icon {
        IconKind::Previous | IconKind::Next | IconKind::PlayPause(_) => PLAY_ICON_W as f32,
        IconKind::Shuffle(_) | IconKind::Repeat(_) => SMALL_ICON_SIZE as f32,
    }
}

fn texture_height(icon: IconKind) -> f32 {
    match icon {
        IconKind::Previous | IconKind::Next | IconKind::PlayPause(_) => PLAY_ICON_H as f32,
        IconKind::Shuffle(_) | IconKind::Repeat(_) => SMALL_ICON_SIZE as f32,
    }
}

fn fit_size(source: [f32; 2], target: [f32; 2]) -> [f32; 2] {
    let scale = (target[0] / source[0]).min(target[1] / source[1]).max(0.0);
    [source[0] * scale, source[1] * scale]
}

const PLAY_ICON_W: u32 = 128;
const PLAY_ICON_H: u32 = 76;
const SMALL_ICON_SIZE: u32 = 64;

const PLAY_RGBA: &[u8] = include_bytes!("../../static/icons/elevenmpv/tex_button_play.rgba");
const PAUSE_RGBA: &[u8] = include_bytes!("../../static/icons/elevenmpv/tex_button_pause.rgba");
const REW_RGBA: &[u8] = include_bytes!("../../static/icons/elevenmpv/tex_button_rew.rgba");
const FF_RGBA: &[u8] = include_bytes!("../../static/icons/elevenmpv/tex_button_ff.rgba");
const SHUFFLE_RGBA: &[u8] = include_bytes!("../../static/icons/elevenmpv/tex_button_shuffle.rgba");
const SHUFFLE_GLOW_RGBA: &[u8] =
    include_bytes!("../../static/icons/elevenmpv/tex_button_shuffle_glow.rgba");
const REPEAT_RGBA: &[u8] = include_bytes!("../../static/icons/elevenmpv/tex_button_repeat.rgba");
const REPEAT_GLOW_RGBA: &[u8] =
    include_bytes!("../../static/icons/elevenmpv/tex_button_repeat_glow.rgba");
const REPEAT_GLOW_ONE_RGBA: &[u8] =
    include_bytes!("../../static/icons/elevenmpv/tex_button_repeat_glow_one.rgba");

fn format_time(seconds: f32) -> String {
    let total = seconds.max(0.0) as u32;
    format!("{}:{:02}", total / 60, total % 60)
}
