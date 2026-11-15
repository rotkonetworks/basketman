//! The league-wide weekly and yearly work of the finance system: interest,
//! tax, building, prices, budgets and debts. `finance.rs` calls these from
//! the hooks it already has.

use crate::league::World;

impl World {
    /// After every round, with `self.round` already counting it. Runs
    /// whether or not anyone is managing a club.
    pub(crate) fn econ_weekly(&mut self) {
        self.charge_interest();
        self.tax_checkpoint();
        self.finish_works();
        self.ai_pricing();
        if self.round == self.pay_rounds() as usize / 2 {
            self.review_budgets();
        }
        self.watch_debts();
    }

    /// The summer: clubs invest, the boards set budgets, prices are chosen.
    pub(crate) fn econ_summer(&mut self) {
        self.ai_invest();
        self.ai_cap_discipline();
        self.open_budgets();
        self.finish_works();
        for club in 0..self.teams.len() {
            if self.watch != Some(club) {
                let price = self.sensible_price(club);
                self.econ.books[club].price = price;
            }
        }
    }
}
