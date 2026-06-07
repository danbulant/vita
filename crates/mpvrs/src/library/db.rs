use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

use crate::library::metadata::TrackMetadata;
use crate::library::{log, DATA_DIR, DB_PATH};

#[derive(Clone, Debug)]
pub struct RootRow {
    pub id: i64,
    pub path: String,
    pub enabled: bool,
    pub last_scanned_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct ArtistRow {
    pub id: i64,
    pub name: String,
    pub track_count: i64,
}

#[derive(Clone, Debug)]
pub struct AlbumRow {
    pub id: i64,
    pub title: String,
    pub album_artist: String,
    pub year: Option<i64>,
    pub art_path: Option<String>,
    pub track_count: i64,
}

#[derive(Clone, Debug)]
pub struct TrackRow {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub track_artist: String,
    pub album: String,
    pub album_artist: String,
    pub duration_ms: Option<i64>,
    pub track_number: Option<i64>,
    pub disc_number: Option<i64>,
    pub art_path: Option<String>,
}

pub struct LibraryDb {
    conn: Connection,
}

impl LibraryDb {
    pub fn open_default() -> Result<Self, String> {
        ensure_data_dirs()?;
        configure_sqlite_threading();
        probe_std_write();
        let conn = open_sqlite_with_diagnostics()?;
        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<(), String> {
        self.conn
            .execute_batch(
                "
                PRAGMA foreign_keys = ON;
                PRAGMA busy_timeout = 5000;
                PRAGMA synchronous = NORMAL;

                CREATE TABLE IF NOT EXISTS library_roots (
                    id INTEGER PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    last_scanned_at INTEGER
                );

                CREATE TABLE IF NOT EXISTS artwork (
                    id INTEGER PRIMARY KEY,
                    source_type TEXT NOT NULL,
                    source_path TEXT,
                    cache_path TEXT NOT NULL UNIQUE,
                    mime_type TEXT,
                    width INTEGER,
                    height INTEGER,
                    hash TEXT UNIQUE
                );

                CREATE TABLE IF NOT EXISTS albums (
                    id INTEGER PRIMARY KEY,
                    title TEXT NOT NULL,
                    album_artist TEXT NOT NULL,
                    year INTEGER,
                    sort_title TEXT NOT NULL,
                    sort_album_artist TEXT NOT NULL,
                    art_id INTEGER,
                    UNIQUE(sort_album_artist, sort_title, year),
                    FOREIGN KEY(art_id) REFERENCES artwork(id)
                );

                CREATE TABLE IF NOT EXISTS artists (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    sort_name TEXT NOT NULL UNIQUE
                );

                CREATE TABLE IF NOT EXISTS tracks (
                    id INTEGER PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    parent_dir TEXT NOT NULL,
                    filename TEXT NOT NULL,
                    title TEXT,
                    track_artist TEXT,
                    album_artist TEXT,
                    album TEXT,
                    genre TEXT,
                    disc_number INTEGER,
                    disc_total INTEGER,
                    track_number INTEGER,
                    track_total INTEGER,
                    date TEXT,
                    year INTEGER,
                    duration_ms INTEGER,
                    sample_rate INTEGER,
                    channels INTEGER,
                    codec TEXT,
                    container TEXT,
                    file_size INTEGER NOT NULL,
                    modified_at INTEGER NOT NULL,
                    album_id INTEGER,
                    art_id INTEGER,
                    indexed_at INTEGER NOT NULL,
                    missing INTEGER NOT NULL DEFAULT 0,
                    FOREIGN KEY(album_id) REFERENCES albums(id),
                    FOREIGN KEY(art_id) REFERENCES artwork(id)
                );

                CREATE TABLE IF NOT EXISTS track_artists (
                    track_id INTEGER NOT NULL,
                    artist_id INTEGER NOT NULL,
                    role TEXT NOT NULL,
                    PRIMARY KEY(track_id, artist_id, role),
                    FOREIGN KEY(track_id) REFERENCES tracks(id) ON DELETE CASCADE,
                    FOREIGN KEY(artist_id) REFERENCES artists(id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_tracks_album_id ON tracks(album_id);
                CREATE INDEX IF NOT EXISTS idx_tracks_track_artist ON tracks(track_artist);
                CREATE INDEX IF NOT EXISTS idx_tracks_album_artist ON tracks(album_artist);
                CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);
                CREATE INDEX IF NOT EXISTS idx_tracks_parent_dir ON tracks(parent_dir);
                CREATE INDEX IF NOT EXISTS idx_albums_artist_title ON albums(sort_album_artist, sort_title);
                CREATE INDEX IF NOT EXISTS idx_artists_sort_name ON artists(sort_name);
                ",
            )
            .map_err(|err| err.to_string())
    }

    pub fn add_root(&self, path: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO library_roots(path, enabled) VALUES(?1, 1)
                 ON CONFLICT(path) DO UPDATE SET enabled = 1",
                params![normalize_root(path)],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn roots(&self) -> Result<Vec<RootRow>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, enabled, last_scanned_at FROM library_roots ORDER BY path")
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(RootRow {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    enabled: row.get::<_, i64>(2)? != 0,
                    last_scanned_at: row.get(3)?,
                })
            })
            .map_err(|err| err.to_string())?;
        collect_rows(rows)
    }

