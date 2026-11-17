//! Finding the moments that matter in a recording, for the screenshot harness:
//! the n-th made three, missed jumper, layup, dunk, block, rebound, steal or
//! free throw. Returns the replay time the moment starts at.

use basketman_core::court::ShotKind;
use basketman_core::replay::{Action, Dribble, EventKind, Footwork, Recording, ShotStage, Style};

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
                            .find(|e| {
                                e.time >= f.time - 0.01 && matches!(e.kind, EventKind::Shot { .. })
                            })
                            .is_some_and(
                                |e| matches!(e.kind, EventKind::Shot { made, .. } if made == m),
                            )
                    });
                    if ok {
                        out.push(f.time);
                    }
                }
            }
        }
        out
    };
    // The start of every run of `min` frames in which somebody does `pred`.
    let streak = |pred: &dyn Fn(&basketman_core::replay::Frame, usize, usize) -> bool,
                  min: usize|
     -> Vec<f32> {
        let mut out = Vec::new();
        let mut run = [[0usize; 5]; 2];
        for f in &rec.frames {
            for s in 0..2 {
                for k in 0..5 {
                    if pred(f, s, k) {
                        run[s][k] += 1;
                        if run[s][k] == min {
                            out.push(f.time - 0.1 * min as f32);
                        }
                    } else {
                        run[s][k] = 0;
                    }
                }
            }
        }
        out
    };
    // Moments of the hands: the sequence starts four tenths before the pass.
    let passes = |pred: &dyn Fn(&basketman_core::replay::PassRec, &Recording) -> bool| -> Vec<f32> {
        rec.passes.iter().filter(|p| pred(p, rec)).map(|p| (p.time - 0.4).max(0.0)).collect()
    };
    let hoop_of = |rec: &Recording, p: &basketman_core::replay::PassRec| -> (f32, f32) {
        let f = &rec.frames[rec.frame_at(p.time)];
        if f.offense == 0 { (26.425, 7.5) } else { (1.575, 7.5) }
    };
    match what {
        "pass" => passes(&|p, _| !p.inbound && p.outcome == basketman_core::replay::PassOutcome::Caught && p.kind != basketman_core::replay::PassKind::Handoff),
        "deflect" => passes(&|p, _| matches!(p.outcome, basketman_core::replay::PassOutcome::Tipped | basketman_core::replay::PassOutcome::Stolen)),
        "bounce" => passes(&|p, _| p.kind == basketman_core::replay::PassKind::Bounce && p.outcome == basketman_core::replay::PassOutcome::Caught),
        "lob" => passes(&|p, _| p.kind == basketman_core::replay::PassKind::Lob && p.outcome == basketman_core::replay::PassOutcome::Caught),
        // A pass to a man at the rim.
        "rimpass" => passes(&|p, rec| {
            let h = hoop_of(rec, p);
            let g = &rec.frames[rec.frame_at(p.time + 0.9)];
            !p.inbound && g.pos[p.side as usize][p.to as usize].dist(basketman_core::court::Vec2 { x: h.0, y: h.1 }) < 3.0
        }),
        // Out of the paint to a man on the arc.
        "kickout" => passes(&|p, rec| {
            let h = hoop_of(rec, p);
            let g = &rec.frames[rec.frame_at(p.time + 0.9)];
            let hp = basketman_core::court::Vec2 { x: h.0, y: h.1 };
            !p.inbound && p.at.dist(hp) < 3.6 && g.pos[p.side as usize][p.to as usize].dist(hp) > 6.0
        }),
        // Within three seconds of a ball screen.
        "pnrpass" => passes(&|p, rec| {
            rec.events.iter().any(|e| matches!(e.kind, EventKind::Screen { .. }) && p.time > e.time && p.time - e.time < 3.0)
        }),
        // A swing, a swing, and a three from the corner: the second of two
        // passes in a chain, the last to a man in a corner who shoots it
        // within a second of the catch. The moment starts before the first.
        "swingcorner" => {
            let in_corner = |p: basketman_core::court::Vec2| (p.x < 4.5 || p.x > 23.5) && (p.y - 7.5).abs() > 5.0;
            let mut out = Vec::new();
            for e in &rec.events {
                let EventKind::Shot { kind: ShotKind::Three, .. } = &e.kind else { continue };
                let side = e.offense;
                let f = &rec.frames[rec.frame_at(e.time)];
                let before: Vec<&basketman_core::replay::PassRec> = rec.passes.iter().filter(|p| !p.inbound && p.side == side && p.time < e.time && e.time - p.time < 7.0).collect();
                let Some((&p2, rest)) = before.split_last() else { continue };
                if e.time - p2.time > 2.2 || !in_corner(f.pos[side as usize][p2.to as usize]) {
                    continue;
                }
                if let Some(p1) = rest.iter().rev().find(|q| q.to == p2.from && p2.time - q.time < 4.5) {
                    out.push((p1.time - 0.4).max(0.0));
                }
            }
            out
        }
        // A man cutting to the basket, thrown the ball: five metres out and
        // running at the rim at three metres a second when it leaves, within
        // three of it a second on.
        "backdoor" => passes(&|p, rec| {
            let h = hoop_of(rec, p);
            let hp = basketman_core::court::Vec2 { x: h.0, y: h.1 };
            let i = rec.frame_at(p.time);
            let (a, b) = (&rec.frames[i.saturating_sub(3)], &rec.frames[i]);
            let (q0, q1) = (a.pos[p.side as usize][p.to as usize], b.pos[p.side as usize][p.to as usize]);
            let toward = (q0.dist(hp) - q1.dist(hp)) / (b.time - a.time).max(0.05);
            let g = &rec.frames[rec.frame_at(p.time + 0.9)];
            !p.inbound && q1.dist(hp) > 4.5 && toward > 2.5 && g.pos[p.side as usize][p.to as usize].dist(hp) < 3.0
        }),
        // A ball screen, then a pass to the man who rolled to the rim.
        "roller" => passes(&|p, rec| {
            let h = hoop_of(rec, p);
            let hp = basketman_core::court::Vec2 { x: h.0, y: h.1 };
            let g = &rec.frames[rec.frame_at(p.time + 0.9)];
            !p.inbound
                && g.pos[p.side as usize][p.to as usize].dist(hp) < 3.0
                && rec.events.iter().any(|e| matches!(e.kind, EventKind::Screen { .. }) && p.time > e.time && p.time - e.time < 4.0)
        }),
        "poke" => rec.pokes.iter().map(|k| (k.time - 0.6).max(0.0)).collect(),
        "pokeaway" => rec.pokes.iter().filter(|k| k.result == basketman_core::replay::PokeResult::Loose).map(|k| (k.time - 0.6).max(0.0)).collect(),
        // A defender slides with the ball handler, who is dribbling.
        "slide" => streak(
            &|f, s, k| {
                let Some((hs, hk)) = f.holder else { return false };
                s != hs as usize
                    && f.pose[s][k].foot == Footwork::Slide
                    && f.pose[s][k].speed() > 1.5
                    && f.pose[hs as usize][hk as usize].dribble == Dribble::Live
                    && f.pos[s][k].dist(f.pos[hs as usize][hk as usize]) < 2.5
            },
            8,
        ),
        "closeout" => streak(&|f, s, k| f.pose[s][k].foot == Footwork::Closeout, 5),
        "backpedal" => streak(
            &|f, s, k| f.pose[s][k].foot == Footwork::Backpedal && f.pose[s][k].speed() > 2.0,
            6,
        ),
        "post" => streak(&|f, s, k| f.pose[s][k].action == Action::PostingUp, 12),
        "pivot" => streak(&|f, s, k| f.pose[s][k].foot == Footwork::Pivot, 4),
        // A catch and a shot with no dribble in between.
        "catchshoot" => {
            let mut out = Vec::new();
            for (i, f) in rec.frames.iter().enumerate().skip(1) {
                let o = f.offense as usize;
                for k in 0..5 {
                    let (p, q) = (&rec.frames[i - 1].pose[o][k], &f.pose[o][k]);
                    if q.action == Action::Shooting
                        && q.stage == ShotStage::Release
                        && p.stage != ShotStage::Release
                        && q.style == Style::Jumper
                        && i >= 12
                        && rec.frames[i - 12..i].iter().all(|g| g.pose[o][k].dribble != Dribble::Live)
                        && rec.frames[i - 12..i].iter().any(|g| g.holder.is_none())
                    {
                        out.push(f.time - 1.0);
                    }
                }
            }
            out
        }
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
                EventKind::Rebound { offensive, .. } if offensive == (what == "offreb") => {
                    Some(e.time)
                }
                _ => None,
            })
            .collect(),
        // The ball is picked off or stripped and the other team has it.
        "steal" => rec
            .events
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::Turnover {
                    stolen_by: Some(_), ..
                } => Some(e.time),
                _ => None,
            })
            .collect(),
        // A whistle: the first frame of each run with an arm up.
        "whistle" => rec
            .frames
            .windows(2)
            .filter(|w| w[1].whistler > 0 && w[0].whistler == 0)
            .map(|w| w[1].time)
            .collect(),
        // A challenge, as it was made.
        "review" => rec
            .events
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::Review { .. } => Some(e.time),
                _ => None,
            })
            .collect(),
        // A catch that is turned to the rim before the shot (the body well off
        // the line to the basket in the second before the release).
        "turnshoot" => {
            let mut out = Vec::new();
            for (i, f) in rec.frames.iter().enumerate().skip(12) {
                let o = f.offense as usize;
                for k in 0..5 {
                    let (p, q) = (&rec.frames[i - 1].pose[o][k], &f.pose[o][k]);
                    if q.action == Action::Shooting
                        && q.stage == ShotStage::Release
                        && p.stage != ShotStage::Release
                        && q.style == Style::Jumper
                        && rec.frames[i - 12..i].iter().any(|g| g.holder.is_none())
                    {
                        let rim = basketman_core::replay::to_court(basketman_core::court::HOOP, o);
                        let g = &rec.frames[i - 10];
                        if let Some(b) = g.pose[o][k].body_dir()
                            && b.dot((rim - g.pos[o][k]).norm()) < 0.3
                        {
                            out.push(f.time - 1.0);
                        }
                    }
                }
            }
            out
        }
        // A finish with a defender running him down from behind.
        "chase" => {
            let mut out = Vec::new();
            for (i, f) in rec.frames.iter().enumerate().skip(6) {
                let o = f.offense as usize;
                let rim = basketman_core::replay::to_court(basketman_core::court::HOOP, o);
                for k in 0..5 {
                    let (p, q) = (&rec.frames[i - 1].pose[o][k], &f.pose[o][k]);
                    if q.action == Action::Shooting
                        && q.stage == ShotStage::Release
                        && p.stage != ShotStage::Release
                        && matches!(q.style, Style::Layup | Style::Floater)
                    {
                        let me = f.pos[o][k];
                        let chaser = (0..5).any(|d| {
                            let c = f.pos[1 - o][d];
                            c.dist(me) < 2.5 && c.dist(rim) > me.dist(rim) + 0.2 && f.pose[1 - o][d].speed() > 3.0
                        });
                        if chaser {
                            out.push(f.time - 0.6);
                        }
                    }
                }
            }
            out
        }
        // The man who lost the ball turns and runs back.
        "getback" => rec
            .events
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::Turnover { stolen_by: Some(_), .. } => Some(e.time - 0.2),
                _ => None,
            })
            .collect(),
        // A throw-in.
        "inbound" => rec.passes.iter().filter(|p| p.inbound).map(|p| p.time - 1.4).collect(),
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
    follow_or_fixed(rec, time, time, how, "")
}

