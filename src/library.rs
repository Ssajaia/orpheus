//! Music directory scanning.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// File extensions (lowercase) the player accepts.
pub const EXTENSIONS: [&str; 4] = ["mp3", "wav", "flac", "ogg"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub path: PathBuf,
    pub name: String,
}

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|s| e.eq_ignore_ascii_case(s)))
}

/// Scans `dir` (non-recursively) for supported audio files, sorted by name.
/// Entries that cannot be read are skipped.
pub fn scan(dir: &Path) -> io::Result<Vec<Track>> {
    let mut tracks: Vec<Track> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .map(|entry| entry.path())
        .filter(|path| is_supported(path))
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().into_owned();
            Some(Track { path, name })
        })
        .collect();
    tracks.sort_by_cached_key(|t| t.name.to_lowercase());
    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn finds_supported_files_sorted() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["b.mp3", "a.WAV", "c.flac", "d.ogg"] {
            File::create(dir.path().join(name)).unwrap();
        }
        let names: Vec<_> = scan(dir.path())
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(names, ["a", "b", "c", "d"]);
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
        assert_eq!(tracks[0].name, "top");
    }
}
