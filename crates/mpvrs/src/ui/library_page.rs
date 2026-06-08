use std::sync::{
    mpsc::{self, Receiver, Sender},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};

use imgui::{Condition, Ui};

use crate::library::db::normalize_root;
use crate::library::metadata::split_artist_value;
use crate::library::{AlbumRow, ArtistRow, LibraryDb, RootRow, ScanProgress, Scanner, TrackRow};
use crate::plumbing::audio::PlaybackMetadata;
use crate::plumbing::ime;
use crate::plumbing::rendering::{SCREEN_H, SCREEN_W};
use crate::queue::{QueueItem, QueueSource};
use crate::ui::components::cover_art::{draw_thumbnail_cover_art_at, CoverArtCache};
use crate::ui::components::scrollable_list::ScrollableList;
use crate::ui::file_tree::FileTreeView;
use crate::ui::{draw_bottom_nav, NavAction};

const ROW_HEIGHT: f32 = 46.0;
const TRACK_ROW_HEIGHT: f32 = 60.0;
const TRACK_ROW_SPACING: f32 = 6.0;
const TRACK_ART_SIZE: f32 = 48.0;
const ALBUM_ROW_HEIGHT: f32 = 64.0;
const ALBUM_ROW_SPACING: f32 = 6.0;
const ALBUM_ART_SIZE: f32 = 54.0;

pub struct LibraryView {
    browser: FileTreeView,
    db: Option<LibraryDb>,
    page: LibraryPage,
    status: String,
    current_folder: String,
    show_current_folder_action: bool,
    list: ScrollableList,
    scan: Option<JoinHandle<ScanProgress>>,
    scan_progress: Option<Arc<Mutex<ScanProgress>>>,
    last_scan: Option<ScanProgress>,
    search_query: String,
    load_tx: Sender<LibraryLoadRequest>,
    load_rx: Receiver<LibraryLoadResponse>,
    load_generation: u64,
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
    OpenAudio {
        path: String,
        metadata: PlaybackMetadata,
        source: QueueSource,
        queue: Vec<QueueItem>,
    },
}

#[derive(Clone, Copy, Debug)]
enum LibraryLoadKind {
    Artists,
    Albums,
    Tracks,
}

struct LibraryLoadRequest {
    kind: LibraryLoadKind,
    generation: u64,
    query: String,
}

struct LibraryLoadResponse {
    generation: u64,
    query: String,
    result: Result<LibraryPage, String>,
}

enum PendingLibraryAction {
    ScanRoot(String),
    LoadArtistTracks {
        id: i64,
        name: String,
    },
    LoadAlbumTracks {
        id: i64,
        title: String,
    },
    OpenAudio {
        path: String,
        metadata: PlaybackMetadata,
        source: QueueSource,
        queue: Vec<QueueItem>,
    },
}

