//! Music directory scanning and basic tag reading.

use std::{
    cmp::Ordering,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};
use symphonia::core::{
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::{MetadataOptions, MetadataRevision, StandardTagKey},
    probe::Hint,
};

/// File extensions (lowercase) the player accepts.
pub const EXTENSIONS: [&str; 4] = ["mp3", "wav", "flac", "ogg"];

/// Label used for tracks that have no album tag.
pub const UNKNOWN_ALBUM: &str = "Unknown Album";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub path: PathBuf,
    /// Title tag, or the file name without extension when there is none.
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub number: Option<u32>,
}

impl Track {
    pub fn album_label(&self) -> &str {
        self.album.as_deref().unwrap_or(UNKNOWN_ALBUM)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Tags {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    number: Option<u32>,
}

impl Tags {
    /// Fills fields that are still empty from `rev`.
    fn merge(&mut self, rev: &MetadataRevision) {
        for tag in rev.tags() {
            let value = tag.value.to_string();
            let value = value.trim_matches(|c: char| c.is_whitespace() || c == '\0');
            if value.is_empty() {
                continue;
            }
            match tag.std_key {
                Some(StandardTagKey::TrackTitle) if self.title.is_none() => {
                    self.title = Some(value.to_owned());
                }
                Some(StandardTagKey::Artist) if self.artist.is_none() => {
                    self.artist = Some(value.to_owned());
                }
                Some(StandardTagKey::Album) if self.album.is_none() => {
                    self.album = Some(value.to_owned());
                }
                Some(StandardTagKey::TrackNumber) if self.number.is_none() => {
                    // Often stored as "3/12".
                    self.number = value.split('/').next().and_then(|n| n.trim().parse().ok());
                }
                _ => {}
            }
        }
    }
}

/// Reads tags from `path`. Any failure (unreadable, unknown or corrupted
/// file) yields empty tags.
fn read_tags(path: &Path) -> Tags {
    let mut tags = Tags::default();
    let Ok(file) = File::open(path) else {
        return tags;
    };
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let Ok(mut probed) = symphonia::default::get_probe().format(
        &hint,
        stream,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    ) else {
        return tags;
    };
    // Tags in the container (FLAC, Ogg, WAV) first, then those found before it (ID3).
    if let Some(rev) = probed.format.metadata().skip_to_latest() {
        tags.merge(rev);
    }
    if let Some(rev) = probed
        .metadata
        .get()
        .as_mut()
        .and_then(|m| m.skip_to_latest())
    {
        tags.merge(rev);
    }
    tags
}

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|s| e.eq_ignore_ascii_case(s)))
}

/// Display order: albums by name (tracks without an album last), then track
/// number (tracks without a number last), then title. Tracks of one album
/// end up next to each other.
fn compare(a: &Track, b: &Track) -> Ordering {
    let album = |t: &Track| (t.album.is_none(), t.album.as_deref().map(str::to_lowercase));
    let title = |t: &Track| t.title.to_lowercase();
    album(a)
        .cmp(&album(b))
        .then_with(|| a.number.is_none().cmp(&b.number.is_none()))
        .then_with(|| a.number.cmp(&b.number))
        .then_with(|| title(a).cmp(&title(b)))
        .then_with(|| a.path.cmp(&b.path))
}

