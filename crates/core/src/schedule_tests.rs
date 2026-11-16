//! Tests and measurements for scouting opponents, match preparation and travel.

use crate::league::World;

/// Home win share and mean home margin over the regular seasons of `seeds`
/// bare leagues of twelve clubs.
pub(crate) fn home_court(seeds: std::ops::RangeInclusive<u64>) -> (f32, f32, usize) {
    let (mut wins, mut games, mut margin) = (0usize, 0usize, 0i64);
    for seed in seeds {
        let mut w = World::generate(seed, 12);
        while !w.season_over() {
            w.play_round();
        }
        for f in w.fixtures.iter().filter(|f| !f.playoff) {
            let r = f.result.as_ref().unwrap();
            games += 1;
            wins += (r.score[0] > r.score[1]) as usize;
            margin += r.score[0] as i64 - r.score[1] as i64;
        }
    }
    (wins as f32 / games as f32, margin as f32 / games as f32, games)
}

#[test]
#[ignore = "a measurement that prints numbers; run with --ignored --nocapture"]
fn measure_home_court() {
    let (win, margin, games) = home_court(1..=12);
    eprintln!("HOME COURT: {games} games, home win {:.1}%, home margin {margin:+.2}", win * 100.0);
}

use crate::command::{Command, CommandError};
use crate::staff::Job;

/// A career at club 0 with the scout's judgement set.
fn career(seed: u64, judgement: u8) -> World {
    let mut w = World::new_career(seed, 8);
    w.apply(Command::TakeCharge(0)).unwrap();
    let scout = w.teams[0].office.staff[Job::Scout as usize].as_mut().expect("a scout");
    scout.judgement = judgement;
    w
}

/// Send the scout to some other club's game today, if there is one.
fn send_scout_today(w: &mut World) -> bool {
    let me = w.watch.unwrap();
    for t in 0..w.teams.len() {
        if t != me
            && w.game_of(t, w.day).is_some_and(|(_, h, a)| h != me && a != me)
            && w.apply(Command::AssignScout { team: t, day: w.day }).is_ok()
        {
            return true;
        }
    }
    false
}

/// Play on until `rounds` rounds are in, with the scout at a game every
/// day there is one.
fn drive(w: &mut World, rounds: usize) {
    let mut guard = 0;
    while w.round < rounds {
        guard += 1;
        assert!(guard < 600);
        send_scout_today(w);
        if w.match_today() {
            w.apply(Command::PlayToday).unwrap();
        } else {
            w.apply(Command::Continue).unwrap();
        }
    }
}

#[test]
fn the_scout_is_sent_to_a_game_and_reports_on_it() {
    let mut w = career(3, 12);
    // Refusals.
    let friendly_day = crate::calendar::FRIENDLY_DAYS[0];
    let own = w.friendlies.iter().find(|f| f.day == friendly_day && (f.home == 0 || f.away == 0)).unwrap().clone();
    let theirs = w.friendlies.iter().find(|f| f.day == friendly_day && f.home != 0 && f.away != 0).unwrap().clone();
    assert!(matches!(w.apply(Command::AssignScout { team: 0, day: friendly_day }), Err(CommandError::Refused(_))));
    assert!(matches!(w.apply(Command::AssignScout { team: own.home + own.away, day: friendly_day - 1 }), Err(_)), "no game that day");
    assert!(w.apply(Command::AssignScout { team: theirs.home, day: friendly_day + 1 }).is_err(), "no game that day");
    // Assigning, replacing, cancelling.
    w.apply(Command::AssignScout { team: theirs.home, day: friendly_day }).unwrap();
    assert_eq!(w.scouting.assignments.len(), 1);
    w.apply(Command::AssignScout { team: theirs.away, day: friendly_day }).unwrap();
    assert_eq!(w.scouting.assignments.len(), 1, "one place a day");
    assert_eq!(w.scouting.assignments[0].team, theirs.away);
    w.apply(Command::UnassignScout { day: friendly_day }).unwrap();
    assert!(w.apply(Command::UnassignScout { day: friendly_day }).is_err());
    w.apply(Command::AssignScout { team: theirs.away, day: friendly_day }).unwrap();
    // No scout, no job.
    let mut bare = career(3, 12);
    bare.teams[0].office.staff[Job::Scout as usize] = None;
    assert!(bare.apply(Command::AssignScout { team: theirs.home, day: friendly_day }).is_err());

    let replay_before = w.replay.as_ref().map(|r| r.0);
    while w.day <= friendly_day {
        w.apply(Command::Continue).unwrap();
    }
    assert!(w.scouting.assignments.is_empty(), "the assignment is used up");
    let report = w.latest_report(theirs.away).expect("a report was filed");
    assert_eq!(report.day, friendly_day);
    assert!(report.friendly && report.vs == theirs.home && !report.home);
    assert!(!report.rotation.is_empty());
    let mail = w.desk.messages.iter().find(|m| m.subject.starts_with("Scout report")).expect("mail");
    assert!(mail.body.contains("How far to trust this"), "{}", mail.body);
    assert_eq!(w.replay.as_ref().map(|r| r.0), replay_before, "the manager's own replay is not touched");
    eprintln!("{}", mail.body);
}

