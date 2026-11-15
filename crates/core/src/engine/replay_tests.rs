//! What a recorded game looks like: continuous in time, bodies that do not
//! teleport, a ball that goes where the result says.

use super::simulate_recorded;
use crate::court::{
    BALL_RADIUS, BOARD_HALF, BOARD_HIGH, BOARD_LOW, BOARD_Y, HOOP, LENGTH, RIM_HEIGHT, RIM_RADIUS,
    Vec2, v,
};
use crate::league::{World, lineup};
use crate::replay::{EventKind, Recording, to_court};
use crate::rng::Rng;

fn games(n: usize) -> Vec<Recording> {
    let w = World::generate(7, 12);
    (0..n)
        .map(|g| {
            let a = g % 12;
            let b = (g * 5 + 1 + g / 12) % 12;
            let b = if a == b { (b + 1) % 12 } else { b };
            let hl = lineup(&w.teams[a], &w.players);
            let al = lineup(&w.teams[b], &w.players);
            simulate_recorded(&hl, &al, &w.rules, &mut Rng::new(300 + g as u64)).1
        })
        .collect()
}

/// The rim in court coordinates for the two baskets.
fn rims() -> [Vec2; 2] {
    [to_court(HOOP, 0), to_court(HOOP, 1)]
}

#[test]
fn time_runs_on_one_tick_at_a_time_and_nobody_teleports() {
    let step = 1.0 / crate::replay::FRAME_RATE;
    for (g, rec) in games(3).into_iter().enumerate() {
        let mut cuts = 0;
        for pair in rec.frames.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if b.cut {
                cuts += 1;
                continue;
            }
            let dt = b.time - a.time;
            assert!(
                (dt - step).abs() < 0.002,
                "gap of {dt} s at {} without a cut",
                a.time
            );
            for side in 0..2 {
                for k in 0..5 {
                    // The same man, found by who he is: court slots move on a
                    // substitution. A man who came on is where the one he
                    // replaced was.
                    let id = a.who[side][k];
                    let Some(j) = b.who[side].iter().position(|&w| w == id) else {
                        continue;
                    };
                    let d = a.pos[side][k].dist(b.pos[side][j]);
                    assert!(
                        d <= 12.0 * dt + 0.15,
                        "side {side} slot {k} moved {d} m in {dt} s at {}",
                        a.time
                    );
                }
            }
            // The ball does not jump either: a pass is quick but not instant.
            assert!(
                a.ball.dist(b.ball) < 2.5,
                "game {g}: ball moved {} m in a frame at {}",
                a.ball.dist(b.ball),
                a.time
            );
        }
        // Timeouts and the breaks between periods are the only cuts.
        assert!((3..60).contains(&cuts), "{cuts} cuts");
    }
}

/// Height at which a descending ball crosses `z`, between two frames, and
/// where, by straight interpolation.
fn crossing(a: (Vec2, f32), b: (Vec2, f32), z: f32) -> Option<Vec2> {
    if a.1 > z && b.1 <= z {
        let u = (a.1 - z) / (a.1 - b.1);
        Some(a.0.lerp(b.0, u))
    } else {
        None
    }
}

#[test]
fn every_make_goes_through_the_ring_and_no_miss_does() {
    let rims = rims();
    let (mut makes, mut misses) = (0, 0);
    for rec in games(6) {
        for e in &rec.events {
            let EventKind::Shot { made, .. } = &e.kind else {
                continue;
            };
            let i = rec.frame_at(e.time);
            // The shot is over within a few seconds; look for the ring.
            let rim = rims[e.offense as usize];
            let mut through = false;
            let mut inside_miss = false;
            let end = rec.frame_at(e.time + 4.0).min(rec.frames.len() - 1);
            for w in rec.frames[i..=end].windows(2) {
                if w[1].cut {
                    break;
                }
                let a = (w[0].ball, w[0].ball_height);
                let b = (w[1].ball, w[1].ball_height);
                if let Some(at) = crossing(a, b, RIM_HEIGHT) {
                    // Interpolation is straight, the ball is not: leave a margin.
                    if at.dist(rim) < RIM_RADIUS {
                        through = true;
                    } else if at.dist(rim) < RIM_RADIUS - 0.1 {
                        inside_miss = true;
                    }
                }
            }
            if *made {
                makes += 1;
                assert!(
                    through,
                    "a make at {} did not pass through the ring",
                    e.time
                );
            } else {
                misses += 1;
                assert!(!inside_miss);
            }
        }
    }
    assert!(makes > 100 && misses > 100, "{makes} makes {misses} misses");
}

