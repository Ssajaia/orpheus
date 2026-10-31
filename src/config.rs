//! Configuration loading and saving.

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub music_dir: Option<PathBuf>,
    pub volume: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            music_dir: None,
            volume: 0.8,
        }
    }
}

impl Config {
    /// Loads the config from `path`. A missing, unreadable or corrupted file
    /// yields the default configuration.
    pub fn load_from(path: &Path) -> Self {
        let mut config: Self = fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default();
        config.volume = if config.volume.is_finite() {
            config.volume.clamp(0.0, 1.0)
        } else {
            Self::default().volume
        };
        config
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(path, text)
    }
}

/// Location of the config file in the OS configuration directory.
pub fn default_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "orpheus").map(|d| d.config_dir().join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        let config = Config {
            music_dir: Some(PathBuf::from("music")),
            volume: 0.25,
        };
        config.save_to(&path).unwrap();
        assert_eq!(Config::load_from(&path), config);
    }

    #[test]
    fn serializes_to_toml() {
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(text.contains("volume"));
    }

    #[test]
    fn deserializes_partial_input_with_defaults() {
        let config: Config = toml::from_str("volume = 0.5").unwrap();
        assert_eq!(config.volume, 0.5);
        assert_eq!(config.music_dir, None);
    }

    #[test]
    fn missing_file_gives_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            Config::load_from(&dir.path().join("nope.toml")),
            Config::default()
        );
    }

    #[test]
    fn corrupted_file_gives_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "volume = [[[ not toml").unwrap();
        assert_eq!(Config::load_from(&path), Config::default());
    }

    #[test]
    fn out_of_range_volume_is_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "volume = 7.0").unwrap();
        assert_eq!(Config::load_from(&path).volume, 1.0);
    }
}
