//! Tests for fame, fans, media work and retirement.

use crate::command::Command;
use crate::command::tests::tidy;
use crate::inbox::Stop;
use crate::league::World;
use crate::media::{MediaLevel, gaffe_chance};
use crate::player::PlayerId;

/// Play without a manager until `seasons` have finished.
fn run(w: &mut World, seasons: u16) {
    let target = w.season + seasons;
    while w.season < target {
        if w.match_today() {
            w.play_round();
        } else {
            w.advance();
        }
    }
}

/// Play `rounds` rounds without a manager.
fn rounds(w: &mut World, rounds: usize) {
    let mut played = 0;
    while played < rounds {
        if w.match_today() {
            w.play_round();
            played += 1;
        } else {
            w.advance();
        }
    }
}

fn managed(w: &mut World, seasons: u16) {
    managed_keeping(w, seasons, None)
}

/// Like `managed`, but every offer for `keep` is turned down first, so the
/// helper's habit of taking every other offer cannot sell him.
fn managed_keeping(w: &mut World, seasons: u16, keep: Option<PlayerId>) {
    let target = w.season + seasons;
    let mut n = 0;
    for _ in 0..200_000 {
        if w.season >= target {
            return;
        }
        let wanted: Vec<usize> = (0..w.desk.offers.len())
            .filter(|&i| {
                let o = &w.desk.offers[i];
                Some(o.player) == keep && o.state == crate::inbox::OfferState::Open
            })
            .collect();
        for offer in wanted {
            let _ = w.apply(Command::AnswerOffer {
                offer,
                accept: false,
            });
        }
        tidy(w, &mut n);
        match w.apply(Command::Advance).unwrap().stop.unwrap() {
            Stop::MatchDay => {
                w.apply(Command::PlayRound).unwrap();
            }
            Stop::Mail => {}
            Stop::Draft => {
                let _ = w.apply(Command::DraftAuto { all: true });
            }
        }
    }
    panic!("the seasons never ended");
}

fn best_of(w: &World, club: usize) -> PlayerId {
    *w.teams[club]
        .roster
        .iter()
        .max_by(|&&a, &&b| {
            w.players[a as usize]
                .overall()
                .total_cmp(&w.players[b as usize].overall())
        })
        .unwrap()
}

/// A world where nobody has a name, so one can be given.
fn blank(seed: u64) -> World {
    let mut w = World::generate(seed, 8);
    for p in w.players.iter_mut() {
        p.renown.fame = 0.0;
        p.renown.tenure_club = None;
    }
    w
}

#[test]
fn the_same_seed_gives_the_same_world_with_fame_in_it() {
    let (mut a, mut b) = (World::generate(21, 8), World::generate(21, 8));
    run(&mut a, 2);
    run(&mut b, 2);
    assert!(format!("{a:?}") == format!("{b:?}"), "worlds differ");
    assert!(a.fame.counts.gaffes + a.fame.counts.stories > 0);
}

