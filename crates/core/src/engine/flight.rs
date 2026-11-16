//! Where the ball goes after it leaves a hand: chains of arcs under gravity.
//!
//! Each `Arc` moves in a straight line over the floor while its height
//! follows a parabola, so a path is a few numbers and can be read at any
//! time. Only arithmetic and `sqrt` are used, so paths are identical on every
//! target. Paths of shots, blocks and tips are written by the ball physics
//! (`ball.rs`); passes use `Arc::reach`.

use crate::court::{BALL_RADIUS, GRAVITY, LENGTH, Vec2, v};

const G: f32 = GRAVITY;
/// How much of its speed the ball keeps off the floor.
const BOUNCE: f32 = 0.55;
/// Longest a path can be, in arcs.
const MAX_ARCS: usize = 64;

#[derive(Copy, Clone, Debug)]
pub(super) struct Arc {
    pub p0: Vec2,
    pub p1: Vec2,
    pub z0: f32,
    pub vz: f32,
    pub dur: f32,
    /// 9.81 in the air, 0 for a ball rolling on the floor.
    pub g: f32,
}

impl Arc {
    /// An arc from `p0` at height `z0` to `p1` at height `z1` in `dur` seconds.
    pub fn reach(p0: Vec2, z0: f32, p1: Vec2, z1: f32, dur: f32) -> Arc {
        let dur = dur.max(0.05);
        Arc {
            p0,
            p1,
            z0,
            vz: (z1 - z0) / dur + 0.5 * G * dur,
            dur,
            g: G,
        }
    }

    pub fn at(&self, t: f32) -> (Vec2, f32) {
        let t = t.clamp(0.0, self.dur);
        (
            self.p0.lerp(self.p1, t / self.dur),
            self.z0 + self.vz * t - 0.5 * self.g * t * t,
        )
    }

}

#[derive(Copy, Clone, Debug)]
pub(super) struct Path {
    arcs: [Arc; MAX_ARCS],
    n: usize,
}

impl Path {
    pub fn new() -> Path {
        let none = Arc {
            p0: Vec2::ZERO,
            p1: Vec2::ZERO,
            z0: 0.0,
            vz: 0.0,
            dur: 1.0,
            g: 0.0,
        };
        Path {
            arcs: [none; MAX_ARCS],
            n: 0,
        }
    }

    pub fn push(&mut self, a: Arc) {
        if self.n < MAX_ARCS {
            self.arcs[self.n] = a;
            self.n += 1;
        }
    }

    pub fn total(&self) -> f32 {
        self.arcs[..self.n].iter().map(|a| a.dur).sum()
    }

    /// Ball position over the floor and height at `t` seconds in.
    pub fn at(&self, mut t: f32) -> (Vec2, f32) {
        for a in &self.arcs[..self.n] {
            if t <= a.dur {
                return a.at(t);
            }
            t -= a.dur;
        }
        self.end()
    }

    pub fn arcs(&self) -> &[Arc] {
        &self.arcs[..self.n]
    }

    /// Position, height and velocity (over the floor, and vertical) at `t`.
    pub fn state(&self, mut t: f32) -> (Vec2, f32, Vec2, f32) {
        for a in &self.arcs[..self.n] {
            if t <= a.dur {
                let t = t.max(0.0);
                let (p, z) = a.at(t);
                return (p, z, (a.p1 - a.p0) * (1.0 / a.dur), a.vz - a.g * t);
            }
            t -= a.dur;
        }
        let (p, z) = self.end();
        (p, z, Vec2::ZERO, 0.0)
    }

    /// First time at or after `t0` that the ball is coming down through
    /// height `h` (or is already below it and coming down), with where it
    /// is then. `None` if it never comes down that far.
    pub fn comes_down_to(&self, t0: f32, h: f32) -> Option<(f32, Vec2)> {
        let total = self.total();
        let mut t = t0;
        while t <= total {
            let (p, z, _, vz) = self.state(t);
            if z <= h && vz < 0.0 {
                return Some((t, p));
            }
            t += 0.02;
        }
        None
    }

    pub fn end(&self) -> (Vec2, f32) {
        match self.arcs[..self.n].last() {
            Some(a) => a.at(a.dur),
            None => (Vec2::ZERO, BALL_RADIUS),
        }
    }

    /// The same path seen from the other end of the court.
    pub fn flipped(&self) -> Path {
        let mut out = *self;
        for a in &mut out.arcs[..out.n] {
            a.p0 = a.p0.flipped();
            a.p1 = a.p1.flipped();
        }
        out
    }

    /// Let the ball fall from `p` at height `z` and vertical speed `vz`,
    /// drifting at `vel` over the floor, bounce and roll to a stop.
    pub fn fall(&mut self, mut p: Vec2, mut z: f32, mut vz: f32, mut vel: Vec2) {
        for _ in 0..3 {
            let drop = (vz * vz + 2.0 * G * (z - BALL_RADIUS)).max(0.0).sqrt();
            let t = ((vz + drop) / G).max(0.02);
            let p1 = keep_on_court(p + vel * t);
            self.push(Arc {
                p0: p,
                p1,
                z0: z,
                vz,
                dur: t,
                g: G,
            });
            let hit = drop;
            p = p1;
            z = BALL_RADIUS;
            vz = hit * BOUNCE;
            vel = vel * 0.7;
            if vz < 1.4 {
                break;
            }
        }
        // Rolling out.
        let p1 = keep_on_court(p + vel * 0.9);
        self.push(Arc {
            p0: p,
            p1,
            z0: BALL_RADIUS,
            vz: 0.0,
            dur: 0.9,
            g: 0.0,
        });
    }

    /// Time at which the ball first touches the floor, if it does.
    pub fn first_floor(&self) -> Option<f32> {
        let mut t = 0.0;
        for a in &self.arcs[..self.n] {
            let (_, z_end) = a.at(a.dur);
            if a.g > 0.0 && z_end <= BALL_RADIUS + 0.001 && a.z0 > BALL_RADIUS + 0.05 {
                return Some(t + a.dur);
            }
            t += a.dur;
        }
        None
    }
}

/// A point inside the lines, as far as a ball can roll.
fn keep_on_court(p: Vec2) -> Vec2 {
    v(p.x.clamp(-7.3, 7.3), p.y.clamp(0.3, LENGTH - 0.3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fall_stops_on_the_floor() {
        let mut p = Path::new();
        p.fall(v(1.0, 1.0), 3.0, -3.0, v(0.2, 0.0));
        let (_, z) = p.end();
        assert!((z - BALL_RADIUS).abs() < 1e-3);
        assert!(p.first_floor().is_some());
    }
}
