//! How a player is built and how he moves: a port of the model in
//! `viewer3d/match.psh` (see `viewer3d/README.md`, "Figures" and "Motion").
//!
//! Three layers, all free of drawing:
//!
//! * `Dims`: every length of the body, derived from his measurements.
//! * `pose`: the joints of one player for one `Gait` (velocity, facing,
//!   stance, gait phase, dribble phase), in his own frame: x forward, y to
//!   his left, z up, origin on the floor under the pelvis.
//! * `Motion`: the gait of every player at every recorded frame, computed
//!   once per recording and extended as a live recording grows, so that any
//!   playback time can be drawn without remembering the last one.

use std::f32::consts::{PI, TAU};
use std::sync::{Arc, Mutex};

use basketman_core::court::Vec2;
use basketman_core::player::{Attr, Body, Hand, Player};
use basketman_core::replay::Recording;

use super::{BALL_RADIUS, Vec3, v3};

pub(crate) fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The ratings that shape a figure's movement, 1..=20.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Stats {
    pub speed: f32,
    pub vertical: f32,
    pub strength: f32,
    pub stamina: f32,
}

impl Stats {
    pub fn of(p: &Player) -> Stats {
        Stats {
            speed: p.attrs[Attr::Speed],
            vertical: p.attrs[Attr::Vertical],
            strength: p.attrs[Attr::Strength],
            stamina: p.attrs[Attr::Stamina],
        }
    }

    pub const AVERAGE: Stats = Stats {
        speed: 10.0,
        vertical: 10.0,
        strength: 10.0,
        stamina: 10.0,
    };
}

/// Every length of a figure, in metres, from the player's own numbers.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Dims {
    pub height: f32,
    pub shoulder_width: f32,
    /// Build factor: scales torso depth and width and the legs' thickness.
    pub build: f32,
    pub hip_z: f32,
    pub thigh: f32,
    pub shin: f32,
    #[allow(dead_code)]
    pub ankle: f32,
    /// Hip to shoulder joints.
    pub torso: f32,
    /// Shoulders to the base of the head.
    pub neck: f32,
    pub head: f32,
    pub upper_arm: f32,
    pub forearm: f32,
    pub hand: f32,
    /// Half the distance between the shoulder joints.
    pub shoulder_half: f32,
    pub hip_half: f32,
    /// Hip to ankle joint.
    pub leg: f32,
    pub foot: f32,
    /// +1 left-handed, -1 right-handed: which side of him the ball is on.
    pub hand_side: f32,
    pub vertical: f32,
    pub stamina: f32,
    /// Speed rating stretches the stride.
    pub stride_factor: f32,
}

impl Dims {
    pub fn new(b: &Body, st: &Stats) -> Dims {
        let h = (b.height_cm / 100.0).clamp(1.2, 2.5);
        let sw = (b.shoulder_width_cm / 100.0).clamp(0.3, 0.7);
        let reach = b.standing_reach_cm / 100.0;
        let hand = (b.hand_length_cm / 100.0).clamp(0.12, 0.3);
        // Shoulder joints sit inside the shoulder width; what is left of half
        // the wingspan is the whole arm including the hand.
        let shl = 0.46 * sw;
        let arm = (b.wingspan_cm / 200.0 - shl).max(hand + 0.2);
        // Longer reach for the height means higher shoulders, which also
        // shortens the legs a little.
        let shf = (0.815 + (reach / h - 1.33)).clamp(0.78, 0.85);
        let shoulder_z = h * shf;
        let hip_z = h * (0.525 - 0.5 * (shf - 0.82));
        let ankle = 0.036 * h;
        let leg = hip_z - ankle;
        let bmi = b.weight_kg / (h * h);
        let build = ((0.9 + 0.075 * (bmi - 22.0)) * (0.93 + 0.012 * st.strength)).clamp(0.8, 1.4);
        Dims {
            height: h,
            shoulder_width: sw,
            build,
            hip_z,
            thigh: leg * 0.505,
            shin: leg * 0.495,
            ankle,
            torso: shoulder_z - hip_z,
            neck: (h * (1.0 - 0.13) - shoulder_z).max(0.03 * h),
            head: 0.13 * h,
            upper_arm: (arm - hand) * 0.56,
            forearm: (arm - hand) * 0.44,
            hand,
            shoulder_half: shl,
            hip_half: 0.24 * sw,
            leg,
            foot: 0.145 * h,
            hand_side: if b.hand == Hand::Left { 1.0 } else { -1.0 },
            vertical: st.vertical,
            stamina: st.stamina,
            stride_factor: 1.0 + 0.012 * (st.speed - 10.0),
        }
    }

    /// Distance a full gait cycle (two steps) covers at speed `v`.
    #[cfg(test)]
    pub fn stride(&self, v: f32, stance: f32) -> f32 {
        stride_len(v, stance, self.leg, self.stride_factor)
    }
}

/// Share of a cycle a foot spends on the floor.
fn duty(v: f32, s: f32) -> f32 {
    let ra = smoothstep(1.6, 4.4, v);
    (0.62 - 0.29 * ra) * (1.0 - s) + 0.58 * s
}

