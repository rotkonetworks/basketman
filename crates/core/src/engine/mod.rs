//! Match engine.
//!
//! A game is played out on a 2D floor, a tenth of a second at a time. The
//! offense runs its set (`offense.rs`), the defense follows the positioning
//! rules of its scheme (`defense.rs`), and every outcome is read off where
//! the ten players actually are: a shot is as open as the nearest defender is
//! far, a drive ends where the help is standing. What beats what is therefore
//! not a table anywhere; it falls out of the geometry and of who the players
//! are. `floor.rs` holds the possession logic.

// The five attackers, five defenders and their matchups live in parallel
// arrays; indexing them by court index reads better than zipped iterators.
#![allow(clippy::needless_range_loop)]

mod ball;
pub mod contact;
mod defense;
mod flight;
mod floor;
mod offense;
mod shot;
pub use shot::make_rate;
#[cfg(test)]
mod relations_tests;
#[cfg(test)]
mod replay_tests;
pub(crate) use offense::{places, template};

use serde::{Deserialize, Serialize};

use crate::player::{Attr, Player, PlayerId};
use crate::replay::Recording;
use crate::rng::Rng;
use crate::tactics::{DefenseScheme, OffenseSet, Press, Tactics};

/// Simulation step in seconds.
const DT: f32 = 0.1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rules {
    pub periods: u8,
    pub period_secs: u32,
    pub overtime_secs: u32,
    pub shot_clock: u32,
    pub foul_limit: u16,
    /// Team fouls per period before every further foul means two free throws.
    pub bonus_after: u16,
    /// Timeouts per side: first half, second half, each overtime. None carry
    /// over. A person on the bench can change tactics only in a timeout or
    /// a break, so they are worth having; the other bench plays by the same
    /// rule.
    #[serde(default = "default_timeouts")]
    pub timeouts: [u8; 3],
}

fn default_timeouts() -> [u8; 3] {
    [2, 3, 1]
}

impl Rules {
    pub fn fiba() -> Self {
        Rules {
            periods: 4,
            period_secs: 600,
            overtime_secs: 300,
            shot_clock: 24,
            foul_limit: 5,
            bonus_after: 4,
            timeouts: default_timeouts(),
        }
    }

