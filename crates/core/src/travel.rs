//! Travel: where the clubs are, and what a trip costs.
//!
//! Every club has a home town (`CITIES`, by club number) with coordinates in
//! tenths of a degree. The distance of a trip is integer arithmetic on them:
//! 11.1 km a tenth of a degree north-south and 6.1 km east-west (right for
//! these latitudes), then an integer square root.
//!
//! An away game involves a trip of one of three lengths:
//!
//! - short (under 150 km): the bus on the day, nothing lost;
//! - medium (150 to 499 km): the bus on the morning of the game, a little
//!   tiredness;
//! - long (500 km or more): the team travels the day before. That day's
//!   training sessions are lost (they are spent as rest, and a preparation
//!   session on it does not count), and the legs pay more.
//!
//! The tiredness is taken off the squad's match fitness (`Player::condition`)
//! when the game is drawn, so it reaches the engine the way tired legs
//! already do. A second away game in a row costs 40% more, a third 80% more;
//! a game within two days of the last costs a little extra for both clubs;
//! a home game after a home game gives the squad a little back. All of it
//! only in a managed career's calendar (`World::calendar_game`).

use serde::{Deserialize, Serialize};

use crate::league::{CITIES, TeamId, World};

/// Latitude and longitude of `CITIES`, in tenths of a degree.
const COORDS: [(i32, i32); 20] = [
    (602, 249), // Helsinki
    (615, 238), // Tampere
    (605, 223), // Turku
    (650, 255), // Oulu
    (610, 257), // Lahti
    (629, 277), // Kuopio
    (594, 247), // Tallinn
    (570, 241), // Riga
    (549, 239), // Kaunas
    (547, 253), // Vilnius
    (544, 187), // Gdansk
    (556, 130), // Malmo
    (604, 53),  // Bergen
    (562, 102), // Aarhus
    (541, 121), // Rostock
    (584, 267), // Tartu
    (602, 247), // Espoo
    (603, 250), // Vantaa
    (615, 218), // Pori
    (626, 298), // Joensuu
];

/// Trips of at least this many km are medium, long.
pub const MEDIUM_KM: u32 = 150;
pub const LONG_KM: u32 = 500;
/// Match fitness a medium and a long trip take off each player.
const MEDIUM_COST: f32 = 0.010;
const LONG_COST: f32 = 0.025;
/// Extra for a game within two days of the last.
const CLOSE_COST: f32 = 0.010;
/// What a game after a game at home gives back.
const HOME_STAND_GAIN: f32 = 0.004;

