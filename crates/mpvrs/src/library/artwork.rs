use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::library::ART_DIR;

#[derive(Clone, Debug)]
pub struct ArtworkInfo {
    pub source_type: String,
    pub source_path: Option<String>,
    pub cache_path: String,
    pub mime_type: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub hash: Option<String>,
}

pub fn cache_embedded_art(
    source_path: &str,
    mime_type: &str,
    width: Option<u32>,
    height: Option<u32>,
    data: &[u8],
) -> Option<ArtworkInfo> {
    if data.is_empty() {
        return None;
    }

    let hash = hash_bytes(data);
    let ext = extension_for_mime(mime_type).unwrap_or("bin");
    let dir = format!("{ART_DIR}/embedded");
    if fs::create_dir_all(&dir).is_err() {
        return None;
    }
    let cache_path = format!("{dir}/{hash}.{ext}");
    if !Path::new(&cache_path).exists() && fs::write(&cache_path, data).is_err() {
        return None;
    }

    Some(ArtworkInfo {
        source_type: "embedded".to_owned(),
        source_path: Some(source_path.to_owned()),
        cache_path,
        mime_type: Some(mime_type.to_owned()),
        width,
        height,
        hash: Some(hash),
    })
}

pub fn find_folder_art(audio_path: &str) -> Option<ArtworkInfo> {
    let parent = Path::new(audio_path).parent()?;
    let names = [
        "cover.jpg",
        "cover.jpeg",
        "cover.png",
        "folder.jpg",
        "folder.jpeg",
        "folder.png",
        "front.jpg",
        "front.jpeg",
        "front.png",
        "album.jpg",
        "album.jpeg",
        "album.png",
        "AlbumArtSmall.jpg",
    ];

    for name in names {
        let candidate = parent.join(name);
        if candidate.is_file() {
            return artwork_for_file(candidate);
        }
    }

    let entries = fs::read_dir(parent).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && is_image_file(&path) {
            return artwork_for_file(path);
        }
    }

    None
}

fn artwork_for_file(path: PathBuf) -> Option<ArtworkInfo> {
    let cache_path = path.to_string_lossy().into_owned();
    Some(ArtworkInfo {
        source_type: "folder".to_owned(),
        source_path: Some(cache_path.clone()),
        mime_type: mime_for_path(&path).map(str::to_owned),
        cache_path,
        width: None,
        height: None,
        hash: None,
    })
}

fn is_image_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "png")
    )
}

fn mime_for_path(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("png") => Some("image/png"),
        _ => None,
    }
}

fn extension_for_mime(mime: &str) -> Option<&'static str> {
    match mime.to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
