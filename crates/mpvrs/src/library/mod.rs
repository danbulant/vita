pub mod artwork;
pub mod db;
pub mod log;
pub mod metadata;
pub mod scan;

pub use db::{AlbumRow, ArtistRow, LibraryDb, RootRow, TrackRow};
pub use scan::{ScanProgress, Scanner};

pub const DATA_DIR: &str = "ux0:data/mpvrs";
pub const DB_PATH: &str = "ux0:data/mpvrs/library.sqlite";
pub const ART_DIR: &str = "ux0:data/mpvrs/art";
