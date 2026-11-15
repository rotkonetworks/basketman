//! What a club's board and finance office keep besides the ledger: the ticket
//! price it has set, the wage and transfer budgets, how its training ground,
//! medical centre and youth setup are doing, and whether it has been told to
//! sell. One `Books` per club, in `Economy::books`, indexed by `TeamId`.
//!
//! The behaviour lives in the modules named after what it does: `pricing`,
//! `income`, `budget`, `debt`, `tax`, `facility`, `projection`.

use serde::{Deserialize, Serialize};

use crate::league::{TeamId, World};
use crate::player::PlayerId;
use crate::rng::Rng;

/// The three things a club can invest in.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FacilityKind {
    /// Players gain more from training.
    Training,
    /// Fewer injuries, and shorter ones.
    Medical,
    /// A better yearly intake of young players.
    Youth,
}

impl FacilityKind {
    pub const ALL: [FacilityKind; 3] = [
        FacilityKind::Training,
        FacilityKind::Medical,
        FacilityKind::Youth,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FacilityKind::Training => "Training ground",
            FacilityKind::Medical => "Medical centre",
            FacilityKind::Youth => "Youth setup",
        }
    }
}

/// Levels, 1 to `MAX_LEVEL`. Level 3 is the league's ordinary: it changes
/// nothing.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facilities {
    pub training: u8,
    pub medical: u8,
    pub youth: u8,
}

impl Facilities {
    pub fn level(&self, k: FacilityKind) -> u8 {
        match k {
            FacilityKind::Training => self.training,
            FacilityKind::Medical => self.medical,
            FacilityKind::Youth => self.youth,
        }
    }

    pub(crate) fn set(&mut self, k: FacilityKind, level: u8) {
        match k {
            FacilityKind::Training => self.training = level,
            FacilityKind::Medical => self.medical = level,
            FacilityKind::Youth => self.youth = level,
        }
    }
}

impl Default for Facilities {
    fn default() -> Self {
        Facilities {
            training: 3,
            medical: 3,
            youth: 3,
        }
    }
}

/// A facility being built: it opens when the calendar reaches
/// (`season`, `round`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Works {
    pub kind: FacilityKind,
    pub to: u8,
    pub season: u16,
    pub round: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Books {
    /// The ticket price. 0 means the arena's own reference price.
    pub price: i64,
    /// Over this season's home games: the sum of how far under what the
    /// crowd would pay the price was (negative when over), and the games.
    pub value_sum: f32,
    pub value_games: u16,
    /// What the board lets the club spend on players' wages this season,
    /// in cash (what the wage bill will take out of the budget).
    pub wage_budget: i64,
    /// What it lets the club spend on fees and cash in trades.
    pub transfer_budget: i64,
    /// What the board first set, before the manager moved money.
    pub wage_board: i64,
    pub transfer_board: i64,
    /// Net fees paid this season (fees received count for half).
    pub transfer_spent: i64,
    pub facilities: Facilities,
    #[serde(default)]
    pub works: Option<Works>,
    /// The finish the board expects this season; 0 until set. The user's is
    /// in `desk.expectation`.
    #[serde(default)]
    pub expect: usize,
    /// A player put on the market because the club is in too deep, and the
    /// round it happened in.
    #[serde(default)]
    pub listed: Option<(PlayerId, u16)>,
    /// The manager was told about the overdraft, or the demand to sell.
    #[serde(default)]
    pub told_overdraft: bool,
    #[serde(default)]
    pub told_sale: bool,
}

impl Books {
    /// Starting facility levels follow the hall's prestige loosely.
    pub(crate) fn generate(seed: u64, club: TeamId, prestige: f32) -> Books {
        let mut rng = Rng::new(seed ^ 0xFAC1_0000 ^ (club as u64).wrapping_mul(0x9E37_79B9));
        let mut level = |bias: f32| {
            let v = 1.0 + 3.0 * prestige + bias + (rng.f32() - 0.5) * 1.6;
            (v + 0.5).floor().clamp(1.0, 4.0) as u8
        };
        let facilities = Facilities {
            training: level(0.0),
            medical: level(0.0),
            youth: level(0.0),
        };
        Books {
            facilities,
            ..Books::default()
        }
    }
}

impl World {
    pub fn books(&self, team: TeamId) -> &Books {
        &self.econ.books[team]
    }
}