/// How far the planted foot travels relative to the hips, in leg lengths.
fn sigma(v: f32, s: f32) -> f32 {
    let ra = smoothstep(1.6, 4.4, v);
    (0.78 + 0.05 * ra + 0.10 * smoothstep(4.4, 7.5, v)) * (1.0 - s) + 0.42 * s
}

/// A planted foot must travel `v * duty * T`, so one cycle covers
/// `sigma * leg / duty`: a tall player takes fewer, longer strides.
pub(crate) fn stride_len(v: f32, s: f32, leg: f32, factor: f32) -> f32 {
    sigma(v, s) * leg / duty(v, s) * factor
}

/// What drives a pose.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Gait {
    /// World velocity, metres per second.
    pub vel: Vec2,
    /// Unit vector he faces along the floor.
    pub face: Vec2,
    /// 0 free, 1 defensive stance.
    pub stance: f32,
    /// Gait phase in radians; advances with distance over stride length.
    pub phase: f32,
    /// Dribble phase in radians.
    pub dribble: f32,
}

impl Gait {
    pub fn standing(face: Vec2) -> Gait {
        Gait {
            vel: Vec2::ZERO,
            face,
            stance: 0.0,
            phase: 0.0,
            dribble: 0.0,
        }
    }
}

// Joint numbers; the pairs are [left, right].
pub(crate) const PELVIS: usize = 0;
pub(crate) const HIP: usize = 1;
pub(crate) const KNEE: usize = 3;
pub(crate) const ANKLE: usize = 5;
pub(crate) const TOE: usize = 7;
pub(crate) const HEEL: usize = 9;
pub(crate) const SHOULDER_MID: usize = 11;
pub(crate) const SHOULDER: usize = 12;
pub(crate) const NECK_BASE: usize = 14;
pub(crate) const NECK_TOP: usize = 15;
pub(crate) const HEAD_TOP: usize = 16;
pub(crate) const ELBOW: usize = 17;
pub(crate) const WRIST: usize = 19;
pub(crate) const FINGERS: usize = 21;
pub(crate) const TORSO_LOW: usize = 23;
pub(crate) const SHORTS_LOW: usize = 24;
pub(crate) const SHORTS_HIGH: usize = 25;
pub(crate) const SHORTS_END: usize = 26;
pub(crate) const HAIR_LOW: usize = 28;
pub(crate) const HAIR_HIGH: usize = 29;
pub(crate) const JERSEY_TOP: usize = 30;
pub(crate) const JOINTS: usize = 31;

/// The joints of one figure, and the ball if he holds it.
#[derive(Clone, Debug)]
pub(crate) struct Pose {
    pub j: [Vec3; JOINTS],
    pub ball: Option<Vec3>,
}

/// Two-bone IK: the middle joint (elbow or knee) of a limb from `s` towards
/// `t` with bones `l1`, `l2`, bent towards `pole`. Returns the middle joint
/// and the end point (pulled in when `t` is out of reach).
pub(crate) fn ik2(s: Vec3, t: Vec3, l1: f32, l2: f32, pole: Vec3) -> (Vec3, Vec3) {
    let mut d = t - s;
    let mut len = d.len();
    if len < 0.01 {
        d = v3(0.0, 0.0, -0.01);
        len = 0.01;
    }
    let u = d * (1.0 / len);
    let max = (l1 + l2) * 0.999;
    let min = (l1 - l2).abs() + 0.02;
    let dd = len.clamp(min, max);
    let mut q = pole - u * pole.dot(u);
    let mut ql = q.len();
    if ql < 0.001 {
        let down = v3(0.0, 0.0, -1.0);
        q = down - u * down.dot(u);
        ql = q.len().max(0.001);
    }
    let a = (dd * dd + l1 * l1 - l2 * l2) / (2.0 * dd);
    let h = (l1 * l1 - a * a).max(0.0).sqrt();
    (s + u * a + q * (h / ql), s + u * dd)
}

