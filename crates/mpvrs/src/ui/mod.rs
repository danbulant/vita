pub mod components;
pub mod file_tree;
pub mod library_page;
pub mod player_page;

use imgui::{StyleVar, Ui};

use crate::plumbing::rendering::{SCREEN_H, SCREEN_W};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavAction {
    Back,
    OpenPlayer,
}

pub fn draw_bottom_nav(ui: &Ui, show_back: bool, show_player: bool) -> Option<NavAction> {
    let mut action = None;
    let _padding = ui.push_style_var(StyleVar::FramePadding([16.0, 12.0]));
    let button_size = [112.0, 48.0];
    let bottom_y = SCREEN_H as f32 - 86.0;

    if show_back {
        ui.set_cursor_pos([12.0, bottom_y]);
        if ui.button_with_size("Back", button_size) {
            action = Some(NavAction::Back);
        }
    }

    if show_player {
        ui.set_cursor_pos([SCREEN_W as f32 - button_size[0] - 12.0, bottom_y]);
        if ui.button_with_size("Player", button_size) {
            action = Some(NavAction::OpenPlayer);
        }
    }

    action
}
