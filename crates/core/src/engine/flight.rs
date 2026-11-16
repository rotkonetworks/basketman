//! Where the ball goes after it leaves a hand: chains of arcs under gravity.
//!
//! Each `Arc` moves in a straight line over the floor while its height
//! follows a parabola, so a path is a few numbers and can be read at any
//! time. Only arithmetic and `sqrt` are used, so paths are identical on every
//! target. The shot builders here make the recorded flight agree with the
//! result: a made shot comes down through the ring and the net, a miss
//! strikes the rim or the glass and comes off, a block is met by a hand.

use crate::court::{BALL_RADIUS, BOARD_Y, GRAVITY, HOOP, LENGTH, RIM_HEIGHT, RIM_RADIUS, Vec2, v};

const G: f32 = GRAVITY;
/// How much of its speed the ball keeps off the floor.
const BOUNCE: f32 = 0.55;
/// Where the ball centre is when it touches the iron: on the ring.
pub(super) const RIM_TOUCH: f32 = RIM_RADIUS + 0.03;
/// Longest a path can be, in arcs.
const MAX_ARCS: usize = 40;

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

    /// Vertical speed when it ends.
    pub fn end_vz(&self) -> f32 {
        self.vz - self.g * self.dur
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

/// Unit vector over the floor from the basket to `p`, or straight out into
/// the court when `p` is on top of it.
fn out_of(p: Vec2) -> Vec2 {
    let d = (p - HOOP).norm();
    if d == Vec2::ZERO { v(0.0, 1.0) } else { d }
}

/// A shot that goes in. From the hand at `from`, height `z0`, it reaches
/// the middle of the ring (give or take `off`) in `dur` seconds, falling,
/// then drops through the net and to the floor. `bank` is the point of the
/// glass it comes off first, if any. Returns the path and the time it is in
/// the ring.
pub(super) fn made(from: Vec2, z0: f32, dur: f32, off: Vec2, bank: Option<f32>) -> (Path, f32) {
    let mut path = Path::new();
    let aim = HOOP + off;
    let ring_at = dur;
    match bank {
        Some(x) => {
            // Off the glass: to a spot on the board, then softly down.
            let spot = v(HOOP.x + x, BOARD_Y + BALL_RADIUS + 0.01);
            let t1 = dur * 0.6;
            let t2 = dur - t1;
            path.push(Arc::reach(from, z0, spot, RIM_HEIGHT + 0.35, t1));
            path.push(Arc::reach(spot, RIM_HEIGHT + 0.35, aim, RIM_HEIGHT, t2));
        }
        None => {
            path.push(Arc::reach(from, z0, aim, RIM_HEIGHT, dur));
        }
    }
    let vz = path.arcs[path.n - 1].end_vz();
    // Through the net: it is slowed a little by the cord, which is why it
    // takes a moment to drop out of the bottom.
    let net_end = (RIM_HEIGHT - 0.45).max(0.0);
    let t_net = {
        let c = RIM_HEIGHT - net_end;
        ((vz + (vz * vz + 2.0 * G * c).sqrt()) / G).max(0.05) * 1.5
    };
    let below = aim + v(0.0, -0.05);
    path.push(Arc {
        p0: aim,
        p1: below,
        z0: RIM_HEIGHT,
        vz: (net_end - RIM_HEIGHT) / t_net + 0.5 * G * t_net,
        dur: t_net,
        g: G,
    });
    let vz_out = path.arcs[path.n - 1].end_vz();
    path.fall(below, net_end, vz_out, out_of(from) * -0.3);
    (path, ring_at)
}

/// A shot that is not going in: it strikes the iron (or the glass) in
/// `dur` seconds. Returns the path to that point, where it is and how high.
pub(super) fn missed(from: Vec2, z0: f32, dur: f32, glass: bool) -> (Path, Vec2, f32) {
    let mut path = Path::new();
    let back = out_of(from);
    let (touch, touch_z) = if glass {
        // The board, a hand's width off centre towards the shooter's side.
        let x = HOOP.x + back.x.clamp(-1.0, 1.0) * 0.3;
        (v(x, BOARD_Y + BALL_RADIUS), RIM_HEIGHT + 0.4)
    } else {
        // Front of the ring, on the shooter's side of it.
        (HOOP + back * RIM_TOUCH, RIM_HEIGHT + 0.02)
    };
    path.push(Arc::reach(from, z0, touch, touch_z, dur));
    (path, touch, touch_z)
}

/// The ball comes off the iron or off a hand at `from`, height `z`, and is
/// next touched at `dest`, height `catch_z`, `dur` seconds later.
pub(super) fn off_the_iron(from: Vec2, z: f32, dest: Vec2, catch_z: f32, dur: f32) -> Path {
    let mut path = Path::new();
    path.push(Arc::reach(from, z, dest, catch_z, dur));
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_made_shot_is_in_the_ring_going_down() {
        for (from, z0, dur) in [
            (v(0.0, 7.5), 2.9, 1.2),
            (v(5.0, 4.0), 2.8, 0.9),
            (v(0.4, 2.4), 3.1, 0.4),
        ] {
            let (path, t) = made(from, z0, dur, v(0.03, 0.0), None);
            let (p, z) = path.at(t);
            assert!((z - RIM_HEIGHT).abs() < 0.01, "{z}");
            assert!(p.dist(HOOP) < RIM_RADIUS);
            let (_, z_after) = path.at(t + 0.05);
            assert!(z_after < z, "going down");
            let (_, z_end) = path.end();
            assert!(z_end <= BALL_RADIUS + 0.01);
        }
    }

    #[test]
    fn a_miss_touches_the_iron_outside_the_ring() {
        let (path, touch, tz) = missed(v(1.0, 5.0), 2.8, 0.9, false);
        let (p, z) = path.end();
        assert!(p.dist(HOOP) > RIM_RADIUS && p.dist(touch) < 1e-3);
        assert!(z > RIM_HEIGHT && (z - tz).abs() < 1e-3);
        let off = off_the_iron(touch, tz, v(0.5, 3.5), 2.8, 0.8);
        let (end, ez) = off.end();
        assert!(end.dist(v(0.5, 3.5)) < 0.01 && (ez - 2.8).abs() < 0.01);
    }

    #[test]
    fn fall_stops_on_the_floor() {
        let mut p = Path::new();
        p.fall(v(1.0, 1.0), 3.0, -3.0, v(0.2, 0.0));
        let (_, z) = p.end();
        assert!((z - BALL_RADIUS).abs() < 1e-3);
        assert!(p.first_floor().is_some());
    }
}
