//! The shape of a game and of a league: what a coach could name as the
//! reason a game is not a blowout, a rotation is ten deep, a hall is worth a
//! few points. Each test states a requirement and measures it over many
//! worlds, so no lucky seed decides.

use super::{Rules, simulate_game};
use crate::league::{World, lineup};
use crate::rng::Rng;

/// Home and away top-division clubs of a generated league, every ordered pair.
fn pairs(w: &World) -> Vec<(usize, usize)> {
    let t = w.division_teams(0);
    let mut v = Vec::new();
    for &a in &t {
        for &b in &t {
            if a != b {
                v.push((a, b));
            }
        }
    }
    v
}

fn crewed(w: &World, home: usize, salt: u64) -> Option<crate::officials::Crew> {
    w.bare_crew(home, salt)
}

/// When a game is out of reach the starters sit: in games decided by more
/// than twenty the five most-used men play clearly less than in close ones.
#[test]
fn starters_sit_when_the_game_is_decided() {
    let (mut blow, mut close) = ((0.0, 0.0), (0.0, 0.0));
    for seed in 0..6u64 {
        let w = World::generate_tiers(500 + seed, 12, 3);
        for (k, (a, b)) in pairs(&w).into_iter().enumerate().step_by(3) {
            let (h, v) = (lineup(&w.teams[a], &w.players), lineup(&w.teams[b], &w.players));
            let mut rng = Rng::new(seed * 10_000 + k as u64);
            let r = simulate_game(&h, &v, &Rules::fiba(), &mut rng);
            let margin = (r.score[0] as i32 - r.score[1] as i32).abs();
            for side in 0..2 {
                let mut m: Vec<f64> = r.boxes[side].iter().map(|(_, l)| l.secs as f64 / 60.0).collect();
                m.sort_by(|x, y| y.total_cmp(x));
                let starters = m.iter().take(5).sum::<f64>() / 5.0;
                let slot = if margin >= 20 { &mut blow } else if margin <= 8 { &mut close } else { continue };
                slot.0 += starters;
                slot.1 += 1.0;
            }
        }
    }
    let (b, c) = (blow.0 / blow.1, close.0 / close.1);
    assert!(blow.1 > 40.0, "too few blowouts to say anything: {}", blow.1);
    assert!(b < c - 1.5, "starters played {b:.1} minutes in blowouts and {c:.1} in close games");
}

/// The default plan is a ten-man rotation: the best five are planned for
/// 25 to 28 of 40 minutes (a little over what they play, as garbage time and
/// foul trouble take some), the next five for 12 to 15.
#[test]
fn the_default_plan_is_a_ten_man_rotation() {
    for seed in 0..4u64 {
        let w = World::generate(seed, 8);
        for t in &w.teams {
            let tac = t.tactics();
            let total: f32 = tac.rotation.iter().map(|s| s.minutes as f32).sum();
            let mut m: Vec<f32> = tac.rotation.iter().map(|s| s.minutes as f32 / total * 200.0).collect();
            m.sort_by(|a, b| b.total_cmp(a));
            let top = m.iter().take(5).sum::<f32>() / 5.0;
            let bench = m.iter().skip(5).take(5).sum::<f32>() / 5.0;
            assert!((25.0..=28.0).contains(&top), "top five planned for {top:.1}");
            assert!((12.0..=15.0).contains(&bench), "bench planned for {bench:.1}");
        }
    }
}

