//! Hands: two per player, each a point in 3D that moves toward a goal at a
//! limited speed and never farther from its shoulder than the arm is long.
//!
//! This is the pure model. The game's side (`floor/hands_hooks.rs`) says what
//! each hand wants (the ball, a lane, a face) and what happens when a hand
//! and the ball are in the same place. Only `sqrt` and arithmetic are used.

use super::ball::{V3, v3};
use crate::court::Vec2;
use crate::replay::{HandKind, HandRec};

/// Index 0 is the right hand, 1 the left.
pub const RIGHT: usize = 0;
pub const LEFT: usize = 1;

/// Radius of the ball, and of the part of a hand that stops it, together.
pub const CONTACT: f32 = 0.25;

#[derive(Copy, Clone, Debug)]
pub struct HandPos {
    pub p: V3,
    pub kind: HandKind,
}

#[derive(Copy, Clone, Debug)]
pub struct Hands {
    pub h: [HandPos; 2],
}

impl Hands {
    pub const NONE: Hands = Hands {
        h: [HandPos {
            p: v3(0.0, 0.0, 1.0),
            kind: HandKind::Rest,
        }; 2],
    };
}

/// Side of the body a hand is on: 1 for the right, -1 for the left.
pub fn sign(i: usize) -> f32 {
    if i == RIGHT { 1.0 } else { -1.0 }
}

/// Unit vector to the right of a facing direction.
pub fn right_of(body: Vec2) -> Vec2 {
    Vec2 { x: body.y, y: -body.x }
}

/// Where shoulder `i` is: out to the side of the centre of the body.
pub fn shoulder(pos: Vec2, body: Vec2, i: usize, half_width: f32, z: f32) -> V3 {
    let r = right_of(body) * (half_width * sign(i));
    v3(pos.x + r.x, pos.y + r.y, z)
}

/// A hand at rest: hanging at the side, a little forward.
pub fn rest_goal(sh: V3, body: Vec2, i: usize, arm: f32) -> V3 {
    let r = right_of(body) * (0.10 * sign(i));
    v3(sh.x + r.x + body.x * 0.12, sh.y + r.y + body.y * 0.12, sh.z - 0.72 * arm)
}

/// Ready hands: up in front of the chest, elbows out.
pub fn guard_goal(sh: V3, body: Vec2, i: usize, arm: f32) -> V3 {
    let r = right_of(body) * (0.12 * sign(i));
    v3(sh.x + r.x + body.x * 0.45 * arm, sh.y + r.y + body.y * 0.45 * arm, sh.z - 0.15 * arm)
}

/// Move a hand toward `goal` at `speed` for `dt` seconds, within `arm` of the
/// shoulder. Returns the distance it moved.
pub fn step(hand: &mut HandPos, goal: V3, sh: V3, arm: f32, speed: f32, dt: f32) -> f32 {
    // A goal beyond the arm is as far as the arm goes toward it.
    let mut g = goal;
    let to = V3 { x: g.x - sh.x, y: g.y - sh.y, z: g.z - sh.z };
    let l = to.len();
    if l > arm {
        let k = arm / l;
        g = v3(sh.x + to.x * k, sh.y + to.y * k, sh.z + to.z * k);
    }
    let d = V3 { x: g.x - hand.p.x, y: g.y - hand.p.y, z: g.z - hand.p.z };
    let len = d.len();
    let reach = speed * dt;
    let before = hand.p;
    hand.p = if len <= reach || len < 1e-6 {
        g
    } else {
        let k = reach / len;
        v3(hand.p.x + d.x * k, hand.p.y + d.y * k, hand.p.z + d.z * k)
    };
    // The shoulder moves as well; the hand is never beyond the arm from it.
    let off = V3 { x: hand.p.x - sh.x, y: hand.p.y - sh.y, z: hand.p.z - sh.z };
    let ol = off.len();
    if ol > arm {
        let k = arm / ol;
        hand.p = v3(sh.x + off.x * k, sh.y + off.y * k, sh.z + off.z * k);
    }
    let m = V3 { x: hand.p.x - before.x, y: hand.p.y - before.y, z: hand.p.z - before.z };
    m.len()
}

/// How fast a hand goes toward something it wants, in metres a second, from
/// the man's quickness and what he is doing (a hand that is already moving
/// with the body is quicker than one that has to start).
pub fn hand_speed(quick: f32, hands: f32) -> f32 {
    4.5 + 0.18 * quick + 0.14 * hands
}

/// A hand as it goes into the recording: its offset from the body's centre
/// in court coordinates (both given in court coordinates) and its height.
pub fn record(hand: &HandPos, hand_court: Vec2, centre_court: Vec2, kind: HandKind) -> HandRec {
    HandRec {
        dx: ((hand_court.x - centre_court.x) * 100.0).round().clamp(-400.0, 400.0) as i16,
        dy: ((hand_court.y - centre_court.y) * 100.0).round().clamp(-400.0, 400.0) as i16,
        z: (hand.p.z * 100.0).round().clamp(0.0, 400.0) as u16,
        kind,
    }
}
