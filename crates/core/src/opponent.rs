//! Scouting the next opponents.
//!
//! The chief scout can be sent to a game of another club (`AssignScout`).
//! When the game is played he files an opponent report built from what
//! happened in it and from that club's real tactics, and it comes as mail.
//! Every claim in a report is rolled against the scout's judgement and the
//! number of that club's games he has seen: a poor scout from one game gets
//! things vague (`None`) or wrong. A report is a claim about the day it was
//! filed; clubs change their tactics now and then, so an old report can
//! mislead. Match preparation (`prep.rs`) is only as good as the report is
//! right on the day of the game.

use serde::{Deserialize, Serialize};

use crate::court::ShotKind;
use crate::engine::{BoxLine, GameResult};
use crate::inbox::Kind;
use crate::league::{TeamId, World};
use crate::player::PlayerId;
use crate::replay::{EventKind, Recording, Source};
use crate::staff::Job;
use crate::tactics::{DefenseScheme, OffenseSet, Press};

/// Reports kept; the oldest go first.
const KEEP: usize = 60;
/// Chance in a week that another club changes its system.
const DRIFT_CHANCE: f32 = 0.05;

/// The chief scout is to watch `team` play on `day`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoutAssignment {
    pub day: u32,
    pub team: TeamId,
}

/// A game on the calendar: a league (or playoff) fixture or a friendly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameRef {
    League(usize),
    Friendly(usize),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tempo {
    Slow,
    Normal,
    Fast,
}

impl Tempo {
    pub fn label(self) -> &'static str {
        match self {
            Tempo::Slow => "a slow pace",
            Tempo::Normal => "an ordinary pace",
            Tempo::Fast => "a fast pace",
        }
    }
}

/// The bench changed how the team plays.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameChange {
    /// 1..
    pub period: u8,
    /// Seconds left in the period.
    pub clock: u16,
    /// Their lead (negative: behind) when they changed.
    pub margin: i16,
    pub offense: OffenseSet,
    pub defense: DefenseScheme,
    pub press: Press,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trait {
    ShootsThrees,
    FinishesInside,
    WinsBoards,
    TakesCare,
    DrawsFouls,
    ForcesTurnovers,
    StopsShots,
    MovesTheBall,
    ColdOutside,
    TurnoverProne,
    SoftOnBoards,
    LeaksPoints,
    FoulProne,
    ThinBench,
    FadesLate,
}

impl Trait {
    pub fn label(self) -> &'static str {
        match self {
            Trait::ShootsThrees => "shoot well from three",
            Trait::FinishesInside => "score easily at the rim",
            Trait::WinsBoards => "win the boards",
            Trait::TakesCare => "take care of the ball",
            Trait::DrawsFouls => "get to the free-throw line",
            Trait::ForcesTurnovers => "force turnovers",
            Trait::StopsShots => "make shots hard to take",
            Trait::MovesTheBall => "move the ball, many assisted baskets",
            Trait::ColdOutside => "cannot shoot from outside",
            Trait::TurnoverProne => "give the ball away",
            Trait::SoftOnBoards => "lose the boards",
            Trait::LeaksPoints => "let shooters get good looks",
            Trait::FoulProne => "foul a lot",
            Trait::ThinBench => "play few players, the starters carry it",
            Trait::FadesLate => "ran out of legs late",
        }
    }
}

/// A man's form on the day of the report.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Form {
    Hot,
    Cold,
}

