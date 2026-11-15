use std::ops::{Index, IndexMut};

use serde::{Deserialize, Serialize};

use crate::training::{Intensity, TrainingFocus};

/// Index into `World::players`.
pub type PlayerId = u32;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Position {
    PG,
    SG,
    SF,
    PF,
    C,
}

impl Position {
    pub const ALL: [Position; 5] = [
        Position::PG,
        Position::SG,
        Position::SF,
        Position::PF,
        Position::C,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Position::PG => "PG",
            Position::SG => "SG",
            Position::SF => "SF",
            Position::PF => "PF",
            Position::C => "C",
        }
    }
}

/// Player attributes on a 1..=20 scale (stored as f32 so training can move
/// them gradually; round for display).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Attr {
    // Technical
    CloseShot,
    MidRange,
    ThreePoint,
    FreeThrow,
    Handling,
    Passing,
    PostMoves,
    OffRebound,
    DefRebound,
    PerimeterD,
    InteriorD,
    Steal,
    Block,
    // Physical
    Speed,
    Strength,
    Vertical,
    Stamina,
    // Mental
    Iq,
    Composure,
    WorkRate,
    // Technical again: added after the others, so it keeps the last slot
    // and saves from before it can be padded.
    Footwork,
}

impl Attr {
    pub const COUNT: usize = 21;
    pub const ALL: [Attr; Attr::COUNT] = [
        Attr::CloseShot,
        Attr::MidRange,
        Attr::ThreePoint,
        Attr::FreeThrow,
        Attr::Handling,
        Attr::Passing,
        Attr::PostMoves,
        Attr::OffRebound,
        Attr::DefRebound,
        Attr::PerimeterD,
        Attr::InteriorD,
        Attr::Steal,
        Attr::Block,
        Attr::Speed,
        Attr::Strength,
        Attr::Vertical,
        Attr::Stamina,
        Attr::Iq,
        Attr::Composure,
        Attr::WorkRate,
        Attr::Footwork,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Attr::CloseShot => "Close",
            Attr::MidRange => "Mid",
            Attr::ThreePoint => "3PT",
            Attr::FreeThrow => "FT",
            Attr::Handling => "Hnd",
            Attr::Passing => "Pas",
            Attr::PostMoves => "Post",
            Attr::OffRebound => "ORb",
            Attr::DefRebound => "DRb",
            Attr::PerimeterD => "PerD",
            Attr::InteriorD => "IntD",
            Attr::Steal => "Stl",
            Attr::Block => "Blk",
            Attr::Footwork => "Foot",
            Attr::Speed => "Spd",
            Attr::Strength => "Str",
            Attr::Vertical => "Vert",
            Attr::Stamina => "Sta",
            Attr::Iq => "IQ",
            Attr::Composure => "Comp",
            Attr::WorkRate => "Work",
        }
    }

    /// Full name, for screens with room for it.
    pub fn name(self) -> &'static str {
        match self {
            Attr::CloseShot => "Close shot",
            Attr::MidRange => "Mid-range",
            Attr::ThreePoint => "Three-point",
            Attr::FreeThrow => "Free throw",
            Attr::Handling => "Ball handling",
            Attr::Passing => "Passing",
            Attr::PostMoves => "Post moves",
            Attr::OffRebound => "Offensive rebounding",
            Attr::DefRebound => "Defensive rebounding",
            Attr::PerimeterD => "Perimeter defense",
            Attr::InteriorD => "Interior defense",
            Attr::Steal => "Steal",
            Attr::Block => "Block",
            Attr::Footwork => "Footwork",
            Attr::Speed => "Speed",
            Attr::Strength => "Strength",
            Attr::Vertical => "Vertical",
            Attr::Stamina => "Stamina",
            Attr::Iq => "Basketball IQ",
            Attr::Composure => "Composure",
            Attr::WorkRate => "Work rate",
        }
    }

    pub fn is_physical(self) -> bool {
        matches!(
            self,
            Attr::Speed | Attr::Strength | Attr::Vertical | Attr::Stamina
        )
    }
}

