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
use library::metadata::read_track_metadata;
use library::{LibraryDb, TrackDisplayRow};
use plumbing::audio::{AudioPlayer, PlaybackMetadata};
use plumbing::input::{self, Button};
use plumbing::rendering::{
    clear_screen, init_vitagl, present, VitaGlImguiRenderer, SCREEN_H, SCREEN_W,
};
use ui::components::cover_art::CoverArtCache;
use ui::file_tree::{FileTreeAction, FileTreeView};
use ui::library_page::{LibraryAction, LibraryView};
use ui::player_page::PlayerView;
use ui::NavAction;

const SEEK_STEP_SECONDS: f32 = 10.0;

struct AppState {
    page: Page,
    history: Vec<Page>,
    player: Option<AudioPlayer>,
    cover_cache: CoverArtCache,
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
            cover_cache: CoverArtCache::new(),
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
                if !page.back() {
                    self.pop_history_or_stay();
                }
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

    fn can_go_back(&self) -> bool {
        match &self.page {
            Page::FileTree(page) => page.can_go_back(),
            Page::Library(page) => page.can_go_back() || !self.history.is_empty(),
            Page::Player(_) => !self.history.is_empty(),
        }
    }

    fn open_player(&mut self) {
        if self.player.is_none() || matches!(self.page, Page::Player(_)) {
            return;
        }

        let previous_page = std::mem::replace(&mut self.page, Page::Player(PlayerView::new()));
        self.history.push(previous_page);
    }

    fn open_library_artist(&mut self, artist: String) {
        self.open_library_page(|page| page.open_artist_by_name(&artist));
    }

    fn open_library_album(&mut self, album: String) {
        self.open_library_page(|page| page.open_album_by_title(&album));
    }

    fn open_library_page(&mut self, open_page: impl FnOnce(&mut LibraryView)) {
        let previous_page = std::mem::replace(
            &mut self.page,
            Page::Library(LibraryView::new(FileTreeView::new(), false)),
        );

        match previous_page {
            Page::Library(mut library) => {
                open_page(&mut library);
                self.page = Page::Library(library);
            }
            Page::FileTree(browser) => {
                let mut library = LibraryView::new(browser, true);
                open_page(&mut library);
                self.page = Page::Library(library);
            }
            Page::Player(player_view) => {
                let mut library = LibraryView::new(FileTreeView::new(), false);
                open_page(&mut library);
                self.page = Page::Library(library);
                self.history.push(Page::Player(player_view));
            }
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
        self.cover_cache.set_playback_active(
            playback
                .as_ref()
                .is_some_and(|snapshot| snapshot.is_playing),
        );
        let window_title = playback
            .as_ref()
            .map(|playback| playback.name.as_str())
            .unwrap_or("mpvrs");
        let show_back = self.can_go_back();
        let show_player = self.player.is_some() && !matches!(self.page, Page::Player(_));
        let mut browse_files = false;
        let (action, nav_action) = match &mut self.page {
            Page::FileTree(page) => page.draw(
                ui,
                playback.as_ref(),
                controller_navigation_active,
                window_title,
                show_back,
                show_player,
            ),
            Page::Library(page) => {
                let (library_action, nav_action) = page.draw(
                    ui,
                    window_title,
                    show_back,
                    show_player,
                    &mut self.cover_cache,
                );
                let action = match library_action {
                    Some(LibraryAction::BrowseFiles) => {
                        browse_files = true;
                        None
                    }
                    Some(LibraryAction::OpenAudio { path, metadata }) => {
                        Some(FileTreeAction::OpenAudio {
                            path,
                            metadata: Some(metadata),
                        })
                    }
                    None => None,
                };
                (action, nav_action)
            }
            Page::Player(page) => {
                let nav_action = page.draw(
                    ui,
                    self.player.as_ref(),
                    window_title,
                    show_back,
                    &mut self.cover_cache,
                );
                (None, nav_action)
            }
        };

        if browse_files {
            self.open_file_browser();
            return;
        }

        if let Some(nav_action) = nav_action {
            match nav_action {
                NavAction::Back => self.back(),
                NavAction::OpenPlayer => self.open_player(),
                NavAction::OpenArtist(artist) => self.open_library_artist(artist),
                NavAction::OpenAlbum(album) => self.open_library_album(album),
            }
            return;
        }

        if let Some(FileTreeAction::OpenAudio { path, metadata }) = action {
            let mut previous_page =
                std::mem::replace(&mut self.page, Page::Player(PlayerView::new()));

            self.player = None;
            let metadata = metadata.unwrap_or_else(|| playback_metadata_for_path(&path));

            match AudioPlayer::open_with_metadata(path, metadata) {
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

fn playback_metadata_for_path(path: &str) -> PlaybackMetadata {
    if let Ok(db) = LibraryDb::open_default() {
        if let Ok(Some(row)) = db.track_display_by_path(path) {
            return playback_metadata_from_db_row(row);
        }
    }

    if let Ok(metadata) = read_track_metadata(path) {
        let title = metadata.title.unwrap_or(metadata.filename);
        return PlaybackMetadata::new(
            title,
            Some(metadata.track_artist),
            metadata.album,
            metadata.artwork.map(|artwork| artwork.cache_path),
        );
    }

    PlaybackMetadata::from_path(path)
}

fn playback_metadata_from_db_row(row: TrackDisplayRow) -> PlaybackMetadata {
    PlaybackMetadata::new(row.title, row.track_artist, row.album, row.art_path)
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
