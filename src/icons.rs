//! Simple vector icons drawn with egui shapes (no font or image assets).

use eframe::egui::{Color32, Painter, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
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
    match icon {
        // Classic skip symbols: a bar and two triangles.
        Icon::Previous => vec![
            vec![at(0.04, 0.1), at(0.15, 0.1), at(0.15, 0.9), at(0.04, 0.9)],
            vec![at(0.56, 0.1), at(0.56, 0.9), at(0.2, 0.5)],
            vec![at(0.94, 0.1), at(0.94, 0.9), at(0.58, 0.5)],
        ],
        Icon::Next => vec![
            vec![at(0.85, 0.1), at(0.96, 0.1), at(0.96, 0.9), at(0.85, 0.9)],
            vec![at(0.44, 0.1), at(0.44, 0.9), at(0.8, 0.5)],
            vec![at(0.06, 0.1), at(0.06, 0.9), at(0.42, 0.5)],
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

/// A non-interactive icon, e.g. a label next to a slider.
pub fn label(ui: &mut Ui, icon: Icon) {
    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    let color = ui.visuals().text_color();
    paint(ui.painter(), rect, icon, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Icon; 3] = [Icon::Previous, Icon::Next, Icon::Volume];

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
    fn next_mirrors_previous() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0));
        let mut previous: Vec<Pos2> = parts(Icon::Previous, rect).into_iter().flatten().collect();
        let mut next: Vec<Pos2> = parts(Icon::Next, rect)
            .into_iter()
            .flatten()
            .map(|p| pos2(100.0 - p.x, p.y))
            .collect();
        let key = |p: &Pos2| ((p.x * 1000.0).round() as i32, (p.y * 1000.0).round() as i32);
        previous.sort_by_key(key);
        next.sort_by_key(key);
        assert_eq!(previous.len(), next.len());
        for (a, b) in previous.iter().zip(&next) {
            assert!(
                (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3,
                "{a:?} {b:?}"
            );
        }
    }
}
