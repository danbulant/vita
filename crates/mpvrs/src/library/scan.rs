use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::library::db::{normalize_root, LibraryDb};
use crate::library::metadata::{file_signature, read_track_metadata};
use crate::plumbing::audio::is_supported_audio_file;

#[derive(Clone, Debug, Default)]
pub struct ScanProgress {
    pub root: String,
    pub current_path: String,
    pub files_seen: usize,
    pub files_remaining: usize,
    pub dirs_remaining: usize,
    pub tracks_indexed: usize,
    pub tracks_skipped: usize,
    pub tracks_duplicated: usize,
    pub errors: usize,
    pub done: bool,
    pub status: String,
}

pub struct Scanner {
    db: LibraryDb,
    progress: ScanProgress,
    shared_progress: Option<Arc<Mutex<ScanProgress>>>,
}

impl Scanner {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            db: LibraryDb::open_default()?,
            progress: ScanProgress::default(),
            shared_progress: None,
        })
    }

    pub fn scan_root_with_progress(
        mut self,
        root: String,
        shared_progress: Arc<Mutex<ScanProgress>>,
    ) -> ScanProgress {
        self.shared_progress = Some(shared_progress);
        self.scan_root(root)
    }

    pub fn scan_root(mut self, root: String) -> ScanProgress {
        let root = normalize_root(&root);
        self.progress.root = root.clone();
        self.progress.status = self.format_running_status();
        self.publish_progress();

        if let Err(err) = self.db.add_root(&root) {
            self.finish_with_error(format!("Failed to add root: {err}"));
            return self.progress;
        }
        if let Err(err) = self.db.begin_scan(&root) {
            self.finish_with_error(format!("Failed to begin scan: {err}"));
            return self.progress;
        }

        self.scan_work(vec![ScanWork::Dir(root.clone())]);

        self.progress.status = "Refreshing library summaries".to_owned();
        self.publish_progress();
        if let Err(err) = self.db.finish_scan(&root) {
            self.progress.errors += 1;
            self.progress.status = format!("Scan finished, but failed to update root: {err}");
        } else {
            self.progress.status = format!(
                "Scan finished: {} indexed, {} unchanged, {} duplicates, {} errors",
                self.progress.tracks_indexed,
                self.progress.tracks_skipped,
                self.progress.tracks_duplicated,
                self.progress.errors
            );
        }
        self.progress.done = true;
        self.publish_progress();
        self.progress
    }

    fn scan_work(&mut self, mut pending: Vec<ScanWork>) {
        self.progress.dirs_remaining = pending
            .iter()
            .filter(|item| matches!(item, ScanWork::Dir(_)))
            .count();
        self.publish_progress();

        while let Some(item) = pending.pop() {
            match item {
                ScanWork::Dir(dir) => {
                    self.progress.dirs_remaining = self.progress.dirs_remaining.saturating_sub(1);
                    self.progress.current_path = dir.clone();
                    let entries = match fs::read_dir(&dir) {
                        Ok(entries) => entries,
                        Err(err) => {
                            self.progress.errors += 1;
                            self.progress.status = format!("Failed to read {dir}: {err}");
                            self.publish_progress();
                            continue;
                        }
                    };

                    for entry in entries.flatten() {
                        let path = entry.path();
                        let path_text = path.to_string_lossy().into_owned();
                        if path.is_dir() {
                            pending.push(ScanWork::Dir(ensure_trailing_slash(&path_text)));
                            self.progress.dirs_remaining += 1;
                        } else if path.is_file() && is_supported_audio_file(&path_text) {
                            pending.push(ScanWork::File(path_text));
                            self.progress.files_remaining += 1;
                        }
                    }
                    self.progress.status = self.format_running_status();
                    self.publish_progress();
                }
                ScanWork::File(path) => {
                    self.progress.files_remaining = self.progress.files_remaining.saturating_sub(1);
                    self.progress.current_path = path.clone();
                    self.index_file(&path);
                }
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
                    self.progress.status = self.format_running_status();
                    self.publish_progress();
                    return;
                }
                Ok(false) => {}
                Err(_) => self.progress.errors += 1,
            }
        }

        match read_track_metadata(path) {
            Ok(metadata) => match self.db.upsert_track(&metadata) {
                Ok(true) => self.progress.tracks_indexed += 1,
                Ok(false) => self.progress.tracks_duplicated += 1,
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
        self.progress.status = self.format_running_status();
        self.publish_progress();
    }

    fn format_running_status(&self) -> String {
        format!(
            "Scanning: {} indexed, {} unchanged, {} duplicates, {} known files remaining, {} dirs queued",
            self.progress.tracks_indexed,
            self.progress.tracks_skipped,
            self.progress.tracks_duplicated,
            self.progress.files_remaining,
            self.progress.dirs_remaining
        )
    }

    fn publish_progress(&self) {
        if let Some(shared_progress) = &self.shared_progress {
            if let Ok(mut shared_progress) = shared_progress.lock() {
                *shared_progress = self.progress.clone();
            }
        }
    }

    fn finish_with_error(&mut self, status: String) {
        self.progress.errors += 1;
        self.progress.done = true;
        self.progress.status = status;
        self.publish_progress();
    }
}

enum ScanWork {
    Dir(String),
    File(String),
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