    pub fn artists(&self) -> Result<Vec<ArtistRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT artists.id, artists.name, COUNT(DISTINCT tracks.id) AS track_count
                 FROM artists
                 JOIN track_artists ON track_artists.artist_id = artists.id
                 JOIN tracks ON tracks.id = track_artists.track_id
                 WHERE tracks.missing = 0
                 GROUP BY artists.id
                 ORDER BY artists.sort_name",
            )
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ArtistRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    track_count: row.get(2)?,
                })
            })
            .map_err(|err| err.to_string())?;
        collect_rows(rows)
    }

    pub fn albums(&self) -> Result<Vec<AlbumRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT albums.id, albums.title, albums.album_artist, albums.year, artwork.cache_path,
                        COUNT(tracks.id) AS track_count
                 FROM albums
                 LEFT JOIN artwork ON artwork.id = albums.art_id
                 LEFT JOIN tracks ON tracks.album_id = albums.id AND tracks.missing = 0
                 GROUP BY albums.id
                 HAVING track_count > 0
                 ORDER BY albums.sort_album_artist, albums.year, albums.sort_title",
            )
            .map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map([], album_from_row)
            .map_err(|err| err.to_string())?;
        collect_rows(rows)
    }

    pub fn tracks(&self) -> Result<Vec<TrackRow>, String> {
        self.query_tracks(
            "SELECT tracks.id, tracks.path, COALESCE(tracks.title, tracks.filename),
                    COALESCE(tracks.track_artist, 'Unknown Artist'),
                    COALESCE(tracks.album, 'Unknown Album'),
                    COALESCE(tracks.album_artist, tracks.track_artist, 'Unknown Artist'),
                    tracks.duration_ms, tracks.track_number, tracks.disc_number, artwork.cache_path
             FROM tracks
             LEFT JOIN artwork ON artwork.id = tracks.art_id
             WHERE tracks.missing = 0
             ORDER BY COALESCE(tracks.track_artist, tracks.album_artist, ''),
                      COALESCE(tracks.album, ''),
                      COALESCE(tracks.disc_number, 1),
                      COALESCE(tracks.track_number, 9999),
                      tracks.filename",
            [],
        )
    }

    pub fn tracks_for_album(&self, album_id: i64) -> Result<Vec<TrackRow>, String> {
        self.query_tracks(
            "SELECT tracks.id, tracks.path, COALESCE(tracks.title, tracks.filename),
                    COALESCE(tracks.track_artist, 'Unknown Artist'),
                    COALESCE(tracks.album, 'Unknown Album'),
                    COALESCE(tracks.album_artist, tracks.track_artist, 'Unknown Artist'),
                    tracks.duration_ms, tracks.track_number, tracks.disc_number, artwork.cache_path
             FROM tracks
             LEFT JOIN artwork ON artwork.id = tracks.art_id
             WHERE tracks.missing = 0 AND tracks.album_id = ?1
             ORDER BY COALESCE(tracks.disc_number, 1), COALESCE(tracks.track_number, 9999), tracks.filename",
            params![album_id],
        )
    }

    pub fn tracks_for_artist(&self, artist_id: i64) -> Result<Vec<TrackRow>, String> {
        self.query_tracks(
            "SELECT tracks.id, tracks.path, COALESCE(tracks.title, tracks.filename),
                    COALESCE(tracks.track_artist, 'Unknown Artist'),
                    COALESCE(tracks.album, 'Unknown Album'),
                    COALESCE(tracks.album_artist, tracks.track_artist, 'Unknown Artist'),
                    tracks.duration_ms, tracks.track_number, tracks.disc_number, artwork.cache_path
             FROM tracks
             LEFT JOIN artwork ON artwork.id = tracks.art_id
             JOIN track_artists ON track_artists.track_id = tracks.id
             WHERE tracks.missing = 0 AND track_artists.artist_id = ?1
             ORDER BY COALESCE(tracks.album, ''), COALESCE(tracks.disc_number, 1),
                      COALESCE(tracks.track_number, 9999), tracks.filename",
            params![artist_id],
        )
    }

    fn query_tracks<P>(&self, sql: &str, params: P) -> Result<Vec<TrackRow>, String>
    where
        P: rusqlite::Params,
    {
        let mut stmt = self.conn.prepare(sql).map_err(|err| err.to_string())?;
        let rows = stmt
            .query_map(params, |row| {
                Ok(TrackRow {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    title: row.get(2)?,
                    track_artist: row.get(3)?,
                    album: row.get(4)?,
                    album_artist: row.get(5)?,
                    duration_ms: row.get(6)?,
                    track_number: row.get(7)?,
                    disc_number: row.get(8)?,
                    art_path: row.get(9)?,
                })
            })
            .map_err(|err| err.to_string())?;
        collect_rows(rows)
    }

    pub fn track_is_current(
        &self,
        path: &str,
        file_size: i64,
        modified_at: i64,
    ) -> Result<bool, String> {
        let current = self
            .conn
            .query_row(
                "SELECT 1 FROM tracks WHERE path = ?1 AND file_size = ?2 AND modified_at = ?3 AND missing = 0",
                params![path, file_size, modified_at],
                |_| Ok(()),
            )
            .optional()
            .map_err(|err| err.to_string())?
            .is_some();
        Ok(current)
    }

    pub fn begin_scan(&self, root: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE tracks SET missing = 1 WHERE path LIKE ?1",
                params![format!("{}%", normalize_root(root))],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn finish_scan(&self, root: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE library_roots SET last_scanned_at = ?2 WHERE path = ?1",
                params![normalize_root(root), now_ts()],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn mark_seen(&self, path: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE tracks SET missing = 0, indexed_at = ?2 WHERE path = ?1",
                params![path, now_ts()],
            )
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub fn upsert_track(&mut self, metadata: &TrackMetadata) -> Result<(), String> {
        let tx = self.conn.transaction().map_err(|err| err.to_string())?;
        let art_id = if let Some(art) = &metadata.artwork {
            tx.execute(
                "INSERT INTO artwork(source_type, source_path, cache_path, mime_type, width, height, hash)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(cache_path) DO UPDATE SET
                    source_type = excluded.source_type,
                    source_path = excluded.source_path,
                    mime_type = excluded.mime_type,
                    width = excluded.width,
                    height = excluded.height,
                    hash = COALESCE(excluded.hash, artwork.hash)",
                params![
                    art.source_type,
                    art.source_path,
                    art.cache_path,
                    art.mime_type,
                    art.width.map(i64::from),
                    art.height.map(i64::from),
                    art.hash,
                ],
            )
            .map_err(|err| err.to_string())?;
            Some(
                tx.query_row(
                    "SELECT id FROM artwork WHERE cache_path = ?1",
                    params![art.cache_path],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|err| err.to_string())?,
            )
        } else {
            None
        };

        let album_artist = metadata
            .album_artist
            .as_deref()
            .unwrap_or(&metadata.track_artist);
        let album_title = metadata.album.as_deref().unwrap_or("Unknown Album");
        let album_id = upsert_album_tx(&tx, album_title, album_artist, metadata.year, art_id)?;
        let track_artist_id = upsert_artist_tx(&tx, &metadata.track_artist)?;
        let album_artist_id = upsert_artist_tx(&tx, album_artist)?;

        tx.execute(
            "INSERT INTO tracks(
                path, parent_dir, filename, title, track_artist, album_artist, album, genre,
                disc_number, disc_total, track_number, track_total, date, year,
                duration_ms, sample_rate, channels, codec, container, file_size, modified_at,
                album_id, art_id, indexed_at, missing
             ) VALUES(
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19, ?20, ?21,
                ?22, ?23, ?24, 0
             )
             ON CONFLICT(path) DO UPDATE SET
                parent_dir = excluded.parent_dir,
                filename = excluded.filename,
                title = excluded.title,
                track_artist = excluded.track_artist,
                album_artist = excluded.album_artist,
                album = excluded.album,
                genre = excluded.genre,
                disc_number = excluded.disc_number,
                disc_total = excluded.disc_total,
                track_number = excluded.track_number,
                track_total = excluded.track_total,
                date = excluded.date,
                year = excluded.year,
                duration_ms = excluded.duration_ms,
                sample_rate = excluded.sample_rate,
                channels = excluded.channels,
                codec = excluded.codec,
                container = excluded.container,
                file_size = excluded.file_size,
                modified_at = excluded.modified_at,
                album_id = excluded.album_id,
                art_id = excluded.art_id,
                indexed_at = excluded.indexed_at,
                missing = 0",
            params![
                metadata.path,
                metadata.parent_dir,
                metadata.filename,
                metadata.title,
                metadata.track_artist,
                metadata.album_artist,
                metadata.album,
                metadata.genre,
                metadata.disc_number.map(i64::from),
                metadata.disc_total.map(i64::from),
                metadata.track_number.map(i64::from),
                metadata.track_total.map(i64::from),
                metadata.date,
                metadata.year.map(i64::from),
                metadata.duration_ms.map(|v| v as i64),
                metadata.sample_rate.map(i64::from),
                metadata.channels.map(i64::from),
                metadata.codec,
                metadata.container,
                metadata.file_size,
                metadata.modified_at,
                album_id,
                art_id,
                now_ts(),
            ],
        )
        .map_err(|err| err.to_string())?;

        let track_id = tx
            .query_row(
                "SELECT id FROM tracks WHERE path = ?1",
                params![metadata.path],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|err| err.to_string())?;
        tx.execute(
            "DELETE FROM track_artists WHERE track_id = ?1",
            params![track_id],
        )
        .map_err(|err| err.to_string())?;
        tx.execute(
            "INSERT OR IGNORE INTO track_artists(track_id, artist_id, role) VALUES(?1, ?2, 'track_artist')",
            params![track_id, track_artist_id],
        )
        .map_err(|err| err.to_string())?;
        tx.execute(
            "INSERT OR IGNORE INTO track_artists(track_id, artist_id, role) VALUES(?1, ?2, 'album_artist')",
            params![track_id, album_artist_id],
        )
        .map_err(|err| err.to_string())?;

        tx.commit().map_err(|err| err.to_string())
    }
}

