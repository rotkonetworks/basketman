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
#[ignore]
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
#[ignore]
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
