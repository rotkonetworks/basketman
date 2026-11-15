//! What the user's club knows about players elsewhere.
//!
//! Attributes of other clubs' players are only known as ranges. Shortlisting
//! a player sends a scout to his games: each game watched produces a report
//! and narrows the ranges. Body measurements are public, like combine data.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::engine::BoxLine;
use crate::league::TeamId;
use crate::player::{Attr, Player, PlayerId};
use crate::signature::{KNOWN_AT, Signature};

/// How many players the scouting staff can follow at once.
pub const SHORTLIST_MAX: usize = 15;
/// What everyone knows about any player before scouting him.
const PUBLIC_KNOWLEDGE: f32 = 0.25;

/// Something a scout noted in a single game.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Observation {
    LimitedMinutes {
        minutes: u32,
    },
    TookOver {
        points: u16,
    },
    HotFromThree {
        made: u16,
        attempts: u16,
    },
    ColdFromThree {
        attempts: u16,
    },
    Efficient {
        made: u16,
        attempts: u16,
    },
    Inefficient {
        made: u16,
        attempts: u16,
    },
    GotToTheLine {
        attempts: u16,
    },
    RanTheOffense {
        assists: u16,
    },
    Careless {
        turnovers: u16,
    },
    OwnedTheGlass {
        rebounds: u16,
    },
    SecondChances {
        offensive: u16,
    },
    ProtectedTheRim {
        blocks: u16,
    },
    Disruptive {
        steals: u16,
    },
    FoulTrouble {
        fouls: u16,
    },
    Quiet,
    /// The scout saw enough of him to know one of his signature moves.
    SignatureSeen {
        signature: Signature,
    },
}

impl Observation {
    /// True for things a scout would count against the player.
    pub fn is_concern(&self) -> bool {
        matches!(
            self,
            Observation::ColdFromThree { .. }
                | Observation::Inefficient { .. }
                | Observation::Careless { .. }
                | Observation::FoulTrouble { .. }
                | Observation::Quiet
        )
    }
}

impl fmt::Display for Observation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Observation::LimitedMinutes { minutes } => {
                write!(f, "only {minutes} minutes, hard to judge")
            }
            Observation::TookOver { points } => {
                write!(f, "took the game over with {points} points")
            }
            Observation::HotFromThree { made, attempts } => {
                write!(f, "{made} of {attempts} from three")
            }
            Observation::ColdFromThree { attempts } => write!(f, "missed all {attempts} threes"),
            Observation::Efficient { made, attempts } => {
                write!(f, "efficient, {made} of {attempts} from the field")
            }
            Observation::Inefficient { made, attempts } => {
                write!(f, "forced shots, {made} of {attempts} from the field")
            }
            Observation::GotToTheLine { attempts } => {
                write!(f, "drew contact, {attempts} free throws")
            }
            Observation::RanTheOffense { assists } => {
                write!(f, "ran the offense, {assists} assists")
            }
            Observation::Careless { turnovers } => {
                write!(f, "careless with the ball, {turnovers} turnovers")
            }
            Observation::OwnedTheGlass { rebounds } => {
                write!(f, "owned the glass, {rebounds} rebounds")
            }
            Observation::SecondChances { offensive } => write!(f, "{offensive} offensive rebounds"),
            Observation::ProtectedTheRim { blocks } => {
                write!(f, "protected the rim, {blocks} blocks")
            }
            Observation::Disruptive { steals } => {
                write!(f, "disruptive on the ball, {steals} steals")
            }
            Observation::FoulTrouble { fouls } => write!(f, "in foul trouble with {fouls}"),
            Observation::Quiet => write!(f, "little impact on the game"),
            Observation::SignatureSeen { signature } => {
                write!(
                    f,
                    "has a signature move: {}",
                    signature.name().to_lowercase()
                )
            }
        }
    }
}

