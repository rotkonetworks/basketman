//! A small software 3D pipeline for the match view.
//!
//! The same design as the Plush prototype in `viewer3d/match.psh`: a
//! perspective camera, near-plane clipping, back-face culling, flat shading
//! with one light and a painter's algorithm. The CPU does all of that and
//! hands egui a mesh of flat-coloured triangles, so it looks the same
//! natively and in the browser. Units are metres; the court lies in the x/y
//! plane as everywhere else (x along it 0..28, y across it 0..15, z up).

mod action;
pub(crate) mod arena;
pub(crate) mod boards;
mod figure;
pub(crate) mod finder;
mod font;
pub(crate) mod gait;

use basketman_core::court::{
    CENTRE_RADIUS, CORNER_X, CORNER_Y, FT_RADIUS, HALF_WIDTH, HOOP, KEY_DEPTH, KEY_HALF_WIDTH,
    LENGTH, NO_CHARGE_RADIUS, THREE_RADIUS,
};
use eframe::egui::epaint::{Vertex, WHITE_UV};
use eframe::egui::{Color32, Mesh, Pos2, Rect, TextureId, pos2};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

pub(crate) use action::Act;
pub(crate) use figure::{FigureSpec, emit_figure, emit_official, to_body};

/// Nothing nearer than this (along the viewing direction) is drawn.
pub(crate) const NEAR: f32 = 0.3;
/// Focal length as a share of the view's width, as in the prototype.
const FOCAL: f32 = 0.95;
const WIDTH: f32 = 2.0 * HALF_WIDTH;
const LINE_WIDTH: f32 = 0.07;

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub(crate) struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub(crate) const fn v3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3 { x, y, z }
}

impl Vec3 {
    pub const Z: Vec3 = v3(0.0, 0.0, 1.0);

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        v3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }

    /// Unit vector, or zero for a zero vector.
    pub fn norm(self) -> Vec3 {
        let l = self.len();
        if l > 1e-9 {
            self * (1.0 / l)
        } else {
            Vec3::default()
        }
    }

    pub fn lerp(self, o: Vec3, t: f32) -> Vec3 {
        self + (o - self) * t
    }
}

impl std::ops::Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl std::ops::Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, k: f32) -> Vec3 {
        v3(self.x * k, self.y * k, self.z * k)
    }
}

impl std::ops::Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        v3(-self.x, -self.y, -self.z)
    }
}

/// A camera: where it is, what it looks at, and its zoom.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    /// Which way is up on the screen, roughly. Straight down needs its own.
    pub up: Vec3,
    pub zoom: f32,
}

/// The camera's axes: `x` right, `y` up on the screen, `z` forward.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Basis {
    pub eye: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub fwd: Vec3,
}

impl Camera {
    pub fn new(eye: Vec3, target: Vec3) -> Self {
        Camera {
            eye,
            target,
            up: Vec3::Z,
            zoom: 1.0,
        }
    }

    pub fn basis(&self) -> Basis {
        let fwd = (self.target - self.eye).norm();
        let right = fwd.cross(self.up).norm();
        let up = right.cross(fwd);
        Basis {
            eye: self.eye,
            right,
            up,
            fwd,
        }
    }

    /// Screen position of a world point in `rect`, and its depth. `None`
    /// if it is behind the near plane.
    pub fn project(&self, p: Vec3, rect: Rect) -> Option<(Pos2, f32)> {
        let b = self.basis();
        let c = b.to_cam(p);
        (c.z >= NEAR).then(|| (screen(c, rect, self.zoom), c.z))
    }

    /// Screen points per metre at a distance `depth` from the camera.
    pub fn scale(&self, depth: f32, rect: Rect) -> f32 {
        rect.width() * FOCAL * self.zoom / depth.max(NEAR)
    }
}

impl Basis {
    pub fn to_cam(&self, p: Vec3) -> Vec3 {
        let d = p - self.eye;
        v3(d.dot(self.right), d.dot(self.up), d.dot(self.fwd))
    }
}

fn screen(c: Vec3, rect: Rect, zoom: f32) -> Pos2 {
    let k = rect.width() * FOCAL * zoom / c.z;
    pos2(rect.center().x + c.x * k, rect.center().y - c.y * k)
}

/// The camera presets.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Preset {
    /// From the near touchline and above, following the ball along the court.
    Broadcast,
    /// Around the middle of the court; drag to turn it.
    Orbit,
    /// Straight down on the whole court.
    Top,
    /// From behind the end line the ball is at.
    Basket,
}

impl Preset {
    pub const ALL: [Preset; 4] = [
        Preset::Broadcast,
        Preset::Orbit,
        Preset::Top,
        Preset::Basket,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Preset::Broadcast => "Broadcast",
            Preset::Orbit => "Orbit",
            Preset::Top => "Top",
            Preset::Basket => "Basket",
        }
    }

    /// Name used by the screenshot harness.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn parse(s: &str) -> Option<Preset> {
        Some(match s {
            "broadcast" => Preset::Broadcast,
            "orbit" => Preset::Orbit,
            "top" => Preset::Top,
            "basket" => Preset::Basket,
            _ => return None,
        })
    }
}

