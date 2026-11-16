//! The break: what a team that has just taken the ball does with it, and what
//! the team that lost it does about it. Everything is read off recorded games
//! (positions every tenth of a second, the event log), aggregated over several
//! worlds so that no lucky seed decides.

use super::{Rules, simulate_recorded};
use crate::court::{LENGTH, ShotKind, Vec2};
use crate::league::{World, lineup};
use crate::player::{Attr, Player};
use crate::replay::{EventKind, Recording, Source};
use crate::rng::Rng;
use crate::tactics::Tactics;

/// Distance of a court position from the basket team `o` shoots at, along the floor.
fn up(o: u8, p: Vec2) -> f32 {
    if o == 0 { LENGTH - p.x } else { p.x }
}

/// Every game of `n` pairings in each of `worlds`, with `change` applied to
/// the players of the club that plays as side 0 and `tune` to its tactics.
fn recorded(
    worlds: &[u64],
    n: usize,
    change: &dyn Fn(&mut Player),
    tune: &dyn Fn(&mut Tactics),
) -> Vec<Recording> {
    let mut out = Vec::new();
    for &seed in worlds {
        let w = World::generate(seed, 12);
        for g in 0..n {
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
            let mut ta = w.teams[a].tactics().clone();
            tune(&mut ta);
            let mut hl = lineup(&w.teams[a], &altered);
            hl.tactics = &ta;
            let al = lineup(&w.teams[b], &w.players);
            out.push(simulate_recorded(&hl, &al, &Rules::fiba(), &mut Rng::new(seed * 1000 + g as u64)).1);
        }
    }
    out
}

fn stock() -> Vec<Recording> {
    recorded(&[51, 52, 53, 54], 40, &|_| {}, &|_| {})
}

/// A steal that the other team ran with: when the ball crossed half court, how
/// many attackers (the man with the ball included) were level with it or
/// ahead, how many defenders were goal side of it, and whether a shot at the
/// rim came within six seconds of the crossing.
struct Run {
    attackers: usize,
    defenders: usize,
    rim_in_six: bool,
    shot_in_six: bool,
}

fn steal_runs(rec: &Recording) -> Vec<Run> {
    let mut out = Vec::new();
    for e in &rec.events {
        let EventKind::Turnover {
            stolen_by: Some(_), ..
        } = e.kind
        else {
            continue;
        };
        let Some(i0) = rec
            .frames
            .iter()
            .position(|f| f.time > e.time && f.offense != e.offense)
        else {
            continue;
        };
        let o = rec.frames[i0].offense;
        // Until the ball is lost again or it is dead: the next change of offense.
        let end = rec.frames[i0..]
            .iter()
            .position(|f| f.offense != o)
            .map_or(rec.frames.len(), |n| i0 + n);
        let Some(c) = rec.frames[i0..end]
            .iter()
            .position(|f| up(o, f.ball) <= 14.0)
        else {
            continue;
        };
        let f = &rec.frames[i0 + c];
        let by = up(o, f.ball);
        let attackers = 1 + (0..5)
            .filter(|&k| {
                f.holder != Some((o, k as u8)) && up(o, f.pos[o as usize][k]) < by + 1.0
            })
            .count();
        let defenders = (0..5)
            .filter(|&k| up(o, f.pos[1 - o as usize][k]) < by + 0.4)
            .count();
        let shots: Vec<(ShotKind, f32)> = rec
            .events
            .iter()
            .filter(|s| s.offense == o && s.time > f.time && s.time <= f.time + 6.0)
            .filter_map(|s| match &s.kind {
                EventKind::Shot { kind, .. } => Some((*kind, s.time)),
                _ => None,
            })
            .collect();
        out.push(Run {
            attackers,
            defenders,
            rim_in_six: shots.first().is_some_and(|s| s.0 == ShotKind::Rim),
            shot_in_six: !shots.is_empty(),
        });
    }
    out
}

