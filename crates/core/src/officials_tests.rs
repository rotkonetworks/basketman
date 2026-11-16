//! What should be true of the officials, stated as tests.

use crate::engine::{Pause, simulate_game_crewed, simulate_recorded_crewed};
use crate::league::{LiveInput, World, lineup};
use crate::officials::{Crew, GameKey, Official, Ruling};
use crate::replay::EventKind;
use crate::rng::Rng;

fn crew(acc: f32, strict: f32) -> Crew {
    let o = Official {
        id: 0,
        accuracy: acc,
        strictness: strict,
        consistency: 0.55,
        home_lean: 0.4,
        composure: 0.55,
        charge_lean: 0.5,
        flop_buyer: 0.5,
    };
    Crew { refs: [o; 3], crowd: 0.5 }
}

/// A live game on `seed` in which the bench challenges whatever it is asked
/// about. Returns the world, the round and the finished game's pieces.
fn live_game(seed: u64, always: bool) -> (crate::engine::GameResult, crate::replay::Recording, usize) {
    let mut w = World::generate(seed, 8);
    let mut live = w.kickoff(0).expect("a game");
    let side = live.side;
    let mut asked = 0;
    let mut guard = 0u32;
    while !live.game.is_over() && guard < 400_000 {
        guard += 1;
        if let Some(Pause::Review(s)) = live.step() {
            assert_eq!(s, side);
            assert!(live.game.review().is_some());
            asked += 1;
            if always {
                live.input(LiveInput::Challenge);
            }
        }
    }
    let (r, rec) = live.game.finish();
    (r, rec, asked)
}

#[test]
fn nobody_works_two_games_on_a_day_across_a_season() {
    let mut w = World::generate_tiers(5, 8, 3);
    w.begin_preseason();
    let mut days = std::collections::BTreeSet::new();
    for f in &w.fixtures {
        days.insert(f.day);
    }
    for f in &w.friendlies {
        days.insert(f.day);
    }
    assert!(days.len() > 10);
    for d in days {
        let crews = w.crews_on(d);
        let mut seen = std::collections::BTreeSet::new();
        for (_, c) in &crews {
            for &id in c {
                assert!(seen.insert(id), "referee {id} works twice on day {d}");
            }
        }
        // The same answer every time it is asked: known before the game.
        assert_eq!(crews, w.crews_on(d));
    }
}

#[test]
fn the_crew_is_known_before_the_game_and_is_the_one_that_works_it() {
    let mut w = World::generate(4, 8);
    let before: Vec<[u16; 3]> = (0..w.fixtures.len())
        .filter(|&i| w.fixtures[i].round == 0)
        .map(|i| w.crew_of(GameKey::League(i)).unwrap())
        .collect();
    w.play_round();
    let mut after = Vec::new();
    for i in (0..w.fixtures.len()).filter(|&i| w.fixtures[i].round == 0) {
        let o = w.fixtures[i].result.as_ref().unwrap().officiating.as_ref().unwrap();
        let mut c = o.crew;
        c.sort();
        after.push(c);
    }
    for (b, a) in before.iter().zip(&after) {
        let mut b = *b;
        b.sort();
        assert_eq!(&b, a);
    }
    // They have a record now.
    let worked: u16 = w.officials.pool.iter().map(|r| r.season.games).sum();
    assert_eq!(worked as usize, 4 * 3);
}

#[test]
fn truth_and_call_are_both_recorded() {
    let w = World::generate(7, 8);
    let (mut whistles, mut differ, mut events) = (0, 0, 0);
    for g in 0..40 {
        let (home, away) = (lineup(&w.teams[g % 8], &w.players), lineup(&w.teams[(g + 3) % 8], &w.players));
        let (r, rec) = simulate_recorded_crewed(
            &home,
            &away,
            &w.rules,
            &mut Rng::new(500 + g as u64),
            Some(&crew(0.4, 0.5)),
        );
        let o = r.officiating.as_ref().expect("officiated");
        whistles += rec.calls.iter().filter(|c| matches!(c.called, Ruling::Foul { .. })).count();
        events += rec.events.iter().filter(|e| matches!(e.kind, EventKind::Foul { .. })).count();
        differ += rec.calls.iter().filter(|c| c.verdict().is_wrong()).count();
        assert_eq!(o.wrong() as usize, rec.calls.iter().filter(|c| c.verdict().is_wrong()).count());
        // Every frame has the three on the floor.
        assert!(rec.frames.iter().all(|f| f.officials.iter().all(|p| p.x >= -1.0 && p.x <= 29.0)));
    }
    assert_eq!(whistles, events, "every whistle is a foul in the log, as called");
    assert!(differ > 20, "some calls were wrong: {differ}");
}

#[test]
fn a_better_crew_errs_less_over_many_games() {
    let w = World::generate(7, 8);
    let rate = |c: Crew| {
        let (mut wrong, mut calls) = (0u32, 0u32);
        for g in 0..160 {
            let (home, away) = (lineup(&w.teams[g % 8], &w.players), lineup(&w.teams[(g * 3 + 1) % 8], &w.players));
            if g % 8 == (g * 3 + 1) % 8 {
                continue;
            }
            let r = simulate_game_crewed(&home, &away, &w.rules, &mut Rng::new(900 + g as u64), Some(&c));
            let o = r.officiating.unwrap();
            wrong += (o.errors[0] + o.errors[1] + o.errors[2]) as u32;
            calls += o.calls as u32;
        }
        wrong as f32 / calls as f32
    };
    let (poor, good) = (rate(crew(0.15, 0.5)), rate(crew(0.9, 0.5)));
    assert!(poor > 1.8 * good, "poor {poor} good {good}");
    assert!(good < 0.06 && poor > 0.08, "poor {poor} good {good}");
}

