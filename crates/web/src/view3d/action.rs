//! What the recording says a player is doing, as a body: the jump shot, the
//! layup and the dunk, the rebound, the contest and the block, the pass, the
//! box out, the screen, the post up, the brace for a charge and the fall.
//!
//! `gait::pose` makes a player stand, run or dribble. For the actions here
//! `apply` takes that pose and rebuilds the legs, trunk and arms from keyframes
//! that run with the recorded `progress`, with limbs from `ik2`. When the ball
//! has been recorded in the air near a hand, the hand goes to the ball, so that
//! a release, a catch and a block meet the recorded path and not a guess.
//! Everything is in the body frame of `gait`: x forward, y to his left, z up.

use basketman_core::replay::{Action, ShotStage, Style};

use super::gait::{
    ANKLE, Dims, ELBOW, FINGERS, HAIR_HIGH, HAIR_LOW, HEAD_TOP, HEEL, HIP, JERSEY_TOP, KNEE,
    NECK_BASE, NECK_TOP, PELVIS, Pose, SHORTS_END, SHORTS_HIGH, SHORTS_LOW, SHOULDER,
    SHOULDER_MID, TOE, TORSO_LOW, WRIST, ik2, smoothstep,
};
use super::{BALL_RADIUS, Vec3, v3};

/// What one player is doing, from `Frame::pose` and the frame around it.
pub(crate) struct Act {
    pub action: Action,
    pub style: Style,
    pub stage: ShotStage,
    /// How far through the action, 0 to 1.
    pub u: f32,
    /// Feet off the floor, metres.
    pub lift: f32,
    /// Standing reach with one arm up, metres.
    pub reach: f32,
    /// The ball is in his hands (the recording has him holding it).
    pub holds: bool,
    /// The recorded ball in his body frame, when it is not in anybody's hands.
    pub ball: Option<Vec3>,
    /// A layup is carried in the left hand (the hand away from the basket).
    pub outside_left: bool,
}

impl Act {
    /// Does this action take over the body from `gait::pose`?
    pub fn takes_over(&self) -> bool {
        use Action::*;
        matches!(
            self.action,
            Shooting
                | Rebounding
                | Contesting
                | Blocking
                | Passing
                | BoxingOut
                | Screening
                | Bracing
                | PostingUp
                | Down
        )
    }

    /// His feet are planted (no running legs under the action).
    pub fn planted(&self) -> bool {
        match self.action {
            Action::Shooting => {
                !(matches!(self.style, Style::Layup | Style::Dunk)
                    && self.stage == ShotStage::Gather)
                    && !(self.stage == ShotStage::Gather && self.u < 0.01)
            }
            Action::Rebounding | Action::Contesting | Action::Blocking | Action::Down => true,
            _ => false,
        }
    }
}

/// How the limbs and trunk go, before they are built.
#[derive(Default)]
struct Plan {
    /// Pelvis lowered by this much.
    crouch: f32,
    /// Trunk pitched forward by this angle (radians).
    lean: f32,
    /// Shoulders turned about the vertical (positive turns the left shoulder forward).
    twist: f32,
    /// Feet this far further apart than a stance.
    wide: f32,
    /// Where each wrist goes, left then right, grounded body frame.
    wrist: [Option<Vec3>; 2],
    /// Where each hand's palm leads when the wrist is placed (flick down).
    flick: [f32; 2],
    /// Foot offsets from where it would stand, left then right.
    foot: [Vec3; 2],
    /// Toes pointed (in the air).
    point: f32,
    /// The ball, if he holds it.
    ball: Option<Vec3>,
    /// Lie down by this much (0 standing, 1 flat on his back).
    lie: f32,
}

fn ease(x: f32) -> f32 {
    smoothstep(0.0, 1.0, x)
}

fn mix(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}

/// The wrist that puts the centre of a hand on `ball`, from shoulder `sh`.
fn grip(d: &Dims, sh: Vec3, ball: Vec3) -> Vec3 {
    let dir = (ball - sh).norm();
    ball - dir * (0.5 * d.hand + 0.4 * BALL_RADIUS)
}