/// The pose for `gait`, with the ball in his hands when `ball`.
pub(crate) fn pose(d: &Dims, g: &Gait, ball: bool) -> Pose {
    let mut j = [Vec3::default(); JOINTS];
    let h = d.height;
    let lr = d.leg;
    let v = g.vel.len().min(8.0);
    let s = g.stance.clamp(0.0, 1.0);
    let phi = g.phase;
    // Direction of travel in his frame (forward, left).
    let (mut mf, mut ml) = (1.0, 0.0);
    if v > 0.05 {
        let (dx, dy) = (g.vel.x / v, g.vel.y / v);
        mf = dx * g.face.x + dy * g.face.y;
        ml = -dx * g.face.y + dy * g.face.x;
    }
    let ra = smoothstep(1.6, 4.4, v);
    let gm = smoothstep(0.12, 0.65, v);
    let duty = duty(v, s);
    let sweep = sigma(v, s) * lr * gm * d.stride_factor;
    let vf = 0.7 + 0.03 * d.vertical;
    let lift_h = (lr * (0.07 + 0.15 * ra) * (1.0 - s) + lr * 0.05 * s) * gm * vf;
    // Hip height: low enough that the planted foot at the end of its sweep is
    // still within reach of the leg, plus a small vault, highest over the
    // planted foot.
    let xh = sweep * 0.5 / lr;
    let ends = 1.0 - (0.994 - xh * xh).max(0.5).sqrt();
    let drop = (1.0 - s) * (0.02 + ends) + s * (0.15 + ends);
    let vault = lr * (0.015 + 0.04 * ra) * gm * vf * (1.0 - 0.5 * s);
    let pel_f = -0.08 * lr * s;
    let pz = d.hip_z - lr * drop + vault * (0.5 + 0.5 * (2.0 * (phi - PI * duty)).cos());
    let lat_base = d.hip_half * (1.0 - s) + 0.15 * h * s;
    let hv = 0.034 * h;
    let heel_l = 0.30 * d.foot;
    let toe_l = d.foot - heel_l;

    // Legs: the feet follow a stance and swing path along the direction of
    // travel; the knees come out of IK. A planted foot moves back relative
    // to the hips exactly as fast as the body moves forward.
    let mut s_left = 0.0;
    for sd in 0..2 {
        let sg = if sd == 0 { 1.0 } else { -1.0 };
        let mut u = phi / TAU + if sd == 0 { 0.0 } else { 0.5 };
        u -= u.floor();
        if sd == 0 {
            s_left = (TAU * u - 0.2).cos();
        }
        let mut pp;
        let lift;
        let mut pitch;
        if u < duty {
            let q = u / duty;
            pp = 0.5 - q;
            lift = 0.0;
            pitch = -0.30 * smoothstep(0.7, 1.0, q) + 0.16 * (1.0 - smoothstep(0.0, 0.2, q));
        } else {
            let q = (u - duty) / (1.0 - duty);
            pp = -0.5 + q * q * (3.0 - 2.0 * q);
            lift = (PI * q).sin() * lift_h;
            pitch = -0.5 * (1.0 - smoothstep(0.0, 0.5, q)) + 0.22 * smoothstep(0.6, 1.0, q);
        }
        pitch *= gm;
        pp *= sweep;
        let (sp, cp) = pitch.sin_cos();
        let az = hv * cp
            + if pitch < 0.0 {
                toe_l * -sp
            } else {
                heel_l * sp
            }
            + lift;
        let hip = v3(pel_f, sg * d.hip_half, pz);
        let target = v3(mf * pp, sg * lat_base + ml * pp, az);
        let (knee, ankle) = ik2(hip, target, d.thigh, d.shin, v3(1.0, sg * 0.35 * s, 0.0));
        j[HIP + sd] = hip;
        j[KNEE + sd] = knee;
        j[ANKLE + sd] = ankle;
        j[HEEL + sd] = v3(ankle.x - cp * heel_l, ankle.y, ankle.z - sp * heel_l);
        j[TOE + sd] = v3(ankle.x + cp * toe_l, ankle.y, ankle.z + sp * toe_l);
        // End of the shorts, a bit over half way down the thigh.
        j[SHORTS_END + sd] = hip + (knee - hip) * 0.62;
    }

    // Trunk and head.
    let lean = ((0.03 + 0.24 * smoothstep(1.0, 6.0, v)) * (1.0 - s) + 0.30 * s)
        + 0.004 * (10.0 - d.stamina) * gm;
    let (sl, cl) = lean.sin_cos();
    j[PELVIS] = v3(pel_f, 0.0, pz);
    j[TORSO_LOW] = v3(pel_f, 0.0, pz + 0.02 * h);
    j[SHORTS_LOW] = v3(pel_f, 0.0, pz - 0.075 * h);
    j[SHORTS_HIGH] = v3(pel_f, 0.0, pz + 0.04 * h);
    j[SHOULDER_MID] = v3(pel_f + sl * d.torso, 0.0, pz + cl * d.torso);
    let jt = d.torso + 0.02 * h;
    j[JERSEY_TOP] = v3(pel_f + sl * jt, 0.0, pz + cl * jt);
    j[NECK_BASE] = j[SHOULDER_MID];
    let (sn4, cn4) = (lean * 0.4).sin_cos();
    j[NECK_TOP] = j[SHOULDER_MID] + v3(sn4 * d.neck, 0.0, cn4 * d.neck);
    let (sh1, ch1) = (lean * 0.1).sin_cos();
    j[HEAD_TOP] = j[NECK_TOP] + v3(sh1 * d.head, 0.0, ch1 * d.head);
    j[HAIR_LOW] = v3(
        j[NECK_TOP].x + sh1 * d.head * 0.55 - 0.012 * h,
        0.0,
        j[NECK_TOP].z + ch1 * d.head * 0.55,
    );
    j[HAIR_HIGH] = j[HEAD_TOP] + v3(-0.012 * h, 0.0, 0.012 * h);
    // Shoulders twist against the hips.
    let psi = (0.10 + 0.14 * ra) * gm * s_left * (1.0 - s);
    let (spsi, cpsi) = psi.sin_cos();
    let sm = j[SHOULDER_MID];
    j[SHOULDER] = v3(sm.x - d.shoulder_half * spsi, d.shoulder_half * cpsi, sm.z);
    j[SHOULDER + 1] = v3(sm.x + d.shoulder_half * spsi, -d.shoulder_half * cpsi, sm.z);

    // The ball: held at the chest when he stands, dribbled at the hand on
    // his shooting side in step with his strides when he moves.
    let sb = d.hand_side;
    let br = BALL_RADIUS;
    let mut bc = Vec3::default();
    let mut hb = 0.0;
    let mut sdb = 0;
    if ball {
        sdb = if sb > 0.0 { 0 } else { 1 };
        hb = smoothstep(0.25, 0.9, v);
        let hold = v3(0.17 * h, sb * 0.09, pz + 0.30 * d.torso);
        let dr_f = (0.17 + 0.05 * ra) * h;
        let dr_l = sb * (d.shoulder_half + 0.04);
        let wzt = pz + 0.12;
        let zt = wzt - 0.07 - br;
        let mut ub = g.dribble / TAU;
        ub -= ub.floor();
        let e = 2.0 * ub - 1.0;
        let zb = br + (zt - br) * (1.0 - e * e);
        bc = hold + (v3(dr_f, dr_l, zb) - hold) * hb;
    }

    // Arms.
    let aamp = (0.28 + 0.45 * ra) * gm;
    let ef = 0.25 + 1.05 * ra * gm;
    let first = if ball { sdb } else { 0 };
    for ai in 0..2 {
        let sd = if ai == 0 { first } else { 1 - first };
        let sg = if sd == 0 { 1.0 } else { -1.0 };
        let sh = j[SHOULDER + sd];
        // Running swing, opposite to the legs.
        let beta = 0.05 - sg * aamp * s_left;
        let gam = beta + ef;
        let u1 = v3(beta.sin(), sg * 0.12, -beta.cos()).norm();
        let u2 = v3(gam.sin(), sg * 0.05, -gam.cos()).norm();
        let r_el = sh + u1 * d.upper_arm;
        let r_wr = r_el + u2 * d.forearm;
        // Defensive stance: out to the sides, hands up.
        let a1 = v3(0.15, sg * 0.85, -0.50).norm();
        let a2 = v3(0.60, sg * 0.60, 0.35).norm();
        let d_el = sh + a1 * d.upper_arm;
        let d_wr = d_el + a2 * d.forearm;
        let el = r_el + (d_el - r_el) * s;
        let mut tw = r_wr + (d_wr - r_wr) * s;
        let mut onball = 0.0;
        if ball {
            if sd == sdb {
                // On top of the ball when dribbling, on its side when held.
                let wzt = pz + 0.12;
                let dribz = (bc.z + br + 0.07).max(wzt - 0.24);
                tw = v3(
                    bc.x,
                    bc.y + sb * (br + 0.04) * (1.0 - hb),
                    bc.z + (dribz - bc.z) * hb,
                );
                onball = 1.0;
            } else {
                let w2 = 1.0 - hb;
                let to = v3(bc.x, bc.y - sb * (br + 0.04), bc.z);
                tw = tw + (to - tw) * w2;
                onball = w2;
            }
        }
        // The pole keeps the elbow where the pose put it.
        let mid = (sh + tw) * 0.5;
        let mut pole = el - mid;
        if pole.dot(pole) < 0.0004 {
            pole = v3(0.0, sg * 0.4, -1.0);
        }
        let (elbow, wrist) = ik2(sh, tw, d.upper_arm, d.forearm, pole);
        j[ELBOW + sd] = elbow;
        j[WRIST + sd] = wrist;
        if ball && sd == sdb {
            // Out of reach: bring the ball to the hand.
            bc = bc + (wrist - tw);
        }
        // The hand continues the forearm, or reaches for the ball.
        let mut hd = (wrist - elbow).norm();
        if onball > 0.0 {
            let to = (bc - wrist).norm();
            hd = (hd + (to - hd) * onball).norm();
        }
        j[FINGERS + sd] = wrist + hd * d.hand;
    }
    Pose {
        j,
        ball: ball.then_some(bc),
    }
}

