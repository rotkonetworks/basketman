//! The game's side of the stance model (see `engine/stance.rs`): what each
//! body wants to face, the turning, the legwork and balance, the ball
//! handler's dribble, and what goes into the recording. It lives under
//! `floor` to read the game's private state, and keeps `floor.rs` itself to
//! a handful of calls.

use super::*;
use crate::engine::stance::{
    self, CATCH_STEPS, Handler, Want, accel_cap, angle, clamp_gaze, move_cap, turn,
};
use crate::replay::{Dribble, Footwork};

/// Seconds of the free throw routine and so on do not matter here; these do:
/// how far from a loose ball a pass receiver still counts as turned to it.
const HELP_MID: f32 = 0.5;

impl Game {
    /// Where each body wants to face this tick, by role and situation.
    pub(super) fn stance_wants(&self) -> [[Want; 5]; 2] {
        let o = self.play.o;
        let ball = self.play.ball;
        let mut out = [[Want::NONE; 5]; 2];
        let shot_flight = matches!(
            self.play.flight,
            Some(Flight {
                kind: FlightKind::Shot { .. },
                ..
            })
        );
        let shot_up = shot_flight || self.play.after.is_some();
        let plan = self.play.shot.filter(|s| self.play.t >= s.start && self.play.t < s.follow_end);
        let line = matches!(self.play.phase, Phase::Line { .. });
        for t in 0..2 {
            for k in 0..5 {
                let me = self.m[t][k].pos;
                let speed = self.m[t][k].vel.len();
                let mut w = Want::NONE;
                if t == o {
                    let holder = self.play.holder == Some(k);
                    let shooting = plan.filter(|s| s.shooter == k);
                    if let Some(s) = shooting {
                        let to_rim = (HOOP - me).norm();
                        w.gaze = Some(HOOP);
                        match s.style {
                            Style::Hook if self.play.t < s.jump_at => {
                                // Side-on to the rim, the near shoulder to it.
                                let perp = if me.x >= 0.0 {
                                    v(-to_rim.y, to_rim.x)
                                } else {
                                    v(to_rim.y, -to_rim.x)
                                };
                                w.body = Some(me + perp);
                            }
                            _ => w.body = Some(HOOP),
                        }
                        w.planting = matches!(s.style, Style::Jumper | Style::Fadeaway)
                            && self.play.t < s.jump_at;
                    } else if holder && !shot_up {
                        w.handler = true;
                        match self.play.phase {
                            Phase::Post { .. } => {
                                w.body = Some(me + (me - HOOP));
                                w.dropping = true;
                            }
                            Phase::Drive { .. } => {
                                w.gaze = Some(HOOP);
                            }
                            Phase::Advance => {}
                            _ => {
                                w.body = Some(HOOP);
                                w.gaze = Some(HOOP);
                                w.pivoting = true;
                            }
                        }
                    } else if let Some(Flight {
                        kind: FlightKind::Pass { from, to },
                        t: ft,
                        ..
                    }) = self.play.flight
                    {
                        if from == k && ft < 0.35 {
                            let r = self.m[o][to].pos;
                            w.body = Some(r);
                            w.gaze = Some(r);
                        } else if to == k {
                            // Squares to the ball: the hands go to where it comes from.
                            w.body = Some(ball);
                            w.gaze = Some(ball);
                        } else {
                            w.gaze = Some(ball);
                        }
                    } else if shot_up {
                        if me.dist(HOOP) < 7.0 {
                            w.body = Some(HOOP);
                        }
                        w.gaze = Some(ball);
                    } else if matches!(self.play.task[k], Task::SetScreen) {
                        if let Some(dk) = self.play.on_ball {
                            w.body = Some(self.dp(dk));
                        }
                        w.gaze = Some(ball);
                    } else if speed > 1.0 {
                        w.gaze = Some(ball);
                    } else {
                        w.body = Some(ball);
                        w.gaze = Some(ball);
                    }
                } else {
                    let d = &self.sides[t];
                    let _ = d;
                    let mine = self.play.guard[k];
                    let man = self.op(mine);
                    let holder_pos = self.play.holder.map(|h| self.op(h));
                    let target_near = self.m[t][k].target.dist(me) < 3.5;
                    let contest = plan.filter(|s| {
                        self.m[o][s.shooter].pos.dist(me) < 3.2 && self.play.t < s.release + 0.3
                    });
                    let hunting = self
                        .play
                        .hunt
                        .is_some_and(|(hk, spot, _)| hk == k && me.dist(spot) < 0.4);
                    if let Some(s) = contest {
                        let sp = self.m[o][s.shooter].pos;
                        w.body = Some(sp);
                        w.gaze = Some(sp);
                        w.square = true;
                    } else if shot_up {
                        w.body = if me.dist(HOOP) < 6.5 { Some(HOOP) } else { None };
                        w.gaze = Some(ball);
                    } else if line {
                        w.body = Some(ball);
                        w.gaze = Some(ball);
                    } else if self.play.boxout && self.play.holder.is_none() {
                        w.body = Some(HOOP);
                        w.gaze = Some(ball);
                    } else if hunting {
                        w.body = holder_pos;
                        w.gaze = holder_pos;
                        w.square = true;
                    } else if self.play.wait > 0.0 {
                        w.body = Some(ball);
                        w.gaze = Some(ball);
                    } else if let Some(hp) = holder_pos
                        && speed > 2.2
                        && self.m[t][k].vel.dot(hp - me) > 0.0
                        && me.dist(hp) > 1.2
                        && me.dist(hp) < 6.0
                    {
                        w.body = Some(hp);
                        w.gaze = Some(hp);
                        w.closing = true;
                    } else if let Some(hp) = holder_pos
                        && self.defender_of(self.play.holder.unwrap_or(0)) == k
                        && !matches!(self.play.phase, Phase::Advance)
                    {
                        // On the ball: square to the handler and slide, unless
                        // he has been beaten and has to turn and run.
                        let beaten = matches!(self.play.phase, Phase::Drive { beaten: true, .. });
                        w.gaze = Some(hp);
                        if beaten {
                            w.body = None;
                        } else {
                            w.body = Some(hp);
                            w.square = true;
                        }
                    } else if matches!(self.play.phase, Phase::Advance) && !self.play.live_start {
                        // Running back: far from the ball he runs, close to it he
                        // backpedals with his eyes on it.
                        w.gaze = Some(ball);
                        if me.dist(ball) < 9.0 {
                            w.body = Some(ball);
                            w.square = true;
                        }
                    } else {
                        w.gaze = Some(ball);
                        if target_near {
                            // Help side: open to see ball and man.
                            w.body = Some(if is_zone(self.scheme()) {
                                ball
                            } else {
                                man.lerp(ball, HELP_MID)
                            });
                            w.square = true;
                        }
                    }
                }
                out[t][k] = w;
            }
        }
        out
    }

