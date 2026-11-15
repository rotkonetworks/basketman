//! The places the game takes place in: your hall seen from the stands, the
//! squad standing on its floor, the practice session, the half court of the
//! tactics board. Each one is the club's own hall from `arena`, the court
//! and the figures of the match view, seen from a different camera.
//!
//! A place is built once and kept as a mesh. It is built again only when
//! something in it changes (a size, a selection, a drawn run, the slow drift
//! of the camera on the home screen), so a screen where nothing moves costs
//! one pass through the mesh and the app can sleep.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use basketman_core::court::{HOOP, Vec2, v};
use basketman_core::league::{TeamId, World};
use basketman_core::player::{Player, PlayerId};
use basketman_core::tactics::Board;
use eframe::egui::{
    self, Align2, Color32, Context, FontId, Mesh, Pos2, Rect, Sense, Shape, Ui, pos2, vec2,
};

use crate::theme;
use crate::view3d::arena::{self, ArenaView, Hall};
use crate::view3d::boards::Atlas;
use crate::view3d::gait::{Gait, Stats};
use crate::view3d::{Camera, FigureSpec, Scene, Vec3, baskets, court, emit_figure, floor_ring, v3};

/// Which place is shown.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub(crate) enum Kind {
    /// Your hall from high in the stands, drifting slowly.
    Home,
    /// A calm look at the hall from the concourse, for the data screens.
    Data,
    /// The squad on its feet on the floor.
    Squad,
    /// The practice floor: lights low, the squad spread out.
    Practice,
}

/// A picture of a place, kept until something changes.
struct Cached {
    key: u64,
    mesh: Arc<Mesh>,
    back: Color32,
    /// The board texture the mesh uses; dropping it would blank the mesh.
    _atlas: Arc<Atlas>,
}

#[derive(Default)]
pub(crate) struct Stage {
    cached: Option<Cached>,
    /// How many times the scene was built, for the report on idling.
    pub builds: u64,
}

fn hash_of(parts: &dyn Fn(&mut DefaultHasher)) -> u64 {
    let mut h = DefaultHasher::new();
    parts(&mut h);
    h.finish()
}

fn rect_bits(r: Rect) -> [u32; 4] {
    [
        r.min.x.to_bits(),
        r.min.y.to_bits(),
        r.max.x.to_bits(),
        r.max.y.to_bits(),
    ]
}

/// Move the camera, without turning it, until `world` is drawn at `want`.
pub(crate) fn pan_to(cam: &mut Camera, rect: Rect, world: Vec3, want: Pos2) {
    for _ in 0..3 {
        let Some((p, depth)) = cam.project(world, rect) else {
            return;
        };
        let k = cam.scale(depth, rect);
        let b = cam.basis();
        let d = p - want;
        let shift = b.right * (d.x / k) + b.up * (-d.y / k);
        cam.eye = cam.eye + shift;
        cam.target = cam.target + shift;
    }
}

/// The point on the floor under a screen position, in court metres. `None`
/// if the pointer is above the horizon.
pub(crate) fn floor_point(cam: &Camera, rect: Rect, p: Pos2) -> Option<(f32, f32)> {
    let b = cam.basis();
    let k = cam.scale(1.0, rect);
    let (dx, dy) = ((p.x - rect.center().x) / k, -(p.y - rect.center().y) / k);
    let dir = b.right * dx + b.up * dy + b.fwd;
    if dir.z >= -1e-4 {
        return None;
    }
    let t = -cam.eye.z / dir.z;
    let at = cam.eye + dir * t;
    Some((at.x, at.y))
}

/// Board metres (across, out from the baseline) to the court: the board's
/// half court is the one at the west end.
pub(crate) fn board_to_court(p: Vec2) -> (f32, f32) {
    (p.y, 7.5 + p.x)
}

pub(crate) fn court_to_board(c: (f32, f32)) -> Vec2 {
    v(c.1 - 7.5, c.0)
}