    pub fn nba() -> Self {
        Rules {
            periods: 4,
            period_secs: 720,
            overtime_secs: 300,
            shot_clock: 24,
            foul_limit: 6,
            bonus_after: 4,
            timeouts: default_timeouts(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct BoxLine {
    pub secs: u32,
    pub pts: u16,
    pub fgm: u16,
    pub fga: u16,
    pub tpm: u16,
    pub tpa: u16,
    pub ftm: u16,
    pub fta: u16,
    pub orb: u16,
    pub drb: u16,
    pub ast: u16,
    pub stl: u16,
    pub blk: u16,
    pub tov: u16,
    pub pf: u16,
    /// Plus/minus: his team's points minus the opponent's while he was on
    /// the floor.
    #[serde(default)]
    pub pm: i16,
}

impl BoxLine {
    pub fn add(&mut self, o: &BoxLine) {
        self.secs += o.secs;
        self.pts += o.pts;
        self.fgm += o.fgm;
        self.fga += o.fga;
        self.tpm += o.tpm;
        self.tpa += o.tpa;
        self.ftm += o.ftm;
        self.fta += o.fta;
        self.orb += o.orb;
        self.drb += o.drb;
        self.ast += o.ast;
        self.stl += o.stl;
        self.blk += o.blk;
        self.tov += o.tov;
        self.pf += o.pf;
        self.pm += o.pm;
    }

    pub fn reb(&self) -> u16 {
        self.orb + self.drb
    }

    /// Hollinger's game score: one number for a box-score line.
    pub fn game_score(&self) -> f32 {
        self.pts as f32 + 0.4 * self.fgm as f32
            - 0.7 * self.fga as f32
            - 0.4 * (self.fta as f32 - self.ftm as f32)
            + 0.7 * self.orb as f32
            + 0.3 * self.drb as f32
            + self.stl as f32
            + 0.7 * self.ast as f32
            + 0.7 * self.blk as f32
            - 0.4 * self.pf as f32
            - self.tov as f32
    }

    /// Average match rating over `games` games of season totals.
    pub fn rating_per(&self, games: u16) -> f32 {
        (5.0 + self.game_score() / games.max(1) as f32 * 0.22).clamp(1.0, 10.0)
    }

    /// Match rating on a 1..=10 scale, 5 being an empty line.
    pub fn rating(&self) -> f32 {
        (5.0 + self.game_score() * 0.22).clamp(1.0, 10.0)
    }
}

/// One team's input to a game. `players` is parallel to `tactics.rotation`.
pub struct Lineup<'a> {
    pub tactics: &'a Tactics,
    pub players: Vec<&'a Player>,
    /// 0..=1, see `TacticSlot::familiarity`.
    pub familiarity: f32,
    /// 0..=1, how far the squad trusts its coach: a team that believes
    /// plays a little above itself and listens to the bench.
    pub belief: f32,
    /// Tactics rating of the man on the bench, 0 for none: how well the
    /// side reads the game and changes its system when it is losing.
    pub coach: u8,
}

/// Index 0 is the home team throughout.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameResult {
    pub score: [u16; 2],
    /// Points per period, overtime periods appended.
    pub periods: Vec<[u16; 2]>,
    pub boxes: [Vec<(PlayerId, BoxLine)>; 2],
    pub possessions: [u16; 2],
}

struct Slot {
    p: Player,
    target_secs: f32,
    /// Appetite for shots, around 1.0.
    usage: f32,
    brief: crate::tactics::Brief,
    /// Where he lines up, and 0..=1 how well he knows the position.
    pos: crate::player::Position,
    fit: f32,
    fatigue: f32,
    /// Tonight's form, around 1.0. Inconsistent players swing further.
    form: f32,
    /// Seconds on court.
    played: f32,
    line: BoxLine,
    /// -1..=1, how hard he is going after it: lifted by the bench, or
    /// deflated by it. Fades back to 0 over a few minutes.
    drive: f32,
    /// -1..=1, concentration: rattled below 0, locked in above.
    focus: f32,
}

struct Side {
    slots: Vec<Slot>,
    /// Slot indices on court, sorted PG..C so index k matches up with the
    /// opponent's index k.
    on: Vec<usize>,
    tactics: Tactics,
    score: u16,
    /// Team fouls in the current period.
    team_fouls: u16,
    /// 0 = fluent in the tactic, 1 = never trained it.
    unfamiliar: f32,
    belief: f32,
    coach: u8,
    /// Changes of system the bench has made on its own.
    adjusted: u8,
    /// A person is on the bench: the engine leaves timeouts to him.
    coached: bool,
    /// The assistant runs the substitutions from the minutes plan.
    auto_subs: bool,
    /// Swaps waiting for the next dead ball, as slot indices (off, on).
    queue: Vec<(usize, usize)>,
    timeouts: u8,
    timeout_wanted: bool,
    /// Game seconds at the side's last stoppage (a timeout or a break).
    stopped_at: f32,
    /// Timeouts this bench called.
    called: u8,
    /// Points the opponent has scored since this side last did.
    unanswered: u16,
    /// 0..=1, how worn the coach's voice is: shouting all night stops
    /// meaning anything.
    voice: f32,
    /// System change waiting for the next dead ball, with its familiarity.
    orders: Option<(Orders, f32)>,
}

impl Side {
    fn new(lineup: &Lineup, regulation: f32, rng: &mut Rng) -> Self {
        assert!(
            lineup.players.len() >= 5,
            "a rotation needs at least five players"
        );
        assert_eq!(lineup.players.len(), lineup.tactics.rotation.len());
        let minutes: f32 = lineup
            .tactics
            .rotation
            .iter()
            .map(|s| s.minutes.max(1) as f32)
            .sum();
        let slots = lineup
            .tactics
            .rotation
            .iter()
            .zip(&lineup.players)
            .map(|(slot, p)| {
                let scoring = (p.attrs[Attr::CloseShot]
                    + p.attrs[Attr::MidRange]
                    + p.attrs[Attr::ThreePoint])
                    / 3.0;
                Slot {
                    p: (*p).clone(),
                    target_secs: slot.minutes.max(1) as f32 / minutes * 5.0 * regulation,
                    usage: slot.role.usage() * (0.4 + scoring / 20.0),
                    brief: slot.brief,
                    pos: slot.position.unwrap_or(p.position),
                    fit: p.knows(slot.position.unwrap_or(p.position)),
                    fatigue: 1.0 - p.condition,
                    form: (1.0
                        + (rng.f32() - 0.5)
                            * 0.008
                            * (21 - p.personality.consistency.min(20)) as f32)
                        * (1.0 + 0.05 * (p.morale - 0.5))
                        * (1.0 - 0.12 * (1.0 - p.sharp)),
                    played: 0.0,
                    line: BoxLine::default(),
                    drive: 0.0,
                    focus: 0.0,
                }
            })
            .collect();
        Side {
            slots,
            on: Vec::new(),
            tactics: lineup.tactics.clone(),
            score: 0,
            team_fouls: 0,
            unfamiliar: 1.0 - lineup.familiarity.clamp(0.0, 1.0),
            belief: lineup.belief.clamp(0.0, 1.0),
            coach: lineup.coach,
            adjusted: 0,
            coached: false,
            auto_subs: true,
            queue: Vec::new(),
            timeouts: 0,
            timeout_wanted: false,
            stopped_at: 0.0,
            called: 0,
            unanswered: 0,
            voice: 0.0,
            orders: None,
        }
    }

    /// Fatigue-scaled attribute of slot `i`.
    fn eff(&self, i: usize, a: Attr) -> f32 {
        let s = &self.slots[i];
        // A man out of position is a step late everywhere.
        s.p.attrs[a]
            * (1.0 - 0.3 * s.fatigue)
            * s.form
            * (1.0 + 0.035 * s.drive + 0.02 * s.focus)
            * (1.0 - 0.14 * (1.0 - s.fit))
            * (1.0 + 0.07 * (self.belief - 0.5))
    }

    /// How readily a defender fouls, 1 for an average one. A strong, sharp,
    /// composed man bodies people legally; a strong one who loses his head
    /// fouls; a weak one is in between.
    fn foul_mult(&self, i: usize) -> f32 {
        let s = &self.slots[i];
        let force = self.eff(i, Attr::Strength) / 20.0;
        let control = (0.35 * self.eff(i, Attr::Iq)
            + 0.35 * self.eff(i, Attr::Composure)
            + 0.3 * s.p.personality.determination as f32)
            / 20.0;
        (1.0 + (0.2 + force) * (0.5 - control) * 2.0).clamp(0.35, 2.2)
    }

    /// How often an attacker gets himself fouled, 1 for an average one:
    /// the ball handler who protects the ball, the strong finisher who goes
    /// through contact, the calm head who knows how to draw it.
    fn draw_mult(&self, i: usize) -> f32 {
        let skill = (0.25 * self.eff(i, Attr::Handling)
            + 0.25 * self.eff(i, Attr::Strength)
            + 0.2 * self.eff(i, Attr::CloseShot)
            + 0.15 * self.eff(i, Attr::Composure)
            + 0.15 * self.eff(i, Attr::Iq))
            / 10.0;
        ((skill - 1.0) * 1.6 + 1.0).clamp(0.35, 2.2)
    }

    /// 0..=1: how likely a defender is to step in front of a driver and take
    /// the charge. Needs a head for it, the nerve, and the will.
    fn hunt_skill(&self, i: usize, inside: bool) -> f32 {
        let s = &self.slots[i];
        (0.3 * self.eff(i, Attr::Iq)
            + 0.2 * self.eff(i, Attr::Composure)
            + 0.2 * s.p.personality.determination as f32
            + 0.15 * self.eff(i, Attr::WorkRate)
            + 0.15
                * self.eff(
                    i,
                    if inside {
                        Attr::InteriorD
                    } else {
                        Attr::PerimeterD
                    },
                ))
            / 20.0
    }

    /// 0..=1: how strongly a defender is drawn to stepping into a driver's
    /// path to take the charge: skill and nerve, and the will to do it.
    fn hunt_urge(&self, i: usize, inside: bool) -> f32 {
        contact::charge_urge(self.hunt_skill(i, inside), &self.slots[i].p.personality)
    }

    /// 0..=1: how much a player is inclined to sell contact. Low
    /// professionalism and temperament go with it.
    fn flop_urge(&self, i: usize) -> f32 {
        let p = &self.slots[i].p.personality;
        ((20 - p.professionalism.min(20)) as f32 + (20 - p.temperament.min(20)) as f32) / 40.0
    }

    /// How much a player's reach helps on a loose or contested ball,
    /// around 1.0 for an average wing.
    fn length(&self, i: usize) -> f32 {
        (1.0 + (self.slots[i].p.body.standing_reach_cm - 265.0) / 80.0).max(0.5)
    }

    /// -0.9..=1.0 from the aggression slider.
    fn aggression(&self) -> f32 {
        (self.tactics.aggression as f32 - 10.0) / 10.0
    }

    /// How much harder a pressing team works.
    fn press_load(&self) -> f32 {
        match self.tactics.press {
            Press::None => 1.0,
            Press::ThreeQuarter => 1.1,
            Press::FullCourt => 1.25,
        }
    }

    fn need(&self, i: usize, elapsed: f32, regulation: f32, foul_limit: u16) -> f32 {
        let s = &self.slots[i];
        if s.line.pf >= foul_limit {
            return -1e6;
        }
        let frac = (elapsed + 120.0) / regulation;
        let stay = if self.on.contains(&i) { 240.0 } else { 0.0 };
        s.target_secs * frac - s.played + stay - s.fatigue * 150.0
    }

    /// Put the five players furthest behind their minutes target on court.
    /// Being on court already is worth a bonus so stints last a few minutes.
    fn substitute(&mut self, elapsed: f32, regulation: f32, foul_limit: u16) {
        if !self.auto_subs && !self.on.is_empty() {
            self.apply_queue(foul_limit);
            let slots = &self.slots;
            self.on
                .sort_by_key(|&i| (slots[i].pos, slots[i].p.body.height_cm as u32));
            return;
        }
        // Before the tip the coach may already have queued swaps: they
        // are made in the starting five.
        let queued = if self.on.is_empty() && !self.auto_subs {
            std::mem::take(&mut self.queue)
        } else {
            Vec::new()
        };
        self.queue.clear();
        self.on = self.picked(elapsed, regulation, foul_limit);
        self.queue = queued;
        if !self.queue.is_empty() {
            self.apply_queue(foul_limit);
        }
        let slots = &self.slots;
        self.on
            .sort_by_key(|&i| (slots[i].pos, slots[i].p.body.height_cm as u32));
    }

    /// Who is on the floor: before the first dead ball, who will start,
    /// with the swaps queued so far.
    fn slots_on(&self, regulation: f32) -> Vec<usize> {
        if !self.on.is_empty() {
            return self.on.clone();
        }
        let mut me = self.picked(0.0, regulation, u16::MAX);
        for &(off, on) in &self.queue {
            if let Some(k) = me.iter().position(|&i| i == off)
                && !me.contains(&on)
            {
                me[k] = on;
            }
        }
        let slots = &self.slots;
        me.sort_by_key(|&i| (slots[i].pos, slots[i].p.body.height_cm as u32));
        me
    }

    /// The five furthest behind their minutes target, as slot indices.
    fn picked(&self, elapsed: f32, regulation: f32, foul_limit: u16) -> Vec<usize> {
        let mut need: Vec<(f32, usize)> = (0..self.slots.len())
            .map(|i| (self.need(i, elapsed, regulation, foul_limit), i))
            .collect();
        need.sort_by(|a, b| b.0.total_cmp(&a.0));
        need.iter().take(5).map(|n| n.1).collect()
    }

    /// Make the queued swaps in `on`.
    fn apply_queue(&mut self, foul_limit: u16) {
        for (off, on) in std::mem::take(&mut self.queue) {
            let k = self.on.iter().position(|&i| i == off);
            if let Some(k) = k
                && !self.on.contains(&on)
                && self.slots[on].line.pf < foul_limit
            {
                self.on[k] = on;
            }
        }
    }

    /// Replace the player at court index `k` (fouled out) with the best
    /// option on the bench, without disturbing the other four.
    fn replace(&mut self, k: usize, elapsed: f32, regulation: f32, foul_limit: u16) {
        let best = (0..self.slots.len())
            .filter(|i| !self.on.contains(i))
            .map(|i| (self.need(i, elapsed, regulation, foul_limit), i))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((need, i)) = best
            && need > -1e5
        {
            self.on[k] = i;
        }
    }

    /// Book one free throw.
    fn book_ft(&mut self, s: usize, made: bool) {
        let line = &mut self.slots[s].line;
        line.fta += 1;
        if made {
            line.ftm += 1;
            line.pts += 1;
            self.score += 1;
        }
    }

    fn tick(&mut self, dur: f32, pace: f32) {
        let load = self.press_load() * (0.8 + pace / 50.0);
        for (i, s) in self.slots.iter_mut().enumerate() {
            if self.on.contains(&i) {
                s.played += dur;
                s.fatigue += dur
                    * 0.0009
                    * (1.6 - s.p.attrs[Attr::Stamina] / 20.0)
                    * load
                    * (1.0 + 0.4 * s.drive.max(0.0));
            } else {
                s.fatigue -= dur * 0.0025;
            }
            s.fatigue = s.fatigue.clamp(0.0, 1.0);
            // Whatever was said from the bench wears off in about five minutes.
            let fade = dur / 300.0;
            s.drive -= s.drive.clamp(-fade, fade);
            s.focus -= s.focus.clamp(-fade, fade);
        }
        self.voice = (self.voice - dur / 400.0).max(0.0);
    }

    /// Does a bench without a person on it want a timeout at this dead
    /// ball? It can change its system only in a timeout or a break, so it
    /// asks for one to stop a run, to change a defense that is being beaten,
    /// and late in a close game. It keeps its last one for the end.
    fn wants_timeout(&self, other: &Side, elapsed: f32, regulation: f32) -> bool {
        if self.timeouts == 0 {
            return false;
        }
        if self.unanswered >= 9 {
            return true;
        }
        if self.coach < 6 || elapsed - self.stopped_at < 60.0 {
            return false;
        }
        let margin = self.score as i32 - other.score as i32;
        let left = regulation - elapsed;
        let late = left > 0.0 && left <= 120.0;
        if late && (-6..=0).contains(&margin) {
            return true;
        }
        margin <= -7
            && self.adjusted < 3
            && elapsed - self.stopped_at >= 180.0
            && (self.timeouts > 1 || left <= regulation * 0.25)
    }

    /// Sixty seconds on the bench: legs back, heads cleared.
    fn rest(&mut self, called: bool) {
        for &i in &self.on {
            let s = &mut self.slots[i];
            s.fatigue = (s.fatigue - 0.05).max(0.0);
            if called {
                s.focus = s.focus.max(0.0) + 0.1;
            }
        }
        if called {
            self.unanswered = 0;
        }
    }

    /// The other team scored again without reply; past seven points the
    /// five on the floor start to lose their heads.
    fn conceded(&mut self, points: u16) {
        self.unanswered += points;
        if self.unanswered >= 7 {
            for &i in &self.on {
                let s = &mut self.slots[i];
                s.focus = (s.focus - 0.08).max(-0.5);
            }
        }
    }

    /// The coach shouts `shout` at slot `target`, or at the five on the
    /// floor. `margin` is the lead in points. How each player takes it
    /// depends on who he is and on whether he has it coming.
    fn shout(
        &mut self,
        shout: Shout,
        target: Option<usize>,
        margin: i32,
    ) -> Vec<(PlayerId, Reaction)> {
        // A coach the squad has stopped believing in is shouting at backs.
        let heard = 0.4 + 1.1 * self.belief;
        let scale = (1.0 - self.voice).max(0.1) * if target.is_some() { 1.3 } else { 1.0 } * heard;
        let standing = (self.belief - 0.5) * 0.9;
        self.voice = (self.voice + 0.3).min(0.9);
        let who: Vec<usize> = match target {
            Some(i) => vec![i],
            None => self.on.clone(),
        };
        let mut out = Vec::new();
        for i in who {
            let s = &mut self.slots[i];
            let per = &s.p.personality;
            let trait_of = |v: u8| (v.min(20) as f32 - 10.0) / 10.0;
            let rating = s.line.rating();
            let settled = s.played > 240.0;
            let poor = settled && rating < 4.6;
            let good = settled && rating > 6.5;
            let (drive, focus, reaction) = match shout {
                Shout::Encourage => {
                    if poor || margin < -6 || s.focus < -0.1 {
                        (0.25, 0.25, Reaction::Lifted)
                    } else {
                        (0.08, 0.08, Reaction::Unmoved)
                    }
                }
                Shout::DemandMore => {
                    let fair = if poor || margin < -4 {
                        0.5
                    } else if good && margin > 8 {
                        -0.6
                    } else {
                        0.0
                    };
                    let takes = trait_of(per.determination) * 0.6
                        + trait_of(per.temperament) * 0.3
                        + fair
                        + standing;
                    if takes > -0.25 {
                        (0.45 + 0.25 * takes.min(1.0), -0.05, Reaction::FiredUp)
                    } else {
                        (-0.35, -0.3, Reaction::Upset)
                    }
                }
                Shout::CalmDown => {
                    let hot = s.drive > 0.2 || s.focus < -0.1 || s.line.pf >= 3;
                    if hot {
                        (-0.2, 0.45, Reaction::Settled)
                    } else {
                        (-0.2, 0.15, Reaction::Unmoved)
                    }
                }
                Shout::Focus => {
                    let listens = 0.75 + 0.25 * trait_of(per.professionalism);
                    let reaction = if s.focus < 0.3 {
                        Reaction::Settled
                    } else {
                        Reaction::Unmoved
                    };
                    (0.0, 0.4 * listens, reaction)
                }
                Shout::Praise => {
                    if good && margin > 8 && per.ambition < 8 {
                        (-0.35, -0.1, Reaction::Complacent)
                    } else if good {
                        (0.2, 0.2, Reaction::Lifted)
                    } else if poor {
                        (0.0, -0.1, Reaction::Unmoved)
                    } else {
                        (0.08, 0.08, Reaction::Unmoved)
                    }
                }
            };
            s.drive = (s.drive + drive * scale).clamp(-1.0, 1.0);
            s.focus = (s.focus + focus * scale).clamp(-1.0, 1.0);
            out.push((s.p.id, reaction));
        }
        out
    }
}

/// Something shouted from the touchline.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shout {
    Encourage,
    DemandMore,
    CalmDown,
    Focus,
    Praise,
}

impl Shout {
    pub const ALL: [Shout; 5] = [
        Shout::Encourage,
        Shout::DemandMore,
        Shout::CalmDown,
        Shout::Focus,
        Shout::Praise,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Shout::Encourage => "Encourage",
            Shout::DemandMore => "Demand more",
            Shout::CalmDown => "Calm down",
            Shout::Focus => "Concentrate",
            Shout::Praise => "Praise",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Shout::Encourage => {
                "Lifts players who are struggling or behind. Does little for anyone else."
            }
            Shout::DemandMore => {
                "More effort, more fouls, tired legs sooner. Determined players respond;                  others sulk, more so when they have been playing well."
            }
            Shout::CalmDown => {
                "Fewer lapses and fouls at the price of some intensity. For players in                  foul trouble or rattled by a run."
            }
            Shout::Focus => "Sharper on defense for a few minutes. Professionals listen best.",
            Shout::Praise => {
                "Keeps a player who is playing well going. Unambitious players with a                  big lead take it as permission to coast."
            }
        }
    }
}