/// Who a moment of kind `what` is about, at `time`: a player `(side, k)` or
/// an official `(2, i)`.
fn subject(rec: &Recording, what: &str, time: f32) -> Option<(usize, usize)> {
    let i = rec.frame_at(time + 0.4);
    let f = rec.frames.get(i)?;
    match what {
        "whistle" => {
            let f = rec.frames.get(rec.frame_at(time + 0.1))?;
            (f.whistler > 0).then(|| (2, f.whistler as usize - 1))
        }
        // The man who had the ball when it was lost, a moment before.
        "getback" => {
            let f = rec.frames.get(rec.frame_at(time + 0.05))?;
            f.holder.map(|(s, k)| (s as usize, k as usize))
        }
        "pass" | "deflect" | "bounce" | "lob" | "rimpass" | "kickout" | "pnrpass" | "swingcorner" | "backdoor" | "roller" => rec
            .passes
            .iter()
            .find(|p| (p.time - 0.4 - time).abs() < 0.011)
            .map(|p| (p.side as usize, p.from as usize)),
        "poke" | "pokeaway" => rec
            .pokes
            .iter()
            .find(|k| (k.time - 0.6 - time).abs() < 0.011)
            .map(|k| (k.side as usize, k.handler as usize)),
        "inbound" => rec.passes.iter().find(|p| p.inbound && (p.time - 1.4 - time).abs() < 0.01).map(|p| (p.side as usize, p.from as usize)),
        "chase" => {
            let f = rec.frames.get(rec.frame_at(time + 0.6))?;
            let o = f.offense as usize;
            (0..5).find(|&k| f.pose[o][k].action == Action::Shooting).map(|k| (o, k))
        }
        _ => {
            let wanted = |a: Action, legs: Footwork| match what {
                "slide" => legs == Footwork::Slide,
                "closeout" => legs == Footwork::Closeout,
                "backpedal" => legs == Footwork::Backpedal,
                "pivot" => legs == Footwork::Pivot,
                "post" => a == Action::PostingUp,
                "defreb" | "offreb" => a == Action::Rebounding,
                "block" => a == Action::Blocking,
                _ => a == Action::Shooting,
            };
            // Shots are looked for a second on from the start of the moment.
            let (from, to) = match what {
                "turnshoot" | "catchshoot" => (time + 0.6, time + 1.6),
                _ => (time - 0.3, time + 0.4),
            };
            for g in &rec.frames[rec.frame_at(from)..=rec.frame_at(to)] {
                for side in 0..2 {
                    for k in 0..5 {
                        if wanted(g.pose[side][k].action, g.pose[side][k].foot) {
                            return Some((side, k));
                        }
                    }
                }
            }
            let _ = f;
            None
        }
    }
}