/// The user's 3D camera: preset, orbit angles, zoom, and the smoothed state
/// that lets the broadcast camera glide after the ball.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Cam3 {
    pub preset: Preset,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub zoom: f32,
    /// Where the camera thinks the ball is along the court.
    follow: f32,
    /// Where the end-line camera stands along the court.
    end: f32,
    /// True while the end-line camera is at the far (x = 28) end.
    far_end: bool,
    primed: bool,
    /// The screenshot harness pins the camera: eye and the point it looks at.
    pub fixed: Option<(Vec3, Vec3)>,
}

impl Default for Cam3 {
    fn default() -> Self {
        Cam3 {
            preset: Preset::Broadcast,
            yaw: 0.6,
            pitch: 0.55,
            dist: 24.0,
            zoom: 1.0,
            follow: LENGTH / 2.0,
            end: -6.0,
            far_end: false,
            primed: false,
            fixed: None,
        }
    }
}

impl Cam3 {
    /// Move the smoothed state `dt` seconds towards the ball and return the
    /// camera to render with.
    pub fn update(&mut self, ball: Vec3, dt: f32) -> Camera {
        let bx = ball.x;
        if !self.primed || (bx - self.follow).abs() > 9.0 {
            // First frame, or the user jumped about in time.
            self.follow = bx;
            self.far_end = bx >= LENGTH / 2.0;
            self.end = if self.far_end { 35.0 } else { -7.0 };
            self.primed = true;
        }
        let k = 1.0 - (-3.0 * dt).exp();
        self.follow += (bx - self.follow) * k;
        if !self.far_end && bx > 17.0 {
            self.far_end = true;
        }
        if self.far_end && bx < 11.0 {
            self.far_end = false;
        }
        let want = if self.far_end { LENGTH + 7.0 } else { -7.0 };
        self.end += (want - self.end) * (1.0 - (-1.5 * dt).exp());
        let mut cam = match self.preset {
            Preset::Broadcast => {
                let tx = self.follow.clamp(5.5, 22.5);
                let ex = (tx * 0.7 + 14.0 * 0.3).clamp(7.5, 20.5);
                Camera::new(v3(ex, -14.0, 11.0), v3(tx, 6.2, 0.0))
            }
            Preset::Orbit => {
                let t = v3(LENGTH * 0.4 + self.follow * 0.6, HALF_WIDTH, 0.8);
                let hd = self.dist * self.pitch.cos();
                Camera::new(
                    t + v3(
                        hd * self.yaw.cos(),
                        hd * self.yaw.sin(),
                        self.dist * self.pitch.sin(),
                    ),
                    t,
                )
            }
            Preset::Top => {
                let mut c = Camera::new(v3(14.0, HALF_WIDTH, 31.0), v3(14.0, HALF_WIDTH, 0.0));
                c.up = v3(0.0, 1.0, 0.0);
                c
            }
            Preset::Basket => Camera::new(
                v3(self.end, -3.0, 7.0),
                v3(self.follow.clamp(2.0, 26.0), ball.y, 1.0),
            ),
        };
        if let Some((eye, at)) = self.fixed {
            cam = Camera::new(eye, at);
        }
        cam.zoom = self.zoom;
        cam
    }

    /// True while the camera is still catching up with the ball, so the
    /// caller keeps repainting.
    pub fn settling(&self, ball: Vec3) -> bool {
        let want = if self.far_end { LENGTH + 7.0 } else { -7.0 };
        (ball.x - self.follow).abs() > 0.02 || (want - self.end).abs() > 0.02
    }
}

/// One flat-coloured triangle in world space, already shaded.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Tri {
    pub v: [Vec3; 3],
    pub color: Color32,
    /// Distance from the eye squared: the painter's algorithm sorts on it.
    depth: f32,
    /// Texture coordinates of the corners, for a textured board; flat
    /// triangles have none and sample the white pixel.
    pub uv: Option<[Pos2; 3]>,
}

/// Everything to draw this frame. `ground` is drawn first, in the order it
/// was added (floor, lines, shadows); `solids` are culled and sorted far to
/// near.
pub(crate) struct Scene {
    eye: Vec3,
    light: Vec3,
    pub ground: Vec<Tri>,
    pub solids: Vec<Tri>,
    /// The floor, the runoff and the key; the arena sets them.
    pub floor: Color32,
    pub runoff: Color32,
    pub paint: Color32,
    /// The texture of the whole mesh, when it has textured boards, and where
    /// its white pixel is.
    pub texture: Option<(TextureId, Pos2)>,
}

impl Scene {
    pub fn new(eye: Vec3) -> Self {
        Scene {
            eye,
            light: v3(-0.42, -0.52, 0.75).norm(),
            ground: Vec::with_capacity(512),
            solids: Vec::with_capacity(2048),
            floor: FLOOR,
            runoff: RUNOFF,
            paint: PAINT,
            texture: None,
        }
    }

    /// A colour as lit for a surface facing `n`.
    pub fn shade(&self, color: Color32, n: Vec3) -> Color32 {
        let k = (0.52 + 0.62 * n.dot(self.light).max(0.0)).min(1.1);
        let c = |v: u8| ((v as f32 * k).round()).clamp(0.0, 255.0) as u8;
        Color32::from_rgba_premultiplied(c(color.r()), c(color.g()), c(color.b()), color.a())
    }

