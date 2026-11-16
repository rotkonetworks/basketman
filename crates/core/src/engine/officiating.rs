//! The officials: three people on the floor who see what happened through
//! their own eyes and decide what is called.
//!
//! The engine keeps computing the truth as before (contact, who was set,
//! where the ball went out, where the shooter stood). This file sits between
//! the truth and the whistle: the official with the best view of a play may
//! see it, miss it, whistle something that was legal, call a charge a block,
//! give the ball the wrong way or miscount a toe on the line. How often
//! depends on who he is (`officials.rs`), how far he is and how crowded the
//! play is, the crowd, the moment and his own earlier calls.
//!
//! Every decision is recorded with what really happened beside it, and the
//! ones a coach may ask to have looked at stay open until play resumes.
//!
//! A child module of `floor` so it can see the game's private state. All
//! arithmetic is plain float and integer: no `powf`, `exp`, `ln` or trig.
//! The officials' dice come from their own stream (`Ofc::rng`), seeded from
//! the game's, so adding or removing a hook never moves the game's.

use super::*;
use crate::court::{CORNER_X, CORNER_Y, THREE_RADIUS};
use crate::officials::{
    CallRecord, Crew, GameOfficiating, Opinion, RefRecord, Review, ReviewPrompt, Ruling, What,
};

/// How often an average official is wrong about a play an average
/// distance away: the share of fouls he fails to see. Phantom whistles are
/// scaled to match, so a crew's errors do not move the league's foul totals.
const ERR_BASE: f32 = 0.033;
/// The chance per legal play at a foul site that he whistles it anyway,
/// for the same viewing conditions.
const PHANTOM_BASE: f32 = 0.0011;
/// Contact below this share of the neutral threshold is a foul by any
/// reading; above 1.15 times it is legal; in between is judgement.
const CLEAR: f32 = 0.7;
/// A charge/block decision is harder than a plain foul by this much.
const KIND_HARD: f32 = 2.4;
/// Ball out of bounds: who touched it last.
const TOUCH_HARD: f32 = 1.6;
/// A shot within this many metres of the three-point line may be miscounted.
const TOE_BAND: f32 = 0.22;
/// Chance of a wrong count for a shot on the line, before the official's
/// own factors.
const TOE_BASE: f32 = 0.30;
/// The most a loud home crowd shifts a whistle (a share of its threshold)
/// for an official who is all lean.
const LEAN_SHIFT: f32 = 0.16;
/// Late in a game the side that is behind fouls to stop the clock: the
/// severity of the contact it makes on the ball handler.
const HACK_SEV: f32 = 40.0;
/// The visitors have not played in this hall: how much less sure they are of
/// the floor, the rims and the backdrop, as the unfamiliarity of a set, for a
/// full and loud house.
const HALL: f32 = 0.3;
/// How much a loud house lifts the home five and rattles the visitors, as
/// concentration (see `Slot::focus`).
const HOUSE: f32 = 0.5;
/// How much more often a shooter is fouled than the contest alone makes him
/// (`foul_chance`): a defender who hits a man in the act, in a crowded paint,
/// is whistled more than a simple count of contested shots says. Fitted so
/// that shooting fouls are about half of all fouls without losing the
/// free-throw rate (see `docs/realism.md`, "League shape").
pub(super) const SHOOTING_FOULS: f32 = 1.10;
/// Seconds a whistle is shown in the replay and the official's arm is up.
const SIGNAL: f32 = 1.0;
/// How fast the officials run, m/s.
const RUN: f32 = 4.2;
/// A window that has not reached its stoppage by then is dropped.
const STALE: f32 = 14.0;

/// How a challenge was settled when the film was looked at, and how the
/// crew chief's quality bears on a marginal play.
const REVIEW_SLIP: f32 = 0.05;

/// Roles of the three, in the standard mechanics.
const LEAD: u8 = 0;
const TRAIL: u8 = 1;
const CENTRE: u8 = 2;

/// How a call is taken back.
#[derive(Clone, Debug)]
enum Undo {
    /// A foul on the defense, with what it set up.
    Defensive {
        side: usize,
        slot: usize,
        /// Court index of the fouled attacker.
        h: usize,
        kind: FoulKind,
        shots: u8,
        /// A shot in the air was fouled: whether it was made, a three,
        /// and whether it was entered in the book as a try.
        shooting: Option<(bool, bool)>,
        holder: Option<usize>,
        ball: Vec2,
        ball_h: f32,
    },
    /// A foul on the offense (a charge, a screen), booked with a turnover.
    Offensive { side: usize, slot: usize, k: usize, dk: usize, kind: FoulKind },
    /// The ball went out and the call gave it one way: where it went out.
    Ball { at: Vec2, z: f32, attackers: bool },
    /// A made basket counted for `called` points when `truth` were right.
    Count { side: usize, slot: usize, called: u8, truth: u8 },
}

struct Window {
    /// Index of the call.
    idx: usize,
    /// The side that may ask.
    side: u8,
    /// The stoppage has been reached.
    ripe: bool,
    /// The person on the bench has been asked.
    offered: bool,
    since: f32,
    undo: Undo,
}

/// What the gate wants the step to do.
pub(super) enum Gate {
    Go,
    Pause(Pause),
}

pub(super) struct Ofc {
    crew: Crew,
    rng: Rng,
    /// Where the three stand, in court coordinates, and their roles.
    pub(super) pos: [Vec2; 3],
    roles: [u8; 3],
    frame: usize,
    /// The official who has just whistled, and until when (anim time).
    pub(super) whistler: Option<(u8, f32)>,
    /// How each is leaning on each kind of foul tonight.
    mood: [[f32; 8]; 3],
    pub(super) calls: Vec<CallRecord>,
    errors: [u16; 5],
    swing: [f32; 2],
    per: [RefRecord; 3],
    fouls: [u16; 2],
    decisions: u16,
    /// Decisions on the ball and the count.
    others: u16,
    /// Challenges each side has left.
    pub(super) left: [u8; 2],
    windows: Vec<Window>,
    /// A fouled-out man waits on the floor until his foul has been looked at.
    out: Vec<(usize, usize)>,
    /// The possession must end now with this.
    pub(super) forced: Option<End>,
    /// Index of the whistle made in this tick, to be booked.
    last: Option<usize>,
    challenge: [Option<CallRecord>; 2],
    pub(super) prompt: Option<ReviewPrompt>,
    /// The shot being fouled: made, and a three.
    shot: Option<(bool, bool)>,
}

