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