// ---------------------------------------------------------------------------
// Motion over a recording.

/// The gait of one player at one frame.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Slot {
    pub vel: Vec2,
    pub face: Vec2,
    pub stance: f32,
    pub phase: f32,
    pub dribble: f32,
}

impl Slot {
    const NONE: Slot = Slot {
        vel: Vec2::ZERO,
        face: Vec2 { x: 1.0, y: 0.0 },
        stance: 0.0,
        phase: 0.0,
        dribble: 0.0,
    };

    pub fn gait(&self) -> Gait {
        Gait {
            vel: self.vel,
            face: self.face,
            stance: self.stance,
            phase: self.phase,
            dribble: self.dribble,
        }
    }
}

type Row = [Slot; 10];

/// Leg length and stride factor of the player in box score slot `who` on
/// side `side`.
pub(crate) type StrideOf<'a> = &'a dyn Fn(usize, u8) -> (f32, f32);

/// A jump this long in one frame is a substitution or a whistle, not running.
const JUMP: f32 = 2.5;

/// The gait of everybody at every frame of one recording. Frames from
/// `rows.len()` on are provisional: they lack the neighbours that smoothing
/// needs, so they are recomputed from the committed rows whenever the
/// recording grows (the last two frames of a finished game stay so).
pub(crate) struct Motion {
    rows: Vec<Row>,
    tail: Vec<Row>,
    /// Frames seen so far.
    len: usize,
    first: Stamp,
    last: Stamp,
}

