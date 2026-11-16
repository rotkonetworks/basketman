//! Players standing still in 3D: the squad in a row on the Squad tab and a
//! single turning figure beside the body measurements on a profile. Both use
//! the figure code of the match view, so a player is built the same way
//! everywhere.

use std::time::Duration;

use basketman_core::court::Vec2;
use basketman_core::player::{Body, Player, PlayerId};
use eframe::egui::{self, Align2, Color32, FontId, Sense, Shape, Stroke, Ui, pos2, vec2};

use crate::theme;
use crate::view3d::gait::{Gait, Stats};
use crate::view3d::{Camera, FigureSpec, SURROUND, Scene, emit_figure, v3};

const STUDIO: Color32 = Color32::from_rgb(48, 55, 74);

/// The squad in the order they stand: by position, then by height.
pub(crate) fn order(players: &[Player], roster: &[PlayerId]) -> Vec<PlayerId> {
    let mut out = roster.to_vec();
    out.sort_by(|&a, &b| {
        let (a, b) = (&players[a as usize], &players[b as usize]);
        a.position
            .cmp(&b.position)
            .then(a.body.height_cm.total_cmp(&b.body.height_cm))
            .then(a.id.cmp(&b.id))
    });
    out
}

/// What the league averages, as a body, for the faint figure beside a
/// profile.
pub(crate) fn average_body(players: &[Player]) -> Body {
    let n = players.len().max(1) as f32;
    let mean = |f: fn(&Body) -> f32| players.iter().map(|p| f(&p.body)).sum::<f32>() / n;
    Body {
        height_cm: mean(|b| b.height_cm),
        weight_kg: mean(|b| b.weight_kg),
        wingspan_cm: mean(|b| b.wingspan_cm),
        standing_reach_cm: mean(|b| b.standing_reach_cm),
        shoulder_width_cm: mean(|b| b.shoulder_width_cm),
        hand_length_cm: mean(|b| b.hand_length_cm),
        hand: basketman_core::player::Hand::Right,
    }
}

/// One player, slowly turning, beside an average player in grey and a
/// height scale, so that his size reads at a glance. `stats` should be
/// `Stats::AVERAGE` when his ratings are not known to the user.
pub(crate) fn profile_figure(
    ui: &mut Ui,
    p: &Player,
    stats: Stats,
    average: &Body,
    size: egui::Vec2,
) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let t = ui.input(|i| i.time) as f32;
    ui.ctx().request_repaint_after(Duration::from_millis(40));
    // Turning slowly, starting from the front.
    let a = -std::f32::consts::FRAC_PI_2 + (t * 0.55).sin() * 1.25;
    let face = Vec2 {
        x: a.cos(),
        y: a.sin(),
    };
    let tall = p.body.height_cm / 100.0;
    let top = tall.max(average.height_cm / 100.0).max(2.0);
    let dist = 7.0;
    let mut cam = Camera::new(v3(-0.1, -dist, 1.4), v3(-0.1, 0.0, 1.05));
    let per_metre = (rect.height() * 0.80 / top).min(rect.width() / 3.2);
    cam.zoom = per_metre / cam.scale(dist, rect);
    if let Some((foot, _)) = cam.project(v3(0.0, 0.0, 0.0), rect) {
        cam.target.z -= (foot.y - (rect.bottom() - 28.0)) / per_metre;
    }
    let mut scene = Scene::new(cam.eye);
    scene.ground_rect(-1.5, -1.3, 1.5, 1.3, STUDIO);
    let (his_x, avg_x) = (-0.35, 0.8);
    for (x, body) in [(his_x, &p.body), (avg_x, average)] {
        let broad = body.shoulder_width_cm / 100.0;
        scene.ground_disc(
            (x, 0.0),
            broad * 0.9,
            broad * 0.75,
            Color32::from_black_alpha(90),
        );
    }
    emit_figure(
        &mut scene,
        &FigureSpec {
            body: average,
            stats: Stats::AVERAGE,
            id: 3,
            pos: Vec2 { x: avg_x, y: 0.0 },
            gait: Gait::standing(face),
            team: Color32::from_rgb(112, 118, 134),
            holds_ball: false,
            act: None,
        },
    );
    emit_figure(
        &mut scene,
        &FigureSpec {
            body: &p.body,
            stats,
            id: p.id,
            pos: Vec2 { x: his_x, y: 0.0 },
            gait: Gait::standing(face),
            team: theme::ACCENT,
            holds_ball: false,
            act: None,
        },
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, SURROUND);
    painter.add(Shape::mesh(scene.mesh(&cam, rect)));

    // A scale on the left, every ten centimetres, and a line at his height.
    let project = |x: f32, z: f32| cam.project(v3(x, 0.0, z), rect).map(|p| p.0);
    let faint = Color32::from_white_alpha(70);
    let rule_x = -1.3;
    if let (Some(a), Some(b)) = (project(rule_x, 0.0), project(rule_x, 2.4)) {
        painter.line_segment([a, b], Stroke::new(1.0, faint));
    }
    for cm in (0..=240).step_by(10) {
        let z = cm as f32 / 100.0;
        let Some(at) = project(rule_x, z) else {
            continue;
        };
        let long = cm % 50 == 0;
        let len = if long { 8.0 } else { 4.0 };
        painter.line_segment([at, at + vec2(len, 0.0)], Stroke::new(1.0, faint));
        if long && cm > 0 {
            painter.text(
                at + vec2(11.0, 0.0),
                Align2::LEFT_CENTER,
                format!("{:.1} m", z),
                FontId::new(crate::theme::fs(10.0), theme::mono_semibold()),
                Color32::from_white_alpha(120),
            );
        }
    }
    if let (Some(a), Some(b)) = (project(rule_x, tall), project(1.25, tall)) {
        painter.line_segment([a, b], Stroke::new(1.0, Color32::from_white_alpha(110)));
        painter.text(
            pos2(a.x + 14.0, a.y - 3.0),
            Align2::LEFT_BOTTOM,
            format!("{:.0} cm", p.body.height_cm),
            FontId::new(crate::theme::fs(11.0), theme::mono_semibold()),
            Color32::WHITE,
        );
    }
    let avg_top = average.height_cm / 100.0;
    if let Some(b) = project(avg_x, avg_top + 0.08) {
        painter.text(
            b,
            Align2::CENTER_BOTTOM,
            "league average",
            FontId::new(crate::theme::fs(10.0), theme::mono_semibold()),
            Color32::from_white_alpha(120),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_row_is_ordered_by_position_then_height() {
        let world = basketman_core::league::World::generate(5, 12);
        let roster = world.teams[0].roster.clone();
        let row = order(&world.players, &roster);
        assert_eq!(row.len(), roster.len());
        for pair in row.windows(2) {
            let (a, b) = (
                &world.players[pair[0] as usize],
                &world.players[pair[1] as usize],
            );
            assert!(a.position <= b.position);
            if a.position == b.position {
                assert!(a.body.height_cm <= b.body.height_cm);
            }
        }
    }
}
