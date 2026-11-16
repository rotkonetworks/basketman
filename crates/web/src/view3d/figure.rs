//! Player figures: nineteen tapered boxes on a small skeleton, every length
//! built from the player's own measurements, after `viewer3d/match.psh`.
//!
//! The renderer knows nothing about them. A figure is whatever `emit_figure`
//! puts into the `Scene`. The pose itself comes from `gait`.

use basketman_core::court::Vec2;
use basketman_core::player::{Body, PlayerId};
use eframe::egui::Color32;

use super::action::{self, Act};
use super::gait::{self, Dims, Gait, Pose, Stats};
use super::{Scene, Vec3, v3};

/// Everything a figure needs to be drawn once.
pub(crate) struct FigureSpec<'a> {
    pub body: &'a Body,
    pub stats: Stats,
    /// Seeds his skin, hair and shoes, so that he looks the same everywhere.
    pub id: PlayerId,
    /// Where he stands on the floor, in court metres.
    pub pos: Vec2,
    pub gait: Gait,
    /// Jersey colour; the shorts are a darker shade of it.
    pub team: Color32,
    /// He holds or dribbles the ball.
    pub holds_ball: bool,
    /// What the recording says he is doing, if it is more than running.
    pub act: Option<Act>,
}

const SKIN_TONES: [[u8; 3]; 6] = [
    [247, 219, 190],
    [238, 196, 156],
    [214, 160, 116],
    [180, 124, 84],
    [138, 92, 62],
    [96, 64, 44],
];
const HAIR_TONES: [[u8; 3]; 5] = [
    [24, 20, 18],
    [50, 36, 28],
    [84, 58, 38],
    [146, 108, 58],
    [112, 46, 28],
];
const SHOE_TONES: [[u8; 3]; 3] = [[238, 238, 232], [34, 34, 40], [226, 196, 60]];

/// Skin, hair and shoes, from the player id.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct Looks {
    pub skin: Color32,
    pub hair: Color32,
    pub shoes: Color32,
}

impl Looks {
    pub fn of(id: PlayerId) -> Looks {
        let h = ((id as u64 + 1)
            .wrapping_mul(2_654_435_761)
            .wrapping_add(40_503))
            % (1 << 32);
        let c = |t: [u8; 3], shift: i32| {
            let f = |v: u8| (v as i32 + shift).clamp(0, 255) as u8;
            Color32::from_rgb(f(t[0]), f(t[1]), f(t[2]))
        };
        let skin_t = SKIN_TONES[((h >> 3) % 6) as usize];
        let jit = ((h >> 9) % 13) as i32 - 6;
        let hair_t = HAIR_TONES[((h >> 12) % 5) as usize];
        let jit2 = ((h >> 15) % 9) as i32 - 4;
        let hair = if (h >> 18).is_multiple_of(9) {
            // One in nine is shaved: hair close to the skin tone.
            let f = |v: u8| (v as f32 * 0.6) as i32 + jit;
            let g = |v: u8| f(v).clamp(0, 255) as u8;
            Color32::from_rgb(g(skin_t[0]), g(skin_t[1]), g(skin_t[2]))
        } else {
            c(hair_t, jit2)
        };
        Looks {
            skin: c(skin_t, jit),
            hair,
            shoes: c(SHOE_TONES[((h >> 21) % 3) as usize], 0),
        }
    }
}