    /// A triangle on the floor, drawn in the order given and never culled.
    pub fn ground_tri(&mut self, a: Vec3, b: Vec3, c: Vec3, color: Color32) {
        self.ground.push(Tri {
            v: [a, b, c],
            color,
            depth: 0.0,
            uv: None,
        });
    }

    /// A convex polygon on the floor.
    pub fn ground_poly(&mut self, pts: &[Vec3], color: Color32) {
        for i in 1..pts.len().saturating_sub(1) {
            self.ground_tri(pts[0], pts[i], pts[i + 1], color);
        }
    }

    pub fn ground_rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Color32) {
        self.ground_poly(
            &[
                v3(x0, y0, 0.0),
                v3(x1, y0, 0.0),
                v3(x1, y1, 0.0),
                v3(x0, y1, 0.0),
            ],
            color,
        );
    }

    /// A line on the floor as a thin quad. It reaches half a width past each
    /// end, so that segments of an arc join without gaps.
    pub fn ground_line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, color: Color32) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            return;
        }
        let (ux, uy) = (dx / len * width * 0.5, dy / len * width * 0.5);
        let (nx, ny) = (-uy, ux);
        let (ax, ay, bx, by) = (a.0 - ux, a.1 - uy, b.0 + ux, b.1 + uy);
        self.ground_poly(
            &[
                v3(ax + nx, ay + ny, 0.0),
                v3(bx + nx, by + ny, 0.0),
                v3(bx - nx, by - ny, 0.0),
                v3(ax - nx, ay - ny, 0.0),
            ],
            color,
        );
    }

    /// An arc of a circle on the floor, angles in radians.
    pub fn ground_arc(
        &mut self,
        c: (f32, f32),
        r: f32,
        from: f32,
        to: f32,
        steps: usize,
        width: f32,
        color: Color32,
    ) {
        let at = |i: usize| {
            let a = from + (to - from) * i as f32 / steps as f32;
            (c.0 + r * a.cos(), c.1 + r * a.sin())
        };
        for i in 0..steps {
            self.ground_line(at(i), at(i + 1), width, color);
        }
    }

    /// A filled ellipse on the floor.
    pub fn ground_disc(&mut self, c: (f32, f32), rx: f32, ry: f32, color: Color32) {
        const N: usize = 14;
        let pts: Vec<Vec3> = (0..N)
            .map(|i| {
                let a = TAU * i as f32 / N as f32;
                v3(c.0 + rx * a.cos(), c.1 + ry * a.sin(), 0.0)
            })
            .collect();
        self.ground_poly(&pts, color);
    }

    /// A triangle that stands in the scene. Wound counter-clockwise seen
    /// from its front, which is the only side drawn unless `both`.
    /// Squared distance from the eye: what things are sorted on.
    pub fn depth(&self, p: Vec3) -> f32 {
        let d = self.eye - p;
        d.dot(d)
    }

    /// As `solid_tri`, sorted as if it were at depth `key`. The triangles of
    /// one box share a key: its faces never overlap one another on screen,
    /// and sorting them separately would let other boxes slip in between.
    pub fn solid_tri_keyed(
        &mut self,
        a: Vec3,
        b: Vec3,
        c: Vec3,
        color: Color32,
        both: bool,
        key: f32,
    ) {
        let n = (b - a).cross(c - a).norm();
        let centre = (a + b + c) * (1.0 / 3.0);
        let to_eye = self.eye - centre;
        let facing = n.dot(to_eye) > 0.0;
        if !facing && !both {
            return;
        }
        // A two-sided triangle is lit from the side the eye is on.
        let n = if facing { n } else { -n };
        self.solids.push(Tri {
            v: [a, b, c],
            color: self.shade(color, n),
            depth: key,
            uv: None,
        });
    }

    pub fn solid_quad(&mut self, p: [Vec3; 4], color: Color32, both: bool) {
        let key = self.depth((p[0] + p[1] + p[2] + p[3]) * 0.25);
        self.solid_quad_keyed(p, color, both, key);
    }

    fn solid_quad_keyed(&mut self, p: [Vec3; 4], color: Color32, both: bool, key: f32) {
        self.solid_tri_keyed(p[0], p[1], p[2], color, both, key);
        self.solid_tri_keyed(p[0], p[2], p[3], color, both, key);
    }

    /// A box given by its centre and its three half-extent vectors, which
    /// need not be axis-aligned or unit length, only mutually perpendicular.
    pub fn cuboid(&mut self, c: Vec3, ax: Vec3, ay: Vec3, az: Vec3, color: Color32) {
        let key = self.depth(c);
        self.cuboid_keyed(c, ax, ay, az, color, key);
    }

    /// As `cuboid`, sorted at depth `key` (see `depth`), so that a part can
    /// be drawn just in front of or behind another.
    pub fn cuboid_keyed(
        &mut self,
        c: Vec3,
        ax: Vec3,
        ay: Vec3,
        az: Vec3,
        color: Color32,
        key: f32,
    ) {
        let axes = [ax, ay, az];
        for i in 0..3 {
            let (u, w) = (axes[(i + 1) % 3], axes[(i + 2) % 3]);
            for sign in [1.0, -1.0] {
                let o = axes[i] * sign;
                let f = c + o;
                let quad = [f - u - w, f + u - w, f + u + w, f - u + w];
                // Wind it so its normal points away from the centre.
                let n = (quad[1] - quad[0]).cross(quad[2] - quad[0]);
                if n.dot(o) >= 0.0 {
                    self.solid_quad_keyed(quad, color, false, key);
                } else {
                    self.solid_quad_keyed([quad[3], quad[2], quad[1], quad[0]], color, false, key);
                }
            }
        }
    }

    /// An upright box standing on `z0`, `hx`/`hy` half-sizes along x and y.
    pub fn block(&mut self, c: (f32, f32), z0: f32, hx: f32, hy: f32, h: f32, color: Color32) {
        self.cuboid(
            v3(c.0, c.1, z0 + h / 2.0),
            v3(hx, 0.0, 0.0),
            v3(0.0, hy, 0.0),
            v3(0.0, 0.0, h / 2.0),
            color,
        );
    }

    /// A low-poly sphere.
    pub fn sphere(&mut self, c: Vec3, r: f32, color: Color32) {
        const SEG: usize = 10;
        const RINGS: usize = 6;
        let key = self.depth(c);
        let p = |ring: usize, seg: usize| {
            let a = PI * ring as f32 / RINGS as f32 - FRAC_PI_2;
            let t = TAU * seg as f32 / SEG as f32;
            c + v3(r * a.cos() * t.cos(), r * a.cos() * t.sin(), r * a.sin())
        };
        for ring in 0..RINGS {
            for seg in 0..SEG {
                let (a, b) = (p(ring, seg), p(ring, seg + 1));
                let (d, e) = (p(ring + 1, seg), p(ring + 1, seg + 1));
                if ring > 0 {
                    self.solid_tri_keyed(a, b, e, color, false, key);
                }
                if ring + 1 < RINGS {
                    self.solid_tri_keyed(a, e, d, color, false, key);
                }
            }
        }
    }

    /// Everything as a mesh in `rect`: the ground in order, then the solids
    /// far to near.
    pub fn mesh(&mut self, cam: &Camera, rect: Rect) -> Mesh {
        let basis = cam.basis();
        let mut mesh = Mesh::default();
        let white = self.texture.map_or(WHITE_UV, |t| t.1);
        if let Some((id, _)) = self.texture {
            mesh.texture_id = id;
        }
        mesh.reserve_triangles(self.ground.len() + self.solids.len());
        mesh.reserve_vertices((self.ground.len() + self.solids.len()) * 3);
        for t in &self.ground {
            emit(&mut mesh, &basis, cam.zoom, rect, t, white);
        }
        // Far to near: the larger distance first.
        self.solids.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        for t in &self.solids {
            emit(&mut mesh, &basis, cam.zoom, rect, t, white);
        }
        mesh
    }
}