fn observe(l: &BoxLine, foul_limit: u16) -> Vec<Observation> {
    use Observation::*;
    let mut out = Vec::new();
    if l.secs < 8 * 60 {
        out.push(LimitedMinutes {
            minutes: l.secs / 60,
        });
        return out;
    }
    if l.pts >= 25 {
        out.push(TookOver { points: l.pts });
    }
    if l.tpa >= 4 && l.tpm * 2 >= l.tpa {
        out.push(HotFromThree {
            made: l.tpm,
            attempts: l.tpa,
        });
    } else if l.tpa >= 4 && l.tpm == 0 {
        out.push(ColdFromThree { attempts: l.tpa });
    }
    if l.fga >= 8 && l.fgm * 10 >= l.fga * 6 {
        out.push(Efficient {
            made: l.fgm,
            attempts: l.fga,
        });
    } else if l.fga >= 8 && l.fgm * 10 <= l.fga * 3 {
        out.push(Inefficient {
            made: l.fgm,
            attempts: l.fga,
        });
    }
    if l.fta >= 8 {
        out.push(GotToTheLine { attempts: l.fta });
    }
    if l.ast >= 7 {
        out.push(RanTheOffense { assists: l.ast });
    }
    if l.tov >= 4 {
        out.push(Careless { turnovers: l.tov });
    }
    if l.reb() >= 10 {
        out.push(OwnedTheGlass { rebounds: l.reb() });
    } else if l.orb >= 4 {
        out.push(SecondChances { offensive: l.orb });
    }
    if l.blk >= 3 {
        out.push(ProtectedTheRim { blocks: l.blk });
    }
    if l.stl >= 3 {
        out.push(Disruptive { steals: l.stl });
    }
    if l.pf + 1 >= foul_limit {
        out.push(FoulTrouble { fouls: l.pf });
    }
    if out.is_empty() && l.game_score() < 4.0 {
        out.push(Quiet);
    }
    out
}

/// One game of a shortlisted player, as seen by the scout.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScoutReport {
    pub player: PlayerId,
    pub round: usize,
    /// Index into `World::fixtures`.
    pub fixture: usize,
    pub opponent: TeamId,
    pub home: bool,
    pub line: BoxLine,
    /// 1..=10 match rating.
    pub rating: f32,
    pub observations: Vec<Observation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scouting {
    pub shortlist: Vec<PlayerId>,
    /// 0..=1 per player, indexed by `PlayerId`.
    knowledge: Vec<f32>,
    /// Oldest first.
    pub reports: Vec<ScoutReport>,
    /// Signature moves the club has seen or been told of: listed in a report
    /// or observed in a game. A move is also known once `knowledge` of its
    /// player reaches `signature::KNOWN_AT`; see `knows_signature`.
    #[serde(default)]
    pub seen_signatures: Vec<(PlayerId, Signature)>,
}

/// Stable pseudo-random value in -1..=1 for a (player, salt) pair, so a
/// scout's bias on an attribute does not change from frame to frame.
pub(crate) fn skew(id: PlayerId, salt: u32) -> f32 {
    let mut x = ((id as u64) << 32 | salt as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 23) as f32 - 1.0
}

impl Scouting {
    pub fn new(players: usize) -> Self {
        Scouting {
            shortlist: Vec::new(),
            knowledge: vec![PUBLIC_KNOWLEDGE; players],
            reports: Vec::new(),
            seen_signatures: Vec::new(),
        }
    }

    /// Make room for players who joined the world later.
    pub fn grow(&mut self, players: usize) {
        if self.knowledge.len() < players {
            self.knowledge.resize(players, PUBLIC_KNOWLEDGE);
        }
    }

    pub fn knowledge(&self, p: PlayerId) -> f32 {
        self.knowledge
            .get(p as usize)
            .copied()
            .unwrap_or(PUBLIC_KNOWLEDGE)
    }

    /// Is one of this player's signature moves known to the club? True once
    /// scouting knowledge of him reaches `KNOWN_AT` (the bar for character),
    /// or once it has been seen. His own club's players are always known:
    /// the caller decides who is ours.
    pub fn knows_signature(&self, p: PlayerId, s: Signature) -> bool {
        self.knowledge(p) >= KNOWN_AT || self.seen_signatures.contains(&(p, s))
    }