impl Ofc {
    pub(super) fn new(crew: Crew, game_rng: &mut Rng) -> Ofc {
        // One draw from the game's stream seeds the officials'.
        let rng = Rng::new(game_rng.next_u64() ^ 0x0FF1_C1A1);
        Ofc {
            crew,
            rng,
            pos: [v(14.0, 2.0), v(14.0, 13.0), v(14.0, 7.5)],
            roles: [CENTRE, LEAD, TRAIL],
            frame: 0,
            whistler: None,
            mood: [[0.0; 8]; 3],
            calls: Vec::new(),
            errors: [0; 5],
            swing: [0.0; 2],
            per: [RefRecord::default(); 3],
            fouls: [0; 2],
            decisions: 0,
            others: 0,
            left: [1, 1],
            windows: Vec::new(),
            out: Vec::new(),
            forced: None,
            last: None,
            challenge: [None, None],
            prompt: None,
            shot: None,
        }
    }

    /// 0.85 for a crew that lets them play to 1.15 for a tight one.
    pub(super) fn tightness(&self) -> f32 {
        WHISTLE_LOOSE + (WHISTLE_TIGHT - WHISTLE_LOOSE) * self.crew.strictness()
    }

    pub(in crate::engine) fn crew(&self) -> &Crew {
        &self.crew
    }
}

/// Distance from `p` to the segment `a`..`b`.
fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    p.dist(p.project(a, b))
}

fn kind_slot(kind: FoulKind) -> usize {
    kind as usize
}

impl Game {
    fn pid(&self, t: usize, k: usize) -> PlayerId {
        let s = &self.sides[t];
        s.slots[s.on[k]].p.id
    }

    /// The crowd is loud and it is the home side's: how hard it leans on a
    /// whistle in 0..=1 for official `i`.
    fn lean_of(&self, i: usize) -> f32 {
        let o = self.ofc.as_ref().expect("officiated").crew();
        o.refs[i].home_lean * o.crowd
    }

    // ---- where the officials stand ---------------------------------------

    /// Move the three towards where the standard mechanics want them. Pure
    /// geometry on the state of the play: no randomness.
    pub(super) fn officials_tick(&mut self) {
        let Some(ofc) = self.ofc.as_mut() else { return };
        if ofc.frame != self.frame {
            // The play has turned round: the lead runs back as the trail and
            // the trail goes ahead as the lead.
            for r in &mut ofc.roles {
                *r = match *r {
                    LEAD => TRAIL,
                    TRAIL => LEAD,
                    c => c,
                }
            }
            ofc.frame = self.frame;
        }
        let ball = self.play.ball;
        let side = if ball.x >= 0.0 { 1.0 } else { -1.0 };
        let f = self.frame;
        let in_play = !matches!(self.play.phase, Phase::Line { .. });
        for i in 0..3 {
            let want = if matches!(self.play.phase, Phase::Line { .. }) {
                // Free throws: lead under the basket, trail at the line's
                // extension, centre at the half-court side.
                match ofc.roles[i] {
                    LEAD => v(3.4, 0.6),
                    TRAIL => v(-3.0, 6.6),
                    _ => v(7.0, 9.0),
                }
            } else {
                match ofc.roles[i] {
                    LEAD => v(side * 6.2, (ball.y - 8.0).clamp(0.6, 14.0)),
                    TRAIL => v(-side * 3.0, (ball.y + 5.5).clamp(5.0, LENGTH - 1.0)),
                    _ => v(-side * 6.8, (ball.y - 1.5).clamp(5.8, 22.0)),
                }
            };
            let target = to_court(want, f);
            let at = ofc.pos[i];
            let d = at.dist(target);
            let step = (RUN * DT).min(d);
            ofc.pos[i] = if d > 1e-4 { at.toward(target, step) } else { at };
        }
        let _ = in_play;
    }

    // ---- seeing a play ----------------------------------------------------

    /// The official with the best look at `at` (attack frame), the distance
    /// to it, and how many bodies stand between.
    fn view(&self, at: Vec2, skip: [(usize, usize); 2]) -> (usize, f32, usize) {
        let ofc = self.ofc.as_ref().expect("officiated");
        let play = to_court(at, self.frame);
        let mut best = (0usize, f32::MAX, 0usize, f32::MAX);
        for i in 0..3 {
            let me = ofc.pos[i];
            let d = me.dist(play);
            let mut blocked = 0;
            for t in 0..2 {
                for k in 0..5 {
                    if skip.contains(&(t, k)) {
                        continue;
                    }
                    let b = to_court(self.m[t][k].pos, self.frame);
                    if b.dist(play) > 0.9 && seg_dist(b, me, play) < 0.45 {
                        blocked += 1;
                    }
                }
            }
            let cost = d + 1.8 * blocked as f32;
            if cost < best.3 {
                best = (i, d, blocked, cost);
            }
        }
        (best.0, best.1, best.2)
    }

    /// How many bodies stand within a body's width or two of `at`.
    fn crowded(&self, at: Vec2, skip: [(usize, usize); 2]) -> usize {
        let mut n = 0;
        for t in 0..2 {
            for k in 0..5 {
                if !skip.contains(&(t, k)) && self.m[t][k].pos.dist(at) < 1.6 {
                    n += 1;
                }
            }
        }
        n
    }

    /// Late in a close game.
    fn big_moment(&self) -> bool {
        let last = self.period + 1 >= self.rules.periods;
        let gap = (self.sides[0].score as i32 - self.sides[1].score as i32).abs();
        last && self.clock < 150.0 && gap <= 6
    }

    /// How much more or less often than an average official on an ordinary
    /// play official `i` is wrong: eyes, distance, bodies in the way, the
    /// press of players, the moment.
    fn muddle(&self, i: usize, dist: f32, blocked: usize, crowd: usize) -> f32 {
        let o = &self.ofc.as_ref().expect("officiated").crew().refs[i];
        let x = (1.5 - 1.1 * o.accuracy) / 0.9;
        let eyes = x * x;
        let far = (0.55 + 0.11 * dist).clamp(0.8, 2.4) * (1.0 + 0.25 * blocked as f32);
        let press = 1.0 + 0.10 * (crowd as f32 - 1.0).max(0.0);
        let moment = if self.big_moment() {
            1.0 + 0.9 * (1.0 - o.composure)
        } else {
            1.0
        };
        eyes * far * press * moment
    }

