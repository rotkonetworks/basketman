//! `basketman lab officials [games]`: what the referees do to the game.
//!
//! The same pairings and seeds are played under different crews (and under
//! none, the old rule), so differences are the officials' and not the draw's.
//! Reads off: fouls and free throws by crew quality and strictness, the
//! home/away foul balance, how often calls are wrong and in what way, how many
//! games a wrong call decided, how the benches use their challenge, and
//! whether the league's totals stay where they were.

use basketman_core::engine::{Lineup, simulate_game_crewed};
use basketman_core::league::{World, lineup};
use basketman_core::officials::{Crew, GameOfficiating, Official};
use basketman_core::rng::Rng;

#[derive(Default, Clone)]
struct Tot {
    games: u32,
    poss: u32,
    pf: u32,
    fta: u32,
    pts: u32,
    home_pf: u32,
    away_pf: u32,
    home_wins: u32,
    calls: u32,
    others: u32,
    errors: [u32; 5],
    decided: u32,
    challenged: u32,
    overturned: u32,
    both_used: u32,
    quality: f32,
}

fn crew(acc: f32, strict: f32, lean: f32, crowd: f32) -> Crew {
    let o = Official {
        id: 0,
        accuracy: acc,
        strictness: strict,
        consistency: 0.55,
        home_lean: lean,
        composure: 0.55,
        charge_lean: 0.5,
        flop_buyer: 0.5,
        manager: 0.0,
    };
    Crew { refs: [o; 3], crowd }
}

fn play(w: &World, games: usize, make: &(dyn Fn(usize) -> Option<Crew> + Sync)) -> Tot {
    let n = w.teams.len();
    let workers = 4;
    let parts: Vec<Tot> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..workers)
            .map(|k| {
                s.spawn(move || {
                    let mut t = Tot::default();
                    for g in (k..games).step_by(workers) {
                        let (a, b) = (g % n, (g * 5 + 1 + g / n) % n);
                        if a == b {
                            continue;
                        }
                        let (a, b) = if g % 2 == 0 { (a, b) } else { (b, a) };
                        let (home, away): (Lineup, Lineup) = (
                            lineup(&w.teams[a], &w.players),
                            lineup(&w.teams[b], &w.players),
                        );
                        let crew = make(g);
                        let mut rng = Rng::new(9000 + g as u64);
                        let r = simulate_game_crewed(&home, &away, &w.rules, &mut rng, crew.as_ref());
                        t.games += 1;
                        t.poss += (r.possessions[0] + r.possessions[1]) as u32;
                        for side in 0..2 {
                            for (_, l) in &r.boxes[side] {
                                t.pf += l.pf as u32;
                                t.fta += l.fta as u32;
                                t.pts += l.pts as u32;
                                if side == 0 {
                                    t.home_pf += l.pf as u32;
                                } else {
                                    t.away_pf += l.pf as u32;
                                }
                            }
                        }
                        t.home_wins += (r.score[0] > r.score[1]) as u32;
                        if let Some(o) = &r.officiating {
                            t.calls += o.calls as u32;
                            t.others += o.others as u32;
                            for k in 0..5 {
                                t.errors[k] += o.errors[k] as u32;
                            }
                            t.decided += o.decided_for(r.score).is_some() as u32;
                            let used: Vec<&GameOfficiating> = vec![o];
                            for o in used {
                                let c: u32 = o.challenge.iter().flatten().count() as u32;
                                t.challenged += c;
                                t.overturned += o
                                    .challenge
                                    .iter()
                                    .flatten()
                                    .filter(|c| c.review.is_some_and(|r| r.overturned))
                                    .count() as u32;
                                t.both_used += (c == 2) as u32;
                            }
                        }
                    }
                    t
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().expect("worker")).collect()
    });
    let mut t = Tot::default();
    for p in parts {
        t.games += p.games;
        t.poss += p.poss;
        t.pf += p.pf;
        t.fta += p.fta;
        t.pts += p.pts;
        t.home_pf += p.home_pf;
        t.away_pf += p.away_pf;
        t.home_wins += p.home_wins;
        t.calls += p.calls;
        t.others += p.others;
        for k in 0..5 {
            t.errors[k] += p.errors[k];
        }
        t.decided += p.decided;
        t.challenged += p.challenged;
        t.overturned += p.overturned;
        t.both_used += p.both_used;
    }
    t
}

