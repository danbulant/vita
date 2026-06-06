#[link(name = "vitaGL", kind = "static")]
#[link(name = "vitashark", kind = "static")]
#[link(name = "SceShaccCg_stub", kind = "static")]
#[link(name = "mathneon", kind = "static")]
#[link(name = "SceShaccCgExt", kind = "static")]
#[link(name = "taihen_stub", kind = "static")]
extern "C" {}

#[no_mangle]
pub static mut _newlib_heap_size_user: i32 = 64 * 1024 * 1024;

mod file_tree;
mod rendering;

use std::backtrace::Backtrace;
use std::panic;
use std::thread;
use std::time::{Duration, Instant};

use file_tree::FileTreeView;
use imgui::{BackendFlags, ConfigFlags, Key, NavInput, Ui};
use rendering::{clear_screen, init_vitagl, present, VitaGlImguiRenderer, SCREEN_H, SCREEN_W};
use vitasdk_sys::{
    sceCtrlPeekBufferPositive, sceCtrlSetSamplingMode, sceTouchPeek, sceTouchSetSamplingState,
    SceCtrlData, SceTouchData, SCE_CTRL_CIRCLE, SCE_CTRL_CROSS, SCE_CTRL_DOWN, SCE_CTRL_LEFT,
    SCE_CTRL_LTRIGGER, SCE_CTRL_RIGHT, SCE_CTRL_RTRIGGER, SCE_CTRL_SELECT, SCE_CTRL_SQUARE,
    SCE_CTRL_TRIANGLE, SCE_CTRL_UP, SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_START,
};

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

struct AppState {
    page: Page,
}

enum Page {
    FileTree(FileTreeView),
}

impl AppState {
    fn new() -> Self {
        Self {
            page: Page::FileTree(FileTreeView::new()),
        }
    }

    fn refresh(&mut self) {
        match &mut self.page {
            Page::FileTree(page) => page.refresh(),
        }
    }

    fn back(&mut self) {
        match &mut self.page {
            Page::FileTree(page) => page.go_up(),
        }
    }

    fn draw(&mut self, ui: &Ui) {
        match &mut self.page {
            Page::FileTree(page) => page.draw(ui),
        }
    }
}

fn main() {
    install_panic_hook();

    unsafe {
        sceCtrlSetSamplingMode(1);
        sceTouchSetSamplingState(SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_START);
    }
    init_vitagl();

    let mut imgui = imgui::Context::create();
    imgui.set_ini_filename(None);
    configure_imgui_io(&mut imgui);

    let mut renderer = VitaGlImguiRenderer::new(&mut imgui);
    let mut app = AppState::new();
    let mut last_frame = Instant::now();
    let mut previous_buttons = 0;

    loop {
        let ctrl = read_ctrl();
        if pressed(&ctrl, SCE_CTRL_SELECT) {
            break;
        }

        let just_pressed = ctrl.buttons & !previous_buttons;
        previous_buttons = ctrl.buttons;

        if (just_pressed & SCE_CTRL_TRIANGLE) != 0 {
            app.refresh();
        }
        if (just_pressed & SCE_CTRL_CIRCLE) != 0 {
            app.back();
        }

        let touch = read_front_touch();

        let now = Instant::now();
        let delta = now.saturating_duration_since(last_frame);
        last_frame = now;

        {
            let io = imgui.io_mut();
            io.update_delta_time(delta);
            io.display_size = [SCREEN_W as f32, SCREEN_H as f32];
            apply_controller_to_imgui(io, &ctrl);
            apply_touch_to_imgui(io, touch);
        }

        let ui = imgui.frame();
        app.draw(ui);

        clear_screen();
        let draw_data = imgui.render();
        renderer.render(draw_data);
        present();
    }

    drop(renderer);
}

fn configure_imgui_io(imgui: &mut imgui::Context) {
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

fn install_panic_hook() {
    panic::set_hook(Box::new(|info| {
        eprintln!("mpvrs panic: {info}");
        eprintln!("{}", Backtrace::force_capture());
        thread::sleep(Duration::from_secs(5));
    }));
}

fn read_ctrl() -> SceCtrlData {
    let mut ctrl = unsafe { std::mem::zeroed::<SceCtrlData>() };
    unsafe {
        sceCtrlPeekBufferPositive(0, &mut ctrl, 1);
    }
    ctrl
}

fn read_front_touch() -> Option<(f32, f32)> {
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

fn apply_touch_to_imgui(io: &mut imgui::Io, touch: Option<(f32, f32)>) {
    if let Some((x, y)) = touch {
        io.mouse_pos = [x, y];
        io.mouse_down[0] = true;
    } else {
        io.mouse_down[0] = false;
    }
}

fn apply_controller_to_imgui(io: &mut imgui::Io, ctrl: &SceCtrlData) {
    for key in io.keys_down.iter_mut() {
        *key = false;
    }
    set_key(io, VITA_IMGUI_KEY_UP, pressed(ctrl, SCE_CTRL_UP));
    set_key(io, VITA_IMGUI_KEY_DOWN, pressed(ctrl, SCE_CTRL_DOWN));
    set_key(io, VITA_IMGUI_KEY_LEFT, pressed(ctrl, SCE_CTRL_LEFT));
    set_key(io, VITA_IMGUI_KEY_RIGHT, pressed(ctrl, SCE_CTRL_RIGHT));
    set_key(io, VITA_IMGUI_KEY_CROSS, pressed(ctrl, SCE_CTRL_CROSS));

    for input in NavInput::VARIANTS {
        io.nav_inputs[input as usize] = 0.0;
    }

    set_nav_button(io, NavInput::Activate, pressed(ctrl, SCE_CTRL_CROSS));
    set_nav_button(io, NavInput::Menu, pressed(ctrl, SCE_CTRL_SQUARE));
    set_nav_button(io, NavInput::Input, pressed(ctrl, SCE_CTRL_TRIANGLE));
    set_nav_button(io, NavInput::DpadLeft, pressed(ctrl, SCE_CTRL_LEFT));
    set_nav_button(io, NavInput::DpadRight, pressed(ctrl, SCE_CTRL_RIGHT));
    set_nav_button(io, NavInput::DpadUp, pressed(ctrl, SCE_CTRL_UP));
    set_nav_button(io, NavInput::DpadDown, pressed(ctrl, SCE_CTRL_DOWN));
    set_nav_button(io, NavInput::FocusPrev, pressed(ctrl, SCE_CTRL_LTRIGGER));
    set_nav_button(io, NavInput::FocusNext, pressed(ctrl, SCE_CTRL_RTRIGGER));
}

fn pressed(ctrl: &SceCtrlData, button: u32) -> bool {
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