    /// The threshold of contact, as a share of its severity, below which
    /// official `i` blows his whistle against side `against`: his own
    /// strictness, the mood he is in for this kind of foul, and how the
    /// home crowd leans on him.
    fn blow_at(&self, i: usize, kind: FoulKind, against: usize) -> f32 {
        let ofc = self.ofc.as_ref().expect("officiated");
        let o = &ofc.crew.refs[i];
        let style = 0.85 + 0.30 * o.strictness;
        let mood = 1.0 + ofc.mood[i][kind_slot(kind)];
        // A loud home hall: more fouls on the visitors, fewer on the home.
        let lean = self.lean_of(i) * LEAN_SHIFT * if against == 1 { 1.0 } else { -1.0 };
        style * mood * (1.0 + lean)
    }

    // ---- making a call ----------------------------------------------------

    fn note(
        &mut self,
        what: What,
        truth: Ruling,
        called: Ruling,
        by: usize,
        who: PlayerId,
        on: PlayerId,
        gains: usize,
        swing: f32,
    ) -> usize {
        let rec = CallRecord {
            time: self.wall,
            period: self.period,
            clock: self.clock.max(0.0),
            by: by as u8,
            what,
            truth,
            called,
            who,
            on,
            gains: gains as u8,
            swing,
            reviewable: !matches!(called, Ruling::Nothing),
            review: None,
        };
        let verdict = rec.verdict();
        let ofc = self.ofc.as_mut().expect("officiated");
        if what == What::Foul {
            ofc.decisions += 1;
            ofc.per[by].calls += 1;
        } else {
            ofc.others += 1;
        }
        if verdict.is_wrong() {
            let k = match verdict {
                crate::officials::Verdict::Missed => 0,
                crate::officials::Verdict::Phantom => 1,
                crate::officials::Verdict::WrongFoul => 2,
                crate::officials::Verdict::WrongBall => 3,
                _ => 4,
            };
            ofc.errors[k] += 1;
            ofc.swing[gains.min(1)] += swing;
            ofc.per[by].wrong += 1;
        }
        let idx = ofc.calls.len();
        ofc.calls.push(rec.clone());
        if let Some(r) = &mut self.rec {
            r.calls.push(rec);
        }
        idx
    }

    /// Points a foul of this kind is roughly worth to the side it favours.
    fn foul_worth(&self, kind: FoulKind, fouled_side: usize) -> f32 {
        let bonus = self.sides[1 - fouled_side].team_fouls + 1 > self.rules.bonus_after;
        match kind {
            FoulKind::Shooting => 1.4,
            k if k.is_offensive() => 1.0,
            _ => 0.25 + if bonus { 1.3 } else { 0.0 },
        }
    }

    /// Whether a foul is whistled. `contact` is the truth that there was
    /// illegal contact of the size `sev` (a share of CALL_BASE): a foul
    /// committed by `fouler` (side, court index) on `fouled`. The old rule
    /// when no crew is assigned.
    pub(super) fn call(
        &mut self,
        contact: bool,
        sev: f32,
        kind: FoulKind,
        fouler: (usize, usize),
        fouled: (usize, usize),
    ) -> bool {
        // The side behind, late, hacks the ball handler on purpose.
        let (contact, sev) = if kind == FoulKind::ReachIn && self.hacking(fouler.0) {
            (true, HACK_SEV)
        } else {
            (contact, sev)
        };
        if self.ofc.is_none() {
            return contact && self.called(sev);
        }
        self.ofc.as_mut().expect("officiated").last = None;
        let at = (self.m[fouler.0][fouler.1].pos + self.m[fouled.0][fouled.1].pos) * 0.5;
        let skip = [fouler, fouled];
        let (i, dist, blocked) = self.view(at, skip);
        let crowd = self.crowded(at, skip);
        let muddle = self.muddle(i, dist, blocked, crowd);
        let against = fouler.0;
        let u = if contact { self.rng.f32() } else { 1.0 };
        let t0 = (CALL_BASE * sev).min(0.97);
        let truth = if !contact || u >= t0 * 1.15 {
            Ruling::Nothing
        } else if u < t0 * CLEAR {
            Ruling::Foul { kind, by: against as u8 }
        } else {
            Ruling::Marginal { kind, by: against as u8 }
        };
        let ct = (t0 * self.blow_at(i, kind, against)).min(0.97);
        let e = (ERR_BASE * muddle).clamp(0.003, 0.45);
        let ph = (PHANTOM_BASE * muddle).clamp(0.0005, 0.12);
        let lean = self.lean_of(i) * LEAN_SHIFT * if against == 1 { 1.0 } else { -1.0 };
        let roll = self.ofc.as_mut().expect("officiated").rng.f32();
        let whistle = if contact {
            if u < ct {
                // He would call it. Does he see it?
                roll >= e
            } else {
                // He would let it go; but he may see a foul that is not
                // there.
                roll < ph * (1.0 + lean)
            }
        } else {
            roll < ph * (1.0 + lean)
        };
        self.settle_call(i, kind, truth, whistle, fouler, fouled, contact)
    }

    /// A whistle has blown that is not yet booked: no violation can be
    /// called on a play the crew has stopped.
    pub(super) fn whistled(&self) -> bool {
        self.ofc.as_ref().is_some_and(|o| o.last.is_some())
    }

    /// Is side `t`, on defense, fouling on purpose: a bench of its own that
    /// is behind by a few points in the last two minutes of the game, in the
    /// bonus or inside the last minute? The fewer the seconds, the more
    /// points it thinks it can make up.
    fn hacking(&self, t: usize) -> bool {
        let s = &self.sides[t];
        if s.coached || self.period + 1 < self.rules.periods {
            return false;
        }
        let behind = s.opp_score as i32 - s.score as i32;
        let left = self.clock;
        if behind < 1 || left > 120.0 {
            return false;
        }
        let reach = (3.0 + left / 15.0).min(10.0) as i32;
        behind <= reach && (left <= 60.0 || s.team_fouls >= self.rules.bonus_after)
    }

