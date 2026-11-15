//! Bodies and heads: injuries and what they leave behind, tired legs,
//! morale, and learning to play somewhere other than where a player grew up.
//!
//! All of it happens between games, from the box scores, on the world's own
//! random stream, so a match plays the same live or simulated.

use serde::{Deserialize, Serialize};

use crate::engine::GameResult;
use crate::inbox::Kind;
use crate::league::{TeamId, World};
use crate::player::{Attr, Player, PlayerId, Position};
use crate::rng::Rng;
use crate::tactics::RotationSlot;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ailment {
    AnkleSprain,
    BackSpasm,
    Concussion,
    Hamstring,
    GroinStrain,
    KneeSprain,
    BrokenHand,
    StressFracture,
    MeniscusTear,
    CruciateLigament,
    Achilles,
}

impl Ailment {
    /// How often each happens, and weeks out at the short and long end.
    const TABLE: [(Ailment, f32, u8, u8); 11] = [
        (Ailment::AnkleSprain, 30.0, 1, 3),
        (Ailment::BackSpasm, 12.0, 1, 2),
        (Ailment::Concussion, 6.0, 1, 2),
        (Ailment::Hamstring, 16.0, 2, 4),
        (Ailment::GroinStrain, 10.0, 1, 4),
        (Ailment::KneeSprain, 10.0, 3, 7),
        (Ailment::BrokenHand, 6.0, 4, 8),
        (Ailment::StressFracture, 4.0, 6, 12),
        (Ailment::MeniscusTear, 3.0, 8, 16),
        (Ailment::CruciateLigament, 2.0, 28, 40),
        (Ailment::Achilles, 1.0, 30, 44),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Ailment::AnkleSprain => "sprained ankle",
            Ailment::BackSpasm => "back spasms",
            Ailment::Concussion => "concussion",
            Ailment::Hamstring => "hamstring strain",
            Ailment::GroinStrain => "groin strain",
            Ailment::KneeSprain => "sprained knee",
            Ailment::BrokenHand => "broken hand",
            Ailment::StressFracture => "stress fracture in the foot",
            Ailment::MeniscusTear => "torn meniscus",
            Ailment::CruciateLigament => "torn cruciate ligament",
            Ailment::Achilles => "ruptured Achilles tendon",
        }
    }
}

/// Weeks out at which an injury counts as serious: it can cost a player
/// some of his body for good.
pub const SERIOUS: u8 = 8;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Injury {
    pub what: Ailment,
    pub weeks_left: u8,
}

/// One line of a medical record.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InjuryRecord {
    pub season: u16,
    pub age: u8,
    pub what: Ailment,
    pub weeks: u8,
    /// Mean physical attribute lost for good.
    pub lost: f32,
}

impl Player {
    pub fn fit(&self) -> bool {
        self.injury.is_none()
    }

    /// 0..=1, how well he knows a position.
    pub fn knows(&self, pos: Position) -> f32 {
        self.familiar[pos as usize]
    }

    /// Positions he can be trusted at besides his own.
    pub fn also_plays(&self) -> Vec<Position> {
        Position::ALL
            .into_iter()
            .filter(|&p| p != self.position && self.knows(p) >= 0.75)
            .collect()
    }

    /// Weeks lost to injury over his career.
    pub fn weeks_lost(&self) -> u32 {
        self.injuries.iter().map(|r| r.weeks as u32).sum()
    }

    /// The word for a morale level.
    pub fn mood(&self) -> &'static str {
        match self.morale {
            m if m >= 0.8 => "Delighted",
            m if m >= 0.62 => "Happy",
            m if m >= 0.42 => "Content",
            m if m >= 0.28 => "Unsettled",
            _ => "Unhappy",
        }
    }
}