#[test]
fn watching_a_game_does_not_change_it() {
    // The same world with and without the scout at the game plays the same.
    let mut a = career(7, 12);
    let mut b = career(7, 12);
    let day = crate::calendar::FRIENDLY_DAYS[0];
    let f = a.friendlies.iter().find(|f| f.day == day && f.home != 0 && f.away != 0).unwrap().clone();
    a.apply(Command::AssignScout { team: f.home, day }).unwrap();
    while a.day <= day {
        a.apply(Command::Continue).unwrap();
        b.apply(Command::Continue).unwrap();
    }
    let ra: Vec<_> = a.friendlies.iter().map(|f| f.result.as_ref().map(|r| (r.score, r.possessions))).collect();
    let rb: Vec<_> = b.friendlies.iter().map(|f| f.result.as_ref().map(|r| (r.score, r.possessions))).collect();
    assert_eq!(ra, rb);
}

#[test]
fn a_scout_at_a_game_is_not_doing_his_player_scouting() {
    // Shortlist players of one game; send the scout to another game the
    // same day: no player report. Send him to the game itself: a report.
    let run = |at_other: bool| {
        let mut w = World::new_career(4, 12);
        w.apply(Command::TakeCharge(0)).unwrap();
        while w.day < crate::calendar::PRESEASON_DAYS + 3 {
            w.apply(Command::Continue).unwrap();
        }
        // The first league day is the first Friday: find a game neither of
        // ours and a second one the same day.
        let day = w.fixtures.iter().filter(|f| f.round == 0 && f.home != 0 && f.away != 0).map(|f| f.day).min().unwrap();
        let games: Vec<_> = w.fixtures.iter().filter(|f| f.day == day && f.home != 0 && f.away != 0).cloned().collect();
        assert!(games.len() >= 2, "two games that day");
        let target = &games[0];
        let other = &games[1];
        let star = w.teams[target.home].roster[0];
        w.apply(Command::Shortlist { player: star, on: true }).unwrap();
        let go = if at_other { other.home } else { target.home };
        w.apply(Command::AssignScout { team: go, day }).unwrap();
        w.apply(Command::PlayRound).unwrap();
        w.scouting.reports_for(star).count()
    };
    assert_eq!(run(false), 1, "at his game he sees the player");
    assert_eq!(run(true), 0, "at another game he cannot");
}

#[test]
fn a_better_scout_reads_a_club_better() {
    // Over several worlds: how often the offense, defense and press he
    // wrote down is what the club actually runs.
    let score = |judgement: u8| {
        let (mut right, mut total, mut none, mut reports) = (0, 0, 0, 0);
        for seed in 1..=6 {
            let mut w = career(seed, judgement);
            drive(&mut w, 8);
            for r in &w.scouting.opponent_reports {
                let t = w.teams[r.team].tactics();
                reports += 1;
                total += 3;
                right += (r.offense == Some(t.offense)) as u32 + (r.defense == Some(t.defense)) as u32 + (r.press == Some(t.press)) as u32;
                none += r.offense.is_none() as u32 + r.defense.is_none() as u32 + r.press.is_none() as u32;
            }
        }
        eprintln!("scout {judgement}: {reports} reports, {right}/{total} claims right, {none} not seen");
        right as f32 / total as f32
    };
    let (poor, good) = (score(2), score(20));
    assert!(good > 0.8, "a good scout is mostly right: {good}");
    assert!(poor < 0.7 && good > poor + 0.15, "good {good} v poor {poor}");
}

#[test]
fn more_games_make_a_report_more_trustworthy() {
    assert!(World::report_confidence(10, 3) > World::report_confidence(10, 1));
    assert!(World::report_confidence(16, 1) > World::report_confidence(6, 1));
    assert!(World::report_confidence(20, 9) <= 100);
}

use crate::engine::simulate_game;
use crate::league::lineup;
use crate::rng::Rng;

