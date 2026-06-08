use std::fs::File;
use std::path::Path;
use std::time::UNIX_EPOCH;

use symphonia::core::codecs::CODEC_TYPE_NULL;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::{MetadataRevision, StandardTagKey, Tag, Value};
use symphonia::core::probe::Hint;
use symphonia::default::get_probe;

use crate::library::artwork::{cache_embedded_art, find_folder_art, ArtworkInfo};
use crate::library::db::parent_dir;

#[derive(Clone, Debug)]
pub struct TrackMetadata {
    pub path: String,
    pub parent_dir: String,
    pub filename: String,
    pub title: Option<String>,
    pub track_artist: String,
    pub track_artists: Vec<String>,
    pub album_artist: Option<String>,
    pub album_artists: Vec<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub disc_number: Option<u32>,
    pub disc_total: Option<u32>,
    pub track_number: Option<u32>,
    pub track_total: Option<u32>,
    pub date: Option<String>,
    pub year: Option<u32>,
    pub duration_ms: Option<u64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub codec: Option<String>,
    pub container: Option<String>,
    pub file_size: i64,
    pub modified_at: i64,
    pub artwork: Option<ArtworkInfo>,
}

#[derive(Default)]
struct Tags {
    title: Option<String>,
    artists: Vec<String>,
    album_artists: Vec<String>,
    album: Option<String>,
    genre: Option<String>,
    disc_number: Option<u32>,
    disc_total: Option<u32>,
    track_number: Option<u32>,
    track_total: Option<u32>,
    date: Option<String>,
    year: Option<u32>,
    artwork: Option<ArtworkInfo>,
}

pub fn read_track_metadata(path: &str) -> Result<TrackMetadata, String> {
    let stat = std::fs::metadata(path).map_err(|err| err.to_string())?;
    let file_size = stat.len() as i64;
    let modified_at = stat
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);

    let file = File::open(path).map_err(|err| err.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(ext) = Path::new(path).extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }

    let format_options = FormatOptions {
        prebuild_seek_index: false,
        seek_index_fill_rate: 1,
        enable_gapless: true,
    };
    let mut probed = get_probe()
        .format(&hint, mss, &format_options, &Default::default())
        .map_err(|err| err.to_string())?;

    let mut tags = Tags::default();
    if let Some(metadata) = probed.metadata.get() {
        if let Some(revision) = metadata.current() {
            merge_revision(path, &mut tags, revision);
        }
    }
    {
        let metadata = probed.format.metadata();
        if let Some(revision) = metadata.current() {
            merge_revision(path, &mut tags, revision);
        }
    }

    let track = probed
        .format
        .default_track()
        .or_else(|| {
            probed
                .format
                .tracks()
                .iter()
                .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        })
        .ok_or_else(|| "No supported audio track found".to_owned())?;
    let params = &track.codec_params;

    let sample_rate = params.sample_rate;
    let duration_ms = match (params.n_frames, sample_rate) {
        (Some(frames), Some(rate)) if rate > 0 => Some(frames.saturating_mul(1000) / rate as u64),
        _ => None,
    };
    let channels = params.channels.map(|channels| channels.count() as u32);
    let codec = Some(format!("{:?}", params.codec));
    let container = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());

    let filename = Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned());
    let fallback_title = Path::new(path)
        .file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| filename.clone());
    let fallback_album = Path::new(path)
        .parent()
        .and_then(|parent| parent.file_name())
        .map(|name| name.to_string_lossy().into_owned());

    let title = tags
        .title
        .or_else(|| Some(strip_track_prefix(&fallback_title)));
    let album = tags.album.or(fallback_album);
    let album_artist = join_artists(&tags.album_artists);
    let track_artists = if tags.artists.is_empty() {
        tags.album_artists.clone()
    } else {
        tags.artists.clone()
    };
    let track_artist = join_artists(&track_artists).unwrap_or_else(|| "Unknown Artist".to_owned());
    let artwork = tags.artwork.or_else(|| find_folder_art(path));

    Ok(TrackMetadata {
        path: path.to_owned(),
        parent_dir: parent_dir(path),
        filename,
        title,
        track_artist,
        track_artists,
        album_artist,
        album_artists: tags.album_artists,
        album,
        genre: tags.genre,
        disc_number: tags.disc_number,
        disc_total: tags.disc_total,
        track_number: tags
            .track_number
            .or_else(|| parse_leading_track_number(&fallback_title)),
        track_total: tags.track_total,
        date: tags.date,
        year: tags.year,
        duration_ms,
        sample_rate,
        channels,
        codec,
        container,
        file_size,
        modified_at,
        artwork,
    })
}

