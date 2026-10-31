//! Application state and GUI.

use crate::{
    audio::{Audio, State},
    config::{self, Config},
    library::{self, Track},
};
use eframe::egui;
use std::{path::PathBuf, time::Duration};

pub struct App {
    config: Config,
    config_path: Option<PathBuf>,
    dir_input: String,
    tracks: Vec<Track>,
    current: Option<usize>,
    audio: Audio,
    message: Option<String>,
}

impl App {
    pub fn new(ctx: &egui::Context) -> Self {
        ctx.set_visuals(egui::Visuals::dark());

        let config_path = config::default_path();
        let config = config_path
            .as_deref()
            .map(Config::load_from)
            .unwrap_or_default();
        let mut app = Self {
            audio: Audio::new(config.volume),
            dir_input: config
                .music_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            config,
            config_path,
            tracks: Vec::new(),
            current: None,
            message: None,
        };
        if let Some(dir) = app.config.music_dir.clone() {
            app.load_dir(dir);
        }
        app
    }

    fn load_dir(&mut self, dir: PathBuf) {
        self.audio.stop();
        self.current = None;
        match library::scan(&dir) {
            Ok(tracks) => {
                self.message = tracks
                    .is_empty()
                    .then(|| "No supported audio files found".into());
                self.tracks = tracks;
                self.config.music_dir = Some(dir);
                self.save_config();
            }
            Err(e) => {
                self.tracks.clear();
                self.message = Some(format!("Cannot read directory: {e}"));
            }
        }
    }

    fn save_config(&mut self) {
        self.config.volume = self.audio.volume();
        if let Some(path) = &self.config_path
            && let Err(e) = self.config.save_to(path)
        {
            self.message = Some(format!("Cannot save settings: {e}"));
        }
    }

    fn play_index(&mut self, index: usize) {
        let Some(track) = self.tracks.get(index) else {
            return;
        };
        self.current = Some(index);
        match self.audio.play(&track.path) {
            Ok(()) => self.message = None,
            Err(e) => self.message = Some(format!("Cannot play {}: {e}", track.name)),
        }
    }

    fn next(&mut self) {
        if self.tracks.is_empty() {
            return;
        }
        let index = self.current.map_or(0, |i| (i + 1) % self.tracks.len());
        self.play_index(index);
    }

    fn previous(&mut self) {
        if self.tracks.is_empty() {
            return;
        }
        let last = self.tracks.len() - 1;
        let index = self
            .current
            .map_or(last, |i| i.checked_sub(1).unwrap_or(last));
        self.play_index(index);
    }

    fn toggle(&mut self) {
        match self.audio.state() {
            State::Playing => self.audio.pause(),
            State::Paused => self.audio.resume(),
            State::Stopped => self.play_index(self.current.unwrap_or(0)),
        }
    }

    /// Advances to the next track when the current one ends; stops after the last.
    fn handle_track_end(&mut self) {
        if !self.audio.finished() {
            return;
        }
        match self.current {
            Some(i) if i + 1 < self.tracks.len() => self.play_index(i + 1),
            _ => self.audio.stop(),
        }
    }
}

fn format_time(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_track_end();
        // Repaint periodically only while audio is actually playing.
        if self.audio.state() == State::Playing {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("top").show(ui, |ui| {
            ui.heading("Orpheus");
            ui.horizontal(|ui| {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.dir_input)
                        .hint_text("Music directory")
                        .desired_width(ui.available_width() - 60.0),
                );
                let submitted = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Load").clicked() || submitted {
                    self.load_dir(PathBuf::from(self.dir_input.trim()));
                }
            });
            if let Some(message) = &self.message {
                ui.colored_label(egui::Color32::LIGHT_RED, message);
            }
        });

        egui::Panel::bottom("player").show(ui, |ui| {
            let title = self
                .current
                .and_then(|i| self.tracks.get(i))
                .map_or("Nothing playing", |t| t.name.as_str());
            ui.label(egui::RichText::new(title).strong());

            let position = self.audio.position();
            let (fraction, total) = match self.audio.duration() {
                Some(d) if !d.is_zero() => (
                    (position.as_secs_f32() / d.as_secs_f32()).clamp(0.0, 1.0),
                    format_time(d),
                ),
                _ => (0.0, "--:--".to_owned()),
            };
            ui.add(
                egui::ProgressBar::new(fraction)
                    .text(format!("{} / {total}", format_time(position))),
            );

            ui.horizontal(|ui| {
                if ui.button("Prev").clicked() {
                    self.previous();
                }
                let label = match self.audio.state() {
                    State::Stopped => "Play",
                    State::Playing => "Pause",
                    State::Paused => "Resume",
                };
                if ui.button(label).clicked() {
                    self.toggle();
                }
                if ui.button("Stop").clicked() {
                    self.audio.stop();
                }
                if ui.button("Next").clicked() {
                    self.next();
                }
                let mut volume = self.audio.volume();
                ui.label("Vol");
                if ui
                    .add(egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false))
                    .changed()
                {
                    self.audio.set_volume(volume);
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let mut clicked = None;
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    for (i, track) in self.tracks.iter().enumerate() {
                        if ui
                            .selectable_label(self.current == Some(i), &track.name)
                            .clicked()
                        {
                            clicked = Some(i);
                        }
                    }
                });
            if let Some(i) = clicked {
                self.play_index(i);
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.audio.stop();
        self.save_config();
    }
}
