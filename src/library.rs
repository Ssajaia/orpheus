//! Music directory scanning and basic tag reading.

use std::{
    cmp::Ordering,
    fs::{self, File},
    io,
    ops::Range,
    path::{Path, PathBuf},
};
use symphonia::core::{
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::{MetadataOptions, MetadataRevision, StandardTagKey},
    probe::{Hint, ProbeResult},
};

/// File extensions (lowercase) the player accepts.
pub const EXTENSIONS: [&str; 4] = ["mp3", "wav", "flac", "ogg"];

/// Label used for tracks that have no album tag.
pub const UNKNOWN_ALBUM: &str = "Unknown Album";

/// Artist shown for an album whose tracks have different artists.
pub const VARIOUS_ARTISTS: &str = "Various Artists";

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

/// Opens `path` and probes its container. `None` for unreadable or
/// unrecognised files.
fn probe(path: &Path) -> Option<ProbeResult> {
    let file = File::open(path).ok()?;
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()
}

/// Latest metadata revisions of a probed file: first the container's own
/// (FLAC, Ogg, WAV), then those found before it (ID3).
fn revisions(probed: &mut ProbeResult) -> [Option<MetadataRevision>; 2] {
    let container = probed.format.metadata().skip_to_latest().cloned();
    let leading = probed
        .metadata
        .get()
        .as_mut()
        .and_then(|m| m.skip_to_latest().cloned());
    [container, leading]
}