fn plan(d: &Dims, a: &Act, base: &Pose) -> Plan {
    let mut p = Plan::default();
    let dom = if d.hand_side > 0.0 { 0usize } else { 1 };
    let sg = |sd: usize| if sd == 0 { 1.0 } else { -1.0 };
    let arm = d.upper_arm + d.forearm;
    let sz = base.j[SHOULDER].z;
    let pz = base.j[PELVIS].z;
    let sh = d.shoulder_half;
    let reach = a.reach;
    let u = a.u.clamp(0.0, 1.0);
    // The ball as the recording has it, in the frame with his feet on the floor.
    let rec = a.ball.map(|b| b - v3(0.0, 0.0, a.lift));
    let chest = v3(0.17 * d.height, sg(dom) * 0.06, pz + 0.30 * d.torso);
    let up_fwd = |s: f32| v3(0.22, s * (sh + 0.02), reach - 0.12);
    match a.action {
        Action::Shooting => {
            let style = a.style;
            let layup = matches!(style, Style::Layup | Style::Dunk);
            let sd = if layup {
                if a.outside_left { 0 } else { 1 }
            } else {
                dom
            };
            let other = 1 - sd;
            let s = sg(sd);
            // Where the ball is at the top, with his hand under it.
            let top = if layup {
                v3(0.32, s * 0.08, reach + 0.02)
            } else if style == Style::Hook {
                v3(0.04, s * 0.30, reach - 0.10)
            } else {
                v3(0.20, s * 0.03, reach - 0.10)
            };
            let low = if style == Style::Hook {
                v3(0.02, s * (sh + 0.38), pz + 0.05)
            } else if layup {
                v3(0.16, s * (sh + 0.10), pz + 0.12)
            } else {
                // The set point: above the forehead, ball at his shooting shoulder.
                v3(0.22, s * 0.05, sz + 0.14)
            };
            let hold = chest;
            let lift_frac = if a.lift > 0.0 { 1.0 } else { 0.0 };
            match a.stage {
                ShotStage::Gather => {
                    let e = ease(u);
                    p.crouch = d.leg * if layup { 0.10 } else { 0.24 } * e;
                    p.lean = 0.10 * e;
                    p.ball = Some(mix(hold, low, e));
                    p.twist = if style == Style::Hook { -0.5 * s * e } else { 0.0 };
                }
                ShotStage::Rise => {
                    let e = ease(u);
                    p.crouch = d.leg * 0.24 * (1.0 - ease(u * 2.0).min(1.0)) * (if layup { 0.5 } else { 1.0 });
                    p.lean = 0.10 * (1.0 - e);
                    p.point = lift_frac * 0.6;
                    p.ball = Some(mix(low, top, ease((u * 1.15).min(1.0))));
                    if layup {
                        // Knee up on the ball side; the other foot trails.
                        p.foot[sd] = v3(0.28 * e, 0.0, d.leg * 0.40 * e);
                        p.foot[other] = v3(-0.12 * e, 0.0, d.leg * 0.05 * e);
                    } else {
                        p.foot[0] = v3(0.0, 0.0, 0.03 * e);
                        p.foot[1] = v3(0.0, 0.0, 0.03 * e);
                    }
                    p.twist = if style == Style::Hook { -0.5 * s * (1.0 - e) } else { 0.0 };
                }
                ShotStage::Release | ShotStage::Fall | ShotStage::Follow => {
                    let released = a.ball.is_some();
                    p.point = if a.stage == ShotStage::Follow { 0.0 } else { 0.5 };
                    let hang = v3(0.06, s * (sh + 0.04), sz - arm * 0.85);
                    let held_top = rec.filter(|b| b.z > 1.5).unwrap_or(top);
                    let fol = if a.stage == ShotStage::Follow {
                        // Wrist held until he turns away, then the arm comes down.
                        ease(((u - 0.7) / 0.3).clamp(0.0, 1.0))
                    } else {
                        0.0
                    };
                    let target = if released || a.stage != ShotStage::Follow {
                        held_top
                    } else {
                        top
                    };
                    p.ball = None;
                    let wr = grip(d, v3(0.0, s * sh, sz), target);
                    p.wrist[sd] = Some(mix(wr, hang, fol));
                    p.flick[sd] = if a.stage == ShotStage::Release { u } else { 1.0 - fol };
                    // The other hand drops away from the ball.
                    p.wrist[other] = Some(mix(
                        v3(0.14, -s * (sh + 0.2), sz + 0.05),
                        v3(0.04, -s * (sh + 0.05), sz - arm * 0.8),
                        fol,
                    ));
                    if layup {
                        p.foot[sd] = v3(0.2, 0.0, d.leg * 0.30 * (1.0 - fol));
                        p.foot[other] = v3(-0.1, 0.0, d.leg * 0.05);
                        p.lean = 0.05;
                        if style == Style::Dunk {
                            p.wrist[other] = Some(grip(d, v3(0.0, -s * sh, sz), held_top + v3(0.0, -s * 0.1, 0.0)));
                        }
                    }
                    if a.stage == ShotStage::Fall {
                        p.crouch = d.leg * 0.1 * ease(u);
                    }
                    if a.stage == ShotStage::Follow {
                        p.crouch = d.leg * 0.12 * (1.0 - ease(u * 2.0).min(1.0));
                    }
                    return finish(p, d, a, base);
                }
            }
            // Ball in hand until the release: both hands on it.
            if let Some(b) = p.ball {
                let guide = if style == Style::Hook { None } else { Some(b + v3(0.0, -s * (BALL_RADIUS + 0.03), 0.0)) };
                p.wrist[sd] = Some(grip(d, v3(0.0, s * sh, sz), b + v3(0.0, s * 0.02, -0.04)));
                let g_far = u > 0.55 && a.stage == ShotStage::Rise;
                p.wrist[other] = match guide {
                    Some(g) if !g_far => Some(grip(d, v3(0.0, -s * sh, sz), g)),
                    Some(_) => Some(v3(0.16, -s * (sh + 0.18), sz + 0.1)),
                    None => Some(v3(0.2, -s * (sh + 0.1), sz + 0.1)),
                };
            }
        }
        Action::Rebounding => {
            p.point = 0.5 * if a.lift > 0.0 { 1.0 } else { 0.0 };
            p.crouch = d.leg * 0.12 * (1.0 - ease(u * 4.0).min(1.0));
            let overhead = v3(0.18, 0.0, reach - 0.12);
            if a.holds {
                // He has it: from over his head down to his chest, elbows out.
                let e = ease(((u - 0.5) / 0.5).clamp(0.0, 1.0));
                let b = mix(overhead, v3(0.2, 0.0, pz + 0.3 * d.torso), e);
                p.ball = Some(b);
                p.wrist[0] = Some(grip(d, v3(0.0, sh, sz), b + v3(0.0, BALL_RADIUS * 0.2, 0.0)));
                p.wrist[1] = Some(grip(d, v3(0.0, -sh, sz), b + v3(0.0, -BALL_RADIUS * 0.2, 0.0)));
                p.crouch += d.leg * 0.1 * e;
            } else {
                for sd in 0..2 {
                    let t = match rec.filter(|b| (*b - overhead).len() < 1.3) {
                        Some(b) => b + v3(0.0, sg(sd) * (BALL_RADIUS - 0.02), 0.0),
                        None => up_fwd(sg(sd)),
                    };
                    p.wrist[sd] = Some(grip(d, v3(0.0, sg(sd) * sh, sz), t));
                }
            }
            p.foot[0] = v3(0.0, 0.0, 0.04);
            p.foot[1] = v3(0.0, 0.0, 0.04);
        }
        Action::Contesting | Action::Blocking => {
            p.point = 0.4 * if a.lift > 0.0 { 1.0 } else { 0.0 };
            p.crouch = d.leg * 0.10 * (1.0 - ease(u * 4.0).min(1.0));
            let blk = a.action == Action::Blocking;
            for sd in 0..2 {
                let lead = sd == dom;
                let mut t = if lead {
                    v3(0.30, sg(sd) * 0.10, reach - 0.04)
                } else {
                    v3(0.22, sg(sd) * (sh + 0.12), reach - 0.30)
                };
                if blk && lead && let Some(b) = rec {
                    t = b;
                }
                p.wrist[sd] = Some(grip(d, v3(0.0, sg(sd) * sh, sz), t));
            }
            p.foot[0] = v3(0.0, 0.0, 0.03);
            p.foot[1] = v3(0.0, 0.0, 0.03);
        }
        Action::Passing => {
            let e = ease((u * 2.0).min(1.0));
            let from = chest;
            let to = match rec {
                Some(b) if b.x > 0.0 => b,
                _ => v3(0.55, 0.0, sz - 0.1),
            };
            let b = mix(from, to, e);
            for sd in 0..2 {
                p.wrist[sd] = Some(grip(d, v3(0.0, sg(sd) * sh, sz), b + v3(0.0, sg(sd) * 0.1, 0.0)));
            }
            p.lean = 0.12 * e;
            p.crouch = d.leg * 0.04;
        }
        Action::BoxingOut => {
            p.crouch = d.leg * 0.14;
            p.wide = 0.20;
            p.lean = 0.16;
            for sd in 0..2 {
                p.wrist[sd] = Some(v3(0.18, sg(sd) * (sh + arm * 0.80), sz - 0.12));
            }
        }
        Action::Screening => {
            p.crouch = d.leg * 0.08;
            p.wide = 0.14;
            for sd in 0..2 {
                p.wrist[sd] = Some(v3(0.20, -sg(sd) * 0.07, sz - 0.30));
            }
        }
        Action::Bracing => {
            p.crouch = d.leg * 0.10;
            p.wide = 0.10;
            p.lean = -0.06;
            for sd in 0..2 {
                p.wrist[sd] = Some(v3(0.17, -sg(sd) * 0.06, sz - 0.24));
            }
        }
        Action::PostingUp => {
            p.crouch = d.leg * 0.16;
            p.wide = 0.22;
            p.lean = -0.14;
            for sd in 0..2 {
                p.wrist[sd] = Some(v3(0.12, sg(sd) * (sh + arm * 0.55), sz + arm * 0.35));
            }
        }
        Action::Down => {
            // Falls over the first third, lies, gets up over the last third.
            let down = ease((u / 0.3).clamp(0.0, 1.0));
            let up = ease(((u - 0.7) / 0.3).clamp(0.0, 1.0));
            p.lie = down * (1.0 - up);
            for sd in 0..2 {
                p.wrist[sd] = Some(v3(0.0, sg(sd) * (sh + arm * 0.5), sz - arm * 0.5));
            }
        }
        _ => {}
    }
    finish(p, d, a, base)
}

