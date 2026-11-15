//! What must hold about clubs, boards and the manager's career.

use crate::club::{PAST_YEARS, START_YEAR};
use crate::command::Command;
use crate::inbox::Stop;
use crate::league::World;

fn play_rounds(w: &mut World, rounds: usize) {
    let mut played = 0;
    for _ in 0..100_000 {
        if played >= rounds {
            return;
        }
        match w.apply(Command::Advance).unwrap().stop.unwrap() {
            Stop::MatchDay => {
                w.apply(Command::PlayRound).unwrap();
                played += 1;
            }
            Stop::Mail => {}
            Stop::Draft => {
                let _ = w.apply(Command::DraftAuto { all: true });
            }
        }
    }
    panic!("rounds never finished");
}

#[test]
fn every_year_has_one_champion_and_the_record_is_continuous() {
    let mut w = World::generate(11, 12);
    // Before the game: each year's champion is one club's title.
    for year in (START_YEAR - PAST_YEARS)..START_YEAR {
        let champs: Vec<usize> = (0..12)
            .filter(|&t| w.honours(t).titles.contains(&year))
            .collect();
        assert_eq!(champs.len(), 1, "year {year}: {champs:?}");
        let seconds = (0..12)
            .filter(|&t| w.honours(t).finals.contains(&year))
            .count();
        assert_eq!(seconds, 1, "year {year}");
    }
    // The seasons played extend it.
    w.apply(Command::TakeCharge(0)).unwrap();
    while w.season < 2 {
        match w.apply(Command::Advance).unwrap().stop.unwrap() {
            Stop::MatchDay => {
                w.apply(Command::PlayRound).unwrap();
            }
            Stop::Mail => {}
            Stop::Draft => {
                let _ = w.apply(Command::DraftAuto { all: true });
            }
        }
        if w.watch.is_none() {
            break;
        }
    }
    for year in (START_YEAR - PAST_YEARS)..(START_YEAR + w.history.len() as u16) {
        let champs = (0..12)
            .filter(|&t| w.honours(t).titles.contains(&year))
            .count();
        assert_eq!(champs, 1, "year {year}");
    }
    let rec = w.club_record(3);
    assert_eq!(rec.len() >= PAST_YEARS as usize - 20, true);
    assert!(rec.windows(2).all(|p| p[0].year + 1 == p[1].year));
    assert_eq!(rec.last().unwrap().year, START_YEAR + w.history.len() as u16 - 1);
}

#[test]
fn identities_are_a_function_of_the_seed_and_an_old_save_gets_the_same() {
    let a = World::generate(5, 8);
    let b = World::generate(5, 8);
    assert_eq!(format!("{:?}", a.club), format!("{:?}", b.club));
    let c = World::generate(6, 8);
    assert_ne!(format!("{:?}", a.club), format!("{:?}", c.club));
    // A save from before clubs had histories: they are made on load.
    let mut old = a.clone();
    old.club = Default::default();
    old.ensure_clubs();
    assert_eq!(format!("{:?}", old.club), format!("{:?}", a.club));
    for t in 0..8 {
        let id = a.identity(t);
        assert!(id.rival != t && a.identity(id.rival).rival == t);
        assert!(!id.legends.is_empty());
    }
}

#[test]
fn a_patient_owner_tolerates_a_longer_bad_run() {
    let run = |patience: u8| -> usize {
        let mut w = World::generate(3, 12);
        w.apply(Command::TakeCharge(0)).unwrap();
        w.club.clubs[0].board.owner.patience = patience;
        w.club.clubs[0].board.chairman = None;
        w.round = 9;
        // The club is bottom of the table.
        for t in 1..12 {
            w.standings[t].wins = 5;
        }
        let mut games = 0;
        while w.watch.is_some() && games < 200 {
            // Lost to a team of no special interest, by a few points.
            w.board_game(0, 5, false, 4, true);
            w.board_verdict(0, 0, 9);
            games += 1;
        }
        assert!(w.watch.is_none(), "never dismissed");
        games
    };
    let (patient, impatient) = (run(19), run(2));
    assert!(patient > impatient + 5, "patient {patient}, impatient {impatient}");
    // The warning comes before the dismissal.
    let mut w = World::generate(3, 12);
    w.apply(Command::TakeCharge(0)).unwrap();
    w.round = 9;
    for t in 1..12 {
        w.standings[t].wins = 5;
    }
    while !w.club.clubs[0].board.warned {
        w.board_game(0, 5, false, 4, true);
        w.board_verdict(0, 0, 9);
    }
    assert!(w.watch.is_some());
    assert!(w.desk.messages.iter().any(|m| m.subject == "A warning"));
}