fn upsert_album_tx(
    tx: &rusqlite::Transaction<'_>,
    title: &str,
    album_artist: &str,
    year: Option<u32>,
    art_id: Option<i64>,
) -> Result<i64, String> {
    let sort_title = normalize_sort(title);
    let sort_album_artist = normalize_sort(album_artist);
    tx.execute(
        "INSERT INTO albums(title, album_artist, year, sort_title, sort_album_artist, art_id)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(sort_album_artist, sort_title, year) DO UPDATE SET
            title = excluded.title,
            album_artist = excluded.album_artist,
            art_id = COALESCE(excluded.art_id, albums.art_id)",
        params![
            title,
            album_artist,
            year.map(i64::from),
            sort_title,
            sort_album_artist,
            art_id
        ],
    )
    .map_err(|err| err.to_string())?;
    tx.query_row(
        "SELECT id FROM albums WHERE sort_album_artist = ?1 AND sort_title = ?2 AND (year IS ?3 OR year = ?3)",
        params![sort_album_artist, sort_title, year.map(i64::from)],
        |row| row.get::<_, i64>(0),
    )
    .map_err(|err| err.to_string())
}

fn upsert_artist_tx(tx: &rusqlite::Transaction<'_>, name: &str) -> Result<i64, String> {
    let sort_name = normalize_sort(name);
    tx.execute(
        "INSERT INTO artists(name, sort_name) VALUES(?1, ?2)
         ON CONFLICT(sort_name) DO UPDATE SET name = excluded.name",
        params![name, sort_name],
    )
    .map_err(|err| err.to_string())?;
    tx.query_row(
        "SELECT id FROM artists WHERE sort_name = ?1",
        params![sort_name],
        |row| row.get::<_, i64>(0),
    )
    .map_err(|err| err.to_string())
}