    /// Book the decision and what it did to the official's mood; returns
    /// whether the whistle blew.
    fn settle_call(
        &mut self,
        i: usize,
        kind: FoulKind,
        truth: Ruling,
        whistle: bool,
        fouler: (usize, usize),
        fouled: (usize, usize),
        contact: bool,
    ) -> bool {
        let against = fouler.0;
        let (who, on) = (self.pid(fouler.0, fouler.1), self.pid(fouled.0, fouled.1));
        let called = if whistle {
            Ruling::Foul { kind, by: against as u8 }
        } else {
            Ruling::Nothing
        };
        // Only whistles and fouls that should have been whistled are
        // worth a record.
        let missed = matches!(truth, Ruling::Foul { .. }) && !whistle;
        if contact {
            let ofc = self.ofc.as_mut().expect("officiated");
            let m = &mut ofc.mood[i][kind_slot(kind)];
            let steady = 1.0 - ofc.crew.refs[i].consistency;
            // Calls beget calls, and a let-go play begets another.
            let step = 0.02 + 0.07 * steady;
            *m = (*m * 0.97 + if whistle { step } else { -step * 0.5 }).clamp(-0.3, 0.3);
        }
        if !whistle && !missed {
            return false;
        }
        let gains = if whistle { 1 - against } else { against };
        let swing = self.foul_worth(kind, 1 - against);
        let idx = self.note(What::Foul, truth, called, i, who, on, gains, swing);
        let anim = self.anim;
        let ofc = self.ofc.as_mut().expect("officiated");
        if whistle {
            ofc.last = Some(idx);
            ofc.whistler = Some((i as u8, anim + SIGNAL));
            ofc.fouls[against] += 1;
            ofc.per[i].fouls += 1;
            if against == 0 {
                ofc.per[i].on_home += 1;
            } else {
                ofc.per[i].on_away += 1;
            }
        }
        whistle
    }

    /// A collision in the driver's path. By the book it is a charge when
    /// `charge`, a block otherwise; `sev` is how much of CALL_BASE the
    /// contact is worth. What is called is returned as a foul kind and the
    /// side it is on, or nothing.
    pub(super) fn call_collision(
        &mut self,
        charge: bool,
        sev: f32,
        h: usize,
        dk: usize,
    ) -> Option<FoulKind> {
        if self.ofc.is_none() {
            return self.called(sev).then_some(if charge {
                FoulKind::Charging
            } else {
                FoulKind::Blocking
            });
        }
        let o = self.play.o;
        let d = 1 - o;
        self.ofc.as_mut().expect("officiated").last = None;
        let (truth_kind, truth_by) = if charge {
            (FoulKind::Charging, o)
        } else {
            (FoulKind::Blocking, d)
        };
        let at = (self.m[o][h].pos + self.m[d][dk].pos) * 0.5;
        let skip = [(o, h), (d, dk)];
        let (i, dist, blocked) = self.view(at, skip);
        let crowd = self.crowded(at, skip);
        let muddle = self.muddle(i, dist, blocked, crowd);
        let u = self.rng.f32();
        let t0 = (CALL_BASE * sev).min(0.97);
        let truth = if u >= t0 * 1.15 {
            Ruling::Nothing
        } else if u < t0 * CLEAR {
            Ruling::Foul { kind: truth_kind, by: truth_by as u8 }
        } else {
            Ruling::Marginal { kind: truth_kind, by: truth_by as u8 }
        };
        let ct = (t0 * self.blow_at(i, truth_kind, truth_by)).min(0.97);
        let e = (ERR_BASE * muddle).clamp(0.003, 0.45);
        let ph = (PHANTOM_BASE * muddle).clamp(0.0005, 0.12);
        let lean_c = self.crew_lean(i);
        let (roll, flip_roll) = {
            let r = &mut self.ofc.as_mut().expect("officiated").rng;
            (r.f32(), r.f32())
        };
        let whistle = if u < ct { roll >= e } else { roll < ph };
        if !whistle {
            let fouler = if charge { (o, h) } else { (d, dk) };
            let fouled = if charge { (d, dk) } else { (o, h) };
            self.settle_call(i, truth_kind, truth, false, fouler, fouled, true);
            return None;
        }
        // He blows it; is it the right way round? How much a man leans to
        // the charge or the block shows here, and how hard it is to see.
        let kind_err = (e * KIND_HARD).clamp(0.01, 0.6);
        let lean = lean_c;
        let toward_charge = self.ofc.as_ref().expect("officiated").crew.refs[i].charge_lean - 0.5;
        let p_flip = if charge {
            kind_err * (1.0 - toward_charge * 1.6)
        } else {
            kind_err * (1.0 + toward_charge * 1.6)
        } * (1.0 + lean);
        let flipped = flip_roll < p_flip.clamp(0.0, 0.8);
        // A whistle on a play that was clean is a phantom, and it goes the
        // way the call was going to go: a charge if the driver was in the
        // wrong place.
        let (kind, by) = if charge ^ flipped {
            (FoulKind::Charging, o)
        } else {
            (FoulKind::Blocking, d)
        };
        let (fouler, fouled) = if by == o { ((o, h), (d, dk)) } else { ((d, dk), (o, h)) };
        let (who, on) = (self.pid(fouler.0, fouler.1), self.pid(fouled.0, fouled.1));
        let called = Ruling::Foul { kind, by: by as u8 };
        let swing = self.foul_worth(kind, 1 - by);
        let idx = self.note(What::Foul, truth, called, i, who, on, 1 - by, swing);
        let anim = self.anim;
        let ofc = self.ofc.as_mut().expect("officiated");
        ofc.last = Some(idx);
        ofc.whistler = Some((i as u8, anim + SIGNAL));
        ofc.fouls[by] += 1;
        ofc.per[i].fouls += 1;
        if by == 0 {
            ofc.per[i].on_home += 1;
        } else {
            ofc.per[i].on_away += 1;
        }
        Some(kind)
    }

    /// How hard the crowd leans on official `i` towards the home side's
    /// favour, as a positive number when the call would favour home.
    fn crew_lean(&self, i: usize) -> f32 {
        self.lean_of(i) * 0.5
    }

    /// A defender who goes down as if hit: does the official buy it?
    /// `FOOLED` is the chance for a middling official in an average mood.
    pub(super) fn buys_flop(&mut self, h: usize, dk: usize) -> bool {
        if self.ofc.is_none() {
            return self.rng.chance(FOOLED * self.whistle);
        }
        let (o, d) = (self.play.o, 1 - self.play.o);
        self.ofc.as_mut().expect("officiated").last = None;
        let at = (self.m[o][h].pos + self.m[d][dk].pos) * 0.5;
        let skip = [(o, h), (d, dk)];
        let (i, dist, blocked) = self.view(at, skip);
        let crowd = self.crowded(at, skip);
        let muddle = self.muddle(i, dist, blocked, crowd);
        let r = &self.ofc.as_ref().expect("officiated").crew.refs[i];
        // A man who buys flops buys them; a sharp pair of eyes does not.
        let p = FOOLED
            * (0.85 + 0.3 * r.strictness)
            * (0.35 + 1.3 * r.flop_buyer)
            * (0.5 + 0.5 * muddle).min(2.0);
        let buy = self.ofc.as_mut().expect("officiated").rng.chance(p.min(0.9));
        if buy {
            let who = self.pid(o, h);
            let on = self.pid(d, dk);
            let truth = Ruling::Nothing;
            let called = Ruling::Foul { kind: FoulKind::Charging, by: o as u8 };
            let swing = self.foul_worth(FoulKind::Charging, d);
            let idx = self.note(What::Foul, truth, called, i, who, on, d, swing);
            let anim = self.anim;
            let ofc = self.ofc.as_mut().expect("officiated");
            ofc.last = Some(idx);
            ofc.whistler = Some((i as u8, anim + SIGNAL));
            ofc.fouls[o] += 1;
            ofc.per[i].fouls += 1;
            if o == 0 {
                ofc.per[i].on_home += 1;
            } else {
                ofc.per[i].on_away += 1;
            }
        }
        buy
    }