fn row(name: &str, t: &Tot) {
    let g = t.games.max(1) as f32;
    // Right on fouls: of every whistle and every foul that should have been
    // one, the share that was right in kind, side and existence.
    let wrong: u32 = t.errors[0] + t.errors[1] + t.errors[2];
    let right = if t.calls > 0 { 100.0 * (1.0 - wrong as f32 / t.calls as f32) } else { f32::NAN };
    println!(
        "{name:<28} PF/100 {:>5.2}  FTA/g {:>5.2}  pts/100 {:>6.2}  home win {:>4.1}%  PF home/away {:>5.2}/{:<5.2}  right {:>5.1}%  wrong/g {:>4.2}  decided {:>4.1}%",
        100.0 * t.pf as f32 / t.poss.max(1) as f32,
        t.fta as f32 / g,
        100.0 * t.pts as f32 / t.poss.max(1) as f32,
        100.0 * t.home_wins as f32 / g,
        t.home_pf as f32 / g,
        t.away_pf as f32 / g,
        right,
        wrong as f32 / g,
        100.0 * t.decided as f32 / g,
    );
}

pub fn run(games: usize) {
    let w = World::generate(7, 12);
    println!("{games} games per arm, the same pairings and seeds in every arm.\n");
    // MGMT_ONLY=1: just the game-management table (the rest is slow).
    if std::env::var_os("MGMT_ONLY").is_some() {
        managers(&w, games);
        return;
    }
    let none = play(&w, games, &|_| None);
    row("no officials (old rule)", &none);
    println!();
    let mut kinds = Vec::new();
    for (an, a) in [("poor eyes", 0.2f32), ("average eyes", 0.55), ("sharp eyes", 0.85)] {
        for (sn, s) in [("loose", 0.12f32), ("even", 0.5), ("tight", 0.88)] {
            let t = play(&w, games, &move |_| Some(crew(a, s, 0.4, 0.5)));
            row(&format!("{an}, {sn} whistle"), &t);
            kinds.push((an, sn, t));
        }
    }
    println!();
    println!("Home crowd (loud hall 1.0, average eyes, even whistle):");
    let t0 = play(&w, games, &|_| Some(crew(0.55, 0.5, 0.0, 1.0)));
    row("  no lean", &t0);
    let t1 = play(&w, games, &|_| Some(crew(0.55, 0.5, 1.0, 1.0)));
    row("  full lean", &t1);
    println!();
    let pool = play(&w, games, &|g| {
        let ids = [
            (g * 7 % w.officials.pool.len()) as u16,
            ((g * 7 + 19) % w.officials.pool.len()) as u16,
            ((g * 7 + 41) % w.officials.pool.len()) as u16,
        ];
        let mut c = w.officials.crew(ids);
        c.crowd = 0.45;
        Some(c)
    });
    row("league pool crews", &pool);
    let d = |a: f32, b: f32| 100.0 * (a - b) / b;
    let per = |t: &Tot, f: &dyn Fn(&Tot) -> f32| f(t);
    let pf = |t: &Tot| t.pf as f32 / t.poss.max(1) as f32;
    let fta = |t: &Tot| t.fta as f32 / t.games.max(1) as f32;
    let pts = |t: &Tot| t.pts as f32 / t.poss.max(1) as f32;
    println!(
        "\nleague crews against no officials: fouls per 100 possessions {:+.1}%, free throw attempts {:+.1}%, points per 100 {:+.1}%, home win {:.1}% -> {:.1}%",
        d(per(&pool, &pf), per(&none, &pf)),
        d(per(&pool, &fta), per(&none, &fta)),
        d(per(&pool, &pts), per(&none, &pts)),
        100.0 * none.home_wins as f32 / none.games.max(1) as f32,
        100.0 * pool.home_wins as f32 / pool.games.max(1) as f32,
    );
    let calls = pool.calls.max(1) as f32;
    let balls = pool.others.max(1) as f32;
    let names = ["missed foul", "phantom foul", "wrong foul (charge/block)", "ball the wrong way", "basket miscounted"];
    println!("\nwhere the league crews are wrong (share of the decisions of their kind):");
    for k in 0..5 {
        let of = if k < 3 { calls } else { balls };
        println!("  {:<28} {:>5.2}% of {}   {:>5.2} per game", names[k], 100.0 * pool.errors[k] as f32 / of, if k < 3 { "foul decisions" } else { "ball and count decisions" }, pool.errors[k] as f32 / pool.games.max(1) as f32);
    }
    println!(
        "\nchallenges: {:.2} per game ({:.0}% of benches use theirs), {:.0}% overturned; games decided by a wrong call {:.1}%",
        pool.challenged as f32 / pool.games.max(1) as f32,
        50.0 * pool.challenged as f32 / pool.games.max(1) as f32,
        100.0 * pool.overturned as f32 / pool.challenged.max(1) as f32,
        100.0 * pool.decided as f32 / pool.games.max(1) as f32,
    );
    let _ = kinds;
    managers(&w, games);
}

