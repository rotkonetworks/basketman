//! Bodies with a stance: three directions per player and the legwork between them.
//!
//! * the **body** (hips and chest) turns at a limited rate that depends on
//!   agility and footwork, on speed (a sprinting body cannot spin) and on
//!   balance;
//! * the **eyes** turn fast but stay within about 80 degrees of the body;
//! * the **movement** is the velocity he really has.
//!
//! The legwork follows from the angle between movement and body: a run when
//! they agree, a slide when he moves sideways to his body, a backpedal when
//! he moves away from the way he faces. Each has its own top speed, so a
//! defender who has to turn his hips to keep up with a drive loses time.
//!
//! Everything here is plain arithmetic on unit vectors (no `sin`, `cos`,
//! `atan2` from the standard library): the results are the same on every
//! machine. The module is pure: the game passes numbers in and takes
//! numbers out.

use super::DT;
use crate::court::{Vec2, v};
use crate::replay::{Dribble, Footwork};

/// Cosine and sine of 80 degrees: the eyes stay inside this cone round the body.
pub const HEAD_COS: f32 = 0.173_648;
pub const HEAD_SIN: f32 = 0.984_808;
/// Cosine of 45 degrees: shoulders further off the rim than this are off line.
pub const SQUARE_COS: f32 = 0.707_107;

const PI: f32 = 3.141_592_7;
const HALF_PI: f32 = 1.570_796_4;
/// How fast the eyes turn, radians per second.
const HEAD_RATE: f32 = 16.0;

/// What a player's body can do, from his ratings (1 to 20, as `eff` gives them).
#[derive(Copy, Clone)]
pub struct Skill {
    /// Quickness of the feet: half speed, half footwork.
    pub agility: f32,
    pub footwork: f32,
    pub strength: f32,
}

impl Skill {
    pub fn new(speed: f32, footwork: f32, strength: f32) -> Skill {
        Skill {
            agility: 0.5 * (speed + footwork),
            footwork,
            strength,
        }
    }
}

/// One player's state, in the attack frame of the side with the ball.
#[derive(Copy, Clone)]
pub struct Stance {
    /// Unit vector: where hips and chest point.
    pub body: Vec2,
    /// Unit vector: where the eyes look.
    pub gaze: Vec2,
    /// 1 when set and square, falling to 0 when thrown off his feet.
    pub balance: f32,
    pub foot: Footwork,
    /// The planted foot of a pivot: 0 none, 1 left, 2 right.
    pub planted: u8,
    /// Radians per second the body turned last tick.
    pub turning: f32,
    /// Velocity of the last tick, for the acceleration.
    pub last_vel: Vec2,
    /// Seconds since the last cut or turn of more than a few degrees.
    pub steady: f32,
    /// The speed he came in at: his fastest of the last second or so, falling
    /// by `PEAK_FALL` m/s each second.
    pub peak: f32,
}

impl Stance {
    pub fn new(facing: Vec2) -> Stance {
        Stance {
            body: facing,
            gaze: facing,
            balance: 1.0,
            foot: Footwork::Stand,
            planted: 0,
            turning: 0.0,
            last_vel: Vec2::ZERO,
            steady: 1.0,
            peak: 0.0,
        }
    }

    /// The same body seen by the team attacking the other basket.
    pub fn flip(&mut self) {
        self.body = self.body * -1.0;
        self.gaze = self.gaze * -1.0;
        self.last_vel = self.last_vel * -1.0;
    }
}

/// What a player wants to do with his body this tick.
#[derive(Copy, Clone)]
pub struct Want {
    /// A point his body should face (`None`: the way he moves).
    pub body: Option<Vec2>,
    /// A point his eyes should be on (`None`: where his body faces).
    pub gaze: Option<Vec2>,
    /// Defending: keep square to `body` and slide instead of turning to run.
    pub square: bool,
    /// Sprinting at a shooter or handler.
    pub closing: bool,
    /// Turning on a planted foot with the ball.
    pub pivoting: bool,
    /// Planting for a shot.
    pub planting: bool,
    /// A post player turning round his man.
    pub dropping: bool,
    /// Holds the ball (a ball handler uses crossovers, not slides).
    pub handler: bool,
}

impl Want {
    pub const NONE: Want = Want {
        body: None,
        gaze: None,
        square: false,
        closing: false,
        pivoting: false,
        planting: false,
        dropping: false,
        handler: false,
    };
}