#[test]
fn the_board_moves_with_results_and_says_why() {
    let mut w = World::generate(9, 12);
    w.apply(Command::TakeCharge(2)).unwrap();
    let start = w.club.clubs[2].board.confidence;
    assert_eq!(start, 60.0 + ((w.career.reputation - w.stature(2) as i32) / 5) as f32);
    play_rounds(&mut w, 6);
    let b = w.boardroom(2);
    assert!(b.confidence != start || w.watch.is_none());
    let s = w.club_summary(2);
    assert_eq!(s.people.is_empty(), false);
    assert!(s.squad.assistant.len() >= 2);
}

#[test]
fn the_front_office_changes_over_the_years_and_the_user_hears_of_it() {
    let mut w = World::generate(4, 12);
    let before: Vec<String> = (0..12).map(|t| w.boardroom(t).owner.name.clone()).collect();
    let gms: Vec<String> = (0..12).map(|t| w.teams[t].office.gm.name.clone()).collect();
    w.apply(Command::TakeCharge(1)).unwrap();
    let mut seasons = 0;
    let mut guard = 0;
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
    }
    let owners = (0..12)
        .filter(|&t| w.boardroom(t).owner.name != before[t])
        .count();
    let gm_changes = (0..12)
        .filter(|&t| w.teams[t].office.gm.name != gms[t])
        .count();
    assert!(seasons >= 3);
    assert!(owners + gm_changes > 0, "{owners} {gm_changes} after {seasons} seasons");
    assert!(
        w.desk.messages.iter().any(|m| m.subject.starts_with("New owner")
            || m.subject.starts_with("Changes at"))
    );
}

use crate::inbox::OfferState;
use crate::staff::Job;

/// Club 1 loses its manager and calls: the world, and the offer's index.
fn offer_from(w: &mut World, club: usize) -> usize {
    w.career.reputation = 95;
    w.club.clubs[club].identity.stature = 90;
    w.manager_out(club, "test");
    let open = w.open_job_offers();
    *open
        .iter()
        .find(|&&i| w.career.offers[i].club == club)
        .expect("the club called")
}

#[test]
fn staff_who_are_asked_and_unwilling_stay_behind() {
    let mut w = World::generate(21, 12);
    w.apply(Command::TakeCharge(0)).unwrap();
    let offer = offer_from(&mut w, 1);
    // One man who will come and one who will not (names decide it).
    let verdict = |w: &World, job: Job| {
        let s = w.teams[0].office.staffer(job).unwrap();
        w.staff_will(1, job, s).willing
    };
    let tweak = |w: &mut World, job: Job, want: bool| {
        for i in 0..400 {
            w.teams[0].office.slot_mut(job).as_mut().unwrap().name = format!("Test Person{i}");
            if verdict(w, job) == want {
                return;
            }
        }
        panic!("no name found");
    };
    tweak(&mut w, Job::Assistant, true);
    tweak(&mut w, Job::Fitness, false);
    let coming = w.teams[0].office.staffer(Job::Assistant).unwrap().name.clone();
    let staying = w.teams[0].office.staffer(Job::Fitness).unwrap().name.clone();
    let keep = vec![Job::Skills, Job::Scout, Job::Press];
    let out = w
        .apply(Command::AcceptJob {
            offer,
            bring: vec![Job::Assistant, Job::Fitness],
            keep,
        })
        .unwrap();
    assert!(out.note.unwrap().contains(&staying));
    assert_eq!(w.watch, Some(1));
    assert_eq!(w.teams[1].office.staffer(Job::Assistant).unwrap().name, coming);
    assert_ne!(w.teams[1].office.staffer(Job::Fitness).map(|s| s.name.clone()), Some(staying.clone()));
    // He is still at the old club.
    assert_eq!(w.teams[0].office.staffer(Job::Fitness).unwrap().name, staying);
    // The old club's assistant is gone and a replacement stands in.
    assert_ne!(w.teams[0].office.staffer(Job::Assistant).map(|s| s.name.clone()), Some(coming));
    // A new head coach is on the old bench, and the old club is AI now.
    assert!(w.teams[0].office.staffer(Job::HeadCoach).is_some());
}

#[test]
fn an_owner_who_loves_his_people_refuses_a_purge() {
    let mut w = World::generate(21, 12);
    w.apply(Command::TakeCharge(0)).unwrap();
    let offer = offer_from(&mut w, 1);
    for who in [&mut w.club.clubs[1].board.owner] {
        who.loyalty = 20;
    }
    w.club.clubs[1].board.chairman = None;
    let r = w.apply(Command::AcceptJob { offer, bring: vec![], keep: vec![] });
    assert!(r.is_err(), "{r:?}");
    assert_eq!(w.watch, Some(0), "nothing changed");
}

