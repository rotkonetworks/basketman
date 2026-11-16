//! Where the books are heading: a projection of the rest of the season at
//! the club's current standing, price and payroll. It is a forecast, not a
//! promise: crowds move with results, the playoffs are not counted, and
//! contracts can change.
//!
//! Pure functions of the world.

use crate::finance::{Cat, PRIZE_TOP};
use crate::league::{TeamId, World};
use crate::pricing::attendance_at_price;

/// What the rest of the season is expected to bring, by kind.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Projection {
    /// Regular rounds, and the club's home games, still to play.
    pub rounds_left: i64,
    pub home_games_left: usize,
    /// Positive in, negative out.
    pub gate: i64,
    pub sponsors: i64,
    pub broadcast: i64,
    pub merchandise: i64,
    pub prize: i64,
    pub wages: i64,
    pub staff: i64,
    pub upkeep: i64,
    pub facilities: i64,
    pub tax: i64,
    pub interest: i64,
    /// The budget now, and what it comes to at the end of the season.
    pub now: i64,
    pub closing: i64,
}

impl Projection {
    pub fn income(&self) -> i64 {
        self.gate + self.sponsors + self.broadcast + self.merchandise + self.prize
    }

    pub fn spending(&self) -> i64 {
        -(self.wages + self.staff + self.upkeep + self.facilities + self.tax + self.interest)
    }

    /// (label, amount) rows for the screen, skipping what is zero.
    pub fn rows(&self) -> Vec<(&'static str, i64)> {
        [
            (Cat::Gate.label(), self.gate),
            (Cat::Sponsorship.label(), self.sponsors),
            (Cat::Broadcast.label(), self.broadcast),
            (Cat::Merchandise.label(), self.merchandise),
            (Cat::Prize.label(), self.prize),
            (Cat::PlayerWages.label(), self.wages),
            (Cat::StaffWages.label(), self.staff),
            (Cat::Upkeep.label(), self.upkeep),
            (Cat::FacilityRunning.label(), self.facilities),
            (Cat::LuxuryTax.label(), self.tax),
            (Cat::Interest.label(), self.interest),
        ]
        .into_iter()
        .filter(|r| r.1 != 0)
        .collect()
    }
}

impl World {
    /// The rest of the season for `team`.
    pub fn projection(&self, team: TeamId) -> Projection {
        let regular = self.pay_rounds();
        let rounds_left = (regular - self.round as i64).max(0);
        let paying = self.wage_rounds_left();
        let price = self.ticket_price(team);
        let fair = self.fair_price(team) as f32;
        let mut home = 0usize;
        let mut gate = 0i64;
        for (i, f) in self.fixtures.iter().enumerate() {
            if f.home == team && f.result.is_none() && !f.playoff {
                home += 1;
                let d = self.draw_for(i);
                gate += attendance_at_price(&d, fair, price as f32) as i64 * price;
            }
        }
        let a = &self.econ.arenas[team];
        let sponsors: i64 = self
            .econ
            .deals
            .iter()
            .filter(|d| d.club == team)
            .map(|d| d.fee / regular.max(1))
            .sum::<i64>()
            * rounds_left;
        let n = self.division_size(team) as i64;
        let place = self
            .table_of(self.teams[team].division)
            .iter()
            .position(|&t| t == team)
            .map_or(0, |p| p as i64);
        let prize = if self.season_over() || n < 2 {
            0
        } else {
            (PRIZE_TOP as f32 * self.money_scale(team)) as i64 * (n - 1 - place) / (n - 1)
        };
        let tax = -self.tax_view(team).to_pay();
        let now = self.teams[team].budget;
        let mut p = Projection {
            rounds_left,
            home_games_left: home,
            gate,
            sponsors,
            broadcast: self.broadcast_season(team) / regular.max(1) * rounds_left,
            merchandise: self.merch_week(team) * rounds_left,
            prize,
            wages: -(self.squad_wages(team) / 52 * paying),
            staff: -(self.teams[team].office.wages() / 52 * paying),
            upkeep: -(a.running_cost() * home as i64),
            facilities: -(self.facilities(team).yearly_running() / regular.max(1) * rounds_left),
            tax,
            interest: -(self.interest_due(team) * rounds_left),
            now,
            closing: 0,
        };
        p.closing =
            now + p.income() + p.wages + p.staff + p.upkeep + p.facilities + p.tax + p.interest;
        p
    }
}
