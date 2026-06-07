use std::thread::{self, JoinHandle};

use imgui::{Condition, Ui};

use crate::library::db::normalize_root;
use crate::library::{AlbumRow, ArtistRow, LibraryDb, RootRow, ScanProgress, Scanner, TrackRow};
use crate::plumbing::rendering::{SCREEN_H, SCREEN_W};
use crate::ui::components::scrollable_list::ScrollableList;
use crate::ui::file_tree::FileTreeView;
use crate::ui::{draw_bottom_nav, NavAction};

const ROW_HEIGHT: f32 = 46.0;

pub struct LibraryView {
    browser: FileTreeView,
    db: Option<LibraryDb>,
    page: LibraryPage,
    status: String,
    current_folder: String,
    show_current_folder_action: bool,
    list: ScrollableList,
    scan: Option<JoinHandle<ScanProgress>>,
    last_scan: Option<ScanProgress>,
}

#[derive(Clone, Debug)]
enum LibraryPage {
    Home,
    Roots(Vec<RootRow>),
    Artists(Vec<ArtistRow>),
    Albums(Vec<AlbumRow>),
    Tracks(Vec<TrackRow>),
    ArtistTracks {
        artist: String,
        tracks: Vec<TrackRow>,
    },
    AlbumTracks {
        album: String,
        tracks: Vec<TrackRow>,
    },
}

#[derive(Clone, Debug)]
pub enum LibraryAction {
    BrowseFiles,
    OpenAudio(String),
}

impl LibraryView {
    pub fn new(browser: FileTreeView, show_current_folder_action: bool) -> Self {
        let current_folder = browser.current_dir().to_owned();
        let mut view = Self {
            browser,
            db: None,
            page: LibraryPage::Home,
            status: String::new(),
            current_folder,
            show_current_folder_action,
            list: ScrollableList::new(),
            scan: None,
            last_scan: None,
        };
        view.open_db();
        view
    }

    pub fn into_browser(self) -> FileTreeView {
        self.browser
    }

    pub fn set_status(&mut self, status: String) {
        self.status = status;
    }

    pub fn refresh(&mut self) {
        self.poll_scan();
        match self.page {
            LibraryPage::Home => {}
            LibraryPage::Roots(_) => self.load_roots(),
            LibraryPage::Artists(_) => self.load_artists(),
            LibraryPage::Albums(_) => self.load_albums(),
            LibraryPage::Tracks(_) => self.load_tracks(),
            LibraryPage::ArtistTracks { .. } | LibraryPage::AlbumTracks { .. } => {}
        }
    }

    pub fn draw(
        &mut self,
        ui: &Ui,
        window_title: &str,
        show_back: bool,
        show_player: bool,
    ) -> (Option<LibraryAction>, Option<NavAction>) {
        self.poll_scan();
        let mut action = None;
        let mut nav_action = None;

        ui.window(&format!("{window_title}###Library"))
            .position([0.0, 0.0], Condition::Always)
            .size([SCREEN_W as f32, SCREEN_H as f32], Condition::Always)
            .movable(false)
            .resizable(false)
            .collapsible(false)
            .build(|| {
                ui.text(self.title());
                ui.separator();

                let list_height = SCREEN_H as f32 - 176.0;
                let mut list = std::mem::take(&mut self.list);
                list.draw(ui, "library-list", [0.0, list_height], true, |ui, touch| {
                    action = self.draw_page_rows(ui, touch.disable_hover || touch.suppress_click);
                });
                self.list = list;

                ui.separator();
                if let Some(scan) = &self.last_scan {
                    ui.text(format!(
                        "Last scan: {} files, {} indexed, {} unchanged, {} errors",
                        scan.files_seen, scan.tracks_indexed, scan.tracks_skipped, scan.errors
                    ));
                }
                if !self.status.is_empty() {
                    ui.text(&self.status);
                }

                nav_action = draw_bottom_nav(ui, show_back, show_player);
            });

        (action, nav_action)
    }

    pub fn can_go_back(&self) -> bool {
        !matches!(self.page, LibraryPage::Home)
    }

    pub fn back(&mut self) -> bool {
        match self.page {
            LibraryPage::Home => false,
            _ => {
                self.page = LibraryPage::Home;
                true
            }
        }
    }

    pub fn home(&mut self) {
        self.page = LibraryPage::Home;
    }

