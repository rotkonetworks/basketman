//! A pass is a thrown ball. The passer picks a kind that fits the lane and a
//! target ahead of the receiver; the ball leaves his hand after the time the
//! throw takes and flies on a real path; hands decide what happens to it. A
//! receiver's hand that gets to it catches it, or fumbles it; a defender's
//! hand that gets to it tips it or takes it. Nothing arrives by a dice roll:
//! the dice are only the passer's error and the hand-eye error of a reach.

use super::*;
use crate::engine::hands::{self, LEFT, RIGHT};
use crate::court::BOARD_HIGH;
use crate::replay::{Dribble, HandKind, PassKind, PassOutcome, TurnoverKind};

/// Metres of aim error at no distance, and per metre thrown, for a chest
/// pass by a passer of rating 10.
const AIM_BASE: f32 = 0.07;
const AIM_PER_M: f32 = 0.014;
/// Seconds before a defender sees the ball go, before his anticipation.
const SEE_BASE: f32 = 0.16;
const SEE_SKILL: f32 = 0.008;
/// How far past his arm a defender can get a hand to a ball by leaning.
const LUNGE_REACH: f32 = 0.35;
/// How likely a defender who can reach a pass goes for it, at rating 10.
const TRY_BASE: f32 = 0.12;
const TRY_SKILL: f32 = 0.030;
const TRY_AGGRESSION: f32 = 0.25;
/// Metres within which a hand and the ball are in contact.
const HIT: f32 = hands::CONTACT;
/// The same for a receiver's hands, which come to the ball together.
const CATCH: f32 = 0.33;
/// Seconds past its arrival that a missed pass is followed before it is
/// just a loose ball.
const OVERTIME: f32 = 0.45;
/// Metres from the line of the ball within which a defender gets in its way.
const CLOSE_LANE: f32 = 0.95;

#[derive(Copy, Clone)]
pub(super) struct Spec {
    /// Metres a second over the floor.
    pub speed: f32,
    /// How much its error is multiplied by.
    pub err: f32,
    /// Seconds to get it away, at rest and facing the man.
    pub wind: f32,
    /// Height of the ball when it reaches the man.
    pub z: f32,
}

pub(super) fn spec(kind: PassKind) -> Spec {
    match kind {
        PassKind::Chest => Spec { speed: 11.5, err: 1.0, wind: 0.16, z: 1.30 },
        PassKind::Bounce => Spec { speed: 10.0, err: 1.15, wind: 0.20, z: 0.95 },
        PassKind::Overhead => Spec { speed: 12.5, err: 1.08, wind: 0.28, z: 1.95 },
        PassKind::Lob => Spec { speed: 7.5, err: 1.15, wind: 0.30, z: 2.05 },
        PassKind::Push => Spec { speed: 11.0, err: 1.35, wind: 0.10, z: 1.35 },
        PassKind::Handoff => Spec { speed: 4.0, err: 0.4, wind: 0.12, z: 1.20 },
        PassKind::Outlet => Spec { speed: 17.0, err: 1.4, wind: 0.24, z: 1.50 },
    }
}

/// The path a pass takes: from the release to the aim, and on.
#[derive(Copy, Clone)]
pub(super) struct Plan {
    pub path: Path,
    /// Seconds from the release to the aim point.
    pub dur: f32,
    pub aim: Vec2,
    pub aim_z: f32,
}

/// A throw in its wind-up.
#[derive(Copy, Clone)]
pub(super) struct Throw {
    pub from: usize,
    pub to: usize,
    pub kind: PassKind,
    /// Play time the ball is let go.
    pub at: f32,
    /// The hand that lets it go (both hands for two-handed passes).
    pub hand: usize,
    pub two: bool,
    /// Where the hands are taking it.
    pub rel: V3,
}

/// A pass in the air: what the hands know about it.
#[derive(Copy, Clone)]
pub(super) struct Thrown {
    pub kind: PassKind,
    pub speed: f32,
    /// Each defender's reach for it: where, and from which flight time.
    pub attempt: [Option<(V3, f32)>; 5],
    /// The tick the receiver's hands were last on it, for the catch.
    pub receiver_seen: bool,
    /// The ball has been inside the lines (a throw-in starts outside).
    pub entered: bool,
    pub dbg: [(Vec2, V3, V3); 5],
    /// Where the receiver has decided to meet it, and when.
    pub catch: Option<(V3, f32)>,
    /// How far off the aim it was thrown, in metres.
    pub err: f32,
}

/// A ball nobody holds, after a pass was fumbled or tipped.
#[derive(Copy, Clone)]
pub(super) struct Scramble {
    pub passer: usize,
    pub to: usize,
    /// The defender (court index) whose hand tipped it, if one did.
    pub tipped_by: Option<usize>,
    pub kind: PassKind,
    /// What the turnover is called if the other team ends up with it.
    pub cause: TurnoverKind,
}

use crate::engine::ball::V3;

impl Game {
    /// Passing skill and vision of man `h`, 0 to 1.
    fn pass_skill(&self, h: usize) -> f32 {
        ((self.oe(h, Attr::Passing) + self.oe(h, Attr::Iq)) * 0.5 / 20.0).clamp(0.0, 1.0)
    }