/// What one arm of the game-management table reads off.
#[derive(Default, Clone)]
struct Mg {
    games: u32,
    margin: f32,
    blow20: u32,
    close5: u32,
    /// Fouls on the side leading and on the side trailing, summed over games.
    lead_fouls: u32,
    trail_fouls: u32,
    calls: u32,
    wrong: u32,
    home_wins: u32,
    down10: u32,
    came_back: u32,
    pf: u32,
}

fn play_mg(w: &World, games: usize, crew_of: &(dyn Fn(usize) -> Crew + Sync)) -> Mg {
    let n = w.teams.len();
    let workers = 4;
    let parts: Vec<Mg> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..workers)
            .map(|k| {
                s.spawn(move || {
                    let mut t = Mg::default();
                    for g in (k..games).step_by(workers) {
                        let (a, b) = (g % n, (g * 5 + 1 + g / n) % n);
                        if a == b {
                            continue;
                        }
                        let (a, b) = if g % 2 == 0 { (a, b) } else { (b, a) };
                        let home = lineup(&w.teams[a], &w.players);
                        let away = lineup(&w.teams[b], &w.players);
                        let crew = crew_of(g);
                        let mut rng = Rng::new(9000 + g as u64);
                        let r = simulate_game_crewed(&home, &away, &w.rules, &mut rng, Some(&crew));
                        t.games += 1;
                        let m = r.score[0] as i32 - r.score[1] as i32;
                        t.margin += m.abs() as f32;
                        t.blow20 += (m.abs() >= 20) as u32;
                        t.close5 += (m.abs() <= 5) as u32;
                        t.home_wins += (m > 0) as u32;
                        for side in 0..2 {
                            for (_, l) in &r.boxes[side] {
                                t.pf += l.pf as u32;
                            }
                        }
                        let half = w.rules.periods as usize / 2;
                        let (h0, h1) = r.periods.iter().take(half).fold((0i32, 0i32), |acc, p| {
                            (acc.0 + p[0] as i32, acc.1 + p[1] as i32)
                        });
                        let ht = h0 - h1;
                        if ht.abs() >= 10 {
                            t.down10 += 1;
                            // The side that trailed at half-time: did it win?
                            let trailing_won = if ht > 0 { m < 0 } else { m > 0 };
                            t.came_back += trailing_won as u32;
                        }
                        if let Some(o) = &r.officiating {
                            t.calls += o.calls as u32;
                            t.wrong += (o.errors[0] + o.errors[1] + o.errors[2]) as u32;
                            for side in 0..2 {
                                t.lead_fouls += o.fouls_state[side][0] as u32;
                                t.trail_fouls += o.fouls_state[side][2] as u32;
                            }
                        }
                    }
                    t
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().expect("worker")).collect()
    });
    let mut t = Mg::default();
    for p in parts {
        t.games += p.games;
        t.margin += p.margin;
        t.blow20 += p.blow20;
        t.close5 += p.close5;
        t.lead_fouls += p.lead_fouls;
        t.trail_fouls += p.trail_fouls;
        t.calls += p.calls;
        t.wrong += p.wrong;
        t.home_wins += p.home_wins;
        t.down10 += p.down10;
        t.came_back += p.came_back;
        t.pf += p.pf;
    }
    t
}

fn mg_row(name: &str, t: &Mg) {
    let g = t.games.max(1) as f32;
    println!(
        "{name:<30} margin {:>5.2}  >=20 {:>4.1}%  <=5 {:>4.1}%  fouls lead-trail {:>+5.2}/g (lead {:>5.2}, trail {:>5.2})  wrong {:>4.2}% of calls  home win {:>4.1}%  comeback from 10+ at half {:>4.1}% (of {:>4.1}% of games)  PF/g {:>5.2}",
        t.margin / g,
        100.0 * t.blow20 as f32 / g,
        100.0 * t.close5 as f32 / g,
        (t.lead_fouls as f32 - t.trail_fouls as f32) / g,
        t.lead_fouls as f32 / g,
        t.trail_fouls as f32 / g,
        100.0 * t.wrong as f32 / t.calls.max(1) as f32,
        100.0 * t.home_wins as f32 / g,
        100.0 * t.came_back as f32 / t.down10.max(1) as f32,
        100.0 * t.down10 as f32 / g,
        t.pf as f32 / g,
    );
}