// ---- angles without trigonometry ----------------------------------------

/// Arctangent of `x` in 0..=1 (error about 1e-5 rad), arithmetic only.
fn atan_unit(x: f32) -> f32 {
    let x2 = x * x;
    x * (0.999_866 + x2 * (-0.330_299_5 + x2 * (0.180_141 + x2 * (-0.085_133 + x2 * 0.020_835_1))))
}

/// The angle between two unit vectors, 0 to pi.
pub fn angle(a: Vec2, b: Vec2) -> f32 {
    let x = a.dot(b);
    let y = (a.x * b.y - a.y * b.x).abs();
    if x.abs() < 1e-6 && y < 1e-6 {
        return 0.0;
    }
    let base = if y <= x.abs() {
        atan_unit(y / x.abs())
    } else {
        HALF_PI - atan_unit(x.abs() / y)
    };
    if x >= 0.0 { base } else { PI - base }
}

/// Rotate `a` by `th` radians (|th| under about 1.3) with polynomials.
fn rotate(a: Vec2, th: f32) -> Vec2 {
    let t2 = th * th;
    let c = 1.0 - t2 * 0.5 + t2 * t2 * (1.0 / 24.0) - t2 * t2 * t2 * (1.0 / 720.0);
    let s = th * (1.0 - t2 * (1.0 / 6.0) + t2 * t2 * (1.0 / 120.0) - t2 * t2 * t2 * (1.0 / 5040.0));
    v(a.x * c - a.y * s, a.x * s + a.y * c).norm()
}

/// Turn unit vector `cur` toward unit vector `want` by at most `max` radians.
/// A turn of exactly half a circle goes the same way every time.
pub fn turn(cur: Vec2, want: Vec2, max: f32) -> Vec2 {
    let max = max.min(1.2);
    let cross = cur.x * want.y - cur.y * want.x;
    let dot = cur.dot(want);
    // Cosine of the step by the same polynomial as `rotate`.
    let m2 = max * max;
    let cmax = 1.0 - m2 * 0.5 + m2 * m2 * (1.0 / 24.0) - m2 * m2 * m2 * (1.0 / 720.0);
    if dot >= cmax {
        return want;
    }
    rotate(cur, if cross >= 0.0 { max } else { -max })
}

/// Keep `gaze` within 80 degrees of `body`.
pub fn clamp_gaze(body: Vec2, gaze: Vec2) -> Vec2 {
    if gaze.dot(body) >= HEAD_COS {
        return gaze;
    }
    let cross = body.x * gaze.y - body.y * gaze.x;
    let s = if cross >= 0.0 { HEAD_SIN } else { -HEAD_SIN };
    v(
        body.x * HEAD_COS - body.y * s,
        body.x * s + body.y * HEAD_COS,
    )
}

// ---- the model -----------------------------------------------------------

/// How fast the body turns, radians per second: quick feet turn faster, a
/// sprinting body hardly at all, and a man off balance slowly.
pub fn turn_rate(sk: &Skill, speed_frac: f32, balance: f32) -> f32 {
    let base = 6.5 + 0.40 * sk.agility;
    base * (1.0 - 0.72 * speed_frac.clamp(0.0, 1.0)) * (0.55 + 0.45 * balance.clamp(0.0, 1.0))
}

/// Seconds to turn through `ang` radians standing, for planning a shot.
pub fn turn_time(sk: &Skill, ang: f32, speed_frac: f32, balance: f32) -> f32 {
    ang / turn_rate(sk, speed_frac, balance).max(1.0)
}

/// The share of top speed a man can use going in a direction at angle `c`
/// (the cosine) to his body: all of it going forward, `side` sliding, `back`
/// backpedalling. Footwork and agility raise the sideways and backward limits.
pub fn move_cap(sk: &Skill, c: f32) -> f32 {
    let side = (0.60 + 0.016 * (sk.footwork - 10.0) + 0.005 * (sk.agility - 10.0)).clamp(0.42, 0.82);
    let back = side - 0.14;
    if c >= 0.62 {
        1.0
    } else if c >= 0.0 {
        side + (1.0 - side) * (c / 0.62)
    } else {
        back + (side - back) * (1.0 + c)
    }
}

/// How well he can change his velocity in a direction relative to his body.
pub fn accel_cap(c: f32) -> f32 {
    0.7 + 0.3 * move_frac(c)
}

fn move_frac(c: f32) -> f32 {
    ((c + 1.0) * 0.5).clamp(0.0, 1.0)
}

