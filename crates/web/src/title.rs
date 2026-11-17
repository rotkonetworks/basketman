//! The mark of the game: a basketball, seams and all. The first screen is
//! the main menu (`menu.rs`).

use eframe::egui::{Color32, Pos2, Sense, Shape, Stroke, Ui, vec2};

use crate::theme;

const BALL: Color32 = Color32::from_rgb(0xE8, 0x6A, 0x1C);

/// The mark: a basketball, seams and all.
pub(crate) fn ball(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let (c, r) = (rect.center(), size / 2.0);
    let painter = ui.painter_at(rect.expand(1.0));
    painter.circle_filled(c, r, BALL);
    let seam = Stroke::new((size / 22.0).max(1.0), theme::TEXT);
    painter.line_segment([c - vec2(r, 0.0), c + vec2(r, 0.0)], seam);
    painter.line_segment([c - vec2(0.0, r), c + vec2(0.0, r)], seam);
    for side in [-1.0, 1.0f32] {
        let curve: Vec<Pos2> = (0..=24)
            .map(|i| {
                let t = i as f32 / 12.0 - 1.0;
                c + vec2(side * r * (0.62 - 0.30 * (1.0 - t * t)), t * r * 0.79)
            })
            .collect();
        painter.add(Shape::line(curve, seam));
    }
    painter.circle_stroke(c, r, seam);
}