    /// Where the ball is let go for a pass of `kind` from man `h` to man
    /// `j`, if his hands got there at once, and the hand that does it.
    fn release_point(&self, h: usize, j: usize, kind: PassKind) -> (V3, usize, bool) {
        let o = self.play.o;
        let me = self.m[o][h].pos;
        let body = self.st[o][h].body;
        let (arm, sz, _) = self.arm_of(o, h);
        let lift = self.lift_at(o, h, 0.0);
        let to = (self.m[o][j].pos - me).norm();
        let right = hands::right_of(body);
        let side = if right.dot(to) >= 0.0 { RIGHT } else { LEFT };
        // The dribbling hand has the ball: a pass off the dribble is one
        // handed with that hand, or he picks it up.
        let live = self.handler.who == Some((o, h)) && self.handler.state == Dribble::Live;
        let one = matches!(kind, PassKind::Push) || (live && !matches!(kind, PassKind::Bounce));
        let hand = if live {
            if self.handler.hand >= 0 { RIGHT } else { LEFT }
        } else {
            side
        };
        let (fwd, z) = match kind {
            PassKind::Chest => (0.55, sz - 0.35 * arm),
            PassKind::Bounce => (0.50, sz - 0.75 * arm),
            PassKind::Overhead => (0.15, sz + 0.80 * arm),
            PassKind::Lob => (0.35, sz + 0.30 * arm),
            PassKind::Push => (0.55, sz - 0.25 * arm),
            PassKind::Handoff => (0.60, sz - 0.30 * arm),
            PassKind::Outlet => (0.25, sz + 0.60 * arm),
        };
        // A push is released out to the side of the hand that throws it.
        let lateral = if one { right * (0.30 * hands::sign(hand)) } else { Vec2::ZERO };
        let dir = if body.dot(to) > 0.3 { to } else { body };
        let p = me + dir * fwd + lateral;
        (v3(p.x, p.y, z + lift), hand, !one)
    }

    /// Where a pass to man `j` should be aimed to meet him: ahead of him as
    /// far as the passer sees where he is going.
    fn pass_aim(&self, h: usize, j: usize, from: Vec2, speed: f32) -> Vec2 {
        let o = self.play.o;
        let m = &self.m[o][j];
        let skill = self.pass_skill(h);
        let lead = (0.55 + 0.45 * skill).min(1.0);
        let mut at = m.pos;
        for _ in 0..2 {
            let t = (from.dist(at) / speed).max(0.1) + 0.1;
            at = match self.play.task[j] {
                // Sent to a place: he gets up to speed on the way, so he is
                // not as far along as a man who was already running.
                Task::Cut(to) if m.pos.dist(to) > 0.8 => {
                    let dir = (to - m.pos).norm();
                    let top = self.cache.top[o * 5 + j] * 0.9;
                    let v0 = m.vel.dot(dir).max(0.0);
                    let dist = (v0 * t + 0.5 * 6.0 * t * t).min(top * t).min(m.pos.dist(to));
                    m.pos + dir * (dist * lead.max(0.8))
                }
                _ => m.pos + m.vel * (t * lead),
            };
        }
        v(at.x.clamp(-7.2, 7.2), at.y.clamp(0.3, LENGTH - 0.3))
    }

    /// The path of a pass released at `rel` to `aim` at height `z`.
    pub(super) fn plan_pass(&self, kind: PassKind, rel: V3, aim: Vec2, z: f32) -> Plan {
        let sp = spec(kind);
        let from = rel.xy();
        let d = from.dist(aim).max(0.3);
        let mut path = Path::new();
        let dur;
        let (end_z, end_vz, end_vel);
        if kind == PassKind::Bounce {
            // Down to the floor two thirds of the way, and up to the hands.
            let bp = from.lerp(aim, 0.65);
            let t1 = (d * 0.65 / sp.speed).max(0.08);
            let t2 = (d * 0.35 / (sp.speed * 0.8)).max(0.08);
            path.push(flight::Arc::reach(from, rel.z, bp, BALL_RADIUS, t1));
            let a2 = flight::Arc::reach(bp, BALL_RADIUS, aim, z, t2);
            path.push(a2);
            dur = t1 + t2;
            end_z = z;
            end_vz = a2.vz - a2.g * a2.dur;
            end_vel = (aim - bp) * (1.0 / t2);
        } else {
            let t = (d / sp.speed).max(0.1);
            let a = flight::Arc::reach(from, rel.z, aim, z, t);
            path.push(a);
            dur = t;
            end_z = z;
            end_vz = a.vz - a.g * a.dur;
            end_vel = (aim - from) * (1.0 / t);
        }
        path.fall_open(aim, end_z, end_vz, end_vel);
        Plan { path, dur, aim, aim_z: z }
    }

    /// The ball a pass of `kind` makes, with no error: for judging it.
    fn ideal_plan(&self, h: usize, j: usize, kind: PassKind) -> Plan {
        let (rel, _, _) = self.release_point(h, j, kind);
        let sp = spec(kind);
        let aim = self.pass_aim(h, j, rel.xy(), sp.speed);
        self.plan_pass(kind, rel, aim, sp.z)
    }

    /// Seconds a defender takes to see a pass go and start for it.
    fn see_time(&self, dk: usize, at: Vec2) -> f32 {
        let anticipation = 0.6 * self.de(dk, Attr::Steal) + 0.4 * self.de(dk, Attr::Iq);
        let looking = self.facing_factor(1 - self.play.o, dk, at).max(0.15);
        (SEE_BASE - SEE_SKILL * (anticipation - 10.0)).clamp(0.05, 0.4) / looking.min(1.0).max(0.4)
    }

    /// How likely defender `dk` is to go for a pass he can reach.
    fn try_odds(&self, dk: usize) -> f32 {
        let anticipation = 0.6 * self.de(dk, Attr::Steal) + 0.4 * self.de(dk, Attr::Iq);
        (TRY_BASE + TRY_SKILL * (anticipation - 10.0) + TRY_AGGRESSION * self.def().aggression())
            .clamp(0.05, 0.95)
    }

