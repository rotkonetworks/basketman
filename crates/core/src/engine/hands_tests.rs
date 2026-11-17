//! Hands: every touch of the ball is a hand within reach of it; passes leave
//! the passer's hands, nobody passes to himself, the ball changes hands only
//! by a hand event, hands do not move faster than a hand can, and the
//! ratings that should show in the hands do. Aggregated over many games and
//! worlds, never over a picked seed.

use super::{BoxLine, Rules, simulate_game, simulate_recorded};
use crate::league::{World, lineup};
use crate::player::{Attr, Player};
use crate::replay::{EventKind, PassKind, PassOutcome, PokeResult, Recording, TurnoverKind};
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
            simulate_recorded(&hl, &al, &w.rules, &mut Rng::new(900 + g as u64)).1
        })
        .collect()
}

/// A hand's reach of the ball when it touches it: a pass is caught within
/// the catching radius (handoffs, hand to hand, a little more), and a
/// defender's hand has to be within the radius of hand and ball together.
#[test]
fn every_catch_deflection_poke_and_strip_has_a_hand_within_reach() {
    let (mut passes, mut touched, mut pokes) = (0, 0, 0);
    for rec in games(8) {
        for p in &rec.passes {
            passes += 1;
            match p.outcome {
                PassOutcome::Caught | PassOutcome::Bobbled => {
                    // Caught by a hand: it was on the ball, unless the ball
                    // came to a man by another way (a stray catch, a scramble).
                    if p.contact_cm > 0 {
                        touched += 1;
                        let limit = if p.kind == PassKind::Handoff { 56 } else { 33 };
                        assert!(p.contact_cm <= limit, "caught with a hand {} cm from the ball ({:?})", p.contact_cm, p.kind);
                    }
                }
                PassOutcome::Tipped | PassOutcome::Stolen => {
                    touched += 1;
                    assert!(p.touched_by.is_some(), "a deflection with no defender named");
                    assert!(p.contact_cm <= 25, "deflected by a hand {} cm from the ball", p.contact_cm);
                }
                _ => {}
            }
        }
        for k in &rec.pokes {
            pokes += 1;
            assert!(k.gap_cm <= 20, "a poke from {} cm", k.gap_cm);
            assert!(matches!(k.result, PokeResult::Taken | PokeResult::Loose | PokeResult::Foul));
        }
    }
    assert!(passes > 1500 && touched > 300 && pokes > 30, "{passes} passes, {touched} touched, {pokes} pokes");
}

/// Nobody passes to himself, and a pass starts where the passer's hands are:
/// within an arm and a step of his body, at a height a hand can be.
#[test]
fn no_pass_leaves_from_a_place_the_hands_were_not_and_nobody_passes_to_himself() {
    let mut checked = 0;
    for rec in games(6) {
        for p in &rec.passes {
            assert_ne!(p.from, p.to, "a pass to himself at {}", p.time);
            // Throw-ins start out of bounds in the thrower's hands: the man's
            // own position is where to look.
            let Some(f) = rec.frames.iter().rev().find(|f| f.time <= p.time + 0.001) else { continue };
            let Some(next) = rec.frames.iter().find(|f| f.time >= p.time && !f.cut) else { continue };
            let side = p.side as usize;
            let from = p.from as usize;
            let d = f.pos[side][from].dist(p.at).min(next.pos[side][from].dist(p.at));
            assert!(d < 1.9, "a {:?} pass left {d:.2} m from the passer's body at {}", p.kind, p.time);
            checked += 1;
        }
    }
    assert!(checked > 1500, "{checked}");
}

