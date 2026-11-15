//! The hall around the court: floor and key colours, stands with a crowd,
//! sponsor boards and the centre mark, all from one club's `Arena`.
//!
//! `ArenaView` is a snapshot of what the scene needs (the look, how full the
//! hall is, who holds which board). The match screen publishes it for the
//! fixture on show with `ArenaView::publish`; `paint_3d` calls `begin` before
//! the court and `Hall::decals` after it. Without a published view a plain
//! default hall is drawn.
//!
//! Court coordinates: x along the court 0..28, y across it 0..15. The scorer's
//! table is on the far sideline (y = 15 side) as the broadcast camera sees it,
//! so its front and the boards behind it can be read. "West" is the end at
//! x = 0, "east" the end at x = 28.
//!
//! The crowd is one block per group of fans. Which seats are taken is a
//! deterministic function of the arena and the attendance: the nearest rows
//! and the middle of each stand fill first, with a hash to scatter them.

use std::sync::Arc;

use basketman_core::arena::{ArenaLook, CentreMark};
use basketman_core::league::World;
use basketman_core::sponsor::{SLOTS, SlotKind, slot_kind};
use eframe::egui::{Color32, Context, Id, Pos2, pos2};

use super::boards::{self, Atlas, BoardKey, UvRect};
use super::{Camera, NEAR, Scene, Tri, Vec3, v3};

type Rgb = [u8; 3];

/// What the scene needs to know about the hall.
#[derive(Clone, Debug)]
pub(crate) struct ArenaView {
    /// The home club's name and its arena's.
    pub club: String,
    pub name: String,
    pub look: ArenaLook,
    pub capacity: u32,
    pub condition: f32,
    pub attendance: u32,
    pub seed: u64,
    /// The in-game sponsor holding each of the twenty slots.
    pub slots: Vec<Option<usize>>,
    /// Name, background and lettering of the league's sponsors.
    pub sponsors: Arc<Vec<(String, Rgb, Rgb)>>,
}

fn view_id() -> Id {
    Id::NULL.with("basketman-arena-view")
}

impl ArenaView {
    /// The hall of fixture `fixture`'s home club, as full as it was.
    pub fn of(w: &World, fixture: usize) -> ArenaView {
        let home = w.fixtures[fixture].home;
        Self::of_club(w, home, w.attendance_at(fixture))
    }

    /// The hall of `home` with `attendance` in it, for the scenes that
    /// are not a game (the office, the practice floor).
    pub fn of_club(w: &World, home: basketman_core::league::TeamId, attendance: u32) -> ArenaView {
        let a = w.arena(home);
        let sponsors: Vec<(String, Rgb, Rgb)> = w
            .econ
            .sponsors
            .iter()
            .map(|s| (s.name.clone(), s.bg, s.fg))
            .collect();
        ArenaView {
            club: w.teams[home].name.clone(),
            name: a.name.clone(),
            look: a.look.clone(),
            capacity: a.capacity,
            condition: a.condition,
            attendance,
            seed: (home as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ a.capacity as u64,
            slots: (0..SLOTS)
                .map(|s| w.slot_deal(home, s).map(|d| d.sponsor))
                .collect(),
            sponsors: Arc::new(sponsors),
        }
    }

    /// A plain hall, for a screen that has not said which one.
    pub fn generic() -> ArenaView {
        ArenaView {
            club: String::new(),
            name: "Arena".into(),
            look: ArenaLook {
                two_tiers: false,
                behind_baskets: false,
                floor: [206, 158, 104],
                paint: [176, 70, 62],
                seats: [60, 80, 130],
                trim: [230, 230, 235],
                mark: CentreMark::Ring,
            },
            capacity: 5000,
            condition: 0.7,
            attendance: 3000,
            seed: 7,
            slots: vec![None; SLOTS],
            sponsors: Arc::new(Vec::new()),
        }
    }

    /// Make this the hall `paint_3d` draws.
    pub fn publish(self, ctx: &Context) {
        ctx.data_mut(|d| d.insert_temp(view_id(), Arc::new(self)));
    }
}

fn current(ctx: &Context) -> Arc<ArenaView> {
    ctx.data(|d| d.get_temp::<Arc<ArenaView>>(view_id()))
        .unwrap_or_else(|| Arc::new(ArenaView::generic()))
}

// ---------------------------------------------------------------- colours

fn lit(c: Rgb, k: f32) -> Color32 {
    let f = |v: u8| (v as f32 * k).clamp(0.0, 255.0) as u8;
    Color32::from_rgb(f(c[0]), f(c[1]), f(c[2]))
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let f = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t) as u8;
    [f(0), f(1), f(2)]
}