/// `SHOT_VIEWPOINT=follow`: the camera stays level with one man, side-on to
/// the line from him to the basket, at a fixed distance, whatever the replay
/// time `at` is. The man is the one the moment of kind `what` found at `time`.
fn follow_or_fixed(
    rec: &Recording,
    time: f32,
    at: f32,
    how: &str,
    what: &str,
) -> Option<(super::Vec3, super::Vec3)> {
    use super::v3;
    // Over the shoulder: behind and above the man, looking where he looks,
    // for a crowd that blocks the side-on view.
    if how == "shoulder" {
        let (side, k) = subject(rec, what, time)?;
        let f = rec.frames.get(rec.frame_at(at))?;
        let hoop = if f.offense == 0 { (26.425, 7.5) } else { (1.575, 7.5) };
        let p = f.pos[side][k];
        let body = f.pose[side][k].body_dir().map_or((hoop.0 - p.x, hoop.1 - p.y), |d| (d.x, d.y));
        let len = (body.0 * body.0 + body.1 * body.1).sqrt().max(0.2);
        let (ux, uy) = (body.0 / len, body.1 / len);
        let eye = v3(p.x - ux * 2.3 - uy * 0.5, p.y - uy * 2.3 + ux * 0.5, 3.1);
        return Some((eye, v3(p.x + ux * 3.0, p.y + uy * 3.0, 1.2)));
    }
    if how == "follow" {
        let (side, k) = subject(rec, what, time)?;
        let f0 = rec.frames.get(rec.frame_at(time + 0.3))?;
        let f = rec.frames.get(rec.frame_at(at))?;
        let hoop = if f0.offense == 0 { (26.425, 7.5) } else { (1.575, 7.5) };
        let p = if side == 2 { f.officials[k] } else { f.pos[side][k] };
        // Whose man, by his id, in case he was substituted: the court slot.
        let toward = (hoop.0 - p.x, hoop.1 - p.y);
        let len = (toward.0 * toward.0 + toward.1 * toward.1).sqrt().max(0.5);
        let (ux, uy) = (toward.0 / len, toward.1 / len);
        // Side-on, 6 m out, on whichever side has fewer bodies in the way.
        let blocked = |sign: f32| -> usize {
            let p0 = if side == 2 { f0.officials[k] } else { f0.pos[side][k] };
            let e = (p0.x + sign * uy * 6.0, p0.y - sign * ux * 6.0);
            let mut n = 0;
            for sd in 0..2 {
                for q in 0..5 {
                    if sd == side && q == k {
                        continue;
                    }
                    let b = f0.pos[sd][q];
                    // Distance of `b` from the segment eye to him.
                    let (dx, dy) = (p0.x - e.0, p0.y - e.1);
                    let l2 = (dx * dx + dy * dy).max(0.01);
                    let u = (((b.x - e.0) * dx + (b.y - e.1) * dy) / l2).clamp(0.0, 1.0);
                    let (cx, cy) = (e.0 + u * dx, e.1 + u * dy);
                    if (b.x - cx).hypot(b.y - cy) < 0.9 {
                        n += 1;
                    }
                }
            }
            n
        };
        let sign = if blocked(-1.0) < blocked(1.0) { -1.0 } else { 1.0 };
        let eye = v3(p.x + sign * uy * 6.0, p.y - sign * ux * 6.0, 2.2);
        return Some((eye, v3(p.x, p.y, 1.0)));
    }
    viewpoint_fixed(rec, time, how)
}