/// What the medical staff would say about a body, from the two hidden
/// ratings. `sure` is how well he is known, 0..=1: below 0.5 there is only
/// the record to go on.
pub fn medical(p: &Player, sure: f32) -> String {
    if sure < 0.5 {
        return "Not seen enough to judge his body; go by the record.".into();
    }
    let prone = match p.proneness {
        ..=5 => "Rarely gets hurt",
        6..=10 => "No more fragile than most",
        11..=14 => "Picks up knocks",
        _ => "Injury prone",
    };
    let back = match p.recovery {
        ..=6 => "slow to come back and seldom the same after a bad one",
        7..=12 => "comes back from injury like most do",
        _ => "heals fast and comes back as good as he was",
    };
    format!("{prone}; {back}.")
}

/// How well each position is known by a player born to `natural`: his own
/// fully, the ones next to it partly, the rest hardly.
pub(crate) fn familiarity(natural: Position, rng: &mut Rng) -> [f32; 5] {
    let mut out = [0.0; 5];
    for pos in Position::ALL {
        let gap = (pos as i32 - natural as i32).unsigned_abs();
        out[pos as usize] = match gap {
            0 => 1.0,
            1 => 0.3 + rng.f32() * 0.45,
            2 => 0.05 + rng.f32() * 0.2,
            _ => 0.0,
        };
    }
    out
}

impl World {
    /// Everything a played game does to the people in it: tired legs,
    /// injuries, and how they feel about the result and their minutes.
    pub(crate) fn after_game(&mut self, fixture: usize, result: &GameResult) {
        let (home, away) = (self.fixtures[fixture].home, self.fixtures[fixture].away);
        let regulation = (self.rules.periods as u32 * self.rules.period_secs) as f32;
        for (side, team) in [home, away].into_iter().enumerate() {
            let won = result.score[side] > result.score[1 - side];
            let margin = result.score[side] as f32 - result.score[1 - side] as f32;
            // Minutes each man has a right to expect, by standing in the squad.
            let mut order: Vec<PlayerId> = self.teams[team]
                .roster
                .iter()
                .copied()
                .filter(|&p| self.players[p as usize].fit())
                .collect();
            order.sort_by(|&a, &b| {
                self.players[b as usize]
                    .overall()
                    .total_cmp(&self.players[a as usize].overall())
            });
            for (rank, &pid) in order.iter().enumerate() {
                let secs = result.boxes[side]
                    .iter()
                    .find(|(id, _)| *id == pid)
                    .map_or(0, |(_, l)| l.secs) as f32;
                let minutes = secs / 60.0;
                let share = secs / regulation;
                let p = &mut self.players[pid as usize];

                // Head: the result, then his minutes against what he expects.
                let expects = match rank {
                    0..=2 => 26.0,
                    3..=4 => 20.0,
                    5..=7 => 10.0,
                    _ => 0.0,
                };
                let mut feel =
                    if won { 0.035 } else { -0.03 } + (margin / 400.0).clamp(-0.03, 0.03);
                if expects > 0.0 {
                    let wants = 0.6 + p.personality.ambition as f32 / 20.0 * 0.8;
                    let calm = 1.3 - p.personality.professionalism as f32 / 20.0 * 0.6;
                    if minutes < expects * 0.6 {
                        feel -= 0.05 * wants * calm * (1.0 - minutes / (expects * 0.6));
                    } else if minutes >= expects {
                        feel += 0.015;
                    }
                } else if minutes >= 8.0 {
                    // A fringe player given a run.
                    feel += 0.03;
                }
                p.morale = (p.morale + feel).clamp(0.0, 1.0);

                self.wear_and_tear(team, pid, minutes, share, 1.0);
            }
        }
    }