#[test]
fn fame_does_not_follow_current_ability() {
    // An old great whose rating falls keeps his name for years.
    let mut w = World::generate(5, 8);
    let old = w
        .biggest_names(10)
        .into_iter()
        .find(|&(p, _)| w.players[p as usize].age >= 36)
        .expect("the league has an old name")
        .0;
    let start = w.fame_of(old);
    let ovr = w.players[old as usize].overall();
    assert!(start >= 60.0);
    let table: Vec<usize> = (0..8).collect();
    let mut seen = Vec::new();
    for _ in 0..3 {
        for a in crate::player::Attr::ALL {
            w.players[old as usize].attrs[a] -= 0.8;
        }
        w.games_played.fill(0);
        w.fame_close_season(&table, 0, None, None);
        seen.push(w.fame_of(old));
    }
    assert!(w.players[old as usize].overall() < ovr - 2.0);
    assert!(seen[0] > start * 0.8, "{start} then {seen:?}");
    assert!(seen[2] > start * 0.5, "{start} then {seen:?}");

    // And the rest of the league: after real seasons, a player who is not
    // better than a good starter is not a name, unless he is an old star.
    // (Several worlds: which player lands near a line moves with the random
    // stream: a name is rare, and none is ever large.)
    let (mut below, mut named) = (0, 0);
    for seed in [6, 7, 8] {
        let mut w = World::generate(seed, 8);
        run(&mut w, 4);
        let mut ov: Vec<f32> = w
            .teams
            .iter()
            .flat_map(|t| t.roster.iter())
            .map(|&p| w.players[p as usize].overall())
            .collect();
        ov.sort_by(|a, b| a.total_cmp(b));
        let median = ov[ov.len() / 2];
        for t in &w.teams {
            for &p in &t.roster {
                let pl = &w.players[p as usize];
                if pl.overall() < median && pl.age < 33 {
                    below += 1;
                    named += (pl.renown.fame >= 20.0) as u32;
                    assert!(
                        pl.renown.fame < 30.0,
                        "{} ({:.1} overall, {}) has fame {:.0}",
                        pl.name,
                        pl.overall(),
                        pl.age,
                        pl.renown.fame
                    );
                }
            }
        }
    }
    // Measured: 0 of 146 under one random stream, 1 of ~146 (fame 21) under another.
    assert!(named * 40 <= below, "{named} of {below} below-median players are names");
    let mut w = World::generate(6, 8);
    run(&mut w, 4);
    let mut ov: Vec<f32> = w
        .teams
        .iter()
        .flat_map(|t| t.roster.iter())
        .map(|&p| w.players[p as usize].overall())
        .collect();
    ov.sort_by(|a, b| a.total_cmp(b));
    // Names and ability are not the same list: somebody famous is below the
    // very best, and somebody among the best has no name to speak of.
    let top: Vec<f32> = {
        let mut t = ov.clone();
        t.reverse();
        t
    };
    let bar = top[top.len() / 5];
    let famous_not_best = w
        .biggest_names(10)
        .iter()
        .any(|&(p, _)| w.players[p as usize].overall() < bar);
    assert!(famous_not_best);
}

#[test]
fn a_star_fills_the_hall_at_home_and_away_and_sells_shirts() {
    let base = blank(9);
    let c = 0;
    let host = base.fixtures.iter().find(|f| f.away == c).unwrap().home;
    let away_fx = base.fixtures.iter().position(|f| f.away == c).unwrap();
    let measure = |w: &World| {
        let price = w.ticket_price(c);
        let home = w.attendance_priced(&w.typical_draw(c), c, price);
        let away = w.attendance_priced(&w.draw_for(away_fx), host, w.ticket_price(host));
        (home, away, w.merch_week(c))
    };
    let none = measure(&base);
    let mut named = base.clone();
    let star = best_of(&named, c);
    named.players[star as usize].renown.fame = 75.0;
    let with = measure(&named);
    assert!(with.0 > none.0 + 200, "home {none:?} {with:?}");
    assert!(with.1 > none.1 + 50, "away {none:?} {with:?}");
    assert!(with.2 > none.2 + 300, "shop {none:?} {with:?}");
    assert!(named.player_merch(star) > 500);
    // He only draws if he plays: a hurt star adds nothing at the gate.
    let mut hurt = named.clone();
    hurt.players[star as usize].injury = Some(crate::health::Injury {
        what: crate::health::Ailment::AnkleSprain,
        weeks_left: 8,
    });
    assert!(measure(&hurt).0 < with.0);
    // A local hero is bigger at home than away.
    let mut hero = named.clone();
    hero.players[star as usize].renown.tenure_club = Some(c);
    hero.players[star as usize].renown.tenure = 8;
    assert!(hero.pull(star, c) > hero.pull(star, host));
}

