//! Bodies with a stance: three directions per player, legwork, balance and
//! what they cost a shot. Every check aggregates over several worlds and
//! many games; none picks a seed.

use super::ball::{self, Ball, v3};
use super::shot::{self, Errors};
use super::stance::{self, Skill};
use super::{Rules, simulate_game, simulate_recorded};
use crate::court::{HOOP, Vec2, v};
use crate::league::{World, lineup};
use crate::player::{Attr, Player};
use crate::replay::{
    Action, Dribble, EventKind, Footwork, Frame, Recording, ShotStage, Source, Style, to_court,
};
use crate::rng::Rng;

/// Recorded games from several worlds.
fn games(per_world: usize) -> Vec<Recording> {
    let mut out = Vec::new();
    for seed in [41u64, 42, 43] {
        let w = World::generate(seed, 12);
        for g in 0..per_world {
            let a = g % 12;
            let b = (g * 5 + 1 + g / 12) % 12;
            let b = if a == b { (b + 1) % 12 } else { b };
            let hl = lineup(&w.teams[a], &w.players);
            let al = lineup(&w.teams[b], &w.players);
            out.push(simulate_recorded(&hl, &al, &w.rules, &mut Rng::new(seed * 100 + g as u64)).1);
        }
    }
    out
}

fn angle_between(a: Vec2, b: Vec2) -> f32 {
    stance::angle(a.norm(), b.norm())
}

/// The same man in the next frame, found by who he is.
fn same(a: &Frame, b: &Frame, s: usize, k: usize) -> Option<usize> {
    let id = a.who[s][k];
    b.who[s].iter().position(|&w| w == id)
}

#[test]
fn no_shot_is_released_with_the_shoulders_off_the_rim() {
    let (mut shots, mut bad, mut worst) = (0, 0, 0.0f32);
    for rec in games(6) {
        for (i, f) in rec.frames.iter().enumerate() {
            let o = f.offense as usize;
            let rim = to_court(HOOP, o);
            for k in 0..5 {
                let p = &f.pose[o][k];
                let first = rec.frames.get(i.wrapping_sub(1)).is_none_or(|q| {
                    let q = &q.pose[o][k];
                    !(q.action == Action::Shooting && q.stage == ShotStage::Release)
                });
                if p.action != Action::Shooting || p.stage != ShotStage::Release || !first {
                    continue;
                }
                // A desperate heave is flagged by its source.
                let heave = rec.events.iter().any(|e| {
                    (e.time - f.time).abs() < 0.25
                        && matches!(e.kind, EventKind::Shot { source: Source::Forced, .. })
                });
                let pos = f.pos[o][k];
                if heave || (matches!(p.style, Style::Layup | Style::Dunk) && pos.dist(rim) < 1.3) {
                    continue;
                }
                let body = p.body_dir().expect("a body direction is recorded");
                let ang = angle_between(body, rim - pos);
                // A hook is side-on, a turnaround turns into it.
                let off = if p.style == Style::Hook { (ang - 1.5708).abs() } else { ang };
                shots += 1;
                worst = worst.max(off);
                if off > 0.85 {
                    bad += 1;
                }
            }
        }
    }
    assert!(shots > 500, "{shots} shots looked at");
    assert_eq!(bad, 0, "{bad} of {shots} shots released with shoulders more than 49 degrees off (worst {worst} rad)");
}

