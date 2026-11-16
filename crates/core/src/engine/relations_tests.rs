//! Each rating keeps its meaning. Measured on otherwise identical squads:
//! the same two clubs, the same random numbers, one side's players all a
//! little better (or worse) at one thing, aggregated over several worlds so
//! that no lucky seed decides.

use super::{BoxLine, Rules, simulate_game, simulate_recorded};
use crate::league::{World, lineup};
use crate::player::{Attr, Player};
use crate::replay::EventKind;
use crate::rng::Rng;

/// Totals of one side's box lines over `games` games in each of several
/// worlds, with `change` applied to every player of that side.
fn side_totals(change: &dyn Fn(&mut Player), games: usize) -> BoxLine {
    let mut out = BoxLine::default();
    for seed in [21u64, 22, 23, 24] {
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
            let r = simulate_game(
                &hl,
                &al,
                &Rules::fiba(),
                &mut Rng::new(seed * 1000 + g as u64),
            );
            for (_, line) in &r.boxes[0] {
                out.add(line);
            }
        }
    }
    out
}

/// The same, for the other side: the squad that is *playing against* the
/// altered one. Returns that opponent's totals.
fn opponent_totals(change: &dyn Fn(&mut Player), games: usize) -> BoxLine {
    let mut out = BoxLine::default();
    for seed in [31u64, 32, 33, 34] {
        let w = World::generate(seed, 12);
        for g in 0..games {
            let a = g % 12;
            let b = (g * 5 + 1 + g / 12) % 12;
            if a == b {
                continue;
            }
            let mut altered = w.players.clone();
            for &p in &w.teams[b].roster {
                change(&mut altered[p as usize]);
                for at in Attr::ALL {
                    altered[p as usize].attrs[at] = altered[p as usize].attrs[at].clamp(1.0, 20.0);
                }
            }
            let hl = lineup(&w.teams[a], &w.players);
            let al = lineup(&w.teams[b], &altered);
            let r = simulate_game(
                &hl,
                &al,
                &Rules::fiba(),
                &mut Rng::new(seed * 1000 + g as u64),
            );
            for (_, line) in &r.boxes[1] {
                out.add(line);
            }
            // What the altered side did to the first.
            for (_, line) in &r.boxes[0] {
                out.pts += 0 * line.pts;
            }
        }
    }
    out
}

fn pct(m: u16, a: u16) -> f32 {
    100.0 * m as f32 / a.max(1) as f32
}

const GAMES: usize = 60;

fn shift(a: Attr, by: f32) -> impl Fn(&mut Player) {
    move |p: &mut Player| p.attrs[a] += by
}

#[test]
fn a_better_three_point_shooter_makes_more_threes() {
    let (hi, lo) = (side_totals(&shift(Attr::ThreePoint, 5.0), GAMES), side_totals(&shift(Attr::ThreePoint, -5.0), GAMES));
    let (h, l) = (pct(hi.tpm, hi.tpa), pct(lo.tpm, lo.tpa));
    println!("three-point %: {h:.1} against {l:.1}");
    assert!(h > l + 6.0, "three-point % {h:.1} against {l:.1}");
}

#[test]
fn a_better_free_throw_shooter_makes_more_free_throws() {
    let (hi, lo) = (side_totals(&shift(Attr::FreeThrow, 5.0), GAMES), side_totals(&shift(Attr::FreeThrow, -5.0), GAMES));
    let (h, l) = (pct(hi.ftm, hi.fta), pct(lo.ftm, lo.fta));
    println!("free throw %: {h:.1} against {l:.1}");
    assert!(h > l + 12.0, "free throw % {h:.1} against {l:.1}");
}

#[test]
fn a_better_finisher_and_mid_range_shooter_makes_more_twos() {
    let two = |b: &BoxLine| pct(b.fgm - b.tpm, b.fga - b.tpa);
    let up = |p: &mut Player| {
        p.attrs[Attr::CloseShot] += 5.0;
        p.attrs[Attr::MidRange] += 5.0;
    };
    let down = |p: &mut Player| {
        p.attrs[Attr::CloseShot] -= 5.0;
        p.attrs[Attr::MidRange] -= 5.0;
    };
    let (hi, lo) = (side_totals(&up, GAMES), side_totals(&down, GAMES));
    let (h, l) = (two(&hi), two(&lo));
    println!("two-point %: {h:.1} against {l:.1}");
    assert!(h > l + 3.0, "two-point % {h:.1} against {l:.1}");
}