    fn draw_page_rows(&mut self, ui: &Ui, disable_hover: bool) -> Option<LibraryAction> {
        match self.page.clone() {
            LibraryPage::Home => self.draw_home(ui, disable_hover),
            LibraryPage::Roots(roots) => self.draw_roots(ui, disable_hover, &roots),
            LibraryPage::Artists(artists) => self.draw_artists(ui, disable_hover, &artists),
            LibraryPage::Albums(albums) => self.draw_albums(ui, disable_hover, &albums),
            LibraryPage::Tracks(tracks)
            | LibraryPage::ArtistTracks { tracks, .. }
            | LibraryPage::AlbumTracks { tracks, .. } => {
                self.draw_tracks(ui, disable_hover, &tracks)
            }
        }
    }

    fn draw_home(&mut self, ui: &Ui, disable_hover: bool) -> Option<LibraryAction> {
        if row(ui, "Artists", disable_hover) {
            self.load_artists();
        }
        if row(ui, "Albums", disable_hover) {
            self.load_albums();
        }
        if row(ui, "Tracks", disable_hover) {
            self.load_tracks();
        }
        if row(ui, "Roots", disable_hover) {
            self.load_roots();
        }
        if row(ui, "Browse files", disable_hover) {
            return Some(LibraryAction::BrowseFiles);
        }
        if self.show_current_folder_action {
            ui.separator();
            if row(ui, &self.current_folder_action_label(), disable_hover) {
                self.start_scan(self.current_folder.clone());
            }
        }
        None
    }

    fn draw_roots(
        &mut self,
        ui: &Ui,
        disable_hover: bool,
        roots: &[RootRow],
    ) -> Option<LibraryAction> {
        for root in roots {
            let scanned = root
                .last_scanned_at
                .map(|ts| format!("last scan {ts}"))
                .unwrap_or_else(|| "never scanned".to_owned());
            let enabled = if root.enabled { "enabled" } else { "disabled" };
            if row(
                ui,
                &format!("{} ({enabled}, {scanned})##root-{}", root.path, root.id),
                disable_hover,
            ) {
                self.start_scan(root.path.clone());
            }
        }
        None
    }

    fn draw_artists(
        &mut self,
        ui: &Ui,
        disable_hover: bool,
        artists: &[ArtistRow],
    ) -> Option<LibraryAction> {
        for artist in artists {
            if row(
                ui,
                &format!(
                    "{} - {} tracks##artist-{}",
                    artist.name, artist.track_count, artist.id
                ),
                disable_hover,
            ) {
                self.load_artist_tracks(artist.id, artist.name.clone());
            }
        }
        None
    }

    fn draw_albums(
        &mut self,
        ui: &Ui,
        disable_hover: bool,
        albums: &[AlbumRow],
    ) -> Option<LibraryAction> {
        for album in albums {
            let year = album
                .year
                .map(|year| format!(" ({year})"))
                .unwrap_or_default();
            let art = album.art_path.as_deref().unwrap_or("no art");
            if row(
                ui,
                &format!(
                    "{} - {}{} - {} tracks - {}##album-{}",
                    album.album_artist, album.title, year, album.track_count, art, album.id
                ),
                disable_hover,
            ) {
                self.load_album_tracks(album.id, album.title.clone());
            }
        }
        None
    }

    fn draw_tracks(
        &mut self,
        ui: &Ui,
        disable_hover: bool,
        tracks: &[TrackRow],
    ) -> Option<LibraryAction> {
        for track in tracks {
            let number = track
                .track_number
                .map(|number| format!("{number:02}. "))
                .unwrap_or_default();
            let duration = track
                .duration_ms
                .map(format_duration)
                .unwrap_or_else(|| "--:--".to_owned());
            let disc = track
                .disc_number
                .map(|disc| format!("d{disc} "))
                .unwrap_or_default();
            let art = track.art_path.as_deref().unwrap_or("no art");
            if row(
                ui,
                &format!(
                    "{}{}{} - {} - {} / {} [{}] - {}##track-{}",
                    disc,
                    number,
                    track.title,
                    track.track_artist,
                    track.album_artist,
                    track.album,
                    duration,
                    art,
                    track.id
                ),
                disable_hover,
            ) {
                return Some(LibraryAction::OpenAudio(track.path.clone()));
            }
        }
        None
    }

    fn open_db(&mut self) {
        match LibraryDb::open_default() {
            Ok(db) => {
                self.db = Some(db);
                self.status.clear();
            }
            Err(err) => {
                self.db = None;
                self.status = format!("Library DB unavailable: {err}");
            }
        }
    }

    fn current_folder_action_label(&mut self) -> String {
        let prefix = if self.current_folder_is_enabled_root() {
            "Reindex current folder"
        } else {
            "Add and index current folder"
        };
        format!("{prefix}: {}", self.current_folder)
    }

