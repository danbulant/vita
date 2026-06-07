use imgui::{BackendFlags, ConfigFlags, Key, NavInput};
use vitasdk_sys::{
    sceCtrlPeekBufferPositive, sceCtrlSetSamplingMode, sceTouchPeek, sceTouchSetSamplingState,
    SceCtrlData, SceTouchData, SCE_CTRL_CIRCLE, SCE_CTRL_CROSS, SCE_CTRL_DOWN, SCE_CTRL_LEFT,
    SCE_CTRL_LTRIGGER, SCE_CTRL_RIGHT, SCE_CTRL_RTRIGGER, SCE_CTRL_SELECT, SCE_CTRL_SQUARE,
    SCE_CTRL_TRIANGLE, SCE_CTRL_UP, SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_START,
};

use crate::plumbing::rendering::{SCREEN_H, SCREEN_W};

const VITA_TOUCH_W: f32 = 1919.0;
const VITA_TOUCH_H: f32 = 1087.0;

// Dear ImGui's legacy `io.KeyMap` maps ImGui keys to indices in
// `io.KeysDown`, so values must be small key indices (< 512), not platform
// button bitmasks like `SCE_CTRL_UP`.
const VITA_IMGUI_KEY_UP: u32 = 256;
const VITA_IMGUI_KEY_DOWN: u32 = 257;
const VITA_IMGUI_KEY_LEFT: u32 = 258;
const VITA_IMGUI_KEY_RIGHT: u32 = 259;
const VITA_IMGUI_KEY_CROSS: u32 = 260;

#[derive(Clone, Copy)]
pub enum Button {
    Select,
    Triangle,
    Circle,
    Left,
    Right,
}

#[derive(Clone, Copy)]
pub struct ControllerState {
    buttons: u32,
}

impl ControllerState {
    pub fn buttons(self) -> u32 {
        self.buttons
    }
}

pub fn init() {
    unsafe {
        sceCtrlSetSamplingMode(1);
        sceTouchSetSamplingState(SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_START);
    }
}

pub fn configure_imgui_io(imgui: &mut imgui::Context) {
    let io = imgui.io_mut();
    io.display_size = [SCREEN_W as f32, SCREEN_H as f32];
    io.config_flags |= ConfigFlags::NAV_ENABLE_KEYBOARD;
    io.config_flags |= ConfigFlags::NAV_ENABLE_GAMEPAD;
    io.config_flags |= ConfigFlags::IS_TOUCH_SCREEN;
    io.backend_flags |= BackendFlags::HAS_GAMEPAD;

    io.key_map[Key::UpArrow as usize] = VITA_IMGUI_KEY_UP;
    io.key_map[Key::DownArrow as usize] = VITA_IMGUI_KEY_DOWN;
    io.key_map[Key::LeftArrow as usize] = VITA_IMGUI_KEY_LEFT;
    io.key_map[Key::RightArrow as usize] = VITA_IMGUI_KEY_RIGHT;
    io.key_map[Key::Enter as usize] = VITA_IMGUI_KEY_CROSS;
    io.key_map[Key::Space as usize] = VITA_IMGUI_KEY_CROSS;
}

pub fn read_controller() -> ControllerState {
    let mut ctrl = unsafe { std::mem::zeroed::<SceCtrlData>() };
    unsafe {
        sceCtrlPeekBufferPositive(0, &mut ctrl, 1);
    }
    ControllerState {
        buttons: ctrl.buttons,
    }
}

pub fn read_front_touch() -> Option<(f32, f32)> {
    let mut touch = unsafe { std::mem::zeroed::<SceTouchData>() };
    let result = unsafe { sceTouchPeek(SCE_TOUCH_PORT_FRONT, &mut touch, 1) };
    if result <= 0 || touch.reportNum == 0 {
        return None;
    }

    let report = touch.report[0];
    let x = (report.x as f32 / VITA_TOUCH_W) * SCREEN_W as f32;
    let y = (report.y as f32 / VITA_TOUCH_H) * SCREEN_H as f32;
    Some((x.clamp(0.0, SCREEN_W as f32), y.clamp(0.0, SCREEN_H as f32)))
}