/// A polygon of at most four corners, as near clipping can turn a triangle
/// into a quad.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Poly {
    pub pts: [Vec3; 4],
    pub n: usize,
}

impl Poly {
    pub fn points(&self) -> &[Vec3] {
        &self.pts[..self.n]
    }
}

/// Clip a triangle in camera space against the near plane `z = NEAR`,
/// keeping what lies in front of it.
pub(crate) fn clip_near(tri: [Vec3; 3]) -> Poly {
    let mut out = Poly {
        pts: [Vec3::default(); 4],
        n: 0,
    };
    for i in 0..3 {
        let (a, b) = (tri[i], tri[(i + 1) % 3]);
        let (a_in, b_in) = (a.z >= NEAR, b.z >= NEAR);
        if a_in {
            out.pts[out.n] = a;
            out.n += 1;
        }
        if a_in != b_in {
            let t = (NEAR - a.z) / (b.z - a.z);
            let mut p = a.lerp(b, t);
            p.z = NEAR;
            out.pts[out.n] = p;
            out.n += 1;
        }
    }
    out
}

fn emit(mesh: &mut Mesh, basis: &Basis, zoom: f32, rect: Rect, t: &Tri, white: Pos2) {
    let c = [
        basis.to_cam(t.v[0]),
        basis.to_cam(t.v[1]),
        basis.to_cam(t.v[2]),
    ];
    if c.iter().all(|p| p.z < NEAR) {
        return;
    }
    let poly = clip_near(c);
    if poly.n < 3 {
        return;
    }
    let base = mesh.vertices.len() as u32;
    for p in poly.points() {
        // A clipped corner lies on an edge or is a corner: its texture
        // coordinate follows from its place in the triangle.
        let uv = t.uv.map_or(white, |uv| clipped_uv(&c, &uv, *p));
        mesh.vertices.push(Vertex {
            pos: screen(*p, rect, zoom),
            uv,
            color: t.color,
        });
    }
    for i in 1..poly.n as u32 - 1 {
        mesh.add_triangle(base, base + i, base + i + 1);
    }
}

/// The texture coordinate at camera-space point `p`, which lies in the plane
/// of the triangle `c` with corner coordinates `uv`.
fn clipped_uv(c: &[Vec3; 3], uv: &[Pos2; 3], p: Vec3) -> Pos2 {
    let (e1, e2, d) = (c[1] - c[0], c[2] - c[0], p - c[0]);
    let (d11, d12, d22) = (e1.dot(e1), e1.dot(e2), e2.dot(e2));
    let (d1, d2) = (d.dot(e1), d.dot(e2));
    let det = d11 * d22 - d12 * d12;
    if det.abs() < 1e-12 {
        return uv[0];
    }
    let b = (d22 * d1 - d12 * d2) / det;
    let g = (d11 * d2 - d12 * d1) / det;
    pos2(
        uv[0].x + b * (uv[1].x - uv[0].x) + g * (uv[2].x - uv[0].x),
        uv[0].y + b * (uv[1].y - uv[0].y) + g * (uv[2].y - uv[0].y),
    )
}