#[test]
fn a_steal_with_the_numbers_usually_ends_at_the_rim_within_six_seconds() {
    let (mut n, mut rim, mut shot, mut all, mut with) = (0, 0, 0, 0, 0);
    for rec in stock() {
        for r in steal_runs(&rec) {
            all += 1;
            // Two more of them than of us, or one against nobody.
            if r.attackers >= r.defenders + 2 || (r.attackers >= 1 && r.defenders == 0) {
                n += 1;
                rim += r.rim_in_six as u32;
                shot += r.shot_in_six as u32;
            }
            with += (r.attackers > r.defenders) as u32;
        }
    }
    println!("{all} steals crossed half court, {with} with the numbers, {n} with a clear advantage: {rim} shots at the rim in six seconds, {shot} shots");
    assert!(n > 100, "{n} clear advantages in {all} steals");
    assert!(
        rim as f32 > 0.6 * n as f32,
        "{rim} of {n} clear numbers advantages ended at the rim within six seconds"
    );
}

/// Seconds-weighted mean speed of the five men of the team that stole the ball
/// in the three seconds that follow, and the fastest any man was in a frame.
#[test]
fn after_a_steal_they_run_and_nobody_runs_faster_than_he_can() {
    let (mut sum, mut n, mut fastest) = (0.0f32, 0u32, 0.0f32);
    let (mut over, mut frames) = (0u32, 0u32);
    for rec in stock() {
        for e in &rec.events {
            let EventKind::Turnover {
                stolen_by: Some(_), ..
            } = e.kind
            else {
                continue;
            };
            let Some(i0) = rec
                .frames
                .iter()
                .position(|f| f.time > e.time && f.offense != e.offense)
            else {
                continue;
            };
            let o = rec.frames[i0].offense as usize;
            for i in i0 + 1..(i0 + 60).min(rec.frames.len()) {
                let (a, b) = (&rec.frames[i - 1], &rec.frames[i]);
                if b.cut || b.whistle || b.offense as usize != o {
                    break;
                }
                for side in 0..2 {
                    for k in 0..5 {
                        let id = a.who[side][k];
                        let Some(j) = b.who[side].iter().position(|&w| w == id) else {
                            continue;
                        };
                        let speed = a.pos[side][k].dist(b.pos[side][j]) / (b.time - a.time);
                        over += (speed > 8.4) as u32;
                        frames += 1;
                        fastest = fastest.max(speed);
                        if side == o && i < i0 + 31 {
                            sum += speed;
                            n += 1;
                        }
                    }
                }
            }
        }
    }
    let mean = sum / n.max(1) as f32;
    println!("mean speed of the stealing team in the 3 s after a steal {mean:.2} m/s, fastest frame {fastest:.2} m/s, {over} of {frames} player-frames above 8.4 m/s");
    // The best sprinter in the league tops out at about 4.6 + 0.16 * 20 m/s
    // (a little more on a very good night); a body that is shoved by a
    // collision can add a little for a frame, which is the rare one.
    assert!(over * 2000 < frames, "{over} of {frames} player-frames faster than a sprinter");
    assert!(fastest < 10.0, "somebody covered ground at {fastest:.2} m/s");
    assert!(mean > 3.2, "the team that stole it moved at {mean:.2} m/s on average");
}

/// Points scored on the break by side 0 and by side 1, and the offensive boards
/// of side 0, per game; and how many possessions side 0 ran a break in.
#[derive(Default, Debug)]
struct Break {
    games: f32,
    scored: f32,
    conceded: f32,
    orb: f32,
    chances: f32,
}

fn tally(recs: &[Recording]) -> Break {
    let mut b = Break::default();
    for rec in recs {
        b.games += 1.0;
        for e in &rec.events {
            match &e.kind {
                EventKind::Shot {
                    made,
                    kind,
                    source: Source::Transition,
                    ..
                } => {
                    if e.offense == 0 {
                        b.chances += 1.0;
                    }
                    if *made {
                        let pts = kind.points() as f32;
                        if e.offense == 0 {
                            b.scored += pts;
                        } else {
                            b.conceded += pts;
                        }
                    }
                }
                EventKind::Rebound {
                    offensive: true, ..
                } if e.offense == 0 => b.orb += 1.0,
                _ => {}
            }
        }
    }
    b
}

fn per_game(x: f32, b: &Break) -> f32 {
    x / b.games
}

#[test]
fn a_faster_team_scores_more_in_transition() {
    let quick = tally(&recorded(&[61, 62, 63, 64], 40, &|p| p.attrs[Attr::Speed] += 4.0, &|_| {}));
    let slow = tally(&recorded(&[61, 62, 63, 64], 40, &|p| p.attrs[Attr::Speed] -= 4.0, &|_| {}));
    println!(
        "transition points a game: speed +4 {:.2} against -4 {:.2} (shots {:.2} / {:.2}; conceded {:.2} / {:.2})",
        per_game(quick.scored, &quick),
        per_game(slow.scored, &slow),
        per_game(quick.chances, &quick),
        per_game(slow.chances, &slow),
        per_game(quick.conceded, &quick),
        per_game(slow.conceded, &slow),
    );
    assert!(quick.scored > slow.scored * 1.08, "{:?} against {:?}", quick, slow);
}

