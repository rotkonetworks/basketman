//! Tests of the club finance system: ticket prices, budgets, debt, the
//! luxury tax, facilities and old saves.

use crate::arena::Draw;
use crate::books::FacilityKind;
use crate::command::tests::tidy;
use crate::command::{Command, CommandError};
use crate::finance::Cat;
use crate::inbox::Stop;
use crate::league::World;
use crate::pricing::{attendance_at_price, best_price, gate_at};
use crate::season::SALARY_CAP;
use crate::tax::annual_tax;

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
    let target = w.round + rounds;
    while w.round < target {
        if w.match_today() {
            w.play_round();
        } else {
            w.advance();
        }
    }
}

/// Play as the manager of club 0, answering mail, through `seasons`.
fn managed(w: &mut World, seasons: u16) {
    let target = w.season + seasons;
    let mut n = 0;
    for _ in 0..200_000 {
        if w.season >= target {
            return;
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

fn contender() -> Draw {
    Draw {
        capacity: 6000,
        support: 0.8,
        form: 0.9,
        position: 0.9,
        belief: 0.8,
        opponent: 0.6,
        condition: 0.8,
        playoff: false,
        luck: 0.0,
        fans: 0.5,
        star: 0.0,
        visitor: 0.0,
        night: 0.0,
    }
}

fn strugglers() -> Draw {
    Draw {
        capacity: 6000,
        support: 0.3,
        form: 0.1,
        position: 0.1,
        belief: 0.4,
        opponent: 0.4,
        condition: 0.5,
        playoff: false,
        luck: 0.0,
        fans: 0.5,
        star: 0.0,
        visitor: 0.0,
        night: 0.0,
    }
}

#[test]
fn the_best_price_is_inside_the_range_and_follows_fortunes() {
    let fair = 9.0;
    let (low, high) = (5, 17);
    let (win, lose) = (contender(), strugglers());
    let best_win = best_price(&win, fair, low, high);
    let best_lose = best_price(&lose, fair, low, high);
    assert!(best_lose > low && best_lose < high, "{best_lose}");
    assert!(best_win > low && best_win < high, "{best_win}");
    assert!(
        best_win > best_lose,
        "a contender can charge more: {best_win} v {best_lose}"
    );
    for (d, best) in [(win, best_win), (lose, best_lose)] {
        let gate = |p| gate_at(&d, fair, p).1;
        // Money rises to the best price and falls after it.
        for p in low..best {
            assert!(gate(p) <= gate(p + 1) + 1, "rises at {p}");
        }
        for p in best..high {
            assert!(gate(p) + 1 >= gate(p + 1), "falls at {p}");
        }
        assert!(gate(best) > gate(low) && gate(best) > gate(high));
    }
    // Overpricing a losing team empties the hall; underpricing a
    // contender leaves money on the table.
    let at = |d: &Draw, p| attendance_at_price(d, fair, p as f32);
    assert!(at(&lose, high) * 3 < at(&lose, best_lose) * 2);
    assert!(gate_at(&win, fair, low).1 * 10 < gate_at(&win, fair, best_win).1 * 9);
    // Dearer always means fewer people.
    for d in [win, lose] {
        for p in low..high {
            assert!(at(&d, p + 1) <= at(&d, p));
        }
    }
}

#[test]
fn the_preview_matches_the_game_and_the_price_moves_the_crowd() {
    let mut w = World::generate(5, 8);
    w.apply(Command::TakeCharge(0)).unwrap();
    // Play to a round where club 0 is at home.
    while w
        .next_home_game(0)
        .is_none_or(|i| w.fixtures[i].round != w.round)
    {
        managed_round(&mut w);
    }
    let pv = w.price_preview(0).unwrap();
    assert!(pv.points[0].price < pv.points[1].price && pv.points[1].price < pv.points[2].price);
    assert!(pv.points[0].attendance >= pv.points[1].attendance);
    assert!(pv.points[1].attendance >= pv.points[2].attendance);
    assert_eq!(pv.points[1].price, w.ticket_price(0));
    let i = pv.fixture;
    managed_round(&mut w);
    let (att, gate) = w.econ.crowds[i];
    assert_eq!(att, pv.points[1].attendance);
    assert_eq!(gate, pv.points[1].gate);
}

fn managed_round(w: &mut World) {
    let mut n = 0;
    loop {
        tidy(w, &mut n);
        if let Stop::MatchDay = w.apply(Command::Advance).unwrap().stop.unwrap() {
            w.apply(Command::PlayRound).unwrap();
            return;
        }
    }
}

#[test]
fn a_low_price_builds_support_and_a_high_one_spends_it() {
    let mut cheap = World::generate(8, 8);
    cheap.apply(Command::TakeCharge(0)).unwrap();
    let mut dear = cheap.clone();
    let (low, high) = cheap.price_range(0);
    cheap.apply(Command::SetTicketPrice(low)).unwrap();
    dear.apply(Command::SetTicketPrice(high)).unwrap();
    managed(&mut cheap, 1);
    managed(&mut dear, 1);
    // The games themselves are the same, so only the price differs.
    assert!(cheap.arena(0).support > dear.arena(0).support + 0.05);
    let (c, d) = (&cheap.econ.history[0][0], &dear.econ.history[0][0]);
    assert!(c.average_attendance() > d.average_attendance());
}

#[test]
fn ai_clubs_price_sensibly() {
    let mut w = World::generate(3, 12);
    rounds(&mut w, 10);
    for t in 0..12 {
        let (low, high) = w.price_range(t);
        let p = w.ticket_price(t);
        assert!(p > low && p < high, "club {t}: {p} in {low}..{high}");
        // No other price in range takes much more at the gate.
        let d = w.typical_draw(t);
        let fair = w.fair_price(t) as f32;
        let at = gate_at(&d, fair, p).1;
        let best = (low..=high).map(|q| gate_at(&d, fair, q).1).max().unwrap();
        assert!(at * 100 >= best * 90, "club {t}: {at} v {best}");
    }
}

#[test]
fn bids_over_the_transfer_budget_are_refused() {
    let mut w = World::generate(4, 8);
    w.apply(Command::TakeCharge(0)).unwrap();
    w.teams[0].budget = 10_000_000;
    w.econ.books[0].transfer_budget = 50_000;
    w.econ.books[0].wage_budget = 10_000_000;
    let target = w.teams[1].roster[0];
    let err = w
        .apply(Command::Bid {
            player: target,
            fee: 100_000,
        })
        .unwrap_err();
    assert!(
        matches!(&err, CommandError::Refused(m) if m.contains("transfer budget")),
        "{err:?}"
    );
    assert!(w.desk.bids.is_empty());
    w.apply(Command::Bid {
        player: target,
        fee: 40_000,
    })
    .unwrap();
    // The bid waiting in the post counts against what is left.
    assert_eq!(w.transfer_left(0), 10_000);
    let other = w.teams[2].roster[0];
    assert!(
        w.apply(Command::Bid {
            player: other,
            fee: 20_000
        })
        .is_err()
    );
}

#[test]
fn the_wage_budget_blocks_signings_but_never_forces_a_sale() {
    let mut w = World::generate(4, 8);
    w.apply(Command::TakeCharge(0)).unwrap();
    w.teams[0].budget = 10_000_000;
    w.econ.books[0].transfer_budget = 5_000_000;
    let roster = w.teams[0].roster.clone();
    // Already over the wage budget.
    w.econ.books[0].wage_budget = w.wage_committed(0) - 1;
    assert!(w.wage_left(0) < 0);
    let target = w.teams[1].roster[0];
    let err = w
        .apply(Command::Bid {
            player: target,
            fee: 30_000,
        })
        .unwrap_err();
    assert!(
        matches!(&err, CommandError::Refused(m) if m.contains("wage bill")),
        "{err:?}"
    );
    // A free agent is no different.
    if let Some(free) = w.free_agents().first().map(|p| p.id) {
        assert!(
            w.apply(Command::Bid {
                player: free,
                fee: 0
            })
            .is_err()
        );
    }
    // A trade that adds to the wage bill is refused; the same man for a
    // dearer one the other way is not a problem for the budget.
    let theirs = w.teams[1].roster[0];
    let mine = *w.teams[0]
        .roster
        .iter()
        .min_by_key(|&&p| w.players[p as usize].contract.salary)
        .unwrap();
    w.players[theirs as usize].contract.salary = w.players[mine as usize].contract.salary + 30_000;
    let deal = crate::inbox::Deal {
        club: 1,
        give: vec![mine],
        get: vec![theirs],
        ..Default::default()
    };
    let verdict = w.trade_verdict(&deal).unwrap_err();
    assert!(verdict.contains("board"), "{verdict}");
    assert!(w.wage_check(0, -5_000).is_ok());
    // Nothing is sold: playing on leaves the squad as it was.
    rounds(&mut w, 3);
    assert_eq!(w.teams[0].roster, roster);
}

#[test]
fn money_moves_between_the_budgets_within_limits() {
    let mut w = World::generate(4, 8);
    w.apply(Command::TakeCharge(0)).unwrap();
    let (wage, transfer) = (w.econ.books[0].wage_budget, w.econ.books[0].transfer_budget);
    w.apply(Command::MoveBudget {
        to_wages: true,
        amount: 10_000,
    })
    .unwrap();
    assert_eq!(w.econ.books[0].wage_budget, wage + 10_000);
    assert_eq!(w.econ.books[0].transfer_budget, transfer - 10_000);
    w.apply(Command::MoveBudget {
        to_wages: false,
        amount: 10_000,
    })
    .unwrap();
    assert_eq!(w.econ.books[0].wage_budget, wage);
    // Past half of what the board gave is refused, and so is more than there is.
    let most = w.budget_move_limit(0, true);
    assert!(most > 0 && most <= transfer / 2 + 1_000);
    assert!(
        w.apply(Command::MoveBudget {
            to_wages: true,
            amount: most + 1_000
        })
        .is_err()
    );
    // The wage budget cannot be cut below what is committed.
    let below = w.wage_committed(0);
    w.econ.books[0].wage_budget = below + 3_000;
    assert!(w.budget_move_limit(0, false) <= 3_000);
    assert!(
        w.apply(Command::MoveBudget {
            to_wages: false,
            amount: 4_000
        })
        .is_err()
    );
}

#[test]
fn a_club_over_its_wage_budget_keeps_renewals_at_current_pay() {
    let rise = |capped: bool| {
        let mut w = World::generate(6, 8);
        w.apply(Command::TakeCharge(0)).unwrap();
        // Everybody's deal runs out and the market says they are worth more.
        for &p in &w.teams[0].roster.clone() {
            let pl = &mut w.players[p as usize];
            pl.contract.years_left = 1;
            pl.contract.salary = 10_000;
        }
        w.econ.books[0].wage_budget = if capped { 1 } else { 10_000_000 };
        let original = w.teams[0].roster.clone();
        managed(&mut w, 1);
        original
            .iter()
            .filter(|&&p| w.teams[0].roster.contains(&p))
            .filter(|&&p| w.players[p as usize].contract.salary > 10_000)
            .count()
    };
    assert_eq!(rise(true), 0);
    assert!(rise(false) > 3);
}

#[test]
fn overdraft_interest_accrues_and_the_freeze_comes_beyond_the_limit() {
    let mut w = World::generate(7, 8);
    w.apply(Command::TakeCharge(0)).unwrap();
    let limit = w.overdraft_limit(0);
    assert!(limit > 100_000);
    w.teams[0].budget = -limit / 2;
    w.econ.ledgers[0].start -= limit / 2 + 1_000_000;
    assert!(!w.spending_frozen(0));
    assert_eq!(w.interest_due(0), (limit / 2 * 4 + 999) / 1000);
    // Inside the limit the club can still run: Renovate fails for lack of
    // money, not because of a freeze.
    let before = w.teams[0].budget;
    let pre_start = w.econ.ledgers[0].start;
    managed_round(&mut w);
    let interest = -w.econ.ledgers[0].total(Cat::Interest);
    assert!(interest > 0, "interest was charged");
    // The interest is a fraction of the debt after the round's money.
    let debt = -(w.teams[0].budget + interest);
    assert!(debt > 0 || before + 1 > 0);
    assert_eq!(interest, (debt * 4 + 999) / 1000);
    assert_eq!(pre_start + w.econ.ledgers[0].net(), w.teams[0].budget);
    assert!(
        w.inbox().iter().any(|m| m.subject == "We are in overdraft"),
        "told about the overdraft"
    );
    // Beyond the limit: frozen, and the board asks for a sale.
    // Far enough past it that one round's takings do not bring it back.
    w.teams[0].budget = -limit - 60_000;
    w.econ.ledgers[0].start -= limit + 60_000 - 1;
    assert!(w.spending_frozen(0));
    let err = w
        .apply(Command::Bid {
            player: w.teams[1].roster[0],
            fee: 1,
        })
        .unwrap_err();
    assert!(matches!(err, CommandError::Refused(_)));
    assert!(
        w.apply(Command::UpgradeFacility(FacilityKind::Youth))
            .is_err()
    );
    managed_round(&mut w);
    assert!(
        w.inbox()
            .iter()
            .any(|m| m.subject == "The board demands a sale"),
        "the board demands a sale"
    );
}

#[test]
fn an_ai_club_in_too_deep_sells_its_best_player_at_a_discount() {
    let mut w = World::generate(10, 8);
    let club = 3;
    let limit = w.overdraft_limit(club);
    let star = *w.teams[club]
        .roster
        .iter()
        .max_by_key(|&&p| (w.value(p), p))
        .unwrap();
    w.teams[club].budget = -limit - 500_000;
    w.econ.ledgers[club].start = w.teams[club].budget;
    w.play_round();
    assert_eq!(w.econ.books[club].listed.map(|l| l.0), Some(star));
    assert!(w.is_listed(star));
    assert_eq!(w.listed_price(star), w.value(star) * 75 / 100 / 1000 * 1000);
    let was = w.teams[club].roster.len();
    for _ in 0..5 {
        w.play_round();
    }
    assert!(!w.teams[club].roster.contains(&star), "sold abroad");
    assert_eq!(w.teams[club].roster.len(), was - 1);
    assert!(w.econ.ledgers[club].total(Cat::TransferIn) >= w.listed_price(star));
    assert!(w.teams[club].roster.len() >= crate::inbox::MIN_ROSTER);
    for (t, l) in w.econ.ledgers.iter().enumerate() {
        assert_eq!(l.start + l.net(), w.teams[t].budget, "club {t}");
    }
}

#[test]
fn the_tax_bands_climb() {
    assert_eq!(annual_tax(0), 0);
    assert_eq!(annual_tax(-5), 0);
    assert_eq!(annual_tax(40_000), 20_000);
    assert_eq!(annual_tax(100_000), 20_000 + 60_000);
    assert_eq!(annual_tax(150_000), 80_000 + 75_000);
}

fn push_payroll(w: &mut World, club: usize, to: i64) {
    let roster = w.teams[club].roster.clone();
    let now = w.payroll(club);
    let extra = (to - now) / roster.len() as i64;
    for p in roster {
        let s = &mut w.players[p as usize].contract.salary;
        *s = (*s as i64 + extra).max(1_000) as u32;
    }
}

#[test]
fn the_tax_collected_is_the_tax_shared() {
    let mut w = World::generate(5, 8);
    // Two clubs over the cap, one by a lot; everyone else under.
    for t in 0..8 {
        push_payroll(&mut w, t, SALARY_CAP - 100_000 - 10_000 * t as i64);
    }
    push_payroll(&mut w, 1, SALARY_CAP + 33_333);
    push_payroll(&mut w, 2, SALARY_CAP + 140_001);
    let (v1, v2) = (w.tax_view(1), w.tax_view(2));
    assert!(v1.excess > 30_000 && v2.annual > v1.annual);
    assert_eq!(v1.checkpoints_left, 2);
    let half = (v1.annual / 2 + v2.annual / 2, v1.annual / 2, v2.annual / 2);
    let half_season = (w.pay_rounds() / 2) as usize;
    rounds(&mut w, half_season);
    let paid: i64 = w
        .econ
        .ledgers
        .iter()
        .map(|l| -l.total(Cat::LuxuryTax))
        .sum();
    let shared: i64 = w.econ.ledgers.iter().map(|l| l.total(Cat::TaxShare)).sum();
    assert!(paid > 0);
    assert_eq!(paid, shared, "collected equals distributed");
    // Payrolls move a little with the week's signings, but the two clubs
    // pay and the others receive; shares differ by at most a unit.
    let (p1, p2) = (
        -w.econ.ledgers[1].total(Cat::LuxuryTax),
        -w.econ.ledgers[2].total(Cat::LuxuryTax),
    );
    assert_eq!((p1, p2), (half.1, half.2));
    let shares: Vec<i64> = (0..8)
        .filter(|&t| t != 1 && t != 2)
        .map(|t| w.econ.ledgers[t].total(Cat::TaxShare))
        .collect();
    assert_eq!(shares.len(), 6);
    assert!(shares.iter().max().unwrap() - shares.iter().min().unwrap() <= 1);
    assert_eq!(w.econ.ledgers[1].total(Cat::TaxShare), 0);
    for (t, l) in w.econ.ledgers.iter().enumerate() {
        assert_eq!(l.start + l.net(), w.teams[t].budget, "club {t}");
    }
    // And again at the end of the regular season.
    rounds(&mut w, half_season);
    let paid: i64 = w
        .econ
        .ledgers
        .iter()
        .map(|l| -l.total(Cat::LuxuryTax))
        .sum();
    let shared: i64 = w.econ.ledgers.iter().map(|l| l.total(Cat::TaxShare)).sum();
    assert_eq!(paid, shared);
    assert!(paid >= half.0 * 2 - 2);
}

#[test]
fn nobody_pays_when_nobody_is_under_the_cap() {
    let mut w = World::generate(5, 8);
    for t in 0..8 {
        push_payroll(&mut w, t, SALARY_CAP + 20_000);
    }
    let half_season = (w.pay_rounds() / 2) as usize;
    rounds(&mut w, half_season);
    for l in &w.econ.ledgers {
        assert_eq!(l.total(Cat::LuxuryTax), 0);
        assert_eq!(l.total(Cat::TaxShare), 0);
    }
}

#[test]
fn general_managers_weigh_the_tax_by_their_patience() {
    let mut w = World::generate(5, 8);
    push_payroll(&mut w, 2, SALARY_CAP - 10_000);
    w.teams[2].office.gm.patience = 1;
    let win_now = w.tax_felt(2, 60_000);
    w.teams[2].office.gm.patience = 20;
    let builder = w.tax_felt(2, 60_000);
    assert!(win_now > 0 && builder > win_now * 2, "{win_now} {builder}");
    assert_eq!(w.tax_felt(2, -20_000), 0);
}

#[test]
fn every_new_category_keeps_the_ledger_whole_over_three_seasons() {
    let mut w = World::generate(9, 8);
    w.apply(Command::TakeCharge(0)).unwrap();
    push_payroll(&mut w, 0, SALARY_CAP + 80_000);
    w.apply(Command::UpgradeFacility(FacilityKind::Training))
        .unwrap();
    w.teams[0].budget -= 1_100_000;
    w.econ.ledgers[0].start -= 1_100_000;
    let mut seen = [false; 20];
    for _ in 0..3 {
        managed(&mut w, 1);
        for (t, l) in w.econ.ledgers.iter().enumerate() {
            assert_eq!(l.start + l.net(), w.teams[t].budget, "club {t}");
            let by_round: i64 = l.rounds.iter().map(|r| r.2).sum();
            assert_eq!(by_round, l.net(), "club {t} per-round record");
        }
        let year = w.econ.history.last().unwrap();
        for (t, l) in year.iter().enumerate() {
            assert_eq!(l.start + l.net(), l.closing, "club {t}");
            for c in Cat::ALL {
                seen[Cat::ALL.iter().position(|&x| x == c).unwrap()] |= l.total(c) != 0;
            }
            // The weekly chart ends where the books end.
            assert!(!l.weekly.is_empty());
            assert_eq!(l.weekly.last().map(|w| w[2]), Some(l.closing));
        }
        let tax: i64 = year
            .iter()
            .map(|l| l.total(Cat::LuxuryTax) + l.total(Cat::TaxShare))
            .sum();
        assert_eq!(tax, 0, "tax is only moved around");
    }
    for c in [
        Cat::Broadcast,
        Cat::Merchandise,
        Cat::LuxuryTax,
        Cat::TaxShare,
        Cat::Interest,
        Cat::FacilityRunning,
        Cat::FacilityWorks,
    ] {
        assert!(
            seen[Cat::ALL.iter().position(|&x| x == c).unwrap()],
            "{c:?} never moved"
        );
    }
}

#[test]
fn finances_are_deterministic_with_a_manager_too() {
    let play = || {
        let mut w = World::generate(13, 8);
        w.apply(Command::TakeCharge(2)).unwrap();
        let now = w.ticket_price(2);
        w.apply(Command::SetTicketPrice(now + 2)).unwrap();
        w.apply(Command::UpgradeFacility(FacilityKind::Youth))
            .unwrap();
        managed_as(&mut w, 2);
        w
    };
    let (a, b) = (play(), play());
    assert!(format!("{:?}", a.econ) == format!("{:?}", b.econ));
    assert_eq!(
        a.teams.iter().map(|t| t.budget).collect::<Vec<_>>(),
        b.teams.iter().map(|t| t.budget).collect::<Vec<_>>()
    );
}

fn managed_as(w: &mut World, seasons: u16) {
    let target = w.season + seasons;
    let mut n = 0;
    while w.season < target {
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
}

fn set_all(w: &mut World, kind: FacilityKind, level: u8) {
    for b in &mut w.econ.books {
        b.facilities.set(kind, level);
    }
}

fn squad_overall(w: &World, club: usize) -> f32 {
    w.teams[club]
        .roster
        .iter()
        .chain(&w.teams[club].reserves)
        .map(|&p| w.players[p as usize].overall())
        .sum()
}

#[test]
fn a_better_training_ground_means_more_progress() {
    let go = |level| {
        let mut w = World::generate(21, 8);
        set_all(&mut w, FacilityKind::Training, level);
        let before: f32 = (0..8).map(|t| squad_overall(&w, t)).sum();
        rounds(&mut w, 14);
        let after: f32 = (0..8).map(|t| squad_overall(&w, t)).sum();
        after - before
    };
    let (poor, good) = (go(1), go(5));
    assert!(good > poor + 0.5, "{poor} v {good}");
}

#[test]
fn a_better_medical_centre_means_fewer_and_shorter_injuries() {
    let go = |level| {
        let mut lost = 0u32;
        let mut count = 0u32;
        // Injuries are rare and noisy: twelve worlds, not four.
        for seed in 31..43 {
            let mut w = World::generate(seed, 8);
            set_all(&mut w, FacilityKind::Medical, level);
            rounds(&mut w, 14);
            for p in &w.players {
                for r in &p.injuries {
                    lost += r.weeks as u32;
                    count += 1;
                }
            }
        }
        (count, lost)
    };
    let (poor, good) = (go(1), go(5));
    assert!(good.0 * 10 < poor.0 * 9, "injuries {poor:?} v {good:?}");
    assert!(good.1 * 10 < poor.1 * 8, "weeks out {poor:?} v {good:?}");
}

#[test]
fn a_better_youth_setup_means_a_better_intake() {
    let go = |level| {
        let mut w = World::generate(41, 8);
        set_all(&mut w, FacilityKind::Youth, level);
        let first = w.players.len();
        w.reserve_summer();
        let new: Vec<_> = w.players[first..].iter().collect();
        assert!(new.len() >= 8);
        let n = new.len() as f32;
        (
            new.iter().map(|p| p.overall()).sum::<f32>() / n,
            new.iter().map(|p| p.potential).sum::<f32>() / n,
        )
    };
    let (poor, good) = (go(1), go(5));
    assert!(good.0 > poor.0 + 0.8, "quality {poor:?} v {good:?}");
    assert!(good.1 > poor.1 + 1.0, "potential {poor:?} v {good:?}");
}

#[test]
fn building_a_facility_costs_money_and_takes_rounds() {
    let mut w = World::generate(4, 8);
    w.apply(Command::TakeCharge(0)).unwrap();
    w.econ.books[0].facilities.medical = 3;
    let before = w.teams[0].budget;
    let cost = crate::facility::build_cost(4);
    w.apply(Command::UpgradeFacility(FacilityKind::Medical))
        .unwrap();
    assert_eq!(w.teams[0].budget, before - cost);
    assert_eq!(w.facilities(0).medical, 3, "not yet");
    // One at a time.
    assert!(
        w.apply(Command::UpgradeFacility(FacilityKind::Youth))
            .is_err()
    );
    for _ in 0..crate::facility::BUILD_ROUNDS {
        managed_round(&mut w);
    }
    assert_eq!(w.facilities(0).medical, 4);
    assert!(
        w.inbox()
            .iter()
            .any(|m| m.subject == "Medical centre is ready")
    );
    // Running costs follow the levels.
    assert!(-w.econ.ledgers[0].total(Cat::FacilityRunning) > 0);
    // At the top it stops; with no money it refuses.
    w.econ.books[0].facilities.training = 5;
    assert!(
        w.apply(Command::UpgradeFacility(FacilityKind::Training))
            .is_err()
    );
    w.teams[0].budget = 1_000;
    assert!(
        w.apply(Command::UpgradeFacility(FacilityKind::Youth))
            .is_err()
    );
}

#[test]
fn clubs_start_at_varied_levels() {
    let w = World::generate(3, 12);
    let mut seen = std::collections::HashSet::new();
    for t in 0..12 {
        let f = w.facilities(t);
        for k in FacilityKind::ALL {
            assert!((1..=5).contains(&f.level(k)));
            seen.insert(f.level(k));
        }
    }
    assert!(seen.len() >= 3, "{seen:?}");
}

#[test]
fn broadcast_money_is_shared_by_last_years_finish() {
    let mut w = World::generate(3, 12);
    run(&mut w, 1);
    let table: Vec<usize> = w.history[0].table.iter().map(|t| t.0).collect();
    let (first, last) = (w.broadcast_season(table[0]), w.broadcast_season(table[11]));
    assert!(first > last);
    let total: i64 = (0..12).map(|t| w.broadcast_season(t)).sum();
    let pool = 12 * crate::income::BROADCAST_PER_CLUB;
    assert!((total - pool).abs() < 20, "{total} v {pool}");
    // Half of it is the same for everyone.
    assert!(last > pool / 24 - 20);
}

#[test]
fn merchandise_follows_support_and_has_a_place_for_players() {
    let mut w = World::generate(3, 8);
    w.econ.arenas[0].support = 0.9;
    w.econ.arenas[1].support = 0.2;
    w.econ.arenas[1].capacity = w.econ.arenas[0].capacity;
    for p in w.players.iter_mut() {
        p.renown.fame = 0.0;
    }
    assert!(w.merch_week(0) > w.merch_week(1));
    // An unknown sells no shirts, a name sells some.
    let who = w.teams[0].roster[0];
    w.players[who as usize].renown.fame = 0.0;
    assert_eq!(w.player_merch(who), 0);
    w.players[who as usize].renown.fame = 70.0;
    assert!(w.player_merch(who) > 500);
}

#[test]
fn saves_from_before_the_finance_system_still_open() {
    use miniz_oxide::deflate::compress_to_vec;
    use miniz_oxide::inflate::decompress_to_vec_with_limit;
    let mut w = World::generate(2, 8);
    w.apply(Command::TakeCharge(0)).unwrap();
    managed(&mut w, 1);
    let bytes = crate::save::encode(&w);
    let json = decompress_to_vec_with_limit(&bytes[5..], 512 * 1024 * 1024).unwrap();
    let mut v: serde_json::Value = serde_json::from_slice(&json).unwrap();
    let econ = v["econ"].as_object_mut().unwrap();
    assert!(econ.remove("books").is_some());
    let cut = |l: &mut serde_json::Value| {
        let l = l.as_object_mut().unwrap();
        l.remove("weekly");
        // The thirteen categories there were.
        l["totals"].as_array_mut().unwrap().truncate(13);
    };
    for l in econ["ledgers"].as_array_mut().unwrap() {
        cut(l);
    }
    for year in econ["history"].as_array_mut().unwrap() {
        for l in year.as_array_mut().unwrap() {
            cut(l);
        }
    }
    let mut old = bytes[..5].to_vec();
    old.extend(compress_to_vec(&serde_json::to_vec(&v).unwrap(), 6));
    let mut loaded = crate::save::decode(&old).expect("an old save opens");
    assert_eq!(loaded.econ.books.len(), 8);
    assert!(loaded.econ.books[0].wage_budget > 0);
    assert!(loaded.ticket_price(0) > 0);
    assert_eq!(loaded.econ.history[0][0].total(Cat::Broadcast), 0);
    managed(&mut loaded, 1);
    for (t, l) in loaded.econ.ledgers.iter().enumerate() {
        assert_eq!(l.start + l.net(), loaded.teams[t].budget, "club {t}");
    }
    assert!(loaded.econ.history[1][0].total(Cat::Broadcast) > 0);
}