    /// Where, and when, man `k` of side `t` could first get a hand to a ball
    /// on `path`: a point on it within his reach that a hand can be at by the
    /// time the ball is. `now` is how far along the ball is, `see_at` the
    /// path time he starts moving (he has to have seen it).
    pub(super) fn reach_for(&self, t: usize, k: usize, path: &Path, dur: f32, now: f32, see_at: f32) -> Option<(V3, f32)> {
        let (arm, _, _) = self.arm_of(t, k);
        let slot = self.sides[t].on[k];
        let quick = self.sides[t].eff(slot, if t == self.play.o { Attr::Handling } else { Attr::Steal });
        let speed = hands::hand_speed(quick, self.sides[t].eff(slot, Attr::Handling));
        let q = self.m[t][k].pos;
        let sh = [self.shoulder_of(t, k, RIGHT), self.shoulder_of(t, k, LEFT)];
        let hand = [self.hands[t][k].h[RIGHT].p, self.hands[t][k].h[LEFT].p];
        let start = see_at.max(now);
        let total = dur + OVERTIME;
        // Far from the whole of its line: nothing to reach.
        {
            let a = path.at(0.0).0;
            let b = path.at(total).0;
            if q.dist(q.project(a, b)) > arm + 1.4 {
                return None;
            }
        }
        let mut tau = start + 0.02;
        while tau < total {
            let (bp, bz) = path.at(tau);
            // Over the floor he can be there by taking a step or two.
            let lean = (q.dist(bp) - 0.35).max(0.0) - LUNGE_REACH;
            if bz < 2.9 && bz > 0.3 && lean <= arm {
                let p = v3(bp.x, bp.y, bz);
                for i in 0..2 {
                    let dist = V3 { x: p.x - hand[i].x, y: p.y - hand[i].y, z: p.z - hand[i].z }.len();
                    let stretch = (V3 { x: p.x - sh[i].x, y: p.y - sh[i].y, z: p.z - sh[i].z }.len() - arm).max(0.0);
                    let time = dist / speed + stretch / 3.0;
                    if time <= tau - start + 0.01 {
                        return Some((p, tau));
                    }
                }
            }
            tau += 0.06;
        }
        None
    }