    fn current_folder_is_enabled_root(&mut self) -> bool {
        self.ensure_db();
        let Some(db) = &self.db else {
            return false;
        };
        let current_folder = normalize_root(&self.current_folder);
        match db.roots() {
            Ok(roots) => roots
                .iter()
                .any(|root| root.enabled && root.path == current_folder),
            Err(err) => {
                self.status = format!("Failed to load roots: {err}");
                false
            }
        }
    }

    fn start_scan(&mut self, root: String) {
        if self.scan.is_some() {
            self.status = "Scan already running".to_owned();
            return;
        }
        self.status = format!("Started scanning {root}");
        self.scan = Some(thread::spawn(move || match Scanner::new() {
            Ok(scanner) => scanner.scan_root(root),
            Err(err) => ScanProgress {
                done: true,
                errors: 1,
                status: format!("Failed to start scanner: {err}"),
                ..ScanProgress::default()
            },
        }));
    }

    fn poll_scan(&mut self) {
        let Some(handle) = self.scan.take() else {
            return;
        };
        if handle.is_finished() {
            match handle.join() {
                Ok(progress) => {
                    self.status = progress.status.clone();
                    self.last_scan = Some(progress);
                    self.open_db();
                    self.refresh_after_scan();
                }
                Err(_) => self.status = "Scanner panicked".to_owned(),
            }
        } else {
            self.status = "Scan running in background...".to_owned();
            self.scan = Some(handle);
        }
    }

    fn refresh_after_scan(&mut self) {
        match self.page {
            LibraryPage::Roots(_) => self.load_roots(),
            LibraryPage::Artists(_) => self.load_artists(),
            LibraryPage::Albums(_) => self.load_albums(),
            LibraryPage::Tracks(_) => self.load_tracks(),
            _ => {}
        }
    }

    fn ensure_db(&mut self) {
        if self.db.is_none() {
            self.open_db();
        }
    }

    fn load_roots(&mut self) {
        self.ensure_db();
        let Some(db) = &self.db else {
            return;
        };
        match db.roots() {
            Ok(roots) => self.page = LibraryPage::Roots(roots),
            Err(err) => self.status = format!("Failed to load roots: {err}"),
        }
    }

    fn load_artists(&mut self) {
        self.ensure_db();
        let Some(db) = &self.db else {
            return;
        };
        match db.artists() {
            Ok(artists) => self.page = LibraryPage::Artists(artists),
            Err(err) => self.status = format!("Failed to load artists: {err}"),
        }
    }

    fn load_albums(&mut self) {
        self.ensure_db();
        let Some(db) = &self.db else {
            return;
        };
        match db.albums() {
            Ok(albums) => self.page = LibraryPage::Albums(albums),
            Err(err) => self.status = format!("Failed to load albums: {err}"),
        }
    }

    fn load_tracks(&mut self) {
        self.ensure_db();
        let Some(db) = &self.db else {
            return;
        };
        match db.tracks() {
            Ok(tracks) => self.page = LibraryPage::Tracks(tracks),
            Err(err) => self.status = format!("Failed to load tracks: {err}"),
        }
    }

    fn load_artist_tracks(&mut self, artist_id: i64, artist: String) {
        self.ensure_db();
        let Some(db) = &self.db else {
            return;
        };
        match db.tracks_for_artist(artist_id) {
            Ok(tracks) => self.page = LibraryPage::ArtistTracks { artist, tracks },
            Err(err) => self.status = format!("Failed to load artist tracks: {err}"),
        }
    }

    fn load_album_tracks(&mut self, album_id: i64, album: String) {
        self.ensure_db();
        let Some(db) = &self.db else {
            return;
        };
        match db.tracks_for_album(album_id) {
            Ok(tracks) => self.page = LibraryPage::AlbumTracks { album, tracks },
            Err(err) => self.status = format!("Failed to load album tracks: {err}"),
        }
    }

    fn title(&self) -> String {
        match &self.page {
            LibraryPage::Home => "Library".to_owned(),
            LibraryPage::Roots(roots) => format!("Library roots - {}", roots.len()),
            LibraryPage::Artists(artists) => format!("Artists - {}", artists.len()),
            LibraryPage::Albums(albums) => format!("Albums - {}", albums.len()),
            LibraryPage::Tracks(tracks) => format!("Tracks - {}", tracks.len()),
            LibraryPage::ArtistTracks { artist, tracks } => {
                format!("{artist} - {} tracks", tracks.len())
            }
            LibraryPage::AlbumTracks { album, tracks } => {
                format!("{album} - {} tracks", tracks.len())
            }
        }
    }
}

fn row(ui: &Ui, label: &str, disable_hover: bool) -> bool {
    ui.selectable_config(label)
        .disabled(disable_hover)
        .size([0.0, ROW_HEIGHT])
        .build()
}

fn format_duration(ms: i64) -> String {
    let seconds = (ms / 1000).max(0);
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