/// Scans `dir` (non-recursively) for supported audio files and reads their
/// tags. Tracks are returned in display order, see [`compare`].
/// Entries that cannot be read are skipped; files without usable tags are kept.
pub fn scan(dir: &Path) -> io::Result<Vec<Track>> {
    let mut tracks: Vec<Track> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .map(|entry| entry.path())
        .filter(|path| is_supported(path))
        .filter_map(|path| {
            let tags = read_tags(&path);
            let title = match tags.title {
                Some(title) => title,
                None => path.file_stem()?.to_string_lossy().into_owned(),
            };
            Some(Track {
                path,
                title,
                artist: tags.artist,
                album: tags.album,
                number: tags.number,
            })
        })
        .collect();
    tracks.sort_by(compare);
    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    /// Builds a small valid WAV file; `info` entries become RIFF INFO tags.
    fn wav(info: &[(&str, &str)]) -> Vec<u8> {
        fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut out = id.to_vec();
            out.extend((body.len() as u32).to_le_bytes());
            out.extend(body);
            if body.len() % 2 == 1 {
                out.push(0);
            }
            out
        }
        let mut fmt = Vec::new();
        fmt.extend(1u16.to_le_bytes()); // PCM
        fmt.extend(1u16.to_le_bytes()); // mono
        fmt.extend(8000u32.to_le_bytes());
        fmt.extend(16000u32.to_le_bytes());
        fmt.extend(2u16.to_le_bytes());
        fmt.extend(16u16.to_le_bytes());

        let mut body = b"WAVE".to_vec();
        body.extend(chunk(b"fmt ", &fmt));
        if !info.is_empty() {
            let mut list = b"INFO".to_vec();
            for (id, value) in info {
                let mut text = value.as_bytes().to_vec();
                text.push(0);
                let id: [u8; 4] = id.as_bytes().try_into().unwrap();
                list.extend(chunk(&id, &text));
            }
            body.extend(chunk(b"LIST", &list));
        }
        body.extend(chunk(b"data", &[0; 160]));
        chunk(b"RIFF", &body)
    }

    fn write(dir: &Path, name: &str, info: &[(&str, &str)]) {
        fs::write(dir.join(name), wav(info)).unwrap();
    }

    fn track(album: Option<&str>, number: Option<u32>, title: &str) -> Track {
        Track {
            path: PathBuf::from(format!("{title}.mp3")),
            title: title.to_owned(),
            artist: None,
            album: album.map(str::to_owned),
            number,
        }
    }

    #[test]
    fn reads_title_artist_album_and_number() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "file.wav",
            &[
                ("INAM", "Song"),
                ("IART", "Band"),
                ("IPRD", "Record"),
                ("IPRT", "3/12"),
            ],
        );
        let tracks = scan(dir.path()).unwrap();
        assert_eq!(tracks.len(), 1);
        let t = &tracks[0];
        assert_eq!(t.title, "Song");
        assert_eq!(t.artist.as_deref(), Some("Band"));
        assert_eq!(t.album.as_deref(), Some("Record"));
        assert_eq!(t.number, Some(3));
    }

    #[test]
    fn missing_tags_fall_back_to_file_name() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "plain.wav", &[]);
        let tracks = scan(dir.path()).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "plain");
        assert_eq!(tracks[0].album, None);
        assert_eq!(tracks[0].album_label(), UNKNOWN_ALBUM);
        assert_eq!(tracks[0].number, None);
    }

    #[test]
    fn partial_tags_keep_other_fields_empty() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "x.wav",
            &[("IPRD", "Only Album"), ("IPRT", "abc")],
        );
        let t = &scan(dir.path()).unwrap()[0];
        assert_eq!(t.title, "x");
        assert_eq!(t.album.as_deref(), Some("Only Album"));
        assert_eq!(t.number, None);
    }

    #[test]
    fn corrupted_file_is_kept_with_file_name() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("broken.mp3"), b"not audio at all").unwrap();
        fs::write(dir.path().join("empty.flac"), b"").unwrap();
        let tracks = scan(dir.path()).unwrap();
        let titles: Vec<_> = tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["broken", "empty"]);
        assert!(tracks.iter().all(|t| t.album.is_none()));
    }

    #[test]
    fn tracks_are_grouped_by_album_and_ordered_by_number() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "a.wav",
            &[("IPRD", "Beta"), ("IPRT", "2"), ("INAM", "B2")],
        );
        write(
            dir.path(),
            "b.wav",
            &[("IPRD", "Alpha"), ("IPRT", "10"), ("INAM", "A10")],
        );
        write(
            dir.path(),
            "c.wav",
            &[("IPRD", "Beta"), ("IPRT", "1"), ("INAM", "B1")],
        );
        write(
            dir.path(),
            "d.wav",
            &[("IPRD", "Alpha"), ("IPRT", "2"), ("INAM", "A2")],
        );
        write(dir.path(), "e.wav", &[]);
        let titles: Vec<_> = scan(dir.path())
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(titles, ["A2", "A10", "B1", "B2", "e"]);
    }

    #[test]
    fn ordering_puts_unnumbered_and_unknown_album_last() {
        let mut tracks = [
            track(None, Some(1), "loose"),
            track(Some("Album"), None, "zeta"),
            track(Some("Album"), None, "alpha"),
            track(Some("Album"), Some(5), "five"),
            track(Some("album"), Some(1), "one"),
        ];
        tracks.sort_by(compare);
        let titles: Vec<_> = tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["one", "five", "alpha", "zeta", "loose"]);
    }

    #[test]
    fn finds_supported_files_sorted() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["b.mp3", "a.WAV", "c.flac", "d.ogg"] {
            File::create(dir.path().join(name)).unwrap();
        }
        let titles: Vec<_> = scan(dir.path())
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(titles, ["a", "b", "c", "d"]);
    }

    #[test]
    fn ignores_unsupported_files() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["notes.txt", "cover.jpg", "noext", "song.mp3.bak"] {
            File::create(dir.path().join(name)).unwrap();
        }
        assert!(scan(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn empty_directory_yields_no_tracks() {
        let dir = tempfile::tempdir().unwrap();
        assert!(scan(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn invalid_directory_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(scan(&dir.path().join("missing")).is_err());
    }

    #[test]
    fn file_path_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.mp3");
        File::create(&file).unwrap();
        assert!(scan(&file).is_err());
    }

    #[test]
    fn does_not_recurse_and_skips_directories() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("album.mp3");
        fs::create_dir(&sub).unwrap();
        File::create(sub.join("inner.mp3")).unwrap();
        File::create(dir.path().join("top.mp3")).unwrap();
        let tracks = scan(dir.path()).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "top");
    }
}
