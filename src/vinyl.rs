//! Vinyl record and tonearm: animation state and drawing.

use crate::audio::State;
use eframe::egui::{
    Color32, Painter, Pos2, Rect, Shape, Stroke, TextureId, Vec2,
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
const NEEDLE: Color32 = Color32::from_rgb(232, 220, 192);

/// 33 1/3 revolutions per minute, in radians per second.
const SPIN_SPEED: f32 = TAU * 100.0 / 180.0;
/// Seconds the tonearm takes to swing onto or off the record.
const ARM_SECONDS: f32 = 0.9;

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
#[derive(Debug, Default)]
pub struct Turntable {
    /// Record rotation in radians, `0..TAU`.
    angle: f32,
    /// Tonearm position: 0 at rest, 1 on the record.
    arm: f32,
}

fn arm_target(state: State) -> f32 {
    if state == State::Stopped { 0.0 } else { 1.0 }
}

impl Turntable {
    /// Advances the animation by `dt` seconds. The record turns only while
    /// playing; the arm rests on the record while playing or paused.
    pub fn update(&mut self, dt: f32, state: State) {
        if state == State::Playing {
            self.angle = (self.angle + SPIN_SPEED * dt).rem_euclid(TAU);
        }
        let target = arm_target(state);
        let step = dt / ARM_SECONDS;
        self.arm = if self.arm < target {
            (self.arm + step).min(target)
        } else {
            (self.arm - step).max(target)
        };
    }

    /// True while something is moving, i.e. frames must keep being drawn.
    pub fn is_animating(&self, state: State) -> bool {
        state == State::Playing || (self.arm - arm_target(state)).abs() > f32::EPSILON
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

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Needle position for an arm position and track progress (0..1).
fn needle(arm: f32, progress: f32) -> Vec2 {
    let r = GROOVE_OUTER + (GROOVE_INNER - GROOVE_OUTER) * progress.clamp(0.0, 1.0);
    let playing = (needle_point(r) - PIVOT).angle();
    let rest = (rest_point() - PIVOT).angle();
    let angle = rest + (playing - rest) * smoothstep(arm);
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

/// Draws the record and tonearm inside `rect`, scaled to fit. `cover` is the
/// label artwork; `progress` (0..1) positions the needle along the grooves.
pub fn paint(
    painter: &Painter,
    rect: Rect,
    turntable: &Turntable,
    cover: Option<TextureId>,
    progress: f32,
) {
    let r = (rect.width() / 2.3).min(rect.height() / 2.2).max(1.0);
    // The arm sticks out on the right and its counterweight above the record,
    // so shift the record left and down to centre the whole.
    let center = pos2(rect.center().x - 0.125 * r, rect.center().y + 0.06 * r);
    let at = |v: Vec2| center + v * r;
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
    let tip = needle(turntable.arm, progress);
    let direction = (tip - PIVOT).normalized();
    let tail = PIVOT - direction * 0.25;
    painter.line_segment([at(tail), at(tip)], Stroke::new((r * 0.022).max(2.0), ARM));
    painter.line_segment(
        [at(tip - direction * 0.14), at(tip)],
        Stroke::new((r * 0.05).max(3.0), ARM),
    );
    painter.circle_filled(at(tail), 0.06 * r, ARM_DARK);
    painter.circle_filled(at(PIVOT), 0.09 * r, ARM_DARK);
    painter.circle_stroke(at(PIVOT), 0.09 * r, Stroke::new(line, BRASS));
    painter.circle_filled(at(tip), (r * 0.012).max(1.5), NEEDLE);
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn record_turns_only_while_playing() {
        let mut t = Turntable::default();
        t.update(0.5, State::Playing);
        let turned = t.angle;
        assert!(turned > 0.0);
        t.update(0.5, State::Paused);
        assert_eq!(t.angle, turned);
        t.update(0.5, State::Stopped);
        assert_eq!(t.angle, turned);
    }

    #[test]
    fn rotation_speed_is_33_rpm_and_wraps() {
        let mut t = Turntable::default();
        t.update(1.8, State::Playing); // exactly one revolution at 33 1/3 rpm
        assert!(t.angle < 1e-3 || close(t.angle, TAU));
        for _ in 0..1000 {
            t.update(0.05, State::Playing);
            assert!((0.0..TAU).contains(&t.angle));
        }
    }

    #[test]
    fn arm_settles_on_record_when_playing_or_paused() {
        for state in [State::Playing, State::Paused] {
            let mut t = Turntable::default();
            t.update(ARM_SECONDS / 2.0, state);
            assert!(close(t.arm, 0.5));
            t.update(ARM_SECONDS, state);
            assert_eq!(t.arm, 1.0);
        }
    }

    #[test]
    fn arm_returns_to_rest_when_stopped() {
        let mut t = Turntable::default();
        t.update(2.0, State::Paused);
        assert_eq!(t.arm, 1.0);
        for _ in 0..(ARM_SECONDS / FRAME) as usize + 2 {
            t.update(FRAME, State::Stopped);
        }
        assert_eq!(t.arm, 0.0);
    }

    #[test]
    fn animation_runs_only_when_something_moves() {
        let mut t = Turntable::default();
        assert!(!t.is_animating(State::Stopped));
        assert!(t.is_animating(State::Playing));
        assert!(t.is_animating(State::Paused)); // arm still has to drop
        t.update(2.0, State::Paused);
        assert!(!t.is_animating(State::Paused));
        assert!(t.is_animating(State::Stopped)); // arm still has to lift
        t.update(2.0, State::Stopped);
        assert!(!t.is_animating(State::Stopped));
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
    fn needle_moves_inward_with_progress() {
        let start = needle(1.0, 0.0).length();
        let middle = needle(1.0, 0.5).length();
        let end = needle(1.0, 1.0).length();
        assert!(close(start, GROOVE_OUTER));
        assert!(close(end, GROOVE_INNER));
        assert!(start > middle && middle > end);
        // Out-of-range progress is clamped.
        assert!(close(needle(1.0, 7.0).length(), GROOVE_INNER));
    }

    #[test]
    fn resting_needle_is_off_the_record() {
        assert!(rest_point().length() > 1.0);
        assert!(close((needle(0.0, 0.3) - rest_point()).length(), 0.0));
        // Swinging keeps the arm length.
        for arm in [0.0, 0.25, 0.5, 1.0] {
            assert!(close((needle(arm, 0.3) - PIVOT).length(), ARM_LENGTH));
        }
    }
}
