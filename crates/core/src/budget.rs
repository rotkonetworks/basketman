//! The board's budgets. At the start of every season the board gives each
//! club a wage budget and a transfer budget, from what the club has in the
//! bank, how last season's books came out and what the board expects of the
//! team. At the half way mark it looks again. The manager of the user's club
//! can move money between the two within limits (`Command::MoveBudget`).
//! Other clubs live by the same budgets.
//!
//! Both are cash for the season, so money moves between them one for one.
//!
//! - Wage budget: the most the player wage bill (first team and reserves)
//!   may come to over the season: what is paid so far plus what the squad
//!   as it stands will be paid to the end of the season. Staff wages are
//!   not in it. A signing or trade that would take the bill past the budget
//!   is refused, and so is a renewal above current pay. A club already over
//!   its budget is never made to sell anybody.
//! - Transfer budget: fees and cash in trades. A bid over what is left is
//!   refused, bids still waiting for an answer count against it, and half of
//!   any fee the club receives is added back to what it can spend.
//!
//! Nothing here draws from `World::rng`.

use crate::finance::Cat;
use crate::inbox::{Kind, money};
use crate::league::{TeamId, World};
use crate::season::SALARY_CAP;

/// Rounds after the regular season that still bring wages (the playoffs).
const PLAYOFF_ROUNDS: i64 = 5;
/// Share of the unspent transfer budget that carries into next season.
const CARRY: i64 = 4;
/// The most the manager can take from either budget, as a share of what
/// the board set (in percent): the donor keeps at least the rest.
const KEEP: i64 = 50;

impl World {
    /// Every player's yearly pay at a club, reserves included: what the
    /// weekly wage bill is made of.
    pub fn squad_wages(&self, team: TeamId) -> i64 {
        self.teams[team]
            .roster
            .iter()
            .chain(&self.teams[team].reserves)
            .map(|&p| self.players[p as usize].contract.salary as i64)
            .sum()
    }

    /// Rounds from now to the end of the season that wages are still paid.
    pub fn wage_rounds_left(&self) -> i64 {
        (self.pay_rounds() + PLAYOFF_ROUNDS - self.round as i64).max(0)
    }

    /// Player wages paid so far this season.
    pub fn wages_paid(&self, team: TeamId) -> i64 {
        -self.econ.ledgers[team].total(Cat::PlayerWages)
    }

    /// What the player wage bill comes to over the season at current
    /// contracts: paid so far plus what is still to pay.
    pub fn wage_committed(&self, team: TeamId) -> i64 {
        self.wages_paid(team) + self.squad_wages(team) * self.wage_rounds_left() / 52
    }

    /// What the wage budget has left; negative when the club is over it.
    pub fn wage_left(&self, team: TeamId) -> i64 {
        self.econ.books[team].wage_budget - self.wage_committed(team)
    }

    /// Fees the user's bids in the post would cost if they all went through.
    pub fn pending_bids(&self, team: TeamId) -> i64 {
        if self.watch == Some(team) {
            self.desk.bids.iter().map(|b| b.fee).sum()
        } else {
            0
        }
    }

    /// What is left of the transfer budget for new fees.
    pub fn transfer_left(&self, team: TeamId) -> i64 {
        let b = &self.econ.books[team];
        b.transfer_budget - b.transfer_spent - self.pending_bids(team)
    }

    /// Would a change that adds `extra` to the yearly wage bill stay inside
    /// the budget? A change that does not add to it always passes.
    pub fn wage_check(&self, team: TeamId, extra: i64) -> Result<(), String> {
        if extra <= 0 {
            return Ok(());
        }
        let after = self.wage_committed(team) + extra * self.wage_rounds_left() / 52;
        let budget = self.econ.books[team].wage_budget;
        if after > budget {
            return Err(format!(
                "That takes the wage bill to {} for the season, over the board's budget of {}.",
                money(after),
                money(budget)
            ));
        }
        Ok(())
    }

