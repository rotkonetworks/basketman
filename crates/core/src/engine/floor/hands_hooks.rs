//! The game's side of the hands model (see `engine/hands.rs`): what each hand
//! wants this tick, by role, and what goes into the recording. A hand that is
//! up is not down: it moves at a limited speed toward where it wants to be.

use super::*;
use crate::engine::ball::V3;
use crate::engine::hands::{self, HandPos, Hands, LEFT, RIGHT};
use crate::replay::{Dribble, HandKind};

/// The dribble's rhythm: seconds a bounce takes at a standstill, and how
/// much shorter it gets at a run.
const BOUNCE_BASE: f32 = 0.64;
const BOUNCE_RUN: f32 = 0.025;
/// Where in the bounce the ball is at the floor (the rest is the way up).
const BOUNCE_DOWN: f32 = 0.42;

/// What a man's arms are, and how fast his hands go.
#[derive(Copy, Clone)]
pub(super) struct HandK {
    pub slot: usize,
    pub arm: f32,
    pub shoulder_z: f32,
    pub half_width: f32,
    pub speed: f32,
    pub side: bool,
}

impl HandK {
    pub const NONE: HandK = HandK { slot: usize::MAX, arm: 1.0, shoulder_z: 1.7, half_width: 0.2, speed: 8.0, side: false };
}

/// What the ball is doing in a dribbling man's hand this tick.
#[derive(Copy, Clone)]
pub(super) struct Bounce {
    pub at: Vec2,
    pub z: f32,
    /// The ball is away from the hand, where a defender can get at it.
    pub exposed: bool,
}

impl Game {
    /// The reach numbers of a man: the arm (shoulder to the middle of the
    /// hand), the shoulder's height standing, and half his shoulder width.
    pub(super) fn arm_of(&self, t: usize, k: usize) -> (f32, f32, f32) {
        let c = &self.hand_k[t * 5 + k];
        if c.slot == self.sides[t].on[k] {
            return (c.arm, c.shoulder_z, c.half_width);
        }
        Self::arm_fresh(&self.sides[t].slots[self.sides[t].on[k]].p.body)
    }

    fn arm_fresh(body: &crate::player::Body) -> (f32, f32, f32) {
        let reach = body.standing_reach_cm / 100.0;
        let arm = (ARM_SHARE * reach).max(0.6) + 0.5 * (body.wingspan_cm - body.height_cm).max(0.0) / 100.0 * 0.3;
        (arm, reach - arm, body.shoulder_width_cm / 200.0)
    }

    /// The numbers that do not change from tick to tick, kept for twenty.
    fn refresh_hand_k(&mut self) {
        for t in 0..2 {
            for k in 0..5 {
                let slot = self.sides[t].on[k];
                let (arm, shoulder_z, half_width) = Self::arm_fresh(&self.sides[t].slots[slot].p.body);
                let quick = self.sides[t].eff(slot, if t == self.play.o { Attr::Handling } else { Attr::Steal });
                let speed = hands::hand_speed(quick, self.sides[t].eff(slot, Attr::Handling));
                self.hand_k[t * 5 + k] = HandK { slot, arm, shoulder_z, half_width, speed, side: t == self.play.o };
            }
        }
    }

    /// Where shoulder `i` of man `k` of side `t` is now.
    pub(super) fn shoulder_of(&self, t: usize, k: usize, i: usize) -> V3Pos {
        let (_, z, hw) = self.arm_of(t, k);
        let lift = self.lift_at(t, k, 0.0);
        hands::shoulder(self.m[t][k].pos, self.st[t][k].body, i, hw, z + lift)
    }

    /// The ball in a live dribble: out to the dribbling hand's side and a
    /// little ahead, down to the floor and back up on a rhythm that is
    /// quicker at a run. How far out it goes is what handling decides: a
    /// good handler keeps it close and low, and the weak hand sends it out.
    pub(super) fn bounce(&self, t: usize, k: usize) -> Bounce {
        self.bounce_at(t, k, self.anim)
    }

