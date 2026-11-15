//! The luxury tax. A club whose payroll (first-team contracts, the figure
//! the salary cap is measured on) is above `SALARY_CAP` pays tax on the
//! excess, at a rate that climbs in bands. It is charged in two halves, at the
//! middle of the regular season and at its end, on the payroll the club has
//! on those days. Everything collected is shared equally among the clubs at
//! or under the cap on the same day, to the last unit: the remainder goes out
//! one unit at a time to the first of them. If every club is over the cap,
//! nobody pays that day, since there is nobody to share it with.
//!
//! General managers know about it. A win-now general manager (low patience)
//! counts a tax bill at half its cost when he weighs a deal, a builder at one
//! and a half times.
//!
//! Nothing here draws from `World::rng`.

use crate::finance::Cat;
use crate::inbox::{Kind, money};
use crate::league::{TeamId, World};
use crate::season::SALARY_CAP;

/// The bands of a year's tax on the excess over the cap: how wide each is,
/// and the percent paid on what falls in it. The last has no end.
pub const BANDS: [(i64, i64); 3] = [(40_000, 50), (60_000, 100), (i64::MAX, 150)];
/// Times in the regular season the tax is charged, each for a share of the
/// year's.
pub const CHECKPOINTS: i64 = 2;

/// The year's tax on `excess` above the cap.
pub fn annual_tax(excess: i64) -> i64 {
    let mut left = excess.max(0);
    let mut tax = 0;
    for (width, pct) in BANDS {
        let part = left.min(width);
        tax += part * pct / 100;
        left -= part;
        if left == 0 {
            break;
        }
    }
    tax
}

/// A club's position on the tax at one moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaxView {
    pub payroll: i64,
    pub cap: i64,
    pub excess: i64,
    /// A full year's tax at this payroll.
    pub annual: i64,
    /// What each of the remaining checkpoints would take.
    pub per_checkpoint: i64,
    /// Checkpoints left this season.
    pub checkpoints_left: i64,
    /// The rate on the next unit over, in percent.
    pub marginal: i64,
}

impl TaxView {
    /// What is still to be paid this season if the payroll stays as it is.
    pub fn to_pay(&self) -> i64 {
        self.per_checkpoint * self.checkpoints_left
    }
}

impl World {
    /// Round after which a checkpoint falls: halfway and the last of the
    /// regular season.
    fn checkpoint_rounds(&self) -> [usize; 2] {
        let r = self.pay_rounds() as usize;
        [r / 2, r]
    }

    pub fn tax_view(&self, team: TeamId) -> TaxView {
        self.tax_view_at(team, self.payroll(team))
    }

    pub fn tax_view_at(&self, team: TeamId, payroll: i64) -> TaxView {
        let _ = team;
        let excess = (payroll - SALARY_CAP).max(0);
        let annual = annual_tax(excess);
        let mut left = 0;
        for c in self.checkpoint_rounds() {
            if self.round < c {
                left += 1;
            }
        }
        let mut marginal = 0;
        let mut seen = 0;
        for (width, pct) in BANDS {
            marginal = pct;
            if width == i64::MAX || excess < seen + width {
                break;
            }
            seen += width;
        }
        TaxView {
            payroll,
            cap: SALARY_CAP,
            excess,
            annual,
            per_checkpoint: annual / CHECKPOINTS,
            checkpoints_left: left,
            marginal: if excess > 0 { marginal } else { 0 },
        }
    }

    /// How much a general manager counts a tax bill: win-now 0.5, builder 1.5.
    pub fn tax_weight(&self, club: TeamId) -> f32 {
        1.0 + 0.5 * self.teams[club].office.gm.lean()
    }

    /// What it would add to a year's tax, as `club`'s general manager
    /// weighs it, to take on `extra` of yearly salary.
    pub fn tax_felt(&self, club: TeamId, extra: i64) -> i64 {
        let now = self.payroll(club);
        let more =
            annual_tax((now + extra - SALARY_CAP).max(0)) - annual_tax((now - SALARY_CAP).max(0));
        (more.max(0) as f32 * self.tax_weight(club)) as i64
    }