/// Where the squad stands, in two staggered rows: the first half in front.
/// The last field is whether the place is in the front row.
pub(crate) fn squad_spots(n: usize) -> Vec<(f32, f32, bool)> {
    const SP: f32 = 1.05;
    let front = n.div_ceil(2);
    let back = n - front;
    let mut out = Vec::with_capacity(n);
    for i in 0..front {
        out.push((
            14.0 + (i as f32 - (front as f32 - 1.0) / 2.0) * SP,
            5.0,
            true,
        ));
    }
    let stagger = if back == front { SP / 2.0 } else { 0.0 };
    for j in 0..back {
        out.push((
            14.0 + (j as f32 - (back as f32 - 1.0) / 2.0) * SP + stagger,
            7.1,
            false,
        ));
    }
    out
}

/// Where the squad spreads out for practice: a loose grid over the floor.
fn practice_spots(n: usize) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let (col, row) = (i % 5, i / 5);
            let jitter = |s: u64| ((((i as u64 + 3) * 2654435761) ^ s) % 100) as f32 / 100.0 - 0.5;
            (
                7.0 + col as f32 * 3.4 + jitter(11) * 1.2,
                3.0 + row as f32 * 3.8 + jitter(23) * 1.2,
            )
        })
        .collect()
}

fn fig<'a>(p: &'a Player, at: (f32, f32), face: Vec2, team: Color32) -> FigureSpec<'a> {
    FigureSpec {
        body: &p.body,
        stats: Stats::of(p),
        id: p.id,
        pos: Vec2 { x: at.0, y: at.1 },
        gait: Gait::standing(face),
        team,
        holds_ball: false,
        act: None,
    }
}

fn unit(a: (f32, f32), b: (f32, f32)) -> Vec2 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l = (dx * dx + dy * dy).sqrt().max(1e-3);
    Vec2 {
        x: dx / l,
        y: dy / l,
    }
}

/// Scale a camera so that `per_metre` points make a metre at its target.
fn zoom_for(cam: &mut Camera, rect: Rect, per_metre: f32) {
    let d = (cam.target - cam.eye).len();
    cam.zoom = per_metre / cam.scale(d, rect);
}

/// A name on a dark pill.
fn tag(painter: &egui::Painter, at: Pos2, anchor: Align2, text: &str, size: f32, lit: bool) {
    let font = FontId::new(size, theme::semibold());
    let galley = painter.layout_no_wrap(text.to_string(), font, Color32::WHITE);
    let pad = vec2(6.0, 2.5);
    let r = anchor.anchor_size(at, galley.size() + pad * 2.0);
    let fill = if lit {
        theme::ACCENT
    } else {
        Color32::from_black_alpha(165)
    };
    painter.rect_filled(r, 5.0, fill);
    painter.galley(r.min + pad, galley, Color32::WHITE);
}

fn short(name: &str) -> String {
    let mut w = name.split_whitespace();
    let first = w.next().unwrap_or("");
    match w.next_back() {
        Some(last) => format!("{}. {last}", first.chars().next().unwrap_or(' ')),
        None => first.to_string(),
    }
}

/// What a click or a drag on the figures of a place did.
#[derive(Default)]
pub(crate) struct Touched {
    pub hovered: Option<PlayerId>,
    pub clicked: Option<PlayerId>,
}

impl Stage {
    /// Draw the mesh of a place, building it first if `key` is new.
    fn show(
        &mut self,
        ui: &Ui,
        full: Rect,
        key: u64,
        scrim: u8,
        build: impl FnOnce(&Context) -> (Mesh, Color32, Arc<Atlas>),
    ) {
        let _p = crate::perf::scope("scene: paint");
        if self.cached.as_ref().is_none_or(|c| c.key != key) {
            let _b = crate::perf::scope("scene: build");
            let (mesh, back, atlas) = build(ui.ctx());
            self.builds += 1;
            self.cached = Some(Cached {
                key,
                mesh: Arc::new(mesh),
                back,
                _atlas: atlas,
            });
        }
        let Some(c) = &self.cached else { return };
        let painter = ui.ctx().layer_painter(egui::LayerId::background());
        let painter = painter.with_clip_rect(full);
        painter.rect_filled(full, 0.0, c.back);
        painter.add(Shape::Mesh(c.mesh.clone()));
        if scrim > 0 {
            painter.rect_filled(full, 0.0, Color32::from_black_alpha(scrim));
        }
    }