    /// The same, `anim` seconds into the game (for the fine steps of a tick).
    pub(super) fn bounce_at(&self, t: usize, k: usize, anim: f32) -> Bounce {
        let me = self.m[t][k].pos;
        let st = &self.st[t][k];
        let speed = me.dist(me + self.m[t][k].vel).min(7.0);
        let period = (BOUNCE_BASE - BOUNCE_RUN * speed).max(0.38);
        let u = (anim / period).fract();
        let (arm, sz, _) = self.arm_of(t, k);
        let top = (sz - 0.30).clamp(0.8, 1.2) + 0.0 * arm;
        let r = BALL_RADIUS;
        let z = if u < BOUNCE_DOWN {
            let w = u / BOUNCE_DOWN;
            top - (top - r) * w * w
        } else {
            let w = (u - BOUNCE_DOWN) / (1.0 - BOUNCE_DOWN);
            r + (top - r) * (1.0 - (1.0 - w) * (1.0 - w))
        };
        let hand = if self.handler.hand >= 0 { RIGHT } else { LEFT };
        let pl = &self.sides[t].slots[self.sides[t].on[k]].p;
        // Dominant hand or not.
        let strong = (pl.body.hand == crate::player::Hand::Right) == (hand == RIGHT);
        let skill = if strong {
            self.sides[t].eff(self.sides[t].on[k], Attr::Handling)
        } else {
            0.5 * (self.sides[t].eff(self.sides[t].on[k], Attr::Handling) + pl.off_hand_skill())
        };
        let out = (0.55 - 0.025 * skill + 0.012 * speed).clamp(0.16, 0.7);
        let side = hands::right_of(st.body) * (hands::sign(hand) * out);
        // The ball is furthest from him at the floor and back at the hand.
        let near = (1.0 - ((z - r) / (top - r)).clamp(0.0, 1.0)) * 0.8 + 0.2;
        let ahead = st.body * (0.20 * near) + self.m[t][k].vel * 0.12;
        let at = me + side * near + ahead;
        Bounce {
            at,
            z,
            exposed: z < top - 0.28,
        }
    }

