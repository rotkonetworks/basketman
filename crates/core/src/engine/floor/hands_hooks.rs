//! The game's side of the hands model (see `engine/hands.rs`): what each hand
//! wants this tick, by role, and what goes into the recording. A hand that is
//! up is not down: it moves at a limited speed toward where it wants to be.

use super::*;
use crate::engine::hands::{self, HandPos, Hands, LEFT, RIGHT};
use crate::replay::{Dribble, HandKind};

/// The dribble's rhythm: seconds a bounce takes at a standstill, and how
/// much shorter it gets at a run.
const BOUNCE_BASE: f32 = 0.64;
const BOUNCE_RUN: f32 = 0.025;
/// Where in the bounce the ball is at the floor (the rest is the way up).
const BOUNCE_DOWN: f32 = 0.42;

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
        let pl = &self.sides[t].slots[self.sides[t].on[k]].p;
        let reach = pl.body.standing_reach_cm / 100.0;
        let arm = (ARM_SHARE * reach).max(0.6) + 0.5 * (pl.body.wingspan_cm - pl.body.height_cm).max(0.0) / 100.0 * 0.3;
        (arm, reach - arm, pl.body.shoulder_width_cm / 200.0)
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
        let me = self.m[t][k].pos;
        let st = &self.st[t][k];
        let speed = me.dist(me + self.m[t][k].vel).min(7.0);
        let period = (BOUNCE_BASE - BOUNCE_RUN * speed).max(0.38);
        let u = (self.anim / period).fract();
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
        let out = (0.42 - 0.012 * skill + 0.012 * speed).clamp(0.18, 0.6);
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
        for t in 0..2 {
            for k in 0..5 {
                let (arm, _, _) = self.arm_of(t, k);
                let slot = self.sides[t].on[k];
                let quick = self.sides[t].eff(slot, if t == o { Attr::Handling } else { Attr::Steal });
                let speed = hands::hand_speed(quick, self.sides[t].eff(slot, Attr::Handling));
                let me = self.m[t][k].pos;
                let body = self.st[t][k].body;
                let lift = self.lift_at(t, k, 0.0);
                let sh = [self.shoulder_of(t, k, RIGHT), self.shoulder_of(t, k, LEFT)];
                let mut goal = [
                    (hands::rest_goal(sh[0], body, RIGHT, arm), HandKind::Rest),
                    (hands::rest_goal(sh[1], body, LEFT, arm), HandKind::Rest),
                ];
                let held = t == o && holder == Some(k);
                if held {
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
                        // A target, then the catch: the hand on the ball's side.
                        let i = if hands::right_of(body).dot(ball - me) >= 0.0 { RIGHT } else { LEFT };
                        let near = ball.dist(me) < 2.2;
                        goal[i] = (v3(ball.x, ball.y, ball_z), if near { HandKind::Catch } else { HandKind::Target });
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
                    } else if on_ball && holder.is_some() {
                        let to_b = (ball - me).norm();
                        let i = if hands::right_of(body).dot(ball - me) >= 0.0 { RIGHT } else { LEFT };
                        let exposed = self.handler.state == Dribble::Live && self.bounce(o, holder.unwrap_or(0)).exposed;
                        let g = if exposed {
                            v3(ball.x, ball.y, ball_z.min(sh[i].z))
                        } else {
                            hands::guard_goal(sh[i], to_b, i, arm)
                        };
                        goal[i] = (g, if exposed { HandKind::Reach } else { HandKind::Guard });
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
                let hs = &mut self.hands[t][k];
                for i in 0..2 {
                    hs.h[i].kind = goal[i].1;
                    hands::step(&mut hs.h[i], goal[i].0, sh[i], arm, speed, DT);
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
            let hc = to_court(v(h.p.x, h.p.y), f);
            pose.hands[i] = hands::record(h, hc, c, h.kind);
        }
    }
}

pub(super) type V3Pos = ball::V3;
pub(super) fn _unused(_: Hands) {}