    /// An empty hall: the home screen's drift and the still view behind
    /// the data screens.
    pub fn hall(&mut self, ui: &Ui, kind: Kind, w: &World, team: TeamId, full: Rect, focus: Rect) {
        let t = ui.input(|i| i.time) as f32;
        let (cam, scrim, tick) = match kind {
            Kind::Home => {
                let a = -2.15 + 0.28 * (t * 0.06).sin();
                let c = v3(14.0, 7.5, 0.0);
                let mut cam = Camera::new(c + v3(a.cos() * 30.0, a.sin() * 22.0, 15.0), c);
                zoom_for(&mut cam, focus, focus.width().max(focus.height()) * 0.034);
                pan_to(&mut cam, focus, v3(14.0, 7.5, 0.0), focus.center());
                (cam, 0, (t * 6.0) as u32)
            }
            _ => {
                let mut cam = Camera::new(v3(-9.0, -10.0, 8.5), v3(20.0, 10.0, 0.0));
                zoom_for(&mut cam, focus, focus.width().max(focus.height()) * 0.036);
                pan_to(&mut cam, focus, v3(16.0, 8.0, 0.0), focus.center());
                (cam, 40, 0)
            }
        };
        let fill = if kind == Kind::Data { 0.3 } else { 0.0 };
        let stamp = Hall::stamp(ui.ctx());
        let key = hash_of(&|h| {
            (
                kind,
                team,
                w.season,
                rect_bits(full),
                rect_bits(focus),
                tick,
                stamp,
            )
                .hash(h)
        });
        self.show(ui, full, key, scrim, |ctx| {
            let attendance = (w.arena(team).capacity as f32 * fill) as u32;
            ArenaView::of_club(w, team, attendance).publish(ctx);
            let mut scene = Scene::new(cam.eye);
            let hall = arena::begin(&mut scene, ctx, &cam);
            court(&mut scene);
            hall.decals(&mut scene);
            baskets(&mut scene);
            let mesh = scene.mesh(&cam, focus);
            (mesh, hall.backdrop, hall.atlas())
        });
        if kind == Kind::Home {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(160));
        }
    }

    /// The squad on the floor, or spread out for practice. Clicking a
    /// figure picks him; `free` is where the scene is not under a panel.
    #[allow(clippy::too_many_arguments)]
    pub fn squad(
        &mut self,
        ui: &mut Ui,
        kind: Kind,
        w: &World,
        team: TeamId,
        order: &[PlayerId],
        selected: Option<PlayerId>,
        full: Rect,
        focus: Rect,
        free: Rect,
    ) -> Touched {
        let n = order.len();
        let mut cam;
        let places: Vec<(f32, f32, bool)> = if kind == Kind::Practice {
            practice_spots(n)
                .into_iter()
                .map(|(x, y)| (x, y, true))
                .collect()
        } else {
            squad_spots(n)
        };
        if kind == Kind::Practice {
            cam = Camera::new(v3(14.0, -9.0, 5.2), v3(14.0, 7.5, 0.8));
            zoom_for(&mut cam, focus, focus.width() / 24.0);
            pan_to(&mut cam, focus, v3(14.0, 7.5, 0.0), focus.center());
        } else {
            cam = Camera::new(v3(14.0, -8.5, 5.4), v3(14.0, 5.5, 1.0));
            let row = places.iter().map(|p| p.0).fold(f32::MIN, f32::max)
                - places.iter().map(|p| p.0).fold(f32::MAX, f32::min)
                + 1.3;
            zoom_for(
                &mut cam,
                focus,
                (focus.width() * 0.94 / row.max(4.0)).min(focus.height() * 0.5 / 2.0),
            );
            pan_to(
                &mut cam,
                focus,
                v3(14.0, 5.0, 0.0),
                pos2(focus.center().x, focus.top() + focus.height() * 0.72),
            );
        }

        // Where each can be picked, before anything is drawn; the back row
        // first, so the front row is on top.
        let mut touched = Touched::default();
        let players = &w.players;
        let mut hits: Vec<(PlayerId, Rect, Pos2, Pos2)> = Vec::new();
        for (i, &pid) in order.iter().enumerate() {
            let (x, y, _) = places[i];
            let tall = players[pid as usize].body.height_cm / 100.0;
            let (Some((head, depth)), Some((foot, _))) = (
                cam.project(v3(x, y, tall + 0.12), focus),
                cam.project(v3(x, y, 0.0), focus),
            ) else {
                continue;
            };
            let half = cam.scale(depth, focus) * 0.34;
            let r = Rect::from_two_pos(head, foot)
                .expand2(vec2(half, 0.0))
                .intersect(full);
            hits.push((pid, r, head, foot));
        }
        let interactive = kind == Kind::Squad;
        for (pid, r, _, _) in hits.iter().rev() {
            if !interactive || !free.contains(r.center()) {
                continue;
            }
            let id = ui.scope_id().with(("figure", *pid));
            let resp = ui
                .interact(*r, id, Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if resp.hovered() && touched.hovered.is_none() {
                touched.hovered = Some(*pid);
            }
            if resp.clicked() {
                touched.clicked = Some(*pid);
            }
        }

        let hovered = touched.hovered;
        let stamp = Hall::stamp(ui.ctx());
        let key = hash_of(&|h| {
            (
                kind,
                team,
                w.season,
                rect_bits(full),
                rect_bits(focus),
                selected,
                hovered,
                order,
                stamp,
            )
                .hash(h);
            for &p in order {
                players[p as usize].injury.is_some().hash(h);
            }
        });
        let scrim = if kind == Kind::Practice { 85 } else { 25 };
        self.show(ui, full, key, scrim, |ctx| {
            ArenaView::of_club(w, team, 0).publish(ctx);
            let mut scene = Scene::new(cam.eye);
            let hall = arena::begin(&mut scene, ctx, &cam);
            court(&mut scene);
            hall.decals(&mut scene);
            for (i, &pid) in order.iter().enumerate() {
                let p = &players[pid as usize];
                let at = (places[i].0, places[i].1);
                let broad = p.body.shoulder_width_cm / 100.0;
                scene.ground_disc(at, broad * 0.9, broad * 0.75, Color32::from_black_alpha(90));
                if p.injury.is_some() {
                    floor_ring(&mut scene, at, 0.5, 0.06, Color32::from_rgb(214, 74, 62));
                }
                if selected == Some(pid) {
                    floor_ring(&mut scene, at, 0.66, 0.1, theme::ACCENT);
                } else if hovered == Some(pid) {
                    floor_ring(&mut scene, at, 0.62, 0.08, Color32::WHITE);
                }
            }
            baskets(&mut scene);
            for (i, &pid) in order.iter().enumerate() {
                let p = &players[pid as usize];
                let at = (places[i].0, places[i].1);
                let face = if kind == Kind::Practice {
                    let h = (pid as f32 * 2.399) + 0.7;
                    Vec2 {
                        x: h.cos(),
                        y: h.sin(),
                    }
                } else {
                    unit((at.0 * 0.9 + 1.4, at.1), (14.0, -8.0))
                };
                let team = if p.injury.is_some() {
                    Color32::from_rgb(120, 100, 140)
                } else {
                    theme::ACCENT
                };
                emit_figure(&mut scene, &fig(p, at, face, team));
            }
            let mesh = scene.mesh(&cam, focus);
            (mesh, hall.backdrop, hall.atlas())
        });

        // Names, on top of the picture.
        let painter = ui.painter().with_clip_rect(full);
        let small = focus.width() < 520.0;
        let size = if small { 10.5 } else { 12.0 };
        let label = |n: &str| {
            if small {
                n.split_whitespace().next_back().unwrap_or(n).to_string()
            } else {
                short(n)
            }
        };
        for (pid, _, head, foot) in &hits {
            let p = &players[*pid as usize];
            let lit = selected == Some(*pid) || hovered == Some(*pid);
            let front = order
                .iter()
                .position(|o| o == pid)
                .is_some_and(|i| places[i].2);
            if kind == Kind::Practice && !lit {
                continue;
            }
            if front {
                tag(
                    &painter,
                    *foot + vec2(0.0, 5.0),
                    Align2::CENTER_TOP,
                    &label(&p.name),
                    size,
                    selected == Some(*pid),
                );
            } else {
                tag(
                    &painter,
                    *head - vec2(0.0, 3.0),
                    Align2::CENTER_BOTTOM,
                    &label(&p.name),
                    size,
                    selected == Some(*pid),
                );
            }
        }
        touched
    }
}