    /// Would a fee fit in the transfer budget?
    pub fn fee_check(&self, team: TeamId, fee: i64) -> Result<(), String> {
        if fee > self.transfer_left(team) {
            return Err(format!(
                "That is over the transfer budget: {} left of {}.",
                money(self.transfer_left(team).max(0)),
                money(self.econ.books[team].transfer_budget)
            ));
        }
        Ok(())
    }

    /// A transfer fee went from `to` to `from`: it counts against the
    /// buyer's budget in full, and for the seller adds back half.
    pub(crate) fn book_fee(&mut self, from: TeamId, to: TeamId, fee: i64) {
        if let Some(b) = self.econ.books.get_mut(to) {
            b.transfer_spent += fee;
        }
        if let Some(b) = self.econ.books.get_mut(from) {
            b.transfer_spent -= fee / 2;
        }
    }

    /// Cash in a trade: `cash` went from `a` to `b` (negative the other way).
    pub(crate) fn book_trade_cash(&mut self, a: TeamId, b: TeamId, cash: i64) {
        if let Some(x) = self.econ.books.get_mut(a) {
            x.transfer_spent += cash;
        }
        if let Some(x) = self.econ.books.get_mut(b) {
            x.transfer_spent -= cash;
        }
    }

    /// What the board expects of the club this season: a final place,
    /// 1 = champions. The user's is on the desk; the others' follow the
    /// strength of their squads the same way.
    pub fn expected_finish(&self, club: TeamId) -> usize {
        if self.watch == Some(club) && self.desk.expectation > 0 {
            return self.desk.expectation;
        }
        let b = self.econ.books[club].expect;
        if b > 0 {
            return b;
        }
        self.strength_target(club)
    }

    pub(crate) fn strength_target(&self, club: TeamId) -> usize {
        let strength = |t: TeamId| {
            let mut ov: Vec<f32> = self.teams[t]
                .roster
                .iter()
                .map(|&p| self.players[p as usize].overall())
                .collect();
            ov.sort_by(|a, b| b.total_cmp(a));
            ov.iter().take(8).sum::<f32>()
        };
        let n = self.teams.len();
        let better = (0..n).filter(|&t| strength(t) > strength(club)).count();
        if better < 3 {
            3
        } else if better < n / 2 {
            n / 2
        } else {
            n.saturating_sub(2)
        }
    }

    /// Money the club makes from running, last season: income less wages,
    /// staff, the hall, facilities, interest and tax. Fees, trades, one-off
    /// building and severance are left out.
    pub(crate) fn operating_result(&self, club: TeamId) -> Option<(i64, i64)> {
        let l = &self.econ.history.last()?[club];
        let capital = [
            Cat::TransferIn,
            Cat::TransferOut,
            Cat::TradeCash,
            Cat::Severance,
            Cat::Renovation,
            Cat::Expansion,
            Cat::FacilityWorks,
        ];
        let op: i64 = Cat::ALL
            .iter()
            .filter(|c| !capital.contains(c))
            .map(|&c| l.total(c))
            .sum();
        Some((l.income(), op))
    }

    /// The year's wage bill at today's contracts, as if no round had been
    /// played.
    pub fn season_wages(&self, club: TeamId) -> i64 {
        self.squad_wages(club) * (self.pay_rounds() + PLAYOFF_ROUNDS) / 52
    }