/// What the scout wrote down about one club after one game. Fields the
/// scout could not make out are `None` or empty; fields he got wrong look
/// the same as right ones, which is why `confidence` is on the report.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpponentReport {
    pub id: u32,
    /// The club the report is about.
    pub team: TeamId,
    pub season: u16,
    /// The day of the game he watched.
    pub day: u32,
    /// Who they played, and whether they were at home.
    pub vs: TeamId,
    pub home: bool,
    /// Their points first.
    pub score: [u16; 2],
    pub friendly: bool,
    /// Games of this club the scout has now watched, this season.
    pub watched: u8,
    /// The scout's judgement rating (1..=20) when he wrote it.
    pub rating: u8,
    /// 0..=100: how far to trust it.
    pub confidence: u8,
    pub offense: Option<OffenseSet>,
    pub defense: Option<DefenseScheme>,
    pub press: Option<Press>,
    /// Share of the game each system ran, percent, biggest first.
    pub offense_mix: Vec<(OffenseSet, u8)>,
    pub defense_mix: Vec<(DefenseScheme, u8)>,
    /// Share of the game with a press on, percent.
    pub press_pct: u8,
    pub changes: Vec<GameChange>,
    pub tempo: Option<Tempo>,
    pub possessions: u16,
    /// The men the offense goes through, with their share of the shots.
    pub through: Vec<(PlayerId, u8)>,
    /// Where the shots came from, percent at the rim, mid-range, three.
    pub shot_mix: Option<[u8; 3]>,
    /// The two commonest ways a shot came about, percent of shots.
    pub sources: Vec<(Source, u8)>,
    /// Who played and for how many minutes, most first.
    pub rotation: Vec<(PlayerId, u8)>,
    pub injured: Vec<(PlayerId, u8)>,
    pub form: Vec<(PlayerId, Form)>,
    pub strengths: Vec<Trait>,
    pub weaknesses: Vec<Trait>,
}

impl OpponentReport {
    /// "high", "fair" or "low".
    pub fn confidence_label(&self) -> &'static str {
        match self.confidence {
            65.. => "high",
            35..=64 => "fair",
            _ => "low",
        }
    }
}

/// League averages a game is judged against.
struct Means {
    fg: f32,
    tp: f32,
    three_share: f32,
    ftr: f32,
    tov: f32,
    ast: f32,
    possessions: f32,
}

/// Add `dt` to the entry for `k`.
fn tally<K: Copy + PartialEq>(v: &mut Vec<(K, f32)>, k: K, dt: f32) {
    match v.iter_mut().find(|e| e.0 == k) {
        Some(e) => e.1 += dt,
        None => v.push((k, dt)),
    }
}

/// Time shares as whole percents, biggest first, none that round to zero.
fn percents<K: Copy>(v: &[(K, f32)], total: f32) -> Vec<(K, u8)> {
    let mut out: Vec<(K, u8)> = v
        .iter()
        .map(|&(k, t)| (k, (t / total * 100.0).round().min(100.0) as u8))
        .filter(|e| e.1 > 0)
        .collect();
    out.sort_by_key(|e| std::cmp::Reverse(e.1));
    out
}

fn ratio(a: u16, b: u16) -> f32 {
    a as f32 / b.max(1) as f32
}

fn turnover_rate(l: &BoxLine) -> f32 {
    l.tov as f32 / (l.fga as f32 + 0.44 * l.fta as f32 + l.tov as f32).max(1.0)
}

impl World {
    // ---- assignments -------------------------------------------------------

    /// The game `team` plays on `day`, if there is one still to play.
    pub fn game_of(&self, team: TeamId, day: u32) -> Option<(GameRef, TeamId, TeamId)> {
        for (i, f) in self.fixtures.iter().enumerate() {
            if f.day == day && f.result.is_none() && (f.home == team || f.away == team) {
                return Some((GameRef::League(i), f.home, f.away));
            }
        }
        for (i, f) in self.friendlies.iter().enumerate() {
            if f.day == day && f.result.is_none() && (f.home == team || f.away == team) {
                return Some((GameRef::Friendly(i), f.home, f.away));
            }
        }
        None
    }

    /// Where the scout is to be on `day`.
    pub fn assignment_on(&self, day: u32) -> Option<ScoutAssignment> {
        self.scouting
            .assignments
            .iter()
            .copied()
            .find(|a| a.day == day)
    }

    /// Is the scout sent to either side of the game `home` v `away` on
    /// `day`?
    pub(crate) fn scout_wants(&self, day: u32, home: TeamId, away: TeamId) -> bool {
        self.watch.is_some()
            && self
                .scouting
                .assignments
                .iter()
                .any(|a| a.day == day && (a.team == home || a.team == away))
    }

    /// May the scout do his player scouting at a game on `day`? Not if he
    /// is at a different game that day.
    pub(crate) fn scout_free_for(&self, day: u32, home: TeamId, away: TeamId) -> bool {
        match self.assignment_on(day) {
            None => true,
            Some(a) => a.team == home || a.team == away,
        }
    }