/// What the tactics place returns: the board after this frame's edits and
/// the starter picked.
pub(crate) struct TacticsOut {
    pub board: Board,
    pub picked: Option<usize>,
}

impl Stage {
    /// The half court with the five on their spots and the drawn runs on
    /// the floor. Drag a man to move his spot, or in draw mode to give him
    /// a run; click to pick him. `free` is the part of the picture not under
    /// a panel, where the pointer is for the scene.
    #[allow(clippy::too_many_arguments)]
    pub fn tactics(
        &mut self,
        ui: &mut Ui,
        w: &World,
        team: TeamId,
        board: &Board,
        cast: &[Option<PlayerId>; 5],
        selected: Option<PlayerId>,
        drawing: bool,
        full: Rect,
        focus: Rect,
        free: Rect,
    ) -> TacticsOut {
        let mut cam = Camera::new(v3(17.0, 7.5, 12.5), v3(4.0, 7.5, 0.0));
        let per = (focus.width() * 0.94 / 15.5).min(focus.height() * 0.95 / 13.0);
        zoom_for(&mut cam, focus, per * 0.9);
        pan_to(&mut cam, focus, v3(6.5, 7.5, 0.0), focus.center());

        let mut out = TacticsOut {
            board: board.clone(),
            picked: None,
        };
        // The pointer on the floor, in board metres.
        for i in 0..5 {
            let (cx, cy) = board_to_court(board.spots[i]);
            let tall = cast[i].map_or(1.95, |p| w.players[p as usize].body.height_cm / 100.0);
            let (Some((head, depth)), Some((foot, _))) = (
                cam.project(v3(cx, cy, tall + 0.1), focus),
                cam.project(v3(cx, cy, 0.0), focus),
            ) else {
                continue;
            };
            let half = (cam.scale(depth, focus) * 0.4).max(16.0);
            let r = Rect::from_two_pos(head, foot)
                .expand2(vec2(half, 6.0))
                .intersect(free);
            if !r.is_positive() {
                continue;
            }
            let id = ui.scope_id().with(("spot", i));
            let resp = ui
                .interact(r, id, Sense::click_and_drag())
                .on_hover_cursor(egui::CursorIcon::Grab);
            if resp.clicked() && drawing {
                out.board.runs[i] = None;
            }
            if resp.clicked() || resp.drag_started() {
                out.picked = Some(i);
            }
            if resp.dragged()
                && let Some(p) = resp.interact_pointer_pos()
                && let Some(at) = floor_point(&cam, focus, p)
            {
                let b = Board::clamp(court_to_board(at));
                if drawing {
                    out.board.runs[i] = Some(b);
                } else {
                    out.board.spots[i] = b;
                }
            }
        }

        let shown = &out.board;
        let stamp = Hall::stamp(ui.ctx());
        let key = hash_of(&|h| {
            (
                team,
                w.season,
                rect_bits(full),
                rect_bits(focus),
                selected,
                stamp,
            )
                .hash(h);
            for s in shown.spots {
                (s.x.to_bits(), s.y.to_bits()).hash(h);
            }
            for r in shown.runs.iter().flatten() {
                (r.x.to_bits(), r.y.to_bits()).hash(h);
            }
            cast.hash(h);
        });
        let players = &w.players;
        self.show(ui, full, key, 20, |ctx| {
            ArenaView::of_club(w, team, 0).publish(ctx);
            let mut scene = Scene::new(cam.eye);
            let hall = arena::begin(&mut scene, ctx, &cam);
            court(&mut scene);
            hall.decals(&mut scene);
            // Runs first, on the floor, under the figures.
            for i in 0..5 {
                let Some(to) = shown.runs[i] else { continue };
                let (a, b) = (board_to_court(shown.spots[i]), board_to_court(to));
                let u = unit(a, b);
                let start = (a.0 + u.x * 0.6, a.1 + u.y * 0.6);
                let accent = theme::ACCENT;
                scene.ground_line(start, b, 0.16, accent);
                let n = (-u.y, u.x);
                let back = (b.0 - u.x * 0.7, b.1 - u.y * 0.7);
                scene.ground_poly(
                    &[
                        v3(b.0, b.1, 0.0),
                        v3(back.0 + n.0 * 0.35, back.1 + n.1 * 0.35, 0.0),
                        v3(back.0 - n.0 * 0.35, back.1 - n.1 * 0.35, 0.0),
                    ],
                    accent,
                );
            }
            for i in 0..5 {
                let at = board_to_court(shown.spots[i]);
                scene.ground_disc(at, 0.5, 0.42, Color32::from_black_alpha(90));
                if cast[i].is_some() && cast[i] == selected {
                    floor_ring(&mut scene, at, 0.7, 0.11, theme::ACCENT);
                }
            }
            baskets(&mut scene);
            let hoop = board_to_court(HOOP);
            for i in 0..5 {
                let at = board_to_court(shown.spots[i]);
                let face = unit(at, hoop);
                match cast[i] {
                    Some(pid) => {
                        let p = &players[pid as usize];
                        emit_figure(&mut scene, &fig(p, at, face, theme::ACCENT));
                    }
                    None => scene.sphere(v3(at.0, at.1, 0.9), 0.3, Color32::from_gray(150)),
                }
            }
            let mesh = scene.mesh(&cam, focus);
            (mesh, hall.backdrop, hall.atlas())
        });

        let painter = ui.painter().with_clip_rect(full);
        for i in 0..5 {
            let (cx, cy) = board_to_court(out.board.spots[i]);
            let Some((foot, _)) = cam.project(v3(cx, cy, 0.0), focus) else {
                continue;
            };
            let lit = cast[i].is_some() && cast[i] == selected;
            let who = cast[i].map_or("open".to_string(), |p| short(&players[p as usize].name));
            tag(
                &painter,
                foot + vec2(0.0, 5.0),
                Align2::CENTER_TOP,
                &who,
                12.0,
                lit,
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_floor_under_the_pointer_round_trips_through_the_camera() {
        let rect = Rect::from_min_size(pos2(100.0, 60.0), vec2(900.0, 600.0));
        let mut cam = Camera::new(v3(17.0, 7.5, 12.5), v3(4.0, 7.5, 0.0));
        cam.zoom = 1.3;
        for &(x, y) in &[(3.0, 7.5), (6.0, 2.0), (9.5, 13.0), (1.575, 7.5)] {
            let (p, _) = cam.project(v3(x, y, 0.0), rect).unwrap();
            let (fx, fy) = floor_point(&cam, rect, p).unwrap();
            assert!((fx - x).abs() < 1e-2 && (fy - y).abs() < 1e-2, "{x},{y}");
        }
    }

    #[test]
    fn the_pointer_above_the_horizon_is_not_on_the_floor() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 500.0));
        let cam = Camera::new(v3(0.0, -10.0, 1.0), v3(0.0, 0.0, 1.5));
        assert!(floor_point(&cam, rect, pos2(400.0, 5.0)).is_none());
    }

