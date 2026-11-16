//! Referees as people: a pool in the world, a crew of three for every game,
//! a season record each, and the slow business of getting better, getting
//! worse and retiring.
//!
//! The pool has its own random stream, seeded from the world seed, so that
//! generating (or topping up) referees never disturbs the stream players are
//! made from. Nothing here is drawn from the world's own `rng`.
//!
//! What a referee does in a game, and how he is wrong, lives in
//! `engine/officiating.rs`. This file only holds who he is.

use serde::{Deserialize, Serialize};

use crate::league::{FIRST_NAMES, LAST_NAMES, World};
use crate::rng::Rng;

pub type RefId = u16;

/// Referees a season's record is kept for before it drops off the end.
const HISTORY: usize = 8;
/// A referee from this age on may retire, and must by `RETIRE_MAX`.
const RETIRE_FROM: u8 = 54;
const RETIRE_MAX: u8 = 62;
/// Games before the league office judges a referee's record.
const AUDIT_GAMES: u16 = 14;
/// The share of his calls a referee may have wrong (as the film shows)
/// before the office acts.
const AUDIT_WRONG: f32 = 0.115;

/// What a referee did over one season.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RefRecord {
    pub games: u16,
    /// Fouls he blew, and how many of them went against the home side.
    pub fouls: u32,
    pub on_home: u32,
    pub on_away: u32,
    /// Calls he made, and how many of them the film shows were wrong
    /// (missed fouls included).
    pub calls: u32,
    pub wrong: u32,
    /// Calls a coach challenged that were overturned, and challenges heard.
    pub overturned: u16,
    pub reviewed: u16,
}

impl RefRecord {
    pub fn fouls_per_game(&self) -> f32 {
        self.fouls as f32 / self.games.max(1) as f32
    }

    /// Share of his decisions that were right, 0..=1.
    pub fn right(&self) -> f32 {
        1.0 - self.wrong as f32 / self.calls.max(1) as f32
    }

    /// Home fouls minus away fouls per game: positive is a man who whistles
    /// the home side more.
    pub fn home_balance(&self) -> f32 {
        (self.on_home as f32 - self.on_away as f32) / self.games.max(1) as f32
    }

    fn add(&mut self, o: &RefRecord) {
        self.games += o.games;
        self.fouls += o.fouls;
        self.on_home += o.on_home;
        self.on_away += o.on_away;
        self.calls += o.calls;
        self.wrong += o.wrong;
        self.overturned += o.overturned;
        self.reviewed += o.reviewed;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Referee {
    pub id: RefId,
    pub name: String,
    pub age: u8,
    /// Games worked in his career, before this season's record.
    pub experience: u16,
    /// 1..=20. How often he sees what happened.
    pub accuracy: u8,
    /// 1..=20. How much contact he lets go: 1 lets them play, 20 whistles
    /// everything.
    pub strictness: u8,
    /// 1..=20. Whether the same contact gets the same call all night.
    pub consistency: u8,
    /// 1..=20. How much the crowd moves him.
    pub home_lean: u8,
    /// 1..=20. How he copes with a late, close game.
    pub composure: u8,
    /// 1..=20. Calls the charge (high) or the block (low) on a collision.
    pub charge_lean: u8,
    /// 1..=20. How easily a flop buys a whistle.
    pub flop_buyer: u8,
    #[serde(default)]
    pub season: RefRecord,
    #[serde(default)]
    pub history: Vec<(u16, RefRecord)>,
    #[serde(default = "yes")]
    pub active: bool,
    /// The league office has taken him off the top games for now.
    #[serde(default)]
    pub warned: bool,
}

fn yes() -> bool {
    true
}

impl Referee {
    /// One number for how good he is, 1..=20: mostly eyes, some nerve and
    /// steadiness.
    pub fn quality(&self) -> f32 {
        (3.0 * self.accuracy as f32 + self.composure as f32 + self.consistency as f32) / 5.0
    }

    /// What he is known for, a few plain phrases.
    pub fn tags(&self) -> Vec<&'static str> {
        let mut t = Vec::new();
        if self.strictness >= 14 {
            t.push("quick whistle");
        } else if self.strictness <= 6 {
            t.push("lets them play");
        }
        if self.accuracy >= 16 {
            t.push("reads it well");
        } else if self.accuracy <= 7 {
            t.push("misses a lot");
        }
        if self.home_lean >= 14 {
            t.push("home crowds get to him");
        }
        if self.charge_lean >= 14 {
            t.push("calls the charge");
        } else if self.charge_lean <= 6 {
            t.push("calls blocks, rarely a charge");
        }
        if self.flop_buyer >= 14 {
            t.push("buys a flop");
        } else if self.flop_buyer <= 6 {
            t.push("not fooled by flops");
        }
        if self.consistency <= 6 {
            t.push("inconsistent");
        }
        if self.composure <= 6 {
            t.push("shaky in close games");
        }
        t
    }

    /// The traits the engine needs, on a 0..=1 scale.
    pub fn official(&self) -> Official {
        let f = |x: u8| (x.clamp(1, 20) as f32 - 1.0) / 19.0;
        Official {
            id: self.id,
            accuracy: f(self.accuracy),
            strictness: f(self.strictness),
            consistency: f(self.consistency),
            home_lean: f(self.home_lean),
            composure: f(self.composure),
            charge_lean: f(self.charge_lean),
            flop_buyer: f(self.flop_buyer),
        }
    }
}

/// A referee as the engine sees him: the traits as 0..=1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Official {
    pub id: RefId,
    pub accuracy: f32,
    pub strictness: f32,
    pub consistency: f32,
    pub home_lean: f32,
    pub composure: f32,
    pub charge_lean: f32,
    pub flop_buyer: f32,
}