    /// The ball went out at `at` (attack frame): by the truth the attackers
    /// touched it last when `def_touch` is false. Returns what is called:
    /// whether the defense touched it.
    pub(super) fn call_touch(&mut self, def_touch: bool, at: Vec2, z: f32) -> bool {
        if self.ofc.is_none() {
            return def_touch;
        }
        let o = self.play.o;
        let (i, dist, blocked) = self.view(at, [(0, 9), (0, 9)]);
        let crowd = self.crowded(at, [(0, 9), (0, 9)]);
        let muddle = self.muddle(i, dist, blocked, crowd);
        let e = (ERR_BASE * TOUCH_HARD * muddle).clamp(0.004, 0.5);
        // The attackers' ball is the home side's if home has it.
        let truth_ball = if def_touch { o } else { 1 - o };
        let lean = self.lean_of(i) * LEAN_SHIFT * 2.0;
        // A flip that gives the ball to home is likelier in a loud home hall.
        let p = e * (1.0 + if truth_ball == 1 { lean } else { -lean });
        let wrong = self.ofc.as_mut().expect("officiated").rng.chance(p.clamp(0.0, 0.7));
        let called_def = def_touch ^ wrong;
        let called_ball = if called_def { o } else { 1 - o };
        let idx = self.note(
            What::OutOfBounds,
            Ruling::Ball(truth_ball as u8),
            Ruling::Ball(called_ball as u8),
            i,
            0,
            0,
            called_ball,
            0.9,
        );
        // The side that lost the ball may ask.
        let side = 1 - called_ball;
        self.open_window(
            idx,
            side,
            true,
            Undo::Ball { at, z, attackers: called_def },
        );
        called_def
    }

    /// A made basket from `at` for a shot of kind `kind` by attacker `h`:
    /// the points it counts. A foot near the line is easy to misjudge.
    pub(super) fn call_count(&mut self, kind: ShotKind, at: Vec2, h: usize, made: bool) -> u16 {
        let truth = kind.points();
        if self.ofc.is_none() || !made || kind == ShotKind::Rim {
            return truth;
        }
        // Distance to the line: the arc, or the straight part in the corner.
        let d = if at.y <= CORNER_Y && at.x.abs() >= CORNER_X - TOE_BAND {
            (at.x.abs() - CORNER_X).abs()
        } else {
            (at.dist(HOOP) - THREE_RADIUS).abs()
        };
        if d >= TOE_BAND {
            return truth;
        }
        let (i, dist, blocked) = self.view(at, [(0, 9), (0, 9)]);
        let muddle = self.muddle(i, dist, blocked, 0);
        let near = 1.0 - d / TOE_BAND;
        let p = (TOE_BASE * near * muddle * 0.8).clamp(0.0, 0.65);
        let wrong = self.ofc.as_mut().expect("officiated").rng.chance(p);
        if !wrong {
            // He saw it right; recorded only when it was a close one.
            return truth;
        }
        let called = if truth == 3 { 2 } else { 3 };
        let o = self.play.o;
        let who = self.oid(h);
        let slot = self.sides[o].on[h];
        let swing = 1.0;
        // The ruling gives the shooter's side the extra point when three is
        // counted, takes it when two is.
        let gains = if called == 3 { o } else { 1 - o };
        let idx = self.note(
            What::Count,
            Ruling::Value(truth as u8),
            Ruling::Value(called as u8),
            i,
            who,
            who,
            gains,
            swing,
        );
        // The side hurt may ask: the shooter's if the count was lowered, the
        // other if it was raised.
        let side = if called == 3 { 1 - o } else { o };
        self.open_window(
            idx,
            side,
            true,
            Undo::Count { side: o, slot, called: called as u8, truth: truth as u8 },
        );
        called
    }

    // ---- reviews ----------------------------------------------------------

    fn open_window(&mut self, idx: usize, side: usize, ripe: bool, undo: Undo) {
        let anim = self.anim;
        let ofc = self.ofc.as_mut().expect("officiated");
        ofc.windows.push(Window { idx, side: side as u8, ripe, offered: false, since: anim, undo });
    }

    /// A defensive foul has been booked: it may be looked at once the
    /// stoppage is reached. Returns true when the foul-out substitution must
    /// wait for that.
    pub(super) fn foul_booked(
        &mut self,
        d: usize,
        dk: usize,
        h: usize,
        shots: u8,
        kind: FoulKind,
    ) -> bool {
        let Some(ofc) = self.ofc.as_mut() else { return false };
        let Some(idx) = ofc.last.take() else { return false };
        // A shot in the air that was fouled: the stoppage comes when the
        // ball is down and the line is set; otherwise now.
        let info = self.ofc.as_mut().and_then(|o| o.shot.take());
        let shooting = if kind == FoulKind::Shooting { info } else { None };
        let slot = self.sides[d].on[dk];
        let undo = Undo::Defensive {
            side: d,
            slot,
            h,
            kind,
            shots,
            shooting,
            holder: self.play.holder,
            ball: self.play.ball,
            ball_h: self.play.ball_h,
        };
        let ripe = shots == 0;
        let side = d;
        self.open_window(idx, side, ripe, undo);
        self.can_challenge(side)
    }

    /// An offensive foul has been booked with a turnover.
    pub(super) fn offensive_booked(&mut self, k: usize, dk: usize, kind: FoulKind) {
        let o = self.play.o;
        let Some(ofc) = self.ofc.as_mut() else { return };
        let Some(idx) = ofc.last.take() else { return };
        let slot = self.sides[o].on[k];
        self.open_window(idx, o, true, Undo::Offensive { side: o, slot, k, dk, kind });
    }

    /// The shot that is about to be fouled: made, and a three.
    pub(super) fn shot_info(&mut self, made: bool, three: bool) {
        if let Some(o) = self.ofc.as_mut() {
            o.shot = Some((made, three));
        }
    }