    #[test]
    fn the_board_maps_onto_the_west_half_of_the_court() {
        // The hoop of the board is the west basket, 1.575 m from the line.
        let c = board_to_court(HOOP);
        assert!((c.0 - 1.575).abs() < 1e-4 && (c.1 - 7.5).abs() < 1e-4);
        let b = court_to_board((6.0, 10.0));
        assert!((b.x - 2.5).abs() < 1e-4 && (b.y - 6.0).abs() < 1e-4);
    }

    #[test]
    fn panning_puts_the_point_where_it_was_asked() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 500.0));
        let mut cam = Camera::new(v3(14.0, -9.0, 5.0), v3(14.0, 7.0, 1.0));
        let want = pos2(333.0, 222.0);
        pan_to(&mut cam, rect, v3(14.0, 5.0, 0.0), want);
        let (p, _) = cam.project(v3(14.0, 5.0, 0.0), rect).unwrap();
        assert!((p - want).length() < 0.5);
    }

    #[test]
    fn the_squad_stands_in_two_rows_with_nobody_on_top_of_anybody() {
        for n in [5, 8, 12, 13, 15] {
            let spots = squad_spots(n);
            assert_eq!(spots.len(), n);
            for (i, a) in spots.iter().enumerate() {
                for b in &spots[i + 1..] {
                    let d = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
                    assert!(d > 0.5, "{n}: {d}");
                }
            }
        }
    }
}
