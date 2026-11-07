//! Vinyl record, tonearm and skip buttons: animation state, drawing and hit testing.

use crate::{
    audio::State,
    icons::{self, Icon},
};
use eframe::egui::{
    Color32, CursorIcon, Painter, Pos2, Rect, Sense, Shape, Stroke, TextureId, Ui, Vec2,
    emath::Rot2,
    epaint::{Mesh, Vertex},
    pos2, vec2,
};
use std::f32::consts::{PI, TAU};

/// Warm near-black used behind the record.
pub const BACKGROUND: Color32 = Color32::from_rgb(20, 17, 14);
const RECORD: Color32 = Color32::from_rgb(12, 12, 12);
const RIM: Color32 = Color32::from_rgb(42, 38, 34);
const LABEL: Color32 = Color32::from_rgb(94, 43, 43);
const BRASS: Color32 = Color32::from_rgb(138, 115, 67);
const ARM: Color32 = Color32::from_rgb(156, 146, 126);
const ARM_DARK: Color32 = Color32::from_rgb(52, 47, 40);
const BUTTON: Color32 = Color32::from_rgb(30, 26, 22);
const NEEDLE: Color32 = Color32::from_rgb(232, 220, 192);

/// 33 1/3 revolutions per minute, in radians per second.
const SPIN_SPEED: f32 = TAU * 100.0 / 180.0;
/// Tonearm swing speed (radians per second) when it moves on its own.
const ARM_SPEED: f32 = 0.6;

// Geometry, in units of the record radius, origin at the record centre, y down.
const PIVOT: Vec2 = vec2(1.15, -0.8);
const ARM_LENGTH: f32 = 1.2;
/// Needle distance from the centre at the start and end of a track.
const GROOVE_OUTER: f32 = 0.93;
const GROOVE_INNER: f32 = 0.45;
const LABEL_RADIUS: f32 = 0.34;
/// How far the arm swings away from the record when at rest (radians).
const REST_SWING: f32 = 0.5;

/// Rotation of the record and position of the tonearm.
///
/// The arm turns about a fixed pivot, so the needle always lies on one arc.
/// `arm` is its angle on that arc, between [`rest_angle`] (off the record)
/// and [`inner_angle`] (innermost groove).
#[derive(Debug)]
pub struct Turntable {
    /// Record rotation in radians, `0..TAU`.
    angle: f32,
    /// Tonearm angle about the pivot, in radians.
    arm: f32,
    /// The user is holding the tonearm.
    dragging: bool,
}

impl Default for Turntable {
    fn default() -> Self {
        Self {
            angle: 0.0,
            arm: rest_angle(),
            dragging: false,
        }
    }
}

/// Where the arm settles by itself: following the track while playing,
/// off the record otherwise.
fn target_angle(state: State, progress: f32) -> f32 {
    if state == State::Playing {
        play_angle(progress)
    } else {
        rest_angle()
    }
}

impl Turntable {
    /// Advances the animation by `dt` seconds. The record turns only while
    /// playing. The arm moves on its own unless the user is holding it.
    pub fn update(&mut self, dt: f32, state: State, progress: f32) {
        if state == State::Playing {
            self.angle = (self.angle + SPIN_SPEED * dt).rem_euclid(TAU);
        }
        if self.dragging {
            return;
        }
        let target = target_angle(state, progress);
        let step = ARM_SPEED * dt;
        self.arm = if self.arm < target {
            (self.arm + step).min(target)
        } else {
            (self.arm - step).max(target)
        };
    }

