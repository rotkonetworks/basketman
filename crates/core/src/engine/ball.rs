//! The ball as a body in three dimensions, at the basket.
//!
//! A ball in flight has a position and a velocity. Gravity pulls it; the
//! iron ring (a torus), the backboard (a slab with edges) and the floor
//! send it back, each with its own give. Nothing here knows what a shot is:
//! `fly` lets a ball go and writes down what it did, as a chain of arcs
//! (between two contacts the path is exactly a parabola) and a few facts read
//! off that path: it went down through the ring, it touched the iron, a
//! defender's hand met it on its way up.
//!
//! The step is fine near the basket and sweeps: a contact is found by
//! bisecting the step in which the ball first overlaps something, so a fast
//! ball cannot pass through iron or glass. Only arithmetic and `sqrt` are
//! used, so the path is the same on every target.

use super::flight::{Arc, Path};
use crate::court::{
    BALL_RADIUS, BOARD_HALF, BOARD_HIGH, BOARD_LOW, BOARD_Y, GRAVITY, HOOP, RIM_HEIGHT, Vec2, v,
};

/// Distance from the middle of the ring to the middle of the iron.
pub const RING_RADIUS: f32 = 0.2286;
/// Radius of the iron tube.
pub const IRON: f32 = 0.0095;
/// Thickness of the glass.
const BOARD_THICK: f32 = 0.03;
/// How much of the speed along the normal the ball keeps off each surface.
const E_IRON: f32 = 0.50;
const E_BOARD: f32 = 0.66;
const E_FLOOR: f32 = 0.74;
/// How much of the speed along a surface is lost in a contact with it.
const SLIP_IRON: f32 = 0.10;
const SLIP_BOARD: f32 = 0.05;
const SLIP_FLOOR: f32 = 0.12;
/// What the net leaves of the ball's speed as it comes through.
const NET_XY: f32 = 0.30;
const NET_Z: f32 = 0.55;
/// Longest a flight is followed, seconds.
const MAX_T: f32 = 9.0;
/// The most iron-and-glass contacts a flight has before it is let fall.
const MAX_CONTACTS: u32 = 18;

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl V3 {
    pub fn xy(self) -> Vec2 {
        v(self.x, self.y)
    }
    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
    pub fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn norm(self) -> V3 {
        let l = self.len();
        if l > 1e-6 { self * (1.0 / l) } else { V3::default() }
    }
}

impl std::ops::Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl std::ops::Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl std::ops::Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, k: f32) -> V3 {
        v3(self.x * k, self.y * k, self.z * k)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Ball {
    pub p: V3,
    pub v: V3,
}

impl Ball {
    /// Where it is after `h` seconds of free flight.
    fn after(&self, h: f32) -> Ball {
        Ball {
            p: v3(
                self.p.x + self.v.x * h,
                self.p.y + self.v.y * h,
                self.p.z + self.v.z * h - 0.5 * GRAVITY * h * h,
            ),
            v: v3(self.v.x, self.v.y, self.v.z - GRAVITY * h),
        }
    }
}

/// An arm that can swat the ball: the shoulder, how long the arm is with the
/// hand, and the jump he is in (so the shoulder rises with the body).
#[derive(Copy, Clone, Debug)]
pub struct Hand {
    pub who: usize,
    /// Where his feet are over the floor at the release, and how fast he
    /// is moving.
    pub pos: Vec2,
    pub vel: Vec2,
    /// Height of the shoulder standing, and length of arm with hand.
    pub shoulder: f32,
    pub arm: f32,
    /// The jump: seconds from the release until the feet leave the floor
    /// (may be negative: already up), how long he is in the air, and the
    /// height of the top.
    pub up_at: f32,
    pub air: f32,
    pub peak: f32,
}

impl Hand {
    pub const NONE: Hand = Hand {
        who: 0,
        pos: Vec2::ZERO,
        vel: Vec2::ZERO,
        shoulder: 0.0,
        arm: 0.0,
        up_at: 0.0,
        air: 0.0,
        peak: 0.0,
    };

    /// Height of the body above standing, `t` seconds after the release.
    pub fn lift(&self, t: f32) -> f32 {
        let u = t - self.up_at;
        if self.air <= 0.0 || u <= 0.0 || u >= self.air {
            return 0.0;
        }
        let w = u / self.air;
        self.peak * 4.0 * w * (1.0 - w)
    }