#[test]
fn the_ball_never_goes_through_the_glass_or_the_floor() {
    for (g, rec) in games(4).into_iter().enumerate() {
        for pair in rec.frames.windows(2) {
            if pair[1].cut {
                continue;
            }
            for f in pair {
                assert!(
                    f.ball_height >= BALL_RADIUS - 0.02,
                    "ball under the floor: {} at {}",
                    f.ball_height,
                    f.time
                );
            }
            // Distance from the baseline, along the court, of each frame.
            let from_line = |p: Vec2| p.x.min(LENGTH - p.x);
            let across = |p: Vec2| (p.y - 7.5).abs();
            let (a, b) = (&pair[0], &pair[1]);
            let (da, db) = (from_line(a.ball), from_line(b.ball));
            // Same end of the court.
            if (a.ball.x < 14.0) != (b.ball.x < 14.0) {
                continue;
            }
            if da >= BOARD_Y && db < BOARD_Y {
                let u = (da - BOARD_Y) / (da - db);
                let at = a.ball.lerp(b.ball, u);
                let z = a.ball_height + (b.ball_height - a.ball_height) * u;
                let through_glass =
                    across(at) < BOARD_HALF - 0.05 && z > BOARD_LOW + 0.05 && z < BOARD_HIGH - 0.05;
                assert!(!through_glass, "game {g}: through the board at {}", a.time);
            }
        }
    }
}

#[test]
fn a_shooter_is_set_when_he_lets_go() {
    use crate::replay::{Action, ShotStage};
    let mut seen = 0;
    let mut moving = 0;
    for rec in games(3) {
        for (i, f) in rec.frames.iter().enumerate().skip(1) {
            let o = f.offense as usize;
            for k in 0..5 {
                let (p, q) = (&rec.frames[i - 1].pose[o][k], &f.pose[o][k]);
                if q.action == Action::Shooting
                    && q.stage == ShotStage::Release
                    && p.stage == ShotStage::Rise
                    && q.style == crate::replay::Style::Jumper
                {
                    seen += 1;
                    let speed = f.pos[o][k].dist(rec.frames[i - 1].pos[o][k]) / 0.1;
                    if speed > 1.5 {
                        moving += 1;
                    }
                }
            }
        }
    }
    assert!(seen > 50, "{seen} jump shots");
    assert!(
        moving * 20 < seen,
        "{moving} of {seen} released on the move"
    );
    let _ = v(0.0, 0.0);
}

#[test]
fn a_full_game_recording_stays_small() {
    let rec = games(1).remove(0);
    let bytes = rec.frames.len() * std::mem::size_of::<crate::replay::Frame>();
    println!(
        "{} frames of {} bytes: {:.1} MB",
        rec.frames.len(),
        std::mem::size_of::<crate::replay::Frame>(),
        bytes as f32 / 1e6
    );
    assert!(bytes < 16_000_000, "{bytes} bytes");
}