    /// The free throws are lined up: windows waiting on them are ripe.
    pub(super) fn ripen(&mut self) {
        if let Some(ofc) = self.ofc.as_mut() {
            for w in &mut ofc.windows {
                w.ripe = true;
            }
        }
    }

    /// At the top of every step: ask the bench about a call at a stoppage,
    /// or let the old ones go.
    pub(super) fn review_gate(&mut self) -> Gate {
        let Some(ofc) = self.ofc.as_mut() else { return Gate::Go };
        if ofc.windows.is_empty() {
            return Gate::Go;
        }
        let now = self.anim;
        // Anything the human has already been asked about is over now.
        let mut i = 0;
        while i < self.ofc.as_ref().expect("officiated").windows.len() {
            let (offered, stale) = {
                let w = &self.ofc.as_ref().expect("officiated").windows[i];
                (w.offered, now - w.since > STALE)
            };
            if offered || stale {
                let w = self.ofc.as_mut().expect("officiated").windows.remove(i);
                self.ofc.as_mut().expect("officiated").prompt = None;
                self.close_window(&w);
            } else {
                i += 1;
            }
        }
        let n = self.ofc.as_ref().expect("officiated").windows.len();
        for i in 0..n {
            let (ripe, side) = {
                let w = &self.ofc.as_ref().expect("officiated").windows[i];
                (w.ripe, w.side as usize)
            };
            if !ripe {
                continue;
            }
            let can = self.can_challenge(side);
            if !can {
                let w = self.ofc.as_mut().expect("officiated").windows.remove(i);
                self.close_window(&w);
                return self.review_gate();
            }
            if self.sides[side].coached {
                // The bench raises a flag on a call the assistant doubts, or
                // one that matters too much to let pass.
                let prompt = self.make_prompt(i);
                let stakes = self.stakes(i);
                if prompt.opinion == Opinion::LooksRight && stakes < 0.65 {
                    let w = self.ofc.as_mut().expect("officiated").windows.remove(i);
                    self.close_window(&w);
                    return self.review_gate();
                }
                let ofc = self.ofc.as_mut().expect("officiated");
                ofc.windows[i].offered = true;
                ofc.prompt = Some(prompt);
                return Gate::Pause(Pause::Review(side));
            }
            // A bench run by the program decides for itself.
            let ask = self.ai_wants(i);
            let w = self.ofc.as_mut().expect("officiated").windows.remove(i);
            if ask {
                self.settle_challenge(w);
            } else {
                self.close_window(&w);
            }
            return self.review_gate();
        }
        Gate::Go
    }

    /// A side may use its challenge: it has exactly one a game, won or lost.
    pub(super) fn can_challenge(&self, side: usize) -> bool {
        self.ofc.as_ref().is_some_and(|o| o.left[side] > 0)
    }

    /// No challenge: the call stands. A fouled-out player leaves now.
    fn close_window(&mut self, w: &Window) {
        self.release_out(w);
    }

    fn release_out(&mut self, w: &Window) {
        if let Undo::Defensive { side, slot, .. } = w.undo {
            let ofc = self.ofc.as_mut().expect("officiated");
            if let Some(n) = ofc.out.iter().position(|&(s, sl)| s == side && sl == slot) {
                ofc.out.remove(n);
                self.send_off(side, slot);
            }
        }
    }

    /// Put a man who has fouled out on the bench.
    fn send_off(&mut self, side: usize, slot: usize) {
        let Some(dk) = self.sides[side].on.iter().position(|&s| s == slot) else { return };
        let who = self.sides[side].slots[slot].p.id;
        let (elapsed, regulation, limit) = (self.elapsed, self.regulation, self.rules.foul_limit);
        self.sides[side].replace(dk, elapsed, regulation, limit);
        self.jump[side][dk] = Jump::NONE;
        self.refresh_bulks();
        let bulk = self.bulks();
        self.collide(&bulk);
        let s = &self.sides[side];
        let on = s.slots[s.on[dk]].p.id;
        if on != who {
            self.event(EventKind::Substitution { on, off: who });
        }
    }

    /// Mark a fouled-out man to be sent off when his foul has been looked at.
    pub(super) fn hold_out(&mut self, side: usize, dk: usize) {
        let slot = self.sides[side].on[dk];
        if let Some(ofc) = self.ofc.as_mut() {
            ofc.out.push((side, slot));
        }
    }

    // ---- what the assistant and the other benches make of it --------------

    fn make_prompt(&mut self, i: usize) -> ReviewPrompt {
        let w = &self.ofc.as_ref().expect("officiated").windows[i];
        let rec = self.ofc.as_ref().expect("officiated").calls[w.idx].clone();
        let side = w.side as usize;
        let wrong = rec.can_be_overturned();
        // The assistant sees it as well as his own eyes allow.
        let coach = self.sides[side].coach as f32;
        let sees = (0.55 + 0.35 * (coach / 20.0).clamp(0.0, 1.0)).min(0.92);
        let right = self.ofc.as_mut().expect("officiated").rng.chance(sees);
        let thinks_wrong = if right { wrong } else { !wrong };
        ReviewPrompt {
            side: side as u8,
            what: rec.what,
            called: rec.called,
            who: rec.who,
            on: rec.on,
            period: rec.period,
            clock: rec.clock,
            by: rec.by,
            opinion: if thinks_wrong { Opinion::LooksWrong } else { Opinion::LooksRight },
            left: self.ofc.as_ref().expect("officiated").left[side],
            timeouts: self.sides[side].timeouts,
        }
    }

    /// How much a call matters to the side it went against, 0..=1.
    fn stakes(&self, i: usize) -> f32 {
        let w = &self.ofc.as_ref().expect("officiated").windows[i];
        let side = w.side as usize;
        // How much it matters: late and close, a star in foul trouble, a
        // bonus's free throws, a made basket.
        let score = self.sides[side].score as i32 - self.sides[1 - side].score as i32;
        let secs_left = self.clock + (self.rules.periods as f32 - 1.0 - self.period as f32).max(0.0) * self.rules.period_secs as f32;
        let mut matters = 0.2f32;
        if secs_left < 240.0 {
            matters += 0.4 * (1.0 - secs_left / 240.0) + 0.2;
        }
        if score.abs() <= 6 {
            matters += 0.15;
        }
        if let Undo::Defensive { side: s, slot, shots, .. } = w.undo {
            let pf = self.sides[s].slots[slot].line.pf;
            if pf + 1 >= self.rules.foul_limit {
                matters += 0.45;
            } else if pf >= self.rules.foul_limit.saturating_sub(2) {
                matters += 0.15;
            }
            if shots > 0 {
                matters += 0.1;
            }
        }
        if let Undo::Offensive { side: s, slot, .. } = w.undo {
            let pf = self.sides[s].slots[slot].line.pf;
            if pf >= self.rules.foul_limit.saturating_sub(1) {
                matters += 0.3;
            }
        }
        matters.min(1.0)
    }