fn ensure_data_dirs() -> Result<(), String> {
    fs::create_dir_all(DATA_DIR).map_err(|err| format!("create {DATA_DIR}: {err}"))?;
    log::append(format!("library: ensured data dir {DATA_DIR}"));
    Ok(())
}

#[cfg(target_os = "vita")]
extern "C" {
    fn mpvrs_sqlite_configure_mutex() -> std::os::raw::c_int;
}

fn configure_sqlite_threading() {
    static CONFIGURED: std::sync::Once = std::sync::Once::new();
    CONFIGURED.call_once(|| unsafe {
        #[cfg(target_os = "vita")]
        let mutex_rc = mpvrs_sqlite_configure_mutex();
        #[cfg(not(target_os = "vita"))]
        let mutex_rc = 0;

        let threadsafe = rusqlite::ffi::sqlite3_threadsafe();
        let serialized_rc = rusqlite::ffi::sqlite3_config(rusqlite::ffi::SQLITE_CONFIG_SERIALIZED);
        log::append(format!(
            "library: sqlite threading: sqlite3_threadsafe={threadsafe}, config_mutex_rc={mutex_rc}, config_serialized_rc={serialized_rc}"
        ));
    });
}

fn probe_std_write() {
    match std::env::current_dir() {
        Ok(dir) => log::append(format!("library: current_dir: {}", dir.display())),
        Err(err) => log::append(format!("library: current_dir failed: {err}")),
    }

    let probes = [format!("{DATA_DIR}/write-probe.tmp")];

    for probe_path in probes {
        match std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&probe_path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, b"mpvrs write probe\n"))
        {
            Ok(()) => {
                log::append(format!("library: std write probe ok: {probe_path}"));
                let _ = fs::remove_file(&probe_path);
            }
            Err(err) => log::append(format!(
                "library: std write probe failed: {probe_path}: {err:?}"
            )),
        }
    }
}

