//! A club's home hall: how big it is, what state it is in, what it looks
//! like, and how many people turn up on a given night.
//!
//! Everything here is a pure function of the world's state. Nothing draws
//! from `World::rng`, so adding arenas leaves every game's dice alone. The
//! money side lives in `finance.rs`, the boards and their sponsors in
//! `sponsor.rs`.

use serde::{Deserialize, Serialize};

use crate::league::{TeamId, World};
use crate::rng::Rng;

/// A colour as red, green, blue.
pub type Rgb = [u8; 3];

pub const MIN_CAPACITY: u32 = 2_500;
pub const MAX_CAPACITY: u32 = 12_000;
/// Seats one expansion adds.
pub const EXPANSION_SEATS: u32 = 1_000;
/// Game-day cost per seat, and extra per seat at the worst condition.
const RUN_BASE: f32 = 1.0;
const RUN_WEAR: f32 = 1.5;

/// The mark painted on the middle of the floor.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CentreMark {
    /// A plain ring.
    Ring,
    /// A star in the ring.
    Star,
    /// A diamond in the ring.
    Diamond,
    /// A bar across the ring.
    Bar,
}

/// What the hall looks like. Colours are plain RGB so any renderer can use
/// them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArenaLook {
    /// Two rings of seats instead of one.
    pub two_tiers: bool,
    /// Seats behind the baskets as well as along the sides.
    pub behind_baskets: bool,
    /// The wood of the court.
    pub floor: Rgb,
    /// The painted key.
    pub paint: Rgb,
    /// The seats.
    pub seats: Rgb,
    /// Rails, fascia and the like.
    pub trim: Rgb,
    pub mark: CentreMark,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Arena {
    pub name: String,
    pub capacity: u32,
    /// 0..=1. Worn out to like new.
    pub condition: f32,
    /// 0..=1. How much the town cares about the club: the intensity. How
    /// many supporters there are is the fan base, see `fans.rs`.
    pub support: f32,
    /// 0..=1. How the club has done lately; sets prices and what sponsors
    /// pay.
    pub prestige: f32,
    /// Seats under construction, open when the season turns.
    #[serde(default)]
    pub pending_seats: u32,
    /// Work was done on the hall this season.
    #[serde(default)]
    pub renovated: bool,
    pub look: ArenaLook,
}

const HALLS: [&str; 7] = [
    "Arena",
    "Sports Hall",
    "Dome",
    "Forum",
    "Coliseum",
    "Sports Palace",
    "Areena",
];

const FLOORS: [Rgb; 6] = [
    [205, 152, 92],
    [222, 178, 118],
    [186, 134, 80],
    [232, 196, 140],
    [170, 120, 72],
    [212, 165, 104],
];

/// Club colours: seats and trim.
const KITS: [(Rgb, Rgb); 12] = [
    ([196, 40, 48], [238, 238, 232]),
    ([30, 84, 168], [244, 200, 60]),
    ([28, 128, 84], [240, 240, 232]),
    ([236, 150, 30], [32, 36, 44]),
    ([104, 52, 150], [236, 220, 120]),
    ([20, 110, 140], [250, 120, 60]),
    ([170, 30, 90], [230, 230, 236]),
    ([60, 64, 76], [220, 70, 60]),
    ([40, 150, 190], [250, 250, 250]),
    ([120, 150, 40], [40, 40, 46]),
    ([210, 70, 30], [30, 60, 110]),
    ([34, 52, 110], [200, 200, 210]),
];

const PAINTS: [Rgb; 6] = [
    [180, 40, 44],
    [36, 84, 150],
    [40, 120, 80],
    [200, 130, 30],
    [100, 56, 140],
    [30, 110, 130],
];

