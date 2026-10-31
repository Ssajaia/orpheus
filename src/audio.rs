//! Audio playback management.

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use std::{fs::File, io::BufReader, path::Path, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Stopped,
    Playing,
    Paused,
}

/// Owns the output device and the single active player.
///
/// The device is opened when playback starts and released when it stops.
pub struct Audio {
    sink: Option<MixerDeviceSink>,
    player: Option<Player>,
    state: State,
    duration: Option<Duration>,
    volume: f32,
}

impl Audio {
    pub fn new(volume: f32) -> Self {
        Self {
            sink: None,
            player: None,
            state: State::Stopped,
            duration: None,
            volume: volume.clamp(0.0, 1.0),
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Total length of the current track, if the decoder can tell.
    pub fn duration(&self) -> Option<Duration> {
        self.duration
    }

    pub fn position(&self) -> Duration {
        self.player.as_ref().map_or(Duration::ZERO, Player::get_pos)
    }

    /// Starts playing `path`, replacing whatever was playing before.
    /// On failure playback is left stopped.
    pub fn play(&mut self, path: &Path) -> Result<(), String> {
        self.stop();

        let file = File::open(path).map_err(|e| e.to_string())?;
        let decoder = Decoder::new(BufReader::new(file)).map_err(|e| e.to_string())?;
        let duration = decoder.total_duration();

        let mut sink = DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
        sink.log_on_drop(false);
        let player = Player::connect_new(sink.mixer());
        player.set_volume(self.volume);
        player.append(decoder);

        self.sink = Some(sink);
        self.player = Some(player);
        self.duration = duration;
        self.state = State::Playing;
        Ok(())
    }

    pub fn pause(&mut self) {
        if let (State::Playing, Some(player)) = (self.state, &self.player) {
            player.pause();
            self.state = State::Paused;
        }
    }

    pub fn resume(&mut self) {
        if let (State::Paused, Some(player)) = (self.state, &self.player) {
            player.play();
            self.state = State::Playing;
        }
    }

    /// Stops playback and releases the player and the output device.
    pub fn stop(&mut self) {
        self.player = None;
        self.sink = None;
        self.duration = None;
        self.state = State::Stopped;
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(player) = &self.player {
            player.set_volume(self.volume);
        }
    }

    /// True once the current track has played to its end.
    pub fn finished(&self) -> bool {
        self.state == State::Playing && self.player.as_ref().is_some_and(Player::empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_stopped() {
        let audio = Audio::new(0.5);
        assert_eq!(audio.state(), State::Stopped);
        assert_eq!(audio.position(), Duration::ZERO);
        assert_eq!(audio.duration(), None);
        assert!(!audio.finished());
    }

    #[test]
    fn pause_and_resume_do_nothing_when_stopped() {
        let mut audio = Audio::new(0.5);
        audio.pause();
        assert_eq!(audio.state(), State::Stopped);
        audio.resume();
        assert_eq!(audio.state(), State::Stopped);
    }

    #[test]
    fn stop_is_idempotent() {
        let mut audio = Audio::new(0.5);
        audio.stop();
        audio.stop();
        assert_eq!(audio.state(), State::Stopped);
    }

    #[test]
    fn volume_is_clamped() {
        let mut audio = Audio::new(3.0);
        assert_eq!(audio.volume(), 1.0);
        audio.set_volume(-1.0);
        assert_eq!(audio.volume(), 0.0);
        audio.set_volume(0.3);
        assert_eq!(audio.volume(), 0.3);
    }

    #[test]
    fn missing_file_fails_and_stays_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let mut audio = Audio::new(0.5);
        assert!(audio.play(&dir.path().join("missing.mp3")).is_err());
        assert_eq!(audio.state(), State::Stopped);
    }

    #[test]
    fn corrupted_file_fails_and_stays_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.mp3");
        std::fs::write(&path, b"this is not audio").unwrap();
        let mut audio = Audio::new(0.5);
        assert!(audio.play(&path).is_err());
        assert_eq!(audio.state(), State::Stopped);
    }
}