fn finish(p: Plan, _d: &Dims, _a: &Act, _base: &Pose) -> Plan {
    p
}

/// Turn the point `q` about the vertical through `c` by `a`.
fn yaw(q: Vec3, c: Vec3, a: f32) -> Vec3 {
    let (s, co) = a.sin_cos();
    let (x, y) = (q.x - c.x, q.y - c.y);
    v3(c.x + x * co - y * s, c.y + x * s + y * co, q.z)
}

/// Turn `q` forward about the y axis through `c` by `a` (the top goes forward).
fn pitch(q: Vec3, c: Vec3, a: f32) -> Vec3 {
    let (s, co) = a.sin_cos();
    let (x, z) = (q.x - c.x, q.z - c.z);
    v3(c.x + x * co + z * s, q.y, c.z - x * s + z * co)
}

/// Rebuild `pose` for the action. Returns where the ball is when he holds it.
pub(crate) fn apply(pose: &mut Pose, d: &Dims, a: &Act) {
    let base = pose.clone();
    let pl = plan(d, a, &base);
    let j = &mut pose.j;
    let pel0 = base.j[PELVIS];
    let pel = v3(pel0.x, 0.0, pel0.z - pl.crouch);
    let hip_half = d.hip_half + pl.wide * 0.5;
    // Trunk and head: pitched about the pelvis, lowered with it.
    let trunk = [
        TORSO_LOW,
        SHORTS_HIGH,
        SHOULDER_MID,
        SHOULDER,
        SHOULDER + 1,
        NECK_BASE,
        NECK_TOP,
        HEAD_TOP,
        HAIR_LOW,
        HAIR_HIGH,
        JERSEY_TOP,
    ];
    for &i in &trunk {
        let q = base.j[i] - pel0;
        j[i] = pitch(q, v3(0.0, 0.0, 0.0), pl.lean) + pel;
    }
    j[PELVIS] = pel;
    j[SHORTS_LOW] = pel + (base.j[SHORTS_LOW] - pel0);
    if pl.twist != 0.0 {
        let c = j[SHOULDER_MID];
        for i in [SHOULDER, SHOULDER + 1] {
            j[i] = yaw(j[i], c, pl.twist);
        }
    }
    // Legs: feet where a stance puts them (wider for a wide base), each with its offset.
    let lat = d.hip_half * 0.9 + pl.wide;
    let hv = base.j[ANKLE].z.min(base.j[ANKLE + 1].z).max(0.03);
    let heel_l = 0.30 * d.foot;
    let toe_l = d.foot - heel_l;
    let pitch_toe = -0.55 * pl.point;
    for sd in 0..2 {
        let sg = if sd == 0 { 1.0 } else { -1.0 };
        let hip = v3(pel.x, sg * hip_half, pel.z);
        let off = pl.foot[sd];
        let target = v3(0.02 + off.x, sg * lat + off.y, hv + off.z);
        let (knee, ankle) = ik2(hip, target, d.thigh, d.shin, v3(1.0, sg * 0.15, 0.0));
        j[HIP + sd] = hip;
        j[KNEE + sd] = knee;
        j[ANKLE + sd] = ankle;
        let (sp, cp) = pitch_toe.sin_cos();
        j[HEEL + sd] = v3(ankle.x - cp * heel_l, ankle.y, ankle.z - sp * heel_l);
        j[TOE + sd] = v3(ankle.x + cp * toe_l, ankle.y, ankle.z + sp * toe_l);
        j[SHORTS_END + sd] = hip + (knee - hip) * 0.62;
    }
    // Arms from the shoulders to where the keyframes put the wrists.
    for sd in 0..2 {
        let sg = if sd == 0 { 1.0 } else { -1.0 };
        let shd = j[SHOULDER + sd];
        let (elbow, wrist, fing) = match pl.wrist[sd] {
            Some(t) => {
                let pole = v3(-0.2, sg * 0.7, -0.6);
                let (e, w) = ik2(shd, t, d.upper_arm, d.forearm, pole);
                let fwd = (w - e).norm();
                let flick = pl.flick[sd];
                let fd = (fwd * (1.0 - flick) + v3(0.5, 0.0, -0.9).norm() * flick).norm();
                (e, w, w + fd * d.hand)
            }
            None => {
                // Hanging at his side, a little forward.
                let t = shd + v3(0.05, sg * 0.04, -(d.upper_arm + d.forearm) * 0.97);
                let (e, w) = ik2(shd, t, d.upper_arm, d.forearm, v3(-0.3, sg * 0.5, -0.2));
                (e, w, w + (w - e).norm() * d.hand)
            }
        };
        j[ELBOW + sd] = elbow;
        j[WRIST + sd] = wrist;
        j[FINGERS + sd] = fing;
    }
    pose.ball = pl.ball;
    // Off the floor, or flat on it.
    if pl.lie > 0.0 {
        let a = -std::f32::consts::FRAC_PI_2 * pl.lie;
        for q in pose.j.iter_mut() {
            *q = pitch(*q, v3(0.0, 0.0, 0.0), a);
            q.z += 0.12 * pl.lie;
        }
        if let Some(b) = pose.ball.as_mut() {
            *b = pitch(*b, v3(0.0, 0.0, 0.0), a);
        }
    }
    if a.lift > 0.0 {
        for q in pose.j.iter_mut() {
            q.z += a.lift;
        }
        if let Some(b) = pose.ball.as_mut() {
            b.z += a.lift;
        }
    }
}