impl LibraryView {
    pub fn new(browser: FileTreeView, show_current_folder_action: bool) -> Self {
        let current_folder = browser.current_dir().to_owned();
        let (load_tx, load_rx) = start_library_load_worker();
        let mut view = Self {
            browser,
            db: None,
            page: LibraryPage::Home,
            status: String::new(),
            current_folder,
            show_current_folder_action,
            list: ScrollableList::new(),
            scan: None,
            scan_progress: None,
            last_scan: None,
            search_query: String::new(),
            load_tx,
            load_rx,
            load_generation: 0,
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
        self.poll_load();
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
        cover_cache: &mut CoverArtCache,
    ) -> (Option<LibraryAction>, Option<NavAction>) {
        self.poll_scan();
        self.poll_load();
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
                if self.search_controls_enabled() {
                    self.draw_search_controls(ui);
                }
                ui.separator();

                let search_height = if self.search_controls_enabled() {
                    36.0
                } else {
                    0.0
                };
                let list_height = SCREEN_H as f32 - 176.0 - search_height;
                let mut list = std::mem::take(&mut self.list);
                list.draw(ui, "library-list", [0.0, list_height], true, |ui, touch| {
                    action = self.draw_page_rows(
                        ui,
                        touch.disable_hover || touch.suppress_click,
                        cover_cache,
                    );
                });
                self.list = list;

                ui.separator();
                if let Some(scan) = &self.last_scan {
                    ui.text(format!(
                        "Last scan: {} files, {} indexed, {} unchanged, {} duplicates, {} errors",
                        scan.files_seen,
                        scan.tracks_indexed,
                        scan.tracks_skipped,
                        scan.tracks_duplicated,
                        scan.errors
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

    pub fn open_artist_by_name(&mut self, artist_name: &str) {
        self.ensure_db();
        let Some(db) = &self.db else {
            return;
        };
        let artists = match db.artists() {
            Ok(artists) => artists,
            Err(err) => {
                self.status = format!("Failed to load artists: {err}");
                return;
            }
        };

        if let Some(artist) = artists
            .iter()
            .find(|artist| artist.name.eq_ignore_ascii_case(artist_name))
        {
            self.load_artist_tracks(artist.id, artist.name.clone());
            return;
        }

        let candidates = split_artist_value(artist_name);
        let matches: Vec<_> = candidates
            .iter()
            .filter_map(|candidate| {
                artists
                    .iter()
                    .find(|artist| artist.name.eq_ignore_ascii_case(candidate))
            })
            .collect();

        match matches.as_slice() {
            [artist] => self.load_artist_tracks(artist.id, artist.name.clone()),
            [] => self.status = format!("Artist not found in library: {artist_name}"),
            _ => {
                self.status = format!(
                    "Multiple artists found: {}",
                    matches
                        .iter()
                        .map(|artist| artist.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
    }

    pub fn open_album_by_title(&mut self, album_title: &str) {
        self.ensure_db();
        let Some(db) = &self.db else {
            return;
        };
        let album = match db.albums() {
            Ok(albums) => albums
                .into_iter()
                .find(|album| album.title.eq_ignore_ascii_case(album_title)),
            Err(err) => {
                self.status = format!("Failed to load albums: {err}");
                return;
            }
        };

        if let Some(album) = album {
            self.load_album_tracks(album.id, album.title);
        } else {
            self.status = format!("Album not found in library: {album_title}");
        }
    }

    fn draw_page_rows(
        &mut self,
        ui: &Ui,
        disable_hover: bool,
        cover_cache: &mut CoverArtCache,
    ) -> Option<LibraryAction> {
        if matches!(self.page, LibraryPage::Home) {
            return self.draw_home(ui, disable_hover);
        }

        let pending = match &self.page {
            LibraryPage::Home => unreachable!(),
            LibraryPage::Roots(roots) => draw_roots(ui, disable_hover, roots),
            LibraryPage::Artists(artists) => draw_artists(ui, disable_hover, artists, cover_cache),
            LibraryPage::Albums(albums) => draw_albums(ui, disable_hover, albums, cover_cache),
            LibraryPage::Tracks(tracks) => draw_tracks(
                ui,
                disable_hover,
                QueueSource::AllTracks,
                tracks,
                cover_cache,
            ),
            LibraryPage::ArtistTracks { artist, tracks } => draw_tracks(
                ui,
                disable_hover,
                QueueSource::Artist(artist.clone()),
                tracks,
                cover_cache,
            ),
            LibraryPage::AlbumTracks { album, tracks } => draw_tracks(
                ui,
                disable_hover,
                QueueSource::Album(album.clone()),
                tracks,
                cover_cache,
            ),
        };

        match pending {
            Some(PendingLibraryAction::ScanRoot(root)) => self.start_scan(root),
            Some(PendingLibraryAction::LoadArtistTracks { id, name }) => {
                self.load_artist_tracks(id, name)
            }
            Some(PendingLibraryAction::LoadAlbumTracks { id, title }) => {
                self.load_album_tracks(id, title)
            }
            Some(PendingLibraryAction::OpenAudio {
                path,
                metadata,
                source,
                queue,
            }) => {
                return Some(LibraryAction::OpenAudio {
                    path,
                    metadata,
                    source,
                    queue,
                });
            }
            None => {}
        }

        None
    }

    fn draw_home(&mut self, ui: &Ui, disable_hover: bool) -> Option<LibraryAction> {
        if row(ui, "Artists", disable_hover) {
            self.clear_search();
            self.load_artists();
        }
        if row(ui, "Albums", disable_hover) {
            self.clear_search();
            self.load_albums();
        }
        if row(ui, "Tracks", disable_hover) {
            self.clear_search();
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

    fn search_controls_enabled(&self) -> bool {
        matches!(
            self.page,
            LibraryPage::Artists(_) | LibraryPage::Albums(_) | LibraryPage::Tracks(_)
        )
    }

    fn draw_search_controls(&mut self, ui: &Ui) {
        if ui.button("Search") {
            match ime::search_text("Search library", &self.search_query) {
                Ok(Some(query)) => self.set_search_query(query),
                Ok(None) => {}
                Err(err) => self.status = err,
            }
        }

        if !self.search_query.is_empty() {
            ui.same_line();
            if ui.button("Clear") {
                self.clear_search();
                self.refresh_search_page();
            }
            ui.same_line();
            ui.text(format!("Filter: {}", self.search_query));
        }
    }

    fn set_search_query(&mut self, query: String) {
        self.search_query = query.trim().to_owned();
        self.list = ScrollableList::new();
        self.refresh_search_page();
    }

    fn clear_search(&mut self) {
        self.search_query.clear();
        self.list = ScrollableList::new();
    }

    fn refresh_search_page(&mut self) {
        match self.page {
            LibraryPage::Artists(_) => self.load_artists(),
            LibraryPage::Albums(_) => self.load_albums(),
            LibraryPage::Tracks(_) => self.load_tracks(),
            _ => {}
        }
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
        let scan_progress = Arc::new(Mutex::new(ScanProgress::default()));
        self.scan_progress = Some(scan_progress.clone());
        self.scan = Some(thread::spawn(move || match Scanner::new() {
            Ok(scanner) => scanner.scan_root_with_progress(root, scan_progress),
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
                    self.scan_progress = None;
                    self.open_db();
                    self.refresh_after_scan();
                }
                Err(_) => {
                    self.status = "Scanner panicked".to_owned();
                    self.scan_progress = None;
                }
            }
        } else {
            if let Some(progress) = self
                .scan_progress
                .as_ref()
                .and_then(|progress| progress.lock().ok().map(|progress| progress.clone()))
            {
                if !progress.status.is_empty() {
                    self.status = progress.status;
                }
            }
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
        self.start_load(LibraryLoadKind::Artists);
    }

    fn load_albums(&mut self) {
        self.start_load(LibraryLoadKind::Albums);
    }

    fn load_tracks(&mut self) {
        self.start_load(LibraryLoadKind::Tracks);
    }

    fn start_load(&mut self, kind: LibraryLoadKind) {
        self.load_generation = self.load_generation.wrapping_add(1);
        let generation = self.load_generation;
        let query = self.search_query.clone();

        self.page = empty_page_for_load(kind);
        self.list = ScrollableList::new();
        self.status = format!("Loading {}...", load_kind_label(kind));

        if self
            .load_tx
            .send(LibraryLoadRequest {
                kind,
                generation,
                query,
            })
            .is_err()
        {
            let message = format!("{} load worker stopped", load_kind_title(kind));
            eprintln!("mpvrs: {message}");
            self.status = message;
        }
    }

    fn poll_load(&mut self) {
        while let Ok(response) = self.load_rx.try_recv() {
            if response.generation != self.load_generation || response.query != self.search_query {
                continue;
            }

            match response.result {
                Ok(page) => {
                    self.page = page;
                    self.list = ScrollableList::new();
                    self.status.clear();
                }
                Err(err) => {
                    eprintln!("mpvrs: async library load failed: {err}");
                    self.status = err;
                }
            }
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

fn start_library_load_worker() -> (Sender<LibraryLoadRequest>, Receiver<LibraryLoadResponse>) {
    let (request_tx, request_rx) = mpsc::channel::<LibraryLoadRequest>();
    let (response_tx, response_rx) = mpsc::channel::<LibraryLoadResponse>();

    thread::spawn(move || {
        let db = LibraryDb::open_default();
        if let Err(err) = &db {
            eprintln!("mpvrs: failed to open async library DB: {err}");
        }

        for request in request_rx {
            let result = match &db {
                Ok(db) => query_library_page(db, request.kind, &request.query),
                Err(err) => Err(format!("Library DB unavailable: {err}")),
            };

            if response_tx
                .send(LibraryLoadResponse {
                    generation: request.generation,
                    query: request.query,
                    result,
                })
                .is_err()
            {
                break;
            }
        }
    });

    (request_tx, response_rx)
}

fn query_library_page(
    db: &LibraryDb,
    kind: LibraryLoadKind,
    query: &str,
) -> Result<LibraryPage, String> {
    match kind {
        LibraryLoadKind::Artists => db
            .artists()
            .map(|artists| LibraryPage::Artists(filter_artists(artists, query)))
            .map_err(|err| format!("Failed to load artists: {err}")),
        LibraryLoadKind::Albums => db
            .albums()
            .map(|albums| LibraryPage::Albums(filter_albums(albums, query)))
            .map_err(|err| format!("Failed to load albums: {err}")),
        LibraryLoadKind::Tracks => db
            .tracks()
            .map(|tracks| LibraryPage::Tracks(filter_tracks(tracks, query)))
            .map_err(|err| format!("Failed to load tracks: {err}")),
    }
}

fn empty_page_for_load(kind: LibraryLoadKind) -> LibraryPage {
    match kind {
        LibraryLoadKind::Artists => LibraryPage::Artists(Vec::new()),
        LibraryLoadKind::Albums => LibraryPage::Albums(Vec::new()),
        LibraryLoadKind::Tracks => LibraryPage::Tracks(Vec::new()),
    }
}

fn load_kind_label(kind: LibraryLoadKind) -> &'static str {
    match kind {
        LibraryLoadKind::Artists => "artists",
        LibraryLoadKind::Albums => "albums",
        LibraryLoadKind::Tracks => "tracks",
    }
}

fn load_kind_title(kind: LibraryLoadKind) -> &'static str {
    match kind {
        LibraryLoadKind::Artists => "Artists",
        LibraryLoadKind::Albums => "Albums",
        LibraryLoadKind::Tracks => "Tracks",
    }
}

fn filter_artists(artists: Vec<ArtistRow>, query: &str) -> Vec<ArtistRow> {
    let Some(query) = normalized_query(query) else {
        return artists;
    };
    artists
        .into_iter()
        .filter(|artist| contains_case_insensitive(&artist.name, &query))
        .collect()
}

fn filter_albums(albums: Vec<AlbumRow>, query: &str) -> Vec<AlbumRow> {
    let Some(query) = normalized_query(query) else {
        return albums;
    };
    albums
        .into_iter()
        .filter(|album| {
            contains_case_insensitive(&album.title, &query)
                || contains_case_insensitive(&album.album_artist, &query)
        })
        .collect()
}

fn filter_tracks(tracks: Vec<TrackRow>, query: &str) -> Vec<TrackRow> {
    let Some(query) = normalized_query(query) else {
        return tracks;
    };
    tracks
        .into_iter()
        .filter(|track| {
            contains_case_insensitive(&track.title, &query)
                || contains_case_insensitive(&track.track_artist, &query)
                || contains_case_insensitive(&track.album, &query)
                || contains_case_insensitive(&track.album_artist, &query)
                || contains_case_insensitive(&track.path, &query)
        })
        .collect()
}

fn normalized_query(query: &str) -> Option<String> {
    let query = query.trim();
    if query.is_empty() {
        None
    } else {
        Some(query.to_ascii_lowercase())
    }
}

fn contains_case_insensitive(value: &str, normalized_query: &str) -> bool {
    value.to_ascii_lowercase().contains(normalized_query)
}

fn draw_roots(ui: &Ui, disable_hover: bool, roots: &[RootRow]) -> Option<PendingLibraryAction> {
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
            return Some(PendingLibraryAction::ScanRoot(root.path.clone()));
        }
    }
    None
}

fn draw_artists(
    ui: &Ui,
    disable_hover: bool,
    artists: &[ArtistRow],
    cover_cache: &mut CoverArtCache,
) -> Option<PendingLibraryAction> {
    let (start, end, top_skip, bottom_skip) =
        visible_row_range(ui, artists.len(), ALBUM_ROW_HEIGHT, ALBUM_ROW_SPACING);

    if top_skip > 0.0 {
        ui.dummy([0.0, top_skip]);
    }

    for artist in &artists[start..end] {
        if artist_row(ui, artist, disable_hover, cover_cache) {
            return Some(PendingLibraryAction::LoadArtistTracks {
                id: artist.id,
                name: artist.name.clone(),
            });
        }
    }

    if bottom_skip > 0.0 {
        ui.dummy([0.0, bottom_skip]);
    }

    None
}

fn draw_albums(
    ui: &Ui,
    disable_hover: bool,
    albums: &[AlbumRow],
    cover_cache: &mut CoverArtCache,
) -> Option<PendingLibraryAction> {
    let (start, end, top_skip, bottom_skip) =
        visible_row_range(ui, albums.len(), ALBUM_ROW_HEIGHT, ALBUM_ROW_SPACING);

    if top_skip > 0.0 {
        ui.dummy([0.0, top_skip]);
    }

    for album in &albums[start..end] {
        if album_row(ui, album, disable_hover, cover_cache) {
            return Some(PendingLibraryAction::LoadAlbumTracks {
                id: album.id,
                title: album.title.clone(),
            });
        }
    }

    if bottom_skip > 0.0 {
        ui.dummy([0.0, bottom_skip]);
    }

    None
}

fn draw_tracks(
    ui: &Ui,
    disable_hover: bool,
    source: QueueSource,
    tracks: &[TrackRow],
    cover_cache: &mut CoverArtCache,
) -> Option<PendingLibraryAction> {
    let (start, end, top_skip, bottom_skip) =
        visible_row_range(ui, tracks.len(), TRACK_ROW_HEIGHT, TRACK_ROW_SPACING);

    if top_skip > 0.0 {
        ui.dummy([0.0, top_skip]);
    }

    for track in &tracks[start..end] {
        if track_row(ui, track, disable_hover, cover_cache) {
            return Some(PendingLibraryAction::OpenAudio {
                path: track.path.clone(),
                metadata: playback_metadata_from_track(track),
                source: source.clone(),
                queue: tracks.iter().map(queue_item_from_track).collect(),
            });
        }
    }

    if bottom_skip > 0.0 {
        ui.dummy([0.0, bottom_skip]);
    }

    None
}

fn queue_item_from_track(track: &TrackRow) -> QueueItem {
    QueueItem {
        path: track.path.clone(),
        metadata: playback_metadata_from_track(track),
    }
}

fn playback_metadata_from_track(track: &TrackRow) -> PlaybackMetadata {
    PlaybackMetadata::new(
        track.title.clone(),
        Some(track.track_artist.clone()),
        Some(track.album.clone()),
        track.art_path.clone(),
    )
}

fn visible_row_range(
    ui: &Ui,
    len: usize,
    row_height: f32,
    row_spacing: f32,
) -> (usize, usize, f32, f32) {
    if len == 0 {
        return (0, 0, 0.0, 0.0);
    }

    let stride = row_height + row_spacing;
    let scroll_y = ui.scroll_y().max(0.0);
    let viewport_h = ui.content_region_avail()[1].max(1.0);
    let overscan_rows = 2_usize;

    let first_visible = (scroll_y / stride).floor().max(0.0) as usize;
    let start = first_visible.saturating_sub(overscan_rows).min(len);
    let visible_rows =
        ((viewport_h / stride).ceil() as usize).saturating_add(overscan_rows * 2 + 2);
    let end = start.saturating_add(visible_rows).min(len);

    let top_skip = start as f32 * stride;
    let bottom_skip = len.saturating_sub(end) as f32 * stride;
    (start, end, top_skip, bottom_skip)
}

fn artist_row(
    ui: &Ui,
    artist: &ArtistRow,
    disable_hover: bool,
    cover_cache: &mut CoverArtCache,
) -> bool {
    let clicked = ui
        .selectable_config(&format!("##artist-{}", artist.id))
        .disabled(disable_hover)
        .size([0.0, ALBUM_ROW_HEIGHT])
        .build();

    let min = ui.item_rect_min();
    let max = ui.item_rect_max();
    let row_height = max[1] - min[1];
    let art_x = min[0] + 6.0;
    let art_y = min[1] + (row_height - ALBUM_ART_SIZE) * 0.5;
    draw_thumbnail_cover_art_at(
        ui,
        cover_cache,
        artist.art_path.as_deref(),
        !disable_hover,
        [art_x, art_y],
        [ALBUM_ART_SIZE, ALBUM_ART_SIZE],
    );

    let detail = format!(
        "{} - {}",
        format_album_count(artist.album_count),
        format_track_count(artist.track_count)
    );
    let text_x = art_x + ALBUM_ART_SIZE + 12.0;
    let title_y = min[1] + 9.0;
    let detail_y = min[1] + 35.0;
    let draw_list = ui.get_window_draw_list();
    draw_list.add_text([text_x, title_y], [0.94, 0.96, 1.0, 1.0], &artist.name);
    draw_list.add_text([text_x, detail_y], [0.66, 0.72, 0.82, 1.0], detail);

    clicked
}

fn album_row(
    ui: &Ui,
    album: &AlbumRow,
    disable_hover: bool,
    cover_cache: &mut CoverArtCache,
) -> bool {
    let clicked = ui
        .selectable_config(&format!("##album-{}", album.id))
        .disabled(disable_hover)
        .size([0.0, ALBUM_ROW_HEIGHT])
        .build();

    let min = ui.item_rect_min();
    let max = ui.item_rect_max();
    let row_height = max[1] - min[1];
    let art_x = min[0] + 6.0;
    let art_y = min[1] + (row_height - ALBUM_ART_SIZE) * 0.5;
    draw_thumbnail_cover_art_at(
        ui,
        cover_cache,
        album.art_path.as_deref(),
        !disable_hover,
        [art_x, art_y],
        [ALBUM_ART_SIZE, ALBUM_ART_SIZE],
    );

    let year = album.year.map(|year| year.to_string());
    let mut detail = album.album_artist.clone();
    if let Some(year) = year {
        detail.push_str(" • ");
        detail.push_str(&year);
    }
    detail.push_str(" • ");
    detail.push_str(&format_track_count(album.track_count));

    let text_x = art_x + ALBUM_ART_SIZE + 12.0;
    let title_y = min[1] + 9.0;
    let detail_y = min[1] + 35.0;
    let draw_list = ui.get_window_draw_list();
    draw_list.add_text([text_x, title_y], [0.94, 0.96, 1.0, 1.0], &album.title);
    draw_list.add_text([text_x, detail_y], [0.66, 0.72, 0.82, 1.0], detail);

    clicked
}

fn track_row(
    ui: &Ui,
    track: &TrackRow,
    disable_hover: bool,
    cover_cache: &mut CoverArtCache,
) -> bool {
    let clicked = ui
        .selectable_config(&format!("##track-{}", track.id))
        .disabled(disable_hover)
        .size([0.0, TRACK_ROW_HEIGHT])
        .build();

    let min = ui.item_rect_min();
    let max = ui.item_rect_max();
    let row_height = max[1] - min[1];
    let art_x = min[0] + 6.0;
    let art_y = min[1] + (row_height - TRACK_ART_SIZE) * 0.5;
    draw_thumbnail_cover_art_at(
        ui,
        cover_cache,
        track.art_path.as_deref(),
        !disable_hover,
        [art_x, art_y],
        [TRACK_ART_SIZE, TRACK_ART_SIZE],
    );

    let duration = track
        .duration_ms
        .map(format_duration)
        .unwrap_or_else(|| "--:--".to_owned());
    let title = track.title.as_str();
    let detail = format!("{} - {} ({duration})", track.track_artist, track.album);
    let text_x = art_x + TRACK_ART_SIZE + 12.0;
    let title_y = min[1] + 8.0;
    let detail_y = min[1] + 32.0;
    let draw_list = ui.get_window_draw_list();
    draw_list.add_text([text_x, title_y], [0.94, 0.96, 1.0, 1.0], title);
    draw_list.add_text([text_x, detail_y], [0.66, 0.72, 0.82, 1.0], detail);

    clicked
}

fn row(ui: &Ui, label: &str, disable_hover: bool) -> bool {
    ui.selectable_config(label)
        .disabled(disable_hover)
        .size([0.0, ROW_HEIGHT])
        .build()
}

fn format_track_count(track_count: i64) -> String {
    if track_count == 1 {
        "1 track".to_owned()
    } else {
        format!("{track_count} tracks")
    }
}

fn format_album_count(album_count: i64) -> String {
    if album_count == 1 {
        "1 album".to_owned()
    } else {
        format!("{album_count} albums")
    }
}

fn format_duration(ms: i64) -> String {
    let seconds = (ms / 1000).max(0);
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