    /// An AI bench decides whether to spend its challenge: it reads the
    /// play as well as its coach does, and the more the call matters the
    /// less sure it needs to be.
    fn ai_wants(&mut self, i: usize) -> bool {
        let w = &self.ofc.as_ref().expect("officiated").windows[i];
        let rec = self.ofc.as_ref().expect("officiated").calls[w.idx].clone();
        let side = w.side as usize;
        let wrong = rec.can_be_overturned();
        let coach = self.sides[side].coach as f32;
        let sees = (0.55 + 0.35 * (coach / 20.0).clamp(0.0, 1.0)).min(0.92);
        let (a, b) = {
            let r = &mut self.ofc.as_mut().expect("officiated").rng;
            (r.f32(), r.f32())
        };
        let thinks_wrong = if a < sees { wrong } else { !wrong };
        // Before the signal, how likely is it that this kind of call is wrong?
        let prior = match rec.what {
            What::Foul => match rec.called {
                Ruling::Foul { kind, .. } if kind == FoulKind::Charging || kind == FoulKind::Blocking => 0.22,
                _ => 0.07,
            },
            What::OutOfBounds => 0.10,
            What::Count => 0.5,
        };
        // After it: Bayes with the coach's reading.
        let (pt, pf) = if thinks_wrong {
            (prior * sees, (1.0 - prior) * (1.0 - sees))
        } else {
            (prior * (1.0 - sees), (1.0 - prior) * sees)
        };
        let posterior = pt / (pt + pf);
        let matters = self.stakes(i);
        // A bench that has seen it clearly and has something to lose.
        let need = (0.9 - matters * 0.85).clamp(0.25, 0.9);
        // A little noise so two benches do not behave as one.
        posterior + (b - 0.5) * 0.12 > need
    }

    /// A human bench asks for a review. Returns whether the call was
    /// overturned, or `None` if there is nothing to challenge.
    pub(in crate::engine) fn challenge(&mut self, side: usize) -> Option<bool> {
        let n = self.ofc.as_ref()?.windows.iter().position(|w| w.side as usize == side && w.offered)?;
        let w = self.ofc.as_mut().expect("officiated").windows.remove(n);
        self.ofc.as_mut().expect("officiated").prompt = None;
        if !self.can_challenge(side) {
            self.close_window(&w);
            return None;
        }
        Some(self.settle_challenge(w))
    }

    /// The review: look at the film. A call that was wrong is overturned,
    /// unless it was a close one the crew chief's quality leaves standing.
    fn settle_challenge(&mut self, w: Window) -> bool {
        let side = w.side as usize;
        let rec = self.ofc.as_ref().expect("officiated").calls[w.idx].clone();
        let wrong = rec.can_be_overturned();
        // The review is not perfect: a close play can stand, and now and
        // then the film is misread. Both depend on the crew chief.
        let q = self.ofc.as_ref().expect("officiated").crew.quality();
        let slip = REVIEW_SLIP * (1.5 - q).clamp(0.3, 1.5);
        let r = self.ofc.as_mut().expect("officiated").rng.f32();
        let close = match (rec.truth, rec.called) {
            // Contact that was nearly nothing, or a toe on the line.
            (Ruling::Nothing, Ruling::Foul { .. }) => 0.18,
            (Ruling::Value(_), _) => 0.25,
            _ => 0.10,
        };
        let stays_close = r < close * (1.3 - q).clamp(0.2, 1.2);
        let r2 = self.ofc.as_mut().expect("officiated").rng.f32();
        let overturned = wrong && !stays_close && r2 >= slip;
        // One challenge a game, used up whatever the film shows. A lost
        // one also costs a timeout, if there is one left to lose.
        let ofc = self.ofc.as_mut().expect("officiated");
        ofc.left[side] = 0;
        if !overturned {
            let t = &mut self.sides[side].timeouts;
            *t = t.saturating_sub(1);
        }
        let review = Review { by: side as u8, overturned };
        let idx = w.idx;
        self.ofc.as_mut().expect("officiated").calls[idx].review = Some(review);
        let mut done = self.ofc.as_ref().expect("officiated").calls[idx].clone();
        done.review = Some(review);
        self.ofc.as_mut().expect("officiated").challenge[side] = Some(done);
        if let Some(r) = &mut self.rec
            && let Some(c) = r.calls.get_mut(idx)
        {
            c.review = Some(review);
        }
        {
            let by = rec.by as usize;
            let ofc = self.ofc.as_mut().expect("officiated");
            ofc.per[by].reviewed += 1;
            if overturned {
                ofc.per[by].overturned += 1;
            }
        }
        self.event(EventKind::Review { side: side as u8, overturned, what: rec.what });
        self.whistle_until = self.anim + SIGNAL;
        if overturned {
            self.overturn(&w, &rec);
        } else {
            self.release_out(&w);
        }
        overturned
    }

    // ---- taking a call back ----------------------------------------------