    /// Send the chief scout to watch `team` play on `day`, replacing what
    /// he was to do that day.
    pub(crate) fn assign_scout(&mut self, team: TeamId, day: u32) -> Result<String, String> {
        let me = self.watch.ok_or("Nobody is in charge.")?;
        if self.teams[me].office.staffer(Job::Scout).is_none() {
            return Err("You have no chief scout. Hire one on the Staff page.".into());
        }
        if team >= self.teams.len() || team == me {
            return Err("Pick another club to scout.".into());
        }
        if day < self.day {
            return Err("That day is past.".into());
        }
        let Some((_, home, away)) = self.game_of(team, day) else {
            return Err(format!(
                "{} has no game to play on {}.",
                self.teams[team].name,
                self.day_label(day)
            ));
        };
        if home == me || away == me {
            return Err("That is our own game.".into());
        }
        let replaced = self.assignment_on(day);
        self.scouting.assignments.retain(|a| a.day != day);
        self.scouting.assignments.push(ScoutAssignment { day, team });
        self.scouting.assignments.sort_by_key(|a| a.day);
        Ok(match replaced {
            Some(old) if old.team != team => format!(
                "The scout goes to {}'s game on {} instead of {}'s.",
                self.teams[team].name,
                self.day_label(day),
                self.teams[old.team].name
            ),
            _ => format!(
                "The scout will watch {} on {}.",
                self.teams[team].name,
                self.day_label(day)
            ),
        })
    }

    pub(crate) fn unassign_scout(&mut self, day: u32) -> Result<(), String> {
        let n = self.scouting.assignments.len();
        self.scouting.assignments.retain(|a| a.day != day);
        if self.scouting.assignments.len() == n {
            Err("The scout has nothing to do that day.".into())
        } else {
            Ok(())
        }
    }

    /// Assignments whose day has gone by without the game.
    pub(crate) fn drop_stale_assignments(&mut self) {
        let day = self.day;
        self.scouting.assignments.retain(|a| a.day >= day);
    }

    // ---- reports -------------------------------------------------------------

    /// Reports on `team`, newest first.
    pub fn reports_on(&self, team: TeamId) -> Vec<&OpponentReport> {
        let mut v: Vec<&OpponentReport> = self
            .scouting
            .opponent_reports
            .iter()
            .filter(|r| r.team == team)
            .collect();
        v.sort_by_key(|r| std::cmp::Reverse((r.season, r.day, r.id)));
        v
    }

    /// The newest report on `team`, if there is one.
    pub fn latest_report(&self, team: TeamId) -> Option<&OpponentReport> {
        self.reports_on(team).into_iter().next()
    }

    /// How many of `team`'s games (friendlies too) the scout has watched
    /// this season.
    pub fn games_watched(&self, team: TeamId) -> u8 {
        self.scouting
            .opponent_reports
            .iter()
            .filter(|r| r.team == team && r.season == self.season)
            .count()
            .min(255) as u8
    }

    /// Upcoming games of `team` the scout could still watch, `(day,
    /// opponent, team is at home)`: from today, not against us.
    pub fn watchable_games(&self, team: TeamId) -> Vec<(u32, TeamId, bool)> {
        let Some(me) = self.watch else {
            return Vec::new();
        };
        let mut out: Vec<(u32, TeamId, bool)> = Vec::new();
        let league = self
            .fixtures
            .iter()
            .map(|f| (f.day, f.home, f.away, f.result.is_none()));
        let friendly = self
            .friendlies
            .iter()
            .map(|f| (f.day, f.home, f.away, f.result.is_none()));
        for (day, home, away, open) in league.chain(friendly) {
            if open && day >= self.day && (home == team || away == team) && home != me && away != me
            {
                out.push((day, if home == team { away } else { home }, home == team));
            }
        }
        out.sort();
        out
    }