impl Arena {
    /// `strength` is the club's standing in the league by squad quality,
    /// 0..=1. Capacity loosely follows it.
    pub(crate) fn generate(seed: u64, id: TeamId, city: &str, strength: f32) -> Arena {
        let mut rng = Rng::new(seed ^ 0xA2E4_A000 ^ (id as u64).wrapping_mul(0x9E37_79B9));
        let size = 0.5 * strength + 0.5 * rng.f32();
        let capacity = (MIN_CAPACITY as f32 + size * 6_500.0) as u32 / 50 * 50;
        let support = (0.2 + 0.6 * (0.6 * size + 0.4 * rng.f32())).clamp(0.1, 0.95);
        let name = format!(
            "{city} {}",
            HALLS[rng.range(0, HALLS.len() as u32) as usize]
        );
        let (seats, trim) = KITS[rng.range(0, KITS.len() as u32) as usize];
        let look = ArenaLook {
            two_tiers: capacity >= 5_500 && rng.chance(0.7) || capacity >= 7_500,
            behind_baskets: capacity >= 4_000 && rng.chance(0.65),
            floor: FLOORS[rng.range(0, FLOORS.len() as u32) as usize],
            paint: PAINTS[rng.range(0, PAINTS.len() as u32) as usize],
            seats,
            trim,
            mark: [
                CentreMark::Ring,
                CentreMark::Star,
                CentreMark::Diamond,
                CentreMark::Bar,
            ][rng.range(0, 4) as usize],
        };
        Arena {
            name,
            capacity,
            condition: 0.45 + 0.5 * rng.f32(),
            support,
            prestige: 0.3 + 0.4 * strength,
            pending_seats: 0,
            renovated: false,
            look,
        }
    }

    /// The state of the hall in a word.
    pub fn condition_words(&self) -> &'static str {
        match self.condition {
            c if c >= 0.85 => "Like new",
            c if c >= 0.65 => "Good",
            c if c >= 0.45 => "Showing its age",
            c if c >= 0.25 => "Worn",
            _ => "Falling apart",
        }
    }

    /// A seat for a home game. Set by the club once a season: how it has
    /// done lately and how nice the hall is.
    pub fn ticket_price(&self) -> i64 {
        let base = 6.5 + 3.5 * self.prestige;
        (base * (0.8 + 0.4 * self.condition) * 2.0).round() as i64 / 2
    }

    /// What it costs to open the hall for one game: the bigger and the
    /// more worn, the more.
    pub fn running_cost(&self) -> i64 {
        (self.capacity as f32 * (RUN_BASE + RUN_WEAR * (1.0 - self.condition))) as i64
    }

    /// What a renovation costs.
    pub fn renovation_cost(&self) -> i64 {
        self.capacity as i64 * 14
    }

    /// What an expansion by `EXPANSION_SEATS` costs.
    pub fn expansion_cost(&self) -> i64 {
        150_000 + self.capacity as i64 * 10
    }

    /// How much one renovation restores.
    pub const RENOVATION_GAIN: f32 = 0.4;
}

/// Everything about a game that decides who comes.
#[derive(Copy, Clone, Debug)]
pub struct Draw {
    pub capacity: u32,
    pub support: f32,
    /// Share of the last games won, 0..=1.
    pub form: f32,
    /// 0..=1, 1 is top of the table.
    pub position: f32,
    pub belief: f32,
    /// How much the visitors are worth seeing, 0..=1.
    pub opponent: f32,
    pub condition: f32,
    pub playoff: bool,
    /// -1..=1, the night's luck.
    pub luck: f32,
    /// 0..=1, the fan base next to the hall; 0.5 is a normal club.
    pub fans: f32,
    /// 0..=1, what the home side's names are worth at the gate.
    pub star: f32,
    /// 0..=1, what the visitors' names are worth.
    pub visitor: f32,
    /// 0..=1, a night with something to celebrate: a farewell.
    pub night: f32,
}

/// The share of seats filled, 0..=1.
pub fn fill(d: &Draw) -> f32 {
    let buzz = 0.5 * d.form + 0.5 * d.belief;
    // The names that fill halls are counted separately below; the base is
    // what an ordinary club with none draws.
    let mut f = 0.23
        + 0.25 * d.support
        + 0.13 * buzz
        + 0.10 * d.position
        + 0.12 * d.opponent
        + 0.04 * d.luck
        + 0.08 * (d.fans - 0.5)
        + 0.12 * d.star
        + 0.06 * d.visitor
        + 0.06 * d.night;
    f *= 0.85 + 0.15 * d.condition;
    if d.playoff {
        f += 0.22;
    }
    f.clamp(0.12, 1.0)
}

/// People in the seats.
pub fn attendance(d: &Draw) -> u32 {
    ((d.capacity as f32 * fill(d)) as u32).min(d.capacity)
}

/// The home crowd as the match should feel it.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Crowd {
    /// 0..=1, how full the hall is.
    pub full: f32,
    /// 0..=1, how loud and behind the team it is.
    pub loud: f32,
}

