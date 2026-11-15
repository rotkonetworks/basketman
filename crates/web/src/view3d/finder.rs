//! Finding the moments that matter in a recording, for the screenshot harness:
//! the n-th made three, missed jumper, layup, dunk, block, rebound or free
//! throw. Returns the replay time the moment starts at.

use basketman_core::court::ShotKind;
use basketman_core::replay::{Action, EventKind, Recording, ShotStage, Style};

/// Times of every moment of kind `what`, in order.
pub(crate) fn moments(rec: &Recording, what: &str) -> Vec<f32> {
    let shots = |f: &dyn Fn(bool, ShotKind, bool) -> bool| -> Vec<f32> {
        rec.events
            .iter()
            .filter_map(|e| match &e.kind {
                EventKind::Shot {
                    made,
                    kind,
                    blocked_by,
                    ..
                } if f(*made, *kind, blocked_by.is_some()) => Some(e.time),
                _ => None,
            })
            .collect()
    };
    let styled = |style: Style, made: Option<bool>| -> Vec<f32> {
        let mut out = Vec::new();
        for (i, f) in rec.frames.iter().enumerate().skip(1) {
            let o = f.offense as usize;
            for k in 0..5 {
                let (p, q) = (&rec.frames[i - 1].pose[o][k], &f.pose[o][k]);
                if q.action == Action::Shooting
                    && q.stage == ShotStage::Release
                    && p.stage != ShotStage::Release
                    && q.style == style
                {
                    let ok = made.is_none_or(|m| {
                        rec.events
                            .iter()
                            .find(|e| e.time >= f.time - 0.01 && matches!(e.kind, EventKind::Shot { .. }))
                            .is_some_and(|e| matches!(e.kind, EventKind::Shot { made, .. } if made == m))
                    });
                    if ok {
                        out.push(f.time);
                    }
                }
            }
        }
        out
    };
    match what {
        "made3" => shots(&|m, k, _| m && k == ShotKind::Three),
        "made_jumper" => shots(&|m, k, _| m && k != ShotKind::Rim),
        "miss_jumper" => shots(&|m, k, b| !m && k != ShotKind::Rim && !b),
        "miss3" => shots(&|m, k, b| !m && k == ShotKind::Three && !b),
        "block" => shots(&|_, _, b| b),
        "layup" => styled(Style::Layup, Some(true)),
        "miss_layup" => styled(Style::Layup, Some(false)),
        "dunk" => styled(Style::Dunk, None),
        "hook" => styled(Style::Hook, None),
        "defreb" | "offreb" => rec
            .events
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::Rebound { offensive, .. } if offensive == (what == "offreb") => Some(e.time),
                _ => None,
            })
            .collect(),
        "ft" => rec
            .events
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::FreeThrows { .. } => Some(e.time - 3.0),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A camera fixed on the action at `time`: from the side, from behind the
/// shooter towards the basket, or close to the basket from the side.
pub(crate) fn viewpoint(
    rec: &Recording,
    time: f32,
    how: &str,
) -> Option<(super::Vec3, super::Vec3)> {
    use super::v3;
    let f = rec.frames.get(rec.frame_at(time + 0.3))?;
    let hoop = if f.offense == 0 { (26.425, 7.5) } else { (1.575, 7.5) };
    // The man the moment is about: whoever is shooting, jumping for a rebound
    // or holding the ball near it, else the ball itself.
    let mut who = None;
    for g in &rec.frames[rec.frame_at(time - 0.3)..=rec.frame_at(time + 0.4)] {
        for side in 0..2 {
            for k in 0..5 {
                let a = g.pose[side][k].action;
                if matches!(a, Action::Shooting | Action::Rebounding | Action::Blocking)
                    && who.is_none()
                {
                    who = Some(g.pos[side][k]);
                }
            }
        }
    }
    let (bx, by) = who.map_or((f.ball.x, f.ball.y), |p| (p.x, p.y));
    let toward = ((hoop.0 - bx), (hoop.1 - by));
    let len = (toward.0 * toward.0 + toward.1 * toward.1).sqrt().max(0.5);
    let (ux, uy) = (toward.0 / len, toward.1 / len);
    Some(match how {
        "behind" => (
            v3(bx - ux * 5.0, by - uy * 5.0 + 0.6, 2.6),
            v3(hoop.0, hoop.1, 2.3),
        ),
        "hoop" => (
            v3(hoop.0 - ux * 5.5, hoop.1 - 4.5, 3.6),
            v3(hoop.0, hoop.1, 3.0),
        ),
        "close" => (
            v3(bx - uy * 4.5 - ux * 1.0, by + ux * 4.5 - uy * 1.0, 2.4),
            v3(bx + ux * 0.5, by + uy * 0.5, 1.7),
        ),
        _ => {
            let mx = (bx + hoop.0) * 0.5;
            (v3(mx, -7.5, 4.0), v3(mx, by, 1.6))
        }
    })
}