    /// Give a club its budgets for the season about to start.
    pub(crate) fn set_budgets(&mut self, club: TeamId) -> String {
        let budget = self.teams[club].budget;
        let expect = if self.watch == Some(club) && self.desk.expectation > 0 {
            self.desk.expectation
        } else {
            self.strength_target(club)
        };
        let last_place = self.last_finish(club) + 1;
        let (income, op) = self.operating_result(club).unwrap_or((0, 0));
        let ratio = if income > 0 {
            op as f32 / income as f32
        } else {
            0.0
        };
        let mut room = 0.03 + 0.5 * ratio;
        if budget < 0 {
            room = room.min(0.0);
        }
        if self.history.last().is_some() {
            if last_place <= expect {
                room += 0.02;
            } else if last_place > expect + 2 {
                room -= 0.02;
            }
        }
        let room = room.clamp(-0.02, 0.15);
        let wages = self.season_wages(club);
        let wage_budget = ((wages as f32 * (1.0 + room)) as i64 / 1000 + 1) * 1000;
        let carry = {
            let b = &self.econ.books[club];
            (b.transfer_budget - b.transfer_spent).max(0) / CARRY
        };
        let transfer_budget = if budget <= 0 {
            0
        } else {
            let fresh = budget / 5 + op.max(0) * 9 / 20;
            ((fresh + carry).min(budget) / 1000) * 1000
        };
        let b = &mut self.econ.books[club];
        b.expect = expect;
        b.wage_budget = wage_budget;
        b.wage_board = wage_budget;
        b.transfer_budget = transfer_budget;
        b.transfer_board = transfer_budget;
        b.transfer_spent = 0;
        if self.history.last().is_none() {
            "This is the first set of books under you, so the board has gone by what the club \
             has in the bank."
                .to_string()
        } else if budget < 0 {
            "The club is overdrawn, so there is no room for new fees and no extra for wages."
                .to_string()
        } else if op >= 0 {
            format!(
                "Last season the club made {} from running itself (income {}), which the board \
                 puts back into the budgets.",
                money(op),
                money(income)
            )
        } else {
            format!(
                "Last season the club lost {} from running itself (income {}), so the board is \
                 holding back.",
                money(-op),
                money(income)
            )
        }
    }

    /// How the year went against the budgets, in a sentence or two.
    pub(crate) fn budget_summary(&self, club: TeamId) -> String {
        let b = &self.econ.books[club];
        format!(
            "Wages came to {} against a budget of {}. On transfers {} was used of {}. The board \
             sets next season's budgets when the new season opens.",
            money(self.wage_committed(club)),
            money(b.wage_budget),
            money(b.transfer_spent.max(0)),
            money(b.transfer_budget),
        )
    }

    /// The board's letter about the budgets, for the user's club.
    pub(crate) fn post_budget_letter(&mut self, club: TeamId, why: &str, day: u32) {
        let b = self.econ.books[club].clone();
        let body = format!(
            "Budgets for season {}.\n\nWages: {} for the season. At today's contracts the bill \
             comes to {}.\nTransfers: {}.\n\n{why}\n\nThe board expects a finish of {} or \
             better. You can move money between the two on the Finances page, up to half of \
             either can be taken. Bids over the transfer budget are refused, and while the wage \
             bill is over its budget there are no new signings, trades that add to it or \
             raises at renewal. Nobody is forced to leave.",
            self.season + 1,
            money(b.wage_budget),
            money(self.wage_committed(club)),
            money(b.transfer_budget),
            ordinal(self.expected_finish(club)),
        );
        self.post(
            day,
            Kind::Board,
            "The board",
            format!("Budgets for season {}", self.season + 1),
            body,
            vec![],
        );
    }

    /// Set every club's budgets for the season; the user gets a letter.
    pub(crate) fn open_budgets(&mut self) {
        for club in 0..self.teams.len() {
            let why = self.set_budgets(club);
            if self.watch == Some(club) {
                let day = self.day;
                self.post_budget_letter(club, &why, day);
            }
        }
    }