pub fn file_signature(path: &str) -> Option<(i64, i64)> {
    let stat = std::fs::metadata(path).ok()?;
    let modified_at = stat
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    Some((stat.len() as i64, modified_at))
}

fn merge_revision(path: &str, tags: &mut Tags, revision: &MetadataRevision) {
    for tag in revision.tags() {
        merge_tag(tags, tag);
    }

    if tags.artwork.is_none() {
        if let Some(visual) = revision.visuals().first() {
            let dimensions = visual.dimensions;
            tags.artwork = cache_embedded_art(
                path,
                &visual.media_type,
                dimensions.map(|size| size.width),
                dimensions.map(|size| size.height),
                &visual.data,
            );
        }
    }
}

fn merge_tag(tags: &mut Tags, tag: &Tag) {
    let value = clean_tag_value(&tag.value);
    if value.is_empty() {
        return;
    }

    match tag.std_key {
        Some(StandardTagKey::TrackTitle) => set_once(&mut tags.title, value),
        Some(StandardTagKey::Artist) => push_artist_value(&mut tags.artists, &value),
        Some(StandardTagKey::AlbumArtist) => push_artist_value(&mut tags.album_artists, &value),
        Some(StandardTagKey::Album) => set_once(&mut tags.album, value),
        Some(StandardTagKey::Genre) => set_once(&mut tags.genre, value),
        Some(StandardTagKey::Date | StandardTagKey::ReleaseDate | StandardTagKey::OriginalDate) => {
            if tags.year.is_none() {
                tags.year = parse_year(&value);
            }
            set_once(&mut tags.date, value);
        }
        Some(StandardTagKey::TrackNumber) => {
            merge_fraction(&value, &mut tags.track_number, &mut tags.track_total)
        }
        Some(StandardTagKey::TrackTotal) => {
            tags.track_total = tags.track_total.or_else(|| parse_u32(&value))
        }
        Some(StandardTagKey::DiscNumber) => {
            merge_fraction(&value, &mut tags.disc_number, &mut tags.disc_total)
        }
        Some(StandardTagKey::DiscTotal) => {
            tags.disc_total = tags.disc_total.or_else(|| parse_u32(&value))
        }
        _ => merge_unmapped_tag(tags, tag, value),
    }
}

fn merge_unmapped_tag(tags: &mut Tags, tag: &Tag, value: String) {
    let key = tag.key.to_ascii_lowercase().replace([' ', '-', '_'], "");
    match key.as_str() {
        "title" => set_once(&mut tags.title, value),
        "artist" | "artists" | "artistsort" => push_artist_value(&mut tags.artists, &value),
        "albumartist" | "albumartists" | "albumartistsort" | "band" => {
            push_artist_value(&mut tags.album_artists, &value)
        }
        "album" => set_once(&mut tags.album, value),
        "genre" => set_once(&mut tags.genre, value),
        "date" | "year" => {
            if tags.year.is_none() {
                tags.year = parse_year(&value);
            }
            set_once(&mut tags.date, value);
        }
        "track" | "tracknumber" => {
            merge_fraction(&value, &mut tags.track_number, &mut tags.track_total)
        }
        "totaltracks" | "tracktotal" => {
            tags.track_total = tags.track_total.or_else(|| parse_u32(&value))
        }
        "disc" | "discnumber" | "disknumber" => {
            merge_fraction(&value, &mut tags.disc_number, &mut tags.disc_total)
        }
        "totaldiscs" | "disctotal" | "disktotal" => {
            tags.disc_total = tags.disc_total.or_else(|| parse_u32(&value))
        }
        _ => {}
    }
}

