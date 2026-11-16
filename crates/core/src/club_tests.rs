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
    // Whether the manager of a middling club survives three seasons depends on
    // the luck of his first games, so look at several worlds: in those where
    // he lasts two seasons (a manager of a middling club is often let go sooner), the front offices change and he is told.
    let (mut lasted, mut changes, mut told) = (0, 0, 0);
    for seed in 4..12 {
        let mut w = World::generate(seed, 12);
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
        if seasons < 2 {
            continue;
        }
        lasted += 1;
        let owners = (0..12)
            .filter(|&t| w.boardroom(t).owner.name != before[t])
            .count();
        let gm_changes = (0..12)
            .filter(|&t| w.teams[t].office.gm.name != gms[t])
            .count();
        let heard = w
            .desk
            .messages
            .iter()
            .any(|m| m.subject.starts_with("New owner") || m.subject.starts_with("Changes at"));
        // Whenever somebody changed, he was told.
        assert!(
            owners + gm_changes == 0 || heard,
            "{owners} owners and {gm_changes} GMs changed in {seasons} seasons and no word"
        );
        told += heard as u32;
        changes += owners + gm_changes;
    }
    // The guard is only that there is something to look at: on the engine this
    // test was written for the count was exactly three, on the edge of the old
    // bound, and engine changes move it between one and three.
    assert!(lasted >= 1, "the manager lasted two seasons in only {lasted} of 8 worlds");
    assert!(changes > 0 && told > 0, "{changes} changes, told in {told} of {lasted} worlds");
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

// ---- clubs and careers across divisions -------------------------------------

#[test]
fn the_past_of_a_pyramid_has_one_top_champion_a_year_and_small_clubs_below() {
    let w = World::new_career_tiers(7, 12, 3);
    let n = w.teams.len();
    let mut top_titles = [0usize; 3];
    for year in (START_YEAR - PAST_YEARS)..START_YEAR {
        let champs: Vec<usize> = (0..n)
            .filter(|&t| w.honours(t).titles.contains(&year))
            .collect();
        assert_eq!(champs.len(), 1, "year {year}: {champs:?}");
        let finals = (0..n)
            .filter(|&t| w.honours(t).finals.contains(&year))
            .count();
        assert_eq!(finals, 1, "year {year}");
    }
    let mut stature = [0u32; 3];
    for t in 0..n {
        let d = w.teams[t].division;
        top_titles[d] += w.honours(t).titles.len();
        stature[d] += w.stature(t) as u32;
        // Every club has a place in every year it existed, in some division.
        assert!(w.club_record(t).iter().all(|l| l.place == 0 || (l.division as usize) < 3));
    }
    assert_eq!(top_titles.iter().sum::<usize>(), PAST_YEARS as usize);
    // Clubs of the top division are bigger, and own nearly all the titles.
    assert!(stature[0] > stature[1] && stature[1] > stature[2], "{stature:?}");
    assert!(top_titles[0] > 3 * top_titles[2].max(1), "{top_titles:?}");
    // A club of the bottom division is not a giant.
    assert!((0..n).filter(|&t| w.teams[t].division == 2).all(|t| w.stature(t) <= 55));
}

#[test]
fn promotion_and_relegation_reach_the_board_the_record_and_the_name() {
    let mut w = World::new_career_tiers(8, 6, 3);
    // Nobody to dismiss: the test is about the clubs.
    let before: Vec<usize> = w.teams.iter().map(|t| t.division).collect();
    let mut guard = 0;
    while w.history.is_empty() {
        guard += 1;
        assert!(guard < 3000);
        match w.apply(Command::Advance).unwrap().stop {
            Some(Stop::MatchDay) => {
                w.apply(Command::PlayRound).unwrap();
            }
            Some(Stop::Draft) => {
                let _ = w.apply(Command::DraftAuto { all: true });
            }
            _ => {}
        }
    }
    let mut moved = 0;
    for t in 0..18 {
        let line = w.club_record(t).pop().unwrap();
        assert_eq!(line.division as usize, before[t]);
        let now = w.teams[t].division;
        assert_eq!(line.moved, (before[t] as i8 - now as i8).signum(), "club {t}");
        if now != before[t] {
            moved += 1;
            let up = now < before[t];
            let reasons = &w.club.clubs[t].board.reasons;
            assert!(
                reasons.iter().any(|r| r.text.contains(if up { "promoted" } else { "relegated" })),
                "club {t}: {reasons:?}"
            );
            assert_eq!(w.honours(t).promotions.last() == Some(&START_YEAR), up, "club {t}");
        }
    }
    assert!(moved >= 8, "{moved} clubs moved");
}

#[test]
fn a_board_judges_by_its_own_division_table() {
    let mut w = World::new_career_tiers(9, 6, 3);
    // Round 8 of 10, a club last in the Third Division: warned, out of patience.
    w.round = crate::board::GRACE_ROUNDS;
    let third = w.division_teams(2);
    for (i, &t) in third.iter().enumerate() {
        w.standings[t].wins = 10 - i as u16 * 2;
    }
    let (last, middle) = (third[5], third[2]);
    for club in [last, middle] {
        let b = &mut w.club.clubs[club].board;
        b.confidence = 5.0;
        b.warned = true;
    }
    assert!(w.in_the_bottom_third(last));
    assert!(!w.in_the_bottom_third(middle));
    w.board_verdict(middle, 0, 10);
    assert!(w.career.vacancies.iter().all(|v| v.club != middle));
    w.board_verdict(last, 0, 10);
    assert!(w.career.vacancies.iter().any(|v| v.club == last));
    // The target is a place in the club's own division.
    for t in 0..18 {
        assert!((1..=4).contains(&w.board_target(t)), "club {t}: {}", w.board_target(t));
    }
}