    /// At the half way mark every board looks again: a club doing better
    /// than asked, with money in the bank, gets a little more; one doing
    /// worse, or short of money, has its transfer budget trimmed. Never
    /// below what is already spent or committed.
    pub(crate) fn review_budgets(&mut self) {
        let table = self.table();
        for club in 0..self.teams.len() {
            let place = table.iter().position(|&t| t == club).map_or(1, |p| p + 1);
            let expect = self.expected_finish(club);
            let budget = self.teams[club].budget;
            let committed = self.wage_committed(club);
            let left = self.transfer_left(club).max(0);
            let b = &mut self.econ.books[club];
            let (w0, t0) = (b.wage_budget, b.transfer_budget);
            if budget < 0 {
                b.transfer_budget = b.transfer_spent.max(0);
            } else if place <= expect && budget > 150_000 {
                b.wage_budget += b.wage_budget / 50;
                b.transfer_budget += (budget / 20).min(60_000) / 1000 * 1000;
            } else if place > expect + 2 {
                let cut = left / 3;
                b.transfer_budget -= cut;
                b.wage_budget = b
                    .wage_budget
                    .min(committed.max(b.wage_budget - b.wage_budget / 50));
            }
            b.wage_board = b.wage_budget;
            b.transfer_board = b.transfer_budget;
            let (w1, t1) = (b.wage_budget, b.transfer_budget);
            if self.watch == Some(club) {
                let verdict = if budget < 0 {
                    "The club is in the red, so the transfer budget is closed.".to_string()
                } else if place <= expect && budget > 150_000 {
                    format!(
                        "We are {} and were asked for {} or better, so the board has added a \
                         little to both.",
                        ordinal(place),
                        ordinal(expect)
                    )
                } else if place > expect + 2 {
                    format!(
                        "We are {} and were asked for {} or better, so the board has trimmed \
                         what is left to spend on transfers.",
                        ordinal(place),
                        ordinal(expect)
                    )
                } else {
                    "Nothing changes.".to_string()
                };
                let body = format!(
                    "The board has reviewed the budgets at the half way mark.\n\nWages: {} \
                     (was {}), the bill stands at {} for the season.\nTransfers: {} (was {}), \
                     {} left to spend.\n\n{verdict}",
                    money(w1),
                    money(w0),
                    money(committed),
                    money(t1),
                    money(t0),
                    money(self.transfer_left(club).max(0)),
                );
                let day = self.day;
                self.post(day, Kind::Board, "The board", "Budget review", body, vec![]);
            }
        }
    }

    /// The most that can be moved to the other budget: the donor keeps what
    /// is committed or spent, and at least `KEEP` percent of what the board
    /// first gave.
    pub fn budget_move_limit(&self, team: TeamId, to_wages: bool) -> i64 {
        let b = &self.econ.books[team];
        let room = if to_wages {
            let floor = (b.transfer_board * KEEP / 100).max(b.transfer_spent);
            b.transfer_budget - floor - self.pending_bids(team)
        } else {
            let floor = (b.wage_board * KEEP / 100).max(self.wage_committed(team));
            b.wage_budget - floor
        };
        room.max(0)
    }

    /// Move `amount` of the budget to wages (or to transfers).
    pub fn move_budget(
        &mut self,
        team: TeamId,
        to_wages: bool,
        amount: i64,
    ) -> Result<String, String> {
        if amount <= 0 {
            return Err("Pick an amount above zero.".into());
        }
        let most = self.budget_move_limit(team, to_wages);
        if amount > most {
            return Err(format!(
                "The board will let you move at most {} that way.",
                money(most)
            ));
        }
        let b = &mut self.econ.books[team];
        if to_wages {
            b.transfer_budget -= amount;
            b.wage_budget += amount;
        } else {
            b.wage_budget -= amount;
            b.transfer_budget += amount;
        }
        Ok(format!(
            "{} moved to the {} budget.",
            money(amount),
            if to_wages { "wage" } else { "transfer" }
        ))
    }

    /// Clubs over their wage budget keep renewals at current pay.
    pub(crate) fn renewals_capped(&self) -> Vec<bool> {
        (0..self.teams.len())
            .map(|t| {
                self.econ
                    .books
                    .get(t)
                    .is_some_and(|b| self.wage_left(t) < 0 && b.wage_budget > 0)
            })
            .collect()
    }

    /// The cap on the whole payroll, for text.
    pub fn cap(&self) -> i64 {
        SALARY_CAP
    }
}

pub(crate) fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}