fn darker(c: Color32, k: f32) -> Color32 {
    let f = |v: u8| (v as f32 * k) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

/// Where a body-frame point is in the world.
struct Frame {
    pos: Vec2,
    face: Vec2,
}

impl Frame {
    fn world(&self, p: Vec3) -> Vec3 {
        v3(
            self.pos.x + p.x * self.face.x - p.y * self.face.y,
            self.pos.y + p.x * self.face.y + p.y * self.face.x,
            p.z,
        )
    }
}

/// One tapered box between two joints: half sizes `wa` x `da` at the first
/// end and `wb` x `db` at the second, `w` along `side` (and `d` across it),
/// lengthened by `ext` half widths at both ends so that bent joints close
/// up. `bias` pulls it towards the eye when it sorts, so that the jersey
/// lies over the shorts and the hair over the head.
struct Seg {
    a: usize,
    b: usize,
    w: (f32, f32),
    d: (f32, f32),
    ext: f32,
    side: Vec3,
    bias: f32,
    color: Color32,
}

fn emit_seg(s: &mut Scene, fr: &Frame, pose: &Pose, seg: &Seg) {
    let (mut a, mut b) = (pose.j[seg.a], pose.j[seg.b]);
    let mut x = b - a;
    let len = x.len();
    if len < 0.002 {
        return;
    }
    x = x * (1.0 / len);
    if seg.ext > 0.0 {
        a = a - x * (seg.ext * seg.w.0);
        b = b + x * (seg.ext * seg.w.1);
    }
    // The side vector made perpendicular to the limb.
    let mut sd = seg.side - x * seg.side.dot(x);
    if sd.len() < 0.3 {
        let alt = if seg.side.y.abs() < 0.5 {
            v3(1.0, 0.0, 0.0)
        } else {
            v3(0.0, 1.0, 0.0)
        };
        sd = alt - x * alt.dot(x);
    }
    let sd = sd.norm();
    let dd = x.cross(sd);
    let corner =
        |p: Vec3, w: f32, d: f32, sw: f32, sdp: f32| fr.world(p + sd * (w * sw) + dd * (d * sdp));
    let c = [
        corner(a, seg.w.0, seg.d.0, 1.0, 1.0),
        corner(a, seg.w.0, seg.d.0, -1.0, 1.0),
        corner(a, seg.w.0, seg.d.0, -1.0, -1.0),
        corner(a, seg.w.0, seg.d.0, 1.0, -1.0),
        corner(b, seg.w.1, seg.d.1, 1.0, 1.0),
        corner(b, seg.w.1, seg.d.1, -1.0, 1.0),
        corner(b, seg.w.1, seg.d.1, -1.0, -1.0),
        corner(b, seg.w.1, seg.d.1, 1.0, -1.0),
    ];
    let centre = fr.world((a + b) * 0.5);
    let key = (s.depth(centre).sqrt() + seg.bias).max(0.0).powi(2);
    let mut face = |q: [usize; 4]| {
        let (p0, p1, p2, p3) = (c[q[0]], c[q[1]], c[q[2]], c[q[3]]);
        let n = (p1 - p0).cross(p2 - p0);
        let mid = (p0 + p1 + p2 + p3) * 0.25;
        // Wind it so that its normal points away from the middle of the box.
        if n.dot(mid - centre) >= 0.0 {
            s.solid_tri_keyed(p0, p1, p2, seg.color, false, key);
            s.solid_tri_keyed(p0, p2, p3, seg.color, false, key);
        } else {
            s.solid_tri_keyed(p2, p1, p0, seg.color, false, key);
            s.solid_tri_keyed(p3, p2, p0, seg.color, false, key);
        }
    };
    for f in 0..4 {
        let g = (f + 1) & 3;
        face([f, g, g + 4, f + 4]);
    }
    face([0, 1, 2, 3]);
    face([4, 5, 6, 7]);
}

/// Put one figure into the scene. Returns where the ball is when he holds
/// it, in court coordinates.
pub(crate) fn emit_figure(s: &mut Scene, f: &FigureSpec) -> Option<Vec3> {
    let d = Dims::new(f.body, &f.stats);
    let face = if f.gait.face.len() > 0.1 {
        f.gait.face.norm()
    } else {
        Vec2 { x: 1.0, y: 0.0 }
    };
    let mut pose = match &f.act {
        Some(a) if a.takes_over() => {
            // Feet planted when the recording says so: no running legs under a jump.
            let g = Gait {
                vel: if a.planted() { Vec2::ZERO } else { f.gait.vel },
                face: f.gait.face,
                stance: f.gait.stance,
                phase: f.gait.phase,
                dribble: f.gait.dribble,
                legs: f.gait.legs,
            };
            let mut p = gait::pose(&d, &g, false);
            action::apply(&mut p, &d, a);
            p
        }
        _ => gait::pose(&d, &f.gait, f.holds_ball),
    };
    // The eyes look where the recording says, within their range of the body.
    pose.turn_head(f.gait.legs.yaw);
    let looks = Looks::of(f.id);
    // A pivot turns about the planted foot, which stays where he is recorded:
    // the hips swing round it.
    let mut at = f.pos;
    if f.gait.legs.pivot > 0.0 && f.gait.legs.planted != 0 {
        let sg = if f.gait.legs.planted == 1 { 1.0 } else { -1.0 };
        let off = d.hip_half * sg * f.gait.legs.pivot;
        at = Vec2 {
            x: at.x + face.y * off,
            y: at.y - face.x * off,
        };
    }
    let fr = Frame { pos: at, face };
    figure_segments(s, &fr, &pose, &d, &looks, f.team);
    pose.ball.take().map(|b| fr.world(b))
}

/// A court point in the frame of a player at `pos` facing `face`.
pub(crate) fn to_body(pos: Vec2, face: Vec2, p: Vec3) -> Vec3 {
    let (dx, dy) = (p.x - pos.x, p.y - pos.y);
    v3(dx * face.x + dy * face.y, -dx * face.y + dy * face.x, p.z)
}

fn figure_segments(s: &mut Scene, fr: &Frame, body: &Pose, d: &Dims, looks: &Looks, team: Color32) {
    use gait::*;
    let h = d.height;
    let (sw, tm, hl) = (d.shoulder_width, d.build, d.hand);
    let ta = tm.sqrt();
    let bw = 0.92 + 0.12 * (tm - 1.0);
    let hv = 0.034 * h;
    let shorts = darker(team, 0.68);
    let skin = looks.skin;
    let across = v3(0.0, 1.0, 0.0);
    let (sy, cy) = body.yaw.sin_cos();
    let head_side = v3(-sy, cy, 0.0);
    let along = v3(1.0, 0.0, 0.0);
    let mut seg = |a, b, w: (f32, f32), dp: (f32, f32), ext, side, bias, color| {
        emit_seg(
            s,
            fr,
            body,
            &Seg {
                a,
                b,
                w,
                d: dp,
                ext,
                side,
                bias,
                color,
            },
        )
    };
    // Shorts and jersey.
    seg(
        SHORTS_LOW,
        SHORTS_HIGH,
        (0.37 * sw * bw, 0.37 * sw * bw),
        (0.26 * sw * tm, 0.26 * sw * tm),
        0.0,
        across,
        0.0,
        shorts,
    );
    seg(
        TORSO_LOW,
        JERSEY_TOP,
        (0.35 * sw * bw, 0.45 * sw * bw),
        (0.25 * sw * tm, 0.28 * sw * tm),
        0.0,
        across,
        -0.05,
        team,
    );
    // Head.
    seg(
        NECK_BASE,
        NECK_TOP,
        (0.026 * h * ta, 0.022 * h * ta),
        (0.026 * h * ta, 0.022 * h * ta),
        0.3,
        across,
        0.0,
        skin,
    );
    seg(
        NECK_TOP,
        HEAD_TOP,
        (0.046 * h, 0.043 * h),
        (0.054 * h, 0.050 * h),
        0.0,
        head_side,
        0.0,
        skin,
    );
    seg(
        HAIR_LOW,
        HAIR_HIGH,
        (0.049 * h, 0.047 * h),
        (0.056 * h, 0.053 * h),
        0.0,
        head_side,
        -0.02,
        looks.hair,
    );
    for sd in 0..2 {
        // Arms: bare skin, the jersey is sleeveless.
        seg(
            SHOULDER + sd,
            ELBOW + sd,
            (0.032 * h * ta, 0.027 * h * ta),
            (0.032 * h * ta, 0.027 * h * ta),
            0.5,
            along,
            0.0,
            skin,
        );
        seg(
            ELBOW + sd,
            WRIST + sd,
            (0.026 * h * ta, 0.019 * h * ta),
            (0.026 * h * ta, 0.019 * h * ta),
            0.5,
            along,
            0.0,
            skin,
        );
        seg(
            WRIST + sd,
            FINGERS + sd,
            (0.17 * hl, 0.15 * hl),
            (0.13 * hl, 0.10 * hl),
            0.0,
            across,
            -0.02,
            skin,
        );
        // Legs.
        seg(
            HIP + sd,
            SHORTS_END + sd,
            (0.056 * h * tm, 0.052 * h * tm),
            (0.054 * h * tm, 0.050 * h * tm),
            0.3,
            across,
            0.0,
            shorts,
        );
        seg(
            SHORTS_END + sd,
            KNEE + sd,
            (0.050 * h * tm, 0.040 * h * tm),
            (0.048 * h * tm, 0.040 * h * tm),
            0.4,
            across,
            0.0,
            skin,
        );
        seg(
            KNEE + sd,
            ANKLE + sd,
            (0.038 * h * tm, 0.024 * h * ta),
            (0.038 * h * tm, 0.024 * h * ta),
            0.4,
            across,
            0.0,
            skin,
        );
        seg(
            HEEL + sd,
            TOE + sd,
            (0.024 * h, 0.030 * h),
            (hv, hv * 0.9),
            0.0,
            across,
            0.0,
            looks.shoes,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use basketman_core::player::{Body, Hand};

    fn body(height: f32) -> Body {
        Body {
            height_cm: height,
            weight_kg: 90.0,
            wingspan_cm: height + 5.0,
            standing_reach_cm: height * 1.3,
            shoulder_width_cm: 46.0,
            hand_length_cm: 20.0,
            hand: Hand::Right,
        }
    }

    fn spec(body: &Body) -> FigureSpec<'_> {
        FigureSpec {
            body,
            stats: Stats::AVERAGE,
            id: 7,
            pos: Vec2 { x: 10.0, y: 5.0 },
            gait: Gait {
                vel: Vec2 { x: 0.0, y: 4.0 },
                face: Vec2 { x: 0.0, y: 1.0 },
                stance: 0.0,
                phase: 1.3,
                dribble: 0.0,
                legs: Default::default(),
            },
            team: Color32::from_rgb(98, 67, 217),
            holds_ball: false,
            act: None,
        }
    }

    fn extent(s: &Scene) -> (f32, f32, f32) {
        let pts = || s.solids.iter().flat_map(|t| t.v.iter());
        let top = pts().map(|p| p.z).fold(f32::MIN, f32::max);
        let low = pts().map(|p| p.z).fold(f32::MAX, f32::min);
        let wide = pts().map(|p| p.x).fold(f32::MIN, f32::max)
            - pts().map(|p| p.x).fold(f32::MAX, f32::min);
        (low, top, wide)
    }

    #[test]
    fn figure_stands_on_the_floor_and_is_as_tall_as_the_player() {
        for cm in [180.0, 198.0, 215.0] {
            let b = body(cm);
            // Seen from above and every side, so that nothing is culled away.
            let mut s = Scene::new(v3(10.0, 5.0, 100.0));
            let mut sp = spec(&b);
            sp.gait.vel = Vec2::ZERO;
            emit_figure(&mut s, &sp);
            let (low, top, _) = extent(&s);
            assert!(low >= -0.02, "feet under the floor: {low}");
            assert!(low < 0.06);
            assert!(
                (top - cm / 100.0).abs() < 0.05 * cm / 100.0,
                "{top} for {cm}"
            );
        }
    }

    #[test]
    fn arms_out_reach_the_wingspan() {
        let b = body(198.0);
        let mut sp = spec(&b);
        sp.gait.vel = Vec2::ZERO;
        sp.gait.stance = 1.0;
        // Facing +y, his arms spread along x.
        let mut s = Scene::new(v3(10.0, 5.0, 100.0));
        emit_figure(&mut s, &sp);
        let (_, _, wide) = extent(&s);
        // Hands are up and forward in a stance, so a bit short of the full span.
        assert!(wide > 1.4 && wide < 2.1, "{wide}");
    }

    #[test]
    fn a_figure_is_about_a_hundred_triangles_once_culled() {
        let b = body(198.0);
        let mut s = Scene::new(v3(10.0, -14.0, 11.0));
        emit_figure(&mut s, &spec(&b));
        println!("triangles per figure: {}", s.solids.len());
        assert!(
            s.solids.len() > 60 && s.solids.len() < 160,
            "{}",
            s.solids.len()
        );
    }

    /// Ten figures in motion, built without the rest of the scene. Run with
    /// `--nocapture` to see the numbers.
    #[test]
    fn ten_figures_are_cheap_to_build() {
        let b = body(198.0);
        let n = 500;
        let mut last = 0;
        let start = std::time::Instant::now();
        for i in 0..n {
            let mut s = Scene::new(v3(14.0, -14.0, 11.0));
            for k in 0..10 {
                let mut sp = spec(&b);
                sp.pos = Vec2 {
                    x: 6.0 + k as f32 * 1.5,
                    y: 3.0 + (k % 5) as f32 * 2.0,
                };
                sp.gait.phase = k as f32 + i as f32 * 0.1;
                sp.holds_ball = k == 3;
                emit_figure(&mut s, &sp);
            }
            last = s.solids.len();
        }
        let each = start.elapsed().as_secs_f64() * 1000.0 / n as f64;
        println!("ten figures: {each:.3} ms, {last} triangles kept");
        assert!(each < 5.0, "{each} ms");
    }

    #[test]
    fn looks_are_stable_and_varied() {
        assert_eq!(Looks::of(41), Looks::of(41));
        let skins: std::collections::HashSet<_> = (0..60).map(|i| Looks::of(i).skin).collect();
        assert!(skins.len() > 6);
    }
}
