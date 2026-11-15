//! Facilities: the training ground, the medical centre and the youth setup.
//! Each has a level from 1 to 5. Level 3 is what the league takes for
//! granted and changes nothing; every step away from it moves the effect.
//!
//! - Training ground: multiplies what players gain from training, on top of
//!   the skills coach. 7% a level.
//! - Medical centre: injury risk (and the wear of heavy weeks) falls 7% a
//!   level, and the time out of an injury 6% a level, on top of the fitness
//!   coach.
//! - Youth setup: the yearly intake of young players arrives better, by
//!   about a third of a point of quality and half a point of potential a
//!   level.
//!
//! Building a level costs a lump sum up front, takes six rounds, and from
//! then on the facility costs more to run: a yearly sum paid a little each
//! regular round. AI clubs invest when they can afford it.
//!
//! Nothing here draws from `World::rng`.

use crate::books::{Facilities, FacilityKind, Works};
use crate::finance::Cat;
use crate::inbox::{Kind, money};
use crate::league::{TeamId, World};

pub const MAX_LEVEL: u8 = 5;
/// Rounds a build takes.
pub const BUILD_ROUNDS: u16 = 6;
/// What it costs to build up to a level, indexed by that level.
const BUILD_COST: [i64; 6] = [0, 0, 40_000, 75_000, 120_000, 180_000];
/// What a facility costs to run for a season at each level.
const RUN_COST: [i64; 6] = [0, 4_000, 8_000, 13_000, 19_000, 26_000];

/// Cost of building `kind` up to `to`.
pub fn build_cost(to: u8) -> i64 {
    BUILD_COST[(to as usize).min(5)]
}

/// Yearly running cost of a facility at `level`.
pub fn running_cost(level: u8) -> i64 {
    RUN_COST[(level as usize).min(5)]
}

fn step(level: u8) -> f32 {
    level as f32 - 3.0
}

impl Facilities {
    /// Multiplier on what training gives.
    pub fn training_mult(&self) -> f32 {
        1.0 + 0.07 * step(self.training)
    }

    /// Multiplier on injury risk and on the wear of heavy weeks.
    pub fn injury_mult(&self) -> f32 {
        1.0 - 0.07 * step(self.medical)
    }

    /// Multiplier on the weeks an injury takes.
    pub fn recovery_mult(&self) -> f32 {
        1.0 - 0.06 * step(self.medical)
    }

    /// Added to the quality of the yearly youth intake.
    pub fn intake_quality(&self) -> f32 {
        0.35 * step(self.youth)
    }

    /// Added to the potential of the yearly youth intake.
    pub fn intake_potential(&self) -> f32 {
        0.5 * step(self.youth)
    }

    /// Yearly cost of running all three.
    pub fn yearly_running(&self) -> i64 {
        running_cost(self.training) + running_cost(self.medical) + running_cost(self.youth)
    }
}

/// What a level does, in words.
pub fn effect_words(kind: FacilityKind, level: u8) -> String {
    let f = |v: f32| {
        let pct = (v * 100.0).round() as i32;
        if pct >= 0 {
            format!("{pct}% more")
        } else {
            format!("{}% less", -pct)
        }
    };
    let mut fac = Facilities::default();
    fac.set(kind, level);
    match kind {
        FacilityKind::Training => {
            let v = fac.training_mult() - 1.0;
            if v == 0.0 {
                "Players gain what they ordinarily do from training.".to_string()
            } else {
                format!(
                    "Players gain {} from training than they ordinarily do.",
                    f(v)
                )
            }
        }
        FacilityKind::Medical => {
            let (risk, time) = (fac.injury_mult() - 1.0, fac.recovery_mult() - 1.0);
            if risk == 0.0 {
                "Injury risk and recovery times are what they ordinarily are.".to_string()
            } else {
                let (r, t) = ((risk.abs() * 100.0).round(), (time.abs() * 100.0).round());
                let (more, less) = if risk < 0.0 {
                    ("lower", "shorter")
                } else {
                    ("higher", "longer")
                };
                format!("Injury risk is {r}% {more} and injuries are {t}% {less} than ordinarily.")
            }
        }
        FacilityKind::Youth => {
            let (q, p) = (fac.intake_quality(), fac.intake_potential());
            if q == 0.0 {
                "The yearly intake of young players is of ordinary quality.".to_string()
            } else {
                format!(
                    "Each year's young players arrive {:+.1} in quality and {:+.1} in potential \
                     against an ordinary intake.",
                    q, p
                )
            }
        }
    }
}

impl World {
    pub fn facilities(&self, team: TeamId) -> Facilities {
        self.econ
            .books
            .get(team)
            .map_or(Facilities::default(), |b| b.facilities)
    }

