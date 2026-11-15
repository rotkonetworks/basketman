//! Ticket prices: what a crowd will pay, and how many come at a given price.
//!
//! `arena::fill` says how full the hall is at the club's reference price
//! (`Arena::ticket_price`). Here a price of the manager's choosing moves that
//! figure. How much the crowd would happily pay (`willingness`) rises with
//! everything that already fills a hall: results, the table, the mood around
//! the team, the visitors, a decent building and the playoffs. Demand falls in
//! a straight line from the reference price to a ceiling set by that
//! willingness, so the best price sits where the loss of seats starts to cost
//! more than the extra money per seat. A winning club with a loud, well
//! supported hall can charge a lot; a losing club that does is left with
//! empty rows. A hall that is already full is the other limit: raising the
//! price is free until seats start to empty.
//!
//! Charging less than the crowd would pay builds support over the season and
//! charging more wears it down (`Books::value_sum`, settled when the season
//! closes).
//!
//! Pure functions of the world: nothing here draws from `World::rng`.

use crate::arena::{Draw, fill};
use crate::league::{TeamId, World};

/// The price at which nobody comes, as a multiple of what the crowd would
/// happily pay.
const CEILING: f32 = 2.2;
/// Cheapest and dearest price, as multiples of the reference price.
const LOW: f32 = 0.55;
const HIGH: f32 = 1.9;
/// How many rounds pass between an AI club looking at its price again.
pub(crate) const AI_REPRICE: usize = 4;

/// What the crowd would happily pay for a seat, given `fair`, the reference
/// price. Between about 0.6 and 1.5 times it.
pub fn willingness(d: &Draw, fair: f32) -> f32 {
    let mood = 0.22 * d.form
        + 0.18 * d.belief
        + 0.22 * d.position
        + 0.14 * d.opponent
        + 0.14 * d.support
        + 0.10 * d.condition
        + 0.10 * d.star
        + 0.04 * d.visitor
        + 0.05 * (d.fans - 0.5);
    let mut w = fair * (0.62 + 0.9 * mood);
    if d.playoff {
        w *= 1.2;
    }
    w
}

/// The share of seats filled at `price`, 0..=1.
pub fn fill_at(d: &Draw, fair: f32, price: f32) -> f32 {
    let w = willingness(d, fair);
    let top = CEILING * w;
    let slope = (top - fair).max(0.1 * fair);
    let g = ((top - price) / slope).max(0.0);
    (fill(d) * g).clamp(0.0, 1.0)
}

/// People in the seats at `price`.
pub fn attendance_at_price(d: &Draw, fair: f32, price: f32) -> u32 {
    ((d.capacity as f32 * fill_at(d, fair, price)) as u32).min(d.capacity)
}

/// The gate for a game, for the preview and for the AI's choice.
pub fn gate_at(d: &Draw, fair: f32, price: i64) -> (u32, i64) {
    let att = attendance_at_price(d, fair, price as f32);
    (att, att as i64 * price)
}

/// How far under what the crowd would pay the price is, as a share of it:
/// positive is a bargain, negative is dear.
pub fn bargain(d: &Draw, fair: f32, price: i64) -> f32 {
    let w = willingness(d, fair).max(0.1);
    ((w - price as f32) / w).clamp(-0.4, 0.4)
}

/// The price that earns most at the gate for a typical game under `d`, with
/// a small lean to the cheaper side when it ties, and a thumb on the scale
/// for the goodwill a bargain buys.
pub fn best_price(d: &Draw, fair: f32, low: i64, high: i64) -> i64 {
    let mut best = (f32::MIN, low);
    for p in low..=high {
        let (_, gate) = gate_at(d, fair, p);
        let score = gate as f32 * (1.0 + 0.12 * bargain(d, fair, p));
        if score > best.0 + 1.0 {
            best = (score, p);
        }
    }
    best.1
}

/// One line of the manager's preview: a price and what it would bring.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PricePoint {
    pub price: i64,
    pub attendance: u32,
    pub gate: i64,
}

/// What the next home game looks like at the price in force and one step
/// either side of it.
#[derive(Clone, Debug, PartialEq)]
pub struct PricePreview {
    pub fixture: usize,
    pub opponent: TeamId,
    pub playoff: bool,
    /// What the crowd would happily pay for this game.
    pub willing: f32,
    /// Lower, current, higher.
    pub points: [PricePoint; 3],
    /// The price that would take most at the gate for this game.
    pub best: i64,
}