    fn means(&self) -> Means {
        let t = self.league_totals();
        if t.games < 24 {
            return Means {
                fg: 0.46,
                tp: 0.35,
                three_share: 0.36,
                ftr: 0.22,
                tov: 0.14,
                ast: 0.55,
                possessions: 74.0,
            };
        }
        let o = &t.own;
        let g = t.games as f32;
        let used = o.fga as f32 + 0.44 * o.fta as f32 + o.tov as f32;
        Means {
            fg: ratio(o.fgm, o.fga),
            tp: ratio(o.tpm, o.tpa),
            three_share: ratio(o.tpa, o.fga),
            ftr: ratio(o.fta, o.fga),
            tov: o.tov as f32 / used.max(1.0),
            ast: ratio(o.ast, o.fgm),
            possessions: ((used - o.orb as f32) / g).max(40.0),
        }
    }

    /// The league's other clubs now and then change how they play: a new
    /// defense or a new offense for the coming weeks. It costs them
    /// familiarity like any change, and it makes an old report wrong.
    pub(crate) fn ai_tactics_drift(&mut self) {
        if !self.calendar_game() {
            return;
        }
        for t in 0..self.teams.len() {
            // Decided by a hash of the club and the week, not the world's
            // stream, so a quiet week draws nothing.
            let salt = (self.season as u32) * 1000 + self.round as u32;
            let u = |k: u32| (crate::scouting::skew(t as u32 + 7919 * k, salt) + 1.0) / 2.0;
            if Some(t) == self.watch || u(1) >= DRIFT_CHANCE * 1.0 {
                continue;
            }
            let pick = ((u(2) * 5.0) as usize).min(4);
            let which = u(3) < 0.5;
            let team = &mut self.teams[t];
            let active = team.active;
            let tactics = &mut team.playbook[active].tactics;
            if which {
                let new = OffenseSet::ALL[pick];
                tactics.offense = if new == tactics.offense {
                    OffenseSet::ALL[(pick + 1) % 5]
                } else {
                    new
                };
            } else {
                let new = DefenseScheme::ALL[pick];
                tactics.defense = if new == tactics.defense {
                    DefenseScheme::ALL[(pick + 1) % 5]
                } else {
                    new
                };
            }
        }
    }

    /// How far to trust a scout of `rating` after `watched` games of a
    /// club, 0..=100.
    pub fn report_confidence(rating: u8, watched: u8) -> u8 {
        (rating.min(20) as u32 * 3 + watched.min(5) as u32 * 8).min(100) as u8
    }

    /// Chance that one claim of a report is right.
    fn claim_odds(confidence: u8) -> f32 {
        0.45 + confidence as f32 / 100.0 * 0.5
    }

    /// Roll a claim: right, or not seen, or wrong.
    fn claim<T: Copy>(&mut self, truth: T, p: f32, wrong: impl Fn(T) -> T) -> Option<T> {
        if self.rng.chance(p) {
            Some(truth)
        } else if self.rng.chance(0.5) {
            None
        } else {
            Some(wrong(truth))
        }
    }

    /// A number written down with the error a poor look brings.
    fn blur(&mut self, value: i32, confidence: u8, scale: i32) -> i32 {
        let width = (100 - confidence as i32) * scale / 100;
        if width <= 0 {
            return value;
        }
        value + self.rng.range(0, 2 * width as u32 + 1) as i32 - width
    }