impl Official {
    /// A middling official, for games without an assigned crew.
    pub const AVERAGE: Official = Official {
        id: RefId::MAX,
        accuracy: 0.6,
        strictness: 0.5,
        consistency: 0.55,
        home_lean: 0.45,
        composure: 0.55,
        charge_lean: 0.5,
        flop_buyer: 0.5,
    };

    pub fn quality(&self) -> f32 {
        0.6 * self.accuracy + 0.2 * self.composure + 0.2 * self.consistency
    }
}

/// Three officials: the crew chief first, then the other two.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crew {
    pub refs: [Official; 3],
    /// 0..=1, how loud the home hall is; 0 where there is no hall.
    #[serde(default)]
    pub crowd: f32,
}

impl Crew {
    pub fn average() -> Crew {
        Crew {
            refs: [Official::AVERAGE; 3],
            crowd: 0.4,
        }
    }

    pub fn ids(&self) -> [RefId; 3] {
        [self.refs[0].id, self.refs[1].id, self.refs[2].id]
    }

    /// 0..=1, the crew chief's quality.
    pub fn quality(&self) -> f32 {
        self.refs[0].quality()
    }

    /// Mean strictness of the three, 0..=1.
    pub fn strictness(&self) -> f32 {
        (self.refs[0].strictness + self.refs[1].strictness + self.refs[2].strictness) / 3.0
    }
}

/// What a call is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum What {
    /// A foul whistled, or a foul that was not.
    Foul,
    /// Who the ball goes to after it left the floor.
    OutOfBounds,
    /// What a made basket is worth.
    Count,
}

/// What happened, or what was called.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ruling {
    /// No foul.
    Nothing,
    /// A foul by side `by` (0 = home).
    Foul { kind: FoulKind, by: u8 },
    /// Contact that could go either way: any call at all is within the
    /// referee's rights.
    Marginal { kind: FoulKind, by: u8 },
    /// The ball goes to this side.
    Ball(u8),
    /// The basket counts this many points.
    Value(u8),
}

use crate::replay::FoulKind;
use crate::player::PlayerId;

/// How a call compares with what happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    Right,
    /// A foul that was there and was not blown. Cannot be challenged.
    Missed,
    /// A whistle for contact that was legal.
    Phantom,
    /// The right whistle with the wrong foul (a block for a charge), or
    /// against the other side.
    WrongFoul,
    WrongBall,
    WrongCount,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Right => "right",
            Verdict::Missed => "missed foul",
            Verdict::Phantom => "phantom foul",
            Verdict::WrongFoul => "wrong foul",
            Verdict::WrongBall => "ball given the wrong way",
            Verdict::WrongCount => "basket miscounted",
        }
    }

    pub fn is_wrong(self) -> bool {
        self != Verdict::Right
    }
}