#[test]
fn changing_club_leaves_nothing_of_the_old_job_on_the_desk() {
    let mut w = World::generate(21, 12);
    w.apply(Command::TakeCharge(0)).unwrap();
    play_rounds(&mut w, 2);
    let target = w.teams[5].roster[0];
    w.apply(Command::Shortlist { player: target, on: true }).unwrap();
    let _ = w.apply(Command::Bid { player: w.teams[6].roster[0], fee: 1_000 });
    w.teams[0].instructions.rules.push(crate::instructions::Rule { name: "mine".into(), when: vec![], then: vec![], max_fires: 1, cooldown_secs: 60 });
    assert!(!w.scouting.shortlist.is_empty());
    let offer = offer_from(&mut w, 1);
    w.apply(Command::AcceptJob { offer, bring: vec![], keep: vec![Job::Assistant, Job::Skills, Job::Fitness, Job::Scout, Job::Press] }).unwrap();
    assert_eq!(w.watch, Some(1));
    assert!(w.scouting.shortlist.is_empty(), "the old shortlist is gone");
    assert!(w.desk.bids.is_empty());
    assert!(w.desk.offers.is_empty());
    assert!(w.desk.press.is_empty());
    assert!(w.desk.pledge.is_none());
    assert!(w.fame.asks.is_empty());
    assert!(w.tapes.is_empty());
    assert_eq!(w.desk.expectation, w.board_target(1).max(w.desk.expectation));
    assert_eq!(w.desk.expectation, w.expected_finish(1));
    // Nothing in the mail points at an old offer, question or request.
    assert!(w
        .desk
        .messages
        .iter()
        .all(|m| m.offer.is_none() && m.press.is_none() && m.appearance.is_none()));
    assert_eq!(format!("{:?}", w.teams[0].instructions), format!("{:?}", crate::instructions::Instructions::default()));
    assert!(w.teams[0].delegate_training);
    // The record has both jobs.
    assert_eq!(w.career.spells.len(), 2);
    assert_eq!(w.career.spells[0].ended, crate::career::Ending::Moved);
    // And the game plays on.
    play_rounds(&mut w, 3);
}

#[test]
fn a_dismissed_manager_takes_another_job_and_the_log_replays() {
    let prep = |w: &mut World| {
        for who in [&mut w.club.clubs[0].board.owner] {
            who.patience = 1;
            who.ambition = 20;
        }
        w.club.clubs[0].board.chairman = None;
    };
    let mut first = World::generate(7, 12);
    prep(&mut first);
    // The weakest squad takes the job.
    first.apply(Command::TakeCharge(0)).unwrap();
    let mut guard = 0;
    while first.watch.is_some() && guard < 200_000 {
        guard += 1;
        match first.apply(Command::Advance).unwrap().stop.unwrap() {
            Stop::MatchDay => {
                first.apply(Command::PlayRound).unwrap();
            }
            Stop::Mail => {}
            Stop::Draft => {
                let _ = first.apply(Command::DraftAuto { all: true });
            }
        }
        if first.season >= 4 {
            break;
        }
    }
    if first.watch.is_some() {
        // Not sacked by results: make sure the path is still covered.
        first.apply(Command::Resign).unwrap();
    }
    assert!(first.jobless());
    assert!(first.desk.messages.iter().any(|m| m.subject == "You are dismissed" || m.subject == "Your resignation"));
    // Out of work, time goes on until a club calls.
    let mut guard = 0;
    while first.open_job_offers().is_empty() && guard < 200_000 {
        guard += 1;
        match first.apply(Command::Advance).unwrap().stop.unwrap() {
            Stop::MatchDay => {
                first.apply(Command::PlayRound).unwrap();
            }
            _ => {}
        }
    }
    let offer = first.open_job_offers()[0];
    let club = first.career.offers[offer].club;
    first
        .apply(Command::AcceptJob { offer, bring: vec![], keep: Job::HIRED.to_vec() })
        .unwrap();
    assert_eq!(first.watch, Some(club));
    assert_eq!(first.career.offers[offer].state, OfferState::Accepted);
    play_rounds(&mut first, 4);

    let mut second = World::generate(7, 12);
    prep(&mut second);
    for cmd in first.log.clone() {
        second.apply(cmd).unwrap();
    }
    assert_eq!(second.log, first.log);
    assert!(format!("{first:?}") == format!("{second:?}"), "worlds differ after replay");
}
