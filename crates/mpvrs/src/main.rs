#[link(name = "vitaGL", kind = "static")]
#[link(name = "vitashark", kind = "static")]
#[link(name = "SceShaccCg_stub", kind = "static")]
#[link(name = "mathneon", kind = "static")]
#[link(name = "SceShaccCgExt", kind = "static")]
#[link(name = "taihen_stub", kind = "static")]
extern "C" {}

#[no_mangle]
pub static mut _newlib_heap_size_user: i32 = 64 * 1024 * 1024;

mod library;
mod plumbing;
mod ui;

use std::backtrace::Backtrace;
use std::panic;
use std::thread;
use std::time::{Duration, Instant};

use imgui::Ui;
use plumbing::audio::AudioPlayer;
use plumbing::input::{self, Button};
use plumbing::rendering::{
    clear_screen, init_vitagl, present, VitaGlImguiRenderer, SCREEN_H, SCREEN_W,
};
use ui::file_tree::{FileTreeAction, FileTreeView};
use ui::library_page::{LibraryAction, LibraryView};
use ui::player_page::PlayerView;

const SEEK_STEP_SECONDS: f32 = 10.0;

struct AppState {
    page: Page,
    history: Vec<Page>,
    player: Option<AudioPlayer>,
}

enum Page {
    FileTree(FileTreeView),
    Library(LibraryView),
    Player(PlayerView),
}

impl AppState {
    fn new() -> Self {
        Self {
            page: Page::Library(LibraryView::new(FileTreeView::new(), false)),
            history: Vec::new(),
            player: None,
        }
    }

    fn refresh(&mut self) {
        match &mut self.page {
            Page::FileTree(page) => page.refresh(),
            Page::Library(page) => page.refresh(),
            Page::Player(_) => {}
        }
    }

    fn back(&mut self) {
        match &mut self.page {
            Page::FileTree(page) => page.go_up(),
            Page::Library(page) => {
                page.back();
            }
            Page::Player(_) => self.pop_history_or_stay(),
        }
    }

    fn open_library(&mut self) {
        match &mut self.page {
            Page::Library(page) => page.home(),
            Page::FileTree(_) => {
                let Page::FileTree(browser) =
                    std::mem::replace(&mut self.page, Page::FileTree(FileTreeView::new()))
                else {
                    unreachable!();
                };
                self.page = Page::Library(LibraryView::new(browser, true));
            }
            Page::Player(_) => {}
        }
    }

    fn open_file_browser(&mut self) {
        let Page::Library(page) =
            std::mem::replace(&mut self.page, Page::FileTree(FileTreeView::new()))
        else {
            unreachable!();
        };
        self.page = Page::FileTree(page.into_browser());
    }

    fn pop_history_or_stay(&mut self) {
        if let Some(previous_page) = self.history.pop() {
            self.page = previous_page;
        }
    }

    fn seek_relative_seconds(&self, seconds: f32) {
        let Page::Player(page) = &self.page else {
            return;
        };
        if !page.is_seek_focused() {
            return;
        }
        if let Some(player) = &self.player {
            player.seek_relative_seconds(seconds);
        }
    }

    fn draw(&mut self, ui: &Ui, controller_navigation_active: bool) {
        let playback = self.player.as_ref().map(|player| player.snapshot());
        let mut browse_files = false;
        let action = match &mut self.page {
            Page::FileTree(page) => page.draw(ui, playback.as_ref(), controller_navigation_active),
            Page::Library(page) => match page.draw(ui) {
                Some(LibraryAction::BrowseFiles) => {
                    browse_files = true;
                    None
                }
                Some(LibraryAction::OpenAudio(path)) => Some(FileTreeAction::OpenAudio(path)),
                None => None,
            },
            Page::Player(page) => {
                page.draw(ui, self.player.as_ref());
                None
            }
        };

        if browse_files {
            self.open_file_browser();
            return;
        }

        if let Some(FileTreeAction::OpenAudio(path)) = action {
            let mut previous_page =
                std::mem::replace(&mut self.page, Page::Player(PlayerView::new()));

            self.player = None;

            match AudioPlayer::open(path) {
                Ok(player) => {
                    self.player = Some(player);
                    self.history.push(previous_page);
                }
                Err(err) => {
                    match &mut previous_page {
                        Page::FileTree(page) => {
                            page.set_status(format!("Failed to open audio: {err}"))
                        }
                        Page::Library(page) => {
                            page.set_status(format!("Failed to open audio: {err}"))
                        }
                        Page::Player(_) => {}
                    }
                    self.page = previous_page;
                }
            }
        }
    }
}

fn main() {
    install_panic_hook();

    input::init();
    init_vitagl();

    let mut imgui = imgui::Context::create();
    imgui.set_ini_filename(None);
    input::configure_imgui_io(&mut imgui);

    let mut renderer = VitaGlImguiRenderer::new(&mut imgui);
    let mut app = AppState::new();
    let mut last_frame = Instant::now();
    let mut previous_buttons = 0;

    loop {
        let ctrl = input::read_controller();
        if input::pressed(&ctrl, Button::Select) {
            break;
        }

        let current_buttons = ctrl.buttons();
        if input::just_pressed(current_buttons, previous_buttons, Button::Triangle) {
            app.refresh();
        }
        if input::just_pressed(current_buttons, previous_buttons, Button::Square) {
            app.open_library();
        }
        if input::just_pressed(current_buttons, previous_buttons, Button::Circle) {
            app.back();
        }
        if input::just_pressed(current_buttons, previous_buttons, Button::Left) {
            app.seek_relative_seconds(-SEEK_STEP_SECONDS);
        }
        if input::just_pressed(current_buttons, previous_buttons, Button::Right) {
            app.seek_relative_seconds(SEEK_STEP_SECONDS);
        }
        previous_buttons = current_buttons;

        let touch = input::read_front_touch();

        let now = Instant::now();
        let delta = now.saturating_duration_since(last_frame);
        last_frame = now;

        {
            let io = imgui.io_mut();
            io.update_delta_time(delta);
            io.display_size = [SCREEN_W as f32, SCREEN_H as f32];
            input::apply_to_imgui(io, &ctrl, touch);
        }

        let ui = imgui.frame();
        app.draw(ui, input::navigation_pressed(&ctrl));

        clear_screen();
        let draw_data = imgui.render();
        renderer.render(draw_data);
        present();
    }

    drop(renderer);
}

fn install_panic_hook() {
    panic::set_hook(Box::new(|info| {
        eprintln!("mpvrs panic: {info}");
        eprintln!("{}", Backtrace::force_capture());
        thread::sleep(Duration::from_secs(5));
    }));
}