pub fn pressed(ctrl: &ControllerState, button: Button) -> bool {
    (ctrl.buttons & button_mask(button)) != 0
}

pub fn just_pressed(current_buttons: u32, previous_buttons: u32, button: Button) -> bool {
    (current_buttons & !previous_buttons & button_mask(button)) != 0
}

pub fn apply_to_imgui(io: &mut imgui::Io, ctrl: &ControllerState, touch: Option<(f32, f32)>) {
    apply_controller_to_imgui(io, ctrl);
    apply_touch_to_imgui(io, touch);
}

fn apply_touch_to_imgui(io: &mut imgui::Io, touch: Option<(f32, f32)>) {
    if let Some((x, y)) = touch {
        io.mouse_pos = [x, y];
        io.mouse_down[0] = true;
    } else {
        io.mouse_down[0] = false;
    }
}

fn apply_controller_to_imgui(io: &mut imgui::Io, ctrl: &ControllerState) {
    for key in io.keys_down.iter_mut() {
        *key = false;
    }
    set_key(io, VITA_IMGUI_KEY_UP, ctrl_pressed(ctrl, SCE_CTRL_UP));
    set_key(io, VITA_IMGUI_KEY_DOWN, ctrl_pressed(ctrl, SCE_CTRL_DOWN));
    set_key(io, VITA_IMGUI_KEY_LEFT, ctrl_pressed(ctrl, SCE_CTRL_LEFT));
    set_key(io, VITA_IMGUI_KEY_RIGHT, ctrl_pressed(ctrl, SCE_CTRL_RIGHT));
    set_key(io, VITA_IMGUI_KEY_CROSS, ctrl_pressed(ctrl, SCE_CTRL_CROSS));

    for input in NavInput::VARIANTS {
        io.nav_inputs[input as usize] = 0.0;
    }

    set_nav_button(io, NavInput::Activate, ctrl_pressed(ctrl, SCE_CTRL_CROSS));
    set_nav_button(io, NavInput::Menu, ctrl_pressed(ctrl, SCE_CTRL_SQUARE));
    set_nav_button(io, NavInput::Input, ctrl_pressed(ctrl, SCE_CTRL_TRIANGLE));
    set_nav_button(io, NavInput::DpadLeft, ctrl_pressed(ctrl, SCE_CTRL_LEFT));
    set_nav_button(io, NavInput::DpadRight, ctrl_pressed(ctrl, SCE_CTRL_RIGHT));
    set_nav_button(io, NavInput::DpadUp, ctrl_pressed(ctrl, SCE_CTRL_UP));
    set_nav_button(io, NavInput::DpadDown, ctrl_pressed(ctrl, SCE_CTRL_DOWN));
    set_nav_button(
        io,
        NavInput::FocusPrev,
        ctrl_pressed(ctrl, SCE_CTRL_LTRIGGER),
    );
    set_nav_button(
        io,
        NavInput::FocusNext,
        ctrl_pressed(ctrl, SCE_CTRL_RTRIGGER),
    );
}

fn button_mask(button: Button) -> u32 {
    match button {
        Button::Select => SCE_CTRL_SELECT,
        Button::Triangle => SCE_CTRL_TRIANGLE,
        Button::Circle => SCE_CTRL_CIRCLE,
        Button::Left => SCE_CTRL_LEFT,
        Button::Right => SCE_CTRL_RIGHT,
    }
}

fn ctrl_pressed(ctrl: &ControllerState, button: u32) -> bool {
    (ctrl.buttons & button) != 0
}

fn set_key(io: &mut imgui::Io, key: u32, pressed: bool) {
    let idx = key as usize;
    if idx < io.keys_down.len() {
        io.keys_down[idx] = pressed;
    }
}

fn set_nav_button(io: &mut imgui::Io, input: NavInput, pressed: bool) {
    io.nav_inputs[input as usize] = if pressed { 1.0 } else { 0.0 };
}