// ------------------------------------------------------------ the scene

pub(crate) const FLOOR: Color32 = Color32::from_rgb(206, 158, 104);
pub(crate) const RUNOFF: Color32 = Color32::from_rgb(160, 118, 76);
pub(crate) const PAINT: Color32 = Color32::from_rgb(176, 70, 62);
pub(crate) const LINE: Color32 = Color32::from_rgb(245, 245, 240);
/// What lies around the court.
pub(crate) const SURROUND: Color32 = Color32::from_rgb(34, 40, 56);
const BALL: Color32 = Color32::from_rgb(232, 118, 36);
const RIM: Color32 = Color32::from_rgb(250, 110, 30);

/// A point at `u` metres from the end line and `v` across from the middle of
/// it, at the end `end` (0 is x = 0), as court coordinates.
fn local(end: usize, u: f32, v: f32) -> (f32, f32) {
    if end == 0 {
        (u, HALF_WIDTH + v)
    } else {
        (LENGTH - u, HALF_WIDTH - v)
    }
}

/// Floor, paint and every line of a FIBA court.
pub(crate) fn court(s: &mut Scene) {
    let (runoff, floor) = (s.runoff, s.floor);
    s.ground_rect(-2.5, -2.0, LENGTH + 2.5, WIDTH + 2.0, runoff);
    s.ground_rect(0.0, 0.0, LENGTH, WIDTH, floor);
    let paint = s.paint;
    for end in 0..2 {
        let (a, b) = (
            local(end, 0.0, -KEY_HALF_WIDTH),
            local(end, KEY_DEPTH, KEY_HALF_WIDTH),
        );
        s.ground_rect(
            a.0.min(b.0),
            a.1.min(b.1),
            a.0.max(b.0),
            a.1.max(b.1),
            paint,
        );
    }
    let w = LINE_WIDTH;
    let line = |s: &mut Scene, a: (f32, f32), b: (f32, f32)| s.ground_line(a, b, w, LINE);
    line(s, (0.0, 0.0), (LENGTH, 0.0));
    line(s, (LENGTH, 0.0), (LENGTH, WIDTH));
    line(s, (LENGTH, WIDTH), (0.0, WIDTH));
    line(s, (0.0, WIDTH), (0.0, 0.0));
    line(s, (LENGTH / 2.0, 0.0), (LENGTH / 2.0, WIDTH));
    s.ground_arc(
        (LENGTH / 2.0, HALF_WIDTH),
        CENTRE_RADIUS,
        0.0,
        TAU,
        40,
        w,
        LINE,
    );
    for end in 0..2 {
        // The arcs are drawn in end-local angles: 0 points away from the
        // end line, along the court. Mirror them for the other end.
        let arc = |s: &mut Scene, u: f32, r: f32, a0: f32, a1: f32, steps: usize| {
            for i in 0..steps {
                let at = |j: usize| {
                    let a = a0 + (a1 - a0) * j as f32 / steps as f32;
                    local(end, u + r * a.cos(), r * a.sin())
                };
                s.ground_line(at(i), at(i + 1), w, LINE);
            }
        };
        // The key.
        line(
            s,
            local(end, 0.0, -KEY_HALF_WIDTH),
            local(end, KEY_DEPTH, -KEY_HALF_WIDTH),
        );
        line(
            s,
            local(end, KEY_DEPTH, -KEY_HALF_WIDTH),
            local(end, KEY_DEPTH, KEY_HALF_WIDTH),
        );
        line(
            s,
            local(end, KEY_DEPTH, KEY_HALF_WIDTH),
            local(end, 0.0, KEY_HALF_WIDTH),
        );
        // Free-throw circle: solid towards midcourt, dashed inside the key.
        arc(s, KEY_DEPTH, FT_RADIUS, -FRAC_PI_2, FRAC_PI_2, 16);
        for i in (0..16).step_by(2) {
            let a0 = FRAC_PI_2 + PI * i as f32 / 16.0;
            arc(s, KEY_DEPTH, FT_RADIUS, a0, a0 + PI / 16.0, 1);
        }
        // Three-point line: the corner lines, then the arc round the hoop.
        line(
            s,
            local(end, 0.0, -CORNER_X),
            local(end, CORNER_Y, -CORNER_X),
        );
        line(s, local(end, 0.0, CORNER_X), local(end, CORNER_Y, CORNER_X));
        let reach = CORNER_Y - HOOP.y;
        let half = CORNER_X.atan2(reach);
        arc(s, HOOP.y, THREE_RADIUS, -half, half, 36);
        // No-charge semicircle, joined to the board.
        arc(s, HOOP.y, NO_CHARGE_RADIUS, -FRAC_PI_2, FRAC_PI_2, 14);
        line(
            s,
            local(end, HOOP.y, -NO_CHARGE_RADIUS),
            local(end, 1.2, -NO_CHARGE_RADIUS),
        );
        line(
            s,
            local(end, HOOP.y, NO_CHARGE_RADIUS),
            local(end, 1.2, NO_CHARGE_RADIUS),
        );
    }
}