/// How a player took a shout.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reaction {
    FiredUp,
    Lifted,
    Settled,
    Unmoved,
    Upset,
    Complacent,
}

impl Reaction {
    pub fn label(self) -> &'static str {
        match self {
            Reaction::FiredUp => "fired up",
            Reaction::Lifted => "lifted",
            Reaction::Settled => "switched on",
            Reaction::Unmoved => "unmoved",
            Reaction::Upset => "did not take it well",
            Reaction::Complacent => "eased off",
        }
    }

    pub fn is_bad(self) -> bool {
        matches!(self, Reaction::Upset | Reaction::Complacent)
    }
}

/// The part of the tactics a coach can change during a game.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Orders {
    pub offense: OffenseSet,
    pub defense: DefenseScheme,
    pub press: Press,
    pub pace: u8,
    pub three_bias: u8,
    pub crash_boards: u8,
    pub aggression: u8,
}

impl Orders {
    pub fn of(t: &Tactics) -> Orders {
        Orders {
            offense: t.offense,
            defense: t.defense,
            press: t.press,
            pace: t.pace,
            three_bias: t.three_bias,
            crash_boards: t.crash_boards,
            aggression: t.aggression,
        }
    }

    fn apply(self, t: &mut Tactics) {
        t.offense = self.offense;
        t.defense = self.defense;
        t.press = self.press;
        t.pace = self.pace;
        t.three_bias = self.three_bias;
        t.crash_boards = self.crash_boards;
        t.aggression = self.aggression;
    }
}