/// The ball changes hands only through a hand: a pass that was caught, a
/// poke or a strip, a rebound or a loose ball picked up, a throw-in. A man
/// who has it one frame and another man has it the next (with nothing loose
/// between) was handed it, stripped of it or fouled.
#[test]
fn the_ball_never_changes_hands_without_a_hand_event() {
    let (mut handoffs, mut checked) = (0, 0);
    for (g, rec) in games(12).into_iter().enumerate() {
        for pair in rec.frames.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if b.cut {
                continue;
            }
            let (Some(x), Some(y)) = (a.holder, b.holder) else { continue };
            if x == y {
                continue;
            }
            checked += 1;
            let t = b.time;
            // A pass thrown in the three seconds before (a lob and a scramble for the
            // ball it left loose take that long): handed, caught or recovered.
            let by_pass = rec.passes.iter().any(|p| p.time <= t && t - p.time < 3.0);
            let by_poke = rec.pokes.iter().any(|k| (k.time - t).abs() < 0.6);
            let by_event = rec.events.iter().any(|e| (e.time - t).abs() < 0.7 && matches!(e.kind, EventKind::Turnover { .. } | EventKind::Foul { .. } | EventKind::Rebound { .. }));
            assert!(by_pass || by_poke || by_event, "game {g}: the ball changed hands at {t} with no pass, poke or event near");
            handoffs += by_pass as u32;
        }
    }
    assert!(checked > 10, "{checked} changes of hands");
    assert!(handoffs > 0);
}

/// A hand does not move faster than the limit for hands: relative to his own
/// body, a recorded hand moves at most about eleven metres a second, and a
/// body turns the shoulders under it by up to about a metre a tick.
#[test]
fn hands_never_move_faster_than_their_limit() {
    let mut worst = 0.0f32;
    let mut frames = 0;
    let mut speeds: Vec<f32> = Vec::new();
    for rec in games(3) {
        for pair in rec.frames.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if b.cut {
                continue;
            }
            let dt = b.time - a.time;
            for side in 0..2 {
                for k in 0..5 {
                    let id = a.who[side][k];
                    let Some(j) = b.who[side].iter().position(|&w| w == id) else { continue };
                    for i in 0..2 {
                        let (h0, h1) = (a.pose[side][k].hands[i], b.pose[side][j].hands[i]);
                        if h0.z == 0 || h1.z == 0 {
                            continue;
                        }
                        let d = ((h1.dx - h0.dx) as f32 / 100.0).hypot((h1.dy - h0.dy) as f32 / 100.0);
                        let dz = (h1.z as f32 - h0.z as f32) / 100.0;
                        let speed = d.hypot(dz) / dt;
                        worst = worst.max(speed);
                        speeds.push(speed);
                        frames += 1;
                    }
                }
            }
        }
    }
    assert!(frames > 10_000, "{frames}");
    speeds.sort_by(|a, b| a.total_cmp(b));
    let p999 = speeds[speeds.len() * 999 / 1000];
    eprintln!("hand speed relative to the body: p99.9 {p999:.1}, worst {worst:.1} m/s");
    // 11 m/s of hand, plus the shoulder moving under it as he turns or jumps.
    assert!(p999 < 16.0, "one frame in a thousand has a hand moving at {p999:.1} m/s relative to its body");
    assert!(worst < 40.0, "a hand moved at {worst:.1} m/s relative to its body");
}

/// A hand reaches no further than the arm: relative to the body centre no
/// hand is more than a wingspan's half and a shoulder away, and it is never
/// lower than the floor or higher than a man can reach and jump.
#[test]
fn hands_stay_within_the_arm() {
    for rec in games(3) {
        for f in &rec.frames {
            for side in 0..2 {
                for k in 0..5 {
                    for h in f.pose[side][k].hands {
                        if h.z == 0 {
                            continue;
                        }
                        let off = (h.dx as f32 / 100.0).hypot(h.dy as f32 / 100.0);
                        assert!(off < 1.6, "a hand {off:.2} m from the middle of the body");
                        assert!(h.z < 360, "a hand at {} cm", h.z);
                    }
                }
            }
        }
    }
}