    /// The fitness coach's wear factor, as the medical centre changes it.
    pub(crate) fn wear_at(&self, team: TeamId) -> f32 {
        self.teams[team].office.wear() * self.facilities(team).injury_mult()
    }

    /// Why `team` cannot build `kind` up a level now, if it cannot.
    pub fn works_refused(&self, team: TeamId, kind: FacilityKind) -> Option<String> {
        let b = &self.econ.books[team];
        let level = b.facilities.level(kind);
        if level >= MAX_LEVEL {
            return Some("It is as good as it gets.".into());
        }
        if let Some(w) = b.works {
            return Some(format!(
                "The builders are already at the {}.",
                w.kind.label().to_lowercase()
            ));
        }
        if self.spending_frozen(team) {
            return Some(
                "The board has frozen spending until the budget is back inside the limit.".into(),
            );
        }
        if build_cost(level + 1) > self.teams[team].budget {
            return Some("The budget does not stretch that far.".into());
        }
        None
    }

    /// Start building `kind` up a level. The caller has checked.
    pub fn upgrade_facility(&mut self, team: TeamId, kind: FacilityKind) -> Result<String, String> {
        if let Some(why) = self.works_refused(team, kind) {
            return Err(why);
        }
        let to = self.econ.books[team].facilities.level(kind) + 1;
        let cost = build_cost(to);
        self.transact(team, -cost, Cat::FacilityWorks);
        self.econ.books[team].works = Some(Works {
            kind,
            to,
            season: self.season,
            round: self.round as u16 + BUILD_ROUNDS,
        });
        Ok(format!(
            "Work has begun on the {} for {}. It will be level {to} in {BUILD_ROUNDS} rounds.",
            kind.label().to_lowercase(),
            money(cost)
        ))
    }

    /// Open whatever has been finished.
    pub(crate) fn finish_works(&mut self) {
        for club in 0..self.teams.len() {
            let Some(w) = self.econ.books[club].works else {
                continue;
            };
            if (self.season, self.round as u16) < (w.season, w.round) {
                continue;
            }
            let b = &mut self.econ.books[club];
            b.facilities.set(w.kind, w.to);
            b.works = None;
            if self.watch == Some(club) {
                let body = format!(
                    "The {} is finished and is now level {} of {MAX_LEVEL}. {} It costs {} a \
                     season to run, and all three facilities together {}.",
                    w.kind.label().to_lowercase(),
                    w.to,
                    effect_words(w.kind, w.to),
                    money(running_cost(w.to)),
                    money(self.facilities(club).yearly_running()),
                );
                let day = self.day;
                self.post(
                    day,
                    Kind::Board,
                    "The board",
                    format!("{} is ready", w.kind.label()),
                    body,
                    vec![],
                );
            }
        }
    }

    /// Facilities' running costs for one regular round.
    pub(crate) fn book_facility_running(&mut self) {
        let rounds = self.pay_rounds();
        for club in 0..self.teams.len() {
            let cost = self.facilities(club).yearly_running() / rounds;
            if cost > 0 {
                self.transact(club, -cost, Cat::FacilityRunning);
            }
        }
    }

    /// The other clubs invest when they can afford it, once a season: the
    /// weakest facility goes up a level, builders favouring the youth setup
    /// and win-now managers the training ground, if the club is richer than
    /// the work costs several times over and the hall's standing justifies
    /// the level.
    pub(crate) fn ai_invest(&mut self) {
        for club in 0..self.teams.len() {
            if Some(club) == self.watch || self.econ.books[club].works.is_some() {
                continue;
            }
            let f = self.facilities(club);
            let prestige = self.econ.arenas[club].prestige;
            let ceiling = (2.0 + 3.0 * prestige).round().clamp(2.0, MAX_LEVEL as f32) as u8;
            let lean = self.teams[club].office.gm.lean();
            let want = |k: FacilityKind| {
                let bias = match k {
                    FacilityKind::Youth => lean,
                    FacilityKind::Training => -lean * 0.5,
                    FacilityKind::Medical => 0.0,
                };
                f.level(k) as f32 - 0.3 * bias
            };
            let Some(kind) = FacilityKind::ALL
                .into_iter()
                .filter(|&k| f.level(k) < ceiling)
                .min_by(|&a, &b| want(a).total_cmp(&want(b)))
            else {
                continue;
            };
            let cost = build_cost(f.level(kind) + 1);
            let budget = self.teams[club].budget;
            // Only a club that is paying its way builds.
            let paying = self.operating_result(club).is_none_or(|r| r.1 >= -20_000);
            if budget > 700_000 + cost * 5 && paying && !self.spending_frozen(club) {
                let _ = self.upgrade_facility(club, kind);
            }
        }
    }
}