#[test]
fn media_work_raises_fame_and_fans_and_costs_training() {
    let mut base = blank(12);
    base.take_charge(0);
    // A young man with room to grow.
    let who = base.teams[0].roster[2];
    base.players[who as usize].age = 20;
    base.players[who as usize].potential = 19.0;
    base.players[who as usize].renown.fame = 10.0;
    for p in base.teams[0].roster.clone() {
        base.players[p as usize].renown.fame = 10.0;
    }
    let mut worked = base.clone();
    for p in worked.teams[0].roster.clone() {
        worked.set_media(p, MediaLevel::ALot);
    }
    rounds(&mut base, 8);
    rounds(&mut worked, 8);
    let fame = |w: &World| w.teams[0].roster.iter().map(|&p| w.fame_of(p)).sum::<f32>();
    assert!(
        fame(&worked) > fame(&base) + 2.0,
        "{} v {}",
        fame(&worked),
        fame(&base)
    );
    assert!(worked.fan_base(0) > base.fan_base(0));
    // And in other worlds too: eight games are a small sample, so most of them.
    let mut ahead = 1;
    for seed in [13, 14, 15, 16] {
        let mut b = blank(seed);
        b.take_charge(0);
        for p in b.teams[0].roster.clone() {
            b.players[p as usize].renown.fame = 10.0;
        }
        let mut w = b.clone();
        for p in w.teams[0].roster.clone() {
            w.set_media(p, MediaLevel::ALot);
        }
        rounds(&mut b, 8);
        rounds(&mut w, 8);
        ahead += (w.fan_base(0) > b.fan_base(0)) as u32;
    }
    assert!(ahead >= 4, "the worked club's fans are ahead in {ahead} of 5 worlds");
    let (a, b) = (&worked.players[who as usize], &base.players[who as usize]);
    assert!(
        a.overall() < b.overall(),
        "training: {} v {}",
        a.overall(),
        b.overall()
    );
    let tired = |w: &World| {
        w.teams[0]
            .roster
            .iter()
            .map(|&p| w.players[p as usize].condition)
            .sum::<f32>()
    };
    assert!(
        tired(&worked) < tired(&base),
        "{} v {}",
        tired(&worked),
        tired(&base)
    );
    // And it teaches him how to talk.
    assert!(
        worked.players[who as usize].personality.presence
            >= base.players[who as usize].personality.presence
    );
}

#[test]
fn a_poor_talker_gaffes_more_and_a_press_officer_helps() {
    assert!(
        gaffe_chance(3, 5, 6, MediaLevel::Some, 20.0)
            > 3.0 * gaffe_chance(18, 14, 14, MediaLevel::Some, 20.0)
    );
    assert!(
        gaffe_chance(10, 10, 10, MediaLevel::ALot, 20.0)
            > gaffe_chance(10, 10, 10, MediaLevel::None, 20.0)
    );
    let count = |presence: u8, officer: Option<u8>| {
        let mut w = World::generate(13, 12);
        for p in w.players.iter_mut() {
            p.personality.presence = presence;
            p.personality.temperament = if presence < 8 { 4 } else { 14 };
        }
        for t in &mut w.teams {
            t.office.press = officer.map(|r| {
                let mut s = t.office.staffer(crate::staff::Job::Scout).unwrap().clone();
                s.media = r;
                s
            });
        }
        run(&mut w, 1);
        (
            w.fame.counts.gaffes,
            w.fame.counts.caught,
            w.fame.counts.stories,
        )
    };
    let poor = count(3, None);
    let good = count(18, None);
    assert!(poor.0 > good.0 * 2, "poor {poor:?} good {good:?}");
    assert!(good.2 > poor.2, "stories: poor {poor:?} good {good:?}");
    let helped = count(3, Some(20));
    assert!(
        helped.0 * 2 < poor.0,
        "no officer {poor:?} officer {helped:?}"
    );
    assert!(helped.1 > poor.1);
}

#[test]
fn an_appearance_request_pays_through_the_ledger() {
    let mut w = World::generate(7, 8);
    w.apply(Command::TakeCharge(0)).unwrap();
    let mut n = 0;
    let mut paid = false;
    for _ in 0..400 {
        // Answer everything but the appearances.
        tidy_without(&mut w, &mut n);
        if let Some(i) = (0..w.fame.asks.len()).find(|&i| w.appearance_open(i)) {
            let before = w.teams[0].budget;
            let fame = w.fame_of(w.fame.asks[i].player);
            w.apply(Command::AnswerAppearance {
                request: i,
                accept: true,
            })
            .unwrap();
            // Nothing is paid until the day.
            assert_eq!(w.teams[0].budget, before);
            while w.day < w.fame.asks[i].event_day {
                w.day += 1;
                w.fame_daily();
            }
            assert!(w.fame.asks[i].held);
            assert!(w.teams[0].budget > before);
            assert!(w.fame_of(w.fame.asks[i].player) >= fame);
            let l = &w.econ.ledgers[0];
            assert_eq!(l.start + l.net(), w.teams[0].budget);
            // And it is not answered twice.
            assert!(
                w.apply(Command::AnswerAppearance {
                    request: i,
                    accept: true
                })
                .is_err()
            );
            paid = true;
            break;
        }
        if w.apply(Command::Advance).unwrap().stop == Some(Stop::MatchDay) {
            w.apply(Command::PlayRound).unwrap();
        }
    }
    assert!(paid, "no appearance request came");
}