    /// What a game does to one body: tired legs, match sharpness, and the
    /// chance that something goes. `load` scales the cost and the risk;
    /// a reserve game is gentler than the real thing.
    pub(crate) fn wear_and_tear(
        &mut self,
        team: TeamId,
        pid: PlayerId,
        minutes: f32,
        share: f32,
        load: f32,
    ) {
        if minutes <= 0.0 {
            return;
        }
        let wear = self.wear_at(team);
        let p = &mut self.players[pid as usize];
        // A full game takes more out of a man with no stamina.
        let cost = 0.075 * share * (1.35 - p.attrs[Attr::Stamina] / 20.0 * 0.7) * wear * load;
        p.condition = (p.condition - cost).clamp(0.7, 1.0);
        p.sharp = (p.sharp + 0.03 + 0.14 * share).min(1.0);
        // The longer he plays, the more tired, the older and the more
        // fragile, the likelier something goes.
        let age = 1.0 + (p.age.saturating_sub(29)) as f32 * 0.07;
        let tired = 1.0 + (1.0 - p.condition) * 3.0;
        let chance = 0.012
            * (minutes / 30.0)
            * (0.35 + p.proneness as f32 / 10.0)
            * age
            * tired
            * wear
            * load;
        if self.rng.chance(chance) {
            self.injure(team, pid, "in the game");
        }
    }

    /// Hurt a player: pick what, work out how long, take what a bad one
    /// takes for good, write it in his record and get him out of the team.
    fn injure(&mut self, team: TeamId, pid: PlayerId, when: &str) {
        let weights: Vec<f32> = Ailment::TABLE.iter().map(|r| r.1).collect();
        let (what, _, lo, hi) = Ailment::TABLE[self.rng.weighted(&weights)];
        let wear = self.wear_at(team);
        let recovery = self.facilities(team).recovery_mult();
        let roll = self.rng.f32();
        let harm = self.rng.f32();
        let season = self.season;
        let p = &mut self.players[pid as usize];
        // A good healer and a good fitness coach both shorten it.
        let heal = (1.3 - p.recovery as f32 * 0.03) * (0.8 + 0.2 * wear) * recovery;
        let weeks = ((lo as f32 + (hi - lo) as f32 * roll) * heal + 0.5).max(1.0) as u8;
        let mut lost = 0.0;
        if weeks >= SERIOUS {
            // The long ones cost speed and spring, more for those who do
            // not come back well, and leave the body likelier to go again.
            lost = (0.25 + harm * 1.1) * (1.15 - p.recovery as f32 / 20.0) * weeks.min(40) as f32
                / 30.0;
            for a in Attr::ALL {
                if a.is_physical() {
                    p.attrs[a] = (p.attrs[a] - lost).max(1.0);
                }
            }
            p.proneness = (p.proneness + 1 + (p.recovery < 8) as u8).min(20);
        }
        p.injury = Some(Injury {
            what,
            weeks_left: weeks,
        });
        p.injuries.push(InjuryRecord {
            season,
            age: p.age,
            what,
            weeks,
            lost,
        });
        let name = p.name.clone();
        let cover = self.sideline(team, pid);
        if self.watch == Some(team) {
            let mut body = format!(
                "{name} went down {when}: {}. He is out for about {weeks} week{}.",
                what.label(),
                if weeks == 1 { "" } else { "s" }
            );
            if lost > 0.0 {
                body += " The doctors do not expect him to be quite the athlete he was.";
            }
            body += &match cover {
                Some(c) => format!(
                    "\n\n{} takes his place in the rotation until you decide otherwise.",
                    self.players[c as usize].name
                ),
                None => "\n\nNobody was free to take his place in the rotation.".into(),
            };
            let mut players = vec![pid];
            players.extend(cover);
            self.post(
                self.day,
                Kind::Training,
                "Fitness coach",
                format!("{name} injured, out {weeks} wk"),
                body,
                players,
            );
        }
    }

