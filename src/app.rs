//! Application state and GUI.

use crate::{
    audio::{Audio, State},
    config::{self, Config},
    icons::{self, Icon},
    library::{self, Album, Track},
    vinyl::{self, Turntable},
};
use eframe::egui;
use std::{
    ops::Range,
    path::{Path, PathBuf},
    time::Duration,
};

/// Edge length (pixels) of the stored cover thumbnails.
const COVER_PX: u32 = 160;
/// Width of the library panel in a wide window.
const LIBRARY_WIDTH: f32 = 360.0;
/// Windows narrower than this give the library panel their whole width.
const WIDE_WINDOW: f32 = 700.0;
/// Seconds the library panel takes to slide open or closed.
const LIBRARY_SLIDE_SECONDS: f32 = 0.25;
/// Narrowest an album card in the grid gets (points).
const CARD_MIN: f32 = 130.0;
const LIBRARY_BACKGROUND: egui::Color32 = egui::Color32::from_rgb(26, 22, 18);
/// Size (points) of the cover in the album view.
const VIEW_COVER: f32 = 96.0;
/// Delay between frames while the record or tonearm is moving (about 60 fps).
const ANIMATION_FRAME: Duration = Duration::from_millis(16);
/// Height reserved below the record for the track title and artist.
const RECORD_TEXT_HEIGHT: f32 = 64.0;
/// Covers decoded per frame at most, so a big grid does not stall the UI.
const COVERS_PER_FRAME: u8 = 2;

/// Cover state of one album: not tried yet, none available, or a ready texture.
#[derive(Clone)]
enum Cover {
    Pending,
    Missing,
    Ready(egui::TextureHandle),
}

pub struct App {
    config: Config,
    config_path: Option<PathBuf>,
    /// Text of the music-folder field while it is being edited.
    dir_input: String,
    /// The music-folder field is open for editing.
    editing_dir: bool,
    /// Give the music-folder field keyboard focus on the next frame.
    focus_dir: bool,
    tracks: Vec<Track>,
    albums: Vec<Album>,
    covers: Vec<Cover>,
    cover_budget: u8,
    /// Whether a cover file in the music folder may serve as album artwork.
    folder_cover: bool,
    /// Album whose tracks are shown in the library; `None` shows the album grid.
    selected: Option<usize>,
    /// The library panel is open (or sliding open).
    library_open: bool,
    turntable: Turntable,
    current: Option<usize>,
    audio: Audio,
    message: Option<String>,
}

impl App {
    pub fn new(ctx: &egui::Context) -> Self {
        Self::with_config(ctx, config::default_path())
    }

    fn with_config(ctx: &egui::Context, config_path: Option<PathBuf>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = vinyl::BACKGROUND;
        ctx.set_visuals(visuals);

        let config = config_path
            .as_deref()
            .map(Config::load_from)
            .unwrap_or_default();
        let mut app = Self {
            audio: Audio::new(config.volume),
            dir_input: path_text(config.music_dir.as_deref()),
            // Without a remembered folder the field is open right away.
            editing_dir: config.music_dir.is_none(),
            focus_dir: false,
            library_open: config.music_dir.is_none(),
            config,
            config_path,
            tracks: Vec::new(),
            albums: Vec::new(),
            covers: Vec::new(),
            cover_budget: COVERS_PER_FRAME,
            folder_cover: false,
            selected: None,
            turntable: Turntable::default(),
            current: None,
            message: None,
        };
        if let Some(dir) = app.config.music_dir.clone() {
            app.load_dir(dir);
        }
        app
    }

    /// Scans `dir` and makes it the music folder. On failure the folder field
    /// stays open so the path can be corrected.
    fn load_dir(&mut self, dir: PathBuf) -> bool {
        self.audio.stop();
        self.current = None;
        self.selected = None;
        match library::scan(&dir) {
            Ok(tracks) => {
                self.message = tracks
                    .is_empty()
                    .then(|| "No supported audio files found".into());
                self.set_tracks(tracks);
                self.dir_input = path_text(Some(&dir));
                self.config.music_dir = Some(dir);
                self.editing_dir = false;
                self.save_config();
                true
            }
            Err(e) => {
                self.set_tracks(Vec::new());
                self.message = Some(format!("Cannot read directory: {e}"));
                self.editing_dir = true;
                self.library_open = true;
                false
            }
        }
    }