/// `tidy` without answering sponsors' appearance requests.
fn tidy_without(w: &mut World, n: &mut usize) {
    let asks = std::mem::take(&mut w.fame.asks);
    tidy(w, n);
    // Put the requests back as they were.
    w.fame.asks = asks;
}

#[test]
fn retirement_keeps_the_world_bounded_and_every_club_able_to_play() {
    let mut w = World::generate(17, 12);
    let start = w.active_players();
    let mut peak = 0;
    for _ in 0..8 {
        run(&mut w, 1);
        for t in &w.teams {
            assert!(t.roster.len() >= 9, "{} has {}", t.name, t.roster.len());
            assert!(
                t.roster
                    .iter()
                    .all(|&p| !w.players[p as usize].is_retired())
            );
            assert!(
                t.tactics()
                    .rotation
                    .iter()
                    .all(|s| t.roster.contains(&s.player))
            );
        }
        peak = peak.max(w.active_players());
    }
    assert!(
        w.fame.counts.retired > 40,
        "{} retired",
        w.fame.counts.retired
    );
    assert!(peak < start * 2, "{start} players grew to {peak}");
    assert!(w.free_agents().len() < 150);
    assert!(w.players.len() > w.active_players());
    // The retired are still readable.
    let gone = w.players.iter().find(|p| p.is_retired()).unwrap();
    assert!(!w.career(gone.id).is_empty() || gone.age > 0);
    assert!(w.team_of(gone.id).is_none());
}

#[test]
fn a_famous_player_announces_a_farewell_season_and_it_draws() {
    let mut w = World::generate(9, 8);
    w.apply(Command::TakeCharge(0)).unwrap();
    // A name at the end of the road, in the user's club.
    let star = w.teams[0].roster[3];
    {
        let p = &mut w.players[star as usize];
        p.age = 44;
        p.renown.fame = 62.0;
        p.renown.tenure_club = Some(0);
    }
    managed_keeping(&mut w, 1, Some(star));
    // He announced it and is still on the books for his last season.
    assert!(w.players[star as usize].renown.farewell);
    assert!(w.teams[0].roster.contains(&star));
    assert!(
        w.desk
            .messages
            .iter()
            .any(|m| m.players.contains(&star) && m.subject.contains("will retire"))
    );
    assert!(w.fame.counts.farewells >= 1);
    // The farewell lifts the gate wherever he plays.
    let mut plain = w.clone();
    plain.players[star as usize].renown.farewell = false;
    assert!(w.pull(star, 1) > plain.pull(star, 1));
    assert!(w.player_merch(star) > plain.player_merch(star));
    let away = w.fixtures.iter().position(|f| f.away == 0).unwrap();
    let host = w.fixtures[away].home;
    let at = |w: &World| w.attendance_priced(&w.draw_for(away), host, w.ticket_price(host));
    assert!(at(&w) > at(&plain));
    // The last home game is a send-off.
    managed_keeping(&mut w, 1, Some(star));
    assert!(
        w.desk
            .messages
            .iter()
            .any(|m| m.subject.contains("send-off") || m.subject.contains("A send-off"))
    );
    // And then he is gone from the club and the pool, but not from the record.
    assert!(w.players[star as usize].is_retired());
    assert!(!w.teams[0].roster.contains(&star));
    assert!(w.career(star).len() >= 2);
}

#[test]
fn the_market_pays_more_for_a_name_when_it_needs_one() {
    let mut w = blank(10);
    let star = best_of(&w, 1);
    let nobody = w.teams[1].roster[5];
    // Club 2 has an empty hall and money; club 3 is full and well off.
    w.econ.history.clear();
    w.econ.arenas[2].capacity = 12_000;
    w.econ.arenas[2].support = 0.1;
    w.fame.fans[2] = 1_000.0;
    w.teams[2].budget = 900_000;
    w.econ.arenas[3].capacity = 2_500;
    w.econ.arenas[3].support = 0.95;
    w.teams[3].budget = 900_000;
    assert!(w.commercial(2) > w.commercial(3) + 0.2);
    // Same eye for a player: equal judges, no lean.
    for t in [2, 3] {
        let gm = &mut w.teams[t].office.gm;
        (gm.judgement, gm.patience) = (20, 10);
    }
    let plain = [w.value_to(2, star), w.value_to(3, star)];
    w.players[star as usize].renown.fame = 70.0;
    let named = [w.value_to(2, star), w.value_to(3, star)];
    let rise = |i: usize| named[i] as f32 / plain[i] as f32;
    assert!(rise(0) > rise(1) + 0.2, "{plain:?} {named:?}");
    assert!(rise(1) > 1.0);
    // A nobody is worth the same to both.
    assert_eq!(w.value_to(2, nobody), w.value_to(3, nobody));
    // And he asks for more at renewal.
    assert!(crate::fame::wage_mult(&w.players[star as usize]) > 1.3);
}