/// Why a live game stopped stepping.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Pause {
    /// Timeout called by this side.
    Timeout(usize),
    PeriodEnd,
    Final,
}

/// One player's state in a live game.
#[derive(Clone, Debug)]
pub struct LivePlayer {
    pub id: PlayerId,
    pub on_court: bool,
    /// 1 = fresh, 0 = spent.
    pub energy: f32,
    pub line: BoxLine,
    pub drive: f32,
    pub focus: f32,
    pub fouled_out: bool,
}

/// A game played a step at a time, with one bench (or neither) run by a
/// person. Left alone it produces exactly what `simulate_recorded` does.
pub struct LiveMatch {
    game: floor::Game,
}

impl LiveMatch {
    /// `human` is the side (0 = home) a person coaches.
    pub fn new(
        home: &Lineup,
        away: &Lineup,
        rules: &Rules,
        rng: Rng,
        human: Option<usize>,
    ) -> Self {
        let mut game = floor::Game::new(home, away, rules, rng, true);
        if let Some(side) = human {
            game.sides[side].coached = true;
        }
        LiveMatch { game }
    }

    /// Advance a tenth of a second, or over one dead ball.
    pub(crate) fn step(&mut self) -> Option<Pause> {
        self.game.step()
    }

    pub fn is_over(&self) -> bool {
        self.game.over()
    }