    /// A game the scout was sent to has been played: file the report and
    /// send the mail. `rec` is the recording made for him.
    pub(crate) fn scout_game(
        &mut self,
        day: u32,
        home: TeamId,
        away: TeamId,
        friendly: bool,
        result: &GameResult,
        rec: &Recording,
    ) {
        let Some(me) = self.watch else { return };
        let Some(a) = self
            .scouting
            .assignments
            .iter()
            .copied()
            .find(|a| a.day == day && (a.team == home || a.team == away))
        else {
            return;
        };
        self.scouting.assignments.retain(|x| *x != a);
        if home == me || away == me {
            return;
        }
        let rating = self.teams[me].office.rating(Job::Scout);
        let team = a.team;
        let side = if team == home { 0 } else { 1 };
        let vs = [away, home][side];
        let watched = self.games_watched(team).saturating_add(1);
        let confidence = Self::report_confidence(rating, watched.max(1));
        let p = Self::claim_odds(confidence);
        let means = self.means();

        let lines = &result.boxes[side];
        let mut own = BoxLine::default();
        for (_, l) in lines {
            own.add(l);
        }
        let mut opp = BoxLine::default();
        for (_, l) in &result.boxes[1 - side] {
            opp.add(l);
        }
        let poss = (result.possessions[0] + result.possessions[1]) / 2;

        // What the club actually ran, over the game.
        let tactics = self.teams[team].tactics().clone();
        let end = rec.frames.last().map_or(1.0, |f| f.time).max(1.0);
        let mut offense_t: Vec<(OffenseSet, f32)> = Vec::new();
        let mut defense_t: Vec<(DefenseScheme, f32)> = Vec::new();
        let (mut cur_o, mut cur_d, mut cur_p) = (tactics.offense, tactics.defense, tactics.press);
        let mut press_t = 0.0f32;
        let mut last = 0.0f32;
        let mut changes = Vec::new();
        let mut zones = [0u32; 3];
        let mut sources: Vec<(Source, f32)> = Vec::new();
        for e in &rec.events {
            match &e.kind {
                EventKind::Adjust {
                    side: s,
                    offense,
                    defense,
                    press,
                } if *s as usize == side => {
                    let dt = (e.time - last).max(0.0);
                    tally(&mut offense_t, cur_o, dt);
                    tally(&mut defense_t, cur_d, dt);
                    if cur_p != Press::None {
                        press_t += dt;
                    }
                    last = e.time;
                    (cur_o, cur_d, cur_p) = (*offense, *defense, *press);
                    changes.push(GameChange {
                        period: e.period + 1,
                        clock: e.clock.max(0.0) as u16,
                        margin: e.score[side] as i16 - e.score[1 - side] as i16,
                        offense: *offense,
                        defense: *defense,
                        press: *press,
                    });
                }
                EventKind::Shot { kind, source, .. } if e.offense as usize == side => {
                    zones[match kind {
                        ShotKind::Rim => 0,
                        ShotKind::Mid => 1,
                        ShotKind::Three => 2,
                    }] += 1;
                    tally(&mut sources, *source, 1.0);
                }
                _ => {}
            }
        }
        let dt = (end - last).max(0.0);
        tally(&mut offense_t, cur_o, dt);
        tally(&mut defense_t, cur_d, dt);
        if cur_p != Press::None {
            press_t += dt;
        }
        let offense_mix = percents(&offense_t, end);
        let defense_mix = percents(&defense_t, end);
        let press_pct = (press_t / end * 100.0).round().min(100.0) as u8;

        // The claims: the system he started with, not the changes.
        let offense = self.claim(tactics.offense, p, |o| match o {
            OffenseSet::Motion => OffenseSet::PickAndRoll,
            OffenseSet::PickAndRoll => OffenseSet::Motion,
            OffenseSet::PostUp => OffenseSet::Isolation,
            OffenseSet::Isolation => OffenseSet::PostUp,
            OffenseSet::FiveOut => OffenseSet::Motion,
        });
        let defense = self.claim(tactics.defense, p, |d| match d {
            DefenseScheme::ManToMan => DefenseScheme::PackLine,
            DefenseScheme::SwitchEverything => DefenseScheme::ManToMan,
            DefenseScheme::Zone23 => DefenseScheme::PackLine,
            DefenseScheme::Zone32 => DefenseScheme::Zone23,
            DefenseScheme::PackLine => DefenseScheme::Zone23,
        });
        let press = self.claim(tactics.press, p, |x| match x {
            Press::None => Press::ThreeQuarter,
            Press::ThreeQuarter => Press::None,
            Press::FullCourt => Press::ThreeQuarter,
        });

        let pace = (poss as f32 / means.possessions * 100.0).round() as i32;
        let true_tempo = if pace >= 106 {
            Tempo::Fast
        } else if pace <= 94 {
            Tempo::Slow
        } else {
            Tempo::Normal
        };
        let tempo = self.claim(true_tempo, p, |t| match t {
            Tempo::Normal => Tempo::Fast,
            _ => Tempo::Normal,
        });

        // Who the offense goes through: by shots taken and drawn.
        let used = |l: &BoxLine| l.fga as f32 + 0.44 * l.fta as f32;
        let total_use: f32 = lines.iter().map(|(_, l)| used(l)).sum::<f32>().max(1.0);
        let mut users: Vec<(PlayerId, f32)> = lines
            .iter()
            .map(|(pid, l)| (*pid, used(l) / total_use))
            .collect();
        users.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut through = Vec::new();
        for (k, &(pid, share)) in users.iter().take(3).enumerate() {
            if share < 0.14 && k > 0 {
                break;
            }
            let shown = self
                .blur((share * 100.0).round() as i32, confidence, 10)
                .clamp(1, 100) as u8;
            if users.len() < 5 || self.rng.chance(p) {
                through.push((pid, shown));
            } else {
                // Took somebody else for the man.
                let other = 3 + self.rng.range(0, (users.len() - 3) as u32) as usize;
                through.push((users[other].0, shown));
            }
        }

        let shots: u32 = zones.iter().sum();
        let shot_mix = (shots >= 20 && self.rng.chance(0.55 + 0.45 * p)).then(|| {
            let mut m = [0u8; 3];
            for (k, n) in zones.iter().enumerate() {
                let v = (*n as f32 / shots as f32 * 100.0).round() as i32;
                m[k] = self.blur(v, confidence, 14).clamp(0, 100) as u8;
            }
            m
        });
        let sources: Vec<(Source, u8)> = {
            let mut s = sources;
            s.sort_by(|a, b| b.1.total_cmp(&a.1));
            s.into_iter()
                .take(2)
                .map(|(src, n)| (src, (n / shots.max(1) as f32 * 100.0).round() as u8))
                .collect()
        };

        // The rotation.
        let mut rotation: Vec<(PlayerId, u8)> = lines
            .iter()
            .filter(|(_, l)| l.secs >= 120)
            .map(|(pid, l)| (*pid, (l.secs as f32 / 60.0).round() as u8))
            .collect();
        rotation.sort_by_key(|r| std::cmp::Reverse(r.1));
        let used_men = rotation.len();
        let rotation: Vec<(PlayerId, u8)> = rotation
            .into_iter()
            .map(|(pid, m)| (pid, self.blur(m as i32, confidence, 4).clamp(1, 48) as u8))
            .collect();

        // Who is hurt, who is flying.
        let mut injured = Vec::new();
        let mut form = Vec::new();
        for &pid in &self.teams[team].roster {
            let pl = &self.players[pid as usize];
            if let Some(i) = &pl.injury {
                injured.push((pid, i.weeks_left));
            } else if pl.morale > 0.68 && pl.sharp > 0.9 {
                form.push((pid, Form::Hot));
            } else if pl.morale < 0.38 || pl.condition < 0.78 {
                form.push((pid, Form::Cold));
            }
        }
        form.truncate(4);
        // A poor look misses some of them.
        injured.retain(|_| self.rng.chance(0.4 + 0.6 * p));
        form.retain(|_| self.rng.chance(0.3 + 0.7 * p));

        let (strengths, weaknesses) = self.traits(&own, &opp, result, side, &means, used_men);

        let id = self
            .scouting
            .opponent_reports
            .iter()
            .map(|r| r.id + 1)
            .max()
            .unwrap_or(0);
        let report = OpponentReport {
            id,
            team,
            season: self.season,
            day,
            vs,
            home: side == 0,
            score: [result.score[side], result.score[1 - side]],
            friendly,
            watched,
            rating,
            confidence,
            offense,
            defense,
            press,
            offense_mix,
            defense_mix,
            press_pct,
            changes,
            tempo,
            possessions: poss,
            through,
            shot_mix,
            sources,
            rotation,
            injured,
            form,
            strengths,
            weaknesses,
        };
        let players: Vec<PlayerId> = report.through.iter().map(|t| t.0).collect();
        let subject = format!(
            "Scout report: {} ({})",
            self.teams[team].name,
            self.day_label(day)
        );
        let body = self.report_text(&report);
        self.scouting.opponent_reports.push(report);
        if self.scouting.opponent_reports.len() > KEEP {
            self.scouting.opponent_reports.remove(0);
        }
        self.post(day, Kind::Scouting, "Chief scout", subject, body, players);
    }