#[test]
fn a_rebounder_meets_the_ball_in_his_hands_at_the_height_his_leap_allows() {
    use crate::player::Attr;
    use crate::replay::{Action, Pose};
    let w = World::generate(7, 12);
    let mut seen = 0;
    let mut airborne = 0;
    for rec in games(4) {
        for e in &rec.events {
            let EventKind::Rebound { who, .. } = &e.kind else {
                continue;
            };
            let f = &rec.frames[rec.frame_at(e.time)];
            // He is the one holding it a tick later.
            let Some(g) = rec.frames.get(rec.frame_at(e.time) + 1) else {
                continue;
            };
            if g.cut {
                continue;
            }
            let Some((s, k)) = g.holder else { continue };
            let p = &w.players[*who as usize];
            let (s, k) = (s as usize, k as usize);
            seen += 1;
            let reach = p.body.standing_reach_cm / 100.0 + Pose::max_lift(p.attrs[Attr::Vertical]);
            assert!(
                f.ball_height <= reach + 0.05,
                "ball taken at {} m by a man who reaches {reach} m",
                f.ball_height
            );
            assert!(
                f.ball.dist(f.pos[s][k]) < 1.2,
                "ball {} m from the man who takes it at {}",
                f.ball.dist(f.pos[s][k]),
                e.time
            );
            if f.pose[s][k].action == Action::Rebounding && f.pose[s][k].lift_cm > 0 {
                airborne += 1;
            }
        }
    }
    assert!(seen > 80, "{seen} rebounds");
    assert!(
        airborne * 10 > seen * 8,
        "{airborne} of {seen} off the floor"
    );
}

#[test]
fn a_ball_is_only_taken_where_a_hand_can_reach_it() {
    let w = World::generate(7, 12);
    // Court slot to player: events name players, frames name box score slots;
    // the lineups are the same two clubs for the whole game, so find each
    // slot's player by its position in the frame list of `who`.
    let mut taken = 0;
    for (g, rec) in games(6).into_iter().enumerate() {
        let a = g % 12;
        let b = {
            let b = (g * 5 + 1 + g / 12) % 12;
            if a == b { (b + 1) % 12 } else { b }
        };
        let teams = [a, b];
        // Box score order is the lineup's order: players of each team, by slot.
        let slot_player = |side: usize, slot: u8| -> &crate::player::Player {
            let t = &w.teams[teams[side]];
            let l = lineup(t, &w.players);
            let id = l.players[slot as usize].id;
            &w.players[id as usize]
        };
        let reach_of = |side: usize, slot: u8, lift: f32| -> (f32, f32) {
            let p = slot_player(side, slot);
            (
                p.body.wingspan_cm / 200.0 + 0.25,
                p.body.standing_reach_cm / 100.0 + lift + 0.12,
            )
        };
        for i in 1..rec.frames.len() {
            let (f0, f1) = (&rec.frames[i - 1], &rec.frames[i]);
            if f1.cut {
                continue;
            }
            let Some((s, k)) = f1.holder else { continue };
            if f0.holder == f1.holder {
                continue;
            }
            // A new man has the ball: it was in his reach in the frame before
            // (or he was at the ball, if it was loose on the floor).
            let (s, k) = (s as usize, k as usize);
            let (out, up) = reach_of(s, f1.who[s][k], f1.pose[s][k].lift());
            let d = f0.ball.dist(f0.pos[s][k]).min(f1.ball.dist(f1.pos[s][k]));
            let z = f0.ball_height.min(f1.ball_height);
            taken += 1;
            assert!(
                d <= out + 0.35 && z <= up + 0.15,
                "game {g} at {}: ball taken {d:.2} m away (reach {out:.2}), {z:.2} m up (reach {up:.2})",
                f1.time
            );
        }
        // A blocker's hand meets the ball.
        for e in &rec.events {
            let EventKind::Shot {
                blocked_by: Some(_),
                ..
            } = &e.kind
            else {
                continue;
            };
            let i = rec.frame_at(e.time);
            let mut best = f32::MAX;
            for f in &rec.frames[i..(i + 6).min(rec.frames.len())] {
                let def = 1 - f.offense as usize;
                for k in 0..5 {
                    if f.pose[def][k].action == crate::replay::Action::Blocking {
                        let (out, up) = reach_of(def, f.who[def][k], f.pose[def][k].lift());
                        let d = f.ball.dist(f.pos[def][k]) - out;
                        if f.ball_height <= up + 0.15 {
                            best = best.min(d);
                        }
                    }
                }
            }
            assert!(
                best <= 0.5,
                "game {g}: a block at {} met nothing (miss by {best})",
                e.time
            );
        }
    }
    assert!(taken > 1000, "{taken} takes");
}