    /// Replay time of the newest moment played.
    pub fn time(&self) -> f32 {
        self.game.wall
    }

    pub fn recording(&self) -> &Recording {
        self.game.rec.as_ref().expect("live games are recorded")
    }

    /// The box score so far.
    pub fn result(&self) -> GameResult {
        self.game.result()
    }

    pub fn finish(self) -> (GameResult, Recording) {
        let result = self.game.result();
        (result, self.game.rec.unwrap_or_default())
    }

    pub fn players(&self, side: usize) -> Vec<LivePlayer> {
        let s = &self.game.sides[side];
        let on = s.slots_on(self.regulation());
        s.slots
            .iter()
            .enumerate()
            .map(|(i, slot)| {
                let mut line = slot.line;
                line.secs = (slot.played + 0.5) as u32;
                LivePlayer {
                    id: slot.p.id,
                    on_court: on.contains(&i),
                    energy: 1.0 - slot.fatigue,
                    line,
                    drive: slot.drive,
                    focus: slot.focus,
                    fouled_out: slot.line.pf >= self.game.rules.foul_limit,
                }
            })
            .collect()
    }

    /// The five on the floor, point guard to centre: the man at each place
    /// guards the opponent's at the same place when they play man to man.
    pub fn five(&self, side: usize) -> Vec<PlayerId> {
        let s = &self.game.sides[side];
        s.slots_on(self.regulation()).iter().map(|&i| s.slots[i].p.id).collect()
    }