/// Height of the rim above the floor.
pub(crate) const RIM_Z: f32 = 3.05;

/// Both baskets: padded stanchion, arm, glass board, rim and net.
pub(crate) fn baskets(s: &mut Scene) {
    baskets_with(s, [0.0, 0.0]);
}

/// The baskets with their nets pulled by a ball that has just gone through:
/// `pull[end]` is 0 at rest and up to 1 at the most.
pub(crate) fn baskets_with(s: &mut Scene, pull: [f32; 2]) {
    let grey = Color32::from_rgb(70, 70, 76);
    let glass = Color32::from_rgb(214, 228, 240);
    let pad = Color32::from_rgb(40, 52, 120);
    for end in 0..2 {
        let sg = if end == 0 { 1.0 } else { -1.0 };
        let base = if end == 0 { 0.0 } else { LENGTH };
        let at = |d: f32| base + sg * d;
        // Glass board, 1.8 x 1.05 m, 1.2 m from the end line.
        s.block((at(1.225), HALF_WIDTH), 2.9, 0.025, 0.9, 1.05, glass);
        // Padded post behind the end line, and the arm to the board.
        s.block((at(-1.2), HALF_WIDTH), 0.0, 0.3, 0.3, 2.6, pad);
        s.block((at(-1.2), HALF_WIDTH), 2.6, 0.12, 0.12, 0.9, grey);
        s.block((at(0.05), HALF_WIDTH), 3.3, 1.25, 0.06, 0.1, grey);
        // Rim: a flat ring, and a net hanging from it.
        let (cx, cy) = (at(HOOP.y), HALF_WIDTH);
        const N: usize = 16;
        let ring = |r: f32, z: f32, i: usize| {
            let a = TAU * i as f32 / N as f32;
            v3(cx + r * a.cos(), cy + r * a.sin(), z)
        };
        for i in 0..N {
            let (o0, o1) = (ring(0.245, RIM_Z, i), ring(0.245, RIM_Z, i + 1));
            let (i0, i1) = (ring(0.205, RIM_Z, i), ring(0.205, RIM_Z, i + 1));
            s.solid_quad([o0, o1, i1, i0], RIM, true);
            let net = Color32::from_rgba_unmultiplied(250, 250, 250, 120);
            if i % 2 == 0 {
                // The cords stretch and flare for a moment as the ball goes down them.
                let (nr, nz) = (0.13 + 0.07 * pull[end], RIM_Z - 0.4 - 0.16 * pull[end]);
                let (l0, l1) = (ring(nr, nz, i), ring(nr, nz, i + 2));
                s.solid_quad([i0, ring(0.205, RIM_Z, i + 2), l1, l0], net, true);
            }
        }
        s.solid_quad(
            [
                v3(at(1.35), cy - 0.04, RIM_Z - 0.01),
                v3(at(1.35), cy + 0.04, RIM_Z - 0.01),
                v3(at(1.205), cy + 0.04, RIM_Z - 0.01),
                v3(at(1.205), cy - 0.04, RIM_Z - 0.01),
            ],
            RIM,
            true,
        );
    }
}

/// A ring on the floor round a player: the ball carrier, or the one picked.
pub(crate) fn floor_ring(s: &mut Scene, at: (f32, f32), r: f32, width: f32, color: Color32) {
    s.ground_arc(at, r, 0.0, TAU, 24, width, color);
}

pub(crate) const BALL_RADIUS: f32 = 0.13;