/// Game management by level: the same pairings, seeds and referees, with
/// the trait zeroed, as the league pool has it, as an NBA-style pool has it
/// (`GAME_MGMT_NBA`), and at the top for every referee.
fn managers(w: &World, games: usize) {
    use basketman_core::officials::GAME_MGMT_NBA;
    println!("\nGame management (the referees' tendency to keep games close), same games and referees in every row:");
    let crew_from = |pool: &basketman_core::officials::Officials, g: usize, force: Option<f32>| {
        let len = pool.pool.len();
        let ids = [(g * 7 % len) as u16, ((g * 7 + 19) % len) as u16, ((g * 7 + 41) % len) as u16];
        let mut c = pool.crew(ids);
        c.crowd = 0.45;
        if let Some(f) = force {
            for r in &mut c.refs {
                r.manager = f;
            }
        }
        c
    };
    let mut nba = w.officials.clone();
    nba.restyle(GAME_MGMT_NBA);
    let rows: [(&str, Mg); 4] = [
        ("none (trait zero)", play_mg(w, games, &|g| crew_from(&w.officials, g, Some(0.0)))),
        ("league pool (Europe, mean 4)", play_mg(w, games, &|g| crew_from(&w.officials, g, None))),
        ("NBA-style pool (mean 12)", play_mg(w, games, &|g| crew_from(&nba, g, None))),
        ("strong (all three at 1.0)", play_mg(w, games, &|g| crew_from(&w.officials, g, Some(1.0)))),
    ];
    for (name, t) in &rows {
        mg_row(name, t);
    }
    let pool_mean = w.officials.pool.iter().filter(|r| r.active).map(|r| r.game_mgmt as f32).sum::<f32>()
        / w.officials.pool.iter().filter(|r| r.active).count().max(1) as f32;
    let nba_mean = nba.pool.iter().filter(|r| r.active).map(|r| r.game_mgmt as f32).sum::<f32>()
        / nba.pool.iter().filter(|r| r.active).count().max(1) as f32;
    println!("pool means of game management (1..20): league {pool_mean:.1}, NBA-style {nba_mean:.1}");
}

/// `basketman lab survival [worlds] [first seed] [with|without]`: how often a
/// manager of club 1 of a twelve-club league is still in the job after two
/// seasons, with and without officials. The league plays itself; the manager
/// only advances.
pub fn survival(worlds: usize, first: u64, with: bool) {
    use basketman_core::command::Command;
    use basketman_core::inbox::Stop;
    let workers = 6;
    let lasted: Vec<(u64, u16)> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..workers)
            .map(|k| {
                s.spawn(move || {
                    let mut out = Vec::new();
                    for i in (k..worlds).step_by(workers) {
                        let seed = first + i as u64;
                        let mut w = World::generate(seed, 12);
                        w.officials.off = !with;
                        w.apply(Command::TakeCharge(1)).unwrap();
                        let mut seasons = 0;
                        let mut guard = 0u32;
                        while w.season < 12 && guard < 1_000_000 {
                            guard += 1;
                            if w.watch.is_none() {
                                break;
                            }
                            match w.apply(Command::Advance).unwrap().stop.unwrap() {
                                Stop::MatchDay => {
                                    w.apply(Command::PlayRound).unwrap();
                                }
                                Stop::Mail => {}
                                Stop::Draft => {
                                    let _ = w.apply(Command::DraftAuto { all: true });
                                }
                            }
                            seasons = w.season;
                            if seasons >= 2 {
                                break;
                            }
                        }
                        out.push((seed, seasons));
                    }
                    out
                })
            })
            .collect();
        hs.into_iter().flat_map(|h| h.join().expect("worker")).collect()
    });
    let k = lasted.iter().filter(|x| x.1 >= 2).count();
    println!(
        "{} officials: the manager lasted two seasons in {k} of {} worlds ({:.1}%)",
        if with { "with" } else { "without" },
        lasted.len(),
        100.0 * k as f32 / lasted.len().max(1) as f32
    );
}