    /// How tightly the crew is calling this game, 0.85 to 1.15.
    pub fn whistle(&self) -> f32 {
        self.game.whistle
    }

    pub fn timeouts_left(&self, side: usize) -> u8 {
        self.game.sides[side].timeouts
    }

    pub fn timeout_wanted(&self, side: usize) -> bool {
        self.game.sides[side].timeout_wanted
    }

    /// Ask for a timeout at the next dead ball; call again to wave it off.
    pub(crate) fn call_timeout(&mut self, side: usize) {
        let s = &mut self.game.sides[side];
        s.timeout_wanted = !s.timeout_wanted && s.timeouts > 0;
    }

    pub fn rules(&self) -> &Rules {
        &self.game.rules
    }

    fn regulation(&self) -> f32 {
        (self.game.rules.periods as u32 * self.game.rules.period_secs) as f32
    }

    /// How many timeouts this bench has taken.
    pub fn timeouts_called(&self, side: usize) -> u8 {
        self.game.sides[side].called
    }

    /// What `player` has been told to do with the ball.
    pub fn brief(&self, side: usize, player: PlayerId) -> Option<crate::tactics::Brief> {
        let s = &self.game.sides[side];
        s.slots.iter().find(|x| x.p.id == player).map(|x| x.brief)
    }

