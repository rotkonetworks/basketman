//! Training has two layers, as in Football Manager: a weekly team schedule
//! of sessions everyone attends, and an individual focus per player on top.

use serde::{Deserialize, Serialize};

use crate::player::{Attr, Player};
use crate::rng::Rng;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrainingFocus {
    Balanced,
    Shooting,
    Finishing,
    Playmaking,
    Rebounding,
    PerimeterDefense,
    InteriorDefense,
    Athleticism,
    Conditioning,
    Mental,
}

impl TrainingFocus {
    pub const ALL: [TrainingFocus; 10] = [
        TrainingFocus::Balanced,
        TrainingFocus::Shooting,
        TrainingFocus::Finishing,
        TrainingFocus::Playmaking,
        TrainingFocus::Rebounding,
        TrainingFocus::PerimeterDefense,
        TrainingFocus::InteriorDefense,
        TrainingFocus::Athleticism,
        TrainingFocus::Conditioning,
        TrainingFocus::Mental,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TrainingFocus::Balanced => "Balanced",
            TrainingFocus::Shooting => "Shooting",
            TrainingFocus::Finishing => "Finishing",
            TrainingFocus::Playmaking => "Playmaking",
            TrainingFocus::Rebounding => "Rebounding",
            TrainingFocus::PerimeterDefense => "Perimeter defense",
            TrainingFocus::InteriorDefense => "Interior defense",
            TrainingFocus::Athleticism => "Athleticism",
            TrainingFocus::Conditioning => "Conditioning",
            TrainingFocus::Mental => "Mental",
        }
    }

    /// Attributes that get the focused growth rate. Empty for `Balanced`.
    pub fn attrs(self) -> &'static [Attr] {
        match self {
            TrainingFocus::Balanced => &[],
            TrainingFocus::Shooting => &[Attr::MidRange, Attr::ThreePoint, Attr::FreeThrow],
            TrainingFocus::Finishing => &[Attr::CloseShot, Attr::PostMoves, Attr::Footwork],
            TrainingFocus::Playmaking => &[Attr::Handling, Attr::Passing],
            TrainingFocus::Rebounding => &[Attr::OffRebound, Attr::DefRebound],
            TrainingFocus::PerimeterDefense => &[Attr::PerimeterD, Attr::Steal, Attr::Footwork],
            TrainingFocus::InteriorDefense => &[Attr::InteriorD, Attr::Block],
            TrainingFocus::Athleticism => &[Attr::Speed, Attr::Strength, Attr::Vertical],
            TrainingFocus::Conditioning => &[Attr::Stamina],
            TrainingFocus::Mental => &[Attr::Iq, Attr::Composure],
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Intensity {
    Light,
    Normal,
    Heavy,
}

impl Intensity {
    pub const ALL: [Intensity; 3] = [Intensity::Light, Intensity::Normal, Intensity::Heavy];

    pub fn label(self) -> &'static str {
        match self {
            Intensity::Light => "Light",
            Intensity::Normal => "Normal",
            Intensity::Heavy => "Heavy",
        }
    }

    fn gain(self) -> f32 {
        match self {
            Intensity::Light => 0.6,
            Intensity::Normal => 1.0,
            Intensity::Heavy => 1.4,
        }
    }

    fn condition_delta(self) -> f32 {
        match self {
            Intensity::Light => 0.02,
            Intensity::Normal => 0.0,
            Intensity::Heavy => -0.03,
        }
    }
}

/// One team session. A week has ten: morning and afternoon, Monday to
/// Friday. The weekend is match day and a day off.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Session {
    Rest,
    Recovery,
    Shooting,
    Finishing,
    Playmaking,
    Rebounding,
    PerimeterDefense,
    InteriorDefense,
    Athleticism,
    Conditioning,
    Tactics,
    Scrimmage,
    FilmStudy,
    /// Work against the next opponent's system, from the scout's report:
    /// helps in that one game and only as far as the report is right. It
    /// builds nothing else.
    MatchPrep,
}

