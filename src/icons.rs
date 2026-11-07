//! Simple vector icons drawn with egui shapes (no font or image assets).

use eframe::egui::{Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, pos2, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Pause,
    Stop,
    Previous,
    Next,
    Volume,
}

/// Convex polygons that make up `icon`, laid out inside `rect`.
fn parts(icon: Icon, rect: Rect) -> Vec<Vec<Pos2>> {
    // Maps unit-square coordinates into `rect`.
    let at = |x: f32, y: f32| {
        pos2(
            rect.min.x + rect.width() * x,
            rect.min.y + rect.height() * y,
        )
    };
    let bar = |x0: f32, x1: f32| vec![at(x0, 0.1), at(x1, 0.1), at(x1, 0.9), at(x0, 0.9)];
    match icon {
        Icon::Play => vec![vec![at(0.18, 0.05), at(0.18, 0.95), at(0.92, 0.5)]],
        Icon::Pause => vec![bar(0.15, 0.4), bar(0.6, 0.85)],
        Icon::Stop => vec![vec![
            at(0.12, 0.12),
            at(0.88, 0.12),
            at(0.88, 0.88),
            at(0.12, 0.88),
        ]],
        Icon::Previous => vec![
            bar(0.08, 0.22),
            vec![at(0.9, 0.08), at(0.9, 0.92), at(0.3, 0.5)],
        ],
        Icon::Next => vec![
            vec![at(0.1, 0.08), at(0.1, 0.92), at(0.7, 0.5)],
            bar(0.78, 0.92),
        ],
        Icon::Volume => vec![
            vec![at(0.05, 0.36), at(0.3, 0.36), at(0.3, 0.64), at(0.05, 0.64)],
            vec![at(0.3, 0.36), at(0.62, 0.1), at(0.62, 0.9), at(0.3, 0.64)],
            // A single sound wave, as a thin bar.
            vec![at(0.76, 0.3), at(0.84, 0.3), at(0.84, 0.7), at(0.76, 0.7)],
        ],
    }
}

/// Draws `icon` filling `rect`.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    for points in parts(icon, rect) {
        painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
    }
}

/// A small clickable icon button with a tooltip.
pub fn button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(32.0, 26.0), Sense::click());
    let visuals = ui.style().interact(&response);
    ui.painter().rect_filled(rect, 4.0, visuals.weak_bg_fill);
    let icon_rect = Rect::from_center_size(rect.center(), vec2(14.0, 14.0));
    paint(ui.painter(), icon_rect, icon, visuals.fg_stroke.color);
    response.on_hover_text(tooltip)
}

/// A non-interactive icon, e.g. a label next to a slider.
pub fn label(ui: &mut Ui, icon: Icon) {
    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    let color = ui.visuals().text_color();
    paint(ui.painter(), rect, icon, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Icon; 6] = [
        Icon::Play,
        Icon::Pause,
        Icon::Stop,
        Icon::Previous,
        Icon::Next,
        Icon::Volume,
    ];

    #[test]
    fn every_icon_stays_inside_its_rect() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(40.0, 40.0));
        for icon in ALL {
            let parts = parts(icon, rect);
            assert!(!parts.is_empty(), "{icon:?}");
            for point in parts.iter().flatten() {
                assert!(rect.contains(*point), "{icon:?} {point:?}");
            }
        }
    }

    #[test]
    fn every_part_is_a_polygon() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(16.0, 16.0));
        for icon in ALL {
            assert!(parts(icon, rect).iter().all(|p| p.len() >= 3), "{icon:?}");
        }
    }

    #[test]
    fn pause_has_two_bars_and_play_one_triangle() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(16.0, 16.0));
        assert_eq!(parts(Icon::Pause, rect).len(), 2);
        let play = parts(Icon::Play, rect);
        assert_eq!(play.len(), 1);
        assert_eq!(play[0].len(), 3);
    }
}