    /// Change one player's instruction with the ball, from the next
    /// possession. False if he is not on the roster.
    pub(crate) fn set_brief(
        &mut self,
        side: usize,
        player: PlayerId,
        brief: crate::tactics::Brief,
    ) -> bool {
        match self.game.sides[side]
            .slots
            .iter_mut()
            .find(|x| x.p.id == player)
        {
            Some(slot) => {
                slot.brief = brief;
                true
            }
            None => false,
        }
    }

    pub fn team_fouls(&self, side: usize) -> u16 {
        self.game.sides[side].team_fouls
    }

    pub fn auto_subs(&self, side: usize) -> bool {
        self.game.sides[side].auto_subs
    }

    pub(crate) fn set_auto_subs(&mut self, side: usize, on: bool) {
        self.game.sides[side].auto_subs = on;
    }

    /// Bring `on` in for `off` at the next dead ball. Returns false if the
    /// swap makes no sense (not on the floor, already on it, fouled out).
    pub(crate) fn swap(&mut self, side: usize, off: PlayerId, on: PlayerId) -> bool {
        let limit = self.game.rules.foul_limit;
        let regulation = self.regulation();
        let s = &mut self.game.sides[side];
        let slot = |id: PlayerId| s.slots.iter().position(|x| x.p.id == id);
        let (Some(off), Some(on)) = (slot(off), slot(on)) else {
            return false;
        };
        let now = s.slots_on(regulation);
        if !now.contains(&off) || now.contains(&on) || s.slots[on].line.pf >= limit {
            return false;
        }
        s.queue.retain(|&(a, b)| a != off && b != on);
        s.queue.push((off, on));
        true
    }