    /// One tick of every hand: where it wants to be, and how far it gets.
    pub(super) fn hands_tick(&mut self) {
        let o = self.play.o;
        let ball = self.play.ball;
        let ball_z = self.play.ball_h;
        let holder = self.play.holder;
        let flight = self.play.flight;
        self.hands_prev = self.hands;
        if self.ticks.is_multiple_of(20)
            || (0..10).any(|i| self.hand_k[i].slot != self.sides[i / 5].on[i % 5] || self.hand_k[i].side != (i / 5 == o))
        {
            self.refresh_hand_k();
        }
        for t in 0..2 {
            for k in 0..5 {
                let (arm, _, _) = self.arm_of(t, k);
                let speed = self.hand_k[t * 5 + k].speed;
                let me = self.m[t][k].pos;
                let body = self.st[t][k].body;
                let lift = self.lift_at(t, k, 0.0);
                let sh = [self.shoulder_of(t, k, RIGHT), self.shoulder_of(t, k, LEFT)];
                let mut goal = [
                    (hands::rest_goal(sh[0], body, RIGHT, arm), HandKind::Rest),
                    (hands::rest_goal(sh[1], body, LEFT, arm), HandKind::Rest),
                ];
                let held = t == o && holder == Some(k);
                let winding = t == o && self.play.throw.is_some_and(|th| th.from == k);
                if winding {
                    let th = self.play.throw.unwrap_or_else(|| unreachable!());
                    let p = th.rel;
                    if th.two {
                        goal = [(p, HandKind::Throw), (p, HandKind::Throw)];
                    } else {
                        goal[th.hand] = (p, HandKind::Throw);
                        let j = 1 - th.hand;
                        goal[j] = (hands::guard_goal(sh[j], body, j, arm), HandKind::Guard);
                    }
                } else if held {
                    let shooting = self.play.shot.is_some_and(|s| s.shooter == k)
                        && matches!(self.play.phase, Phase::Gather { .. } | Phase::Air | Phase::Line { .. });
                    if shooting {
                        let up = v3(me.x + body.x * 0.25, me.y + body.y * 0.25, sh[0].z + 0.55 * arm);
                        goal = [(up, HandKind::Hold), (up, HandKind::Hold)];
                    } else if self.handler.state == Dribble::Live {
                        let b = self.bounce(t, k);
                        let d = if self.handler.hand >= 0 { RIGHT } else { LEFT };
                        let f = 1 - d;
                        goal[d] = (v3(b.at.x, b.at.y, b.z + 0.1), HandKind::Dribble);
                        // The free hand is up between the ball and the man.
                        let (dk, dist) = self.nearest_defender_of(t, me);
                        let toward = if dist < 4.0 { (self.m[1 - t][dk].pos - me).norm() } else { body };
                        let g = hands::guard_goal(sh[f], toward, f, arm);
                        goal[f] = (g, HandKind::Guard);
                    } else {
                        let c = v3(me.x + body.x * 0.35, me.y + body.y * 0.35, sh[0].z - 0.2 * arm);
                        goal = [(c, HandKind::Hold), (c, HandKind::Hold)];
                    }
                } else if let Some(Flight {
                    kind: FlightKind::Pass { from, to },
                    ..
                }) = flight
                    && t == o
                    && (k == from || k == to)
                {
                    if k == to {
                        // A target, then the catch: the hand on the ball's side,
                        // taken to where the ball will be a moment on.
                        let (ft, path, dur) = flight.map(|f| (f.t, f.path, f.dur)).unwrap_or((0.0, None, 0.0));
                        let (ap, az) = match path {
                            Some(p) => {
                                // Where a hand can meet it: kept once chosen
                                // while it is still within reach, so that the
                                // hand waits for the ball.
                                let arm2 = arm + 0.45;
                                let kept = self.play.thrown.and_then(|th| th.catch).filter(|(q, tau)| {
                                    *tau + 0.06 > ft
                                        && (0..2).any(|i| {
                                            let d = V3 { x: q.x - sh[i].x, y: q.y - sh[i].y, z: q.z - sh[i].z };
                                            d.len() <= arm2
                                        })
                                });
                                let choice = kept.or_else(|| {
                                    let see = 0.06 + 0.25 * (1.0 - (self.st[t][k].gaze.dot((self.m[t][from].pos - me).norm()) * 0.5 + 0.5).clamp(0.0, 1.0));
                                    self.catch_point(t, k, &p, dur, ft, see)
                                });
                                if let Some(th) = self.play.thrown.as_mut() {
                                    th.catch = choice;
                                }
                                match choice {
                                    Some((q, _)) => (q.xy(), q.z),
                                    None => p.at(ft + 0.10),
                                }
                            }
                            None => (ball, ball_z),
                        };
                        let i = if hands::right_of(body).dot(ap - me) >= 0.0 { RIGHT } else { LEFT };
                        let near = ball.dist(me) < 2.2;
                        goal[i] = (v3(ap.x, ap.y, az), if near { HandKind::Catch } else { HandKind::Target });
                        let j = 1 - i;
                        goal[j] = (hands::guard_goal(sh[j], body, j, arm), HandKind::Guard);
                    } else {
                        // Following through.
                        let to_p = self.m[t][to].pos;
                        let dir = (to_p - me).norm();
                        for i in 0..2 {
                            goal[i] = (v3(me.x + dir.x * 0.7, me.y + dir.y * 0.7, sh[i].z - 0.2 * arm), HandKind::Throw);
                        }
                    }
                } else if t != o && matches!(flight, Some(Flight { kind: FlightKind::Pass { .. }, .. })) && self.pass_hand_goals(k, flight.map_or(0.0, |f| f.t)).is_some() {
                    let g = self.pass_hand_goals(k, flight.map_or(0.0, |f| f.t)).unwrap_or_else(|| unreachable!());
                    // The hand nearer the ball goes for it, the other is up.
                    let i = if (g.x - sh[0].x).powi(2) + (g.y - sh[0].y).powi(2) + (g.z - sh[0].z).powi(2)
                        <= (g.x - sh[1].x).powi(2) + (g.y - sh[1].y).powi(2) + (g.z - sh[1].z).powi(2)
                    {
                        RIGHT
                    } else {
                        LEFT
                    };
                    goal[i] = (g, HandKind::Reach);
                    let j = 1 - i;
                    goal[j] = (hands::guard_goal(sh[j], body, j, arm), HandKind::Guard);
                } else if t != o {
                    // A defender.
                    let man = self.play.guard[k];
                    let on_ball = self.play.on_ball == Some(k)
                        || holder.is_some_and(|h| self.m[o][h].pos.dist(me) < 1.8);
                    let shot_in_air = matches!(flight, Some(Flight { kind: FlightKind::Shot { .. }, .. }))
                        || matches!(self.play.phase, Phase::Gather { .. });
                    if shot_in_air && ball.dist(me) < 3.2 && lift >= 0.0 {
                        // Up in the shooter's face.
                        let up = v3(me.x + (ball.x - me.x) * 0.15, me.y + (ball.y - me.y) * 0.15, sh[0].z + 0.8 * arm);
                        goal = [(up, HandKind::Up), (up, HandKind::Up)];
                    } else if !on_ball
                        && holder.is_none()
                        && let Some(Flight { kind: FlightKind::Pass { from, .. }, t: ft, .. }) = flight
                        && ft < 0.6
                        && self.m[o][from].pos.dist(me) < 2.4
                    {
                        // He has just thrown it: the man on him keeps his hands
                        // where they were, in front of him, until it is gone.
                        let to_b = (self.m[o][from].pos - me).norm();
                        for i in 0..2 {
                            goal[i] = (hands::guard_goal(sh[i], to_b, i, arm), HandKind::Guard);
                        }
                    } else if on_ball && holder.is_some() {
                        let to_b = (ball - me).norm();
                        let i = if hands::right_of(body).dot(ball - me) >= 0.0 { RIGHT } else { LEFT };
                        let exposed = self.handler.state == Dribble::Live && self.bounce(o, holder.unwrap_or(0)).exposed;
                        let g = if let Some(e) = self.poke.tries[k].filter(|_| exposed) {
                            // Going for the ball, with the aim his hands have.
                            v3(ball.x + e.x, ball.y + e.y, (ball_z + e.z).clamp(0.2, sh[i].z))
                        } else {
                            hands::guard_goal(sh[i], to_b, i, arm)
                        };
                        goal[i] = (g, if exposed && self.poke.tries[k].is_some() { HandKind::Reach } else { HandKind::Guard });
                        let j = 1 - i;
                        goal[j] = (hands::guard_goal(sh[j], to_b, j, arm), HandKind::Guard);
                    } else if let Some(h) = holder.filter(|&h| h != man && man < 5) {
                        // A hand in the lane to his man, when the ball is a pass away.
                        let a = self.m[o][h].pos;
                        let b = self.m[o][man].pos;
                        if a.dist(b) < 11.0 && me.dist(b) < 3.0 {
                            let lane = me.project(a, b);
                            let i = if hands::right_of(body).dot(lane - me) >= 0.0 { RIGHT } else { LEFT };
                            goal[i] = (v3(lane.x, lane.y, sh[i].z - 0.1 * arm), HandKind::Lane);
                            let j = 1 - i;
                            goal[j] = (hands::guard_goal(sh[j], body, j, arm), HandKind::Guard);
                        } else if me.dist(ball) < 6.0 {
                            for i in 0..2 {
                                goal[i] = (hands::guard_goal(sh[i], body, i, arm), HandKind::Guard);
                            }
                        }
                    }
                } else if t == o
                    && let Some(h) = holder.filter(|&h| h != k)
                {
                    // A man without the ball, looking at the man who has it,
                    // shows him a hand.
                    let hp = self.m[o][h].pos;
                    if me.dist(hp) < 10.0 && me.dist(hp) > 1.5 && self.st[o][k].gaze.dot((hp - me).norm()) > 0.5 {
                        let to_h = (hp - me).norm();
                        let i = if hands::right_of(body).dot(to_h) >= 0.0 { RIGHT } else { LEFT };
                        goal[i] = (v3(me.x + to_h.x * 0.5 * arm, me.y + to_h.y * 0.5 * arm, sh[i].z + 0.15 * arm), HandKind::Target);
                    }
                }
                let rel = &mut self.hand_rel[t][k];
                let hs = &mut self.hands[t][k];
                for i in 0..2 {
                    // The hand goes with the shoulder: a body that runs, turns, jumps
                    // or is put somewhere else takes its hands with it, and the hand
                    // moves from there by its own speed.
                    if !rel[i].z.is_nan() {
                        hs.h[i].p = v3(sh[i].x + rel[i].x, sh[i].y + rel[i].y, sh[i].z + rel[i].z);
                    }
                    hs.h[i].kind = goal[i].1;
                    hands::step(&mut hs.h[i], goal[i].0, sh[i], arm, speed, DT);
                    rel[i] = V3 { x: hs.h[i].p.x - sh[i].x, y: hs.h[i].p.y - sh[i].y, z: hs.h[i].p.z - sh[i].z };
                }
                let _ = HandPos { p: hs.h[0].p, kind: hs.h[0].kind };
            }
        }
    }

    /// What goes into the recording for one player's hands.
    pub(super) fn hands_pose(&self, t: usize, k: usize, pose: &mut Pose) {
        let f = self.frame;
        let c = to_court(self.m[t][k].pos, f);
        for i in 0..2 {
            let h = &self.hands[t][k].h[i];
            // Where the hand is by the end of the tick: with the shoulder, which
            // has moved on with the body since the hands were worked out.
            let rel = self.hand_rel[t][k][i];
            let p = if rel.z.is_nan() {
                h.p
            } else {
                let sh = self.shoulder_of(t, k, i);
                v3(sh.x + rel.x, sh.y + rel.y, sh.z + rel.z)
            };
            let hc = to_court(v(p.x, p.y), f);
            pose.hands[i] = hands::record(&HandPos { p, kind: h.kind }, hc, c, h.kind);
        }
    }
}

pub(super) type V3Pos = ball::V3;
pub(super) fn _unused(_: Hands) {}