/// What the legs are doing: from his speed, the cosine between his movement
/// and his body, and what he is trying to do.
pub fn footwork(speed: f32, top: f32, c: f32, w: &Want, turning: f32) -> Footwork {
    if w.planting && speed < 1.5 {
        return Footwork::JumpStop;
    }
    if speed < 0.45 {
        return if w.dropping {
            Footwork::DropStep
        } else if w.pivoting && turning > 0.8 {
            Footwork::Pivot
        } else {
            Footwork::Stand
        };
    }
    if w.closing {
        return if speed > 0.6 * top { Footwork::Closeout } else { Footwork::Chop };
    }
    if c >= 0.62 {
        Footwork::Run
    } else if c <= -0.5 {
        Footwork::Backpedal
    } else if w.handler {
        Footwork::Crossover
    } else {
        Footwork::Slide
    }
}

/// One tick of balance. `acc` is the change of velocity in m/s per second
/// (halved when he brakes on purpose), `turning` the rate the body turned,
/// `contact` how hard he was hit (0 to 1), `set` how long he has stood.
pub fn balance(
    prev: f32,
    sk: &Skill,
    acc: f32,
    turning: f32,
    speed: f32,
    contact: f32,
    off_axis: f32,
) -> f32 {
    let loss = (acc - 7.0).max(0.0) * 0.06
        + (turning * speed - 10.0).max(0.0) * 0.02
        + contact * 3.0
        + off_axis * speed * 0.03;
    let rec = 0.9 + 0.035 * sk.footwork + 0.02 * sk.strength;
    let calm = if loss < 0.05 { 1.0 } else { 0.0 };
    (prev - loss * DT + rec * calm * DT).clamp(0.0, 1.0)
}

// ---- shots, passes and contests ------------------------------------------

/// The extra error of a shot from the angle between his shoulders and the
/// line to the rim (`ang` radians, 0 square), the way he moves (`fwd`, `side`
/// in m/s along and across the line to the rim, backward negative) and his
/// balance. `skill` (0 to 1) is how much the matching skill and signature
/// move take off. A multiplier of the launch error, 1 for a set shot.
pub fn shot_multiplier(ang: f32, fwd: f32, side: f32, balance: f32, skill: f32, scale: f32) -> f32 {
    let turned = (ang - 0.2).max(0.0);
    let back = (-fwd).max(0.0);
    let off = 1.0 - balance.clamp(0.0, 1.0);
    let raw = 1.4 * turned * turned + 0.14 * side.abs() + 0.08 * back + 1.2 * off * off;
    1.0 + raw * scale * (1.0 - 0.6 * skill.clamp(0.0, 1.0))
}

/// Seconds a pass takes to be released, and how much its aim is off, for a
/// ball thrown at a receiver at cosine `c` to where his body faces: in front
/// is quick and true, behind is slow and loose. `skill` is passing and vision
/// together, 0 to 1.
pub fn pass_cost(c: f32, skill: f32) -> (f32, f32) {
    let behind = ((0.7 - c) / 1.7).clamp(0.0, 1.0);
    let relief = 1.0 - 0.6 * skill.clamp(0.0, 1.0);
    (0.30 * behind * relief, 0.6 * behind * relief)
}

// ---- the ball handler's dribble ------------------------------------------

/// The ball handler's dribble: who has the ball, where he is in it.
#[derive(Copy, Clone)]
pub struct Handler {
    pub who: Option<(usize, usize)>,
    pub state: Dribble,
    /// 1 right, -1 left.
    pub hand: i8,
    /// Where he caught it, then where he picked up.
    pub pivot: Vec2,
    /// How far from `pivot` he may move once he has picked up.
    pub radius: f32,
    /// Seconds since the dribbling hand last changed.
    pub switched: f32,
}

impl Handler {
    pub const NONE: Handler = Handler {
        who: None,
        state: Dribble::None,
        hand: 1,
        pivot: Vec2::ZERO,
        radius: 0.0,
        switched: 1.0,
    };

    pub fn flip(&mut self) {
        self.pivot = self.pivot.flipped();
    }
}

/// How fast the memory of the speed he came in at fades, m/s per second.
pub const PEAK_FALL: f32 = 3.0;

/// Metres a ball handler may travel from where he caught it before it
/// counts that he is dribbling (the two steps of a catch on the move).
pub const CATCH_STEPS: f32 = 1.7;