/// Enough of a frame to tell that a recording is still the same one.
#[derive(Copy, Clone, PartialEq, Debug)]
struct Stamp {
    time: f32,
    pos: Vec2,
    who: [[u8; 5]; 2],
}

impl Stamp {
    fn of(rec: &Recording, i: usize) -> Stamp {
        let f = &rec.frames[i];
        Stamp {
            time: f.time,
            pos: f.pos[0][0],
            who: f.who,
        }
    }
}

fn pos_of(rec: &Recording, i: usize, k: usize) -> Vec2 {
    rec.frames[i].pos[k / 5][k % 5]
}

/// Frame `i` to `i + 1` is one continuous movement of slot `k`: the same
/// player, no jump, no whistle gap.
fn cont(rec: &Recording, i: usize, k: usize) -> bool {
    let (Some(a), Some(b)) = (rec.frames.get(i), rec.frames.get(i + 1)) else {
        return false;
    };
    let (s, n) = (k / 5, k % 5);
    let (pa, pb) = (a.pos[s][n], b.pos[s][n]);
    a.who[s][n] == b.who[s][n]
        && (pb.x - pa.x).abs() <= JUMP
        && (pb.y - pa.y).abs() <= JUMP
        && !b.cut
        && b.time - a.time < 0.25
}

/// Central difference where both neighbours are continuous, else one-sided.
fn raw_vel(rec: &Recording, i: usize, k: usize) -> Vec2 {
    let t = |i: usize| rec.frames[i].time;
    let a = i > 0 && cont(rec, i - 1, k);
    let b = cont(rec, i, k);
    let (p0, p1, span) = match (a, b) {
        (true, true) => (
            pos_of(rec, i - 1, k),
            pos_of(rec, i + 1, k),
            t(i + 1) - t(i - 1),
        ),
        (false, true) => (pos_of(rec, i, k), pos_of(rec, i + 1, k), t(i + 1) - t(i)),
        (true, false) => (pos_of(rec, i - 1, k), pos_of(rec, i, k), t(i) - t(i - 1)),
        (false, false) => return Vec2::ZERO,
    };
    (p1 - p0) * (1.0 / span.max(1e-3))
}

/// Raw velocity smoothed 1-2-1 across continuous frames.
fn vel(rec: &Recording, i: usize, k: usize) -> Vec2 {
    if i > 0 && cont(rec, i - 1, k) && cont(rec, i, k) {
        let (a, b, c) = (
            raw_vel(rec, i - 1, k),
            raw_vel(rec, i, k),
            raw_vel(rec, i + 1, k),
        );
        return a * 0.25 + b * 0.5 + c * 0.25;
    }
    raw_vel(rec, i, k)
}