    /// A game showed this signature move: from now on the club knows it. The
    /// match engine calls this when a move is performed in a game the club
    /// has film of. Returns true if it was news.
    pub fn observe_signature(&mut self, p: PlayerId, s: Signature) -> bool {
        if self.knows_signature(p, s) {
            return false;
        }
        self.seen_signatures.push((p, s));
        true
    }

    /// After a scouting report on `p`: if his knowledge now passes the bar,
    /// list the signatures he has that were not yet listed, on that report.
    pub(crate) fn reveal_signatures(&mut self, p: PlayerId, signatures: &[Signature]) {
        if self.knowledge(p) < KNOWN_AT {
            return;
        }
        for &s in signatures {
            if self.seen_signatures.contains(&(p, s)) {
                continue;
            }
            self.seen_signatures.push((p, s));
            if let Some(r) = self.reports.last_mut().filter(|r| r.player == p) {
                // First in the list: it is the news.
                r.observations
                    .insert(0, Observation::SignatureSeen { signature: s });
            }
        }
    }

    pub fn is_shortlisted(&self, p: PlayerId) -> bool {
        self.shortlist.contains(&p)
    }

    /// Add or remove a player. Returns false if the shortlist is full.
    pub fn toggle(&mut self, p: PlayerId) -> bool {
        if let Some(i) = self.shortlist.iter().position(|&s| s == p) {
            self.shortlist.remove(i);
            true
        } else if self.shortlist.len() < SHORTLIST_MAX {
            self.shortlist.push(p);
            true
        } else {
            false
        }
    }

    /// Low and high estimate for a value on the 1..=20 scale. The true value
    /// is always inside; the range closes as knowledge reaches 1.
    fn range(&self, p: PlayerId, truth: f32, salt: u32) -> (f32, f32) {
        let unknown = 1.0 - self.knowledge(p);
        let center = truth + skew(p, salt) * unknown * 2.5;
        let half = unknown * 3.5;
        (
            (center - half).clamp(1.0, 20.0),
            (center + half).clamp(1.0, 20.0),
        )
    }

    pub fn estimate(&self, p: &Player, a: Attr) -> (f32, f32) {
        self.range(p.id, p.attrs[a], a as u32)
    }

    /// Overall is easier to judge than any single attribute.
    pub fn estimate_overall(&self, p: &Player) -> (f32, f32) {
        let (lo, hi) = self.range(p.id, p.overall(), 1000);
        let mid = (lo + hi) / 2.0;
        (
            p.overall().min(mid - (mid - lo) * 0.5),
            p.overall().max(mid + (hi - mid) * 0.5),
        )
    }

    pub fn estimate_potential(&self, p: &Player) -> (f32, f32) {
        self.range(p.id, p.potential, 1001)
    }

    /// Reports on one player, newest first.
    pub fn reports_for(&self, p: PlayerId) -> impl Iterator<Item = &ScoutReport> {
        self.reports.iter().rev().filter(move |r| r.player == p)
    }

    pub fn average_rating(&self, p: PlayerId) -> Option<f32> {
        let (sum, n) = self
            .reports_for(p)
            .fold((0.0, 0u32), |(s, n), r| (s + r.rating, n + 1));
        (n > 0).then_some(sum / n as f32)
    }

    /// Record a game the scout attended. Longer looks teach more.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn watch(
        &mut self,
        player: PlayerId,
        round: usize,
        fixture: usize,
        opponent: TeamId,
        home: bool,
        line: &BoxLine,
        foul_limit: u16,
        department: u8,
    ) {
        let seen = (line.secs as f32 / 1800.0).min(1.0);
        if let Some(k) = self.knowledge.get_mut(player as usize) {
            // A good scouting department learns a player in half the games
            // a poor one needs.
            let skill = 0.65 + department.min(20) as f32 * 0.035;
            *k = (*k + (0.05 + 0.09 * seen) * skill).min(1.0);
        }
        self.reports.push(ScoutReport {
            player,
            round,
            fixture,
            opponent,
            home,
            line: *line,
            rating: line.rating(),
            observations: observe(line, foul_limit),
        });
    }
}