    fn start_editing_dir(&mut self) {
        self.editing_dir = true;
        self.focus_dir = true;
    }

    /// Closes the folder field, discarding what was typed.
    fn cancel_editing_dir(&mut self) {
        self.editing_dir = false;
        self.dir_input = path_text(self.config.music_dir.as_deref());
    }

    fn set_tracks(&mut self, tracks: Vec<Track>) {
        self.albums = library::albums(&tracks);
        self.covers = vec![Cover::Pending; self.albums.len()];
        let tagged = self
            .albums
            .iter()
            .filter(|a| tracks[a.tracks.start].album.is_some())
            .count();
        self.folder_cover = tagged == 1;
        self.tracks = tracks;
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
            Err(e) => self.message = Some(format!("Cannot play {}: {e}", track.title)),
        }
    }

    /// Tracks that next/previous and automatic advance move within: the album
    /// of the current track, else the selected album, else everything.
    fn scope(&self) -> Range<usize> {
        self.current
            .and_then(|i| library::album_of(&self.albums, i))
            .or(self.selected)
            .and_then(|a| self.albums.get(a))
            .map_or(0..self.tracks.len(), |a| a.tracks.clone())
    }

    fn next(&mut self) {
        let scope = self.scope();
        if let Some(index) = next_index(&scope, self.current) {
            self.play_index(index);
        }
    }

    fn previous(&mut self) {
        let scope = self.scope();
        if let Some(index) = previous_index(&scope, self.current) {
            self.play_index(index);
        }
    }

    fn apply_needle(&mut self, action: NeedleAction) {
        match action {
            NeedleAction::Keep => {}
            NeedleAction::Pause => self.audio.pause(),
            NeedleAction::Resume => self.audio.resume(),
            NeedleAction::Restart => {
                let index = self.current.unwrap_or(self.scope().start);
                self.play_index(index);
            }
        }
    }

    /// Advances to the next track of the album when the current one ends;
    /// stops after the album's last track.
    fn handle_track_end(&mut self) {
        if !self.audio.finished() {
            return;
        }
        let scope = self.scope();
        match self.current {
            Some(i) if i + 1 < scope.end => self.play_index(i + 1),
            _ => self.audio.stop(),
        }
    }

    /// Thumbnail of album `index`, decoding it on first use (a few per frame).
    fn cover(&mut self, ctx: &egui::Context, index: usize) -> Option<egui::TextureHandle> {
        match self.covers.get(index)? {
            Cover::Ready(texture) => return Some(texture.clone()),
            Cover::Missing => return None,
            Cover::Pending => {}
        }
        if self.cover_budget == 0 {
            ctx.request_repaint();
            return None;
        }
        self.cover_budget -= 1;
        let texture = load_cover(
            ctx,
            &self.tracks[self.albums[index].tracks.clone()],
            self.folder_cover,
        );
        self.covers[index] = match &texture {
            Some(t) => Cover::Ready(t.clone()),
            None => Cover::Missing,
        };
        texture
    }

    fn show_cover(
        &mut self,
        ui: &mut egui::Ui,
        album: usize,
        size: f32,
        playing: bool,
    ) -> egui::Response {
        let texture = self.cover(ui.ctx(), album);
        let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
        match texture {
            Some(texture) => {
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter()
                    .image(texture.id(), rect, uv, egui::Color32::WHITE);
            }
            None => {
                ui.painter()
                    .rect_filled(rect, 4.0, ui.visuals().widgets.inactive.bg_fill);
            }
        }
        // A gold frame marks the album that is playing, a lighter one the hovered album.
        let stroke = if playing {
            Some(egui::Stroke::new(2.0, vinyl::BRASS))
        } else if response.hovered() {
            Some(ui.visuals().widgets.hovered.fg_stroke)
        } else {
            None
        };
        if let Some(stroke) = stroke {
            ui.painter()
                .rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Inside);
        }
        response
    }

    /// Grid of album covers with title and artist. Only the visible rows are drawn.
    fn album_grid(&mut self, ui: &mut egui::Ui) {
        if self.albums.is_empty() {
            ui.label(egui::RichText::new("No albums yet").weak());
            return;
        }
        let gap = ui.spacing().item_spacing.x;
        let width = ui.available_width() - 6.0; // room for the scroll bar
        let columns = grid_columns(width, gap);
        let size = card_size(width, columns, gap);
        let row_height =
            size + 2.0 * ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
        let rows = self.albums.len().div_ceil(columns);
        let playing = self
            .current
            .and_then(|i| library::album_of(&self.albums, i));

        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("albums")
            .auto_shrink(false)
            .show_rows(ui, row_height, rows, |ui, range| {
                for row in range {
                    ui.horizontal(|ui| {
                        let end = ((row + 1) * columns).min(self.albums.len());
                        for i in row * columns..end {
                            ui.vertical(|ui| {
                                ui.set_width(size);
                                if self.show_cover(ui, i, size, playing == Some(i)).clicked() {
                                    clicked = Some(i);
                                }
                                let album = &self.albums[i];
                                ui.add(
                                    egui::Label::new(egui::RichText::new(&album.title).strong())
                                        .truncate(),
                                );
                                // A non-breaking space keeps rows the same height without an artist.
                                let artist = album.artist.as_deref().unwrap_or("\u{a0}");
                                ui.add(
                                    egui::Label::new(egui::RichText::new(artist).weak().small())
                                        .truncate(),
                                );
                            });
                        }
                    });
                }
            });
        if clicked.is_some() {
            self.selected = clicked;
        }
    }

    /// One album: cover, title, artist, a play button and its tracks in order.
    fn album_view(&mut self, ui: &mut egui::Ui, selected: usize) {
        let Some(album) = self.albums.get(selected).cloned() else {
            self.selected = None;
            return;
        };
        if ui.button("Back").clicked() {
            self.selected = None;
        }
        ui.horizontal(|ui| {
            self.show_cover(ui, selected, VIEW_COVER, false);
            ui.vertical(|ui| {
                ui.add(egui::Label::new(egui::RichText::new(&album.title).heading()).truncate());
                if let Some(artist) = &album.artist {
                    ui.add(egui::Label::new(artist).truncate());
                }
                ui.label(format!("{} tracks", album.tracks.len()));
                if ui.button("Play album").clicked() {
                    self.play_index(album.tracks.start);
                }
            });
        });
        ui.separator();

        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("tracks")
            .auto_shrink(false)
            .show(ui, |ui| {
                for i in album.tracks.clone() {
                    let track = &self.tracks[i];
                    let label = match track.number {
                        Some(n) => format!("{n}. {}", track.title),
                        None => track.title.clone(),
                    };
                    if ui
                        .selectable_label(self.current == Some(i), label)
                        .clicked()
                    {
                        clicked = Some(i);
                    }
                }
            });
        if let Some(i) = clicked {
            self.play_index(i);
        }
    }

    /// Contents of the library panel: title, close button, music folder, then
    /// the album grid or the open album.
    fn library_ui(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Library");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::button(ui, Icon::Close, "Close").clicked() {
                            self.library_open = false;
                        }
                    });
                });
                self.library_bar(ui);
                if let Some(message) = &self.message {
                    ui.colored_label(egui::Color32::LIGHT_RED, message);
                }
                ui.separator();
                match self.selected {
                    Some(album) => self.album_view(ui, album),
                    None => self.album_grid(ui),
                }
            });
        // Escape closes the panel unless the folder field already used it.
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.library_open = false;
        }
    }

    /// The music-folder line under the title: the current path with a button
    /// to change it, or an editable path field.
    fn library_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if self.editing_dir {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.dir_input)
                        .hint_text("Path to your music folder")
                        .desired_width((ui.available_width() - 120.0).max(60.0)),
                );
                if std::mem::take(&mut self.focus_dir) {
                    edit.request_focus();
                }
                let entered = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Open").clicked() || entered {
                    self.load_dir(PathBuf::from(self.dir_input.trim()));
                } else if ui.button("Cancel").clicked()
                    || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                {
                    self.cancel_editing_dir();
                }
            } else {
                let (text, button) = match &self.config.music_dir {
                    Some(dir) => (path_text(Some(dir)), "Change"),
                    None => ("No music folder selected".to_owned(), "Choose"),
                };
                if ui.small_button(button).clicked() {
                    self.start_editing_dir();
                }
                ui.add(egui::Label::new(egui::RichText::new(&text).weak().small()).truncate())
                    .on_hover_text(text);
            }
        });
    }

    /// Lets the user hold the tonearm and move it along its arc. Lifting the
    /// needle off the record pauses at once; dropping it back on resumes.
    fn drag_needle(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        layout: &vinyl::Layout,
        state: State,
    ) {
        let origin = ui.input(|i| i.pointer.press_origin());
        if response.drag_started()
            && origin.is_some_and(|p| vinyl::arm_contains(layout, &self.turntable, p))
        {
            self.turntable.grab();
        }
        if !self.turntable.is_dragging() {
            if response
                .hover_pos()
                .is_some_and(|p| vinyl::arm_contains(layout, &self.turntable, p))
            {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            return;
        }
        if !response.dragged() {
            self.turntable.release();
            let action = needle_action(state, self.turntable.over_record());
            self.apply_needle(action);
            return;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some(pos) = response.interact_pointer_pos() {
            self.turntable.drag_to(vinyl::arm_angle_at(layout, pos));
        }
        let action = needle_action(state, self.turntable.over_record());
        if action == NeedleAction::Pause {
            self.apply_needle(action);
        }
    }

    /// The vinyl player: record with cover art, draggable tonearm,
    /// previous/next buttons, title and artist.
    fn record_ui(&mut self, ui: &mut egui::Ui) {
        let current = self.current.filter(|&i| i < self.tracks.len());
        let cover = current
            .and_then(|i| library::album_of(&self.albums, i))
            .and_then(|album| self.cover(ui.ctx(), album));

        let state = self.audio.state();
        let progress = progress(self.audio.position(), self.audio.duration());
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        self.turntable.update(dt, state, progress);
        if self.turntable.is_animating(state, progress) {
            ui.ctx().request_repaint_after(ANIMATION_FRAME);
        }

        let size = egui::vec2(
            ui.available_width(),
            (ui.available_height() - RECORD_TEXT_HEIGHT).max(0.0),
        );
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::drag());
        let layout = vinyl::layout(rect);
        self.drag_needle(ui, &response, &layout, state);
        vinyl::paint(
            ui.painter(),
            &layout,
            &self.turntable,
            cover.as_ref().map(egui::TextureHandle::id),
        );
        if vinyl::skip_button(ui, layout.prev, layout.button, Icon::Previous, "Previous") {
            self.previous();
        }
        if vinyl::skip_button(ui, layout.next, layout.button, Icon::Next, "Next") {
            self.next();
        }

        ui.vertical_centered(|ui| match current.map(|i| &self.tracks[i]) {
            Some(track) => {
                ui.add(
                    egui::Label::new(egui::RichText::new(&track.title).size(20.0).strong())
                        .truncate(),
                );
                let artist = track.artist.as_deref().unwrap_or("Unknown Artist");
                ui.add(egui::Label::new(egui::RichText::new(artist).weak()).truncate());
            }
            None => {
                ui.label(egui::RichText::new("Nothing playing").size(20.0).strong());
            }
        });
    }
}