    /// Turn every body and head toward what they want, limited by the rate
    /// his ratings, speed and balance allow.
    pub(super) fn stance_turn(&mut self, wants: &[[Want; 5]; 2]) {
        let sk = self.cache.skill;
        for t in 0..2 {
            for k in 0..5 {
                let m = self.m[t][k];
                let top = self.cache.top[t * 5 + k];
                let st = &mut self.st[t][k];
                let want = wants[t][k];
                let dir = match want.body {
                    Some(p) if p.dist(m.pos) > 0.05 => (p - m.pos).norm(),
                    Some(_) => st.body,
                    None => {
                        let to = m.target - m.pos;
                        if to.len() > 0.35 && m.pace > 0.0 {
                            to.norm()
                        } else if m.vel.len() > 0.5 {
                            m.vel.norm()
                        } else {
                            st.body
                        }
                    }
                };
                let rate = stance::turn_rate(&sk[t * 5 + k], m.vel.len() / top.max(1.0), st.balance);
                let before = st.body;
                st.body = turn(st.body, dir, rate * DT);
                st.turning = angle(before, st.body) / DT;
                let gdir = match want.gaze {
                    Some(p) if p.dist(m.pos) > 0.05 => (p - m.pos).norm(),
                    _ => st.body,
                };
                let g = clamp_gaze(st.body, gdir);
                st.gaze = clamp_gaze(st.body, turn(st.gaze, g, 16.0 * DT));
            }
        }
    }

    /// The top speed and the acceleration limit of one player heading for
    /// `aim`: a share of what he has going forward.
    pub(super) fn stance_limits(&self, t: usize, k: usize, aim: Vec2) -> (f32, f32) {
        let m = &self.m[t][k];
        let to = aim - m.pos;
        let len = to.len();
        if len < 0.3 {
            return (1.0, 1.0);
        }
        let c = to.dot(self.st[t][k].body) / len;
        let sk = &self.cache.skill[t * 5 + k];
        (move_cap(sk, c), accel_cap(c))
    }