    /// Swaps waiting for the next dead ball, as (off, on).
    pub fn queued(&self, side: usize) -> Vec<(PlayerId, PlayerId)> {
        let s = &self.game.sides[side];
        s.queue
            .iter()
            .map(|&(a, b)| (s.slots[a].p.id, s.slots[b].p.id))
            .collect()
    }

    pub(crate) fn cancel_swap(&mut self, side: usize, n: usize) {
        let q = &mut self.game.sides[side].queue;
        if n < q.len() {
            q.remove(n);
        }
    }

    /// 0 = every shout lands in full, towards 1 = nobody is listening.
    pub fn voice(&self, side: usize) -> f32 {
        self.game.sides[side].voice
    }

    pub(crate) fn shout(
        &mut self,
        side: usize,
        shout: Shout,
        target: Option<PlayerId>,
    ) -> Vec<(PlayerId, Reaction)> {
        let margin = self.game.sides[side].score as i32 - self.game.sides[1 - side].score as i32;
        let s = &mut self.game.sides[side];
        let target = match target {
            Some(id) => match s.slots.iter().position(|x| x.p.id == id) {
                Some(i) => Some(i),
                None => return Vec::new(),
            },
            None => None,
        };
        let out = s.shout(shout, target, margin);
        self.game.shouted(side, shout);
        out
    }

    /// How many times a side's bench has changed its system by itself.
    pub fn adjustments(&self, side: usize) -> u8 {
        self.game.sides[side].adjusted
    }

    /// The orders in force, or those waiting for the next dead ball.
    pub fn orders(&self, side: usize) -> Orders {
        let s = &self.game.sides[side];
        s.orders.map_or(Orders::of(&s.tactics), |o| o.0)
    }

    /// Change the system from the next dead ball on. `familiarity` is how
    /// well the squad knows the new one, 0..=1.
    pub(crate) fn set_orders(&mut self, side: usize, orders: Orders, familiarity: f32) {
        self.game.sides[side].orders = Some((orders, familiarity));
    }
}

pub fn simulate_game(home: &Lineup, away: &Lineup, rules: &Rules, rng: &mut Rng) -> GameResult {
    let mut game = floor::Game::new(home, away, rules, rng.clone(), false);
    while !game.over() {
        game.step();
    }
    *rng = game.rng.clone();
    game.result()
}

/// Like `simulate_game`, and also returns the full recording. Given the same
/// inputs and seed the result is identical to the unrecorded game.
pub fn simulate_recorded(
    home: &Lineup,
    away: &Lineup,
    rules: &Rules,
    rng: &mut Rng,
) -> (GameResult, Recording) {
    let mut game = floor::Game::new(home, away, rules, rng.clone(), true);
    while !game.over() {
        game.step();
    }
    *rng = game.rng.clone();
    (game.result(), game.rec.unwrap_or_default())
}
