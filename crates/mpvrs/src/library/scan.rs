use std::fs;
use std::path::Path;

use crate::library::db::{normalize_root, LibraryDb};
use crate::library::metadata::{file_signature, read_track_metadata};
use crate::plumbing::audio::is_supported_audio_file;

#[derive(Clone, Debug, Default)]
pub struct ScanProgress {
    pub root: String,
    pub current_path: String,
    pub files_seen: usize,
    pub tracks_indexed: usize,
    pub tracks_skipped: usize,
    pub errors: usize,
    pub done: bool,
    pub status: String,
}

pub struct Scanner {
    db: LibraryDb,
    progress: ScanProgress,
}

impl Scanner {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            db: LibraryDb::open_default()?,
            progress: ScanProgress::default(),
        })
    }

    pub fn scan_root(mut self, root: String) -> ScanProgress {
        let root = normalize_root(&root);
        self.progress.root = root.clone();
        self.progress.status = format!("Scanning {root}");

        if let Err(err) = self.db.add_root(&root) {
            self.finish_with_error(format!("Failed to add root: {err}"));
            return self.progress;
        }
        if let Err(err) = self.db.begin_scan(&root) {
            self.finish_with_error(format!("Failed to begin scan: {err}"));
            return self.progress;
        }

        self.walk_dir(&root);

        if let Err(err) = self.db.finish_scan(&root) {
            self.progress.errors += 1;
            self.progress.status = format!("Scan finished, but failed to update root: {err}");
        } else {
            self.progress.status = format!(
                "Scan finished: {} indexed, {} unchanged, {} errors",
                self.progress.tracks_indexed, self.progress.tracks_skipped, self.progress.errors
            );
        }
        self.progress.done = true;
        self.progress
    }

    fn walk_dir(&mut self, dir: &str) {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) => {
                self.progress.errors += 1;
                self.progress.status = format!("Failed to read {dir}: {err}");
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let path_text = path.to_string_lossy().into_owned();
            self.progress.current_path = path_text.clone();

            if path.is_dir() {
                self.walk_dir(&ensure_trailing_slash(&path_text));
            } else if path.is_file() && is_supported_audio_file(&path_text) {
                self.index_file(&path_text);
            }
        }
    }

    fn index_file(&mut self, path: &str) {
        self.progress.files_seen += 1;

        if let Some((size, modified)) = file_signature(path) {
            match self.db.track_is_current(path, size, modified) {
                Ok(true) => {
                    if self.db.mark_seen(path).is_err() {
                        self.progress.errors += 1;
                    }
                    self.progress.tracks_skipped += 1;
                    return;
                }
                Ok(false) => {}
                Err(_) => self.progress.errors += 1,
            }
        }

        match read_track_metadata(path) {
            Ok(metadata) => match self.db.upsert_track(&metadata) {
                Ok(()) => self.progress.tracks_indexed += 1,
                Err(err) => {
                    self.progress.errors += 1;
                    self.progress.status = format!("Failed to save {path}: {err}");
                }
            },
            Err(err) => {
                self.progress.errors += 1;
                self.progress.status = format!("Failed to read {path}: {err}");
            }
        }
    }

    fn finish_with_error(&mut self, status: String) {
        self.progress.errors += 1;
        self.progress.done = true;
        self.progress.status = status;
    }
}

fn ensure_trailing_slash(path: &str) -> String {
    if path.ends_with('/') {
        path.to_owned()
    } else {
        format!("{path}/")
    }
}

#[allow(dead_code)]
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}