impl Session {
    pub const ALL: [Session; 14] = [
        Session::Rest,
        Session::Recovery,
        Session::Shooting,
        Session::Finishing,
        Session::Playmaking,
        Session::Rebounding,
        Session::PerimeterDefense,
        Session::InteriorDefense,
        Session::Athleticism,
        Session::Conditioning,
        Session::Tactics,
        Session::Scrimmage,
        Session::FilmStudy,
        Session::MatchPrep,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Session::Rest => "Rest",
            Session::Recovery => "Recovery",
            Session::Shooting => "Shooting",
            Session::Finishing => "Finishing",
            Session::Playmaking => "Playmaking",
            Session::Rebounding => "Rebounding",
            Session::PerimeterDefense => "Perimeter defense",
            Session::InteriorDefense => "Interior defense",
            Session::Athleticism => "Athleticism",
            Session::Conditioning => "Conditioning",
            Session::Tactics => "Tactics",
            Session::Scrimmage => "Scrimmage",
            Session::FilmStudy => "Film study",
            Session::MatchPrep => "Match preparation",
        }
    }

    /// What the session is for, in a few words.
    pub fn purpose(self) -> &'static str {
        match self {
            Session::Rest => "fitness back, nothing learned",
            Session::Recovery => "light work, some fitness back",
            Session::Shooting => "mid-range, threes, free throws",
            Session::Finishing => "close shots, post moves and post footwork",
            Session::Playmaking => "handling and passing",
            Session::Rebounding => "both boards",
            Session::PerimeterDefense => "on-ball defense, steals and footwork",
            Session::InteriorDefense => "paint defense and blocks",
            Session::Athleticism => "speed, strength, leaping",
            Session::Conditioning => "stamina, at a fitness cost",
            Session::Tactics => "tactic familiarity",
            Session::Scrimmage => "a bit of everything, some familiarity",
            Session::FilmStudy => "IQ, composure, some familiarity",
            Session::MatchPrep => "the next opponent's plays, from the scout's report; nothing else grows",
        }
    }

    /// Attributes the session develops directly.
    pub fn attrs(self) -> &'static [Attr] {
        match self {
            Session::Shooting => TrainingFocus::Shooting.attrs(),
            Session::Finishing => TrainingFocus::Finishing.attrs(),
            Session::Playmaking => TrainingFocus::Playmaking.attrs(),
            Session::Rebounding => TrainingFocus::Rebounding.attrs(),
            Session::PerimeterDefense => TrainingFocus::PerimeterDefense.attrs(),
            Session::InteriorDefense => TrainingFocus::InteriorDefense.attrs(),
            Session::Athleticism => TrainingFocus::Athleticism.attrs(),
            Session::Conditioning => TrainingFocus::Conditioning.attrs(),
            Session::FilmStudy => TrainingFocus::Mental.attrs(),
            Session::Rest
            | Session::Recovery
            | Session::Tactics
            | Session::Scrimmage
            | Session::MatchPrep => &[],
        }
    }

    /// Change in match fitness from one session.
    pub fn condition_delta(self) -> f32 {
        match self {
            Session::Rest => 0.03,
            Session::Recovery => 0.015,
            Session::FilmStudy => 0.0,
            Session::Tactics | Session::MatchPrep => -0.004,
            Session::Scrimmage | Session::Athleticism => -0.010,
            Session::Conditioning => -0.012,
            _ => -0.006,
        }
    }

    /// Tactic familiarity gained from one session, before it is split
    /// across the playbook.
    pub fn familiarity(self) -> f32 {
        match self {
            Session::Tactics => 0.05,
            Session::Scrimmage => 0.025,
            Session::FilmStudy => 0.015,
            _ => 0.0,
        }
    }
}

pub const DAYS: [&str; 5] = ["Mon", "Tue", "Wed", "Thu", "Fri"];
pub const SESSIONS_PER_WEEK: usize = 10;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Preset {
    Balanced,
    Offense,
    Defense,
    Physical,
    Tactical,
    Light,
}

impl Preset {
    pub const ALL: [Preset; 6] = [
        Preset::Balanced,
        Preset::Offense,
        Preset::Defense,
        Preset::Physical,
        Preset::Tactical,
        Preset::Light,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Preset::Balanced => "Balanced",
            Preset::Offense => "Offense",
            Preset::Defense => "Defense",
            Preset::Physical => "Physical",
            Preset::Tactical => "Tactical",
            Preset::Light => "Light week",
        }
    }
}

/// The team's training week: `sessions[day * 2]` is the morning,
/// `sessions[day * 2 + 1]` the afternoon.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    pub sessions: [Session; SESSIONS_PER_WEEK],
}

/// Fitness everyone gets back over the weekend.
const WEEKEND_RECOVERY: f32 = 0.04;

