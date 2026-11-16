//! A shot is a launch.
//!
//! The shooter works out the velocity that would put the middle of the ball
//! through the middle of the ring for the arc he has chosen (`ideal`), and
//! what he actually does is that velocity with an error in direction, speed
//! and elevation (`perturb`). The size of the error is where skill lives
//! (`Errors`). Whether it goes in is then read off the ball (`ball::fly`).

use super::ball::{self, Ball, V3, v3};
use crate::court::{BALL_RADIUS, BOARD_HALF, BOARD_HIGH, BOARD_LOW, BOARD_Y, GRAVITY, HOOP, RIM_HEIGHT};
use crate::replay::Style;
use crate::rng::Rng;

/// The glass gives back this much of the speed into it; a shooter who banks
/// it knows.
const BOARD_E: f32 = 0.66;
const BOARD_SLIP: f32 = 0.05;

/// The velocity that drops a ball released at `from` through the middle of
/// the ring, and whether it comes off the glass first.
pub(super) fn ideal(from: V3, style: Style) -> (V3, bool) {
    let hx = HOOP.x - from.x;
    let hy = HOOP.y - from.y;
    let d = (hx * hx + hy * hy).sqrt();
    let drop = RIM_HEIGHT - from.z;
    let direct = |t: f32| -> V3 {
        v3(hx / t, hy / t, drop / t + 0.5 * GRAVITY * t)
    };
    match style {
        Style::Dunk => (direct(0.12 + 0.1 * d), false),
        Style::Layup => {
            // From the side he lays it off the glass; from in front he lays
            // it over the front of the ring.
            if from.x.abs() > 0.55 {
                for t1 in [0.30, 0.38, 0.46, 0.55, 0.65, 0.8] {
                    if let Some(v) = bank(from, t1) {
                        return (v, true);
                    }
                }
            }
            if d < 1.3 {
                return (direct(0.38 + 0.05 * d), false);
            }
            // Too far out and too wide for either: a high floater.
            let a = 1.9;
            let vh = (0.5 * GRAVITY * d * d / (a * d - drop).max(0.1)).sqrt();
            (direct(d / vh), false)
        }
        _ => {
            // A launch slope that falls with distance: high for a free
            // throw, flatter from far out.
            let mut a = (1.4 - 0.045 * d).clamp(1.0, 1.4);
            let room = a * d - drop;
            if room < 0.3 {
                a = (drop + 0.3) / d.max(0.3);
            }
            let vh2 = 0.5 * GRAVITY * d * d / (a * d - drop).max(0.1);
            let vh = vh2.sqrt();
            let t = d / vh;
            (direct(t), false)
        }
    }
}

/// Off the glass: a launch that meets the board and comes off it into the
/// middle of the ring. `t1` is the time to the glass.
fn bank(from: V3, t1: f32) -> Option<V3> {
    let yb = BOARD_Y + BALL_RADIUS;
    let dy = from.y - yb;
    if dy < 0.1 {
        return None;
    }
    let keep = 1.0 - BOARD_SLIP;
    let t2 = (HOOP.y - yb) * t1 / (BOARD_E * dy);
    if !(0.1..1.0).contains(&t2) {
        return None;
    }
    let xb = (HOOP.x * t1 + from.x * keep * t2) / (t1 + keep * t2);
    let zb = (keep * (from.z / t1 + 0.5 * GRAVITY * t1)
        + RIM_HEIGHT / t2
        + 0.5 * GRAVITY * t2)
        / (keep / t1 + 1.0 / t2);
    if zb < BOARD_LOW + 0.1 || zb > BOARD_HIGH - 0.1 || (xb - HOOP.x).abs() > BOARD_HALF - 0.1 {
        return None;
    }
    // It has to come down into the ring steeply enough to fit through it.
    let vzb2 = (RIM_HEIGHT - zb) / t2 + 0.5 * GRAVITY * t2;
    let vh2 = ((HOOP.x - xb).powi(2) + (HOOP.y - yb).powi(2)).sqrt() / t2;
    let in_z = vzb2 - GRAVITY * t2;
    if -in_z < 1.0 * vh2 {
        return None;
    }
    Some(v3(
        (xb - from.x) / t1,
        (yb - from.y) / t1,
        (zb - from.z) / t1 + 0.5 * GRAVITY * t1,
    ))
}

/// Error at the ring, in metres (side, depth), of an average open shot in
/// play before any skill or contest: constant-ish with distance, with the
/// rim finish and the dunk having their own.
pub(super) fn base(style: Style, rim: bool, d: f32) -> (f32, f32) {
    match style {
        Style::Dunk => (0.02, 0.03),
        Style::Layup => (0.065, 0.13),
        _ if rim => (0.065, 0.13),
        _ => (0.05 + 0.004 * d, 0.10 + 0.003 * d),
    }
}

/// How big the shooter's mistakes are, as standard deviations: of the
/// direction over the floor (radians), of the speed (a fraction of it) and
/// of the elevation (radians).
#[derive(Copy, Clone, Debug)]
pub(super) struct Errors {
    pub dir: f32,
    pub speed: f32,
    pub elev: f32,
}

impl Errors {
    /// The errors that put the ball `lat` metres to the side and `depth`
    /// metres long or short (one standard deviation) at the ring, for a shot
    /// from `d` metres.
    pub fn at_rim(d: f32, lat: f32, depth: f32) -> Errors {
        let d = d.max(0.5);
        // However bad his shot, a man does not throw it twice as hard.
        let speed = (depth / (2.0 * d)).min(0.25);
        Errors {
            dir: (lat / d).min(0.3),
            speed,
            elev: speed * 0.5,
        }
    }

}