/// Fatigue makes it the right plan: the same club planned for 24 minutes a
/// starter beats itself planned for 33.
#[test]
fn playing_the_starters_too_long_loses() {
    let (mut margin, mut n) = (0.0f64, 0.0f64);
    for seed in 0..6u64 {
        let w = World::generate_tiers(700 + seed, 12, 3);
        for (k, &t) in w.division_teams(0).iter().enumerate() {
            let mut short = w.teams[t].tactics().clone();
            let mut long = short.clone();
            for (i, s) in short.rotation.iter_mut().enumerate() {
                s.minutes = if i < 5 { 120 } else { 80 };
            }
            for (i, s) in long.rotation.iter_mut().enumerate() {
                s.minutes = if i < 5 { 165 } else { 35 };
            }
            let base = lineup(&w.teams[t], &w.players);
            fn mk_lineup<'a>(w: &'a World, base: &super::Lineup, tac: &'a crate::tactics::Tactics) -> super::Lineup<'a> {
                super::Lineup {
                tactics: tac,
                players: tac.rotation.iter().map(|s| &w.players[s.player as usize]).collect(),
                familiarity: base.familiarity,
                belief: base.belief,
                coach: base.coach,
                }
            }
            for swap in 0..2 {
                let mut rng = Rng::new(900_000 + seed * 1000 + k as u64 * 2 + swap);
                let (a, b) = (mk_lineup(&w, &base, &short), mk_lineup(&w, &base, &long));
                let r = if swap == 0 {
                    simulate_game(&a, &b, &Rules::fiba(), &mut rng)
                } else {
                    simulate_game(&b, &a, &Rules::fiba(), &mut rng)
                };
                let d = r.score[0] as f64 - r.score[1] as f64;
                margin += if swap == 0 { d } else { -d };
                n += 1.0;
            }
        }
    }
    assert!(margin / n > 0.0, "33 minutes a starter beat 24: {:+.2} points a game over {n} games", -margin / n);
}

/// The home side wins a little under two games in three: a loud house, the
/// whistle, an unfamiliar floor, in a league of equal clubs (each club plays
/// a copy of itself).
#[test]
fn the_home_side_wins_sixty_percent() {
    let (mut wins, mut n) = (0.0f64, 0.0f64);
    for seed in 0..5u64 {
        let w = World::generate_tiers(800 + seed, 12, 3);
        for (k, &t) in w.division_teams(0).iter().enumerate() {
            for rep in 0..24u64 {
                let (h, v) = (lineup(&w.teams[t], &w.players), lineup(&w.teams[t], &w.players));
                let mut rng = Rng::new(seed * 100_000 + k as u64 * 100 + rep);
                let crew = crewed(&w, t, seed + rep);
                let r = super::simulate_game_crewed(&h, &v, &Rules::fiba(), &mut rng, crew.as_ref());
                wins += match r.score[0].cmp(&r.score[1]) {
                    std::cmp::Ordering::Greater => 1.0,
                    std::cmp::Ordering::Equal => 0.5,
                    _ => 0.0,
                };
                n += 1.0;
            }
        }
    }
    let p = wins / n * 100.0;
    assert!((56.0..=67.0).contains(&p), "home side won {p:.1}% of {n} games");
}

/// Late in a close game the side that is behind fouls: in the last two
/// minutes of games within eight points, the defense that trails commits at
/// least twice as many fouls as the defense that leads.
#[test]
fn the_side_that_is_behind_fouls_late() {
    use crate::replay::EventKind;
    let (mut behind, mut ahead) = (0u32, 0u32);
    for seed in 0..4u64 {
        let w = World::generate_tiers(900 + seed, 12, 3);
        for (k, (a, b)) in pairs(&w).into_iter().enumerate().step_by(2) {
            let (h, v) = (lineup(&w.teams[a], &w.players), lineup(&w.teams[b], &w.players));
            let mut rng = Rng::new(seed * 77_777 + k as u64);
            let (r, rec) = super::simulate_recorded(&h, &v, &Rules::fiba(), &mut rng);
            for e in &rec.events {
                if e.period != 3 || e.clock > 120.0 {
                    continue;
                }
                let EventKind::Foul { who, kind, .. } = e.kind else { continue };
                // fouls by the defense on the ball handler
                if matches!(kind, crate::replay::FoulKind::Charging | crate::replay::FoulKind::IllegalScreen | crate::replay::FoulKind::PostPushOff) {
                    continue;
                }
                let side = if r.boxes[0].iter().any(|(id, _)| *id == who) { 0 } else { 1 };
                let lead = e.score[side] as i32 - e.score[1 - side] as i32;
                if (-8..=-1).contains(&lead) {
                    behind += 1;
                } else if (1..=8).contains(&lead) {
                    ahead += 1;
                }
            }
        }
    }
    assert!(behind + ahead > 100, "too few late fouls to say anything: {}", behind + ahead);
    assert!(behind as f64 >= 2.0 * ahead as f64, "late fouls by the side behind {behind}, by the side ahead {ahead}");
}