impl Schedule {
    pub fn preset(p: Preset) -> Self {
        use Session::*;
        let sessions = match p {
            Preset::Balanced => [
                Shooting,
                Tactics,
                PerimeterDefense,
                Playmaking,
                Conditioning,
                Rest,
                InteriorDefense,
                Tactics,
                Scrimmage,
                FilmStudy,
            ],
            Preset::Offense => [
                Shooting, Tactics, Finishing, Playmaking, Shooting, Rest, Playmaking, Tactics,
                Scrimmage, FilmStudy,
            ],
            Preset::Defense => [
                PerimeterDefense,
                Tactics,
                InteriorDefense,
                Rebounding,
                PerimeterDefense,
                Rest,
                Rebounding,
                Tactics,
                Scrimmage,
                FilmStudy,
            ],
            Preset::Physical => [
                Athleticism,
                Conditioning,
                Shooting,
                Rest,
                Athleticism,
                Conditioning,
                Rebounding,
                Tactics,
                Scrimmage,
                Recovery,
            ],
            Preset::Tactical => [
                Tactics, FilmStudy, Tactics, Scrimmage, Shooting, Rest, Tactics, FilmStudy,
                Scrimmage, Tactics,
            ],
            Preset::Light => [
                Shooting, Rest, Recovery, Tactics, Rest, Rest, Playmaking, Recovery, FilmStudy,
                Rest,
            ],
        };
        Schedule { sessions }
    }

    pub fn count(&self, s: Session) -> usize {
        self.sessions.iter().filter(|&&x| x == s).count()
    }

    /// Weekly tactic familiarity to share across the playbook.
    pub fn familiarity_gain(&self) -> f32 {
        self.sessions.iter().map(|s| s.familiarity()).sum()
    }

    /// Weekly change in match fitness for a player on normal intensity.
    pub fn condition_delta(&self) -> f32 {
        WEEKEND_RECOVERY
            + self
                .sessions
                .iter()
                .map(|s| s.condition_delta())
                .sum::<f32>()
    }

    /// How many sessions' worth of work an attribute gets this week.
    /// Scrimmages count a little toward everything.
    pub fn work(&self, a: Attr) -> f32 {
        self.sessions
            .iter()
            .map(|s| {
                if s.attrs().contains(&a) {
                    1.0
                } else if *s == Session::Scrimmage {
                    0.3
                } else {
                    0.0
                }
            })
            .sum()
    }
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule::preset(Preset::Balanced)
    }
}

/// Weekly growth from one team session touching an attribute.
const SESSION_RATE: f32 = 0.012;
/// Extra weekly growth on the attributes of a player's individual focus.
const FOCUS_RATE: f32 = 0.035;
/// Extra weekly growth on every attribute for a balanced individual plan.
const BALANCED_RATE: f32 = 0.008;

/// One week of training for one player: the team schedule plus his own
/// focus, scaled by age, room to grow, work rate and professionalism, then
/// physical decline for veterans and the week's fitness cost.
///
/// `teaching` multiplies what he learns and `wear` what it costs his legs:
/// the club's skills and fitness coaches, 1.0 for an ordinary one.
pub fn train_week(p: &mut Player, schedule: &Schedule, rng: &mut Rng, teaching: f32, wear: f32) {
    let age_factor = match p.age {
        ..=20 => 1.0,
        21..=23 => 0.75,
        24..=26 => 0.45,
        27..=29 => 0.2,
        _ => 0.05,
    };
    let headroom = ((p.potential - p.overall()) / 4.0).clamp(0.0, 1.0);
    let attitude = 0.55
        + p.attrs[Attr::WorkRate] / 20.0 * 0.45
        + p.personality.professionalism as f32 / 20.0 * 0.5;
    let scale = age_factor * headroom * attitude;
    let focus = p.training.attrs();

    for a in Attr::ALL {
        if a == Attr::WorkRate {
            continue;
        }
        let individual = if focus.is_empty() {
            BALANCED_RATE
        } else if focus.contains(&a) {
            FOCUS_RATE
        } else {
            0.0
        };
        let rate = SESSION_RATE * schedule.work(a) + individual * p.intensity.gain();
        let mut delta = rate * scale * (0.5 + rng.f32()) * teaching;
        if p.age > 30 && a.is_physical() {
            let slowed = if p.training == TrainingFocus::Conditioning {
                0.5
            } else {
                1.0
            };
            delta -= 0.015 * (p.age - 30) as f32 * slowed * wear;
        }
        p.attrs[a] = (p.attrs[a] + delta).clamp(1.0, 20.0);
    }

    // A good fitness coach gets more recovery out of a light week and
    // loses less to a heavy one.
    let week = schedule.condition_delta() + p.intensity.condition_delta();
    let week = if week < 0.0 {
        week * wear
    } else {
        week * (2.0 - wear)
    };
    p.condition = (p.condition + week).clamp(0.7, 1.0);
}