    /// The chance a pass of `kind` is picked off or tipped, from where the
    /// defenders' hands are and how fast they get anywhere: the passer's
    /// reading of the lane. The flight itself decides what really happens.
    pub(super) fn lane_risk(&self, h: usize, j: usize, kind: PassKind) -> f32 {
        let plan = self.ideal_plan(h, j, kind);
        let (rel, _, _) = self.release_point(h, j, kind);
        let from = rel.xy();
        // The ball is not let go for a moment: the defenders have moved.
        let mut safe = 1.0f32;
        for dk in 0..5 {
            if self.play.stun[dk] > 0.4 {
                continue;
            }
            let see = self.see_time(dk, from);
            if self.reach_for(1 - self.play.o, dk, &plan.path, plan.dur, 0.0, see).is_some() {
                // Going for it, and getting a hand on it despite his aim.
                safe *= 1.0 - self.try_odds(dk) * 0.45;
            }
        }
        // The ball's path, sampled once: 0.03 s apart.
        let mut smp = [(0.0f32, 0.0f32, 0.0f32); 64];
        let mut n = 0;
        while n < 64 && (n as f32) * 0.03 < plan.dur {
            let (bp, bz) = plan.path.at(n as f32 * 0.03);
            smp[n] = (bp.x, bp.y, bz);
            n += 1;
        }
        // A hand that is in the way: out in the lane, or up in front of him.
        let d = 1 - self.play.o;
        let (la, lb) = (v(smp[0].0, smp[0].1), plan.aim);
        let reach2 = (HIT + 0.06) * (HIT + 0.06);
        for dk in 0..5 {
            let q = self.dp(dk);
            // Nowhere near the line of the ball: nothing to read.
            if q.dist(q.project(la, lb)) > 3.2 {
                continue;
            }
            let vel = self.m[d][dk].vel;
            let mut hit = false;
            'hands: for i in 0..2 {
                let hp = self.hands[d][dk].h[i].p;
                for (s, &(bx, by, bz)) in smp[..n].iter().enumerate() {
                    // He has moved by the time the ball is there.
                    let m = (s as f32 * 0.03 + 0.25) * 0.6;
                    let dx = hp.x + vel.x * m - bx;
                    let dy = hp.y + vel.y * m - by;
                    let dz = hp.z - bz;
                    if dx * dx + dy * dy + dz * dz < reach2 {
                        hit = true;
                        break 'hands;
                    }
                }
            }
            if hit {
                safe *= 0.15;
            } else {
                // Close to the ball's line: he steps into it, and a hand or a
                // leg comes with him.
                let mut dmin2 = f32::MAX;
                for (s, &(bx, by, bz)) in smp[..n].iter().enumerate().step_by(2) {
                    if bz < 2.4 {
                        let m = (s as f32 * 0.03 + 0.25) * 0.4;
                        let (dx, dy) = (q.x + vel.x * m - bx, q.y + vel.y * m - by);
                        dmin2 = dmin2.min(dx * dx + dy * dy);
                    }
                }
                let k = (1.0 - dmin2.sqrt() / CLOSE_LANE).clamp(0.0, 1.0);
                safe *= 1.0 - 0.55 * k * k;
            }
        }
        // Through the glass is no way to throw it.
        for s in 1..n {
            let (last, now) = (smp[s - 1], smp[s]);
            if last.1 >= BOARD_Y && now.1 < BOARD_Y && now.0.abs() < BOARD_HALF && now.2 > BOARD_LOW && now.2 < BOARD_HIGH {
                safe *= 0.2;
                break;
            }
        }
        let mut risk = 1.0 - safe;
        // Even an open lane has a ball that can be fumbled or thrown wide.
        let len = from.dist(plan.aim);
        let skill = self.pass_skill(h);
        let sp = spec(kind);
        risk += (0.004 + len * 0.0011) * sp.err * (1.6 - 1.1 * skill);
        risk * self.pass_stance(h, j).min(2.0)
    }

    /// Which kind of pass man `h` throws to man `j`: the one that fits the
    /// lane best. He knows the ones he can throw.
    pub(super) fn pass_kind(&self, h: usize, j: usize) -> PassKind {
        let o = self.play.o;
        let d = self.m[o][h].pos.dist(self.m[o][j].pos);
        let passing = self.oe(h, Attr::Passing);
        let live = self.handler.who == Some((o, h)) && self.handler.state == Dribble::Live;
        if d < 1.9 {
            return PassKind::Handoff;
        }
        let mut best = (PassKind::Chest, self.lane_risk(h, j, PassKind::Chest));
        // An open lane: a chest pass, and no need to look further.
        if best.1 < 0.035 && !live && d <= 13.0 {
            return PassKind::Chest;
        }
        let mut consider = |kind: PassKind, game: &Self, extra: f32| {
            let r = game.lane_risk(h, j, kind) + extra;
            if r < best.1 {
                best = (kind, r);
            }
        };
        if d > 13.0 {
            consider(PassKind::Outlet, self, -0.01);
        }

        if passing >= 6.0 {
            consider(PassKind::Bounce, self, 0.02);
        }
        if passing >= 8.0 && d < 13.0 {
            consider(PassKind::Overhead, self, 0.04);
        }
        let pj = self.m[o][j].pos;
        if passing >= 9.0 && (pj.dist(HOOP) < 4.0 || d > 7.0) {
            consider(PassKind::Lob, self, 0.035);
        }
        if live && passing >= 5.0 && d < 9.0 {
            consider(PassKind::Push, self, 0.0);
        }
        best.0
    }

    /// Man `h` passes to man `j`: the throw starts. `safe` is a throw to a
    /// man who is waiting for it (an outlet): there is no reading to do.
    pub(super) fn pass(&mut self, h: usize, j: usize, safe: bool) -> Option<End> {
        let o = self.play.o;
        let _ = safe;
        debug_assert_ne!(h, j);
        let kind = self.pass_kind(h, j);
        let (rel, hand, two) = self.release_point(h, j, kind);
        let wind = spec(kind).wind + self.pass_stance_delay(h, j);
        // Walking up to the throw: the hands have to get there.
        let sh = self.shoulder_of(o, h, hand);
        let hp = self.hands[o][h].h[hand].p;
        let travel = V3 { x: rel.x - hp.x, y: rel.y - hp.y, z: rel.z - hp.z }.len();
        let slot = self.off().on[h];
        let speed = hands::hand_speed(self.sides[o].eff(slot, Attr::Handling), self.sides[o].eff(slot, Attr::Passing));
        let _ = sh;
        let at = self.play.t + wind.max(travel / speed) * 0.5 + 0.02;
        let (wall, at_court) = (self.wall, crate::replay::to_court(rel.xy(), self.frame));
        if let Some(rec) = &mut self.rec {
            rec.passes.push(crate::replay::PassRec {
                time: wall,
                side: o as u8,
                from: h as u8,
                to: j as u8,
                at: at_court,
                inbound: self.play.inbound,
                kind,
                outcome: PassOutcome::Caught,
                contact_cm: 0,
                touched_by: None,
            });
        }
        self.play.throw = Some(Throw { from: h, to: j, kind, at, hand, two, rel });
        self.lapses();
        if !matches!(self.play.phase, Phase::Advance) {
            self.play.phase = Phase::Set;
        }
        None
    }

    /// The wind-up of a throw: until the ball is let go.
    pub(super) fn throw_tick(&mut self) -> Option<End> {
        let Some(th) = self.play.throw else { return None };
        if self.play.t < th.at {
            return None;
        }
        self.play.throw = None;
        self.release(th)
    }

    /// A defender's hand is on the ball's way out of the passer's hands.
    fn hands_in_the_way(&self, h: usize, j: usize, kind: PassKind, rel: V3) -> bool {
        let sp = spec(kind);
        let aim = self.pass_aim(h, j, rel.xy(), sp.speed);
        let plan = self.plan_pass(kind, rel, aim, sp.z);
        let d = 1 - self.play.o;
        for dk in 0..5 {
            for i in 0..2 {
                let hp = self.hands[d][dk].h[i].p;
                let mut tau = 0.0;
                while tau < plan.dur.min(0.35) {
                    let (bp, bz) = plan.path.at(tau);
                    if (V3 { x: hp.x - bp.x, y: hp.y - bp.y, z: hp.z - bz }).len() < HIT + 0.04 {
                        return true;
                    }
                    tau += 0.015;
                }
            }
        }
        false
    }

    /// The ball leaves his hand.
    fn release(&mut self, th: Throw) -> Option<End> {
        let o = self.play.o;
        let (h, j, kind) = (th.from, th.to, th.kind);
        // Where it is let go: where his hands are.
        let (a, b) = if th.two {
            (self.hands[o][h].h[RIGHT].p, self.hands[o][h].h[LEFT].p)
        } else {
            (self.hands[o][h].h[th.hand].p, self.hands[o][h].h[th.hand].p)
        };
        let rel = v3((a.x + b.x) * 0.5, (a.y + b.y) * 0.5, ((a.z + b.z) * 0.5).max(0.4));
        // A hand in front of the ball as it leaves is not a pass to throw: he
        // changes it for one that goes round the hands, or holds it.
        let mut kind = kind;
        if self.hands_in_the_way(h, j, kind, rel) {
            let alt = [PassKind::Bounce, PassKind::Overhead, PassKind::Chest, PassKind::Push];
            let found = alt.into_iter().find(|&k| k != kind && !self.hands_in_the_way(h, j, k, rel));
            match found {
                Some(k) => kind = k,
                None => {
                    if self.play.aborts < 2 && self.rng.chance(0.4 + 0.5 * self.pass_skill(h)) {
                        self.play.aborts += 1;
                        self.play.think = 0.15;
                        return None;
                    }
                }
            }
        }
        let sp = spec(kind);
        let aim = self.pass_aim(h, j, rel.xy(), sp.speed);
        // The error: from skill, pressure, balance, and how he stands to the
        // man; bigger the further and the stranger the throw.
        let d = rel.xy().dist(aim);
        let skill = self.oe(h, Attr::Passing);
        let st = &self.st[o][h];
        let (near_def, near_d) = self.nearest_defender(self.op(h));
        let _ = near_def;
        let pressure = 1.0 + 0.4 * (1.0 - near_d / 1.4).clamp(0.0, 1.0);
        let footing = 1.0 + 0.6 * (1.0 - st.balance).max(0.0);
        let weak = {
            let pl = &self.off().slots[self.off().on[h]].p;
            let strong = (pl.body.hand == crate::player::Hand::Right) == (th.hand == RIGHT);
            if th.two || strong { 1.0 } else { 1.0 + 0.5 * (1.0 - pl.off_hand_skill() / 20.0) }
        };
        let moving = 1.0 + 0.04 * self.m[o][h].vel.len();
        let sigma = (AIM_BASE + AIM_PER_M * d)
            * sp.err
            * (1.9 - skill / 20.0 * 1.6).max(0.3)
            * pressure
            * footing
            * weak
            * moving
            * self.pass_stance(h, j).min(2.5)
            * (1.0 + 0.5 * self.off().unfamiliar);
        let ex = shot::gauss(&mut self.rng) * sigma;
        let ey = shot::gauss(&mut self.rng) * sigma;
        let ez = shot::gauss(&mut self.rng) * sigma * 0.5;
        let aim_err = v((aim.x + ex).clamp(-7.35, 7.35), (aim.y + ey).clamp(0.2, LENGTH - 0.2));
        let plan = self.plan_pass(kind, rel, aim_err, (sp.z + ez).clamp(0.5, 2.6));
        // The defenders see it go: who goes for it, and where.
        let mut attempt = [None; 5];
        for dk in 0..5 {
            if self.play.stun[dk] > 0.5 {
                continue;
            }
            let see = self.see_time(dk, rel.xy());
            if let Some((p, tau)) = self.reach_for(1 - o, dk, &plan.path, plan.dur, 0.0, see)
                && self.rng.chance(self.try_odds(dk))
            {
                // Hand-eye: his aim at the spot is off by his skill.
                let sig = 0.20 * (1.6 - self.de(dk, Attr::Steal) / 20.0 * 1.1).max(0.4);
                let e = v3(
                    shot::gauss(&mut self.rng) * sig,
                    shot::gauss(&mut self.rng) * sig,
                    shot::gauss(&mut self.rng) * sig * 0.8,
                );
                attempt[dk] = Some((v3(p.x + e.x, p.y + e.y, (p.z + e.z).max(0.3)), tau));
            }
        }
        if std::env::var("DBG").is_ok() {
            eprintln!("release {kind:?} from {:?} h{h}->{j} rel {:?} aim {:?} dur {:.2} recvpos {:?} t {:.1}", self.m[o][h].pos, rel, plan.aim, plan.dur, self.m[o][j].pos, self.play.t);
        }
        self.cut_log(0.0);
        self.log_path(&plan.path, 0.0, 0.0);
        self.play.flight = Some(Flight {
            from: rel.xy(),
            to: plan.aim,
            t: 0.0,
            dur: plan.dur,
            kind: FlightKind::Pass { from: h, to: j },
            path: Some(plan.path),
        });
        self.play.thrown = Some(Thrown { kind, speed: sp.speed, attempt, receiver_seen: false, entered: rel.x.abs() <= 7.5 && rel.y >= 0.0 && rel.y <= LENGTH, catch: None, err: (ex * ex + ey * ey + ez * ez).sqrt(), dbg: std::array::from_fn(|dk| (self.dp(dk), self.hands[1 - o][dk].h[0].p, self.hands[1 - o][dk].h[1].p)) });
        self.play.holder = None;
        self.play.tips = 0;
        self.play.def_touch = false;
        self.play.ball = rel.xy();
        self.play.ball_h = rel.z;
        self.handler.who = None;
        None
    }

    /// One tick of a pass in the air: what the hands do to it.
    pub(super) fn pass_tick(&mut self, f: Flight) -> Option<End> {
        let o = self.play.o;
        let FlightKind::Pass { from, to } = f.kind else { return None };
        let Some(path) = f.path else { return None };
        let Some(th) = self.play.thrown else { return None };
        let steps = 5;
        let mut entered = th.entered;
        let t1 = f.t;
        let t0 = (t1 - DT).max(0.0);
        let mut before = path.at(t0);
        for s in 1..=steps {
            let u = s as f32 / steps as f32;
            let ts = t0 + (t1 - t0) * u;
            let (bp, bz) = path.at(ts);
            let ball = v3(bp.x, bp.y, bz);
            // The glass: a ball that reaches the backboard is off it.
            if before.0.y >= BOARD_Y
                && bp.y < BOARD_Y
                && bp.x.abs() < BOARD_HALF
                && bz > BOARD_LOW
                && bz < BOARD_HIGH
            {
                let (_, _, bv, bvz) = path.state(ts);
                self.cut_log(ts - f.t);
                self.play.flight = None;
                self.play.thrown = None;
                self.note_pass(PassOutcome::Missed, 0.0, None);
                self.play.scramble = Some(Scramble { passer: from, to, tipped_by: None, kind: th.kind, cause: TurnoverKind::BadPass });
                let mut np = Path::new();
                np.fall_open(v(bp.x, BOARD_Y + 0.05), bz, -bvz.abs() * 0.3, v(bv.x * 0.4, bv.y.abs() * 0.4 + 0.5));
                self.play.ball = bp;
                self.play.ball_h = bz;
                return self.go_live(np, 0.0, false);
            }
            before = (bp, bz);
            // A defender's hand on it.
            let mut hit: Option<(usize, f32)> = None;
            for dk in 0..5 {
                let d = 1 - o;
                for i in 0..2 {
                    let h0 = self.hands_prev[d][dk].h[i].p;
                    let h1 = self.hands[d][dk].h[i].p;
                    let hp = v3(h0.x + (h1.x - h0.x) * u, h0.y + (h1.y - h0.y) * u, h0.z + (h1.z - h0.z) * u);
                    let gap = V3 { x: hp.x - ball.x, y: hp.y - ball.y, z: hp.z - ball.z }.len();
                    if gap <= HIT && hit.is_none_or(|(_, g)| gap < g) {
                        hit = Some((dk, gap));
                    }
                }
            }
            if let Some((dk, gap)) = hit {
                return self.defender_on_pass(f, ts, dk, gap, from, to, th);
            }
            // The receiver's.
            if ts >= 0.08 {
                let kind = th.kind;
                let mut best: Option<f32> = None;
                for i in 0..2 {
                    let h0 = self.hands_prev[o][to].h[i].p;
                    let h1 = self.hands[o][to].h[i].p;
                    let hp = v3(h0.x + (h1.x - h0.x) * u, h0.y + (h1.y - h0.y) * u, h0.z + (h1.z - h0.z) * u);
                    let gap = V3 { x: hp.x - ball.x, y: hp.y - ball.y, z: hp.z - ball.z }.len();
                    if gap <= CATCH * if kind == PassKind::Handoff { 1.7 } else { 1.0 } && best.is_none_or(|g| gap < g) {
                        best = Some(gap);
                    }
                }
                if let Some(gap) = best {
                    return self.receiver_on_pass(f, ts, gap, from, to, th);
                }
            }
            // Someone else on his own team: it is his ball if his hand is on it.
            if ts > 0.12 {
                for k in 0..5 {
                    if k == from || k == to {
                        continue;
                    }
                    for i in 0..2 {
                        let h1 = self.hands[o][k].h[i].p;
                        let gap = V3 { x: h1.x - ball.x, y: h1.y - ball.y, z: h1.z - ball.z }.len();
                        if gap <= HIT * 0.8 {
                            return self.stray_catch(f, ts, k, from, to);
                        }
                    }
                }
            }
            // Out of play.
            let outside = bp.x.abs() > 7.5 + BALL_RADIUS || bp.y < -BALL_RADIUS || bp.y > LENGTH + BALL_RADIUS;
            if outside && entered {
                return self.pass_gone(f, ts, from, to, None);
            }
            if !outside && !entered {
                entered = true;
                if let Some(t) = self.play.thrown.as_mut() {
                    t.entered = true;
                }
            }
        }
        if std::env::var("DBG").is_ok() && t1 >= f.dur && t1 < f.dur + DT {
            let (bp, bz) = path.at(t1);
            let h = &self.hands[o][to];
            eprintln!("   at arrival ball {:.2},{:.2},{:.2} recv {:?} hands {:?} {:?} kind {:?} {:?}", bp.x, bp.y, bz, self.m[o][to].pos, h.h[0].p, h.h[1].p, h.h[0].kind, h.h[1].kind);
        }
        if t1 >= f.dur + OVERTIME {
            // Nobody got to it: it is a loose ball now.
            return self.pass_loose(f, t1, from, to, None, kind_of(th));
        }
        None
    }

    /// A defender's hand is on the ball.
    #[allow(clippy::too_many_arguments)]
    fn defender_on_pass(&mut self, f: Flight, ts: f32, dk: usize, gap: f32, from: usize, to: usize, th: Thrown) -> Option<End> {
        let path = f.path.unwrap_or_else(Path::new);
        let (bp, bz, bv, bvz) = path.state(ts);
        if std::env::var("DBG").is_ok() {
            eprintln!("  DEF attempt {} d_to_passer {:.2} d_to_recv {:.2} hands {:?} {:?} z {:.2} {:.2}", th.attempt[dk].is_some(), self.dp(dk).dist(self.m[self.play.o][from].pos), self.dp(dk).dist(self.m[self.play.o][to].pos), self.hands[1-self.play.o][dk].h[0].kind, self.hands[1-self.play.o][dk].h[1].kind, self.hands[1-self.play.o][dk].h[0].p.z, self.hands[1-self.play.o][dk].h[1].p.z);
        }
        if std::env::var("DBG").is_ok() {
            let o2 = self.play.o;
            let (dp0, h0, h1) = th.dbg[dk];
            let now = (self.dp(dk), self.hands[1 - o2][dk].h[0].p, self.hands[1 - o2][dk].h[1].p);
            eprintln!("  TIP ts {ts:.2} ball {:.2},{:.2},{bz:.2} | at release def {:.2},{:.2} h0 {:.2},{:.2},{:.2} h1 {:.2},{:.2},{:.2} | now def {:.2},{:.2} h0 {:.2},{:.2},{:.2} h1 {:.2},{:.2},{:.2} kinds {:?} {:?}", bp.x,bp.y, dp0.x,dp0.y,h0.x,h0.y,h0.z,h1.x,h1.y,h1.z, now.0.x,now.0.y,now.1.x,now.1.y,now.1.z,now.2.x,now.2.y,now.2.z, self.hands[1-o2][dk].h[0].kind, self.hands[1-o2][dk].h[1].kind);
        }
        dbg_out("defender", ts, f.dur, bp, bz);
        let speed = (bv.dot(bv) + bvz * bvz).sqrt();
        let steal = self.de(dk, Attr::Steal);
        let hands = self.de(dk, Attr::Handling);
        let dead_on = (1.0 - gap / HIT).clamp(0.0, 1.0);
        // Taking it cleanly: hands, and a ball that is not too hard, on the
        // palm and not the fingertips.
        let p_take = (0.10 + 0.020 * (0.6 * steal + 0.4 * hands - 10.0) + 0.30 * dead_on - 0.018 * (speed - 8.0).max(0.0))
            .clamp(0.05, 0.85);
        let who = self.did(dk);
        let _ = who;
        self.cut_log(ts - f.t);
        let taken = self.rng.chance(p_take);
        self.note_pass(if taken { PassOutcome::Stolen } else { PassOutcome::Tipped }, gap, Some(dk));
        if taken {
            // Picked off.
            self.steal_flew = 0.0;
            self.play.flight = None;
            self.play.thrown = None;
            let (hp, hz) = self.nearest_hand(1 - self.play.o, dk, bp, bz);
            self.catch_at = Some((hp, hz));
            let end = self.turnover(from, TurnoverKind::BadPass, Some(dk), 1.0);
            self.play.ball = bp;
            return end;
        }
        // Tipped: the ball goes on, changed, and is loose.
        let vel = v(bv.x * 0.35, bv.y * 0.35);
        let (dx, dy) = (self.rng.f32() * 2.0 - 1.0, self.rng.f32() * 2.0 - 1.0);
        let kick = v(dx, dy).norm() * (1.5 + self.rng.f32() * 2.0);
        let mut np = Path::new();
        np.fall_open(bp, bz, (bvz * 0.3).max(0.0) + 1.0 + self.rng.f32(), vel + kick);
        self.play.flight = None;
        self.play.thrown = None;
        self.play.def_touch = true;
        self.play.scramble = Some(Scramble { passer: from, to, tipped_by: Some(dk), kind: th.kind, cause: TurnoverKind::BadPass });
        // A reach that missed the ball costs him his footing a little.
        self.st[1 - self.play.o][dk].balance = (self.st[1 - self.play.o][dk].balance - 0.05).max(0.2);
        self.play.ball = bp;
        self.play.ball_h = bz;
        self.go_live(np, 0.0, false)
    }

    /// The receiver's hand is on it: a catch, or a fumble.
    fn receiver_on_pass(&mut self, f: Flight, ts: f32, gap: f32, from: usize, to: usize, th: Thrown) -> Option<End> {
        let o = self.play.o;
        let path = f.path.unwrap_or_else(Path::new);
        let (bp, bz, bv, bvz) = path.state(ts);
        let speed = (bv.dot(bv) + bvz * bvz).sqrt();
        dbg_out("receiver", ts, f.dur, bp, bz);
        let hands_skill = self.oe(to, Attr::Handling);
        // Stretching for it, a hard pass, not looking, and running all
        // make it harder to hold.
        let me = self.m[o][to].pos;
        let (arm, _, _) = self.arm_of(o, to);
        let stretch = (0..2)
            .map(|i| {
                let sh = self.shoulder_of(o, to, i);
                V3 { x: bp.x - sh.x, y: bp.y - sh.y, z: bz - sh.z }.len() / arm
            })
            .fold(f32::MAX, f32::min)
            .clamp(0.0, 1.3);
        let back = (self.m[o][from].pos - me).norm();
        let looking = (self.st[o][to].gaze.dot(back) * 0.5 + 0.5).clamp(0.0, 1.0);
        let run = self.m[o][to].vel.len();
        let base = 0.996 + 0.0004 * hands_skill;
        let pen = 0.15 * (stretch - 0.85).max(0.0).powi(2)
            + 0.30 * ((th.err - 0.50).max(0.0) / 0.6).min(1.5)
            + 0.03 * (speed - 12.0).max(0.0) / 4.0
            + 0.02 * (1.0 - looking)
            + 0.001 * run
            + 0.012 * (gap / CATCH);
        let p = (base - pen).clamp(0.40, 0.995);
        if std::env::var("DBG").is_ok() {
            eprintln!("  catchp {p:.2} stretch {stretch:.2} speed {speed:.1} looking {looking:.2} run {run:.1} gap {gap:.2}");
        }
        self.cut_log(ts - f.t);
        let held = self.rng.chance(p);
        self.note_pass(if held { PassOutcome::Caught } else { PassOutcome::Bobbled }, gap, None);
        if held {
            self.play.flight = None;
            self.play.thrown = None;
            let (hp, hz) = self.nearest_hand(o, to, bp, bz);
            self.play.ball = hp;
            self.play.ball_h = hz.min(2.4);
            self.ease = 0.5;
            self.ease_from = hp;
            return self.land(f);
        }
        // A bobble: it comes off his hands and is loose where he stands.
        let vel = v(bv.x * 0.25, bv.y * 0.25);
        let (dx, dy) = (self.rng.f32() * 2.0 - 1.0, self.rng.f32() * 2.0 - 1.0);
        let kick = v(dx, dy).norm() * (0.8 + self.rng.f32() * 1.2);
        let mut np = Path::new();
        np.fall_open(bp, bz, 1.2 + self.rng.f32() * 1.2, vel + kick);
        self.play.flight = None;
        self.play.thrown = None;
        self.play.scramble = Some(Scramble { passer: from, to, tipped_by: None, kind: th.kind, cause: TurnoverKind::BadPass });
        self.play.ball = bp;
        self.play.ball_h = bz;
        self.go_live(np, 0.0, false)
    }

    /// A pass a teammate other than the one it was meant for gets a hand to.
    fn stray_catch(&mut self, f: Flight, ts: f32, k: usize, from: usize, to: usize) -> Option<End> {
        let path = f.path.unwrap_or_else(Path::new);
        let (bp, bz, _, _) = path.state(ts);
        self.cut_log(ts - f.t);
        self.play.flight = None;
        self.play.thrown = None;
        let (hp, hz) = self.nearest_hand(self.play.o, k, bp, bz);
        self.play.ball = hp;
        self.play.ball_h = hz.min(2.4);
        self.ease = 0.5;
        self.ease_from = hp;
        let mut g = f;
        g.kind = FlightKind::Pass { from, to: k };
        let _ = to;
        self.land(g)
    }

    /// The ball has left the court.
    fn pass_gone(&mut self, f: Flight, ts: f32, from: usize, to: usize, tipped: Option<usize>) -> Option<End> {
        let path = f.path.unwrap_or_else(Path::new);
        let (bp, bz, _, _) = path.state(ts);
        let _ = (to, tipped);
        dbg_out("gone", ts, f.dur, bp, bz);
        self.cut_log(ts - f.t);
        self.play.flight = None;
        self.play.thrown = None;
        self.note_pass(PassOutcome::Out, 0.0, None);
        let end = self.turnover(from, TurnoverKind::BadPass, None, 0.0);
        let mut p = Path::new();
        p.fall(
            v(bp.x.clamp(-7.8, 7.8), bp.y.clamp(-0.3, LENGTH + 0.3)),
            bz.max(BALL_RADIUS),
            0.0,
            Vec2::ZERO,
        );
        self.loose = Some((p, 0.0));
        end
    }

    /// Nobody has got a hand to it by the time it has passed the man: it
    /// is a loose ball, and his if he is the one nearest.
    fn pass_loose(&mut self, f: Flight, t: f32, from: usize, to: usize, tipped: Option<usize>, kind: PassKind) -> Option<End> {
        let path = f.path.unwrap_or_else(Path::new);
        self.play.flight = None;
        self.play.thrown = None;
        self.play.scramble = Some(Scramble { passer: from, to, tipped_by: tipped, kind, cause: TurnoverKind::BadPass });
        let (bp, bz) = path.at(t);
        self.note_pass(PassOutcome::Missed, 0.0, None);
        dbg_out("loose", t, f.dur, bp, bz);
        self.play.ball = bp;
        self.play.ball_h = bz;
        // The rest of the path goes on from where it is.
        self.go_live(path, t, false)
    }

    /// The hand of man `k` of side `t` nearest the point, and where it is.
    pub(super) fn nearest_hand(&self, t: usize, k: usize, at: Vec2, z: f32) -> (Vec2, f32) {
        let a = self.hands[t][k].h[RIGHT].p;
        let b = self.hands[t][k].h[LEFT].p;
        let da = (a.x - at.x).powi(2) + (a.y - at.y).powi(2) + (a.z - z).powi(2);
        let db = (b.x - at.x).powi(2) + (b.y - at.y).powi(2) + (b.z - z).powi(2);
        let h = if da <= db { a } else { b };
        (v(h.x, h.y), h.z)
    }

    /// Write down what became of the latest pass.
    fn note_pass(&mut self, outcome: PassOutcome, gap: f32, by: Option<usize>) {
        if let Some(rec) = &mut self.rec
            && let Some(p) = rec.passes.last_mut()
        {
            p.outcome = outcome;
            p.contact_cm = (gap * 100.0).round() as u16;
            p.touched_by = by.map(|d| d as u8);
        }
    }

    /// Somebody has got hold of a ball that was thrown, tipped or fumbled.
    pub(super) fn finish_scramble(&mut self, a: After, ts: f32, who: usize, sc: Scramble) -> Option<End> {
        self.cut_log(ts - a.t);
        self.play.after = None;
        let (bp, bz, _, _) = a.path.state(ts);
        self.play.ball = bp;
        self.play.ball_h = bz.min(2.4);
        if who >= 5 {
            // A defender has it: the pass was lost.
            let dk = who - 5;
            self.steal_flew = 0.0;
            let tipped = sc.tipped_by.is_some();
            let (hp, hz) = self.nearest_hand(1 - self.play.o, dk, bp, bz);
            self.catch_at = Some((hp, hz));
            let end = self.turnover(sc.passer, sc.cause, Some(dk), 1.0);
            let _ = tipped;
            return end;
        }
        // The attackers have it back.
        let (hp, hz) = self.nearest_hand(self.play.o, who, bp, bz);
        self.play.ball = hp;
        self.play.ball_h = hz.min(2.4);
        self.ease = 0.5;
        self.ease_from = hp;
        self.play.holder = Some(who);
        self.play.phase = if matches!(self.play.phase, Phase::Advance) { Phase::Advance } else { Phase::Set };
        self.play.think = 0.45 + self.rng.f32() * 0.2;
        self.play.dribbled = false;
        self.play.moves = 0;
        self.play.assist = (who == sc.to && sc.passer != who).then_some((sc.passer, self.play.t));
        self.play.from = (sc.passer != who).then_some(sc.passer);
        self.play.source = Source::CatchAndShoot;
        if self.play.brk {
            self.play.phase = Phase::Advance;
            self.play.source = Source::Transition;
            self.play.think = 0.1;
        }
        None
    }

    /// How a defender's hands move for a pass in the air: the reach he has
    /// decided on, from the time he has seen it go.
    pub(super) fn pass_hand_goals(&self, dk: usize, t_flight: f32) -> Option<V3> {
        let th = self.play.thrown?;
        let (p, tau) = th.attempt[dk]?;
        let see = self.see_time(dk, self.play.ball);
        let _ = tau;
        (t_flight - DT >= see).then_some(p)
    }

    pub(super) fn show_kind(&self, _h: HandKind) {}
}

fn kind_of(th: Thrown) -> PassKind {
    th.kind
}

fn dbg_out(what: &str, ts: f32, dur: f32, bp: Vec2, bz: f32) {
    if std::env::var("DBG").is_ok() {
        eprintln!("  {what} at {ts:.2}/{dur:.2} ball {:.2},{:.2} z {bz:.2}", bp.x, bp.y);
    }
}