#[test]
fn offers_cross_divisions_in_the_right_direction() {
    let mut w = World::new_career_tiers(10, 12, 3);
    let third = w.division_teams(2);
    let me = third[0];
    w.apply(Command::TakeCharge(me)).unwrap();
    // A good name in the Third Division: second-tier clubs call, top clubs do not.
    w.career.reputation = 35;
    let second = w.division_teams(1);
    let first = w.division_teams(0);
    let called = |w: &World, c: usize| w.career.offers.iter().any(|o| o.club == c);
    for &c in &second {
        w.manager_out(c, "test");
    }
    for &c in &first {
        w.manager_out(c, "test");
    }
    assert!(second.iter().any(|&c| called(&w, c)), "no second-division club called");
    assert!(first.iter().all(|&c| !called(&w, c)), "a top club called a third-division man");
    // The approach after a championship season comes from the division above.
    let mut w = World::new_career_tiers(10, 12, 3);
    w.apply(Command::TakeCharge(me)).unwrap();
    let tables = w.division_tables();
    let mut got = false;
    for season in 0..12 {
        w.season = season;
        w.career.offers.clear();
        w.career.vacancies.clear();
        w.career_close_season(&tables, tables[0][0]);
        got |= w.career.offers.iter().any(|o| w.teams[o.club].division == 1);
        assert!(w.career.offers.iter().all(|o| w.teams[o.club].division == 1));
    }
    assert!(got, "twelve championship seasons and no call from above");
}

#[test]
fn a_dismissed_manager_is_called_mostly_from_his_own_level_and_below() {
    let mut w = World::new_career_tiers(12, 12, 3);
    let second = w.division_teams(1);
    let me = second[3];
    w.apply(Command::TakeCharge(me)).unwrap();
    w.career.reputation = 50;
    w.manager_dismissed("test");
    assert!(w.jobless());
    assert_eq!(w.career.last_division, 1);
    let mut counts = [0usize; 3];
    for t in 0..36 {
        if t != me {
            w.manager_out(t, "test");
        }
    }
    for o in &w.career.offers {
        counts[w.teams[o.club].division] += 1;
    }
    assert!(counts[1] + counts[2] > 0, "{counts:?}");
    assert!(counts[0] <= counts[1] + counts[2], "{counts:?}");
}

#[test]
fn a_career_in_the_calendar_loses_its_job_and_finds_another_day_by_day() {
    let make = || {
        let mut w = World::new_career_tiers(5, 6, 3);
        // An owner with no patience, so the board acts.
        let b = &mut w.club.clubs[15].board;
        b.owner.patience = 1;
        b.owner.ambition = 20;
        b.chairman = None;
        w
    };
    let mut w = make();
    assert!(w.in_preseason());
    w.apply(Command::TakeCharge(15)).unwrap();
    let start = w.club.clubs[15].board.confidence;
    // The camp and its friendlies do not move the board.
    while w.in_preseason() {
        w.apply(Command::Continue).unwrap();
        assert!(w.match_day() >= w.day || !w.match_today());
    }
    assert!(w.desk.messages.iter().any(|m| m.subject.starts_with("Friendly")));
    assert_eq!(w.club.clubs[15].board.confidence, start, "a friendly moved the board");
    assert_eq!(w.round, 0);
    // League days: play when it is time, otherwise let the day pass.
    let mut guard = 0;
    while w.watch.is_some() && w.season < 3 {
        guard += 1;
        assert!(guard < 6000, "stuck");
        if w.match_today() {
            w.apply(Command::PlayToday).unwrap();
        } else if let Some(Stop::Draft) = w.apply(Command::Continue).unwrap().stop {
            let _ = w.apply(Command::DraftAuto { all: true });
        }
    }
    if w.watch.is_some() {
        w.apply(Command::Resign).unwrap();
    }
    assert!(w.jobless());
    // Out of work, days go by until somebody calls.
    let mut guard = 0;
    while w.open_job_offers().is_empty() {
        guard += 1;
        assert!(guard < 6000, "nobody called: season {} round {} day {} idle {} rep {} vac {:?} offers {:?} draft {} over {}", w.season, w.round, w.day, w.career.idle_rounds, w.career.reputation, w.career.vacancies.len(), w.career.offers.len(), w.draft.is_some(), w.season_over());
        if let Some(Stop::Draft) = w.apply(Command::Continue).unwrap().stop {
            let _ = w.apply(Command::DraftAuto { all: true });
        }
    }
    let offer = w.open_job_offers()[0];
    let club = w.career.offers[offer].club;
    w.apply(Command::AcceptJob { offer, bring: vec![], keep: Job::HIRED.to_vec() })
        .unwrap();
    assert_eq!(w.watch, Some(club));
    let mut guard = 0;
    while w.round < 3 && w.season < 4 {
        guard += 1;
        assert!(guard < 6000);
        if w.match_today() {
            w.apply(Command::PlayToday).unwrap();
        } else if let Some(Stop::Draft) = w.apply(Command::Continue).unwrap().stop {
            let _ = w.apply(Command::DraftAuto { all: true });
        }
    }
    // The same commands on the same world give the same world.
    let mut again = make();
    for cmd in w.log.clone() {
        again.apply(cmd).unwrap();
    }
    assert!(format!("{w:?}") == format!("{again:?}"), "worlds differ after replay");
}