/// What lifting or dropping the needle does to playback.
#[derive(Debug, PartialEq, Eq)]
enum NeedleAction {
    Keep,
    /// Needle left the record while playing.
    Pause,
    /// Needle returned to the record while paused; continues where it stopped.
    Resume,
    /// Needle dropped on the record with nothing playing: start the track.
    Restart,
}

fn needle_action(state: State, on_record: bool) -> NeedleAction {
    match (state, on_record) {
        (State::Playing, false) => NeedleAction::Pause,
        (State::Paused, true) => NeedleAction::Resume,
        (State::Stopped, true) => NeedleAction::Restart,
        _ => NeedleAction::Keep,
    }
}

/// Width of the library panel when fully open.
fn library_width(window: f32) -> f32 {
    if window >= WIDE_WINDOW {
        LIBRARY_WIDTH
    } else {
        window
    }
}

/// Number of album cards per row for a grid `width` wide.
fn grid_columns(width: f32, gap: f32) -> usize {
    (((width + gap) / (CARD_MIN + gap)).floor() as usize).max(1)
}

/// Edge length of an album card so `columns` of them fill `width`.
fn card_size(width: f32, columns: usize, gap: f32) -> f32 {
    ((width - gap * (columns - 1) as f32) / columns as f32).max(1.0)
}