#[cfg(test)]
thread_local! {
    /// Lets a measurement play the same worlds with the toll of travel off.
    pub(crate) static TRAVEL_OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn off_for_measuring() -> bool {
    #[cfg(test)]
    {
        TRAVEL_OFF.with(|c| c.get())
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// Integer square root.
fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TripKind {
    /// Same town or close by: no trip to speak of.
    Short,
    Medium,
    /// The team leaves the day before.
    Long,
}

impl TripKind {
    pub fn of(km: u32) -> TripKind {
        if km >= LONG_KM {
            TripKind::Long
        } else if km >= MEDIUM_KM {
            TripKind::Medium
        } else {
            TripKind::Short
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TripKind::Short => "short trip",
            TripKind::Medium => "bus on the morning of the game",
            TripKind::Long => "long trip, we leave the day before",
        }
    }
}

/// Where a club has been lately.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TeamTravel {
    /// Day of the club's last game this season.
    pub last_day: Option<u32>,
    /// Games in a row away, and at home, up to now.
    pub away_run: u8,
    pub home_run: u8,
    /// Weekday (0 = Monday) of this week's training lost to a long trip.
    pub lost_day: Option<u8>,
}

/// Distance in km between two clubs' towns.
pub fn distance_km(a: TeamId, b: TeamId) -> u32 {
    let (la, oa) = COORDS[a % CITIES.len()];
    let (lb, ob) = COORDS[b % CITIES.len()];
    let dy = (la - lb).unsigned_abs() as u64 * 111 / 10;
    let dx = (oa - ob).unsigned_abs() as u64 * 61 / 10;
    isqrt(dx * dx + dy * dy) as u32
}

impl World {
    /// The town a club plays in.
    pub fn city_of(&self, team: TeamId) -> &'static str {
        CITIES[team % CITIES.len()]
    }

    /// km the away club travels to play at `home`.
    pub fn trip_km(&self, away: TeamId, home: TeamId) -> u32 {
        distance_km(away, home)
    }

    /// Weekday (0 = Monday) of the training lost to travel before a game
    /// of `team` on `day`, if its trip is a long one away.
    pub(crate) fn travel_day_lost(&self, team: TeamId, day: u32) -> Option<usize> {
        if !self.calendar_game() {
            return None;
        }
        let f = self
            .fixtures
            .iter()
            .find(|f| f.day == day && f.away == team && !f.playoff)?;
        if TripKind::of(distance_km(f.away, f.home)) != TripKind::Long || day == 0 {
            return None;
        }
        let before = (day - 1) % 7;
        (before < 5).then_some(before as usize)
    }

    /// What the trip costs a squad: a share of match fitness.
    fn trip_cost(km: u32, away_run: u8) -> f32 {
        let base = match TripKind::of(km) {
            TripKind::Short => 0.0,
            TripKind::Medium => MEDIUM_COST,
            TripKind::Long => LONG_COST,
        };
        base * (1.0 + 0.4 * away_run.min(2) as f32)
    }

    /// The games of a block are about to be played: the away clubs travel,
    /// the home clubs are at home. Takes the toll on match fitness.
    pub(crate) fn travel_for(&mut self, played: &[usize]) {
        if !self.calendar_game() || off_for_measuring() {
            return;
        }
        if self.travel.len() < self.teams.len() {
            self.travel.resize(self.teams.len(), TeamTravel::default());
        }
        for &i in played {
            let f = &self.fixtures[i];
            let (home, away, day) = (f.home, f.away, f.day);
            let km = distance_km(away, home);
            for (team, is_home) in [(home, true), (away, false)] {
                let log = self.travel[team].clone();
                let mut cost = 0.0f32;
                if !is_home {
                    cost += Self::trip_cost(km, log.away_run);
                }
                if log.last_day.is_some_and(|d| day.saturating_sub(d) <= 2) {
                    cost += CLOSE_COST;
                }
                let gain = if is_home && log.home_run >= 1 { HOME_STAND_GAIN } else { 0.0 };
                if cost != gain {
                    for &p in &self.teams[team].roster {
                        let pl = &mut self.players[p as usize];
                        pl.condition = (pl.condition - cost + gain).clamp(0.7, 1.0);
                    }
                }
                let t = &mut self.travel[team];
                t.last_day = Some(day);
                if is_home {
                    t.home_run = t.home_run.saturating_add(1);
                    t.away_run = 0;
                } else {
                    t.away_run = t.away_run.saturating_add(1);
                    t.home_run = 0;
                    if TripKind::of(km) == TripKind::Long && day > 0 && (day - 1) % 7 < 5 {
                        t.lost_day = Some(((day - 1) % 7) as u8);
                    }
                }
            }
        }
    }

    /// The training week a squad actually had: a day lost to a long trip
    /// is spent resting.
    pub(crate) fn week_schedule(&mut self, team: TeamId) -> crate::training::Schedule {
        let mut s = self.teams[team].schedule.clone();
        if let Some(t) = self.travel.get_mut(team)
            && let Some(d) = t.lost_day.take()
        {
            for slot in [d as usize * 2, d as usize * 2 + 1] {
                s.sessions[slot] = crate::training::Session::Rest;
            }
        }
        s
    }
}
