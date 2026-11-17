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
        manager: 0.0,
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
        // A foul the film put right (a charge that was a block, or the
        // reverse) is booked as an event of its own.
        whistles += rec
            .calls
            .iter()
            .filter(|c| c.review.is_some_and(|r| r.overturned) && matches!(c.truth, Ruling::Foul { .. }))
            .count();
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
    // A bench has one challenge a game and wins it a few times in a hundred
    // games: twelve games were too few to count on a win, so the book is
    // checked in every game of thirty-six and the win is asked of the lot.
    for seed in 0..36 {
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
        // The true foul of a call put right (a charge that was a block) is an
        // event of its own, so it is among the fouls logged already; what is
        // taken off is the foul that was called.
        let sheet: i32 = r.boxes.iter().flatten().map(|(_, l)| l.pf as i32).sum();
        let logged: i32 = expect.values().sum();
        assert_eq!(sheet, logged, "seed {seed}");
    }
    assert!(overturned > 0, "no challenge succeeded in thirty-six games");
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
        // One challenge a game, won or lost: no side is ever heard twice.
        for side in 0..2u8 {
            let n = reviews.iter().filter(|c| c.review.unwrap().by == side).count();
            assert!(n <= 1, "side {side} challenged {n} times (seed {seed})");
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

#[test]
fn a_review_allows_only_the_challenge_and_only_once() {
    use crate::league::Refusal;
    let mut seen = 0;
    for seed in 0..12 {
        let mut w = World::generate(seed, 8);
        let mut live = w.kickoff(0).unwrap();
        // Outside a review there is nothing to challenge.
        assert_eq!(live.check(&LiveInput::Challenge), Err(Refusal::NothingToChallenge));
        let mut spent = false;
        let mut guard = 0u32;
        while !live.game.is_over() && guard < 400_000 {
            guard += 1;
            if let Some(Pause::Review(_)) = live.step() {
                seen += 1;
                assert_eq!(live.stoppage(), Some(crate::league::Stoppage::Review));
                assert_eq!(live.check(&LiveInput::Timeout), Err(Refusal::ReviewOnly));
                assert_eq!(live.check(&LiveInput::AutoSubs(true)), Err(Refusal::ReviewOnly));
                assert_eq!(live.talks_left(), (0, 0));
                assert!(!spent, "a second review was offered after the challenge was spent");
                if live.try_input(LiveInput::Challenge).is_ok() {
                    spent = true;
                    assert_eq!(live.game.challenges_left(live.side), 0);
                }
            }
        }
    }
    assert!(seen > 0, "no review came up in twelve games");
}

#[test]
fn with_a_crew_nobody_teleports_and_the_three_keep_clear_of_a_throw_in() {
    let w = World::generate(7, 8);
    let step = 1.0 / crate::replay::FRAME_RATE;
    let mut throw_ins = 0;
    for g in 0..4 {
        let (home, away) = (lineup(&w.teams[g % 8], &w.players), lineup(&w.teams[(g + 3) % 8], &w.players));
        let (_, rec) = simulate_recorded_crewed(&home, &away, &w.rules, &mut Rng::new(900 + g as u64), Some(&crew(0.4, 0.5)));
        for pair in rec.frames.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if b.cut {
                continue;
            }
            let dt = b.time - a.time;
            assert!((dt - step).abs() < 0.002, "gap of {dt} s at {}", a.time);
            for side in 0..2 {
                for k in 0..5 {
                    let id = a.who[side][k];
                    let Some(j) = b.who[side].iter().position(|&w| w == id) else { continue };
                    assert!(a.pos[side][k].dist(b.pos[side][j]) <= 12.0 * dt + 0.15, "player moved too far at {}", a.time);
                }
            }
            for i in 0..3 {
                let d = a.officials[i].dist(b.officials[i]);
                assert!(d <= 6.0 * dt + 0.05, "official {i} moved {d} m in {dt} s at {}", a.time);
            }
            // A ball out of bounds in a man's hands is a throw-in: nobody in
            // stripes stands on top of him.
            let (x, y) = (b.ball.x, b.ball.y);
            if x < -0.3 || x > 28.3 || y < -0.3 || y > 15.3 {
                throw_ins += 1;
                for o in &b.officials {
                    assert!(o.dist(b.ball) > 0.6, "an official on the inbounder at {}", b.time);
                }
            }
        }
    }
    assert!(throw_ins > 50, "{throw_ins}");
}

// ---- game management -------------------------------------------------------

/// A crew with some game management and everything else middling.
fn managed(m: f32) -> Crew {
    let mut c = crew(0.6, 0.5);
    for r in &mut c.refs {
        r.consistency = 0.55;
        r.manager = m;
    }
    c
}

/// What one set of games under one crew came to.
struct Arm {
    games: usize,
    margin: f32,
    /// Fouls on the leading side minus fouls on the trailing side, summed.
    lead_minus_trail: i32,
    /// The referees' season records, summed over the games.
    rec_lead: u32,
    rec_trail: u32,
    wrong: u32,
    calls: u32,
    /// Of the marginal whistles an official blew right after a call that
    /// wronged a side (a wrong whistle on it, or a foul on it let go): how
    /// many went against that same side, and how many there were.
    after_wrong_against: u32,
    after_wrong: u32,
}

