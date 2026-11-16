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
            // The ball's own recorded path, a few milliseconds at a time (the
            // 10 Hz frames are too coarse to see a rattle-in): does it cross
            // the height of the ring going down, inside it?
            let rim = rims[e.offense as usize];
            let mut through = false;
            let mut last: Option<(Vec2, f32)> = None;
            let mut t = e.time;
            while t < e.time + 6.0 {
                let Some(now) = rec.ball_at(t) else {
                    if t > e.time + 0.3 {
                        break;
                    }
                    t += 0.002;
                    continue;
                };
                if let Some(before) = last
                    && let Some(at) = crossing(before, now, RIM_HEIGHT)
                    && at.dist(rim) < RIM_RADIUS
                {
                    through = true;
                }
                last = Some(now);
                t += 0.002;
            }
            if *made {
                makes += 1;
                assert!(through, "a make at {} did not pass through the ring", e.time);
            } else {
                misses += 1;
                assert!(!through, "a miss at {} passed through the ring", e.time);
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
    let (mut above, mut above_air) = (0, 0);
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
            // In a jump: for a board, a block or a contest (a man who swats a
            // shot and then takes it himself is still in his leap).
            let in_air = matches!(
                f.pose[s][k].action,
                Action::Rebounding | Action::Blocking | Action::Contesting
            ) && f.pose[s][k].lift_cm > 0;
            if in_air {
                airborne += 1;
            }
            // A ball above his standing reach can only be taken in a jump.
            if f.ball_height > p.body.standing_reach_cm / 100.0 + 0.02 {
                above += 1;
                if in_air {
                    above_air += 1;
                }
            }
        }
    }
    assert!(seen > 80, "{seen} rebounds");
    // Most boards are taken in the air; the rest are balls that came down
    // low (long rebounds, bounces) and were gathered on the floor.
    assert!(
        airborne * 10 > seen * 7,
        "{airborne} of {seen} off the floor"
    );
    assert!(
        above > 100 && above_air * 10 >= above * 9,
        "{above_air} of {above} balls above his reach were taken in a jump"
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






/// The recorded path of every shot is the ball's own: chains of arcs that
/// join up, a make is a descent through the ring and nothing else is, and
/// nothing ever passes into the iron, the glass or the floor.
#[test]
fn the_recorded_ball_path_is_the_physical_one() {
    use super::ball::{IRON, RING_RADIUS};
    let rims = rims();
    let (mut makes, mut misses, mut contacts) = (0, 0, 0);
    for rec in games(6) {
        assert!(!rec.ball_arcs.is_empty());
        // Sorted, and each arc starts where the one before it was cut short.
        for w in rec.ball_arcs.windows(2) {
            assert!(w[1].t0 >= w[0].t0 - 1e-3, "arcs out of order");
        }
        for e in &rec.events {
            let EventKind::Shot { made, blocked_by, .. } = &e.kind else {
                continue;
            };
            let rim = rims[e.offense as usize];
            // Sample the recorded path finely from the release for 6 s.
            let mut t = e.time;
            let mut last: Option<(Vec2, f32)> = None;
            let mut down_through = false;
            let mut iron_dive = false;
            while t < e.time + 6.0 {
                let Some((p, z)) = rec.ball_at(t) else {
                    // The ball was taken by a hand: the flight is over.
                    if t > e.time + 0.2 {
                        break;
                    }
                    t += 0.002;
                    continue;
                };
                assert!(z >= BALL_RADIUS - 0.01, "under the floor: {z}");
                // Never inside the iron: nearest point of the ring.
                let d = p.dist(rim);
                let ring = ((d - RING_RADIUS).powi(2) + (z - RIM_HEIGHT).powi(2)).sqrt();
                if d > 1e-3 && ring < BALL_RADIUS + IRON - 0.02 {
                    iron_dive = true;
                }
                if let Some((q, y)) = last {
                    if y > RIM_HEIGHT - 0.2 && z <= RIM_HEIGHT - 0.2 && q.dist(rim) < RING_RADIUS && p.dist(rim) < RING_RADIUS {
                        down_through = true;
                    }
                    // Per 2 ms the ball does not jump.
                    assert!(q.dist(p) < 0.08 && (y - z).abs() < 0.08, "ball jumps in a step at {t}");
                    if y > BALL_RADIUS + 0.05 && z <= BALL_RADIUS + 0.05 {
                        contacts += 1;
                    }
                }
                last = Some((p, z));
                t += 0.002;
            }
            assert!(!iron_dive, "ball inside the iron");
            if *made {
                makes += 1;
                assert!(down_through, "a make at {} did not go down through the ring", e.time);
            } else {
                misses += 1;
                assert!(!down_through, "a miss at {} went down through the ring", e.time);
            }
            let _ = blocked_by;
        }
    }
    assert!(makes > 100 && misses > 100 && contacts > 50, "{makes} {misses} {contacts}");
}