fn compute_row(rec: &Recording, i: usize, prev: Option<&Row>, stride: StrideOf) -> Row {
    let f = &rec.frames[i];
    let mut out = [Slot::NONE; 10];
    let off = (f.offense as usize).min(1);
    // Whom the defenders face: the man with the ball, else the ball.
    let hold = f.holder.map(|(s, n)| {
        (
            s as usize * 5 + n as usize,
            f.pos[s as usize % 2][n as usize % 5],
        )
    });
    let hpos = hold.map_or(f.ball, |h| h.1);
    for k in 0..10 {
        let (side, n) = (k / 5, k % 5);
        let p = f.pos[side][n];
        let vl = vel(rec, i, k);
        let spd = vl.len();
        let linked = i > 0 && cont(rec, i - 1, k);
        let before = prev.map(|r| r[k]);
        // In stance: a defender in the half the other side attacks, with an
        // attacker close.
        let mut tgt = 0.0;
        let mut near = Vec2::ZERO;
        let mut dmin = 9999.0f32;
        if side != off {
            for &a in &f.pos[off] {
                let q = a - p;
                let d = q.len();
                if d < dmin {
                    dmin = d;
                    near = q;
                }
            }
            let front = if off == 0 { p.x > 13.0 } else { p.x < 15.0 };
            if dmin < 3.4 && front && spd < 4.5 {
                tgt = 1.0;
            }
        }
        let sp = if linked {
            before.map_or(tgt, |b| b.stance)
        } else {
            tgt
        };
        let st = sp + (tgt - sp) * if tgt > sp { 0.33 } else { 0.22 };
        // Facing: the direction of travel when moving, the ball when
        // standing; in stance the man with the ball (if close), else his
        // nearest attacker.
        let to_ball = f.ball - p;
        let bl = to_ball.len().max(0.01);
        let wv = (spd / 1.6).min(1.0);
        let mut nrm = to_ball * (1.0 / bl * (1.0 - wv));
        if spd > 0.01 {
            nrm = nrm + vl * (1.0 / spd * wv);
        }
        let mut t = nrm;
        if st > 0.02 {
            let mut sdir = to_ball * (1.0 / bl);
            let hd = hpos - p;
            let hl = hd.len();
            if hold.is_some() && hl < 6.0 && hl > 0.3 {
                sdir = hd * (1.0 / hl);
            } else if dmin < 20.0 {
                sdir = near * (1.0 / dmin.max(0.01));
            }
            let nl = nrm.len().max(0.001);
            t = sdir * st + nrm * (1.0 / nl * (1.0 - st));
        }
        if t.len() < 0.05 {
            t = nrm;
        }
        // What the engine recorded he faces beats what the viewer would guess.
        if let Some(fc) = f.pose[side][n].face() {
            t = fc;
        }
        if t.len() < 0.001 {
            t = Vec2 { x: 1.0, y: 0.0 };
        }
        t = t.norm();
        let mut face = t;
        if let (true, Some(b)) = (linked, before) {
            let kf = if spd > 0.5 || st > 0.5 { 0.33 } else { 0.16 };
            let m = b.face * (1.0 - kf) + t * kf;
            face = if m.len() < 0.05 { t } else { m.norm() };
        }
        let (leg, factor) = stride(side, f.who[side][n]);
        let (phase, dribble) = match (linked, before) {
            (true, Some(b)) => {
                let dist = p.dist(pos_of(rec, i - 1, k));
                let sl = stride_len(spd, st, leg, factor);
                let dph = TAU * dist / sl;
                (
                    b.phase + dph,
                    b.dribble + dph * (2.0 - smoothstep(1.5, 3.5, spd)),
                )
            }
            _ => ((f.who[side][n] as f32 * 1.7 + k as f32 * 0.9) % TAU, 0.0),
        };
        out[k] = Slot {
            vel: vl,
            face,
            stance: st,
            phase,
            dribble,
        };
    }
    out
}

impl Motion {
    fn new() -> Motion {
        let none = Stamp {
            time: 0.0,
            pos: Vec2::ZERO,
            who: [[0; 5]; 2],
        };
        Motion {
            rows: Vec::new(),
            tail: Vec::new(),
            len: 0,
            first: none,
            last: none,
        }
    }

    /// Whether `rec` is a recording this was built from, possibly grown.
    fn matches(&self, rec: &Recording) -> bool {
        self.len > 0
            && rec.frames.len() >= self.len
            && Stamp::of(rec, 0) == self.first
            && Stamp::of(rec, self.len - 1) == self.last
    }

    /// Process the frames that came since the last call.
    fn extend(&mut self, rec: &Recording, stride: StrideOf) {
        let n = rec.frames.len();
        if n == 0 {
            return;
        }
        if self.len == n && self.len > 0 {
            return;
        }
        // Commit every frame whose neighbours, two either way, exist.
        while self.rows.len() + 2 < n {
            let i = self.rows.len();
            let row = compute_row(rec, i, self.rows.last(), stride);
            self.rows.push(row);
        }
        self.tail.clear();
        for i in self.rows.len()..n {
            let prev = self.tail.last().or(self.rows.last()).copied();
            let row = compute_row(rec, i, prev.as_ref(), stride);
            self.tail.push(row);
        }
        self.len = n;
        self.first = Stamp::of(rec, 0);
        self.last = Stamp::of(rec, n - 1);
    }

    fn row(&self, i: usize) -> &Row {
        self.rows
            .get(i)
            .unwrap_or_else(|| &self.tail[i - self.rows.len()])
    }

    /// Everybody's gait at `time`, blended between the two frames around it
    /// the way the positions are.
    pub fn at(&self, rec: &Recording, time: f32) -> [[Gait; 5]; 2] {
        let i = rec.frame_at(time);
        let a = self.row(i);
        let mut u = 0.0;
        if let Some(b) = rec.frames.get(i + 1) {
            let span = b.time - rec.frames[i].time;
            if span > 0.0 && span <= 0.25 && !b.cut {
                u = ((time - rec.frames[i].time) / span).clamp(0.0, 1.0);
            }
        }
        let mut out = [[Gait::standing(Vec2 { x: 1.0, y: 0.0 }); 5]; 2];
        for k in 0..10 {
            let mut g = a[k].gait();
            if u > 0.0 && cont(rec, i, k) {
                let b = &self.row(i + 1)[k];
                let face = a[k].face * (1.0 - u) + b.face * u;
                g = Gait {
                    vel: a[k].vel * (1.0 - u) + b.vel * u,
                    face: if face.len() < 0.05 {
                        a[k].face
                    } else {
                        face.norm()
                    },
                    stance: a[k].stance + (b.stance - a[k].stance) * u,
                    phase: a[k].phase + (b.phase - a[k].phase) * u,
                    dribble: a[k].dribble + (b.dribble - a[k].dribble) * u,
                };
            }
            out[k / 5][k % 5] = g;
        }
        out
    }
}