    /// True while something is moving, i.e. frames must keep being drawn.
    pub fn is_animating(&self, state: State, progress: f32) -> bool {
        state == State::Playing
            || self.dragging
            || (self.arm - target_angle(state, progress)).abs() > f32::EPSILON
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// The user picks up the tonearm.
    pub fn grab(&mut self) {
        self.dragging = true;
    }

    /// Moves the held arm to `angle`, kept on the arc.
    pub fn drag_to(&mut self, angle: f32) {
        self.arm = angle.clamp(rest_angle(), inner_angle());
    }

    /// The user lets go; the arm settles on its own again.
    pub fn release(&mut self) {
        self.dragging = false;
    }

    /// True when the needle is over the record.
    pub fn over_record(&self) -> bool {
        self.arm >= edge_angle()
    }
}

/// The point at distance `r` from the record centre that the arm can reach.
fn needle_point(r: f32) -> Vec2 {
    let d = PIVOT.length();
    let a = (r * r - ARM_LENGTH * ARM_LENGTH + d * d) / (2.0 * d);
    let h = (r * r - a * a).max(0.0).sqrt();
    let along = PIVOT / d;
    let across = vec2(-along.y, along.x);
    let (first, second) = (along * a + across * h, along * a - across * h);
    if first.y >= second.y { first } else { second }
}

/// Needle position when the arm is off the record.
fn rest_point() -> Vec2 {
    let from_pivot = needle_point(GROOVE_OUTER) - PIVOT;
    let swung = |angle: f32| PIVOT + Rot2::from_angle(angle) * from_pivot;
    let (left, right) = (swung(REST_SWING), swung(-REST_SWING));
    if left.length() >= right.length() {
        left
    } else {
        right
    }
}

/// Arm angle with the needle off the record. The smallest angle of the arc.
fn rest_angle() -> f32 {
    (rest_point() - PIVOT).angle()
}

/// Arm angle with the needle on the innermost groove. The largest angle of the arc.
fn inner_angle() -> f32 {
    (needle_point(GROOVE_INNER) - PIVOT).angle()
}

/// Arm angle with the needle exactly at the record's edge.
fn edge_angle() -> f32 {
    (needle_point(1.0) - PIVOT).angle()
}

/// Arm angle for a track that is `progress` (0..1) played.
fn play_angle(progress: f32) -> f32 {
    let r = GROOVE_OUTER + (GROOVE_INNER - GROOVE_OUTER) * progress.clamp(0.0, 1.0);
    (needle_point(r) - PIVOT).angle()
}

/// Needle position for an arm angle.
fn arm_tip(angle: f32) -> Vec2 {
    PIVOT + Vec2::angled(angle) * ARM_LENGTH
}

/// A disc textured with `texture`, rotated by `angle`.
fn art_mesh(texture: TextureId, center: Pos2, radius: f32, angle: f32) -> Mesh {
    const SEGMENTS: u32 = 64;
    let mut mesh = Mesh::with_texture(texture);
    mesh.vertices.push(Vertex {
        pos: center,
        uv: pos2(0.5, 0.5),
        color: Color32::WHITE,
    });
    for i in 0..=SEGMENTS {
        let a = TAU * i as f32 / SEGMENTS as f32;
        mesh.vertices.push(Vertex {
            pos: center + Vec2::angled(a + angle) * radius,
            uv: pos2(0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin()),
            color: Color32::WHITE,
        });
    }
    for i in 0..SEGMENTS {
        mesh.indices.extend([0, i + 1, i + 2]);
    }
    mesh
}

/// A faint reflection on the vinyl, turning with the record so rotation is visible.
fn sheen(painter: &Painter, center: Pos2, radius: f32, start: f32) {
    const WIDTH: f32 = 0.35;
    const STEPS: usize = 8;
    let mut points = vec![center];
    for i in 0..=STEPS {
        let a = start + WIDTH * i as f32 / STEPS as f32;
        points.push(center + Vec2::angled(a) * radius);
    }
    painter.add(Shape::convex_polygon(
        points,
        Color32::from_white_alpha(9),
        Stroke::NONE,
    ));
}

/// Where everything is drawn inside the area given to the record view.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    /// Record centre.
    pub center: Pos2,
    pub radius: f32,
    /// Centres of the previous and next buttons, left and right of the record.
    pub prev: Pos2,
    pub next: Pos2,
    /// Button diameter.
    pub button: f32,
}

/// Lays out the previous button, the record with its tonearm, and the next
/// button as one group centred in `rect`.
pub fn layout(rect: Rect) -> Layout {
    let button = (rect.height() * 0.16).clamp(40.0, 72.0);
    let column = button + 12.0;
    // Horizontally the record and arm span -1.0 to 1.5 radii around the centre.
    let radius = ((rect.width() - 2.0 * column) / 2.5)
        .min(rect.height() / 2.2)
        .max(1.0);
    let left = rect.center().x - (2.0 * column + 2.5 * radius) / 2.0;
    // The arm's counterweight reaches above the record, so shift down a little.
    let center = pos2(left + column + radius, rect.center().y + 0.06 * radius);
    Layout {
        center,
        radius,
        prev: pos2(left + column / 2.0, center.y),
        next: pos2(left + column + 2.5 * radius + column / 2.0, center.y),
        button,
    }
}

fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() == 0.0 {
        0.0
    } else {
        ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
    };
    (p - (a + ab * t)).length()
}

/// Ends of the tonearm's bar on screen, from counterweight to needle.
fn arm_segment(layout: &Layout, turntable: &Turntable) -> (Pos2, Pos2) {
    let tip = arm_tip(turntable.arm);
    let tail = PIVOT - (tip - PIVOT).normalized() * 0.25;
    let at = |v: Vec2| layout.center + v * layout.radius;
    (at(tail), at(tip))
}