/// A path as shown to the user; empty when there is none.
fn path_text(path: Option<&Path>) -> String {
    path.map(|p| p.display().to_string()).unwrap_or_default()
}

/// Fraction (0..1) of the track that has been played; 0 when the length is unknown.
fn progress(position: Duration, total: Option<Duration>) -> f32 {
    match total {
        Some(total) if !total.is_zero() => {
            (position.as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0)
        }
        _ => 0.0,
    }
}

/// Next track within `scope`, wrapping to its start. Starts at the scope's
/// first track when nothing is playing (or the current track is outside it).
fn next_index(scope: &Range<usize>, current: Option<usize>) -> Option<usize> {
    if scope.is_empty() {
        return None;
    }
    Some(match current.filter(|i| scope.contains(i)) {
        Some(i) if i + 1 < scope.end => i + 1,
        _ => scope.start,
    })
}

/// Previous track within `scope`, wrapping to its end.
fn previous_index(scope: &Range<usize>, current: Option<usize>) -> Option<usize> {
    if scope.is_empty() {
        return None;
    }
    Some(match current.filter(|i| scope.contains(i)) {
        Some(i) if i > scope.start => i - 1,
        _ => scope.end - 1,
    })
}

/// Decodes the album's artwork into a small square texture. `None` when the
/// album has no artwork or it cannot be decoded.
fn load_cover(
    ctx: &egui::Context,
    tracks: &[Track],
    folder_cover: bool,
) -> Option<egui::TextureHandle> {
    let bytes = library::artwork(tracks, folder_cover)?;
    let image = image::load_from_memory(&bytes)
        .ok()?
        .resize_to_fill(COVER_PX, COVER_PX, image::imageops::FilterType::Triangle)
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let pixels = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(ctx.load_texture("cover", pixels, egui::TextureOptions::LINEAR))
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.cover_budget = COVERS_PER_FRAME;
        self.handle_track_end();
        // Repaint periodically only while audio is actually playing.
        if self.audio.state() == State::Playing {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The library panel slides open to its full width; the player gets the rest.
        let width = library_width(ui.max_rect().width());
        let open = ui.ctx().animate_bool_with_time_and_easing(
            egui::Id::unique("library_open"),
            self.library_open,
            LIBRARY_SLIDE_SECONDS,
            egui::emath::easing::cubic_out,
        );
        if open > 0.0 {
            egui::Panel::left("library")
                .exact_size(width * open)
                .resizable(false)
                .frame(egui::Frame::new().fill(LIBRARY_BACKGROUND))
                .show(ui, |ui| {
                    // Lay the contents out at full width and let the panel reveal them.
                    let panel = ui.max_rect();
                    let full = egui::Rect::from_min_size(
                        egui::pos2(panel.right() - width, panel.top()),
                        egui::vec2(width, panel.height()),
                    );
                    let mut contents = ui.new_child(egui::UiBuilder::new().max_rect(full));
                    contents.set_clip_rect(panel);
                    self.library_ui(&mut contents);
                });
        }

        egui::Panel::top("top").show(ui, |ui| {
            ui.horizontal(|ui| {
                if icons::button(ui, Icon::Library, "Library").clicked() {
                    self.library_open = !self.library_open;
                }
                ui.heading("Orpheus");
            });
            // While the library is showing it displays messages itself.
            if open == 0.0
                && let Some(message) = &self.message
            {
                ui.colored_label(egui::Color32::LIGHT_RED, message);
            }
        });

        egui::Panel::bottom("volume").show(ui, |ui| {
            ui.horizontal(|ui| {
                icons::label(ui, Icon::Volume);
                ui.spacing_mut().slider_width = (ui.available_width() - 16.0).max(60.0);
                let mut volume = self.audio.volume();
                if ui
                    .add(egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false))
                    .changed()
                {
                    self.audio.set_volume(volume);
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            // A fully covered player (narrow window, library open) is not drawn.
            if ui.available_width() >= 120.0 {
                self.record_ui(ui);
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.audio.stop();
        self.save_config();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_moves_forward_and_wraps_within_scope() {
        let scope = 3..6;
        assert_eq!(next_index(&scope, Some(3)), Some(4));
        assert_eq!(next_index(&scope, Some(5)), Some(3));
    }

    #[test]
    fn previous_moves_back_and_wraps_within_scope() {
        let scope = 3..6;
        assert_eq!(previous_index(&scope, Some(5)), Some(4));
        assert_eq!(previous_index(&scope, Some(3)), Some(5));
    }

    #[test]
    fn nothing_playing_starts_at_scope_edges() {
        let scope = 3..6;
        assert_eq!(next_index(&scope, None), Some(3));
        assert_eq!(previous_index(&scope, None), Some(5));
    }

    #[test]
    fn current_outside_scope_is_ignored() {
        let scope = 3..6;
        assert_eq!(next_index(&scope, Some(9)), Some(3));
        assert_eq!(previous_index(&scope, Some(1)), Some(5));
    }

    #[test]
    fn needle_off_the_record_pauses_only_while_playing() {
        assert_eq!(needle_action(State::Playing, false), NeedleAction::Pause);
        assert_eq!(needle_action(State::Paused, false), NeedleAction::Keep);
        assert_eq!(needle_action(State::Stopped, false), NeedleAction::Keep);
    }

    #[test]
    fn needle_on_the_record_resumes_or_starts() {
        assert_eq!(needle_action(State::Paused, true), NeedleAction::Resume);
        assert_eq!(needle_action(State::Stopped, true), NeedleAction::Restart);
        assert_eq!(needle_action(State::Playing, true), NeedleAction::Keep);
    }

    fn app_with(config_path: &Path) -> App {
        App::with_config(&egui::Context::default(), Some(config_path.to_owned()))
    }

    #[test]
    fn without_a_saved_folder_the_field_starts_open() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(&dir.path().join("config.toml"));
        assert!(app.editing_dir);
        assert!(app.tracks.is_empty());
    }

    #[test]
    fn loading_a_folder_saves_it_and_closes_the_field() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let music = dir.path().join("music");
        std::fs::create_dir(&music).unwrap();
        std::fs::write(music.join("a.mp3"), b"").unwrap();

        let mut app = app_with(&config);
        assert!(app.load_dir(music.clone()));
        assert!(!app.editing_dir);
        assert_eq!(app.tracks.len(), 1);
        assert_eq!(app.dir_input, music.display().to_string());
        assert_eq!(Config::load_from(&config).music_dir, Some(music.clone()));

        // The saved folder is loaded on the next start and the field stays closed.
        let again = app_with(&config);
        assert!(!again.editing_dir);
        assert_eq!(again.tracks.len(), 1);
    }

    #[test]
    fn a_bad_folder_keeps_the_field_open_and_the_old_folder() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let music = dir.path().join("music");
        std::fs::create_dir(&music).unwrap();

        let mut app = app_with(&config);
        assert!(app.load_dir(music.clone()));
        app.start_editing_dir();
        assert!(!app.load_dir(dir.path().join("missing")));
        assert!(app.editing_dir);
        assert!(app.message.is_some());
        assert_eq!(app.config.music_dir, Some(music.clone()));
        assert_eq!(Config::load_from(&config).music_dir, Some(music));
    }

    #[test]
    fn cancelling_restores_the_current_path() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let mut app = app_with(&config);
        assert!(app.load_dir(dir.path().to_owned()));
        app.start_editing_dir();
        assert!(app.editing_dir && app.focus_dir);
        app.dir_input = "something else".into();
        app.cancel_editing_dir();
        assert!(!app.editing_dir);
        assert_eq!(app.dir_input, dir.path().display().to_string());
    }

    #[test]
    fn an_unreadable_saved_folder_opens_the_field() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        Config {
            music_dir: Some(dir.path().join("gone")),
            volume: 0.5,
        }
        .save_to(&config)
        .unwrap();
        let app = app_with(&config);
        assert!(app.editing_dir);
        assert!(app.message.is_some());
    }

    #[test]
    fn library_takes_a_fixed_width_in_wide_windows_and_all_of_a_narrow_one() {
        assert_eq!(library_width(1200.0), LIBRARY_WIDTH);
        assert_eq!(library_width(WIDE_WINDOW), LIBRARY_WIDTH);
        assert_eq!(library_width(WIDE_WINDOW - 1.0), WIDE_WINDOW - 1.0);
        assert_eq!(library_width(360.0), 360.0);
        // A wide window always leaves room for the player next to the panel.
        const { assert!(WIDE_WINDOW - LIBRARY_WIDTH >= 300.0) };
    }

    #[test]
    fn grid_fills_the_width_with_cards_of_at_least_the_minimum_size() {
        let gap = 8.0;
        for width in [120.0, 336.0, 450.0, 800.0, 1500.0] {
            let columns = grid_columns(width, gap);
            let size = card_size(width, columns, gap);
            assert!(columns >= 1);
            assert!(size >= CARD_MIN.min(width), "{width}: {size}");
            let used = columns as f32 * size + gap * (columns - 1) as f32;
            assert!((used - width).abs() < 0.01, "{width}: {used}");
        }
        assert_eq!(grid_columns(336.0, gap), 2); // the panel in a wide window
        assert_eq!(grid_columns(50.0, gap), 1);
    }

    #[test]
    fn library_starts_open_only_without_a_working_folder() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        assert!(app_with(&config).library_open);

        let mut app = app_with(&config);
        assert!(app.load_dir(dir.path().to_owned()));
        assert!(!app_with(&config).library_open);

        Config {
            music_dir: Some(dir.path().join("gone")),
            volume: 0.5,
        }
        .save_to(&config)
        .unwrap();
        assert!(app_with(&config).library_open);
    }

    #[test]
    fn a_failed_folder_change_reopens_the_library() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let mut app = app_with(&config);
        assert!(app.load_dir(dir.path().to_owned()));
        app.library_open = false;
        assert!(!app.load_dir(dir.path().join("missing")));
        assert!(app.library_open);
    }

    #[test]
    fn choosing_a_track_in_the_library_makes_it_current() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let music = dir.path().join("music");
        std::fs::create_dir(&music).unwrap();
        for name in ["a.mp3", "b.mp3"] {
            std::fs::write(music.join(name), b"").unwrap();
        }
        let mut app = app_with(&config);
        assert!(app.load_dir(music));
        assert_eq!(app.albums.len(), 1);
        let album = app.albums[0].tracks.clone();
        app.selected = Some(0);

        // The files are not playable audio, so playback fails gracefully, but
        // the chosen track is still the current one and the error is shown.
        app.play_index(album.start + 1);
        assert_eq!(app.current, Some(album.start + 1));
        assert!(app.message.is_some());
        assert_eq!(app.audio.state(), State::Stopped);
        // Previous and next then stay inside the open album.
        assert_eq!(app.scope(), album);
    }

    #[test]
    fn the_needle_starts_the_open_album_when_nothing_has_played() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let music = dir.path().join("music");
        std::fs::create_dir(&music).unwrap();
        std::fs::write(music.join("a.mp3"), b"").unwrap();
        let mut app = app_with(&config);
        assert!(app.load_dir(music));
        assert_eq!(app.current, None);
        app.apply_needle(NeedleAction::Restart);
        assert_eq!(app.current, Some(0));
    }

    #[test]
    fn progress_is_a_clamped_fraction() {
        let total = Some(Duration::from_secs(200));
        assert_eq!(progress(Duration::from_secs(50), total), 0.25);
        assert_eq!(progress(Duration::from_secs(500), total), 1.0);
        assert_eq!(progress(Duration::ZERO, total), 0.0);
    }

    #[test]
    fn progress_is_zero_without_a_known_length() {
        assert_eq!(progress(Duration::from_secs(5), None), 0.0);
        assert_eq!(progress(Duration::from_secs(5), Some(Duration::ZERO)), 0.0);
    }

    #[test]
    fn empty_scope_has_no_tracks() {
        assert_eq!(next_index(&(0..0), None), None);
        assert_eq!(previous_index(&(0..0), Some(0)), None);
    }
}