/// Totals of side 0's box and of the turnovers by kind that were its own.
fn totals(change: &dyn Fn(&mut Player), games_n: usize) -> (BoxLine, BoxLine, u32, u32, f32) {
    let (mut mine, mut theirs) = (BoxLine::default(), BoxLine::default());
    let (mut bad, mut strip) = (0u32, 0u32);
    let mut passes = 0.0f32;
    for seed in [31u64, 32, 33, 34] {
        let w = World::generate(seed, 12);
        for g in 0..games_n {
            let a = g % 12;
            let b = (g * 5 + 1 + g / 12) % 12;
            if a == b {
                continue;
            }
            let mut altered = w.players.clone();
            for &p in &w.teams[a].roster {
                change(&mut altered[p as usize]);
                for at in Attr::ALL {
                    altered[p as usize].attrs[at] = altered[p as usize].attrs[at].clamp(1.0, 20.0);
                }
            }
            let hl = lineup(&w.teams[a], &altered);
            let al = lineup(&w.teams[b], &w.players);
            let (r, rec) = simulate_recorded(&hl, &al, &Rules::fiba(), &mut Rng::new(seed * 1000 + g as u64));
            for (_, l) in &r.boxes[0] {
                mine.add(l);
            }
            for (_, l) in &r.boxes[1] {
                theirs.add(l);
            }
            let ids: Vec<u32> = r.boxes[0].iter().map(|(id, _)| *id as u32).collect();
            passes += rec.passes.iter().filter(|p| p.side == 0).count() as f32;
            for e in &rec.events {
                if let EventKind::Turnover { who, kind, .. } = &e.kind
                    && ids.contains(&(*who as u32))
                {
                    match kind {
                        TurnoverKind::BadPass => bad += 1,
                        TurnoverKind::Stripped => strip += 1,
                        _ => {}
                    }
                }
            }
        }
    }
    (mine, theirs, bad, strip, passes)
}

/// Passing and vision: more of his team's baskets are assisted, and a
/// pass is likelier to be completed.
#[test]
fn better_passers_have_more_assists_and_complete_more_passes() {
    let (lo, _, _, _, _) = totals(&|p| p.attrs[Attr::Passing] -= 5.0, 28);
    let (hi, _, _, _, _) = totals(&|p| p.attrs[Attr::Passing] += 5.0, 28);
    let (r_lo, r_hi) = (lo.ast as f32 / lo.fgm.max(1) as f32, hi.ast as f32 / hi.fgm.max(1) as f32);
    assert!(r_hi > r_lo + 0.05, "assists per made basket {r_lo:.3} for worse passers, {r_hi:.3} for better");
    // Completion: bad passes per pass thrown.
    let fails = |change: &dyn Fn(&mut Player)| {
        let (_, _, bad, _, passes) = totals(change, 28);
        bad as f32 / passes.max(1.0)
    };
    let (f_lo, f_hi) = (fails(&|p| p.attrs[Attr::Passing] -= 5.0), fails(&|p| p.attrs[Attr::Passing] += 5.0));
    assert!(f_hi < f_lo, "bad passes per pass: {f_lo:.4} for worse passers, {f_hi:.4} for better");
}

/// Handling: fewer lost balls on the dribble.
#[test]
fn better_ball_handlers_lose_fewer_dribbles() {
    let (_, _, _, s_lo, _) = totals(&|p| p.attrs[Attr::Handling] -= 5.0, 28);
    let (_, _, _, s_hi, _) = totals(&|p| p.attrs[Attr::Handling] += 5.0, 28);
    assert!(s_hi * 10 < s_lo * 8, "strips: {s_lo} for worse handlers, {s_hi} for better");
}

/// Steal: his team's defense takes the ball away more.
#[test]
fn better_thieves_get_more_steals() {
    let (lo, _, _, _, _) = totals(&|p| p.attrs[Attr::Steal] -= 5.0, 28);
    let (hi, _, _, _, _) = totals(&|p| p.attrs[Attr::Steal] += 5.0, 28);
    assert!(hi.stl as f32 > lo.stl as f32 * 1.3, "steals: {} for worse thieves, {} for better", lo.stl, hi.stl);
}

/// A game played on the instant path and one played for the camera are the
/// same game: the hands change nothing about what happens.
#[test]
fn the_hands_are_the_same_recorded_or_not() {
    let w = World::generate(7, 12);
    for g in 0..3 {
        let hl = lineup(&w.teams[g], &w.players);
        let al = lineup(&w.teams[g + 4], &w.players);
        let a = simulate_game(&hl, &al, &Rules::fiba(), &mut Rng::new(55 + g as u64));
        let (b, _) = simulate_recorded(&hl, &al, &Rules::fiba(), &mut Rng::new(55 + g as u64));
        assert_eq!(a.score, b.score);
        assert_eq!(a.possessions, b.possessions);
    }
}