/// A few recent recordings' motion, so that the film room can go back and
/// forth between games without computing them again.
#[derive(Default)]
pub(crate) struct MotionCache {
    items: Vec<Motion>,
}

impl MotionCache {
    /// The motion of `rec`, extended to its present length.
    pub fn get(&mut self, rec: &Recording, stride: StrideOf) -> &Motion {
        let at = match self.items.iter().position(|m| m.matches(rec)) {
            Some(at) => at,
            None => {
                self.items.push(Motion::new());
                if self.items.len() > 3 {
                    self.items.remove(0);
                }
                self.items.len() - 1
            }
        };
        self.items[at].extend(rec, stride);
        &self.items[at]
    }
}

/// The gait of everybody in `rec` at `time`, from a cache kept in egui's
/// temporary data (so it lives as long as the app and costs nothing to find).
pub(crate) fn sample(
    ctx: &eframe::egui::Context,
    rec: &Recording,
    time: f32,
    stride: StrideOf,
) -> [[Gait; 5]; 2] {
    if rec.frames.is_empty() {
        return [[Gait::standing(Vec2 { x: 1.0, y: 0.0 }); 5]; 2];
    }
    let id = eframe::egui::Id::unique("motion_cache");
    let cache: Arc<Mutex<MotionCache>> = ctx.data_mut(|d| {
        d.get_temp_mut_or_insert_with(id, || Arc::new(Mutex::new(MotionCache::default())))
            .clone()
    });
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    cache.get(rec, stride).at(rec, time)
}

#[cfg(test)]
mod tests {
    use super::*;
    use basketman_core::court::v;
    use basketman_core::replay::Frame;

    fn body(h: f32, hand: Hand) -> Body {
        Body {
            height_cm: h,
            weight_kg: h * 0.45,
            wingspan_cm: h + 6.0,
            standing_reach_cm: h * 1.32,
            shoulder_width_cm: 46.0,
            hand_length_cm: 19.5,
            hand,
        }
    }

    fn dims(h: f32) -> Dims {
        Dims::new(&body(h, Hand::Right), &Stats::AVERAGE)
    }

    #[test]
    fn segments_add_up_to_height_and_wingspan() {
        for cm in [175.0, 198.0, 221.0] {
            let b = body(cm, Hand::Right);
            let d = Dims::new(&b, &Stats::AVERAGE);
            let tall = d.ankle + d.thigh + d.shin + d.torso + d.neck + d.head;
            assert!((tall - cm / 100.0).abs() < 1e-4, "{tall} for {cm}");
            let span = 2.0 * (d.shoulder_half + d.upper_arm + d.forearm + d.hand);
            assert!((span - b.wingspan_cm / 100.0).abs() < 1e-4, "{span}");
        }
    }

    #[test]
    fn standing_pose_reaches_his_height_and_his_feet_are_on_the_floor() {
        let d = dims(198.0);
        let p = pose(&d, &Gait::standing(Vec2 { x: 1.0, y: 0.0 }), false);
        let top = p.j[HAIR_HIGH].z.max(p.j[HEAD_TOP].z);
        assert!((top - 1.98).abs() < 0.06, "{top}");
        for sd in 0..2 {
            // The sole is half a shoe's height under the heel and toe joints.
            let low = p.j[HEEL + sd].z.min(p.j[TOE + sd].z) - 0.034 * 1.98;
            assert!(low.abs() < 0.04, "foot {low}");
        }
    }

    fn run(d: &Dims, v: f32, t: f32) -> Pose {
        let sl = d.stride(v, 0.0);
        let gait = Gait {
            vel: Vec2 { x: v, y: 0.0 },
            face: Vec2 { x: 1.0, y: 0.0 },
            stance: 0.0,
            phase: TAU * v * t / sl,
            dribble: 0.0,
        };
        pose(d, &gait, false)
    }

    #[test]
    fn the_planted_foot_does_not_slide_at_a_steady_run() {
        let d = dims(198.0);
        for v in [2.0, 4.0, 6.0] {
            let dt = 0.002;
            let cycle = d.stride(v, 0.0) / v;
            let mut checked = 0;
            let mut t = 0.0;
            while t < cycle {
                // World position of the left ankle in two moments.
                let a = run(&d, v, t).j[ANKLE].x + v * t;
                let b = run(&d, v, t + dt).j[ANKLE].x + v * (t + dt);
                let phase = (TAU * v * t / d.stride(v, 0.0)) / TAU;
                let u = phase - phase.floor();
                let duty = duty(v, 0.0);
                // Away from heel strike and toe off.
                if u > 0.1 * duty && u < 0.9 * duty {
                    let speed = ((b - a) / dt).abs();
                    assert!(speed < 0.25, "v {v}: foot slides at {speed} m/s (u {u})");
                    checked += 1;
                }
                t += dt;
            }
            assert!(checked > 20);
        }
    }

    #[test]
    fn a_taller_player_has_a_longer_stride() {
        let (short, tall) = (dims(180.0), dims(215.0));
        for v in [1.0, 3.0, 6.0] {
            assert!(tall.stride(v, 0.0) > short.stride(v, 0.0) * 1.1);
        }
    }