    fn overturn(&mut self, w: &Window, rec: &CallRecord) {
        match w.undo.clone() {
            Undo::Defensive { side, slot, h, kind, shots, shooting, holder, ball, ball_h } => {
                // The foul comes off him and the team.
                self.sides[side].slots[slot].line.pf =
                    self.sides[side].slots[slot].line.pf.saturating_sub(1);
                self.sides[side].team_fouls = self.sides[side].team_fouls.saturating_sub(1);
                // A man who was to leave stays.
                let ofc = self.ofc.as_mut().expect("officiated");
                if let Some(n) = ofc.out.iter().position(|&(s, sl)| s == side && sl == slot) {
                    ofc.out.remove(n);
                }
                let o = 1 - side;
                if let Some((made, three)) = shooting {
                    if !made {
                        // The try that was not entered in the book because of
                        // the foul now is.
                        let line = &mut self.sides[o].slots[self.sides[o].on[h]].line;
                        line.fga += 1;
                        if three {
                            line.tpa += 1;
                        }
                    }
                    // No free throws. A basket that went in is a dead ball for
                    // the other side. A miss is a loose ball the defense gets:
                    // the defender nearest the basket is credited the board.
                    self.cancel_line();
                    let end = if made {
                        End::Dead
                    } else {
                        let dk = (0..5)
                            .min_by(|&a, &b| {
                                self.dp(a).dist(HOOP).total_cmp(&self.dp(b).dist(HOOP))
                            })
                            .unwrap_or(0);
                        self.dline(dk).drb += 1;
                        let who = self.did(dk);
                        self.event(EventKind::Rebound { who, offensive: false });
                        End::Live(dk)
                    };
                    self.ofc.as_mut().expect("officiated").forced = Some(end);
                } else if shots > 0 {
                    // A bonus foul with no shot: back to a throw-in.
                    self.cancel_line();
                    self.play.holder = holder;
                    self.play.fetcher = None;
                    self.play.ball = ball;
                    self.play.ball_h = ball_h;
                } else {
                    let _ = kind;
                }
                // A truth of the other foul: a block called a charge.
                if let Ruling::Foul { kind: tk, by } = rec.truth
                    && by as usize == o
                    && tk == FoulKind::Charging
                {
                    // He was the one at fault: the charge is booked now.
                    let dk = self.sides[side].on.iter().position(|&s| s == slot).unwrap_or(0);
                    if let Some(end) = self.offensive_foul(h, dk, tk) {
                        self.ofc.as_mut().expect("officiated").forced = Some(end);
                    }
                }
            }
            Undo::Offensive { side, slot, k, dk, kind } => {
                let line = &mut self.sides[side].slots[slot].line;
                line.pf = line.pf.saturating_sub(1);
                line.tov = line.tov.saturating_sub(1);
                // Play goes on: the same side has the ball back, or the
                // defender is whistled if that is what it was.
                self.possessions[side] = self.possessions[side].saturating_sub(1);
                self.stage = Stage::Play;
                self.loose = None;
                self.play.o = side;
                self.play.dead = DEAD_FOUL;
                self.play.phase = Phase::Set;
                self.play.think = 0.8;
                self.play.task = [Task::Spot; 5];
                self.play.dribbled = false;
                self.play.assist = None;
                self.play.drop = None;
                self.play.shot_clock = self.play.shot_clock.max(14.0);
                self.play.holder = Some(k);
                self.play.ball = self.op(k);
                let _ = (slot, kind);
                if let Ruling::Foul { kind: tk, by } = rec.truth
                    && by as usize != side
                {
                    // The defender who was in the way is the one at fault.
                    if let Some(end) = self.floor_foul(dk, k, tk) {
                        self.ofc.as_mut().expect("officiated").forced = Some(end);
                    }
                }
            }
            Undo::Ball { at, z, attackers } => {
                if attackers {
                    // It was given to the attackers: it goes to the other side.
                    self.play.after = None;
                    let mut path = Path::new();
                    path.fall(
                        Vec2 { x: at.x.clamp(-7.8, 7.8), y: at.y.clamp(-0.3, LENGTH + 0.3) },
                        z.max(BALL_RADIUS),
                        0.0,
                        Vec2::ZERO,
                    );
                    self.loose = Some((path, 0.0));
                    self.credit = 0.0;
                    self.ofc.as_mut().expect("officiated").forced = Some(End::Dead);
                } else {
                    // It was given to the defense: the attackers keep it.
                    let o = self.play.o;
                    self.possessions[o] = self.possessions[o].saturating_sub(1);
                    self.stage = Stage::Possession(o, Start::Inbound);
                }
            }
            Undo::Count { side, slot, called, truth, .. } => {
                let d = truth as i32 - called as i32;
                let line = &mut self.sides[side].slots[slot].line;
                line.pts = (line.pts as i32 + d).max(0) as u16;
                self.sides[side].score = (self.sides[side].score as i32 + d).max(0) as u16;
                if truth == 3 {
                    line.tpm += 1;
                    line.tpa += 1;
                } else {
                    line.tpm = line.tpm.saturating_sub(1);
                    line.tpa = line.tpa.saturating_sub(1);
                }
            }
        }
    }

    /// Take the free-throw line away and put the dead ball in the state of
    /// any other whistle without free throws.
    fn cancel_line(&mut self) {
        self.play.phase = Phase::Set;
        self.play.think = 0.8;
        self.play.dead = DEAD_FOUL;
        self.play.shot_clock = self.play.shot_clock.max(14.0);
        self.play.task = [Task::Spot; 5];
        self.play.dribbled = false;
        self.play.assist = None;
        self.play.drop = None;
    }

    /// The `End` a review has decided on, if any.
    pub(super) fn forced_end(&mut self) -> Option<End> {
        self.ofc.as_mut().and_then(|o| o.forced.take())
    }

    // ---- the record -------------------------------------------------------

    pub(super) fn officiating(&self) -> Option<GameOfficiating> {
        let ofc = self.ofc.as_ref()?;
        let mut per = ofc.per;
        for p in &mut per {
            p.games = 1;
        }
        Some(GameOfficiating {
            crew: ofc.crew.ids(),
            fouls: ofc.fouls,
            calls: ofc.decisions,
            others: ofc.others,
            errors: ofc.errors,
            swing: ofc.swing,
            notable: ofc
                .calls
                .iter()
                .filter(|c| c.verdict().is_wrong() || c.review.is_some())
                .cloned()
                .collect(),
            challenge: ofc.challenge.clone(),
            per_ref: per,
            left: ofc.left,
        })
    }

    /// The review being asked of the bench, if any.
    pub(in crate::engine) fn open_review(&self) -> Option<&ReviewPrompt> {
        self.ofc.as_ref().and_then(|o| o.prompt.as_ref())
    }

    pub(in crate::engine) fn challenges_left(&self, side: usize) -> u8 {
        self.ofc.as_ref().map_or(0, |o| o.left[side])
    }

    pub(in crate::engine) fn crew(&self) -> Option<&Crew> {
        self.ofc.as_ref().map(|o| o.crew())
    }
}

impl Game {
    /// Assign a crew to a game before it starts: its tightness becomes the
    /// game's whistle.
    pub(in crate::engine) fn assign_crew(&mut self, crew: Crew) {
        let ofc = Ofc::new(crew, &mut self.rng);
        self.whistle = ofc.tightness();
        // The hall: the visitors are less at home in it, the house lifts
        // the home five and rattles the visitors, all in proportion to how
        // full and loud it is. Applied once, as the game starts; `Side::tick`
        // keeps the lift going.
        let loud = ofc.crew.crowd.clamp(0.0, 1.0);
        self.sides[1].unfamiliar = (self.sides[1].unfamiliar + HALL * loud).min(1.0);
        self.sides[0].house = HOUSE * loud;
        self.sides[1].house = -HOUSE * loud;
        if let Some(r) = &mut self.rec {
            r.whistle = self.whistle;
        }
        self.ofc = Some(ofc);
    }
}