/// Games of every ordered pair of one world's clubs, plain and with one
/// side's belief raised by `boost`. Returns, summed over the world, the
/// prepared side's wins and points margin with and without the boost, and
/// the games each arm has.
fn prep_world(seed: u64, boost: f32) -> (f32, f32, f32, f32, usize) {
    let w = World::generate(seed, 12);
    let (mut plain_w, mut plain_m, mut prep_w, mut prep_m, mut n) = (0.0, 0.0, 0.0, 0.0, 0);
    let mut k = 0u64;
    for h in 0..12 {
        for a in 0..12 {
            if h == a {
                continue;
            }
            k += 1;
            for prepared in [0usize, 1] {
                // The same seed for the plain and the prepared game of a
                // side: the stream is shared until the boost bites.
                let sd = seed * 1_000_003 + k * 2 + prepared as u64;
                let mut hl = lineup(&w.teams[h], &w.players);
                let mut al = lineup(&w.teams[a], &w.players);
                let plain = simulate_game(&hl, &al, &w.rules, &mut Rng::new(sd));
                if prepared == 0 {
                    hl.belief = (hl.belief + boost).min(1.0);
                } else {
                    al.belief = (al.belief + boost).min(1.0);
                }
                let prep = simulate_game(&hl, &al, &w.rules, &mut Rng::new(sd));
                let m = |r: &crate::engine::GameResult| {
                    let d = r.score[prepared] as i32 - r.score[1 - prepared] as i32;
                    (d as f32, (d > 0) as u8 as f32)
                };
                let (pm, pw) = m(&plain);
                let (qm, qw) = m(&prep);
                plain_m += pm;
                plain_w += pw;
                prep_m += qm;
                prep_w += qw;
                n += 1;
            }
        }
    }
    (plain_w, plain_m, prep_w, prep_m, n)
}