/// The result of a coach's challenge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    /// The side that challenged.
    pub by: u8,
    pub overturned: bool,
}

/// One decision by one official, with what really happened beside it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CallRecord {
    /// Replay time, period and seconds left.
    pub time: f32,
    pub period: u8,
    pub clock: f32,
    /// The official in the crew's order (0 = crew chief).
    pub by: u8,
    pub what: What,
    pub truth: Ruling,
    pub called: Ruling,
    /// The player blamed as called, and the one on the other end of it.
    pub who: PlayerId,
    pub on: PlayerId,
    /// The side that profits from the call as made, and by how many
    /// points, roughly, against the right one.
    pub gains: u8,
    pub swing: f32,
    /// Can a coach ask for this to be looked at.
    pub reviewable: bool,
    pub review: Option<Review>,
}

impl CallRecord {
    pub fn verdict(&self) -> Verdict {
        match (self.truth, self.called) {
            (Ruling::Nothing, Ruling::Nothing) => Verdict::Right,
            (Ruling::Marginal { .. }, Ruling::Nothing | Ruling::Foul { .. }) => Verdict::Right,
            (Ruling::Foul { .. }, Ruling::Nothing) => Verdict::Missed,
            (Ruling::Nothing, Ruling::Foul { .. } | Ruling::Marginal { .. }) => Verdict::Phantom,
            (Ruling::Foul { kind: a, by: x }, Ruling::Foul { kind: b, by: y }) => {
                if a == b && x == y {
                    Verdict::Right
                } else {
                    Verdict::WrongFoul
                }
            }
            (Ruling::Ball(a), Ruling::Ball(b)) => {
                if a == b { Verdict::Right } else { Verdict::WrongBall }
            }
            (Ruling::Value(a), Ruling::Value(b)) => {
                if a == b { Verdict::Right } else { Verdict::WrongCount }
            }
            _ => Verdict::Right,
        }
    }

    /// The call is overturned if challenged and was wrong, and is not a
    /// no-call.
    pub fn can_be_overturned(&self) -> bool {
        self.reviewable && !matches!(self.verdict(), Verdict::Right | Verdict::Missed)
    }
}

/// A game's officiating, kept with its result.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GameOfficiating {
    /// The crew, chief first.
    pub crew: [RefId; 3],
    /// Fouls called, by the side that committed them.
    pub fouls: [u16; 2],
    /// Decisions made (whistles and fouls that should have been), and how
    /// many of each kind of error: missed, phantom, wrong foul, ball
    /// given the wrong way, miscounted.
    pub calls: u16,
    /// Decisions on out of bounds and on the count that were looked at.
    #[serde(default)]
    pub others: u16,
    pub errors: [u16; 5],
    /// Expected points the wrong calls gave each side.
    pub swing: [f32; 2],
    /// The wrong calls and every reviewed one, in order.
    pub notable: Vec<CallRecord>,
    /// Each side's challenge, if used.
    pub challenge: [Option<CallRecord>; 2],
    /// Each referee's share of it, in the crew's order.
    pub per_ref: [RefRecord; 3],
    /// Challenges left at the end.
    pub left: [u8; 2],
}

impl GameOfficiating {
    pub fn wrong(&self) -> u16 {
        self.errors.iter().sum()
    }

    /// The side that a wrong call decided the game for: the winner's
    /// margin was no bigger than the points the wrong calls gave him
    /// beyond those given the other side.
    pub fn decided_for(&self, score: [u16; 2]) -> Option<u8> {
        let margin = score[0] as f32 - score[1] as f32;
        if margin == 0.0 {
            return None;
        }
        let w = if margin > 0.0 { 0 } else { 1 };
        let net = self.swing[w] - self.swing[1 - w];
        (net >= margin.abs()).then_some(w as u8)
    }
}

/// What the assistant coach thinks he saw.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Opinion {
    LooksWrong,
    LooksRight,
}

/// What the manager is asked at a stoppage.
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewPrompt {
    /// The side that may challenge.
    pub side: u8,
    pub what: What,
    pub called: Ruling,
    pub who: PlayerId,
    pub on: PlayerId,
    pub period: u8,
    pub clock: f32,
    /// The official who made it, in the crew's order.
    pub by: u8,
    pub opinion: Opinion,
    /// Challenges the side has left, and timeouts (an unsuccessful
    /// challenge costs one).
    pub left: u8,
    pub timeouts: u8,
}