    /// Take a player out of every rotation of his club and put the best
    /// fit man not in it in his place. Returns who came in.
    pub(crate) fn sideline(&mut self, team: TeamId, pid: PlayerId) -> Option<PlayerId> {
        let pos = self.players[pid as usize].position;
        let mut came = None;
        for i in 0..self.teams[team].playbook.len() {
            let rotation = &self.teams[team].playbook[i].tactics.rotation;
            let Some(k) = rotation.iter().position(|s| s.player == pid) else {
                continue;
            };
            let cover = self.teams[team]
                .roster
                .iter()
                .copied()
                .filter(|&q| {
                    self.players[q as usize].fit() && !rotation.iter().any(|s| s.player == q)
                })
                .max_by(|&a, &b| {
                    let score = |q: PlayerId| {
                        let p = &self.players[q as usize];
                        p.overall() + p.knows(pos) * 2.0
                    };
                    score(a).total_cmp(&score(b))
                });
            let rotation = &mut self.teams[team].playbook[i].tactics.rotation;
            match cover {
                Some(q) => {
                    rotation[k] = RotationSlot {
                        player: q,
                        position: None,
                        ..rotation[k].clone()
                    };
                    came = came.or(Some(q));
                }
                None if rotation.len() > 5 => {
                    rotation.remove(k);
                }
                // Five and no cover: he plays hurt.
                None => {}
            }
        }
        came
    }