fn set_once(slot: &mut Option<String>, value: String) {
    if slot.is_none() {
        *slot = Some(value);
    }
}

fn push_artist_value(artists: &mut Vec<String>, value: &str) {
    for artist in split_artist_value(value) {
        if !artists
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&artist))
        {
            artists.push(artist);
        }
    }
}

fn join_artists(artists: &[String]) -> Option<String> {
    if artists.is_empty() {
        None
    } else {
        Some(artists.join(", "))
    }
}

fn split_artist_value(value: &str) -> Vec<String> {
    let mut text = value.trim().to_owned();
    for delimiter in [" featuring ", " feat. ", " feat ", " ft. ", " ft ", " & "] {
        text = replace_ascii_case_insensitive(&text, delimiter, ",");
    }

    let mut artists = Vec::new();
    for part in text.split([',', ';']) {
        let artist = part.trim().trim_matches('\0').trim();
        if !artist.is_empty()
            && !artists
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(artist))
        {
            artists.push(artist.to_owned());
        }
    }
    artists
}

fn replace_ascii_case_insensitive(value: &str, needle: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut index = 0;
    while index < value.len() {
        if value[index..]
            .get(..needle.len())
            .is_some_and(|part| part.eq_ignore_ascii_case(needle))
        {
            out.push_str(replacement);
            index += needle.len();
        } else if let Some(ch) = value[index..].chars().next() {
            out.push(ch);
            index += ch.len_utf8();
        } else {
            break;
        }
    }
    out
}

fn clean_tag_value(value: &Value) -> String {
    value
        .to_string()
        .trim()
        .trim_matches('\0')
        .trim()
        .to_owned()
}

fn merge_fraction(value: &str, number: &mut Option<u32>, total: &mut Option<u32>) {
    let mut parts = value.split('/');
    if number.is_none() {
        *number = parts.next().and_then(parse_u32);
    }
    if total.is_none() {
        *total = parts.next().and_then(parse_u32);
    }
}

fn parse_u32(value: &str) -> Option<u32> {
    let digits: String = value
        .trim()
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn parse_year(value: &str) -> Option<u32> {
    let digits: String = value
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .take(4)
        .collect();
    if digits.len() == 4 {
        digits.parse().ok()
    } else {
        None
    }
}

fn parse_leading_track_number(value: &str) -> Option<u32> {
    let trimmed = value.trim_start();
    let digits: String = trimmed
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.is_empty() || digits.len() > 3 {
        return None;
    }
    let rest = &trimmed[digits.len()..];
    if rest.starts_with([' ', '.', '-', '_']) {
        digits.parse().ok()
    } else {
        None
    }
}

fn strip_track_prefix(value: &str) -> String {
    let trimmed = value.trim_start();
    let digits: String = trimmed
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.is_empty() || digits.len() > 3 {
        return value.to_owned();
    }
    let rest = trimmed[digits.len()..]
        .trim_start_matches([' ', '.', '-', '_'])
        .trim_start();
    if rest.is_empty() {
        value.to_owned()
    } else {
        rest.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::split_artist_value;

    #[test]
    fn splits_common_multi_artist_formats() {
        let cases = [
            (
                "Porter Robinson & Riot Games",
                vec!["Porter Robinson", "Riot Games"],
            ),
            (
                "Porter Robinson feat. Amy Millan",
                vec!["Porter Robinson", "Amy Millan"],
            ),
            (
                "Porter Robinson, Amy Millan, ODESZA",
                vec!["Porter Robinson", "Amy Millan", "ODESZA"],
            ),
            (
                "Raja Kumari & Stefflon Don feat. Jarina De Marco",
                vec!["Raja Kumari", "Stefflon Don", "Jarina De Marco"],
            ),
            ("Secondcity ft. Ali Love", vec!["Secondcity", "Ali Love"]),
        ];

        for (input, expected) in cases {
            assert_eq!(split_artist_value(input), expected);
        }
    }

    #[test]
    fn deduplicates_artists_case_insensitively() {
        assert_eq!(
            split_artist_value("Porter Robinson, porter robinson & Riot Games"),
            vec!["Porter Robinson", "Riot Games"]
        );
    }
}