fn arm(m: f32, games: usize) -> Arm {
    let w = World::generate(7, 8);
    let c = managed(m);
    let mut a = Arm {
        games: 0,
        margin: 0.0,
        lead_minus_trail: 0,
        rec_lead: 0,
        rec_trail: 0,
        wrong: 0,
        calls: 0,
        after_wrong_against: 0,
        after_wrong: 0,
    };
    for g in 0..games {
        let (h, v) = (g % 8, (g * 3 + 1) % 8);
        if h == v {
            continue;
        }
        let (home, away) = (lineup(&w.teams[h], &w.players), lineup(&w.teams[v], &w.players));
        let (r, rec) = simulate_recorded_crewed(&home, &away, &w.rules, &mut Rng::new(4200 + g as u64), Some(&c));
        let o = r.officiating.as_ref().expect("officiated");
        a.games += 1;
        a.margin += (r.score[0] as f32 - r.score[1] as f32).abs();
        for side in 0..2 {
            a.lead_minus_trail += o.fouls_state[side][0] as i32 - o.fouls_state[side][2] as i32;
        }
        for p in &o.per_ref {
            a.rec_lead += p.fouls_lead;
            a.rec_trail += p.fouls_trail;
        }
        a.wrong += (o.errors[0] + o.errors[1] + o.errors[2]) as u32;
        a.calls += o.calls as u32;
        // Each official's wronged side so far, in the order of the calls.
        let mut wronged: [Option<u8>; 3] = [None; 3];
        for c in rec.calls.iter().filter(|c| c.what == crate::officials::What::Foul) {
            let by = c.by as usize;
            if let (Ruling::Marginal { .. }, Ruling::Foul { by: side, .. }) = (c.truth, c.called) {
                if let Some(x) = wronged[by].take() {
                    a.after_wrong += 1;
                    a.after_wrong_against += (side == x) as u32;
                }
                continue;
            }
            if c.verdict().is_wrong() {
                wronged[by] = match (c.truth, c.called) {
                    (_, Ruling::Foul { by: x, .. }) => Some(x),
                    (Ruling::Foul { by: x, .. }, _) => Some(1 - x),
                    _ => None,
                };
            }
        }
    }
    a
}

fn arms() -> &'static (Arm, Arm) {
    static ARMS: std::sync::OnceLock<(Arm, Arm)> = std::sync::OnceLock::new();
    ARMS.get_or_init(|| (arm(0.0, 360), arm(1.0, 360)))
}

#[test]
fn a_strong_game_manager_crew_keeps_games_closer_than_one_with_none() {
    let (none, strong) = arms();
    let (m0, m1) = (none.margin / none.games as f32, strong.margin / strong.games as f32);
    assert!(m1 < m0 - 0.4, "mean margin none {m0:.2}, strong {m1:.2}");
    // Modest: a crew of the most game-managing men there are does not
    // decide games.
    assert!(m1 > m0 - 3.0, "mean margin none {m0:.2}, strong {m1:.2}");
}

#[test]
fn a_strong_game_manager_crew_tilts_the_fouls_towards_the_trailing_side() {
    let (none, strong) = arms();
    let (d0, d1) = (
        none.lead_minus_trail as f32 / none.games as f32,
        strong.lead_minus_trail as f32 / strong.games as f32,
    );
    assert!(d1 > d0 + 1.0, "fouls on the leading side minus the trailing, per game: none {d0:.2}, strong {d1:.2}");
    // The season record shows the same (a level game counts for neither).
    let (r0, r1) = (
        (none.rec_lead as f32 - none.rec_trail as f32) / none.games as f32,
        (strong.rec_lead as f32 - strong.rec_trail as f32) / strong.games as f32,
    );
    assert!((r0 - d0).abs() < 1e-3 && (r1 - d1).abs() < 1e-3, "record {r0} {r1} against game {d0} {d1}");
}

#[test]
fn a_game_managers_calls_are_ordinary_calls_and_no_more_often_wrong() {
    let (none, strong) = arms();
    let (w0, w1) = (none.wrong as f32 / none.calls as f32, strong.wrong as f32 / strong.calls as f32);
    // Leaning is only on contact the book leaves to judgement, so it makes
    // no error of its own: the share of wrong calls stays where it was.
    assert!(w1 < w0 * 1.15 && w1 > w0 * 0.85, "wrong share none {w0:.4}, strong {w1:.4}");
    assert!(none.wrong > 100 && strong.wrong > 100);
}