#[test]
fn a_wrong_call_that_is_overturned_is_corrected_in_the_box_score() {
    let (mut overturned, mut lost) = (0, 0);
    for seed in 0..12 {
        let (r, rec, _) = live_game(seed, true);
        for side in 0..2 {
            // The book adds up.
            let pts: u16 = r.boxes[side].iter().map(|(_, l)| l.pts).sum();
            assert_eq!(pts, r.score[side], "seed {seed} side {side}");
            for (_, l) in &r.boxes[side] {
                assert!(l.tpm <= l.tpa && l.fgm <= l.fga && l.tpm <= l.fgm, "seed {seed}: {l:?}");
                assert_eq!(l.pts, (l.fgm - l.tpm) * 2 + l.tpm * 3 + l.ftm, "seed {seed}: {l:?}");
            }
        }
        // Fouls on the sheet are the fouls called, less those taken back.
        let mut expect = std::collections::BTreeMap::new();
        for e in &rec.events {
            if let EventKind::Foul { who, .. } = &e.kind {
                *expect.entry(*who).or_insert(0i32) += 1;
            }
        }
        for c in &rec.calls {
            if let Some(rv) = c.review {
                if rv.overturned {
                    overturned += 1;
                    if let (crate::officials::What::Foul, Ruling::Nothing) = (c.what, c.truth) {
                        *expect.entry(c.who).or_insert(0i32) -= 1;
                    } else if c.what == crate::officials::What::Foul {
                        // Taken off him; the true foul is somebody else's and
                        // counted again below.
                        *expect.entry(c.who).or_insert(0i32) -= 1;
                    }
                } else {
                    lost += 1;
                }
            }
        }
        let overturned_wrong_kind = rec
            .calls
            .iter()
            .filter(|c| c.review.is_some_and(|r| r.overturned) && matches!(c.truth, Ruling::Foul { .. }))
            .count();
        let sheet: i32 = r.boxes.iter().flatten().map(|(_, l)| l.pf as i32).sum();
        let logged: i32 = expect.values().sum();
        assert_eq!(sheet, logged + overturned_wrong_kind as i32, "seed {seed}");
    }
    assert!(overturned > 0, "no challenge succeeded in twelve games");
    let _ = lost;
}

#[test]
fn a_bench_has_one_challenge_and_a_no_call_cannot_be_challenged() {
    let mut used = 0;
    for seed in 0..12 {
        let (_, rec, asked) = live_game(seed, true);
        let _ = asked;
        let reviews: Vec<_> = rec.calls.iter().filter(|c| c.review.is_some()).collect();
        used += reviews.len();
        // Only a whistle was ever reviewed.
        assert!(reviews.iter().all(|c| c.reviewable && c.called != Ruling::Nothing));
        // The human side (home, team 0) loses its challenge when it loses
        // one: nothing is heard after a failure.
        let mine: Vec<_> = reviews.iter().filter(|c| c.review.unwrap().by == 0).collect();
        if let Some(n) = mine.iter().position(|c| !c.review.unwrap().overturned) {
            assert_eq!(n + 1, mine.len(), "a lost challenge ends them (seed {seed})");
        }
        // And a side never asks twice for the same call.
        let mut at: Vec<f32> = reviews.iter().map(|c| c.time).collect();
        at.dedup();
        assert_eq!(at.len(), reviews.len());
    }
    assert!(used > 0);
}

#[test]
fn the_benches_the_program_runs_use_their_challenge() {
    let w = World::generate(7, 8);
    let (mut used, mut won) = (0, 0);
    for g in 0..80 {
        let (home, away) = (lineup(&w.teams[g % 8], &w.players), lineup(&w.teams[(g * 3 + 1) % 8], &w.players));
        if g % 8 == (g * 3 + 1) % 8 {
            continue;
        }
        let r = simulate_game_crewed(&home, &away, &w.rules, &mut Rng::new(70 + g as u64), Some(&crew(0.4, 0.5)));
        for c in r.officiating.unwrap().challenge.iter().flatten() {
            used += 1;
            won += c.review.unwrap().overturned as u32;
        }
    }
    assert!(used >= 15, "benches used it {used} times in 80 games");
    assert!(won > 0 && won < used, "{won} of {used}");
}

#[test]
fn a_game_with_challenges_replays_exactly_from_its_log() {
    use crate::command::Command;
    use crate::inbox::Stop;
    let mut found = false;
    for seed in 3..9 {
        let mut a = World::generate(seed, 8);
        a.apply(Command::TakeCharge(0)).unwrap();
        loop {
            match a.apply(Command::Advance).unwrap().stop.unwrap() {
                Stop::MatchDay => break,
                _ => {}
            }
        }
        let mut live = a.kickoff(0).unwrap();
        let mut guard = 0;
        while !live.game.is_over() && guard < 400_000 {
            guard += 1;
            if let Some(Pause::Review(_)) = live.step() {
                live.input(LiveInput::Challenge);
            }
        }
        let asked = live.game.calls().iter().any(|c| c.review.is_some());
        a.full_time(live);
        let mut b = World::generate(seed, 8);
        for cmd in a.log.clone() {
            b.apply(cmd).unwrap();
        }
        let fa = format!("{:?}", a.fixtures.iter().map(|f| &f.result).collect::<Vec<_>>());
        let fb = format!("{:?}", b.fixtures.iter().map(|f| &f.result).collect::<Vec<_>>());
        assert!(fa == fb, "seed {seed}: the replay differs");
        found |= asked;
    }
    assert!(found, "no challenge was made in six games");
}