/// A player's footwork from the rest of what he is. Follows his level
/// (the mean of his other attributes) and his speed, falls with mass and
/// height, and has plenty of spread of its own, so a nimble big man or a
/// heavy-footed guard exists but is uncommon. The spread is a pure function
/// of his id and body, so it draws nothing from the generation stream.
pub fn roll_footwork(id: PlayerId, attrs: &Attributes, body: &Body) -> f32 {
    let others: f32 = Attr::ALL
        .iter()
        .filter(|&&a| a != Attr::Footwork)
        .map(|&a| attrs[a])
        .sum();
    let level = others / (Attr::COUNT - 1) as f32;
    let salt = body.weight_kg.to_bits() ^ body.height_cm.to_bits().rotate_left(11);
    let noise =
        (crate::scouting::skew(id, salt) + crate::scouting::skew(id, salt ^ 0x5F00_7ABC)) * 0.5;
    (level + 0.35 * (attrs[Attr::Speed] - level)
        - 0.07 * (body.weight_kg - 90.0)
        - 0.03 * (body.height_cm - 200.0)
        + noise * 6.5)
        .clamp(1.0, 20.0)
}

/// Saves from before Footwork hold twenty numbers: the missing one reads as
/// zero, which no real attribute is, and `World::repair_new_fields` fills it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(from = "Vec<f32>")]
pub struct Attributes(pub [f32; Attr::COUNT]);

impl From<Vec<f32>> for Attributes {
    fn from(v: Vec<f32>) -> Self {
        let mut a = [0.0; Attr::COUNT];
        for (slot, x) in a.iter_mut().zip(v) {
            *slot = x;
        }
        Attributes(a)
    }
}

impl Index<Attr> for Attributes {
    type Output = f32;
    fn index(&self, a: Attr) -> &f32 {
        &self.0[a as usize]
    }
}

impl IndexMut<Attr> for Attributes {
    fn index_mut(&mut self, a: Attr) -> &mut f32 {
        &mut self.0[a as usize]
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hand {
    Right,
    Left,
}

/// Physical measurements as a draft combine would take them. They feed the
/// match engine (length contests shots and wins rebounds) and are enough to
/// proportion a 3D body later.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Body {
    /// Barefoot height.
    pub height_cm: f32,
    pub weight_kg: f32,
    /// Fingertip to fingertip, arms out.
    pub wingspan_cm: f32,
    /// Flat-footed, one arm up.
    pub standing_reach_cm: f32,
    pub shoulder_width_cm: f32,
    /// Wrist crease to tip of the middle finger.
    pub hand_length_cm: f32,
    pub hand: Hand,
}

impl Body {
    /// Wingspan minus height; positive means long for his size.
    pub fn ape_index_cm(&self) -> f32 {
        self.wingspan_cm - self.height_cm
    }
}

/// Hidden character traits, 1..=20. They are not skills: they decide how a
/// player trains, how steady he is from game to game and how he behaves.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Personality {
    /// How seriously he takes training: scales attribute growth.
    pub professionalism: u8,
    pub ambition: u8,
    pub determination: u8,
    /// Low values pick up needless fouls.
    pub temperament: u8,
    pub leadership: u8,
    /// Low values swing further from their level game to game.
    pub consistency: u8,
    pub loyalty: u8,
    /// How well he comes across in front of a microphone or a sponsor's
    /// guests; see `media.rs`. Coaching it (media work with a press officer)
    /// moves it.
    #[serde(default = "middling")]
    pub presence: u8,
}

/// What a save from before presence existed gets: nobody is a liability or
/// a natural until the game says so.
fn middling() -> u8 {
    10
}

/// The one-line description staff would give of a player's character.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PersonalityType {
    ModelProfessional,
    Leader,
    Driven,
    Resolute,
    Volatile,
    Casual,
    Streaky,
    Loyal,
    Mercenary,
    /// At ease in front of cameras and crowds.
    Showman,
    /// Says the wrong thing when a microphone is near.
    LooseTongue,
    Balanced,
}