    /// The shoulder at `t`.
    pub fn at(&self, t: f32) -> V3 {
        let p = self.pos + self.vel * t.min(0.35);
        v3(p.x, p.y, self.shoulder + self.lift(t))
    }
}

/// What a flight did.
#[derive(Copy, Clone, Debug)]
pub struct Flown {
    pub path: Path,
    /// The ball went down through the ring.
    pub made: bool,
    /// Seconds in when its middle was level with the ring on the way down,
    /// inside it.
    pub ring_at: f32,
    /// First contact with iron or glass (seconds, over the floor, height).
    pub touch: Option<(f32, Vec2, f32)>,
    /// Iron contacts, and glass contacts.
    pub iron: u8,
    pub glass: u8,
    /// A hand met it: who and when.
    pub blocked: Option<(usize, f32)>,
}

/// Closest point of the glass slab to `p`.
fn board_point(p: V3) -> V3 {
    let lo = v3(HOOP.x - BOARD_HALF, BOARD_Y - BOARD_THICK, BOARD_LOW);
    let hi = v3(HOOP.x + BOARD_HALF, BOARD_Y, BOARD_HIGH);
    v3(p.x.clamp(lo.x, hi.x), p.y.clamp(lo.y, hi.y), p.z.clamp(lo.z, hi.z))
}

#[derive(Copy, Clone, PartialEq)]
enum Surface {
    Iron,
    Glass,
    Floor,
}

/// What the ball at `p` is overlapping, if anything: the surface and the
/// unit normal pointing from it to the ball.
fn overlap(p: V3, scoring: bool) -> Option<(Surface, V3)> {
    if p.z < BALL_RADIUS {
        return Some((Surface::Floor, v3(0.0, 0.0, 1.0)));
    }
    if scoring {
        // Above the floor level only the iron and the glass can be hit, and
        // only near the basket.
        let dx = p.x - HOOP.x;
        let dy = p.y - HOOP.y;
        if dx * dx + dy * dy > 1.6 && p.y > BOARD_Y + 1.0 {
            return None;
        }
        let q = board_point(p);
        let d = p - q;
        let l2 = d.dot(d);
        if l2 < BALL_RADIUS * BALL_RADIUS {
            let l = l2.sqrt();
            let n = if l > 1e-6 {
                d * (1.0 / l)
            } else {
                // The middle of the ball is inside the slab: out the front.
                v3(0.0, 1.0, 0.0)
            };
            return Some((Surface::Glass, n));
        }
        let d2 = dx * dx + dy * dy;
        let dh = d2.sqrt();
        let (ux, uy) = if dh > 1e-5 { (dx / dh, dy / dh) } else { (1.0, 0.0) };
        let q = v3(HOOP.x + ux * RING_RADIUS, HOOP.y + uy * RING_RADIUS, RIM_HEIGHT);
        let d = p - q;
        let reach = BALL_RADIUS + IRON;
        let l2 = d.dot(d);
        if l2 < reach * reach {
            let l = l2.sqrt();
            let n = if l > 1e-6 { d * (1.0 / l) } else { v3(0.0, 0.0, 1.0) };
            return Some((Surface::Iron, n));
        }
    }
    None
}

/// A ball cannot start inside iron or glass: a hand that would put it there
/// (a man under the rim) lifts it over the ring first.
pub fn clear_start(mut p: V3) -> V3 {
    for _ in 0..40 {
        if overlap(p, true).is_none() {
            break;
        }
        p.z += 0.03;
    }
    p
}

/// Let the ball go from `ball` and follow it. `hands` are the arms that may
/// reach it in its first moments, on the way up and before it has touched
/// anything.
pub fn fly(ball: Ball, hands: &[Hand]) -> Flown {
    let mut b = ball;
    let mut t = 0.0f32;
    let mut seg = (b, 0.0f32);
    let mut path = Path::new();
    let mut flown = Flown {
        path: Path::new(),
        made: false,
        ring_at: 0.0,
        touch: None,
        iron: 0,
        glass: 0,
        blocked: None,
    };
    let mut contacts = 0u32;
    let mut entered: Option<f32> = None;
    let mut scoring = true;
    let mut rolled = false;
    while t < MAX_T {
        let near_hoop = {
            let dx = b.p.x - HOOP.x;
            let dy = b.p.y - HOOP.y;
            let dz = b.p.z - RIM_HEIGHT;
            dx * dx + dy * dy + dz * dz < 1.5 * 1.5
        };
        let fast = b.v.len();
        let h = if near_hoop {
            0.003
        } else if fast > 9.0 {
            0.008
        } else {
            0.012
        };
        // A defender's hand meets a ball that is still going up.
        if flown.blocked.is_none() && flown.touch.is_none() && b.v.z > 0.0 && t < 0.6 {
            for hand in hands {
                let s = hand.at(t);
                let d = b.p - s;
                if d.len() < hand.arm + 0.04 {
                    // The ball goes where the hand sends it: off the arm,
                    // and down and away from the basket, hard.
                    let n = d.norm();
                    let vn = b.v.dot(n);
                    let mut out = b.v - n * ((1.0 + 0.45) * vn.min(0.0));
                    let away = (b.p.xy() - HOOP).norm();
                    out = out + v3(away.x * 2.5, away.y * 2.5, -2.0);
                    out.z = out.z.min(1.5);
                    flown.blocked = Some((hand.who, t));
                    flown.touch.get_or_insert((t, b.p.xy(), b.p.z));
                    push(&mut path, &seg, t);
                    b.v = out;
                    seg = (b, t);
                    scoring = true;
                    entered = None;
                    break;
                }
            }
        }
        if b.p.z >= RIM_HEIGHT {
            entered = None;
        }
        let mut next = b.after(h);
        if let Some((_, _)) = overlap(next.p, scoring) {
            // Find where the step first touched: the last point clear.
            let (mut lo, mut hi) = (0.0f32, h);
            for _ in 0..12 {
                let mid = 0.5 * (lo + hi);
                if overlap(b.after(mid).p, scoring).is_some() {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            let clear = b.after(lo);
            let pen = b.after(hi);
            let (surface, n) = overlap(pen.p, scoring).unwrap_or((Surface::Floor, v3(0.0, 0.0, 1.0)));
            t += hi;
            let mut nb = pen;
            let vn = nb.v.dot(n);
            if vn < 0.0 {
                let (e, slip) = match surface {
                    Surface::Iron => (E_IRON, SLIP_IRON),
                    Surface::Glass => (E_BOARD, SLIP_BOARD),
                    Surface::Floor => (E_FLOOR, SLIP_FLOOR),
                };
                let normal = n * vn;
                let along = nb.v - normal;
                nb.v = along * (1.0 - slip) - normal * e;
                // Out of the surface.
                nb.p = clear.p;
                if surface != Surface::Floor {
                    nb.p = nb.p + n * 0.0005;
                }
            } else {
                nb.p = clear.p;
            }
            match surface {
                Surface::Floor => {
                    nb.p.z = nb.p.z.max(BALL_RADIUS);
                    push(&mut path, &seg, t);
                    seg = (nb, t);
                    b = nb;
                    // Out of bounds it is dead where it lands.
                    let out = b.p.x.abs() > 7.5 || b.p.y < 0.0 || b.p.y > crate::court::LENGTH;
                    if out {
                        path_push_roll(&mut path, b.p, b.p);
                        t += 0.9;
                        rolled = true;
                        break;
                    }
                    if b.v.z < 1.4 {
                        // It stops bouncing and rolls out.
                        let roll = v3(b.v.x * 0.7, b.v.y * 0.7, 0.0);
                        let p1 = b.p + roll * 0.9;
                        path_push_roll(&mut path, b.p, p1);
                        t += 0.9;
                        rolled = true;
                        break;
                    }
                    continue;
                }
                s => {
                    contacts += 1;
                    if s == Surface::Iron {
                        flown.iron += 1;
                    } else {
                        flown.glass += 1;
                    }
                    flown.touch.get_or_insert((t, nb.p.xy(), nb.p.z));
                    push(&mut path, &seg, t);
                    seg = (nb, t);
                    b = nb;
                    if contacts >= MAX_CONTACTS {
                        scoring = false;
                    }
                    continue;
                }
            }
        }
        // Down through the ring?
        let above = b.p.z >= RIM_HEIGHT;
        let d_after = {
            let dx = next.p.x - HOOP.x;
            let dy = next.p.y - HOOP.y;
            (dx * dx + dy * dy).sqrt()
        };
        if above && next.p.z < RIM_HEIGHT && b.v.z < 0.0 && d_after < RING_RADIUS - IRON && entered.is_none() {
            entered = Some(t + h);
        }
        t += h;
        if let Some(at) = entered
            && !flown.made
            && next.p.z < RIM_HEIGHT - 0.2
        {
            let dx = next.p.x - HOOP.x;
            let dy = next.p.y - HOOP.y;
            if (dx * dx + dy * dy).sqrt() < RING_RADIUS {
                flown.made = true;
                flown.ring_at = at;
                // The net takes the speed off it.
                push(&mut path, &seg, t);
                next.v = v3(next.v.x * NET_XY, next.v.y * NET_XY, next.v.z * NET_Z);
                seg = (next, t);
                scoring = false;
                b = next;
                continue;
            }
        }
        b = next;
    }
    if !rolled {
        // Followed long enough, or never came down: end where it is.
        push(&mut path, &seg, t);
    }
    // A ball that stops out of bounds is brought back to the line.
    let (end, _) = path.end();
    let spot = v(end.x.clamp(-7.2, 7.2), end.y.clamp(0.4, crate::court::LENGTH - 0.4));
    if spot.dist(end) > 0.01 {
        path.push(Arc {
            p0: end,
            p1: spot,
            z0: BALL_RADIUS,
            vz: 0.0,
            dur: (spot.dist(end) / 2.0).max(1.0),
            g: 0.0,
        });
    }
    flown.path = path;
    flown
}

/// Close the arc that ran from `seg` to time `t`.
fn push(path: &mut Path, seg: &(Ball, f32), t: f32) {
    let dur = (t - seg.1).max(1e-4);
    path.push(Arc {
        p0: seg.0.p.xy(),
        p1: seg.0.p.xy() + seg.0.v.xy() * dur,
        z0: seg.0.p.z,
        vz: seg.0.v.z,
        dur,
        g: GRAVITY,
    });
}

fn path_push_roll(path: &mut Path, from: V3, to: V3) {
    path.push(Arc {
        p0: from.xy(),
        p1: to.xy(),
        z0: BALL_RADIUS,
        vz: 0.0,
        dur: 0.9,
        g: 0.0,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drop_on(x: f32, y: f32, z: f32) -> Flown {
        fly(
            Ball {
                p: v3(HOOP.x + x, HOOP.y + y, z),
                v: v3(0.0, 0.0, -1.0),
            },
            &[],
        )
    }

    #[test]
    fn dropped_on_the_middle_it_goes_through() {
        let f = drop_on(0.0, 0.0, RIM_HEIGHT + 0.6);
        assert!(f.made && f.iron == 0);
    }

    #[test]
    fn dropped_on_the_iron_it_does_not() {
        let f = drop_on(RING_RADIUS, 0.0, RIM_HEIGHT + 0.6);
        assert!(!f.made && f.iron >= 1);
        // And bounces up and out.
        let (t, _, _) = f.touch.unwrap();
        let (_, z0) = f.path.at(t);
        let (_, z1) = f.path.at(t + 0.1);
        assert!(z1 > z0 - 0.01);
    }

    #[test]
    fn a_ball_at_the_front_iron_comes_back_toward_the_shooter() {
        // From the free throw line, flat and short.
        let from = v3(0.0, 5.8, 2.6);
        let to = v3(HOOP.x, HOOP.y + RING_RADIUS + 0.12, RIM_HEIGHT);
        let t = 0.8;
        let vv = (to - from) * (1.0 / t);
        let f = fly(
            Ball {
                p: from,
                v: v3(vv.x, vv.y, vv.z + 0.5 * GRAVITY * t),
            },
            &[],
        );
        assert!(!f.made);
        let (tt, _, _) = f.touch.expect("touches");
        let (p_a, _) = f.path.at(tt - 0.02);
        let (p_b, _) = f.path.at(tt + 0.1);
        assert!(p_b.y > p_a.y, "{p_a:?} {p_b:?}");
    }

    #[test]
    fn the_glass_sends_it_back() {
        let f = fly(
            Ball {
                p: v3(0.0, 3.0, 2.8),
                v: v3(0.0, -5.0, 3.0),
            },
            &[],
        );
        let (t, at, z) = f.touch.expect("hit the glass");
        assert!((at.y - (BOARD_Y + BALL_RADIUS)).abs() < 0.02 && z > BOARD_LOW);
        let (a, _) = f.path.at(t - 0.02);
        let (b, _) = f.path.at(t + 0.05);
        assert!(b.y > a.y - 0.05 && f.glass == 1);
    }

    #[test]
    fn nothing_goes_through_iron_or_glass_however_fast() {
        for speed in [6.0f32, 12.0, 20.0, 30.0] {
            for k in 0..40 {
                let x = -0.5 + k as f32 * 0.025;
                // Straight at the glass.
                let f = fly(
                    Ball {
                        p: v3(x, 3.0, 3.3),
                        v: v3(0.0, -speed, 0.5),
                    },
                    &[],
                );
                let mut t = 0.0;
                while t < f.path.total().min(3.0) {
                    let (p, z) = f.path.at(t);
                    let behind = p.y < BOARD_Y - 0.05 && z > BOARD_LOW && z < BOARD_HIGH;
                    assert!(!(behind && p.x.abs() < BOARD_HALF), "through glass at {speed}");
                    t += 0.002;
                }
            }
            // And down at the ring from above.
            for k in 0..60 {
                let x = -0.3 + k as f32 * 0.01;
                let f = fly(
                    Ball {
                        p: v3(HOOP.x + x, HOOP.y, RIM_HEIGHT + 1.0),
                        v: v3(0.0, 0.0, -speed),
                    },
                    &[],
                );
                let inside = x.abs() < RING_RADIUS - IRON - BALL_RADIUS - 0.01;
                if inside {
                    assert!(f.made, "x {x} speed {speed}");
                }
                if x.abs() > RING_RADIUS - IRON - BALL_RADIUS + 0.04 && x.abs() < RING_RADIUS + BALL_RADIUS {
                    assert!(!f.made || f.iron > 0, "through the iron at x {x} speed {speed}");
                }
            }
        }
    }

    #[test]
    fn nothing_gets_behind_the_glass_from_anywhere_near_the_basket() {
        let mut rng = crate::rng::Rng::new(5);
        let mut bad = 0;
        for _ in 0..20000 {
            let p = v3(
                -1.0 + rng.f32() * 2.0,
                BOARD_Y + 0.13 + rng.f32() * 1.5,
                2.6 + rng.f32() * 1.6,
            );
            let v = v3(
                (rng.f32() - 0.5) * 8.0,
                (rng.f32() - 0.7) * 8.0,
                (rng.f32() - 0.3) * 8.0,
            );
            if overlap(p, true).is_some() {
                continue;
            }
            let f = fly(Ball { p, v }, &[]);
            let mut t = 0.0;
            let (mut last, mut lz) = f.path.at(0.0);
            while t < f.path.total().min(4.0) {
                t += 0.005;
                let (q, z) = f.path.at(t);
                if last.y >= BOARD_Y && q.y < BOARD_Y {
                    let u = (last.y - BOARD_Y) / (last.y - q.y);
                    let x = last.x + (q.x - last.x) * u;
                    let h = lz + (z - lz) * u;
                    if x.abs() < BOARD_HALF - 0.02 && h > BOARD_LOW + 0.02 && h < BOARD_HIGH - 0.02 {
                        bad += 1;
                        println!("through glass: start {p:?} v {v:?} at t {t:.3} {q:?} {z}");
                        break;
                    }
                }
                last = q;
                lz = z;
            }
        }
        assert_eq!(bad, 0);
    }

    #[test]
    fn a_hand_on_the_way_up_changes_where_it_goes() {
        let ball = Ball {
            p: v3(0.0, 5.0, 2.8),
            v: v3(0.0, -3.0, 5.0),
        };
        let free = fly(ball, &[]);
        let hand = Hand {
            who: 3,
            pos: v2(0.0, 4.4),
            vel: Vec2::ZERO,
            shoulder: 1.75,
            arm: 1.05,
            up_at: -0.1,
            air: 0.6,
            peak: 0.4,
        };
        let f = fly(ball, &[hand]);
        assert_eq!(f.blocked.map(|b| b.0), Some(3));
        assert!(!f.made);
        let _ = free;
    }

    fn v2(x: f32, y: f32) -> Vec2 {
        v(x, y)
    }
}