    /// After the step: the legwork, balance, the ball handler's dribble.
    pub(super) fn stance_after(&mut self, wants: &[[Want; 5]; 2]) {
        let sk = self.cache.skill;
        for t in 0..2 {
            for k in 0..5 {
                let m = self.m[t][k];
                let i = t * 5 + k;
                let speed = m.vel.len();
                let top = self.cache.top[i];
                let touch = self.touch[i];
                let st = &mut self.st[t][k];
                let c = if speed > 0.3 { m.vel.dot(st.body) / speed } else { 1.0 };
                let acc = (m.vel - st.last_vel).len() / DT * if m.hard { 0.4 } else { 1.0 };
                let contact = if touch.is_some() {
                    (touch.closing * 0.1 + touch.pushed * 4.0).min(1.0)
                } else {
                    0.0
                };
                // Going sideways or backward on the feet is a little unsteady.
                let off_axis = if c < 0.62 { 0.62 - c } else { 0.0 };
                st.balance = stance::balance(st.balance, &sk[i], acc, st.turning, speed, contact, off_axis);
                st.foot = stance::footwork(speed, top, c, &wants[t][k], st.turning);
                st.planted = if matches!(st.foot, Footwork::Pivot | Footwork::DropStep) {
                    let id = self.sides[t].slots[self.sides[t].on[k]].p.id as u8;
                    1 + (id & 1)
                } else {
                    0
                };
                st.last_vel = m.vel;
            }
        }
        self.update_handler();
    }

    /// The ball handler's dribble state, and the rule that once he has picked
    /// it up he stays near where he is.
    fn update_handler(&mut self) {
        let o = self.play.o;
        let who = self.play.holder.map(|h| (o, h));
        if who != self.handler.who {
            self.handler = Handler::NONE;
            self.handler.who = who;
            if let Some((t, h)) = who {
                self.handler.state = Dribble::NotYet;
                self.handler.pivot = self.m[t][h].pos;
                let id = self.sides[t].slots[self.sides[t].on[h]].p.id;
                self.handler.hand = if id % 5 == 0 { -1 } else { 1 };
            }
        }
        let Some((t, h)) = self.handler.who else { return };
        let pos = self.m[t][h].pos;
        let speed = self.m[t][h].vel.len();
        let planned = self
            .play
            .shot
            .is_some_and(|s| s.shooter == h && self.play.t >= s.start);
        let picking = match self.play.phase {
            Phase::Gather { .. } => true,
            Phase::Line { .. } => planned,
            _ => false,
        };
        match self.handler.state {
            Dribble::PickedUp => {
                let off = pos - self.handler.pivot;
                let d = off.len();
                if d > self.handler.radius {
                    let dir = off * (1.0 / d);
                    self.m[t][h].pos = self.handler.pivot + dir * self.handler.radius;
                    let out = self.m[t][h].vel.dot(dir);
                    if out > 0.0 {
                        self.m[t][h].vel = self.m[t][h].vel - dir * out;
                    }
                }
            }
            _ if picking => {
                self.handler.state = Dribble::PickedUp;
                self.handler.pivot = pos;
                let layup = self
                    .play
                    .shot
                    .is_some_and(|s| matches!(s.style, Style::Layup | Style::Dunk));
                self.handler.radius = if layup {
                    2.7
                } else {
                    0.6 + speed * speed / (2.0 * ACCEL * BRAKE) * 1.3
                };
            }
            Dribble::NotYet => {
                let started = self.play.dribbled
                    || matches!(self.play.phase, Phase::Drive { .. })
                    || pos.dist(self.handler.pivot) > CATCH_STEPS;
                if started {
                    self.handler.state = Dribble::Live;
                }
            }
            _ => {}
        }
        if self.handler.state == Dribble::Live {
            self.handler.switched += DT;
            // The hand away from his defender, once in a while.
            let (dk, dist) = self.nearest_defender_of(t, pos);
            if dist < 3.0 && self.handler.switched > 0.8 {
                let away = pos - self.m[1 - t][dk].pos;
                let side = self.st[t][h].body;
                let cross = side.x * away.y - side.y * away.x;
                let hand = if cross >= 0.0 { -1 } else { 1 };
                if hand != self.handler.hand {
                    self.handler.hand = hand;
                    self.handler.switched = 0.0;
                }
            }
        }
    }

    fn nearest_defender_of(&self, t: usize, p: Vec2) -> (usize, f32) {
        let mut best = (0, f32::MAX);
        for dk in 0..5 {
            let d = self.m[1 - t][dk].pos.dist(p);
            if d < best.1 {
                best = (dk, d);
            }
        }
        best
    }

    /// What goes into the recording for one player.
    pub(super) fn stance_pose(&self, t: usize, k: usize, pose: &mut Pose) {
        let st = &self.st[t][k];
        let m = &self.m[t][k];
        pose.body = self.face_court(st.body);
        pose.gaze = self.face_court(st.gaze);
        pose.facing = pose.body;
        let speed = m.vel.len();
        if speed > 0.05 {
            pose.moving = self.face_court(m.vel);
        }
        pose.speed_dm = (speed * 10.0).round().min(255.0) as u8;
        pose.foot = st.foot;
        pose.planted = st.planted;
        pose.balance = (st.balance * 100.0).round() as u8;
        if self.handler.who == Some((t, k)) {
            pose.dribble = self.handler.state;
            if self.handler.state == Dribble::Live {
                pose.hand = self.handler.hand;
            }
        }
    }
}