/// An approximately normal draw, sum of six uniforms.
pub(super) fn gauss(rng: &mut Rng) -> f32 {
    let mut s = 0.0;
    for _ in 0..6 {
        s += rng.f32();
    }
    (s - 3.0) * 1.414_213_5
}

/// The launch he really makes.
pub(super) fn perturb(ideal: V3, e: Errors, rng: &mut Rng) -> V3 {
    let (a, s, l) = (gauss(rng) * e.dir, gauss(rng) * e.speed, gauss(rng) * e.elev);
    let vh = (ideal.x * ideal.x + ideal.y * ideal.y).sqrt();
    if vh < 1e-4 {
        return ideal * (1.0 + s);
    }
    // Turned over the floor by `a`, tilted up by `l`, scaled by `s`.
    let (ux, uy) = (ideal.x / vh, ideal.y / vh);
    let (px, py) = (-uy, ux);
    let (dx, dy) = (ux + px * a, uy + py * a);
    let n = (dx * dx + dy * dy).sqrt();
    let (dx, dy) = (dx / n, dy / n);
    let vz = ideal.z + l * vh;
    v3(dx * vh * (1.0 + s), dy * vh * (1.0 + s), vz * (1.0 + s))
}

/// Make rate of a launch from `from` with the given errors, over `n` tries
/// (for calibration and tests, with no defender).
pub fn make_rate(from: (f32, f32, f32), style: Style, e: (f32, f32), n: usize, seed: u64) -> f32 {
    let mut rng = Rng::new(seed);
    let f = v3(from.0, from.1, from.2);
    let (v, _) = ideal(f, style);
    let d = ((HOOP.x - f.x).powi(2) + (HOOP.y - f.y).powi(2)).sqrt();
    let e = Errors::at_rim(d, e.0, e.1);
    let mut made = 0;
    for _ in 0..n {
        let vel = perturb(v, e, &mut rng);
        if ball::fly(Ball { p: f, v: vel }, &[]).made {
            made += 1;
        }
    }
    made as f32 / n as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::court::HOOP;

    /// The same shooter, the same error at the ring, only the distance
    /// changes: closer is easier, and a smaller error is better.
    #[test]
    fn closer_is_easier_and_a_steadier_hand_is_better() {
        let from = |d: f32| (0.0, HOOP.y + d, 3.0);
        let rate = |d: f32, k: f32| {
            let e = Errors::at_rim(d, (0.05 + 0.004 * d) * k, (0.10 + 0.003 * d) * k);
            let mut rng = Rng::new(3);
            let f = from(d);
            let (v, _) = ideal(v3(f.0, f.1, f.2), Style::Jumper);
            let mut made = 0;
            for _ in 0..3000 {
                let vel = perturb(v, e, &mut rng);
                if ball::fly(Ball { p: v3(f.0, f.1, f.2), v: vel }, &[]).made {
                    made += 1;
                }
            }
            made as f32 / 3000.0
        };
        let by_distance: Vec<f32> = [2.5, 4.0, 5.5, 7.0, 9.0].iter().map(|&d| rate(d, 1.0)).collect();
        for w in by_distance.windows(2) {
            assert!(w[0] > w[1], "{by_distance:?}");
        }
        for d in [3.0, 5.0, 7.0] {
            let (steady, shaky) = (rate(d, 0.7), rate(d, 1.4));
            assert!(steady > shaky + 0.1, "{d}: {steady} {shaky}");
        }
    }

    #[test]
    fn the_ideal_launch_goes_in_from_everywhere() {
        let mut tried = 0;
        for k in 0..60 {
            let x = -6.5 + k as f32 * 0.22;
            for (y, style, z0) in [
                (4.2, Style::Jumper, 2.5),
                (6.0, Style::Jumper, 2.9),
                (8.5, Style::Jumper, 3.0),
                (10.5, Style::Jumper, 3.0),
                (2.3, Style::Hook, 3.0),
                (2.0, Style::Layup, 3.2),
                (2.4, Style::Layup, 3.3),
                (1.9, Style::Dunk, 3.5),
            ] {
                let from = v3(x, y, z0);
                let d = ((HOOP.x - x).powi(2) + (HOOP.y - y).powi(2)).sqrt();
                if d > 9.0 && matches!(style, Style::Layup | Style::Dunk | Style::Hook) {
                    continue;
                }
                if d > 8.0 && (style == Style::Layup || style == Style::Dunk) {
                    continue;
                }
                let far = match style {
                    Style::Dunk => 0.55,
                    Style::Layup => 2.2,
                    Style::Jumper => 99.0,
                    _ => 3.0,
                };
                if d > far {
                    continue;
                }
                if d < 1.0 && style == Style::Jumper {
                    continue;
                }
                let (vel, _) = ideal(from, style);
                let f = ball::fly(Ball { p: from, v: vel }, &[]);
                tried += 1;
                assert!(f.made, "ideal {style:?} from ({x:.2},{y:.2},{z0}) missed ({:?})", f.touch);
            }
        }
        assert!(tried > 100);
    }
}
#[cfg(any())]
mod dbg {
    use super::*;
    #[test]
    fn dbg_bank() {
        let from = v3(-2.1, 2.0, 3.2);
        let (vel, bank) = ideal(from, Style::Layup);
        println!("vel {vel:?} bank {bank}");
        let f = ball::fly(Ball { p: from, v: vel }, &[]);
        let mut t = 0.0;
        while t < 1.2 {
            let (p, z) = f.path.at(t);
            println!("{t:.2} {:.3} {:.3} {:.3}", p.x, p.y, z);
            t += 0.05;
        }
        println!("{:?} iron {} glass {}", f.touch, f.iron, f.glass);
    }
}