impl Opinion {
    /// What the assistant says, in his words.
    pub fn words(self) -> &'static str {
        match self {
            Opinion::LooksWrong => "Looked clean to me.",
            Opinion::LooksRight => "Looked like a fair call to me.",
        }
    }
}

impl ReviewPrompt {
    /// The call, in plain words.
    pub fn call_text<'a>(&self, name: impl Fn(PlayerId) -> &'a str) -> String {
        match self.called {
            Ruling::Foul { kind, .. } | Ruling::Marginal { kind, .. } => {
                if kind.is_offensive() {
                    format!("{} on {}", kind.label(), name(self.who))
                } else {
                    format!("{} foul on {}", kind.label(), name(self.who))
                }
            }
            Ruling::Ball(s) => format!("Out of bounds, the ball goes to the {}", if s == 0 { "home side" } else { "visitors" }),
            Ruling::Value(n) => format!("{}'s basket counts {}", name(self.who), n),
            Ruling::Nothing => "No call".to_string(),
        }
    }
}

/// The pool and its own random stream.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Officials {
    pub pool: Vec<Referee>,
    pub rng: Option<Rng>,
    /// Added to the day when crews are drawn, so another league gets other
    /// crews on the same dates.
    pub salt: u64,
    /// What the league office has done, newest last: (season, text).
    #[serde(default)]
    pub notices: Vec<(u16, String)>,
    /// A league without referees: the engine's old whistle, no calls, no
    /// challenges. For measuring what officiating does.
    #[serde(default)]
    pub off: bool,
}

/// A bell-shaped draw in `mean ± spread` (sum of three uniforms).
fn bell(rng: &mut Rng, mean: f32, spread: f32) -> f32 {
    mean + spread * (rng.f32() + rng.f32() + rng.f32() - 1.5) * 2.0
}

fn trait_of(rng: &mut Rng, mean: f32, spread: f32) -> u8 {
    (bell(rng, mean, spread).round() as i32).clamp(1, 20) as u8
}