    /// Strengths and weaknesses a game showed, against the league's means.
    fn traits(
        &self,
        own: &BoxLine,
        opp: &BoxLine,
        result: &GameResult,
        side: usize,
        m: &Means,
        used: usize,
    ) -> (Vec<Trait>, Vec<Trait>) {
        let (mut good, mut bad) = (Vec::new(), Vec::new());
        let tov = turnover_rate(own);
        let opp_tov = turnover_rate(opp);
        let boards = (own.orb + own.drb) as f32
            / ((own.orb + own.drb + opp.orb + opp.drb).max(1)) as f32;
        if own.tpa >= 12 && ratio(own.tpm, own.tpa) >= m.tp + 0.05 {
            good.push(Trait::ShootsThrees);
        } else if own.tpa >= 12 && ratio(own.tpm, own.tpa) <= m.tp - 0.06 {
            bad.push(Trait::ColdOutside);
        }
        if ratio(own.fgm, own.fga) >= m.fg + 0.05 && ratio(own.tpa, own.fga) <= m.three_share {
            good.push(Trait::FinishesInside);
        }
        if boards >= 0.53 {
            good.push(Trait::WinsBoards);
        } else if boards <= 0.47 {
            bad.push(Trait::SoftOnBoards);
        }
        if tov <= m.tov - 0.025 {
            good.push(Trait::TakesCare);
        } else if tov >= m.tov + 0.025 {
            bad.push(Trait::TurnoverProne);
        }
        if ratio(own.fta, own.fga) >= m.ftr + 0.08 {
            good.push(Trait::DrawsFouls);
        }
        if opp_tov >= m.tov + 0.03 {
            good.push(Trait::ForcesTurnovers);
        }
        if ratio(opp.fgm, opp.fga) <= m.fg - 0.05 {
            good.push(Trait::StopsShots);
        } else if ratio(opp.fgm, opp.fga) >= m.fg + 0.06 {
            bad.push(Trait::LeaksPoints);
        }
        if ratio(own.ast, own.fgm) >= m.ast + 0.12 {
            good.push(Trait::MovesTheBall);
        }
        if own.pf >= 25 {
            bad.push(Trait::FoulProne);
        }
        if used <= 7 {
            bad.push(Trait::ThinBench);
        }
        let n = result.periods.len();
        if n >= 4 {
            let diff = |p: &[u16; 2]| p[side] as i32 - p[1 - side] as i32;
            if diff(&result.periods[0]) - diff(&result.periods[n - 1]) >= 10 {
                bad.push(Trait::FadesLate);
            }
        }
        good.truncate(3);
        bad.truncate(3);
        (good, bad)
    }