    #[test]
    fn the_ball_is_on_the_shooting_side() {
        for (hand, sign) in [(Hand::Left, 1.0), (Hand::Right, -1.0)] {
            let d = Dims::new(&body(196.0, hand), &Stats::AVERAGE);
            for v in [0.0, 5.0] {
                let g = Gait {
                    vel: Vec2 { x: v, y: 0.0 },
                    face: Vec2 { x: 1.0, y: 0.0 },
                    stance: 0.0,
                    phase: 1.0,
                    dribble: 2.0,
                };
                let ball = pose(&d, &g, true).ball.unwrap();
                assert!(ball.y * sign > 0.03, "{hand:?} at {v}: {}", ball.y);
                assert!(ball.x > 0.1, "in front of him");
            }
        }
    }

    #[test]
    fn limbs_keep_their_lengths_in_a_stance_and_a_run() {
        let d = dims(201.0);
        for (v, s) in [(0.0, 1.0), (3.0, 1.0), (7.0, 0.0), (0.0, 0.0)] {
            let g = Gait {
                vel: Vec2 { x: v, y: 0.3 },
                face: Vec2 { x: 1.0, y: 0.0 },
                stance: s,
                phase: 2.2,
                dribble: 0.7,
            };
            let p = pose(&d, &g, true);
            for sd in 0..2 {
                let l = |a: usize, b: usize| (p.j[a + sd] - p.j[b + sd]).len();
                assert!((l(KNEE, HIP) - d.thigh).abs() < 1e-3);
                assert!((l(KNEE, ANKLE) - d.shin).abs() < 1e-3);
                assert!((l(ELBOW, SHOULDER) - d.upper_arm).abs() < 1e-3);
                assert!((l(ELBOW, WRIST) - d.forearm).abs() < 1e-3);
            }
        }
    }

    fn frame(t: f32, x: f32, who: u8) -> Frame {
        Frame {
            time: t,
            period: 0,
            clock: 0.0,
            shot_clock: 24.0,
            score: [0, 0],
            offense: 0,
            pos: [[v(x, 3.0); 5], [v(20.0, 8.0); 5]],
            who: [[who; 5], [1; 5]],
            ball: v(x, 3.0),
            ball_height: 1.0,
            holder: None,
            pose: Default::default(),
            whistle: false,
            cut: false,
            swish: false,
            officials: Default::default(),
            whistler: 0,
        }
    }

    fn rec(n: usize, sub_at: usize) -> Recording {
        Recording {
            frames: (0..n)
                .map(|i| {
                    frame(
                        i as f32 * 0.2,
                        2.0 + i as f32 * 0.9,
                        if i < sub_at { 0 } else { 2 },
                    )
                })
                .collect(),
            events: Vec::new(),
            ..Default::default()
        }
    }

    const LEG: &dyn Fn(usize, u8) -> (f32, f32) = &|_, _| (0.9, 1.0);

    #[test]
    fn growing_a_recording_gives_the_same_motion_as_having_it_all() {
        let full = rec(60, 1000);
        let mut cache = MotionCache::default();
        // Fed a frame at a time, as a live match is.
        for n in 1..=full.frames.len() {
            let part = Recording {
                frames: full.frames[..n].to_vec(),
                events: Vec::new(),
                ..Default::default()
            };
            let m = cache.get(&part, LEG);
            let _ = m.at(&part, part.duration());
        }
        let whole = {
            let mut c = MotionCache::default();
            let m = c.get(&full, LEG);
            m.at(&full, 5.0)
        };
        let grown = cache.get(&full, LEG).at(&full, 5.0);
        assert_eq!(cache.items.len(), 1, "one recording, one entry");
        for k in 0..5 {
            assert!((grown[0][k].phase - whole[0][k].phase).abs() < 1e-4);
            assert!((grown[0][k].vel.x - whole[0][k].vel.x).abs() < 1e-4);
        }
    }

    #[test]
    fn a_substitution_restarts_the_gait() {
        let r = rec(40, 20);
        let mut c = MotionCache::default();
        let m = c.get(&r, LEG);
        let before = m.row(19)[0].phase;
        let after = m.row(20)[0].phase;
        let after2 = m.row(21)[0].phase;
        assert!(before > 1.0);
        // The newcomer's phase is his own seed and then advances from it.
        assert!((after - (2.0 * 1.7) % TAU).abs() < 1e-4, "{after}");
        assert!(after2 > after);
        // At the changeover the blend does not smear the two men together.
        let g = m.at(&r, 3.9);
        assert!((g[0][0].phase - before).abs() < 1.5);
    }

    #[test]
    fn a_runner_gets_a_steady_phase_and_a_forward_face() {
        let r = rec(40, 1000);
        let mut c = MotionCache::default();
        let m = c.get(&r, LEG);
        let g = m.at(&r, 4.0);
        assert!(g[0][0].face.x > 0.95);
        assert!(g[0][0].vel.x > 3.5);
        let g2 = m.at(&r, 4.1);
        assert!(g2[0][0].phase > g[0][0].phase);
    }
}