fn open_sqlite_with_diagnostics() -> Result<Connection, String> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;

    let mut errors = Vec::new();

    log::append(format!("library: trying sqlite open: {DB_PATH}"));
    match Connection::open_with_flags(DB_PATH, flags) {
        Ok(conn) => {
            log::append(format!("library: sqlite open ok: {DB_PATH}"));
            Ok(conn)
        }
        Err(err) => {
            log::append(format!("library: sqlite open failed: {DB_PATH}: {err:?}"));
            errors.push(format!("{DB_PATH}: {err}"));
            Err(format!("open sqlite failed: {}", errors.join(" | ")))
        }
    }
}

fn album_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AlbumRow> {
    Ok(AlbumRow {
        id: row.get(0)?,
        title: row.get(1)?,
        album_artist: row.get(2)?,
        year: row.get(3)?,
        art_path: row.get(4)?,
        track_count: row.get(5)?,
    })
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> Result<Vec<T>, String> {
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())
}

pub fn normalize_root(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.ends_with('/') {
        trimmed.to_owned()
    } else {
        format!("{trimmed}/")
    }
}

pub fn normalize_sort(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub fn parent_dir(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|path| {
            let mut text = path.to_string_lossy().into_owned();
            if !text.ends_with('/') {
                text.push('/');
            }
            text
        })
        .unwrap_or_default()
}