    /// The week between games: injuries heal, training can hurt, positions
    /// are learned, moods settle.
    pub(crate) fn health_week(&mut self) {
        let me = self.watch;
        for team in 0..self.teams.len() {
            let wear = self.wear_at(team);
            let mut back = Vec::new();
            let squad: Vec<PlayerId> = self.teams[team]
                .roster
                .iter()
                .chain(&self.teams[team].reserves)
                .copied()
                .collect();
            for pid in squad {
                let p = &mut self.players[pid as usize];
                // Moods drift back to the middle, and an edge that is not
                // used goes dull.
                p.morale += (0.5 - p.morale) * 0.04;
                p.sharp = (p.sharp - 0.03).max(0.5);
                match &mut p.injury {
                    Some(inj) => {
                        inj.weeks_left = inj.weeks_left.saturating_sub(1);
                        if inj.weeks_left == 0 {
                            // Back, but not sharp: how far off depends on
                            // how long he was out and how he recovers.
                            let out = p.injuries.last().map_or(1, |r| r.weeks) as f32;
                            let rust = (out * 0.012).min(0.22) * (1.4 - p.recovery as f32 / 20.0);
                            p.condition = (0.97 - rust).clamp(0.7, 1.0);
                            p.sharp = p.sharp.min(1.0 - out * 0.04).max(0.5);
                            p.injury = None;
                            back.push(pid);
                        }
                    }
                    None => {
                        let load = match p.intensity {
                            crate::training::Intensity::Heavy => 2.2,
                            crate::training::Intensity::Normal => 1.0,
                            crate::training::Intensity::Light => 0.5,
                        };
                        let chance = 0.0022
                            * load
                            * (0.35 + p.proneness as f32 / 10.0)
                            * (1.0 + (1.0 - p.condition) * 3.0)
                            * wear;
                        if self.rng.chance(chance) {
                            self.injure(team, pid, "in training");
                        }
                    }
                }
            }
            if me == Some(team) {
                for pid in back {
                    let p = &self.players[pid as usize];
                    let (name, cond, sharp) =
                        (p.name.clone(), p.condition * 100.0, p.sharp * 100.0);
                    let first = self.teams[team].roster.contains(&pid);
                    if first && sharp < 85.0 {
                        // Straight to the reserves to get games into him.
                        self.set_down(team, pid, true);
                    }
                    self.post(
                        self.day,
                        Kind::Training,
                        "Fitness coach",
                        format!("{name} is fit again"),
                        format!(
                            "{name} has been passed fit: {cond:.0}% condition, {sharp:.0}% match \
                             sharpness. {} He is not back in the rotation until you put him there.",
                            if first && sharp < 85.0 {
                                "He is short of games, so he has been put down for the reserves: \
                                 while he is out of the first-team rotation he plays for them, and \
                                 each game brings him closer to his level."
                            } else {
                                ""
                            }
                        ),
                        vec![pid],
                    );
                }
            } else if !back.is_empty() {
                // Other clubs' coaches put their rotation back together.
                self.refit(team);
            }

            // Learning a position: a week's work in the system at it, more
            // for the minutes he is given there, for the young and bright.
            let active = self.teams[team].active;
            let slots = self.teams[team].playbook[active].tactics.rotation.clone();
            let drill = self.teams[team].office.drilling();
            for s in slots {
                let Some(pos) = s.position else { continue };
                let p = &mut self.players[s.player as usize];
                if !p.fit() || p.knows(pos) >= 1.0 {
                    continue;
                }
                let young = match p.age {
                    ..=22 => 1.3,
                    23..=26 => 1.0,
                    27..=30 => 0.7,
                    _ => 0.45,
                };
                let bright = 0.6 + p.attrs[Attr::Iq] / 20.0 * 0.8;
                let gain =
                    (0.012 + 0.02 * s.minutes.min(36) as f32 / 30.0) * young * bright * drill;
                p.familiar[pos as usize] = (p.familiar[pos as usize] + gain).min(1.0);
            }
        }

        // Somebody who has had enough says so, once.
        if let Some(me) = me {
            for pid in self.teams[me].roster.clone() {
                let p = &mut self.players[pid as usize];
                if p.morale < 0.25 && !p.complained {
                    p.complained = true;
                    let name = p.name.clone();
                    self.post(
                        self.day,
                        Kind::Board,
                        &name.clone(),
                        format!("{name} is unhappy"),
                        format!(
                            "{name} has asked to see you. He is not happy with how things are \
                             going for him here: results, and above all the minutes he is \
                             getting. An unhappy player trains and plays below himself."
                        ),
                        vec![pid],
                    );
                } else if p.morale > 0.45 {
                    p.complained = false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A few seasons of a whole league: people get hurt at a believable
    /// rate, nobody plays while injured, legs and moods stay in range.
    #[test]
    fn a_league_gets_hurt_and_carries_on() {
        let mut w = World::generate(11, 8);
        let mut games = 0;
        let (mut cond, mut n) = (0.0, 0);
        {
            while !w.season_over() {
                for i in w.play_round() {
                    games += 1;
                    let f = &w.fixtures[i];
                    for team in [f.home, f.away] {
                        let fit = w.teams[team]
                            .roster
                            .iter()
                            .filter(|&&p| w.players[p as usize].fit())
                            .count();
                        assert!(fit >= 5, "club {team} has {fit} fit players");
                    }
                }
                for t in &w.teams {
                    for s in &t.tactics().rotation {
                        let p = &w.players[s.player as usize];
                        assert!(p.fit() || t.roster.len() <= 5, "{} plays hurt", p.name);
                        cond += p.condition;
                        n += 1;
                    }
                }
            }
        }
        let hurt: usize = w.players.iter().map(|p| p.injuries.len()).sum();
        let per_team_season = hurt as f32 / 8.0;
        let morale: Vec<f32> = w
            .teams
            .iter()
            .flat_map(|t| t.roster.iter().map(|&p| w.players[p as usize].morale))
            .collect();
        let (lo, hi) = morale
            .iter()
            .fold((1.0f32, 0.0f32), |(a, b), &m| (a.min(m), b.max(m)));
        let mut margins: Vec<i32> = w
            .fixtures
            .iter()
            .filter_map(|f| f.result.as_ref())
            .map(|r| (r.score[0] as i32 - r.score[1] as i32).abs())
            .collect();
        margins.sort();
        eprintln!(
            "margins: median {} p90 {}",
            margins[margins.len() / 2],
            margins[margins.len() * 9 / 10]
        );
        eprintln!(
            "{games} games, {hurt} injuries ({per_team_season:.1} per club-season), mean rotation condition {:.3}, morale {lo:.2}..{hi:.2}",
            cond / n as f32
        );
        assert!((1.0..9.0).contains(&per_team_season));
        assert!(cond / n as f32 > 0.86);
    }
}