#[test]
fn the_body_turns_no_faster_than_it_can_and_a_sprinter_hardly_at_all() {
    let (mut frames, mut fast_sprint) = (0u32, 0u32);
    let mut worst_sprint = 0.0f32;
    for rec in games(3) {
        for pair in rec.frames.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if b.cut {
                continue;
            }
            let dt = b.time - a.time;
            for s in 0..2 {
                for k in 0..5 {
                    let Some(j) = same(a, b, s, k) else { continue };
                    if a.who[s][k] != b.who[s][j] {
                        continue;
                    }
                    let (Some(x), Some(y)) = (a.pose[s][k].body_dir(), b.pose[s][j].body_dir()) else {
                        continue;
                    };
                    // A change of possession flips the frame: no turn is meant.
                    if a.offense != b.offense {
                        continue;
                    }
                    let rate = angle_between(x, y) / dt;
                    frames += 1;
                    // The quickest feet, standing, turn about 14.5 rad/s (plus bytes).
                    assert!(rate < 15.4, "body turned {rate} rad/s at {}", a.time);
                    if a.pose[s][k].speed() > 6.0 {
                        worst_sprint = worst_sprint.max(rate);
                        if rate > 7.0 {
                            fast_sprint += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(frames > 100_000);
    assert_eq!(fast_sprint, 0, "a sprinting body spun at {worst_sprint} rad/s");
}

#[test]
fn the_head_stays_within_its_range_of_the_body() {
    let (mut frames, mut turned) = (0u32, 0u32);
    for rec in games(3) {
        for f in &rec.frames {
            for s in 0..2 {
                for k in 0..5 {
                    let p = &f.pose[s][k];
                    let (Some(b), Some(g)) = (p.body_dir(), p.gaze_dir()) else { continue };
                    frames += 1;
                    // 80 degrees, and a little for the bytes.
                    if b.dot(g) < stance::HEAD_COS - 0.03 {
                        turned += 1;
                    }
                    if g.dot(b) < 0.6 {
                        turned += 0; // looking well away is allowed
                    }
                }
            }
        }
    }
    assert!(frames > 100_000);
    assert_eq!(turned, 0, "{turned} of {frames} frames with the head more than 80 degrees round");
}

#[test]
fn speed_stays_inside_what_the_footwork_allows() {
    // The fastest anybody can run here is 4.6 + 0.16 * 20; sliding and
    // backpedalling are slower by their caps (with a tick or two of
    // slowing down from the run that came before).
    let top = 4.6 + 0.16 * 20.0;
    let (mut slide, mut back, mut slide_over, mut back_over, mut all_over) = (0u32, 0u32, 0u32, 0u32, 0u32);
    for rec in games(4) {
        for f in &rec.frames {
            for s in 0..2 {
                for k in 0..5 {
                    let p = &f.pose[s][k];
                    let sp = p.speed();
                    if sp > top + 0.5 {
                        all_over += 1;
                    }
                    match p.foot {
                        Footwork::Slide => {
                            slide += 1;
                            if sp > 0.82 * top + 0.2 {
                                slide_over += 1;
                            }
                        }
                        Footwork::Backpedal => {
                            back += 1;
                            if sp > 0.68 * top + 0.2 {
                                back_over += 1;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    assert!(slide > 5_000 && back > 1_000, "slides {slide} backpedals {back}");
    assert_eq!(all_over, 0);
    // Coming out of a run into a slide takes a few ticks: a few frames in a hundred.
    assert!(slide_over * 40 < slide, "{slide_over} of {slide} slide frames too fast");
    assert!(back_over * 40 < back, "{back_over} of {back} backpedal frames too fast");
}

#[test]
fn a_slide_is_recorded_when_a_defender_moves_sideways_to_his_body() {
    let (mut sideways, mut recorded, mut defenders) = (0u32, 0u32, 0u32);
    for rec in games(3) {
        for f in &rec.frames {
            let o = f.offense as usize;
            let d = 1 - o;
            for k in 0..5 {
                let p = &f.pose[d][k];
                let (Some(m), Some(b)) = (p.move_dir(), p.body_dir()) else { continue };
                defenders += 1;
                if p.speed() < 0.6 || matches!(p.foot, Footwork::Closeout | Footwork::Chop) {
                    continue;
                }
                let c = m.dot(b);
                if c.abs() < 0.3 {
                    sideways += 1;
                    if p.foot == Footwork::Slide {
                        recorded += 1;
                    }
                }
            }
        }
    }
    assert!(defenders > 100_000);
    assert!(sideways > 3_000, "only {sideways} sideways defender frames");
    assert_eq!(recorded, sideways, "a sideways step was not recorded as a slide");
}

#[test]
fn a_ball_handler_who_has_picked_up_stays_put_and_does_not_dribble_again() {
    let (mut pickups, mut moved, mut again, mut dribbles) = (0u32, 0u32, 0u32, 0u32);
    for rec in games(4) {
        // (side, k, where he picked up, he finished at the rim)
        let mut held: Option<(u8, u8, Vec2, bool)> = None;
        for f in &rec.frames {
            let Some((s, k)) = f.holder else {
                held = None;
                continue;
            };
            let p = &f.pose[s as usize][k as usize];
            let pos = f.pos[s as usize][k as usize];
            if p.dribble == Dribble::Live {
                dribbles += 1;
                assert_ne!(p.hand, 0, "a dribbling hand is recorded");
            }
            match held {
                Some((hs, hk, at, rim)) if (hs, hk) == (s, k) => {
                    if p.dribble == Dribble::Live {
                        again += 1;
                    }
                    // A finish carries him to the rim; anything else is a pivot and a plant.
                    let r = if rim { 6.0 } else { 1.8 };
                    if pos.dist(at) > r {
                        moved += 1;
                    }
                }
                _ => {
                    held = None;
                    if p.dribble == Dribble::PickedUp {
                        pickups += 1;
                        let near = [to_court(HOOP, 0), to_court(HOOP, 1)]
                            .iter()
                            .any(|r| r.dist(pos) < 7.0);
                        held = Some((s, k, pos, near && matches!(p.style, Style::Layup | Style::Dunk)));
                    }
                }
            }
        }
    }
    assert!(pickups > 500, "{pickups} pick-ups");
    assert!(dribbles > 10_000);
    assert_eq!(again, 0, "{again} frames of dribbling after he had picked up");
    assert_eq!(moved, 0, "{moved} frames with a picked-up handler away from his pivot");
}

// ---- the relations ---------------------------------------------------------

/// Make rate of a 5.5 m shot from a given launch error multiplier, through
/// the real launch and flight.
fn make_rate(mult: f32, n: usize) -> f32 {
    let mut rng = Rng::new(99);
    let from = v(0.0, 5.5);
    let start = ball::clear_start(v3(from.x, from.y - 0.2, 2.35));
    let d = from.dist(HOOP);
    let (lat, depth) = shot::base(Style::Jumper, false, d);
    let (ideal, _) = shot::ideal(start, Style::Jumper);
    let mut made = 0;
    for _ in 0..n {
        let e = Errors::at_rim(d, lat * mult * 1.14, depth * mult * 1.14);
        let launch = shot::perturb(ideal, e, &mut rng);
        made += ball::fly(Ball { p: start, v: launch }, &[]).made as usize;
    }
    made as f32 / n as f32
}

#[test]
fn a_shot_goes_in_less_the_further_the_body_is_from_square_and_set() {
    let n = 6000;
    let set = make_rate(stance::shot_multiplier(0.0, 0.0, 0.0, 1.0, 0.0, 1.0), n);
    let a30 = make_rate(stance::shot_multiplier(0.52, 0.0, 0.0, 1.0, 0.0, 1.0), n);
    let a60 = make_rate(stance::shot_multiplier(1.05, 0.0, 0.0, 1.0, 0.0, 1.0), n);
    let side = make_rate(stance::shot_multiplier(0.0, 0.0, 3.0, 1.0, 0.0, 1.0), n);
    let fade = make_rate(stance::shot_multiplier(0.0, -3.0, 0.0, 1.0, 0.0, 1.0), n);
    let wobbly = make_rate(stance::shot_multiplier(0.0, 0.0, 0.0, 0.5, 0.0, 1.0), n);
    let skilled = make_rate(stance::shot_multiplier(1.05, 0.0, 0.0, 1.0, 1.0, 1.0), n);
    assert!(set > a30 && a30 > a60, "{set} {a30} {a60}");
    assert!(set - a60 > 0.08, "turned 60 degrees only costs {}", set - a60);
    assert!(set > side + 0.02 && set > fade + 0.02 && set > wobbly + 0.02, "{set} {side} {fade} {wobbly}");
    // The matching skill takes a good part of it back.
    assert!(skilled > a60 + 0.03, "{skilled} against {a60}");
}

#[test]
fn quicker_feet_square_up_sooner_and_turn_faster() {
    let slow = Skill::new(4.0, 4.0, 10.0);
    let quick = Skill::new(16.0, 16.0, 10.0);
    for ang in [1.0f32, 2.0, 3.0] {
        assert!(stance::turn_time(&quick, ang, 0.0, 1.0) < stance::turn_time(&slow, ang, 0.0, 1.0));
    }
    // A sprinting body turns slowly, a man off balance too.
    assert!(stance::turn_time(&quick, 2.0, 1.0, 1.0) > 2.0 * stance::turn_time(&quick, 2.0, 0.0, 1.0));
    assert!(stance::turn_time(&quick, 2.0, 0.0, 0.3) > stance::turn_time(&quick, 2.0, 0.0, 1.0));
    // Sliding is slower than sprinting, backpedalling slower still, and footwork helps.
    for sk in [slow, quick] {
        assert!(stance::move_cap(&sk, 1.0) > stance::move_cap(&sk, 0.0));
        assert!(stance::move_cap(&sk, 0.0) > stance::move_cap(&sk, -1.0));
    }
    assert!(stance::move_cap(&quick, 0.0) > stance::move_cap(&slow, 0.0));
}

/// Points per 100 possessions the team `b` scores against team `a`, over
/// several worlds, when `change` is applied to every player of `a`.
fn conceded(change: &dyn Fn(&mut Player), games: usize) -> f32 {
    let (mut pts, mut poss) = (0.0f32, 0.0f32);
    for seed in [51u64, 52, 53, 54] {
        let w = World::generate(seed, 12);
        for g in 0..games {
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
            let r = simulate_game(&hl, &al, &Rules::fiba(), &mut Rng::new(seed * 1000 + g as u64));
            pts += r.score[1] as f32;
            poss += r.possessions[1] as f32;
        }
    }
    100.0 * pts / poss
}

#[test]
fn better_footwork_defends_better() {
    let good = conceded(&|p| p.attrs[Attr::Footwork] += 5.0, 48);
    let bad = conceded(&|p| p.attrs[Attr::Footwork] -= 5.0, 48);
    assert!(good < bad - 0.5, "defenders with quick feet concede {good}, slow feet {bad}");
}