/// Reads tags from `path`. Any failure (unreadable, unknown or corrupted
/// file) yields empty tags.
fn read_tags(path: &Path) -> Tags {
    let mut tags = Tags::default();
    if let Some(mut probed) = probe(path) {
        for rev in revisions(&mut probed).iter().flatten() {
            tags.merge(rev);
        }
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

/// A run of consecutive tracks (in the sorted list from [`scan`]) that share an album.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Album {
    pub title: String,
    pub artist: Option<String>,
    /// Indices into the track list.
    pub tracks: Range<usize>,
}

/// Groups a list sorted by [`scan`] into albums. Tracks without an album
/// form one "Unknown Album" group.
pub fn albums(tracks: &[Track]) -> Vec<Album> {
    let key = |t: &Track| t.album.as_ref().map(|a| a.to_lowercase());
    let mut albums: Vec<Album> = Vec::new();
    let mut start = 0;
    while start < tracks.len() {
        let group_key = key(&tracks[start]);
        let len = tracks[start..]
            .iter()
            .take_while(|t| key(t) == group_key)
            .count();
        let group = &tracks[start..start + len];

        let mut artists = group.iter().filter_map(|t| t.artist.as_deref());
        let artist = artists.next().map(|first| {
            if artists.all(|a| a.eq_ignore_ascii_case(first)) {
                first.to_owned()
            } else {
                VARIOUS_ARTISTS.to_owned()
            }
        });
        albums.push(Album {
            title: group[0].album_label().to_owned(),
            artist,
            tracks: start..start + len,
        });
        start += len;
    }
    albums
}

/// Index of the album that contains track `track`.
pub fn album_of(albums: &[Album], track: usize) -> Option<usize> {
    albums.iter().position(|a| a.tracks.contains(&track))
}

/// Image file names looked up next to the tracks of a tagged album.
const COVER_FILES: [&str; 4] = ["cover.jpg", "cover.png", "folder.jpg", "folder.png"];

/// Raw (still encoded) cover image for an album: embedded artwork of the
/// first track that has some, otherwise, if `folder_cover` is set, a cover
/// file in the album's folder. Callers set it only when the folder holds a
/// single album, since all albums of a shared folder would get the same image.
pub fn artwork(tracks: &[Track], folder_cover: bool) -> Option<Vec<u8>> {
    for track in tracks {
        let Some(mut probed) = probe(&track.path) else {
            continue;
        };
        for rev in revisions(&mut probed).iter().flatten() {
            if let Some(visual) = rev.visuals().iter().find(|v| !v.data.is_empty()) {
                return Some(visual.data.to_vec());
            }
        }
    }
    // Loose, untagged files do not belong to one album, so no folder cover.
    let first = tracks
        .first()
        .filter(|t| folder_cover && t.album.is_some())?;
    let dir = first.path.parent()?;
    COVER_FILES
        .iter()
        .find_map(|name| fs::read(dir.join(name)).ok())
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

    /// Builds a FLAC file with only metadata blocks (no audio frames).
    fn flac(comments: &[&str], picture: Option<&[u8]>) -> Vec<u8> {
        fn block(kind: u8, last: bool, body: &[u8]) -> Vec<u8> {
            let mut out = vec![kind | if last { 0x80 } else { 0 }];
            out.extend(&(body.len() as u32).to_be_bytes()[1..]);
            out.extend(body);
            out
        }
        let mut info = Vec::new();
        info.extend(4096u16.to_be_bytes());
        info.extend(4096u16.to_be_bytes());
        info.extend([0; 6]); // frame sizes unknown
        info.extend([0x0A, 0xC4, 0x42, 0xF0, 0, 0, 0, 0]); // 44.1 kHz, stereo, 16 bit
        info.extend([0; 16]); // MD5

        let mut vorbis = Vec::new();
        vorbis.extend(0u32.to_le_bytes()); // empty vendor string
        vorbis.extend((comments.len() as u32).to_le_bytes());
        for c in comments {
            vorbis.extend((c.len() as u32).to_le_bytes());
            vorbis.extend(c.as_bytes());
        }

        let mut out = b"fLaC".to_vec();
        out.extend(block(0, false, &info));
        out.extend(block(4, picture.is_none(), &vorbis));
        if let Some(data) = picture {
            let mut pic = Vec::new();
            pic.extend(3u32.to_be_bytes()); // front cover
            pic.extend(9u32.to_be_bytes());
            pic.extend(b"image/png");
            pic.extend(0u32.to_be_bytes()); // description
            pic.extend([0; 16]); // width, height, depth, colours
            pic.extend((data.len() as u32).to_be_bytes());
            pic.extend(data);
            out.extend(block(6, true, &pic));
        }
        // The reader needs one frame header (with CRC-8) to find the audio start.
        out.extend([0xFF, 0xF8, 0xC9, 0x18, 0x00, 0xC2]);
        out.extend([0; 64]);
        out
    }

    #[test]
    fn reads_flac_tags() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = flac(
            &["TITLE=Song", "ARTIST=Band", "ALBUM=Record", "TRACKNUMBER=4"],
            None,
        );
        fs::write(dir.path().join("a.flac"), bytes).unwrap();
        let t = &scan(dir.path()).unwrap()[0];
        assert_eq!(t.title, "Song");
        assert_eq!(t.artist.as_deref(), Some("Band"));
        assert_eq!(t.album.as_deref(), Some("Record"));
        assert_eq!(t.number, Some(4));
    }

    #[test]
    fn albums_group_consecutive_tracks_and_keep_order() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "a.wav",
            &[
                ("IPRD", "Beta"),
                ("IPRT", "2"),
                ("INAM", "B2"),
                ("IART", "Y"),
            ],
        );
        write(
            dir.path(),
            "b.wav",
            &[
                ("IPRD", "Alpha"),
                ("IPRT", "2"),
                ("INAM", "A2"),
                ("IART", "X"),
            ],
        );
        write(
            dir.path(),
            "c.wav",
            &[
                ("IPRD", "Beta"),
                ("IPRT", "1"),
                ("INAM", "B1"),
                ("IART", "Y"),
            ],
        );
        write(
            dir.path(),
            "d.wav",
            &[
                ("IPRD", "alpha"),
                ("IPRT", "1"),
                ("INAM", "A1"),
                ("IART", "Z"),
            ],
        );
        write(dir.path(), "e.wav", &[]);
        let tracks = scan(dir.path()).unwrap();
        let found = albums(&tracks);

        let summary: Vec<_> = found
            .iter()
            .map(|a| (a.title.as_str(), a.artist.as_deref(), a.tracks.clone()))
            .collect();
        assert_eq!(
            summary,
            [
                ("alpha", Some(VARIOUS_ARTISTS), 0..2),
                ("Beta", Some("Y"), 2..4),
                (UNKNOWN_ALBUM, None, 4..5),
            ]
        );
        let titles = |a: &Album| {
            tracks[a.tracks.clone()]
                .iter()
                .map(|t| t.title.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(titles(&found[0]), ["A1", "A2"]);
        assert_eq!(titles(&found[1]), ["B1", "B2"]);
        assert_eq!(titles(&found[2]), ["e"]);
    }

    #[test]
    fn no_tracks_no_albums() {
        assert!(albums(&[]).is_empty());
    }

    #[test]
    fn finds_album_of_track() {
        let tracks = [
            track(Some("A"), Some(1), "a1"),
            track(Some("A"), Some(2), "a2"),
            track(None, None, "x"),
        ];
        let found = albums(&tracks);
        assert_eq!(album_of(&found, 0), Some(0));
        assert_eq!(album_of(&found, 1), Some(0));
        assert_eq!(album_of(&found, 2), Some(1));
        assert_eq!(album_of(&found, 3), None);
    }

    #[test]
    fn artwork_comes_from_embedded_picture() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = flac(&["ALBUM=Record"], Some(b"PNGDATA"));
        fs::write(dir.path().join("a.flac"), bytes).unwrap();
        let tracks = scan(dir.path()).unwrap();
        assert_eq!(artwork(&tracks, false).as_deref(), Some(&b"PNGDATA"[..]));
    }

    #[test]
    fn artwork_falls_back_to_cover_file() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.wav", &[("IPRD", "Record")]);
        fs::write(dir.path().join("folder.png"), b"FILEDATA").unwrap();
        let tracks = scan(dir.path()).unwrap();
        assert_eq!(artwork(&tracks, true).as_deref(), Some(&b"FILEDATA"[..]));
        assert_eq!(artwork(&tracks, false), None);
    }

    #[test]
    fn missing_artwork_is_none() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.wav", &[("IPRD", "Record")]);
        fs::write(dir.path().join("b.mp3"), b"broken").unwrap();
        let tracks = scan(dir.path()).unwrap();
        assert_eq!(artwork(&tracks, true), None);
    }

    #[test]
    fn untagged_files_do_not_use_folder_cover() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.wav", &[]);
        fs::write(dir.path().join("cover.jpg"), b"FILEDATA").unwrap();
        let tracks = scan(dir.path()).unwrap();
        assert_eq!(artwork(&tracks, true), None);
    }
}