fn mix(a: u64, b: u64) -> u64 {
    let mut x = a ^ b.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

impl Officials {
    /// How many active referees a league of `teams` clubs needs: three for
    /// every game of a day on which every club plays, and some to spare.
    pub fn wanted(teams: usize) -> usize {
        (teams * 2 + 6).max(30)
    }

    /// A pool for a league of `teams` clubs, from `seed`.
    pub fn generate(seed: u64, teams: usize) -> Officials {
        let mut o = Officials {
            pool: Vec::new(),
            rng: Some(Rng::new(seed ^ 0x0FF1_C1A1_5EED)),
            salt: mix(seed, 0x0FF1),
            notices: Vec::new(),
            off: false,
        };
        o.top_up(teams, true);
        o
    }

    /// Hire until `wanted(teams)` referees are active. At the start the pool
    /// has every age; later hires are young.
    pub fn top_up(&mut self, teams: usize, veterans: bool) {
        let want = Officials::wanted(teams);
        let rng = self.rng.get_or_insert_with(|| Rng::new(self.salt ^ 0x5EED));
        let mut active = self.pool.iter().filter(|r| r.active).count();
        while active < want {
            let id = self.pool.len() as RefId;
            let age = if veterans {
                (bell(rng, 42.0, 8.0).round() as i32).clamp(26, 60) as u8
            } else {
                (bell(rng, 27.0, 1.5).round() as i32).clamp(24, 31) as u8
            };
            let seasoned = (age.saturating_sub(24)) as f32;
            let experience = if veterans {
                (seasoned * (20.0 + 14.0 * rng.f32())) as u16
            } else {
                0
            };
            // Eyes sharpen with the years, a little.
            let sharp = (seasoned * 0.08).min(1.6);
            let first = FIRST_NAMES[rng.range(0, FIRST_NAMES.len() as u32) as usize];
            let last = LAST_NAMES[rng.range(0, LAST_NAMES.len() as u32) as usize];
            let r = Referee {
                id,
                name: format!("{first} {last}"),
                age,
                experience,
                accuracy: trait_of(rng, 11.2 + sharp, 3.0),
                strictness: trait_of(rng, 10.5, 4.2),
                consistency: trait_of(rng, 10.5, 3.6),
                home_lean: trait_of(rng, 8.0, 4.0),
                composure: trait_of(rng, 10.5 + sharp * 0.5, 3.4),
                charge_lean: trait_of(rng, 10.5, 4.0),
                flop_buyer: trait_of(rng, 10.0, 4.0),
                season: RefRecord::default(),
                history: Vec::new(),
                active: true,
                warned: false,
            };
            self.pool.push(r);
            active += 1;
        }
    }

    pub fn get(&self, id: RefId) -> Option<&Referee> {
        self.pool.get(id as usize)
    }

    /// Crews for the games of one day, in the order of `games` (each a sort
    /// key: lower is a bigger game). Referees are drawn best first for the
    /// bigger games, nobody works twice in a day, and the order of the
    /// pool is jittered by day so the same three do not get every final.
    pub fn crews_for_day(&self, day: u32, games: &[u32]) -> Vec<[RefId; 3]> {
        let mut ranked: Vec<(f32, RefId)> = self
            .pool
            .iter()
            .filter(|r| r.active)
            .map(|r| {
                let jitter = (mix(self.salt ^ day as u64, r.id as u64) % 1000) as f32 / 1000.0;
                let bench = if r.warned { 5.0 } else { 0.0 };
                (r.quality() + (jitter - 0.5) * 3.0 - bench, r.id)
            })
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut order: Vec<usize> = (0..games.len()).collect();
        order.sort_by_key(|&i| (games[i], i));
        let mut out = vec![[0; 3]; games.len()];
        if ranked.is_empty() {
            return out;
        }
        for (n, &g) in order.iter().enumerate() {
            for j in 0..3 {
                out[g][j] = ranked[(n * 3 + j) % ranked.len()].1;
            }
        }
        out
    }

    /// The engine's view of a crew: the best-rated referee is the chief.
    pub fn crew(&self, ids: [RefId; 3]) -> Crew {
        let mut refs: Vec<Official> = ids
            .iter()
            .map(|&i| self.get(i).map_or(Official::AVERAGE, |r| r.official()))
            .collect();
        refs.sort_by(|a, b| b.quality().total_cmp(&a.quality()).then(a.id.cmp(&b.id)));
        Crew {
            refs: [refs[0], refs[1], refs[2]],
            crowd: 0.4,
        }
    }

    /// A game's worth of record for the referees who worked it.
    pub fn book(&mut self, ids: [RefId; 3], per: &[RefRecord; 3]) {
        for (&id, rec) in ids.iter().zip(per) {
            if let Some(r) = self.pool.get_mut(id as usize) {
                r.season.add(rec);
            }
        }
    }

    /// The season ends: every referee ages, drifts and may retire; the
    /// office looks at the film; the pool is topped up with rookies.
    /// Returns what the office did, in plain words, for the mail.
    pub fn new_season(&mut self, season: u16, teams: usize) -> Vec<String> {
        let mut said = Vec::new();
        let rng = self.rng.get_or_insert_with(|| Rng::new(self.salt ^ 0x5EED));
        for r in self.pool.iter_mut().filter(|r| r.active) {
            let rec = r.season;
            // The office's audit: a man with enough games and a poor
            // record is taken off the big games; one who has mended is
            // put back.
            if rec.games >= AUDIT_GAMES {
                let wrong = 1.0 - rec.right();
                if wrong > AUDIT_WRONG && !r.warned {
                    r.warned = true;
                    said.push(format!(
                        "The league office has taken {} off the top games after a poor season: {:.0}% of his decisions were wrong on film.",
                        r.name,
                        wrong * 100.0
                    ));
                } else if wrong <= AUDIT_WRONG * 0.8 && r.warned {
                    r.warned = false;
                }
            }
            r.experience = r.experience.saturating_add(rec.games);
            if rec.games > 0 {
                r.history.push((season, rec));
                if r.history.len() > HISTORY {
                    r.history.remove(0);
                }
            }
            r.season = RefRecord::default();
            r.age = r.age.saturating_add(1);
            // Eyes: better to the mid-forties, worse after fifty.
            let slope = if r.age < 36 {
                0.30
            } else if r.age < 50 {
                0.0
            } else {
                -0.25
            };
            let drift = |rng: &mut Rng, v: u8, bias: f32| -> u8 {
                let x = v as f32 + bias + (rng.f32() - 0.5) * 1.2;
                (x.round() as i32).clamp(1, 20) as u8
            };
            r.accuracy = drift(rng, r.accuracy, slope);
            r.composure = drift(rng, r.composure, slope * 0.5);
            r.consistency = drift(rng, r.consistency, 0.0);
            r.strictness = drift(rng, r.strictness, 0.0);
            r.home_lean = drift(rng, r.home_lean, -0.1);
            // A warned man works at it.
            if r.warned {
                r.accuracy = (r.accuracy + 1).min(20);
            }
            let retire = r.age >= RETIRE_MAX
                || (r.age >= RETIRE_FROM && rng.chance(0.12 * (r.age - RETIRE_FROM + 1) as f32 / 4.0))
                || (r.warned && r.age >= 40 && rng.chance(0.25));
            if retire {
                r.active = false;
                said.push(format!("{} retires from officiating.", r.name));
            }
        }
        self.top_up(teams, false);
        for s in &said {
            self.notices.push((season, s.clone()));
        }
        if self.notices.len() > 40 {
            let cut = self.notices.len() - 40;
            self.notices.drain(..cut);
        }
        said
    }
}

/// How a day's games are ranked for crews: playoffs first, then by
/// division, top first.
pub fn game_rank(playoff: bool, division: usize) -> u32 {
    if playoff { 0 } else { 1 + division as u32 }
}

impl World {
    /// Old saves have no referees; make a pool for them. The stream is
    /// seeded from the world's, which is only read.
    pub fn ensure_officials(&mut self) {
        let teams = self.teams.len();
        if self.officials.pool.is_empty() {
            let seed = self.rng.clone().next_u64();
            self.officials = Officials::generate(seed, teams);
        } else if self.officials.pool.iter().filter(|r| r.active).count() < Officials::wanted(teams) {
            self.officials.top_up(teams, false);
        }
    }

    /// The crew for each game of `day`, league fixtures and friendlies
    /// alike, known before the game: drawn for the whole day at once so
    /// nobody works two games on it.
    pub fn crews_on(&self, day: u32) -> Vec<(GameKey, [RefId; 3])> {
        let mut keys: Vec<GameKey> = Vec::new();
        let mut ranks: Vec<u32> = Vec::new();
        for (j, g) in self.fixtures.iter().enumerate().filter(|(_, g)| g.day == day) {
            keys.push(GameKey::League(j));
            ranks.push(game_rank(g.playoff, g.division));
        }
        for (j, g) in self.friendlies.iter().enumerate().filter(|(_, g)| g.day == day) {
            keys.push(GameKey::Friendly(j));
            ranks.push(game_rank(false, g.division));
        }
        let crews = self.officials.crews_for_day(day, &ranks);
        keys.into_iter().zip(crews).collect()
    }

    pub fn crew_of(&self, key: GameKey) -> Option<[RefId; 3]> {
        let day = match key {
            GameKey::League(i) => self.fixtures.get(i)?.day,
            GameKey::Friendly(i) => self.friendlies.get(i)?.day,
        };
        self.crews_on(day).into_iter().find(|c| c.0 == key).map(|c| c.1)
    }

    /// The crew for league fixture `i` as people (before the game, or
    /// after it from the result).
    pub fn crew_of_fixture(&self, i: usize) -> Option<[RefId; 3]> {
        if let Some(r) = self.fixtures.get(i)?.result.as_ref().and_then(|r| r.officiating.as_ref()) {
            return Some(r.crew);
        }
        self.crew_of(GameKey::League(i))
    }

    /// The engine's crew for a game, with the home hall's volume.
    pub fn crew_for(&self, key: GameKey) -> Option<Crew> {
        if self.officials.off {
            return None;
        }
        let ids = self.crew_of(key)?;
        let mut crew = self.officials.crew(ids);
        if let GameKey::League(i) = key {
            crew.crowd = self.crowd_effect(i).loud;
        }
        Some(crew)
    }

    /// A game is in the book: the referees' records.
    pub(crate) fn book_officials(&mut self, i: usize) {
        let Some(report) = self.fixtures[i]
            .result
            .as_ref()
            .and_then(|r| r.officiating.as_ref())
        else {
            return;
        };
        let (crew, per) = (report.crew, report.per_ref);
        self.officials.book(crew, &per);
    }

    /// The referees named, as people.
    pub fn referees_of(&self, ids: [RefId; 3]) -> Vec<&Referee> {
        ids.iter().filter_map(|&i| self.officials.get(i)).collect()
    }

    /// The season ends for the referees.
    pub(crate) fn officials_new_season(&mut self) {
        let season = self.season;
        let teams = self.teams.len();
        let said = self.officials.new_season(season, teams);
        let day = self.day;
        for s in said {
            self.post(day, crate::inbox::Kind::League, "League office", "Officials", s, vec![]);
        }
    }
}

impl World {
    /// Who works fixture `i` and what they are known for, with a hint on
    /// how to play under them, for the assistant's preview.
    pub(crate) fn crew_preview(&self, fixture: usize, home: bool) -> String {
        let Some(ids) = self.crew_of(GameKey::League(fixture)) else {
            return String::new();
        };
        let refs = self.referees_of(ids);
        if refs.is_empty() {
            return String::new();
        }
        let names: Vec<&str> = refs.iter().map(|r| r.name.as_str()).collect();
        let mut s = format!("\n\nOfficials: {}.", names.join(", "));
        for r in &refs {
            let t = r.tags();
            if !t.is_empty() {
                s += &format!(" {} is known for it: {}.", r.name, t.join(", "));
            }
        }
        let n = refs.len() as f32;
        let strict = refs.iter().map(|r| r.strictness as f32).sum::<f32>() / n;
        let lean = refs.iter().map(|r| r.home_lean as f32).sum::<f32>() / n;
        if strict <= 8.5 {
            s += " A loose crew: aggressive defense will not cost us much in fouls.";
        } else if strict >= 12.5 {
            s += " A tight crew: keep the hands off, and mind the players on three fouls.";
        }
        if lean >= 11.5 {
            s += if home {
                " The hall will get to them, which should help us."
            } else {
                " The home crowd gets to this crew; expect close calls to go against us."
            };
        }
        s
    }

    /// What the game's officiating comes to, for the manager's report:
    /// fouls each way, the calls the film shows were wrong, and each
    /// bench's challenge.
    pub fn officiating_report(&self, o: &GameOfficiating, side: usize, home: &str, away: &str) -> String {
        let us = [home, away][side];
        let them = [away, home][side];
        let mut s = String::new();
        let names: Vec<String> = self
            .referees_of(o.crew)
            .iter()
            .map(|r| r.name.clone())
            .collect();
        if !names.is_empty() {
            s += &format!("Officials: {}.\n", names.join(", "));
        }
        s += &format!(
            "Fouls called: {} {}, {} {}.\n",
            us, o.fouls[side], them, o.fouls[1 - side]
        );
        let wrong: Vec<&CallRecord> = o.notable.iter().filter(|c| c.verdict().is_wrong()).collect();
        let against = wrong.iter().filter(|c| c.gains as usize != side).count();
        let for_us = wrong.len() - against;
        if wrong.is_empty() {
            s += "The film shows no wrong calls that mattered.\n";
        } else {
            s += &format!(
                "On the film {} calls were wrong: {} against us, {} for us.\n",
                wrong.len(),
                against,
                for_us
            );
        }
        for (k, name) in [(side, us), (1 - side, them)] {
            if let Some(c) = &o.challenge[k] {
                let r = c.review.map_or(false, |r| r.overturned);
                s += &format!(
                    "{name} challenged a call ({}): {}.\n",
                    c.verdict().label(),
                    if r { "overturned" } else { "the call stood" }
                );
            }
        }
        s
    }
}

/// A game on the calendar: a league or playoff fixture, or a friendly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameKey {
    League(usize),
    Friendly(usize),
}