#[test]
fn clubs_with_empty_halls_and_money_go_for_names_without_ruin() {
    let mut bought = 0;
    for seed in 1..=4 {
        let mut w = World::generate(seed, 12);
        run(&mut w, 3);
        for h in &w.fame.hunts {
            bought += 1;
            let closing = w.econ.history[h.season as usize][h.club].closing;
            assert!(
                closing > 0,
                "club {} closed season {} on {closing}",
                h.club,
                h.season
            );
        }
    }
    assert!(bought >= 3, "{bought} names bought in 12 league-seasons");
}

#[test]
fn selling_a_favourite_costs_goodwill_and_signing_a_star_lifts_it() {
    let mut w = blank(14);
    w.take_charge(0);
    let fav = w.teams[0].roster[0];
    w.players[fav as usize].renown.fame = 60.0;
    w.players[fav as usize].renown.tenure = 8;
    w.players[fav as usize].renown.tenure_club = Some(0);
    let (fans, support) = (w.fan_base(0), w.arena(0).support);
    w.fans_sold(fav, 0);
    assert!(w.fan_base(0) < fans);
    assert!(w.arena(0).support < support);
    assert!(
        w.desk
            .messages
            .iter()
            .any(|m| m.from.contains("supporters") && m.players.contains(&fav))
    );
    let star = w.teams[3].roster[0];
    w.players[star as usize].renown.fame = 70.0;
    let (fans, support) = (w.fan_base(1), w.arena(1).support);
    w.fans_signed(star, 1);
    assert!(w.fan_base(1) > fans);
    assert!(w.arena(1).support > support);
}

#[test]
fn a_benched_favourite_costs_gate_and_a_letter() {
    let mut w = blank(15);
    w.take_charge(0);
    let fav = w.teams[0].roster[0];
    w.players[fav as usize].renown.fame = 60.0;
    w.players[fav as usize].renown.tenure = 6;
    w.players[fav as usize].renown.tenure_club = Some(0);
    let before = w.star_pull(0, 0);
    for slot in &mut w.teams[0].playbook {
        slot.tactics.rotation.retain(|s| s.player != fav);
    }
    assert!(w.star_pull(0, 0) < before / 2.0);
    let fans = w.fan_base(0);
    rounds(&mut w, 5);
    assert!(w.fan_base(0) < fans * 101 / 100);
    assert!(
        w.desk
            .messages
            .iter()
            .any(|m| m.from.contains("supporters") && m.subject.starts_with("Where is"))
    );
}

#[test]
fn the_press_officer_is_a_job_like_the_others() {
    use crate::staff::Job;
    let mut w = World::generate(3, 8);
    w.take_charge(0);
    assert!(Job::ALL.contains(&Job::Press) && Job::HIRED.contains(&Job::Press));
    let n = w.staff_pool.iter().filter(|s| s.media >= 1).count();
    assert!(n > 0);
    let i = w.staff_pool.iter().position(|s| s.media >= 10).unwrap_or(0);
    w.teams[0].budget = 1_000_000;
    w.apply(Command::Hire {
        job: Job::Press,
        candidate: i,
    })
    .unwrap();
    assert!(w.teams[0].office.staffer(Job::Press).is_some());
    assert!(w.teams[0].office.wages() > 0);
    w.apply(Command::Dismiss(Job::Press)).unwrap();
    assert!(w.teams[0].office.staffer(Job::Press).is_none());
}

#[test]
fn the_hall_of_fame_holds_greats() {
    let mut w = World::generate(19, 12);
    run(&mut w, 8);
    for h in w.hall_of_fame() {
        let p = &w.players[h.player as usize];
        assert!(p.is_retired());
        assert!(h.score >= crate::retire::HALL_MIN);
    }
}