impl World {
    /// The price the club starts from: set by prestige and the hall.
    pub fn fair_price(&self, team: TeamId) -> i64 {
        self.econ.arenas[team].ticket_price()
    }

    /// The price in force.
    pub fn ticket_price(&self, team: TeamId) -> i64 {
        match self.econ.books.get(team).map_or(0, |b| b.price) {
            0 => self.fair_price(team),
            p => p,
        }
    }

    /// Cheapest and dearest price the club may set.
    pub fn price_range(&self, team: TeamId) -> (i64, i64) {
        let fair = self.fair_price(team) as f32;
        (
            ((fair * LOW).round() as i64).max(2),
            (fair * HIGH).round() as i64,
        )
    }

    /// People at `d` for the club at home, at `price`.
    pub fn attendance_priced(&self, d: &Draw, home: TeamId, price: i64) -> u32 {
        attendance_at_price(d, self.fair_price(home) as f32, price as f32)
    }

    /// A game with nothing special about it, as the club is now: for
    /// choosing a price and for projections.
    pub fn typical_draw(&self, team: TeamId) -> Draw {
        let a = &self.econ.arenas[team];
        Draw {
            capacity: a.capacity,
            support: a.support,
            form: self.form(team),
            position: self.standing_of(team),
            belief: self.teams[team].belief,
            opponent: 0.5,
            condition: a.condition,
            playoff: false,
            luck: 0.0,
            fans: self.fan_index(team),
            star: self.star_pull(team, team),
            visitor: 0.0,
            night: 0.0,
        }
    }

    /// The price an AI club would set today.
    pub fn sensible_price(&self, team: TeamId) -> i64 {
        let (low, high) = self.price_range(team);
        best_price(
            &self.typical_draw(team),
            self.fair_price(team) as f32,
            low,
            high,
        )
    }

    /// The next home game of `team` that has not been played.
    pub fn next_home_game(&self, team: TeamId) -> Option<usize> {
        (0..self.fixtures.len()).find(|&i| {
            let f = &self.fixtures[i];
            f.home == team && f.result.is_none()
        })
    }

    /// Crowd and gate for the next home game at the price in force and a
    /// step either side. `None` when there is no home game left.
    pub fn price_preview(&self, team: TeamId) -> Option<PricePreview> {
        let i = self.next_home_game(team)?;
        let f = &self.fixtures[i];
        let d = self.draw_for(i);
        let fair = self.fair_price(team) as f32;
        let now = self.ticket_price(team);
        let (low, high) = self.price_range(team);
        let point = |price: i64| {
            let (attendance, gate) = gate_at(&d, fair, price);
            let gate = if f.playoff { gate * 3 / 2 } else { gate };
            PricePoint {
                price,
                attendance,
                gate,
            }
        };
        let step = (now / 8).max(1);
        Some(PricePreview {
            fixture: i,
            opponent: f.away,
            playoff: f.playoff,
            willing: willingness(&d, fair),
            points: [
                point((now - step).max(low)),
                point(now),
                point((now + step).min(high)),
            ],
            best: best_price(&d, fair, low, high),
        })
    }

    /// Note how good a bargain the home game was, for the support the
    /// season's prices build or spend.
    pub(crate) fn note_bargain(&mut self, home: TeamId, d: &Draw, price: i64) {
        let fair = self.fair_price(home) as f32;
        let v = bargain(d, fair, price);
        if let Some(b) = self.econ.books.get_mut(home) {
            b.value_sum += v;
            b.value_games += 1;
        }
    }

    /// AI clubs look at their price now and then.
    pub(crate) fn ai_pricing(&mut self) {
        if !self.round.is_multiple_of(AI_REPRICE) {
            return;
        }
        for club in 0..self.teams.len() {
            if Some(club) == self.watch {
                continue;
            }
            let price = self.sensible_price(club);
            self.econ.books[club].price = price;
        }
    }
}

impl World {
    /// The season is over: a club that charged less than the crowd would
    /// have paid has built some support, one that charged more has spent it.
    pub(crate) fn settle_bargains(&mut self) {
        for club in 0..self.teams.len() {
            let b = &mut self.econ.books[club];
            if b.value_games > 0 {
                let avg = b.value_sum / b.value_games as f32;
                let a = &mut self.econ.arenas[club];
                a.support = (a.support + 0.25 * avg).clamp(0.1, 0.95);
            }
            b.value_sum = 0.0;
            b.value_games = 0;
        }
    }
}