#[test]
fn a_team_told_to_get_back_concedes_fewer_break_points_and_gets_fewer_offensive_boards() {
    let crash = tally(&recorded(&[71, 72, 73, 74], 40, &|_| {}, &|t| t.crash_boards = 19));
    let back = tally(&recorded(&[71, 72, 73, 74], 40, &|_| {}, &|t| t.crash_boards = 2));
    println!(
        "told to crash / get back: break points conceded {:.2} / {:.2}; offensive boards {:.2} / {:.2}",
        per_game(crash.conceded, &crash),
        per_game(back.conceded, &back),
        per_game(crash.orb, &crash),
        per_game(back.orb, &back),
    );
    assert!(back.conceded < crash.conceded * 0.95, "{:?} against {:?}", back, crash);
    assert!(back.orb < crash.orb * 0.95, "{:?} against {:?}", back, crash);
}

#[test]
fn a_tired_team_runs_less() {
    let tired = tally(&recorded(&[81, 82, 83, 84], 40, &|p| p.condition = 0.35, &|_| {}));
    let fresh = tally(&recorded(&[81, 82, 83, 84], 40, &|p| p.condition = 1.0, &|_| {}));
    println!(
        "transition points a game: tired {:.2} against fresh {:.2}; transition shots {:.2} / {:.2}",
        per_game(tired.scored, &tired),
        per_game(fresh.scored, &fresh),
        per_game(tired.chances, &tired),
        per_game(fresh.chances, &fresh),
    );
    assert!(tired.chances < fresh.chances * 0.95, "{:?} against {:?}", tired, fresh);
    assert!(tired.scored < fresh.scored, "{:?} against {:?}", tired, fresh);
}

#[test]
fn the_break_is_deterministic() {
    let run = || {
        let w = World::generate(91, 12);
        let hl = lineup(&w.teams[2], &w.players);
        let al = lineup(&w.teams[7], &w.players);
        let (r, rec) = simulate_recorded(&hl, &al, &Rules::fiba(), &mut Rng::new(4));
        let runs = steal_runs(&rec).len();
        (r.score, rec.frames.len(), rec.events.len(), runs)
    };
    assert_eq!(run(), run());
}

#[test]
fn nobody_passes_to_himself_and_a_throw_in_after_a_basket_starts_out_of_bounds() {
    let (mut passes, mut inbounds, mut baskets, mut checked) = (0, 0, 0, 0);
    for rec in stock() {
        for p in &rec.passes {
            passes += 1;
            inbounds += p.inbound as u32;
            assert_ne!(p.from, p.to, "a pass from slot {} to himself at {:.1} s", p.from, p.time);
        }
        for e in &rec.events {
            // A basket with a foul on it goes to the line, and one at the
            // buzzer ends the period.
            let EventKind::Shot {
                made: true,
                fouled: false,
                ..
            } = e.kind
            else {
                continue;
            };
            if e.clock < 1.0 {
                continue;
            }
            baskets += 1;
            // The next pass after the net moves, by the other side.
            let Some(p) = rec.passes.iter().find(|p| p.time > e.time + 0.05) else {
                continue;
            };
            // Not if the period ended first: that restart is at the sideline.
            let ended = rec
                .events
                .iter()
                .any(|x| x.time > e.time && x.time <= p.time && matches!(x.kind, EventKind::PeriodEnd));
            if ended {
                continue;
            }
            checked += 1;
            let out = p.at.x < -0.05 || p.at.x > 28.05 || p.at.y < -0.05 || p.at.y > 15.05;
            assert!(
                out && p.inbound && p.side != e.offense,
                "after the basket at {:.1} s the next pass (side {}, inbound {}) began at ({:.1}, {:.1})",
                e.time, p.side, p.inbound, p.at.x, p.at.y
            );
        }
    }
    println!("{passes} passes, {inbounds} throw-ins, {baskets} baskets, {checked} followed by a throw-in");
    assert!(passes > 10_000 && checked * 10 > baskets * 9);
}
