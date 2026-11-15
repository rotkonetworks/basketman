//! Court geometry, FIBA dimensions, in metres.
//!
//! The match engine works in the *attack frame* of the team with the ball:
//! x runs across the court (-7.5..=7.5, 0 on the basket line), y runs away
//! from the baseline being attacked (0..=28). The hoop is at `HOOP`.
//!
//! Only `sqrt` is used here: it is exact on every target, unlike the
//! trigonometric functions, so positions stay bit-identical in wasm.

use std::ops::{Add, Mul, Sub};

use serde::{Deserialize, Serialize};

pub const LENGTH: f32 = 28.0;
pub const HALF_WIDTH: f32 = 7.5;
pub const HOOP: Vec2 = Vec2 { x: 0.0, y: 1.575 };
pub const THREE_RADIUS: f32 = 6.75;
/// The three-point line runs straight, 0.9 m inside the sideline, until it
/// meets the arc.
pub const CORNER_X: f32 = 6.6;
pub const CORNER_Y: f32 = 2.99;
pub const KEY_HALF_WIDTH: f32 = 2.45;
pub const KEY_DEPTH: f32 = 5.8;
pub const FT_RADIUS: f32 = 1.8;
pub const NO_CHARGE_RADIUS: f32 = 1.25;
pub const CENTRE_RADIUS: f32 = 1.8;
/// Height of the rim above the floor, and the radius of the ring's inside.
pub const RIM_HEIGHT: f32 = 3.05;
pub const RIM_RADIUS: f32 = 0.225;
/// Radius of the ball, and the front face of the backboard in the attack
/// frame (y), where it spans 0.9 m either side of the basket line and from
/// 2.9 m to 3.95 m up.
pub const BALL_RADIUS: f32 = 0.12;
pub const BOARD_Y: f32 = 1.2;
pub const BOARD_HALF: f32 = 0.9;
pub const BOARD_LOW: f32 = 2.9;
pub const BOARD_HIGH: f32 = 3.95;
/// Gravity, m/s^2.
pub const GRAVITY: f32 = 9.81;
/// Shots from inside this distance are layups, dunks and hooks.
pub const RIM_RANGE: f32 = 1.9;

#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

pub const fn v(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}

impl Vec2 {
    pub const ZERO: Vec2 = v(0.0, 0.0);

    pub fn len(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn dist(self, o: Vec2) -> f32 {
        (self - o).len()
    }

    pub fn dot(self, o: Vec2) -> f32 {
        self.x * o.x + self.y * o.y
    }

    /// Unit vector, or zero for a zero vector.
    pub fn norm(self) -> Vec2 {
        let l = self.len();
        if l > 1e-5 {
            self * (1.0 / l)
        } else {
            Vec2::ZERO
        }
    }

    pub fn lerp(self, o: Vec2, t: f32) -> Vec2 {
        self + (o - self) * t
    }

    /// The point `d` metres from `self` in the direction of `to`.
    pub fn toward(self, to: Vec2, d: f32) -> Vec2 {
        self + (to - self).norm() * d
    }

    /// The same point seen by the team attacking the other basket.
    pub fn flipped(self) -> Vec2 {
        v(-self.x, LENGTH - self.y)
    }

    /// Closest point to `self` on the segment `a`..`b`.
    pub fn project(self, a: Vec2, b: Vec2) -> Vec2 {
        let ab = b - a;
        let l2 = ab.dot(ab);
        if l2 < 1e-6 {
            return a;
        }
        a + ab * ((self - a).dot(ab) / l2).clamp(0.0, 1.0)
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        v(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        v(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f32) -> Vec2 {
        v(self.x * k, self.y * k)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShotKind {
    Rim,
    Mid,
    Three,
}

impl ShotKind {
    pub fn at(p: Vec2) -> ShotKind {
        let d = p.dist(HOOP);
        if d < RIM_RANGE {
            ShotKind::Rim
        } else if d >= THREE_RADIUS || (p.x.abs() >= CORNER_X && p.y <= CORNER_Y) {
            ShotKind::Three
        } else {
            ShotKind::Mid
        }
    }

    pub fn points(self) -> u16 {
        if self == ShotKind::Three { 3 } else { 2 }
    }
}