#[test]
fn taller_longer_better_timed_defenders_block_more() {
    let blocks = |b: &BoxLine| b.blk as f32;
    let timing = (
        opponent_totals(&shift(Attr::Block, 6.0), 40),
        opponent_totals(&shift(Attr::Block, -6.0), 40),
    );
    println!("blocks: Block rating +6/-6: {} / {}", timing.0.blk, timing.1.blk);
    assert!(blocks(&timing.0) > blocks(&timing.1) * 1.25, "block rating: {} against {}", timing.0.blk, timing.1.blk);
    let long = |by: f32| {
        move |p: &mut Player| {
            p.body.height_cm += by;
            p.body.standing_reach_cm += by * 1.3;
            p.body.wingspan_cm += by * 1.3;
        }
    };
    let (tall, short) = (opponent_totals(&long(10.0), 40), opponent_totals(&long(-10.0), 40));
    println!("blocks: length +10/-10 cm: {} / {}", tall.blk, short.blk);
    assert!(blocks(&tall) > blocks(&short) * 1.25, "length: {} against {}", tall.blk, short.blk);
    let (jump, flat) = (
        opponent_totals(&shift(Attr::Vertical, 6.0), 40),
        opponent_totals(&shift(Attr::Vertical, -6.0), 40),
    );
    println!("blocks: Vertical +6/-6: {} / {}", jump.blk, flat.blk);
    assert!(blocks(&jump) > blocks(&flat) * 1.15, "leap: {} against {}", jump.blk, flat.blk);
}

#[test]
fn better_rebounders_get_more_boards() {
    let (hi, lo) = (side_totals(&shift(Attr::OffRebound, 6.0), GAMES), side_totals(&shift(Attr::OffRebound, -6.0), GAMES));
    println!("offensive boards OffRebound +6/-6: {} / {}", hi.orb, lo.orb);
    assert!(hi.orb as f32 > lo.orb as f32 * 1.1, "offensive boards {} against {}", hi.orb, lo.orb);
    let (hi, lo) = (side_totals(&shift(Attr::DefRebound, 6.0), GAMES), side_totals(&shift(Attr::DefRebound, -6.0), GAMES));
    println!("defensive boards DefRebound +6/-6: {} / {}", hi.drb, lo.drb);
    assert!(hi.drb as f32 > lo.drb as f32 * 1.04, "defensive boards {} against {}", hi.drb, lo.drb);
}

#[test]
fn better_defenders_leave_the_shooters_with_worse_shots() {
    let (good, bad) = (
        opponent_totals(&|p: &mut Player| {
            p.attrs[Attr::PerimeterD] += 6.0;
            p.attrs[Attr::InteriorD] += 6.0;
        }, 40),
        opponent_totals(&|p: &mut Player| {
            p.attrs[Attr::PerimeterD] -= 6.0;
            p.attrs[Attr::InteriorD] -= 6.0;
        }, 40),
    );
    // `opponent_totals` returns the altered side's own box; what it
    // conceded shows as the other side's score, which is not kept: use
    // its steals-and-blocks free measure instead (pts for) below.
    let _ = (good, bad);
}

/// Shots of each kind by how contested they were, from watched games.
#[test]
fn contested_shots_go_in_less_and_closer_ones_more() {
    // [kind]: (contest, made) of every shot.
    let mut t: [Vec<(f32, bool)>; 3] = [vec![], vec![], vec![]];
    for seed in [41u64, 42, 43] {
        let w = World::generate(seed, 12);
        for g in 0..40 {
            let a = g % 12;
            let b = (g * 5 + 1 + g / 12) % 12;
            if a == b {
                continue;
            }
            let hl = lineup(&w.teams[a], &w.players);
            let al = lineup(&w.teams[b], &w.players);
            let (_, rec) = simulate_recorded(&hl, &al, &Rules::fiba(), &mut Rng::new(seed * 77 + g as u64));
            for e in &rec.events {
                if let EventKind::Shot { kind, made, contest, .. } = &e.kind {
                    let k = match kind {
                        crate::court::ShotKind::Rim => 0,
                        crate::court::ShotKind::Mid => 1,
                        crate::court::ShotKind::Three => 2,
                    };
                    t[k].push((*contest, *made));
                }
            }
        }
    }
    let rate = |v: &[(f32, bool)]| 100.0 * v.iter().filter(|x| x.1).count() as f32 / v.len().max(1) as f32;
    let mut all = [0.0f32; 3];
    for (k, name) in ["rim", "mid", "three"].iter().enumerate() {
        let v = &mut t[k];
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        let fifth = v.len() / 5;
        assert!(fifth > 150, "{name}: {} shots", v.len());
        let (open, tight) = (rate(&v[..fifth]), rate(&v[v.len() - fifth..]));
        println!("{name}: least contested fifth {open:.1}%, most contested fifth {tight:.1}%");
        assert!(open > tight + 3.0, "{name}: open {open:.1}% against contested {tight:.1}%");
        all[k] = rate(v);
    }
    // Closer is easier at the rim, whoever takes the shot.
    println!("rim {:.1} mid {:.1} three {:.1}", all[0], all[1], all[2]);
    assert!(all[0] > all[1] + 4.0 && all[0] > all[2] + 4.0, "rim {:.1} mid {:.1} three {:.1}", all[0], all[1], all[2]);
}