    /// Charge the tax if this is a checkpoint.
    pub(crate) fn tax_checkpoint(&mut self) {
        if !self.checkpoint_rounds().contains(&self.round) {
            return;
        }
        let n = self.teams.len();
        let payrolls: Vec<i64> = (0..n).map(|t| self.payroll(t)).collect();
        let under: Vec<TeamId> = (0..n).filter(|&t| payrolls[t] <= SALARY_CAP).collect();
        let mut bills = vec![0i64; n];
        if !under.is_empty() {
            for t in 0..n {
                bills[t] = self.tax_view_at(t, payrolls[t]).annual / CHECKPOINTS;
            }
        }
        let total: i64 = bills.iter().sum();
        for (t, &bill) in bills.iter().enumerate() {
            if bill > 0 {
                self.transact(t, -bill, Cat::LuxuryTax);
            }
        }
        let mut shares = vec![0i64; n];
        if total > 0 {
            let each = total / under.len() as i64;
            let extra = (total % under.len() as i64) as usize;
            for (i, &t) in under.iter().enumerate() {
                shares[t] = each + (i < extra) as i64;
            }
            for (t, &s) in shares.iter().enumerate() {
                if s > 0 {
                    self.transact(t, s, Cat::TaxShare);
                }
            }
        }
        let Some(me) = self.watch else { return };
        let day = self.day;
        if bills[me] > 0 {
            let v = self.tax_view_at(me, payrolls[me]);
            let body = format!(
                "The luxury tax has been charged: {} for a payroll of {} against the cap of {}. \
                 That is the tax on {} over the cap, at rates that climb from {}% to {}% by \
                 band. It is charged in two halves a season and the money is shared among the \
                 clubs under the cap.\n\nIf the payroll stays as it is the next charge is {}.",
                money(bills[me]),
                money(v.payroll),
                money(SALARY_CAP),
                money(v.excess),
                BANDS[0].1,
                BANDS[BANDS.len() - 1].1,
                if self.round < self.pay_rounds() as usize {
                    money(v.per_checkpoint)
                } else {
                    "due next season".to_string()
                },
            );
            self.post(
                day,
                Kind::Board,
                "The board",
                "Luxury tax bill",
                body,
                vec![],
            );
        } else if shares[me] > 0 {
            let body = format!(
                "The league has shared out {} of luxury tax among the {} clubs at or under the \
                 cap. Our payroll of {} is under the cap of {}, and our share is {}.",
                money(total),
                under.len(),
                money(payrolls[me]),
                money(SALARY_CAP),
                money(shares[me]),
            );
            self.post(
                day,
                Kind::League,
                "League office",
                "Luxury tax share",
                body,
                vec![],
            );
        }
    }
}

impl World {
    /// In the summer a general manager whose payroll is past the cap by more
    /// than he is prepared to pay tax on lets the least useful players go
    /// (a builder at once, a win-now manager only when it is a lot). They
    /// become free agents.
    pub(crate) fn ai_cap_discipline(&mut self) {
        for club in 0..self.teams.len() {
            if Some(club) == self.watch {
                continue;
            }
            let lean = self.teams[club].office.gm.lean();
            let tolerance = (60_000.0 * (1.0 - lean)) as i64;
            while self.payroll(club) > SALARY_CAP + tolerance
                && self.teams[club].roster.len() > crate::inbox::MIN_ROSTER + 1
            {
                // Not one of the eight best; the one who costs most for
                // what he gives.
                let mut order: Vec<_> = self.teams[club].roster.clone();
                order.sort_by(|&a, &b| {
                    self.players[b as usize]
                        .overall()
                        .total_cmp(&self.players[a as usize].overall())
                });
                let Some(&out) = order.iter().skip(8).max_by_key(|&&p| {
                    self.players[p as usize].contract.salary as i64 - self.value(p) / 8
                }) else {
                    break;
                };
                self.teams[club].roster.retain(|&p| p != out);
                self.teams[club].down.retain(|&p| p != out);
                self.refit(club);
            }
        }
    }
}