#[test]
fn make_up_calls_follow_wrong_calls_more_often_than_chance() {
    let (none, strong) = arms();
    assert!(none.after_wrong > 150 && strong.after_wrong > 150, "{} {}", none.after_wrong, strong.after_wrong);
    let (s0, s1) = (
        none.after_wrong_against as f32 / none.after_wrong as f32,
        strong.after_wrong_against as f32 / strong.after_wrong as f32,
    );
    eprintln!("after a wrong call: {} (none) and {} (strong) marginal whistles follow; against the wronged side {s0:.3} and {s1:.3}", none.after_wrong, strong.after_wrong);
    // After a call that wronged a side, the next marginal whistle of the man
    // who made it is against that side less often than for a man who does not
    // make up.
    assert!(s1 < s0 - 0.04, "next marginal whistle against the wronged side: none {s0:.3}, strong {s1:.3}");
}

#[test]
fn a_game_managers_crew_still_records_truth_and_call_both() {
    let w = World::generate(7, 8);
    let (mut whistles, mut events) = (0, 0);
    for g in 0..30 {
        let (home, away) = (lineup(&w.teams[g % 8], &w.players), lineup(&w.teams[(g + 3) % 8], &w.players));
        let (r, rec) = simulate_recorded_crewed(&home, &away, &w.rules, &mut Rng::new(700 + g as u64), Some(&managed(1.0)));
        let o = r.officiating.as_ref().expect("officiated");
        assert_eq!(o.wrong() as usize, rec.calls.iter().filter(|c| c.verdict().is_wrong()).count());
        whistles += rec.calls.iter().filter(|c| matches!(c.called, Ruling::Foul { .. })).count();
        whistles += rec
            .calls
            .iter()
            .filter(|c| c.review.is_some_and(|r| r.overturned) && matches!(c.truth, Ruling::Foul { .. }))
            .count();
        events += rec.events.iter().filter(|e| matches!(e.kind, EventKind::Foul { .. })).count();
        // Every foul is in one of the three states, and they add up.
        let by_state: u16 = o.fouls_state.iter().flatten().sum();
        assert_eq!(by_state, o.fouls[0] + o.fouls[1]);
    }
    assert_eq!(whistles, events);
}

#[test]
fn game_management_is_rare_in_a_european_pool_common_in_an_nba_style_one() {
    use crate::engine::Rules;
    use crate::officials::Officials;
    assert!(Rules::nba().ref_game_mgmt > Rules::fiba().ref_game_mgmt + 5.0);
    let mean = |o: &Officials| o.pool.iter().map(|r| r.game_mgmt as f32).sum::<f32>() / o.pool.len() as f32;
    let (mut eu, mut nba) = (0.0, 0.0);
    for seed in 1..=12 {
        let mut pool = Officials::generate(seed, 24, Rules::fiba().ref_game_mgmt);
        assert!(pool.pool.iter().all(|r| (1..=20).contains(&r.game_mgmt)));
        eu += mean(&pool) / 12.0;
        let strong = pool.pool.iter().filter(|r| r.game_mgmt >= 9).count();
        assert!(strong * 4 <= pool.pool.len(), "only a few clearly do it: {strong} of {}", pool.pool.len());
        pool.restyle(Rules::nba().ref_game_mgmt);
        nba += mean(&pool) / 12.0;
    }
    assert!(eu > 3.0 && eu < 7.0, "European pool mean {eu}");
    assert!(nba > 10.0, "NBA-style pool mean {nba}");
    // It is not drawn from the stream the other traits are: the same seed
    // under another culture has every other trait as it was.
    let a = Officials::generate(9, 24, 4.0);
    let b = Officials::generate(9, 24, 12.0);
    for (x, y) in a.pool.iter().zip(&b.pool) {
        assert_eq!((x.name.as_str(), x.accuracy, x.strictness, x.home_lean), (y.name.as_str(), y.accuracy, y.strictness, y.home_lean));
    }
}

#[test]
fn an_old_save_without_game_management_gets_it_on_load() {
    let w = World::generate(11, 8);
    let mut v = serde_json::to_value(&w).unwrap();
    v["rules"].as_object_mut().unwrap().remove("ref_game_mgmt");
    let o = v["officials"].as_object_mut().unwrap();
    o.remove("game_mgmt_mean");
    for r in o["pool"].as_array_mut().unwrap() {
        let r = r.as_object_mut().unwrap();
        r.remove("game_mgmt");
        for rec in [r.get_mut("season").unwrap()] {
            let rec = rec.as_object_mut().unwrap();
            rec.remove("fouls_lead");
            rec.remove("fouls_trail");
        }
    }
    let mut old: World = serde_json::from_value(v.clone()).expect("an old save still loads");
    assert!(old.officials.pool.iter().all(|r| r.game_mgmt == 0));
    old.ensure_officials();
    // Given by the deterministic draw a new referee gets: the same as a world
    // made with the trait.
    assert!(old.officials.pool.iter().all(|r| r.game_mgmt >= 1));
    let was: Vec<u8> = w.officials.pool.iter().map(|r| r.game_mgmt).collect();
    let now: Vec<u8> = old.officials.pool.iter().map(|r| r.game_mgmt).collect();
    assert_eq!(was, now);
    let mut again: World = serde_json::from_value(v).unwrap();
    again.ensure_officials();
    assert_eq!(now, again.officials.pool.iter().map(|r| r.game_mgmt).collect::<Vec<u8>>());
}