/// True when `pos` is on the tonearm, i.e. it can be picked up there.
pub fn arm_contains(layout: &Layout, turntable: &Turntable, pos: Pos2) -> bool {
    let (tail, tip) = arm_segment(layout, turntable);
    distance_to_segment(pos, tail, tip) <= (0.07 * layout.radius).max(10.0)
}

/// Arm angle that puts the needle toward `pos`, kept on the arc.
pub fn arm_angle_at(layout: &Layout, pos: Pos2) -> f32 {
    let unit = (pos - layout.center) / layout.radius;
    (unit - PIVOT).angle().clamp(rest_angle(), inner_angle())
}

/// A large round, classic-style button. Returns true when clicked.
pub fn skip_button(ui: &mut Ui, center: Pos2, diameter: f32, icon: Icon, tooltip: &str) -> bool {
    let rect = Rect::from_center_size(center, Vec2::splat(diameter));
    let response = ui.interact(rect, ui.scope_id().with(tooltip), Sense::click());
    let over = response
        .hover_pos()
        .is_some_and(|p| (p - center).length() <= diameter / 2.0);
    if over {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let (fill, color) = if over {
        (ARM_DARK, NEEDLE)
    } else {
        (BUTTON, ARM)
    };
    let painter = ui.painter();
    painter.circle_filled(center, diameter / 2.0, fill);
    painter.circle_stroke(
        center,
        diameter / 2.0,
        Stroke::new((diameter * 0.04).max(1.5), BRASS),
    );
    painter.circle_stroke(
        center,
        diameter * 0.41,
        Stroke::new(1.0, BRASS.gamma_multiply(0.35)),
    );
    let icon_rect = Rect::from_center_size(center, Vec2::splat(diameter * 0.44));
    icons::paint(painter, icon_rect, icon, color);
    let clicked = over && response.clicked();
    response.on_hover_text(tooltip);
    clicked
}

/// Draws the record and tonearm. `cover` is the label artwork.
pub fn paint(painter: &Painter, layout: &Layout, turntable: &Turntable, cover: Option<TextureId>) {
    let (center, r) = (layout.center, layout.radius);
    let line = (r * 0.006).max(1.0);

    // Record.
    painter.circle_filled(center, r, RECORD);
    painter.circle_stroke(center, r, Stroke::new(line, RIM));
    let mut groove = 0.40;
    while groove < 0.96 {
        painter.circle_stroke(
            center,
            groove * r,
            Stroke::new(line, Color32::from_white_alpha(7)),
        );
        groove += 0.035;
    }
    for half in 0..2 {
        sheen(
            painter,
            center,
            0.97 * r,
            turntable.angle + half as f32 * PI,
        );
    }

    // Label with artwork (or a plain label with a mark so the turning shows).
    let label = LABEL_RADIUS * r;
    match cover {
        Some(texture) => {
            painter.add(Shape::mesh(art_mesh(
                texture,
                center,
                label,
                turntable.angle,
            )));
        }
        None => {
            painter.circle_filled(center, label, LABEL);
            let mark = Vec2::angled(turntable.angle);
            painter.line_segment(
                [center + mark * label * 0.45, center + mark * label * 0.85],
                Stroke::new(line * 2.0, BRASS),
            );
        }
    }
    painter.circle_stroke(center, label, Stroke::new(line, BRASS));
    painter.circle_filled(center, 0.03 * r, BACKGROUND);

    // Tonearm.
    let at = |v: Vec2| center + v * r;
    let (tail, tip) = arm_segment(layout, turntable);
    let direction = (tip - tail).normalized();
    painter.line_segment([tail, tip], Stroke::new((r * 0.022).max(2.0), ARM));
    painter.line_segment(
        [tip - direction * 0.14 * r, tip],
        Stroke::new((r * 0.05).max(3.0), ARM),
    );
    painter.circle_filled(tail, 0.06 * r, ARM_DARK);
    painter.circle_filled(at(PIVOT), 0.09 * r, ARM_DARK);
    painter.circle_stroke(at(PIVOT), 0.09 * r, Stroke::new(line, BRASS));
    painter.circle_filled(tip, (r * 0.012).max(1.5), NEEDLE);
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn scene() -> (Rect, Layout) {
        let rect = Rect::from_min_size(pos2(7.0, 9.0), vec2(640.0, 480.0));
        (rect, layout(rect))
    }

    #[test]
    fn layout_fits_everything_in_the_rect() {
        for size in [
            vec2(480.0, 400.0),
            vec2(1000.0, 300.0),
            vec2(360.0, 300.0),
            vec2(900.0, 700.0),
        ] {
            let rect = Rect::from_min_size(pos2(7.0, 9.0), size);
            let l = layout(rect);
            let half = l.button / 2.0;
            assert!(l.prev.x - half >= rect.min.x - 0.5, "{size:?}");
            assert!(l.next.x + half <= rect.max.x + 0.5, "{size:?}");
            assert!(l.center.y - 1.1 * l.radius >= rect.min.y - 0.5, "{size:?}");
            assert!(l.center.y + l.radius <= rect.max.y + 0.5, "{size:?}");
        }
    }

    #[test]
    fn buttons_flank_the_record_without_touching_it_or_the_arm() {
        let (_, l) = scene();
        let half = l.button / 2.0;
        assert_eq!(l.prev.y, l.center.y);
        assert_eq!(l.next.y, l.center.y);
        assert!(l.prev.x + half <= l.center.x - l.radius);
        // The resting needle reaches furthest right, 1.5 radii from the centre.
        assert!(l.next.x - half >= l.center.x + 1.5 * l.radius);
        assert!(l.button >= 40.0);
    }

    #[test]
    fn arc_angles_are_ordered_rest_edge_groove_inner() {
        let outer = play_angle(0.0);
        assert!(rest_angle() < edge_angle());
        assert!(edge_angle() < outer);
        assert!(outer < play_angle(0.5) && play_angle(0.5) < play_angle(1.0));
        assert!(close(play_angle(1.0), inner_angle()));
        // Out-of-range progress is clamped.
        assert!(close(play_angle(9.0), inner_angle()));
    }

    #[test]
    fn needle_stays_on_the_arc_for_any_drag() {
        let (_, l) = scene();
        let mut t = Turntable::default();
        t.grab();
        for pos in [
            pos2(-500.0, -500.0),
            pos2(0.0, 0.0),
            l.center,
            pos2(900.0, 40.0),
            pos2(300.0, 900.0),
            l.next,
        ] {
            t.drag_to(arm_angle_at(&l, pos));
            assert!(t.arm >= rest_angle() - 1e-6 && t.arm <= inner_angle() + 1e-6);
            // The rigid arm keeps its length, so the tip is always on the arc.
            assert!(close((arm_tip(t.arm) - PIVOT).length(), ARM_LENGTH));
        }
    }

    #[test]
    fn dragging_follows_the_pointer_along_the_arc() {
        let (_, l) = scene();
        let angle = (edge_angle() + inner_angle()) / 2.0;
        let tip = l.center + arm_tip(angle) * l.radius;
        assert!(close(arm_angle_at(&l, tip), angle));
        // A pointer far off the arc maps to its nearest direction, still clamped.
        assert!(close(
            arm_angle_at(&l, pos2(-1e4, l.center.y)),
            inner_angle()
        ));
        assert!(close(arm_angle_at(&l, pos2(1e4, -1e4)), rest_angle()));
    }

    #[test]
    fn the_arm_can_be_grabbed_only_on_the_arm() {
        let (_, l) = scene();
        let t = Turntable::default();
        let (tail, tip) = arm_segment(&l, &t);
        assert!(arm_contains(&l, &t, tip));
        assert!(arm_contains(&l, &t, tail));
        assert!(arm_contains(&l, &t, tip + (tail - tip) * 0.5));
        assert!(!arm_contains(&l, &t, l.center));
        assert!(!arm_contains(&l, &t, tip + vec2(0.0, 80.0)));
    }

    #[test]
    fn over_record_changes_at_the_record_edge() {
        let mut t = Turntable::default();
        assert!(!t.over_record()); // resting
        t.drag_to(edge_angle() - 0.01);
        assert!(!t.over_record());
        t.drag_to(edge_angle() + 0.01);
        assert!(t.over_record());
        t.drag_to(inner_angle());
        assert!(t.over_record());
        // At the edge the needle is exactly one radius from the centre.
        assert!(close(arm_tip(edge_angle()).length(), 1.0));
    }

    #[test]
    fn record_turns_only_while_playing() {
        let mut t = Turntable::default();
        t.update(0.5, State::Playing, 0.0);
        let turned = t.angle;
        assert!(turned > 0.0);
        t.update(0.5, State::Paused, 0.0);
        assert_eq!(t.angle, turned);
        t.update(0.5, State::Stopped, 0.0);
        assert_eq!(t.angle, turned);
    }

    #[test]
    fn rotation_speed_is_33_rpm_and_wraps() {
        let mut t = Turntable::default();
        t.update(1.8, State::Playing, 0.0); // exactly one revolution at 33 1/3 rpm
        assert!(t.angle < 1e-3 || close(t.angle, TAU));
        for _ in 0..1000 {
            t.update(0.05, State::Playing, 0.0);
            assert!((0.0..TAU).contains(&t.angle));
        }
    }

    #[test]
    fn arm_lands_on_the_record_and_follows_progress_while_playing() {
        let mut t = Turntable::default();
        for _ in 0..200 {
            t.update(FRAME, State::Playing, 0.0);
        }
        assert!(close(t.arm, play_angle(0.0)));
        assert!(t.over_record());
        // A track advances slowly, so the arm keeps up with it exactly.
        let mut progress = 0.0;
        for _ in 0..600 {
            progress += 0.0001;
            t.update(FRAME, State::Playing, progress);
            assert!(close(t.arm, play_angle(progress)));
        }
        // A sudden jump (a new track starting over) is followed at the arm's own speed.
        t.update(FRAME, State::Playing, 0.0);
        assert!(t.arm > play_angle(0.0));
        for _ in 0..200 {
            t.update(FRAME, State::Playing, 0.0);
        }
        assert!(close(t.arm, play_angle(0.0)));
    }

    #[test]
    fn arm_lifts_off_when_paused_or_stopped() {
        for state in [State::Paused, State::Stopped] {
            let mut t = Turntable::default();
            for _ in 0..200 {
                t.update(FRAME, State::Playing, 0.3);
            }
            assert!(t.over_record());
            for _ in 0..200 {
                t.update(FRAME, state, 0.3);
            }
            assert!(close(t.arm, rest_angle()));
            assert!(!t.over_record());
        }
    }

    #[test]
    fn arm_swings_at_a_limited_speed() {
        let mut t = Turntable::default();
        t.update(0.1, State::Playing, 0.0);
        assert!(close(t.arm, rest_angle() + ARM_SPEED * 0.1));
    }

    #[test]
    fn held_arm_ignores_the_automatic_position_until_released() {
        let mut t = Turntable::default();
        for _ in 0..200 {
            t.update(FRAME, State::Playing, 0.2);
        }
        t.grab();
        assert!(t.is_dragging());
        t.drag_to(rest_angle());
        for _ in 0..30 {
            t.update(FRAME, State::Playing, 0.2);
        }
        assert!(close(t.arm, rest_angle()), "stays where it was held");
        t.release();
        assert!(!t.is_dragging());
        for _ in 0..200 {
            t.update(FRAME, State::Playing, 0.2);
        }
        assert!(close(t.arm, play_angle(0.2)), "settles back onto the track");
    }

    #[test]
    fn record_keeps_turning_while_the_arm_is_held() {
        let mut t = Turntable::default();
        t.grab();
        t.update(0.5, State::Playing, 0.0);
        assert!(t.angle > 0.0);
    }

    #[test]
    fn animation_runs_only_when_something_moves() {
        let mut t = Turntable::default();
        assert!(!t.is_animating(State::Stopped, 0.0));
        assert!(!t.is_animating(State::Paused, 0.4));
        assert!(t.is_animating(State::Playing, 0.0));
        t.update(5.0, State::Playing, 0.0);
        t.update(FRAME, State::Paused, 0.0);
        assert!(t.is_animating(State::Paused, 0.0)); // arm still lifting
        t.update(5.0, State::Paused, 0.0);
        assert!(!t.is_animating(State::Paused, 0.0));
        t.grab();
        assert!(t.is_animating(State::Paused, 0.0));
    }

    #[test]
    fn needle_reaches_the_requested_groove_with_a_rigid_arm() {
        for r in [GROOVE_INNER, 0.6, GROOVE_OUTER] {
            let p = needle_point(r);
            assert!(close(p.length(), r), "radius {r}");
            assert!(close((p - PIVOT).length(), ARM_LENGTH), "arm at {r}");
        }
    }

    #[test]
    fn resting_needle_is_off_the_record() {
        assert!(arm_tip(rest_angle()).length() > 1.0);
        assert!(close((arm_tip(rest_angle()) - rest_point()).length(), 0.0));
    }

    #[test]
    fn distance_to_segment_handles_ends_and_degenerate_segments() {
        let (a, b) = (pos2(0.0, 0.0), pos2(10.0, 0.0));
        assert!(close(distance_to_segment(pos2(5.0, 3.0), a, b), 3.0));
        assert!(close(distance_to_segment(pos2(-4.0, 3.0), a, b), 5.0));
        assert!(close(distance_to_segment(pos2(3.0, 4.0), a, a), 5.0));
    }
}