/// The crowd for `attendance` people in a hall of `capacity`, with the
/// team's `buzz` (0..=1: form and the dressing room's belief).
pub fn crowd(attendance: u32, capacity: u32, buzz: f32, playoff: bool) -> Crowd {
    let full = if capacity == 0 {
        0.0
    } else {
        (attendance as f32 / capacity as f32).clamp(0.0, 1.0)
    };
    let loud =
        (0.15 + 0.4 * full + 0.3 * buzz + if playoff { 0.15 } else { 0.0 }) * (0.6 + 0.4 * full);
    Crowd {
        full,
        loud: loud.clamp(0.0, 1.0),
    }
}

impl World {
    pub fn arena(&self, team: TeamId) -> &Arena {
        &self.econ.arenas[team]
    }

    /// 0..=1, 1 on top of the table. Early in the year it leans on how the
    /// club did last year.
    pub(crate) fn standing_of(&self, team: TeamId) -> f32 {
        let n = self.division_size(team);
        let s = &self.standings[team];
        let games = (s.wins + s.losses) as f32;
        let prestige = self.econ.arenas[team].prestige;
        if n < 2 {
            return prestige;
        }
        let rank = self
            .table_of(self.teams[team].division)
            .iter()
            .position(|&t| t == team)
            .unwrap_or(0);
        let now = 1.0 - rank as f32 / (n - 1) as f32;
        let w = (games / 8.0).min(1.0);
        w * now + (1.0 - w) * prestige
    }

    /// Share of the club's last five finished games that it won, 0.5 when
    /// there are none.
    pub fn form(&self, team: TeamId) -> f32 {
        let mut wins = 0;
        let mut seen = 0;
        for f in self.fixtures.iter().rev() {
            if seen == 5 {
                break;
            }
            let Some(r) = &f.result else { continue };
            let side = if f.home == team {
                0
            } else if f.away == team {
                1
            } else {
                continue;
            };
            seen += 1;
            wins += (r.score[side] > r.score[1 - side]) as u32;
        }
        if seen == 0 {
            0.5
        } else {
            wins as f32 / seen as f32
        }
    }

    /// The factors behind the crowd at fixture `i`, from the world as it is
    /// now.
    pub fn draw_for(&self, i: usize) -> Draw {
        let f = &self.fixtures[i];
        let arena = &self.econ.arenas[f.home];
        let away = &self.econ.arenas[f.away];
        let mut luck = Rng::new(
            self.econ.seed
                ^ ((self.season as u64) << 40)
                ^ ((i as u64) << 8)
                ^ (f.home as u64 * 0x1_0001),
        );
        Draw {
            capacity: arena.capacity,
            support: arena.support,
            form: self.form(f.home),
            position: self.standing_of(f.home),
            belief: self.teams[f.home].belief,
            opponent: 0.65 * self.standing_of(f.away) + 0.35 * away.support,
            condition: arena.condition,
            playoff: f.playoff,
            luck: luck.f32() * 2.0 - 1.0,
            fans: self.fan_index(f.home),
            star: self.star_pull(f.home, f.home),
            visitor: self.star_pull(f.away, f.home),
            night: self.farewell_night(i).map_or(0.0, |_| 1.0),
        }
    }

    /// People at fixture `i`: the count taken when it was played, or what
    /// the world says it would be now.
    pub fn attendance_at(&self, i: usize) -> u32 {
        match self.econ.crowds.get(i) {
            Some(&(a, _)) if a > 0 => a,
            _ => {
                let home = self.fixtures[i].home;
                self.attendance_priced(&self.draw_for(i), home, self.ticket_price(home))
            }
        }
    }

    /// The home crowd at fixture `i`, for the match to use: how full and how
    /// loud, both 0..=1. A pure function of the world; the engine does not
    /// read it yet.
    pub fn crowd_effect(&self, i: usize) -> Crowd {
        let f = &self.fixtures[i];
        let d = self.draw_for(i);
        let buzz = 0.5 * d.form + 0.5 * d.belief;
        crowd(self.attendance_at(i), d.capacity, buzz, f.playoff)
    }

    /// What the hall is likely to fill to in a normal game, for pricing
    /// the boards.
    pub fn expected_attendance(&self, team: TeamId) -> u32 {
        let a = &self.econ.arenas[team];
        let d = Draw {
            capacity: a.capacity,
            support: a.support,
            form: 0.5,
            position: a.prestige,
            belief: self.teams[team].belief,
            opponent: 0.5,
            condition: a.condition,
            playoff: false,
            luck: 0.0,
            fans: self.fan_index(team),
            star: self.star_pull(team, team),
            visitor: 0.0,
            night: 0.0,
        };
        attendance(&d)
    }
}