#[test]
#[ignore = "a measurement that prints numbers; run with --ignored --nocapture"]
fn measure_prep_edge() {
    let boost: f32 = std::env::var("BOOST").ok().and_then(|s| s.parse().ok()).unwrap_or(crate::prep::PREP_BELIEF);
    let seeds: Vec<u64> = (1..=16).collect();
    let results: Vec<_> = std::thread::scope(|s| {
        let hs: Vec<_> = seeds.iter().map(|&sd| s.spawn(move || prep_world(sd, boost))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let (mut pw, mut pm, mut qw, mut qm, mut n) = (0.0, 0.0, 0.0, 0.0, 0usize);
    for r in results {
        pw += r.0;
        pm += r.1;
        qw += r.2;
        qm += r.3;
        n += r.4;
    }
    let n = n as f32;
    eprintln!(
        "PREP EDGE boost {boost}: {n} games per arm; unprepared win {:.1}% margin {:+.2}; prepared win {:.1}% margin {:+.2}; edge {:+.1} pts win, {:+.2} pts margin",
        pw / n * 100.0, pm / n, qw / n * 100.0, qm / n, (qw - pw) / n * 100.0, (qm - pm) / n
    );
}

#[test]
fn a_full_edge_is_a_modest_real_gain() {
    // 16 worlds of 12 clubs, every ordered pair, the prepared side first
    // at home, then away: aggregate, with the same seeds for both arms.
    let boost = crate::prep::PREP_BELIEF;
    let results: Vec<_> = std::thread::scope(|s| {
        let hs: Vec<_> = (1..=16u64).map(|sd| s.spawn(move || prep_world(sd, boost))).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let (mut pw, mut pm, mut qw, mut qm, mut n) = (0.0, 0.0, 0.0, 0.0, 0usize);
    for r in results {
        pw += r.0;
        pm += r.1;
        qw += r.2;
        qm += r.3;
        n += r.4;
    }
    let n = n as f32;
    let (win, margin) = ((qw - pw) / n * 100.0, (qm - pm) / n);
    eprintln!("full edge: {win:+.1} points of win rate, {margin:+.2} points a game over {n} games a side");
    assert!(margin > 0.5 && margin < 3.0, "margin edge {margin}");
    assert!(win > 1.0 && win < 8.0, "win edge {win}");
}

/// Our career with a report on club `opp` that says what the club
/// actually runs.
fn with_report(w: &mut World, opp: usize, right: (bool, bool)) {
    let day = crate::calendar::FRIENDLY_DAYS[0];
    let f = w.friendlies.iter().find(|f| f.day == day && (f.home == opp || f.away == opp)).unwrap().clone();
    assert!(f.home != 0 && f.away != 0, "pick a club that does not play us");
    w.apply(Command::AssignScout { team: opp, day }).unwrap();
    while w.day <= day {
        w.apply(Command::Continue).unwrap();
    }
    let t = w.teams[opp].tactics().clone();
    let r = w.scouting.opponent_reports.last_mut().unwrap();
    use crate::tactics::{DefenseScheme as D, OffenseSet as O};
    r.offense = Some(if right.0 { t.offense } else if t.offense == O::Motion { O::FiveOut } else { O::Motion });
    r.defense = Some(if right.1 { t.defense } else if t.defense == D::ManToMan { D::Zone23 } else { D::ManToMan });
}

fn prep_sessions(w: &mut World, n: usize) {
    use crate::training::Session;
    let mut s = w.teams[0].schedule.clone();
    for (k, slot) in [6usize, 7, 4, 5].iter().enumerate() {
        s.sessions[*slot] = if k < n { Session::MatchPrep } else { Session::Rest };
    }
    w.apply(Command::SetSchedule(s)).unwrap();
}

#[test]
fn preparation_helps_as_far_as_the_report_is_right() {
    let mut w = career(11, 20);
    let opp = (1..8).find(|&t| w.friendlies.iter().any(|f| f.day == crate::calendar::FRIENDLY_DAYS[0] && f.home != 0 && f.away != 0 && (f.home == t || f.away == t))).unwrap();
    with_report(&mut w, opp, (true, true));
    let day = w.fixtures.iter().find(|f| f.round == 0 && (f.home == 0 || f.away == 0)).map(|f| f.day).unwrap();
    // Nothing set: no edge from a good report.
    prep_sessions(&mut w, 0);
    assert_eq!(w.prep_for(0, opp, day).edge, 0.0);
    // Sessions scale the edge, up to four.
    prep_sessions(&mut w, 2);
    assert_eq!(w.prep_sessions(0, day), 2);
    assert!((w.prep_for(0, opp, day).edge - 0.5).abs() < 1e-6);
    prep_sessions(&mut w, 4);
    let full = w.prep_for(0, opp, day);
    assert!(full.offense_ok && full.defense_ok && (full.edge - 1.0).abs() < 1e-6);
    // A report right about half of it gives half, a wrong one nothing.
    let mut half = w.clone();
    for (right, edge) in [((true, false), 0.5), ((false, true), 0.5), ((false, false), 0.0)] {
        let r = half.scouting.opponent_reports.last_mut().unwrap();
        let t = half.teams[opp].tactics().clone();
        use crate::tactics::{DefenseScheme as D, OffenseSet as O};
        r.offense = Some(if right.0 { t.offense } else if t.offense == O::Motion { O::FiveOut } else { O::Motion });
        r.defense = Some(if right.1 { t.defense } else if t.defense == D::ManToMan { D::Zone23 } else { D::ManToMan });
        assert!((half.prep_for(0, opp, day).edge - edge).abs() < 1e-6, "{right:?}");
    }
    // Stale: the club changed its defense since the report.
    let mut stale = w.clone();
    let slot = stale.teams[opp].active;
    let t = &mut stale.teams[opp].playbook[slot].tactics;
    t.defense = if t.defense == crate::tactics::DefenseScheme::Zone23 { crate::tactics::DefenseScheme::PackLine } else { crate::tactics::DefenseScheme::Zone23 };
    assert!((stale.prep_for(0, opp, day).edge - 0.5).abs() < 1e-6);
    // A report on somebody else is no use.
    let other = (1..8).find(|&t| t != opp).unwrap();
    assert!(w.prep_for(0, other, day).report.is_none() && w.prep_for(0, other, day).edge == 0.0);
    // The engine is handed the edge as belief; with no edge, none.
    let belief = |w: &World, opp| {
        let (h, _) = w.lineups_for(0, opp, Some(day));
        h.belief
    };
    assert!((belief(&w, opp) - (w.teams[0].belief + crate::prep::PREP_BELIEF).min(1.0)).abs() < 1e-6);
    assert_eq!(belief(&w, other), w.teams[0].belief, "no report, no edge");
    let mut wrong = w.clone();
    with_wrong(&mut wrong, opp);
    assert_eq!(belief(&wrong, opp), wrong.teams[0].belief, "a wrong report does not help");
}

fn with_wrong(w: &mut World, opp: usize) {
    use crate::tactics::{DefenseScheme as D, OffenseSet as O};
    let t = w.teams[opp].tactics().clone();
    let r = w.scouting.opponent_reports.last_mut().unwrap();
    r.offense = Some(if t.offense == O::Motion { O::FiveOut } else { O::Motion });
    r.defense = Some(if t.defense == D::ManToMan { D::Zone23 } else { D::ManToMan });
}

#[test]
fn preparation_costs_the_training_it_replaces() {
    use crate::training::{Schedule, Session, train_week};
    let growth = |prep: bool| {
        let mut w = World::generate(6, 2);
        let mut s = Schedule::default();
        if prep {
            for slot in [0usize, 2, 3, 6] {
                s.sessions[slot] = Session::MatchPrep;
            }
        }
        let mut rng = Rng::new(1);
        let before: f32 = w.players.iter().map(|p| p.overall()).sum();
        for p in &mut w.players {
            for _ in 0..20 {
                train_week(p, &s, &mut rng, 1.0, 1.0);
            }
        }
        w.players.iter().map(|p| p.overall()).sum::<f32>() - before
    };
    assert!(growth(false) > growth(true), "preparation sessions train nothing: {} v {}", growth(false), growth(true));
    assert_eq!(Session::MatchPrep.familiarity(), 0.0);
    assert!(Session::MatchPrep.attrs().is_empty());
}

#[test]
fn the_assistant_keeps_the_preparation_the_manager_set() {
    let mut w = career(12, 12);
    prep_sessions(&mut w, 3);
    crate::assistant::run(&mut w, 0);
    assert_eq!(w.teams[0].schedule.count(crate::training::Session::MatchPrep), 3);
}

#[test]
fn clubs_nobody_manages_prepare_in_a_simple_way() {
    let w = World::new_career(5, 12);
    // No manager: everyone is the same kind of club. The average edge is
    // a fraction of the most a manager can have.
    let day = 40;
    let (mut sum, mut n, mut full) = (0.0, 0, 0);
    for a in 0..12 {
        for b in 0..12 {
            if a != b {
                let p = w.prep_for(a, b, day);
                sum += p.edge;
                n += 1;
                full += (p.edge > 0.2) as u32;
            }
        }
    }
    let mean = sum / n as f32;
    eprintln!("AI mean edge {mean:.3}");
    assert!(mean > 0.1 && mean < 0.4, "{mean}");
    assert!(full > 0 && full < n, "some clubs read the opponent right, some not");
}

/// Home win share and margin over `seeds` careers of twelve clubs, one
/// manager at club 0 who plays his rounds, with the toll of travel on or
/// off. Returns (games, home wins, margin sum).
fn career_home_court(seeds: std::ops::RangeInclusive<u64>, travel: bool) -> (usize, usize, i64) {
    let seeds: Vec<u64> = seeds.collect();
    let parts: Vec<(usize, usize, i64)> = std::thread::scope(|s| {
        let hs: Vec<_> = seeds
            .iter()
            .map(|&seed| {
                s.spawn(move || {
                    crate::travel::TRAVEL_OFF.with(|c| c.set(!travel));
                    let mut w = World::new_career(seed, 12);
                    w.apply(Command::TakeCharge(0)).unwrap();
                    let mut guard = 0;
                    while !w.season_over() {
                        guard += 1;
                        assert!(guard < 4000);
                        if w.match_today() {
                            w.apply(Command::PlayRound).unwrap();
                        } else {
                            w.apply(Command::Advance).unwrap();
                        }
                    }
                    let (mut g, mut wins, mut m) = (0, 0, 0i64);
                    for f in w.fixtures.iter().filter(|f| !f.playoff) {
                        let r = f.result.as_ref().unwrap();
                        g += 1;
                        wins += (r.score[0] > r.score[1]) as usize;
                        m += r.score[0] as i64 - r.score[1] as i64;
                    }
                    (g, wins, m)
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    parts.into_iter().fold((0, 0, 0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2))
}

#[test]
#[ignore = "a measurement that prints numbers; run with --ignored --nocapture"]
fn measure_career_home_court() {
    for travel in [false, true] {
        let (g, wins, m) = career_home_court(1..=24, travel);
        eprintln!(
            "CAREER HOME COURT travel {travel}: {g} games, home win {:.1}%, home margin {:+.2}",
            wins as f32 / g as f32 * 100.0,
            m as f32 / g as f32
        );
    }
}

#[test]
fn clubs_have_places_and_trips_have_lengths() {
    use crate::travel::{TripKind, distance_km};
    // Helsinki to Tallinn and Espoo, to Bergen.
    assert!(distance_km(0, 6) > 60 && distance_km(0, 6) < 130, "{}", distance_km(0, 6));
    assert_eq!(TripKind::of(distance_km(0, 6)), TripKind::Short);
    assert_eq!(distance_km(0, 16), distance_km(16, 0));
    assert!(distance_km(0, 16) < 20);
    let bergen = distance_km(0, 12);
    assert!(bergen > 900 && bergen < 1300, "{bergen}");
    assert_eq!(TripKind::of(bergen), TripKind::Long);
    // Tampere is a bus ride from Helsinki.
    assert_eq!(TripKind::of(distance_km(0, 1)), TripKind::Medium);
    for a in 0..20 {
        for b in 0..20 {
            assert_eq!(distance_km(a, b), distance_km(b, a));
        }
    }
}

#[test]
fn a_long_trip_tires_the_visitors_and_takes_the_day_before() {
    // Find a league game in a managed career whose away club has a long
    // trip: its squad arrives with less match fitness than before, the
    // home squad does not, and the day before is lost to the trip.
    let mut w = World::new_career(2, 20);
    w.apply(Command::TakeCharge(0)).unwrap();
    while w.day < crate::calendar::PRESEASON_DAYS + 2 {
        w.apply(Command::Continue).unwrap();
    }
    let i = w
        .fixtures
        .iter()
        .position(|f| f.round == 0 && f.home != 0 && f.away != 0 && crate::travel::distance_km(f.away, f.home) >= crate::travel::LONG_KM)
        .expect("a long trip in round 0 of twenty clubs");
    let (home, away, day) = (w.fixtures[i].home, w.fixtures[i].away, w.fixtures[i].day);
    let cond = |w: &World, t: usize| w.teams[t].roster.iter().map(|&p| w.players[p as usize].condition).sum::<f32>();
    // Rested squads: it is the trip that costs.
    for t in [home, away] {
        for &p in &w.teams[t].roster.clone() {
            w.players[p as usize].condition = 1.0;
        }
    }
    let (h0, a0) = (cond(&w, home), cond(&w, away));
    assert_eq!(w.travel_day_lost(away, day), Some(((day - 1) % 7) as usize));
    assert_eq!(w.travel_day_lost(home, day), None);
    w.travel_for(&[i]);
    let (dh, da) = (h0 - cond(&w, home), a0 - cond(&w, away));
    assert!(dh.abs() < 1e-3, "the home side did not travel: {dh}");
    let per_man = da / 12.0;
    assert!(per_man > 0.02 && per_man < 0.04, "a long trip costs a little: {per_man}");
    // A preparation session on the travel day does not count.
    use crate::training::Session;
    let lost = w.travel_day_lost(away, day).unwrap();
    let mut s = w.teams[away].schedule.clone();
    s.sessions = [Session::Rest; 10];
    s.sessions[lost * 2] = Session::MatchPrep;
    s.sessions[(lost * 2 + 2) % 10] = Session::MatchPrep;
    w.teams[away].schedule = s;
    assert_eq!(w.prep_sessions(away, day), 1);
    // The week's training: the lost day was a rest day.
    let week = w.week_schedule(away);
    assert_eq!(week.sessions[lost * 2], Session::Rest);
    assert_eq!(w.week_schedule(away).sessions[lost * 2], Session::MatchPrep, "the lost day is used up: the next week is whole");
}

#[test]
fn home_stands_help_and_road_trips_cost_more() {
    let mut w = World::new_career(2, 20);
    w.apply(Command::TakeCharge(0)).unwrap();
    let (a, b) = (3usize, 12usize);
    let ra = |w: &World, t: usize| w.teams[t].roster.iter().map(|&p| w.players[p as usize].condition).sum::<f32>() / 12.0;
    // Two away games in a row cost the second more than the first.
    w.fixtures.push(crate::league::Fixture { round: 0, home: b, away: a, result: None, playoff: false, day: 40, division: 0 });
    w.fixtures.push(crate::league::Fixture { round: 0, home: b, away: a, result: None, playoff: false, day: 47, division: 0 });
    let n = w.fixtures.len();
    for &p in &w.teams[a].roster.clone() {
        w.players[p as usize].condition = 1.0;
    }
    w.travel_for(&[n - 2]);
    let first = 1.0 - ra(&w, a);
    for &p in &w.teams[a].roster.clone() {
        w.players[p as usize].condition = 1.0;
    }
    w.travel_for(&[n - 1]);
    let second = 1.0 - ra(&w, a);
    assert!(second > first * 1.3, "first {first}, second {second}");
    // A home game after a home game gives something back.
    for &p in &w.teams[b].roster.clone() {
        w.players[p as usize].condition = 0.9;
    }
    let before = ra(&w, b);
    w.fixtures.push(crate::league::Fixture { round: 0, home: b, away: a, result: None, playoff: false, day: 54, division: 0 });
    let n = w.fixtures.len();
    w.travel_for(&[n - 1]);
    assert!(ra(&w, b) > before, "a home stand rests the squad");
}

#[test]
fn travel_only_applies_in_a_managed_calendar() {
    let mut w = World::new_career(2, 20);
    let i = w.fixtures.iter().position(|f| crate::travel::distance_km(f.away, f.home) >= crate::travel::LONG_KM).unwrap();
    let before: Vec<f32> = w.players.iter().map(|p| p.condition).collect();
    w.travel_for(&[i]);
    let after: Vec<f32> = w.players.iter().map(|p| p.condition).collect();
    assert_eq!(before, after, "nobody in charge: no travel");
}

#[test]
fn home_court_stays_realistic_with_travel() {
    // Aggregate over 12 careers of twelve clubs: home teams win a bit
    // more than half their games and by a few points, with or without
    // the toll of travel, and the toll adds only a little.
    let (g0, w0, m0) = career_home_court(1..=12, false);
    let (g1, w1, m1) = career_home_court(1..=12, true);
    let (win0, win1) = (w0 as f32 / g0 as f32, w1 as f32 / g1 as f32);
    let (mar0, mar1) = (m0 as f32 / g0 as f32, m1 as f32 / g1 as f32);
    eprintln!("home court, travel off: {:.1}% {:+.2}; on: {:.1}% {:+.2}", win0 * 100.0, mar0, win1 * 100.0, mar1);
    for (win, mar) in [(win0, mar0), (win1, mar1)] {
        assert!((0.50..=0.62).contains(&win), "home win share {win}");
        assert!((0.5..=3.5).contains(&mar), "home margin {mar}");
    }
    assert!(mar1 - mar0 < 1.0, "travel adds {}", mar1 - mar0);
}

#[test]
fn the_agenda_lists_games_trips_training_scouting_and_why_time_waits() {
    use crate::agenda::AgendaItem;
    let mut w = World::new_career(5, 12);
    w.apply(Command::TakeCharge(0)).unwrap();
    while w.day < crate::calendar::PRESEASON_DAYS - 3 {
        w.apply(Command::Continue).unwrap();
    }
    let a = w.agenda(21);
    assert_eq!(a.days.len(), 21);
    assert!(a.days[0].today);
    // Our games are there: the opening game and the first league weeks.
    let games: Vec<_> = a
        .days
        .iter()
        .flat_map(|d| d.items.iter())
        .filter_map(|i| if let AgendaItem::Game(g) = i { Some(g) } else { None })
        .collect();
    assert!(games.iter().any(|g| g.competition == "league"));
    let first = games.iter().find(|g| g.competition == "league").unwrap();
    assert_eq!(first.city, w.city_of(first.opponent));
    assert!(first.trip.is_some() == !first.home);
    // The scout can still be sent to games of theirs before we meet.
    assert!(first.watch.iter().all(|g| g.day >= w.day));
    // Training days are Monday to Friday and preparation is marked.
    w.apply(Command::SetSession { slot: 6, session: crate::training::Session::MatchPrep }).unwrap();
    assert!(w.apply(Command::SetSession { slot: 6, session: crate::training::Session::MatchPrep }).is_err());
    assert!(w.apply(Command::SetSession { slot: 10, session: crate::training::Session::Rest }).is_err());
    let a = w.agenda(14);
    let thu = a.days.iter().find(|d| w.weekday(d.day) == "Thu").unwrap();
    assert!(thu.items.iter().any(|i| matches!(i, AgendaItem::Training(t) if t.prep[0] && t.sessions[0] == "Match preparation")));
    assert!(a.days.iter().filter(|d| w.weekday(d.day) == "Sat").all(|d| !d.items.iter().any(|i| matches!(i, AgendaItem::Training(_)))));
    // A scout assignment shows on its day.
    let day = crate::calendar::FRIENDLY_DAYS[3];
    let other = w.friendlies.iter().find(|f| f.day == day && f.home != 0 && f.away != 0).unwrap().home;
    w.apply(Command::AssignScout { team: other, day }).unwrap();
    let a = w.agenda(14);
    assert!(a.days.iter().any(|d| d.items.iter().any(|i| matches!(i, AgendaItem::Scout { team, .. } if *team == other))));
    // Why it waits: nothing, then a game.
    assert!(a.hold.reason.starts_with("Nothing") || a.hold.reason.contains("message"), "{}", a.hold.reason);
    while !w.match_today() {
        w.apply(Command::Continue).unwrap();
    }
    let hold = w.agenda(3).hold;
    assert!(hold.reason.starts_with("A game is waiting"), "{}", hold.reason);
    assert!(hold.can.contains(&"Play match"));
    // And it serialises for the agents.
    assert!(serde_json::to_string(&w.agenda(21)).unwrap().contains("\"kind\":\"game\""));
}

#[test]
fn what_the_scout_sees_of_the_next_opponent_can_be_acted_on() {
    // The assignment from the agenda's own list of games works.
    let mut w = World::new_career(8, 12);
    w.apply(Command::TakeCharge(0)).unwrap();
    let a = w.agenda(45);
    let g = a.days.iter().flat_map(|d| d.items.iter()).find_map(|i| match i {
        crate::agenda::AgendaItem::Game(g) if g.competition == "league" => Some(g.clone()),
        _ => None,
    }).unwrap();
    let pick = g.watch.iter().find(|x| x.before_meeting).expect("one of their games before ours");
    w.apply(Command::AssignScout { team: g.opponent, day: pick.day }).unwrap();
    let again = w.agenda(45);
    let g2 = again.days.iter().flat_map(|d| d.items.iter()).find_map(|i| match i {
        crate::agenda::AgendaItem::Game(g) if g.competition == "league" => Some(g.clone()),
        _ => None,
    }).unwrap();
    assert!(g2.watch.iter().any(|x| x.assigned && x.day == pick.day));
}

/// A digest of a managed season with the scout out every day and
/// preparation set, to compare between the serial and parallel builds.
#[test]
#[ignore = "a measurement that prints numbers; run with --ignored --nocapture"]
fn career_digest() {
    let mut w = career(21, 14);
    for slot in [6usize, 7] {
        w.apply(Command::SetSession { slot, session: crate::training::Session::MatchPrep }).unwrap();
    }
    drive(&mut w, 12);
    let text = format!("{:?}", w.fixtures.iter().map(|f| f.result.as_ref().map(|r| (r.score, r.possessions))).collect::<Vec<_>>());
    let reports = format!("{:?}", w.scouting.opponent_reports.iter().map(|r| (r.team, r.day, r.confidence, r.offense, r.defense, r.through.clone())).collect::<Vec<_>>());
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.bytes().chain(reports.bytes()) {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    eprintln!("CAREER DIGEST {h:016x} reports {}", w.scouting.opponent_reports.len());
}

#[test]
fn a_live_day_and_an_instant_day_give_the_same_world_with_scout_prep_and_travel() {
    use crate::league::Scope;
    let mut w = World::new_career(11, 12);
    w.apply(Command::TakeCharge(0)).unwrap();
    w.teams[0].office.staff[Job::Scout as usize].as_mut().unwrap().judgement = 18;
    // A report on a club, and preparation set.
    let day = crate::calendar::FRIENDLY_DAYS[0];
    let opp = w.friendlies.iter().find(|f| f.day == day && f.home != 0 && f.away != 0).unwrap().home;
    with_report(&mut w, opp, (true, true));
    prep_sessions(&mut w, 4);
    // On to the first league day of ours.
    while !w.match_today() {
        w.apply(Command::Continue).unwrap();
    }
    // The scout goes to another club's game today.
    assert!(send_scout_today(&mut w), "another game today to watch");
    let reports = w.scouting.opponent_reports.len();
    let mut instant = w.clone();
    let mut live = w.clone();
    let mut late = w.clone();
    instant.apply(Command::PlayToday).unwrap();
    let mine = live.user_fixture();
    let run = |w: &mut World, clear: bool| {
        let me = w.watch.unwrap();
        let mut round = w.kickoff_scoped(me, Scope::Day).unwrap();
        if clear {
            round.pre.clear();
            round.around.clear();
        }
        while !round.game.is_over() {
            round.step();
        }
        w.full_time(round)
    };
    let played = run(&mut live, false);
    run(&mut late, true);
    assert!(played.len() > 1);
    for &i in played.iter().filter(|&&i| Some(i) != mine) {
        assert_eq!(
            serde_json::to_string(&instant.fixtures[i].result).unwrap(),
            serde_json::to_string(&live.fixtures[i].result).unwrap(),
            "fixture {i}"
        );
    }
    let state = |w: &World| {
        let mut w = w.clone();
        w.log.clear();
        w.replay = None;
        w.tapes.clear();
        serde_json::to_string(&w).unwrap()
    };
    assert_eq!(state(&late), state(&live));
    assert_eq!(live.scouting.opponent_reports.len(), reports + 1, "the scouted game was written up");
    assert_eq!(instant.scouting.opponent_reports.len(), reports + 1);
    assert_eq!(
        serde_json::to_string(&instant.scouting.opponent_reports.last()).unwrap(),
        serde_json::to_string(&live.scouting.opponent_reports.last()).unwrap(),
        "the same report either way"
    );
}
