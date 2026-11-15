//! Running short of money. A club whose budget goes below zero is in
//! overdraft: the bank lends up to a limit that depends on the size of the
//! club, and charges interest every round on what is owed. Beyond the limit
//! the board freezes spending (no bids, hires, renovations, expansions or new
//! facilities) and demands a sale: the user gets a letter, and an AI club
//! puts its most valuable player it can spare on the market at a discount,
//! and if nobody takes him within a few rounds sells him abroad. There is no
//! bankruptcy and there are no points deductions.
//!
//! Nothing here draws from `World::rng`.

use crate::finance::Cat;
use crate::inbox::{Kind, MIN_ROSTER, money};
use crate::league::{TeamId, World};
use crate::player::PlayerId;

/// Interest charged on an overdraft each round, in thousandths.
pub const INTEREST_PER_ROUND: i64 = 4;
/// What a listed player goes for, in percent of his value.
pub const FIRE_SALE: i64 = 75;
/// Rounds a listed player waits for a buyer before he is sold abroad.
const WAIT: u16 = 3;

/// A club's money in a word.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MoneyState {
    Sound,
    Tight,
    Overdrawn,
    Frozen,
}

impl MoneyState {
    pub fn label(self) -> &'static str {
        match self {
            MoneyState::Sound => "Sound",
            MoneyState::Tight => "Tight",
            MoneyState::Overdrawn => "Overdrawn",
            MoneyState::Frozen => "Over its limit",
        }
    }
}

impl World {
    /// How far below zero the bank lets the club go. Bigger clubs borrow
    /// more.
    pub fn overdraft_limit(&self, team: TeamId) -> i64 {
        120_000 + 40 * self.econ.arenas[team].capacity as i64
    }

    pub fn in_overdraft(&self, team: TeamId) -> bool {
        self.teams[team].budget < 0
    }

    /// How a club's money looks from outside: a rival manager can read it
    /// off the league's published accounts without seeing the figure.
    pub fn money_state(&self, team: TeamId) -> MoneyState {
        let b = self.teams[team].budget;
        if self.spending_frozen(team) {
            MoneyState::Frozen
        } else if b < 0 {
            MoneyState::Overdrawn
        } else if b < 300_000 {
            MoneyState::Tight
        } else {
            MoneyState::Sound
        }
    }

    /// What the overdraft costs for the next round, 0 if there is none.
    pub fn interest_due(&self, team: TeamId) -> i64 {
        let owed = -self.teams[team].budget;
        if owed <= 0 {
            0
        } else {
            (owed * INTEREST_PER_ROUND + 999) / 1000
        }
    }

    /// Charge the round's interest on every overdraft.
    pub(crate) fn charge_interest(&mut self) {
        for club in 0..self.teams.len() {
            let due = self.interest_due(club);
            if due > 0 {
                self.transact(club, -due, Cat::Interest);
            }
        }
    }

    /// The player a club would sell first: the most valuable it can do
    /// without, leaving at least the minimum squad.
    fn saleable(&self, club: TeamId) -> Option<PlayerId> {
        if self.teams[club].roster.len() <= MIN_ROSTER + 1 {
            return None;
        }
        self.teams[club]
            .roster
            .iter()
            .copied()
            .max_by_key(|&p| (self.value(p), p))
    }

    /// The price a listed player asks.
    pub fn listed_price(&self, p: PlayerId) -> i64 {
        self.value(p) * FIRE_SALE / 100 / 1000 * 1000
    }

    /// Who is on the market because their club is in too deep.
    pub fn is_listed(&self, p: PlayerId) -> bool {
        self.econ
            .books
            .iter()
            .any(|b| b.listed.is_some_and(|l| l.0 == p))
    }

    /// Debts after a round: tell the user, and make AI clubs sell.
    pub(crate) fn watch_debts(&mut self) {
        let round = self.round as u16;
        for club in 0..self.teams.len() {
            let frozen = self.spending_frozen(club);
            let budget = self.teams[club].budget;
            let mine = self.watch == Some(club);
            if budget >= 0 {
                let b = &mut self.econ.books[club];
                b.told_overdraft = false;
            }
            if !frozen {
                let b = &mut self.econ.books[club];
                b.told_sale = false;
                if budget > -self.overdraft_limit(club) / 2 {
                    self.econ.books[club].listed = None;
                }
            }
            if mine && budget < 0 && !self.econ.books[club].told_overdraft && !frozen {
                self.econ.books[club].told_overdraft = true;
                let body = format!(
                    "The budget stands at {}, so we are in overdraft. The bank will lend us up to \
                     {} and charges {} a round on what we owe, about {} at the moment. Gate, \
                     sponsor and broadcast money keep coming in and wages are paid as usual, so \
                     the books recover if results and crowds hold up. Over the limit the board \
                     freezes spending.",
                    money(budget),
                    money(self.overdraft_limit(club)),
                    percent(INTEREST_PER_ROUND),
                    money(self.interest_due(club)),
                );
                let day = self.day;
                self.post(
                    day,
                    Kind::Board,
                    "The board",
                    "We are in overdraft",
                    body,
                    vec![],
                );
            }
            if frozen && !self.econ.books[club].told_sale {
                self.econ.books[club].told_sale = true;
                self.econ.warned = true;
                if mine {
                    let who = self.saleable(club);
                    let name = who.map_or("someone we can spare".to_string(), |p| {
                        format!(
                            "{} (worth about {})",
                            self.players[p as usize].name,
                            money(self.value(p))
                        )
                    });
                    let body = format!(
                        "The budget stands at {}, which is past the {} the bank allows. Until it \
                         is back inside the limit the board will not sign off on bids, new staff, \
                         renovations, expansion or new facilities. The board expects a sale: {name} \
                         would bring the club back in line. Gate, sponsor and broadcast money \
                         keep coming in, and wages are paid as usual.",
                        money(budget),
                        money(self.overdraft_limit(club)),
                    );
                    let day = self.day;
                    self.post(
                        day,
                        Kind::Board,
                        "The board",
                        "The board demands a sale",
                        body,
                        who.into_iter().collect(),
                    );
                }
            }
            if frozen && !mine {
                self.ai_sells(club, round);
            }
        }
    }

    /// An AI club in too deep puts a player up at a discount, and when
    /// nobody has bought him after a few rounds sells him abroad.
    fn ai_sells(&mut self, club: TeamId, round: u16) {
        match self.econ.books[club].listed {
            None => {
                if let Some(p) = self.saleable(club) {
                    self.econ.books[club].listed = Some((p, round));
                }
            }
            Some((p, since)) => {
                let gone = !self.teams[club].roster.contains(&p);
                if gone {
                    self.econ.books[club].listed = None;
                } else if round >= since + WAIT && self.teams[club].roster.len() > MIN_ROSTER {
                    let fee = self.listed_price(p);
                    self.teams[club].roster.retain(|&q| q != p);
                    self.teams[club].down.retain(|&q| q != p);
                    self.transact(club, fee, Cat::TransferIn);
                    self.econ.books[club].listed = None;
                    self.refit(club);
                }
            }
        }
    }
}

fn percent(thousandths: i64) -> String {
    format!("{}.{}%", thousandths / 10, thousandths % 10)
}