fn hash(a: u64, b: u64) -> u64 {
    let mut x = a ^ b.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn unit(h: u64) -> f32 {
    (h >> 40) as f32 / (1u64 << 24) as f32
}

const SHIRTS: [Rgb; 8] = [
    [196, 196, 200],
    [60, 70, 110],
    [150, 50, 50],
    [220, 220, 210],
    [60, 120, 90],
    [200, 160, 60],
    [90, 90, 100],
    [160, 90, 140],
];
const SKINS: [Rgb; 4] = [
    [236, 190, 150],
    [205, 150, 110],
    [150, 100, 70],
    [105, 70, 50],
];

// ------------------------------------------------------------------ stands

#[derive(Copy, Clone, Debug, PartialEq)]
enum Side {
    Near,
    Far,
    West,
    East,
}

/// Where a stand's front edge is, and the way it faces.
fn at(side: Side, a: f32, o: f32, z: f32) -> Vec3 {
    match side {
        Side::Near => v3(a, -3.6 - o, z),
        Side::Far => v3(a, 18.6 + o, z),
        Side::West => v3(-4.6 - o, a, z),
        Side::East => v3(32.6 + o, a, z),
    }
}

fn toward(side: Side) -> Vec3 {
    match side {
        Side::Near => v3(0.0, 1.0, 0.0),
        Side::Far => v3(0.0, -1.0, 0.0),
        Side::West => v3(1.0, 0.0, 0.0),
        Side::East => v3(-1.0, 0.0, 0.0),
    }
}

/// Is the eye outside this stand, on the far side of it from the court?
fn eye_outside(side: Side, eye: Vec3) -> bool {
    match side {
        Side::Near => eye.y < -2.0,
        Side::Far => eye.y > 17.0,
        Side::West => eye.x < -2.0,
        Side::East => eye.x > 30.0,
    }
}

const PITCH: f32 = 0.85;
const CELL: f32 = 1.5;

struct Stand {
    side: Side,
    a0: f32,
    a1: f32,
    rows: usize,
    /// First row of the upper tier, if there is one.
    upper: Option<usize>,
}

impl Stand {
    /// Distance outwards of the front of row `r`, and the height of its tread.
    fn row(&self, r: usize) -> (f32, f32) {
        let mut o = r as f32 * PITCH;
        let mut z = 0.5;
        match self.upper {
            Some(u) if r >= u => {
                o += 1.8;
                z += u as f32 * 0.5 + 1.4 + (r - u) as f32 * 0.62;
            }
            _ => z += r as f32 * 0.5,
        }
        (o, z)
    }

    fn cells(&self) -> usize {
        (((self.a1 - self.a0) / CELL) as usize).max(1)
    }
}

fn stands(view: &ArenaView) -> Vec<Stand> {
    let rows = (4 + view.capacity as usize / 900).clamp(5, 14);
    let end_rows = rows.saturating_sub(2).max(4);
    let split = |n: usize| {
        view.look
            .two_tiers
            .then(|| (n * 55 / 100).max(2))
            .filter(|&u| u + 1 < n)
    };
    let mut out = vec![
        Stand {
            side: Side::Near,
            a0: -3.0,
            a1: 31.0,
            rows,
            upper: split(rows),
        },
        Stand {
            side: Side::Far,
            a0: -3.0,
            a1: 31.0,
            rows,
            upper: split(rows),
        },
    ];
    if view.look.behind_baskets {
        for side in [Side::West, Side::East] {
            out.push(Stand {
                side,
                a0: -1.0,
                a1: 16.0,
                rows: end_rows,
                upper: split(end_rows),
            });
        }
    }
    out
}

fn quad(scene: &mut Scene, pts: [Vec3; 4], want: Vec3, color: Color32, key: f32) {
    let n = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
    let p = if n.dot(want) >= 0.0 {
        pts
    } else {
        [pts[3], pts[2], pts[1], pts[0]]
    };
    scene.solid_quad_keyed(p, color, false, key);
}

fn dist2(a: Vec3, b: Vec3) -> f32 {
    let d = a - b;
    d.dot(d)
}

/// Squared distance from the eye to the nearest point of a row, the key its
/// pieces are sorted on.
fn row_key(s: &Stand, eye: Vec3, o: f32, z: f32) -> f32 {
    let a = match s.side {
        Side::Near | Side::Far => eye.x,
        Side::West | Side::East => eye.y,
    }
    .clamp(s.a0, s.a1);
    dist2(eye, at(s.side, a, o + PITCH / 2.0, z))
}

// -------------------------------------------------------------------- hall

/// What `begin` hands on: what is needed once the court is down.
pub(crate) struct Hall {
    /// Behind the picture.
    pub backdrop: Color32,
    atlas: Arc<Atlas>,
    view: Arc<ArenaView>,
    keys: Vec<BoardKey>,
    /// The advertisers' manifest, for boards drawn as geometry.
    loaded: Arc<boards::Loaded>,
    paint: Color32,
    tone: f32,
}

/// Colours of the floor, the stands with their crowd and every board, around
/// the court that `court` then draws. Stands the eye is behind are left out,
/// so the near stand never hides the court from the broadcast camera.
pub(crate) fn begin(scene: &mut Scene, ctx: &Context, cam: &Camera) -> Hall {
    boards::start_loading(ctx);
    let view = current(ctx);
    let loaded = boards::loaded(ctx);
    let stamp = boards::stamp(&view.sponsors, &loaded);
    let atlas = boards::atlas(ctx, stamp, || boards::Source {
        sponsors: view.sponsors.to_vec(),
        manifest: loaded.manifest.clone(),
        images: loaded.images.clone(),
        squares: loaded.squares.clone(),
    });
    scene.texture = Some((atlas.texture.id(), atlas.white));

    let cond = view.condition.clamp(0.0, 1.0);
    let tone = 0.5 + 0.5 * cond;
    // A worn hall is greyer as well as darker.
    let grey = [150, 140, 130];
    let floor = mix(view.look.floor, grey, (1.0 - cond) * 0.35);
    scene.floor = lit(floor, 0.8 + 0.2 * cond);
    scene.runoff = lit(floor, 0.62 + 0.14 * cond);
    scene.paint = lit(
        mix(view.look.paint, grey, (1.0 - cond) * 0.3),
        0.8 + 0.2 * cond,
    );
    scene.ground_rect(-24.0, -24.0, 52.0, 39.0, lit([66, 68, 78], tone));

    let keys: Vec<BoardKey> = (0..SLOTS)
        .map(|s| {
            if let Some(i) = loaded.manifest.resolve(&view.club, &view.name, s) {
                BoardKey::Real(i)
            } else if let Some(i) = view.slots[s].filter(|&i| i < view.sponsors.len()) {
                BoardKey::Sponsor(i)
            } else {
                BoardKey::League
            }
        })
        .collect();
    let hall = Hall {
        backdrop: lit([34, 40, 56], 0.45 + 0.55 * cond),
        atlas,
        view,
        keys,
        loaded,
        paint: scene.paint,
        tone,
    };
    hall.stands(scene, cam);
    hall.boards(scene);
    hall
}

impl Hall {
    /// The board texture, to keep alive for as long as a cached picture
    /// of the hall refers to it.
    pub fn atlas(&self) -> Arc<Atlas> {
        self.atlas.clone()
    }

    /// Identifies the board texture a picture of the hall was made with.
    pub fn stamp(ctx: &Context) -> u64 {
        let view = current(ctx);
        boards::stamp(&view.sponsors, &boards::loaded(ctx))
    }

    fn stands(&self, scene: &mut Scene, cam: &Camera) {
        let v = &*self.view;
        let eye = cam.eye;
        let basis = cam.basis();
        let all = stands(v);
        let tone = self.tone;
        let cond = v.condition.clamp(0.0, 1.0);
        // How taken each seat is: a hash, leaning towards the front rows and
        // the middle of the stand. The fullest `attendance / capacity`
        // share of all the seats is occupied.
        let mut ranks: Vec<f32> = Vec::new();
        let score = |si: usize, s: &Stand, r: usize, c: usize| {
            let h = unit(hash(v.seed ^ si as u64, (r * 64 + c) as u64));
            let mid = (c as f32 + 0.5) / s.cells() as f32;
            0.55 * h + 0.30 * r as f32 / s.rows as f32 + 0.15 * (mid - 0.5).abs() * 2.0
        };
        for (si, s) in all.iter().enumerate() {
            for r in 0..s.rows {
                for c in 0..s.cells() {
                    ranks.push(score(si, s, r, c));
                }
            }
        }
        ranks.sort_by(|a, b| a.total_cmp(b));
        let fill = (v.attendance as f32 / v.capacity.max(1) as f32).clamp(0.0, 1.0);
        let cut = ranks
            .get(((ranks.len() as f32 * fill) as usize).min(ranks.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0.0);
        let cut = if fill >= 0.999 { f32::MAX } else { cut };

        let seat = v.look.seats;
        let trim = v.look.trim;
        let wall = lit([30, 32, 44], tone);
        let broken = (0.75 - cond).max(0.0) * 0.5;
        for (si, s) in all.iter().enumerate() {
            if eye_outside(s.side, eye) {
                continue;
            }
            let n = toward(s.side);
            let (a0, a1) = (s.a0, s.a1);
            // Behind the camera altogether?
            let mid = at(
                s.side,
                (a0 + a1) / 2.0,
                s.row(s.rows / 2).0,
                s.row(s.rows / 2).1,
            );
            if basis.to_cam(mid).z < -((a1 - a0).max(20.0)) {
                continue;
            }
            let cells = s.cells();
            let span = (a1 - a0) / cells as f32;
            // The barrier at the front.
            let k0 = row_key(s, eye, -0.2, 0.0);
            quad(
                scene,
                [
                    at(s.side, a0, 0.0, 0.0),
                    at(s.side, a1, 0.0, 0.0),
                    at(s.side, a1, 0.0, 0.55),
                    at(s.side, a0, 0.0, 0.55),
                ],
                n,
                lit(trim, tone * 0.9),
                k0 + 4.0,
            );
            for r in 0..s.rows {
                let (o, z) = s.row(r);
                let rise = if s.upper.is_some_and(|u| r >= u) {
                    0.62
                } else {
                    0.5
                };
                let key = row_key(s, eye, o, z);
                let stripe = if r % 2 == 0 { 1.0 } else { 0.9 };
                // A concourse step in front of the upper tier.
                if s.upper == Some(r) {
                    quad(
                        scene,
                        [
                            at(s.side, a0, o - 1.8, z - 1.4),
                            at(s.side, a1, o - 1.8, z - 1.4),
                            at(s.side, a1, o - 1.8, z - 0.2),
                            at(s.side, a0, o - 1.8, z - 0.2),
                        ],
                        n,
                        lit(trim, tone * 0.8),
                        key + 1.0,
                    );
                    quad(
                        scene,
                        [
                            at(s.side, a0, o - 1.8, z - 1.4),
                            at(s.side, a1, o - 1.8, z - 1.4),
                            at(s.side, a1, o, z - 1.4),
                            at(s.side, a0, o, z - 1.4),
                        ],
                        Vec3 {
                            x: 0.0,
                            y: 0.0,
                            z: 1.0,
                        },
                        lit([70, 72, 84], tone),
                        key + 0.5,
                    );
                }
                // The seats of the row: tread and the riser below it.
                quad(
                    scene,
                    [
                        at(s.side, a0, o, z),
                        at(s.side, a1, o, z),
                        at(s.side, a1, o + PITCH, z),
                        at(s.side, a0, o + PITCH, z),
                    ],
                    Vec3 {
                        x: 0.0,
                        y: 0.0,
                        z: 1.0,
                    },
                    lit(seat, tone * stripe),
                    key,
                );
                quad(
                    scene,
                    [
                        at(s.side, a0, o, z - rise),
                        at(s.side, a1, o, z - rise),
                        at(s.side, a1, o, z),
                        at(s.side, a0, o, z),
                    ],
                    n,
                    lit(seat, tone * stripe * 0.72),
                    key,
                );
                for c in 0..cells {
                    let ac = a0 + (c as f32 + 0.5) * span;
                    let centre = at(s.side, ac, o + 0.45, z + 0.5);
                    let cc = basis.to_cam(centre);
                    // Off to the side of the picture or behind the camera.
                    if cc.z < NEAR || cc.x.abs() > cc.z * (0.62 / cam.zoom.max(0.3)) + 3.0 {
                        continue;
                    }
                    let h = hash(v.seed ^ si as u64, (r * 64 + c) as u64);
                    let taken = score(si, s, r, c) < cut;
                    if !taken {
                        if unit(h >> 7) < broken {
                            // A seat that is missing or smashed.
                            let (o0, o1) = (o + 0.15, o + 0.7);
                            quad(
                                scene,
                                [
                                    at(s.side, ac - 0.4, o0, z + 0.01),
                                    at(s.side, ac + 0.4, o0, z + 0.01),
                                    at(s.side, ac + 0.4, o1, z + 0.01),
                                    at(s.side, ac - 0.4, o1, z + 0.01),
                                ],
                                Vec3 {
                                    x: 0.0,
                                    y: 0.0,
                                    z: 1.0,
                                },
                                Color32::from_rgb(150, 142, 130),
                                key,
                            );
                        }
                        continue;
                    }
                    let u = unit(h >> 9);
                    let shirt = if u < 0.3 {
                        seat
                    } else if u < 0.45 {
                        trim
                    } else {
                        SHIRTS[(h >> 3) as usize % SHIRTS.len()]
                    };
                    let skin = SKINS[(h >> 13) as usize % SKINS.len()];
                    let dull = 0.7 + 0.3 * cond;
                    let (o0, o1) = (o + 0.15, o + 0.7);
                    // Torso: its face to the court, and its top.
                    quad(
                        scene,
                        [
                            at(s.side, ac - 0.4, o0, z),
                            at(s.side, ac + 0.4, o0, z),
                            at(s.side, ac + 0.4, o0, z + 0.6),
                            at(s.side, ac - 0.4, o0, z + 0.6),
                        ],
                        n,
                        lit(shirt, dull * 1.3),
                        key,
                    );
                    quad(
                        scene,
                        [
                            at(s.side, ac - 0.4, o0, z + 0.6),
                            at(s.side, ac + 0.4, o0, z + 0.6),
                            at(s.side, ac + 0.4, o1, z + 0.6),
                            at(s.side, ac - 0.4, o1, z + 0.6),
                        ],
                        Vec3 {
                            x: 0.0,
                            y: 0.0,
                            z: 1.0,
                        },
                        lit(shirt, dull * 1.1),
                        key,
                    );
                    quad(
                        scene,
                        [
                            at(s.side, ac - 0.18, o0 + 0.1, z + 0.6),
                            at(s.side, ac + 0.18, o0 + 0.1, z + 0.6),
                            at(s.side, ac + 0.18, o0 + 0.1, z + 0.98),
                            at(s.side, ac - 0.18, o0 + 0.1, z + 0.98),
                        ],
                        n,
                        lit(skin, dull),
                        key,
                    );
                }
            }
            // The wall at the back, with a stripe of the club's colour.
            let (o, z) = s.row(s.rows - 1);
            let (ob, zt) = (o + PITCH, z + 3.0);
            let key = row_key(s, eye, ob, z) + 10.0;
            quad(
                scene,
                [
                    at(s.side, a0, ob, 0.0),
                    at(s.side, a1, ob, 0.0),
                    at(s.side, a1, ob, zt),
                    at(s.side, a0, ob, zt),
                ],
                n,
                wall,
                key,
            );
            quad(
                scene,
                [
                    at(s.side, a0, ob, z + 0.8),
                    at(s.side, a1, ob, z + 0.8),
                    at(s.side, a1, ob, z + 1.2),
                    at(s.side, a0, ob, z + 1.2),
                ],
                n,
                lit(seat, tone * 0.8),
                key - 1.0,
            );
        }
    }

    fn boards(&self, scene: &mut Scene) {
        let eye = scene.eye;
        let tint_v = (190.0 + 65.0 * self.view.condition.clamp(0.0, 1.0)) as u8;
        let tint = Color32::from_rgb(tint_v, tint_v, tint_v);
        let up = v3(0.0, 0.0, 1.0);
        let x_dir = v3(1.0, 0.0, 0.0);
        let y_dir = v3(0.0, 1.0, 0.0);
        let (bw, bh) = (3.6, 0.9);
        let side_x = [2.5, 6.3, 10.1, 17.9, 21.7, 25.5];
        for slot in 0..SLOTS {
            let cells = self.atlas.cells(self.keys[slot]);
            // A real advertiser's board is drawn as geometry when its
            // artwork came as triangles, and as a picture otherwise.
            let art = self.art(slot, false);
            let art_sq = self.art(slot, true);
            let plate = |scene: &mut Scene,
                         eye: Vec3,
                         uv: UvRect,
                         bl: Vec3,
                         u: Vec3,
                         up: Vec3,
                         w: f32,
                         h: f32,
                         tint: Color32| {
                let square = (w - h).abs() < 1e-3;
                match if square { &art_sq } else { &art } {
                    Some((a, bg, ink)) => {
                        vector_plate(scene, eye, a, *bg, *ink, bl, u, up, w, h, tint)
                    }
                    None => plate(scene, eye, uv, bl, u, up, w, h, tint),
                }
            };
            match slot_kind(slot) {
                SlotKind::TableSide(k) => {
                    // Along the far sideline, facing the court (towards -y).
                    let bl = v3(side_x[k as usize] - bw / 2.0, 17.3, 0.0);
                    plate(scene, eye, cells.wide, bl, x_dir, up, bw, bh, tint);
                }
                SlotKind::FarSide(k) => {
                    // The near hoarding is printed on both faces.
                    let cx = side_x[k as usize];
                    let front = v3(cx + bw / 2.0, -2.2, 0.0);
                    plate(scene, eye, cells.wide, front, -x_dir, up, bw, bh, tint);
                    let back = v3(cx - bw / 2.0, -2.24, 0.0);
                    plate(scene, eye, cells.wide, back, x_dir, up, bw, bh, tint);
                }
                SlotKind::BaselineA(n) => {
                    let cy = 7.5 + if n == 0 { -2.2 } else { 2.2 };
                    let front = v3(-2.6, cy - bw / 2.0, 0.0);
                    plate(scene, eye, cells.wide, front, y_dir, up, bw, bh, tint);
                    let back = v3(-2.64, cy + bw / 2.0, 0.0);
                    plate(scene, eye, cells.wide, back, -y_dir, up, bw, bh, tint);
                }
                SlotKind::BaselineB(n) => {
                    let cy = 7.5 + if n == 0 { -2.2 } else { 2.2 };
                    let front = v3(30.6, cy + bw / 2.0, 0.0);
                    plate(scene, eye, cells.wide, front, -y_dir, up, bw, bh, tint);
                    let back = v3(30.64, cy - bw / 2.0, 0.0);
                    plate(scene, eye, cells.wide, back, y_dir, up, bw, bh, tint);
                }
                SlotKind::Padding(end) => {
                    // The court side of the padded post, at hip height.
                    let s = 0.56;
                    if end == 0 {
                        let bl = v3(-0.89, 7.5 - s / 2.0, 0.9);
                        plate(scene, eye, cells.square, bl, y_dir, up, s, s, tint);
                    } else {
                        let bl = v3(28.89, 7.5 + s / 2.0, 0.9);
                        plate(scene, eye, cells.square, bl, -y_dir, up, s, s, tint);
                    }
                }
                SlotKind::Table => {
                    // The scorer's table, on the far sideline.
                    scene.block(
                        (14.0, 16.2),
                        0.0,
                        2.4,
                        0.6,
                        0.78,
                        lit([52, 54, 64], self.tone),
                    );
                    let bl = v3(14.0 - 1.6, 15.59, 0.04);
                    plate(scene, eye, cells.wide, bl, x_dir, up, 3.2, 0.7, tint);
                }
                SlotKind::Centre => {}
            }
        }
    }

    /// The marks painted on the floor: the club's, and the centre board over
    /// it. Draw straight after the court, so the lines are under them.
    pub fn decals(&self, scene: &mut Scene) {
        let (cx, cy) = (14.0, 7.5);
        let c = self.paint;
        let at = |r: f32, a: f32| v3(cx + r * a.cos(), cy + r * a.sin(), 0.0);
        let ctr = v3(cx, cy, 0.0);
        use std::f32::consts::{PI, TAU};
        match self.view.look.mark {
            CentreMark::Ring => scene.ground_arc((cx, cy), 1.35, 0.0, TAU, 24, 0.28, c),
            CentreMark::Star => {
                for i in 0..10 {
                    let (a0, a1) = (
                        -PI / 2.0 + TAU * i as f32 / 10.0,
                        -PI / 2.0 + TAU * (i + 1) as f32 / 10.0,
                    );
                    let r = |i: usize| if i.is_multiple_of(2) { 1.55 } else { 0.65 };
                    scene.ground_tri(ctr, at(r(i), a0), at(r(i + 1), a1), c);
                }
            }
            CentreMark::Diamond => {
                let p: Vec<Vec3> = (0..4).map(|i| at(1.5, PI / 2.0 * i as f32)).collect();
                scene.ground_poly(&p, c);
            }
            CentreMark::Bar => {
                scene.ground_arc((cx, cy), 1.4, 0.0, TAU, 24, 0.22, c);
                scene.ground_rect(cx - 1.7, cy - 0.3, cx + 1.7, cy + 0.3, c);
            }
        }
        // The centre board: always drawn, the league's if unsold.
        let cells = self.atlas.cells(self.keys[16]);
        let (w, h) = (3.2, 0.8);
        let bl = v3(cx - w / 2.0, cy - h / 2.0, 0.0);
        match self.art(16, false) {
            Some((a, bg, ink)) => ground_vector(scene, &a, bg, ink, bl, w, h),
            None => ground_plate(scene, cells.wide, bl, w, h),
        }
    }
}

// ------------------------------------------------------------------ boards

const NU: usize = 8;
const NV: usize = 2;

/// A board standing in the scene, `w` wide along `u` and `h` up from `bl`,
/// printed with `uv`. It is cut into small quads so the affine texture
/// mapping of the mesh cannot bend the lettering. Only drawn from the side
/// the lettering reads correctly from.
#[allow(clippy::too_many_arguments)]
fn plate(
    scene: &mut Scene,
    eye: Vec3,
    uv: UvRect,
    bl: Vec3,
    u: Vec3,
    up: Vec3,
    w: f32,
    h: f32,
    tint: Color32,
) {
    let normal = u.cross(up);
    if normal.dot(eye - (bl + u * (w / 2.0) + up * (h / 2.0))) <= 0.0 {
        return;
    }
    let key = scene.depth(bl + u * (w / 2.0) + up * (h / 2.0)) - 0.05;
    for j in 0..NV {
        for i in 0..NU {
            let p = |a: usize, b: usize| {
                bl + u * (w * a as f32 / NU as f32) + up * (h * b as f32 / NV as f32)
            };
            let t = |a: usize, b: usize| {
                pos2(
                    uv[0] + (uv[2] - uv[0]) * a as f32 / NU as f32,
                    uv[3] - (uv[3] - uv[1]) * b as f32 / NV as f32,
                )
            };
            let (c00, c10, c11, c01) = (p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1));
            let (t00, t10, t11, t01) = (t(i, j), t(i + 1, j), t(i + 1, j + 1), t(i, j + 1));
            scene.solids.push(Tri {
                v: [c00, c10, c11],
                color: tint,
                depth: key,
                uv: Some([t00, t10, t11]),
            });
            scene.solids.push(Tri {
                v: [c00, c11, c01],
                color: tint,
                depth: key,
                uv: Some([t00, t11, t01]),
            });
        }
    }
}

/// A board lying on the floor, read along x with the top towards +y.
/// The vector artwork of a slot, with its background and ink colours, if a
/// real advertiser holds it and sent geometry.
type Art = (Arc<boards::VectorArt>, Color32, Color32);

impl Hall {
    fn art(&self, slot: usize, square: bool) -> Option<Art> {
        let BoardKey::Real(i) = self.keys[slot] else {
            return None;
        };
        let set = if square {
            &self.loaded.vector_squares
        } else {
            &self.loaded.vectors
        };
        let art = set.get(i)?.clone()?;
        let b = self.loaded.manifest.boards.get(i)?;
        let rgb = |c: [u8; 3]| Color32::from_rgb(c[0], c[1], c[2]);
        Some((
            art,
            rgb(b.bg.unwrap_or([255, 255, 255])),
            rgb(b.ink.unwrap_or([30, 30, 30])),
        ))
    }
}

/// Where the artwork sits on a board `w` by `h`: scaled to fit and centred.
/// Returns the scale and the offset of its bottom-left corner.
fn fit(art: &boards::VectorArt, w: f32, h: f32) -> (f32, f32, f32) {
    let k = (w / art.w).min(h);
    (k, (w - art.w * k) / 2.0, (h - k) / 2.0)
}

fn shade(c: Color32, tint: Color32) -> Color32 {
    let m = |a: u8, b: u8| (a as u16 * b as u16 / 255) as u8;
    Color32::from_rgb(m(c.r(), tint.r()), m(c.g(), tint.g()), m(c.b(), tint.b()))
}

/// A standing board drawn as geometry: the background, then the artwork a
/// hair nearer the eye so it is painted over it.
#[allow(clippy::too_many_arguments)]
fn vector_plate(
    scene: &mut Scene,
    eye: Vec3,
    art: &boards::VectorArt,
    bg: Color32,
    ink: Color32,
    bl: Vec3,
    u: Vec3,
    up: Vec3,
    w: f32,
    h: f32,
    tint: Color32,
) {
    let normal = u.cross(up);
    let centre = bl + u * (w / 2.0) + up * (h / 2.0);
    if normal.dot(eye - centre) <= 0.0 {
        return;
    }
    let key = scene.depth(centre) - 0.05;
    let at = |x: f32, y: f32| bl + u * x + up * y;
    let (bg, ink) = (shade(bg, tint), shade(ink, tint));
    for v in [
        [at(0.0, 0.0), at(w, 0.0), at(w, h)],
        [at(0.0, 0.0), at(w, h), at(0.0, h)],
    ] {
        scene.solids.push(Tri {
            v,
            color: bg,
            depth: key,
            uv: None,
        });
    }
    let (k, ox, oy) = fit(art, w, h);
    for t in art.tris.chunks_exact(6) {
        let p = |i: usize| at(ox + t[i] * k, oy + t[i + 1] * k);
        scene.solids.push(Tri {
            v: [p(0), p(2), p(4)],
            color: ink,
            depth: key - 0.02,
            uv: None,
        });
    }
}

/// The same on the floor, read from the broadcast side.
fn ground_vector(
    scene: &mut Scene,
    art: &boards::VectorArt,
    bg: Color32,
    ink: Color32,
    bl: Vec3,
    w: f32,
    h: f32,
) {
    let at = |x: f32, y: f32| bl + v3(x, y, 0.0);
    for v in [
        [at(0.0, 0.0), at(w, 0.0), at(w, h)],
        [at(0.0, 0.0), at(w, h), at(0.0, h)],
    ] {
        scene.ground.push(Tri {
            v,
            color: bg,
            depth: 0.0,
            uv: None,
        });
    }
    let (k, ox, oy) = fit(art, w, h);
    for t in art.tris.chunks_exact(6) {
        let p = |i: usize| at(ox + t[i] * k, oy + t[i + 1] * k);
        scene.ground.push(Tri {
            v: [p(0), p(2), p(4)],
            color: ink,
            depth: 0.0,
            uv: None,
        });
    }
}

fn ground_plate(scene: &mut Scene, uv: UvRect, bl: Vec3, w: f32, h: f32) {
    let white = Color32::WHITE;
    for j in 0..NV {
        for i in 0..NU {
            let p = |a: usize, b: usize| {
                bl + v3(w * a as f32 / NU as f32, h * b as f32 / NV as f32, 0.0)
            };
            let t = |a: usize, b: usize| -> Pos2 {
                pos2(
                    uv[0] + (uv[2] - uv[0]) * a as f32 / NU as f32,
                    uv[3] - (uv[3] - uv[1]) * b as f32 / NV as f32,
                )
            };
            let (c00, c10, c11, c01) = (p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1));
            let (t00, t10, t11, t01) = (t(i, j), t(i + 1, j), t(i + 1, j + 1), t(i, j + 1));
            scene.ground.push(Tri {
                v: [c00, c10, c11],
                color: white,
                depth: 0.0,
                uv: Some([t00, t10, t11]),
            });
            scene.ground.push(Tri {
                v: [c00, c11, c01],
                color: white,
                depth: 0.0,
                uv: Some([t00, t11, t01]),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view3d::{Cam3, Preset, baskets, court};
    use eframe::egui::{Rect, vec2};
    use std::time::Instant;

    fn scene_for(ctx: &Context, cam: &Camera) -> (Scene, usize, usize) {
        let mut scene = Scene::new(cam.eye);
        let hall = begin(&mut scene, ctx, cam);
        court(&mut scene);
        hall.decals(&mut scene);
        baskets(&mut scene);
        let n = (scene.ground.len(), scene.solids.len());
        (scene, n.0, n.1)
    }

    #[test]
    fn frame_cost_with_the_arena() {
        let ctx = Context::default();
        let mut w = World::generate(3, 12);
        w.play_round();
        // The biggest hall, full.
        let home = (0..12).max_by_key(|&t| w.arena(t).capacity).unwrap();
        let fixture = w.fixtures.iter().position(|f| f.home == home).unwrap();
        let mut view = ArenaView::of(&w, fixture);
        view.attendance = view.capacity;
        view.look.two_tiers = true;
        view.look.behind_baskets = true;
        view.publish(&ctx);
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(930.0, 524.0));
        for preset in Preset::ALL {
            let mut c3 = Cam3 {
                preset,
                ..Cam3::default()
            };
            let cam = c3.update(v3(14.0, 7.5, 1.0), 0.016);
            let _ = scene_for(&ctx, &cam);
            let runs = 100;
            let t0 = Instant::now();
            let mut tris = (0, 0, 0);
            for _ in 0..runs {
                let (mut scene, g, s) = scene_for(&ctx, &cam);
                let mesh = scene.mesh(&cam, rect);
                tris = (g, s, mesh.indices.len() / 3);
            }
            let ms = t0.elapsed().as_secs_f64() * 1000.0 / runs as f64;
            println!(
                "{preset:?}: ground {} solid {} emitted {} in {ms:.2} ms",
                tris.0, tris.1, tris.2
            );
            #[cfg(not(debug_assertions))]
            assert!(ms < 2.0, "{preset:?} took {ms} ms");
        }
    }

    #[test]
    fn the_near_stand_is_left_out_for_the_broadcast_camera() {
        let ctx = Context::default();
        let mut c3 = Cam3::default();
        let cam = c3.update(v3(14.0, 7.5, 1.0), 0.016);
        let (scene, _, _) = scene_for(&ctx, &cam);
        // Nothing solid stands between the broadcast eye and the near
        // touchline above the hoarding: no triangle is both nearer to the
        // eye than the court's near edge and taller than the hoarding.
        assert!(
            !scene
                .solids
                .iter()
                .any(|t| t.v.iter().all(|p| p.y < -3.0 && p.y > -14.0 && p.z > 1.0)),
            "the near stand is in the way"
        );
    }

    #[test]
    fn a_fuller_hall_draws_more_fans() {
        let ctx = Context::default();
        let mut c3 = Cam3::default();
        let cam = c3.update(v3(14.0, 7.5, 1.0), 0.016);
        let count = |fill: f32| {
            let mut v = ArenaView::generic();
            v.attendance = (v.capacity as f32 * fill) as u32;
            v.publish(&ctx);
            scene_for(&ctx, &cam).2
        };
        let (empty, half, full) = (count(0.1), count(0.5), count(1.0));
        assert!(empty < half && half < full, "{empty} {half} {full}");
    }
}