impl PersonalityType {
    pub fn label(self) -> &'static str {
        match self {
            PersonalityType::ModelProfessional => "Model professional",
            PersonalityType::Leader => "Leader",
            PersonalityType::Driven => "Driven",
            PersonalityType::Resolute => "Resolute",
            PersonalityType::Volatile => "Volatile",
            PersonalityType::Casual => "Casual",
            PersonalityType::Streaky => "Streaky",
            PersonalityType::Loyal => "Loyal",
            PersonalityType::Mercenary => "Mercenary",
            PersonalityType::Showman => "Showman",
            PersonalityType::LooseTongue => "Loose tongue",
            PersonalityType::Balanced => "Balanced",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            PersonalityType::ModelProfessional => "trains hard every day and keeps his head",
            PersonalityType::Leader => "others follow him, on and off the court",
            PersonalityType::Driven => "wants to reach the top and works for it",
            PersonalityType::Resolute => "does not fold when things go against him",
            PersonalityType::Volatile => "loses his temper and picks up cheap fouls",
            PersonalityType::Casual => "coasts through training",
            PersonalityType::Streaky => "brilliant one night, invisible the next",
            PersonalityType::Loyal => "attached to his club",
            PersonalityType::Mercenary => "ambitious, and will leave for a better offer",
            PersonalityType::Showman => "comes across well and the cameras love him",
            PersonalityType::LooseTongue => "is a risk whenever a microphone is near",
            PersonalityType::Balanced => "no trait stands out",
        }
    }

    /// True for types a manager would think twice about.
    pub fn is_concern(self) -> bool {
        matches!(
            self,
            PersonalityType::Volatile
                | PersonalityType::Casual
                | PersonalityType::Streaky
                | PersonalityType::Mercenary
                | PersonalityType::LooseTongue
        )
    }
}

impl Personality {
    pub fn kind(&self) -> PersonalityType {
        if self.professionalism >= 16 && self.temperament >= 12 {
            PersonalityType::ModelProfessional
        } else if self.leadership >= 16 && self.determination >= 12 {
            PersonalityType::Leader
        } else if self.ambition >= 16 && self.determination >= 13 {
            PersonalityType::Driven
        } else if self.determination >= 16 {
            PersonalityType::Resolute
        } else if self.temperament <= 5 {
            PersonalityType::Volatile
        } else if self.professionalism <= 5 {
            PersonalityType::Casual
        } else if self.consistency <= 5 {
            PersonalityType::Streaky
        } else if self.loyalty >= 17 {
            PersonalityType::Loyal
        } else if self.ambition >= 16 && self.loyalty <= 7 {
            PersonalityType::Mercenary
        } else if self.presence >= 16 {
            PersonalityType::Showman
        } else if self.presence <= 4 {
            PersonalityType::LooseTongue
        } else {
            PersonalityType::Balanced
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Contract {
    /// Yearly salary.
    pub salary: u32,
    pub years_left: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub age: u8,
    pub body: Body,
    pub position: Position,
    pub attrs: Attributes,
    pub personality: Personality,
    /// Ceiling for `overall()`; hidden from the user behind scouting later.
    pub potential: f32,
    pub training: TrainingFocus,
    pub intensity: Intensity,
    /// Match fitness, 0.7..=1.0. Heavy training lowers it.
    pub condition: f32,
    pub contract: Contract,
    /// 0..=1 per position, indexed by `Position as usize`: his own is 1,
    /// the rest are learned by being played there.
    pub familiar: [f32; 5],
    /// 0..=1, 0.5 is neither here nor there. Winning and minutes lift it.
    pub morale: f32,
    /// He has already told the manager he is unhappy.
    #[serde(default)]
    pub complained: bool,
    /// Match sharpness, 0.5..=1: kept by playing games, lost on the bench
    /// and in the treatment room.
    pub sharp: f32,
    /// Hidden, 1..=20: how easily his body breaks.
    pub proneness: u8,
    /// Hidden, 1..=20: how fast he heals and how much of himself he gets
    /// back after a bad injury.
    pub recovery: u8,
    pub injury: Option<crate::health::Injury>,
    /// Every injury of his career, oldest first.
    pub injuries: Vec<crate::health::InjuryRecord>,
    /// Fame, media work and retirement; see `fame.rs`.
    #[serde(default)]
    pub renown: crate::fame::Renown,
    /// Innate signature moves, at most three, fixed at birth; see
    /// `signature.rs`. Nothing ever changes them.
    #[serde(default)]
    pub signatures: Vec<crate::signature::Signature>,
    /// His signatures have been decided. False only for players from saves
    /// made before signatures existed, until they are repaired on load.
    #[serde(default)]
    pub signatures_rolled: bool,
}

impl Player {
    /// Position-agnostic mean of all attributes.
    pub fn overall(&self) -> f32 {
        self.attrs.0.iter().sum::<f32>() / Attr::COUNT as f32
    }

    /// Running max vertical leap, from the Vertical attribute.
    pub fn max_vertical_cm(&self) -> f32 {
        40.0 + self.attrs[Attr::Vertical] * 2.5
    }

    /// Highest point the player can touch: standing reach plus leap.
    pub fn max_touch_cm(&self) -> f32 {
        self.body.standing_reach_cm + self.max_vertical_cm()
    }
}