/// The ball and its shadow, `height` metres above the floor under it.
pub(crate) fn ball(s: &mut Scene, at: (f32, f32), height: f32) {
    // The shadow spreads and fades as the ball goes up.
    let up = height.clamp(0.0, 4.0);
    let alpha = (90.0 - up * 12.0).max(30.0) as u8;
    let r = BALL_RADIUS * (1.0 + up * 0.15);
    s.ground_disc(at, r * 1.3, r * 1.3, Color32::from_black_alpha(alpha));
    s.sphere(v3(at.0, at.1, height.max(BALL_RADIUS)), BALL_RADIUS, BALL);
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::vec2;

    fn rect() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), vec2(1000.0, 560.0))
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn target_is_in_the_middle() {
        let cam = Camera::new(v3(3.0, -10.0, 8.0), v3(14.0, 7.0, 0.0));
        let (p, depth) = cam.project(cam.target, rect()).unwrap();
        assert!(near(p.x, rect().center().x) && near(p.y, rect().center().y));
        assert!(near(depth, (cam.target - cam.eye).len()));
    }

    #[test]
    fn right_is_right_and_up_is_up() {
        let cam = Camera::new(v3(14.0, -14.0, 11.0), v3(14.0, 6.0, 0.0));
        let mid = cam.project(v3(14.0, 6.0, 0.0), rect()).unwrap().0;
        let east = cam.project(v3(18.0, 6.0, 0.0), rect()).unwrap().0;
        let high = cam.project(v3(14.0, 6.0, 2.0), rect()).unwrap().0;
        assert!(east.x > mid.x && near(east.y, mid.y));
        assert!(high.y < mid.y && near(high.x, mid.x));
    }

    #[test]
    fn behind_the_camera_is_not_projected() {
        let cam = Camera::new(v3(0.0, -10.0, 5.0), v3(0.0, 0.0, 0.0));
        assert!(cam.project(v3(0.0, -20.0, 5.0), rect()).is_none());
    }

    #[test]
    fn broadcast_near_sideline_is_below_far_sideline() {
        let mut cam = Cam3::default();
        let ball = v3(14.0, 7.5, 1.0);
        let c = cam.update(ball, 0.016);
        let near_side = c.project(v3(14.0, 0.0, 0.0), rect()).unwrap().0;
        let far_side = c.project(v3(14.0, WIDTH, 0.0), rect()).unwrap().0;
        assert!(near_side.y > far_side.y);
        // The near sideline is drawn wider, as it is nearer.
        let a = c.project(v3(10.0, 0.0, 0.0), rect()).unwrap().0;
        let b = c.project(v3(18.0, 0.0, 0.0), rect()).unwrap().0;
        let a2 = c.project(v3(10.0, WIDTH, 0.0), rect()).unwrap().0;
        let b2 = c.project(v3(18.0, WIDTH, 0.0), rect()).unwrap().0;
        assert!((b.x - a.x) > (b2.x - a2.x));
        // The ball at midcourt puts the centre of the court mid-picture.
        let mid = c.project(v3(14.0, 7.5, 0.0), rect()).unwrap().0;
        assert!(rect().contains(mid));
        assert!((mid.x - rect().center().x).abs() < 1.0);
    }

    #[test]
    fn top_down_keeps_the_court_in_view() {
        let mut cam = Cam3 {
            preset: Preset::Top,
            ..Default::default()
        };
        let c = cam.update(v3(14.0, 7.5, 1.0), 0.016);
        for (x, y) in [(0.0, 0.0), (LENGTH, 0.0), (LENGTH, WIDTH), (0.0, WIDTH)] {
            let p = c.project(v3(x, y, 0.0), rect()).unwrap().0;
            assert!(rect().contains(p), "{x},{y} -> {p:?}");
        }
        let lo = c.project(v3(2.0, 2.0, 0.0), rect()).unwrap().0;
        let hi = c.project(v3(4.0, 2.0, 0.0), rect()).unwrap().0;
        assert!(hi.x > lo.x);
    }

    #[test]
    fn clipping_keeps_what_is_in_front() {
        // One corner behind the near plane: a quad, every corner in front.
        let p = clip_near([v3(0.0, 0.0, 2.0), v3(1.0, 0.0, 2.0), v3(0.5, 0.0, -2.0)]);
        assert_eq!(p.n, 4);
        assert!(p.points().iter().all(|q| q.z >= NEAR - 1e-6));
        // Two corners behind: a triangle.
        let p = clip_near([v3(0.0, 0.0, 2.0), v3(1.0, 0.0, -1.0), v3(0.5, 0.0, -2.0)]);
        assert_eq!(p.n, 3);
        assert!(p.points().iter().all(|q| q.z >= NEAR - 1e-6));
        // All in front: untouched. All behind: gone.
        let t = [v3(0.0, 0.0, 1.0), v3(1.0, 0.0, 1.0), v3(0.0, 1.0, 1.0)];
        let p = clip_near(t);
        assert_eq!(p.n, 3);
        assert_eq!(p.pts[..3], t);
        assert_eq!(
            clip_near([v3(0.0, 0.0, -1.0), v3(1.0, 0.0, -1.0), v3(0.0, 1.0, -1.0)]).n,
            0
        );
    }

    #[test]
    fn clipped_floor_stays_finite_and_in_front() {
        // The camera hovers over the floor: the floor reaches behind it.
        let cam = Camera::new(v3(14.0, 2.0, 3.0), v3(14.0, 20.0, 0.0));
        let mut s = Scene::new(cam.eye);
        s.ground_rect(-30.0, -30.0, 60.0, 40.0, FLOOR);
        let mesh = s.mesh(&cam, rect());
        assert!(!mesh.is_empty());
        assert!(
            mesh.vertices
                .iter()
                .all(|v| v.pos.x.is_finite() && v.pos.y.is_finite())
        );
    }

    #[test]
    fn solids_are_culled_and_sorted_far_to_near() {
        let eye = v3(0.0, -10.0, 2.0);
        let cam = Camera::new(eye, v3(0.0, 0.0, 2.0));
        let mut s = Scene::new(eye);
        // A box seen from the front shows at most three faces.
        s.block((0.0, 5.0), 0.0, 1.0, 1.0, 1.0, FLOOR);
        assert!(s.solids.len() <= 6, "{}", s.solids.len());
        assert!(!s.solids.is_empty());
        // Nearer boxes come later in the mesh.
        s.block((0.0, 0.0), 0.0, 1.0, 1.0, 1.0, PAINT);
        s.block((0.0, 20.0), 0.0, 1.0, 1.0, 1.0, LINE);
        s.mesh(&cam, rect());
        let d: Vec<f32> = s.solids.iter().map(|t| t.depth).collect();
        assert!(d.windows(2).all(|w| w[0] >= w[1]));
    }

    #[test]
    fn cuboid_faces_point_outwards() {
        let eye = v3(0.0, -10.0, 0.0);
        let mut s = Scene::new(eye);
        s.cuboid(
            v3(0.0, 0.0, 0.0),
            v3(1.0, 0.0, 0.0),
            v3(0.0, 1.0, 0.0),
            v3(0.0, 0.0, 1.0),
            FLOOR,
        );
        // Looking along +y at a cube: only the face at y = -1 is seen.
        assert_eq!(s.solids.len(), 2);
        assert!(s.solids.iter().all(|t| t.v.iter().all(|p| near(p.y, -1.0))));
    }

    #[test]
    fn lines_are_thin_quads_centred_on_the_line() {
        let mut s = Scene::new(v3(0.0, 0.0, 5.0));
        s.ground_line((0.0, 0.0), (10.0, 0.0), 0.1, LINE);
        assert_eq!(s.ground.len(), 2);
        let ys: Vec<f32> = s
            .ground
            .iter()
            .flat_map(|t| t.v.iter().map(|p| p.y))
            .collect();
        assert!(ys.iter().all(|y| near(y.abs(), 0.05)));
        let xs: Vec<f32> = s
            .ground
            .iter()
            .flat_map(|t| t.v.iter().map(|p| p.x))
            .collect();
        assert!(xs.iter().any(|x| near(*x, -0.05)) && xs.iter().any(|x| near(*x, 10.05)));
    }

    #[test]
    fn court_is_mirrored_end_to_end() {
        // The three-point arc and the key stand at both ends alike.
        let (a, b) = (local(0, 5.8, 2.45), local(1, 5.8, 2.45));
        assert!(near(a.0, 5.8) && near(b.0, LENGTH - 5.8));
        assert!(near(a.1, HALF_WIDTH + 2.45) && near(b.1, HALF_WIDTH - 2.45));
        let mut s = Scene::new(v3(14.0, -14.0, 11.0));
        court(&mut s);
        assert!(s.ground.len() > 200);
        // Every ground point lies on or around the court.
        for t in &s.ground {
            for p in &t.v {
                assert!(p.x > -3.0 && p.x < LENGTH + 3.0 && p.y > -3.0 && p.y < WIDTH + 3.0);
                assert!(near(p.z, 0.0));
            }
        }
    }

    #[test]
    fn camera_follows_the_ball_smoothly() {
        let mut cam = Cam3::default();
        cam.update(v3(8.0, 7.0, 1.0), 0.016);
        let a = cam.follow;
        cam.update(v3(12.0, 7.0, 1.0), 0.016);
        // It moved towards the ball, but did not jump there.
        assert!(cam.follow > a && cam.follow < 12.0);
        for _ in 0..600 {
            cam.update(v3(12.0, 7.0, 1.0), 0.016);
        }
        assert!(near(cam.follow, 12.0));
        assert!(!cam.settling(v3(12.0, 7.0, 1.0)));
    }

    /// Building, sorting and meshing a whole frame: floor, baskets, ten
    /// figures and the ball. Run with `--nocapture` to see the figures.
    #[test]
    fn a_frame_is_cheap() {
        use basketman_core::court::Vec2;
        use basketman_core::player::{Body, Hand};
        let body = Body {
            height_cm: 198.0,
            weight_kg: 90.0,
            wingspan_cm: 205.0,
            standing_reach_cm: 260.0,
            shoulder_width_cm: 46.0,
            hand_length_cm: 20.0,
            hand: Hand::Right,
        };
        let mut cam3 = Cam3::default();
        let frame = |cam3: &mut Cam3, t: f32| {
            let cam = cam3.update(v3(10.0 + t, 7.5, 1.0), 0.016);
            let mut s = Scene::new(cam.eye);
            court(&mut s);
            baskets(&mut s);
            for i in 0..10 {
                let at = emit_figure(
                    &mut s,
                    &FigureSpec {
                        body: &body,
                        stats: gait::Stats::AVERAGE,
                        id: i as u32,
                        pos: Vec2 {
                            x: 6.0 + i as f32 * 1.5,
                            y: 3.0 + (i % 5) as f32 * 2.0,
                        },
                        gait: gait::Gait {
                            vel: Vec2 { x: 3.0, y: 0.0 },
                            face: Vec2 { x: 1.0, y: 0.0 },
                            stance: if i > 5 { 1.0 } else { 0.0 },
                            phase: i as f32 + t * 6.0,
                            dribble: t * 6.0,
                        },
                        team: PAINT,
                        holds_ball: i == 3,
                        act: None,
                    },
                );
                if let Some(at) = at {
                    ball(&mut s, (at.x, at.y), at.z);
                }
            }
            let (g, solids) = (s.ground.len(), s.solids.len());
            let mesh = s.mesh(&cam, rect());
            (g, solids, mesh.indices.len() / 3)
        };
        let _ = frame(&mut cam3, 0.0);
        let n = 500;
        let start = std::time::Instant::now();
        let mut last = (0, 0, 0);
        for i in 0..n {
            last = frame(&mut cam3, i as f32 * 0.02);
        }
        let each = start.elapsed().as_secs_f64() * 1000.0 / n as f64;
        println!(
            "frame: {each:.3} ms; ground {}, solids kept {}, mesh triangles {}",
            last.0, last.1, last.2
        );
        assert!(each < 25.0, "{each} ms");
    }
}