    /// The report in words: what it says, how far to trust it and what to
    /// do with it.
    pub fn report_text(&self, r: &OpponentReport) -> String {
        let name = |p: PlayerId| self.players[p as usize].name.clone();
        let team = &self.teams[r.team].name;
        let vs = &self.teams[r.vs].name;
        let mut s = format!(
            "Our scout watched {team} {} {vs} on {}. {} {}-{}.{}\n",
            if r.home { "at home to" } else { "away at" },
            self.day_long(r.day),
            if r.score[0] > r.score[1] {
                "They won"
            } else {
                "They lost"
            },
            r.score[0],
            r.score[1],
            if r.friendly { " (a friendly)" } else { "" },
        );
        s += &format!(
            "\nHow far to trust this: {}. This is game {} of theirs he has seen this season, and \
             he is a {} judge of a team.{}\n",
            r.confidence_label(),
            r.watched,
            match r.rating {
                0..=7 => "poor",
                8..=13 => "decent",
                _ => "good",
            },
            if r.confidence < 65 {
                " Some of it may be wrong; another look would firm it up."
            } else {
                ""
            },
        );
        s += "\nHow they play\n";
        match r.offense {
            Some(o) => {
                s += &format!("- Offense: {}", o.label().to_lowercase());
                if let Some(&(m, pct)) = r.offense_mix.first()
                    && m == o
                    && pct < 100
                {
                    s += &format!(" for {pct}% of the game");
                }
                s += ".\n";
            }
            None => s += "- He could not tell what they run on offense.\n",
        }
        match r.defense {
            Some(d) => {
                s += &format!("- Defense: {}", d.label().to_lowercase());
                if let Some(&(m, pct)) = r.defense_mix.first()
                    && m == d
                    && pct < 100
                {
                    s += &format!(" for {pct}% of the game");
                }
                s += ".\n";
            }
            None => s += "- He could not tell what they defend.\n",
        }
        match r.press {
            Some(Press::None) => s += "- No press.\n",
            Some(p) => s += &format!(
                "- Press: {}{}.\n",
                p.label().to_lowercase(),
                if r.press_pct > 0 && r.press_pct < 100 {
                    format!(" for {}% of the game", r.press_pct)
                } else {
                    String::new()
                }
            ),
            None => {}
        }
        if let Some(t) = r.tempo {
            s += &format!("- Tempo: {} ({} possessions).\n", t.label(), r.possessions);
        }
        if let Some(m) = r.shot_mix {
            s += &format!(
                "- Their shots: about {}% at the rim, {}% mid-range, {}% from three.\n",
                m[0], m[1], m[2]
            );
        }
        if !r.sources.is_empty() {
            let v: Vec<String> = r
                .sources
                .iter()
                .map(|(src, n)| format!("{} ({n}%)", src.label().to_lowercase()))
                .collect();
            s += &format!("- Most of their shots come off: {}.\n", v.join(", "));
        }
        if !r.through.is_empty() {
            let v: Vec<String> = r
                .through
                .iter()
                .map(|(p, n)| format!("{} (about {n}% of the shots)", name(*p)))
                .collect();
            s += &format!("- The offense goes through {}.\n", v.join(", "));
        }
        for c in &r.changes {
            s += &format!(
                "- In period {} with {}:{:02} left, {} by {}, they changed to {} defense{}.\n",
                c.period,
                c.clock / 60,
                c.clock % 60,
                if c.margin < 0 { "behind" } else { "ahead" },
                c.margin.abs(),
                c.defense.label().to_lowercase(),
                match c.press {
                    Press::None => String::new(),
                    p => format!(" with a {}", p.label().to_lowercase()),
                },
            );
        }
        if r.changes.is_empty() {
            s += "- They changed nothing during the game.\n";
        }
        if !r.rotation.is_empty() {
            let v: Vec<String> = r
                .rotation
                .iter()
                .take(9)
                .map(|(p, m)| format!("{} {}", name(*p), m))
                .collect();
            s += &format!("\nRotation, minutes: {}.\n", v.join(", "));
        }
        let pick = |f: Form| -> Vec<String> {
            r.form.iter().filter(|x| x.1 == f).map(|x| name(x.0)).collect()
        };
        let (hot, cold) = (pick(Form::Hot), pick(Form::Cold));
        if !hot.is_empty() {
            s += &format!("In form: {}.\n", hot.join(", "));
        }
        if !cold.is_empty() {
            s += &format!("Out of sorts: {}.\n", cold.join(", "));
        }
        if !r.injured.is_empty() {
            let v: Vec<String> = r
                .injured
                .iter()
                .map(|(p, w)| format!("{} (about {} weeks)", name(*p), w))
                .collect();
            s += &format!("Injured: {}.\n", v.join(", "));
        }
        if !r.strengths.is_empty() {
            let v: Vec<&str> = r.strengths.iter().map(|t| t.label()).collect();
            s += &format!("\nStrengths: they {}.\n", v.join("; they "));
        }
        if !r.weaknesses.is_empty() {
            let v: Vec<&str> = r.weaknesses.iter().map(|t| t.label()).collect();
            s += &format!("Weaknesses: they {}.\n", v.join("; they "));
        }
        if let (Some(o), Some(d)) = (r.offense, r.defense) {
            s += &format!(
                "\nWhat it means: {}\n",
                crate::inbox::hint(o, d, r.press.unwrap_or(Press::None))
            );
        }
        s += "\nClubs change their tactics now and then, so check the date before relying on \
              this. To use it, set match preparation sessions in the week of the game \
              (Training), and look at the Tactics screen for how to meet what they play.";
        s
    }

    /// A line for the assistant's mail about a club: is there a report?
    pub(crate) fn report_pointer(&self, team: TeamId) -> String {
        match self.latest_report(team) {
            Some(r) => format!(
                "The scout's report from {} is in the inbox (confidence {}).",
                self.day_label(r.day),
                r.confidence_label()
            ),
            None => "No scout has seen them play yet. Send him to one of their games from the \
                     agenda and he will report on how they play."
                .to_string(),
        }
    }
}