/// What a crew is known for, one plain line for a preview.
pub fn crew_line(refs: &[&Referee]) -> String {
    if refs.is_empty() {
        return String::new();
    }
    let n = refs.len() as f32;
    let strict = refs.iter().map(|r| r.strictness as f32).sum::<f32>() / n;
    let acc = refs.iter().map(|r| r.accuracy as f32).sum::<f32>() / n;
    let lean = refs.iter().map(|r| r.home_lean as f32).sum::<f32>() / n;
    let mut parts = Vec::new();
    parts.push(if strict >= 12.5 {
        "a quick whistle"
    } else if strict <= 8.5 {
        "will let them play"
    } else {
        "an even whistle"
    });
    if acc >= 13.5 {
        parts.push("usually gets it right");
    } else if acc <= 9.5 {
        parts.push("misses calls");
    }
    if lean >= 11.5 {
        parts.push("home crowds get to them");
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pool_is_made_from_its_own_stream_and_is_the_same_every_time() {
        let a = Officials::generate(7, 24);
        let b = Officials::generate(7, 24);
        assert_eq!(a.pool.len(), Officials::wanted(24));
        assert_eq!(
            a.pool.iter().map(|r| (&r.name, r.accuracy, r.age)).collect::<Vec<_>>(),
            b.pool.iter().map(|r| (&r.name, r.accuracy, r.age)).collect::<Vec<_>>()
        );
        let c = Officials::generate(8, 24);
        assert!(a.pool.iter().zip(&c.pool).any(|(x, y)| x.accuracy != y.accuracy));
        for r in &a.pool {
            for t in [r.accuracy, r.strictness, r.consistency, r.home_lean, r.composure] {
                assert!((1..=20).contains(&t));
            }
        }
    }

    #[test]
    fn the_pool_is_varied() {
        let o = Officials::generate(3, 24);
        let lo = o.pool.iter().map(|r| r.accuracy).min().unwrap();
        let hi = o.pool.iter().map(|r| r.accuracy).max().unwrap();
        assert!(hi - lo >= 6, "{lo} {hi}");
        let lo = o.pool.iter().map(|r| r.strictness).min().unwrap();
        let hi = o.pool.iter().map(|r| r.strictness).max().unwrap();
        assert!(hi - lo >= 8, "{lo} {hi}");
    }

    #[test]
    fn nobody_works_two_games_on_a_day_and_the_big_games_get_the_best() {
        let o = Officials::generate(5, 30);
        // Fifteen games: a playoff, then divisions 0, 1, 2.
        let ranks: Vec<u32> = (0..15).map(|i| game_rank(i == 0, i % 3)).collect();
        let crews = o.crews_for_day(12, &ranks);
        let mut seen = std::collections::BTreeSet::new();
        for c in &crews {
            for &id in c {
                assert!(seen.insert(id), "referee {id} works twice");
            }
        }
        let q = |c: &[RefId; 3]| o.crew(*c).quality();
        let top: f32 = (0..15).filter(|&i| ranks[i] <= 1).map(|i| q(&crews[i])).sum::<f32>()
            / (0..15).filter(|&i| ranks[i] <= 1).count() as f32;
        let low: f32 = (0..15).filter(|&i| ranks[i] == 3).map(|i| q(&crews[i])).sum::<f32>()
            / (0..15).filter(|&i| ranks[i] == 3).count() as f32;
        assert!(top > low, "{top} {low}");
        // And the draw is stable: known before the game.
        assert_eq!(crews, o.crews_for_day(12, &ranks));
    }

    #[test]
    fn referees_age_retire_and_are_replaced() {
        let mut o = Officials::generate(9, 24);
        let n = o.pool.len();
        let mut retired = 0;
        for s in 0..25 {
            o.new_season(s, 24);
            retired = o.pool.iter().filter(|r| !r.active).count();
            assert_eq!(
                o.pool.iter().filter(|r| r.active).count(),
                Officials::wanted(24)
            );
        }
        assert!(retired > 10 && o.pool.len() > n);
        assert!(o.pool.iter().filter(|r| r.active).all(|r| r.age <= RETIRE_MAX));
    }

    #[test]
    fn a_bad_season_gets_a_referee_warned() {
        let mut o = Officials::generate(2, 24);
        o.pool[0].season = RefRecord {
            games: 30,
            calls: 1000,
            wrong: 250,
            ..Default::default()
        };
        o.pool[0].age = 40;
        let said = o.new_season(1, 24);
        assert!(said.iter().any(|s| s.contains("taken") && s.contains(&o.pool[0].name)));
        assert!(o.pool[0].warned || !o.pool[0].active);
    }
}