pub(crate) fn viewpoint_at(
    rec: &Recording,
    time: f32,
    at: f32,
    how: &str,
    what: &str,
) -> Option<(super::Vec3, super::Vec3)> {
    follow_or_fixed(rec, time, at, how, what)
}

fn viewpoint_fixed(
    rec: &Recording,
    time: f32,
    how: &str,
) -> Option<(super::Vec3, super::Vec3)> {
    use super::v3;
    let f = rec.frames.get(rec.frame_at(time + 0.3))?;
    let hoop = if f.offense == 0 {
        (26.425, 7.5)
    } else {
        (1.575, 7.5)
    };
    // The man the moment is about: whoever is shooting, jumping for a rebound
    // or holding the ball near it, else the ball itself.
    let mut who = None;
    for g in &rec.frames[rec.frame_at(time - 0.3)..=rec.frame_at(time + 0.4)] {
        for side in 0..2 {
            for k in 0..5 {
                let a = g.pose[side][k].action;
                let legs = g.pose[side][k].foot;
                if (matches!(a, Action::Shooting | Action::Rebounding | Action::Blocking | Action::PostingUp)
                    || matches!(legs, Footwork::Slide | Footwork::Closeout | Footwork::Backpedal | Footwork::Pivot))
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
        // The whole floor from the side, high enough to see the lanes: a
        // break is five against five along twenty-eight metres.
        "broadcast" => (v3(14.0, -15.0, 11.0), v3(14.0, 7.5, 0.0)),
        "behind" => (
            v3(bx - ux * 5.0, by - uy * 5.0 + 0.6, 2.6),
            v3(hoop.0, hoop.1, 2.3),
        ),
        "hoop" => (
            v3(hoop.0 - ux * 5.5, hoop.1 - 4.5, 3.6),
            v3(hoop.0, hoop.1, 3.0),
        ),
        // Further out and square to the line to the basket: sliding, closeouts, backpedals.
        "side" => (
            v3(bx - uy * 5.0, by + ux * 5.0, 2.0),
            v3((bx + f.ball.x) * 0.5, (by + f.ball.y) * 0.5, 0.9),
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
