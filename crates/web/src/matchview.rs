//! The match screen: the user's game on a 2D court with the event feed next
//! to it. While the game is live the touchline sits underneath: timeouts,
//! substitutions, shouts and changes of system. Afterwards the same screen
//! is the replay.

use basketman_core::court::Vec2;
use basketman_core::engine::{GameResult, LivePlayer, Orders, Pause, Reaction, Shout};
use basketman_core::league::{LiveInput, LiveRound};
use basketman_core::player::{Player, PlayerId};
use basketman_core::replay::{EventKind, Frame, Recording};
use basketman_core::tactics::{DefenseScheme, OffenseSet, Press};
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, pos2, vec2,
};

use crate::app::{App, header_row, name};
use crate::nav::{Link, follow};
use crate::theme::{self, Backdrop};
use crate::view3d::gait::{Dims, Stats};
use crate::view3d::{self, Cam3, FigureSpec, Preset, Scene, Vec3, v3};

mod feeds;
mod hud;
pub(crate) use feeds::Ctx as FeedCtx;
pub(crate) use hud::Hud;

const LENGTH: f32 = 28.0;
const WIDTH: f32 = 15.0;
const HOOP_X: f32 = 1.575;
const BALL: Color32 = Color32::from_rgb(0xE8, 0x6A, 0x1C);
const SPEEDS: [f32; 4] = [1.0, 2.0, 4.0, 8.0];

pub(crate) struct Playback {
    pub time: f32,
    pub playing: bool,
    pub speed: f32,
}

impl Playback {
    pub fn start() -> Self {
        Playback {
            time: 0.0,
            playing: true,
            speed: 4.0,
        }
    }
}

/// What the user did on the touchline this frame. Applied once the screen
/// is drawn, because the screen reads the game these change.
pub(crate) enum Command {
    Timeout,
    AutoSubs(bool),
    /// Off, on.
    Swap(PlayerId, PlayerId),
    CancelSwap(usize),
    Shout(Shout, Option<PlayerId>),
    Orders(Orders),
    SkipPeriod,
    SkipAll,
}

#[derive(Default)]
pub(crate) struct Touchline {
    /// Player on the floor picked to come off.
    pub off: Option<PlayerId>,
    /// Who the next shout is for; `None` is the five on the floor.
    target: Option<PlayerId>,
    /// The last shout and how each player took it.
    said: Option<(Shout, Vec<(PlayerId, Reaction)>)>,
    /// Why the game is stopped.
    notice: Option<String>,
}

impl Touchline {
    pub fn show(&mut self, shout: Shout, reactions: Vec<(PlayerId, Reaction)>) {
        self.said = Some((shout, reactions));
    }
}

fn mood(p: &LivePlayer) -> (&'static str, Color32) {
    if p.fouled_out {
        ("fouled out", theme::WARN)
    } else if p.focus < -0.15 {
        ("rattled", theme::WARN)
    } else if p.drive < -0.15 {
        ("flat", theme::WARN)
    } else if p.drive > 0.25 {
        ("fired up", theme::POS)
    } else if p.focus > 0.25 {
        ("locked in", theme::POS)
    } else {
        ("", theme::TEXT_DIM)
    }
}

/// How far behind the simulation a live game is drawn, in seconds of game
/// time: a little more than the gap between two recorded positions.
const LIVE_DELAY: f32 = 0.3;

/// How the court is drawn.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ViewMode {
    /// A real 3D scene with a moving camera.
    Three,
    /// The older fake-perspective 2D drawing. No longer offered: the 3D
    /// view replaced it. Kept only so the drawing code can still be
    /// compared against in tests of the harness.
    Angled,
    /// Straight down, the 2D view.
    Flat,
    /// No picture: the play-by-play, large.
    Commentary,
}

impl ViewMode {
    const ALL: [(ViewMode, &'static str); 3] = [
        (ViewMode::Three, "3D"),
        (ViewMode::Flat, "2D"),
        (ViewMode::Commentary, "Commentary"),
    ];

    /// What is stored under `VIEW_KEY`. The two older values keep their
    /// meaning.
    pub fn key(self) -> &'static str {
        match self {
            ViewMode::Three => "3d",
            ViewMode::Angled => "angled",
            ViewMode::Flat => "flat",
            ViewMode::Commentary => "commentary",
        }
    }

    pub fn from_key(key: &str) -> Option<ViewMode> {
        Some(match key {
            "3d" => ViewMode::Three,
            "angled" => ViewMode::Angled,
            "flat" => ViewMode::Flat,
            "commentary" => ViewMode::Commentary,
            _ => return None,
        })
    }
}

/// The view the user picked, and the state of the 3D camera.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Look {
    pub mode: ViewMode,
    pub cam: Cam3,
}

impl Default for Look {
    fn default() -> Self {
        Look {
            mode: ViewMode::Three,
            cam: Cam3::default(),
        }
    }
}

/// The row of buttons that picks the view. `commentary` is false where a
/// text-only view makes no sense, as in the film room.
pub(crate) fn view_buttons(ui: &mut Ui, mode: &mut ViewMode, commentary: bool) {
    for (m, text) in ViewMode::ALL {
        if m == ViewMode::Commentary && !commentary {
            continue;
        }
        if ui.add(egui::Button::selectable(*mode == m, text)).clicked() {
            *mode = m;
        }
    }
}

thread_local! {
    /// A height the 3D court is drawn at, whatever its width.
    static FILL: std::cell::Cell<Option<f32>> = const { std::cell::Cell::new(None) };
}

/// Draw the 3D court this high until told otherwise (`None`).
fn set_fill_height(h: Option<f32>) {
    FILL.with(|f| f.set(h));
}

/// Height of the picture for a given width.
fn view_height(width: f32, mode: ViewMode) -> f32 {
    // The screen's own frame asks for a court that fills its window.
    if matches!(mode, ViewMode::Three | ViewMode::Commentary)
        && let Some(h) = FILL.with(std::cell::Cell::get)
    {
        return h;
    }
    match mode {
        ViewMode::Three | ViewMode::Commentary => (width * 0.5625).min(720.0),
        ViewMode::Angled => Pitch::height_for(width, true),
        ViewMode::Flat => Pitch::height_for(width, false),
    }
}

/// How much smaller the far sideline is drawn than the near one in the
/// angled view.
const FAR: f32 = 0.74;
/// How much the court's depth is squashed by the camera angle.
const TILT: f32 = 0.60;

/// Maps court metres to screen points: straight down, or from a camera
/// above the near sideline looking across the court, as on television.
struct Pitch {
    rect: Rect,
    /// Screen points per metre at the near sideline.
    unit: f32,
    angled: bool,
}

impl Pitch {
    fn new(rect: Rect, angled: bool) -> Self {
        let unit = rect.width() / if angled { LENGTH + 0.4 } else { LENGTH };
        Pitch { rect, unit, angled }
    }

    /// Height of the drawing for a given width.
    fn height_for(width: f32, angled: bool) -> f32 {
        if angled {
            let unit = width / (LENGTH + 0.4);
            // Headroom for the far players and the baskets, then the floor.
            (2.6 + WIDTH * TILT * (FAR + 1.0) / 2.0 + 0.5) * unit
        } else {
            width * WIDTH / LENGTH
        }
    }

    /// Size of a metre at court depth `y`, in screen points.
    fn metre(&self, y: f32) -> f32 {
        if self.angled {
            self.unit * (FAR + (1.0 - FAR) * (y / WIDTH).clamp(0.0, 1.0))
        } else {
            self.unit
        }
    }

    /// A point `z` metres above the floor.
    fn at3(&self, x: f32, y: f32, z: f32) -> Pos2 {
        if !self.angled {
            return pos2(
                self.rect.left() + x * self.unit,
                self.rect.top() + y * self.unit,
            );
        }
        let t = y / WIDTH;
        // Rows get taller towards the camera in step with how much wider
        // they get, which is what makes it read as perspective.
        let down = (FAR * t + (1.0 - FAR) * t * t / 2.0) / (FAR + (1.0 - FAR) / 2.0);
        let depth = WIDTH * TILT * (FAR + 1.0) / 2.0 * self.unit;
        let m = self.metre(y);
        pos2(
            self.rect.center().x + (x - LENGTH / 2.0) * m,
            self.rect.top() + 2.6 * self.unit + depth * down - z * m * 0.95,
        )
    }

    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.at3(x, y, 0.0)
    }

    fn of(&self, p: Vec2) -> Pos2 {
        self.at(p.x, p.y)
    }

    /// Part of a circle around `(cx, cy)` at height `z`, angles in turns
    /// from the +x axis.
    fn arc3(&self, cx: f32, cy: f32, z: f32, r: f32, from: f32, to: f32) -> Vec<Pos2> {
        (0..=48)
            .map(|i| {
                let a = (from + (to - from) * i as f32 / 48.0) * std::f32::consts::TAU;
                self.at3(cx + r * a.cos(), cy + r * a.sin(), z)
            })
            .collect()
    }

    fn arc(&self, cx: f32, cy: f32, r: f32, from: f32, to: f32) -> Vec<Pos2> {
        self.arc3(cx, cy, 0.0, r, from, to)
    }

    fn quad(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Pos2> {
        vec![
            self.at(x0, y0),
            self.at(x1, y0),
            self.at(x1, y1),
            self.at(x0, y1),
        ]
    }
}

fn paint_court(painter: &egui::Painter, pitch: &Pitch) {
    let line = Stroke::new(1.5, theme::TEXT_MUTE);
    let floor = Color32::from_rgba_unmultiplied(255, 255, 255, 150);
    let paint = Color32::from_rgba_unmultiplied(0x62, 0x43, 0xD9, 20);
    painter.add(Shape::convex_polygon(
        pitch.quad(0.0, 0.0, LENGTH, WIDTH),
        floor,
        Stroke::new(2.0, theme::TEXT_MUTE),
    ));

    for end in [0.0, 1.0f32] {
        // Mirror everything for the far basket.
        let x = |d: f32| if end == 0.0 { d } else { LENGTH - d };
        painter.add(Shape::convex_polygon(
            pitch.quad(x(0.0), 5.05, x(5.8), 9.95),
            paint,
            line,
        ));
        // Free-throw circle, three-point line, no-charge semicircle.
        painter.add(Shape::line(
            pitch.arc(x(5.8), 7.5, 1.8, 0.0, 1.0),
            Stroke::new(1.0, theme::LINE_2),
        ));
        let (a0, a1) = if end == 0.0 {
            (-0.2168, 0.2168)
        } else {
            (0.5 + 0.2168, 0.5 - 0.2168)
        };
        let mut three = vec![pitch.at(x(0.0), 0.9)];
        three.extend(pitch.arc(x(HOOP_X), 7.5, 6.75, a0, a1));
        three.push(pitch.at(x(0.0), 14.1));
        painter.add(Shape::line(three, line));
        let (c0, c1) = if end == 0.0 {
            (-0.25, 0.25)
        } else {
            (0.25, 0.75)
        };
        painter.add(Shape::line(
            pitch.arc(x(HOOP_X), 7.5, 1.25, c0, c1),
            Stroke::new(1.0, theme::LINE_2),
        ));
        if !pitch.angled {
            // Backboard and rim from above.
            painter.line_segment(
                [pitch.at(x(1.2), 6.6), pitch.at(x(1.2), 8.4)],
                Stroke::new(2.5, theme::TEXT),
            );
            painter.circle_stroke(
                pitch.at(x(HOOP_X), 7.5),
                0.23 * pitch.unit,
                Stroke::new(2.0, BALL),
            );
        }
    }
    painter.line_segment([pitch.at(14.0, 0.0), pitch.at(14.0, WIDTH)], line);
    painter.add(Shape::line(pitch.arc(14.0, 7.5, 1.8, 0.0, 1.0), line));
}

/// The baskets as they stand in the angled view: post, board, rim and net.
/// Drawn after the players, who mostly pass in front of or under them.
fn paint_baskets(painter: &egui::Painter, pitch: &Pitch) {
    for end in [0.0, 1.0f32] {
        let x = |d: f32| if end == 0.0 { d } else { LENGTH - d };
        let ink = Stroke::new(2.0, theme::TEXT);
        painter.line_segment(
            [pitch.at3(x(0.2), 7.5, 0.0), pitch.at3(x(0.2), 7.5, 2.9)],
            ink,
        );
        painter.line_segment(
            [pitch.at3(x(0.2), 7.5, 2.9), pitch.at3(x(1.2), 7.5, 3.2)],
            ink,
        );
        let board = vec![
            pitch.at3(x(1.2), 6.6, 2.9),
            pitch.at3(x(1.2), 8.4, 2.9),
            pitch.at3(x(1.2), 8.4, 3.95),
            pitch.at3(x(1.2), 6.6, 3.95),
        ];
        painter.add(Shape::convex_polygon(
            board,
            Color32::from_rgba_unmultiplied(255, 255, 255, 170),
            Stroke::new(1.5, theme::TEXT),
        ));
        let rim = pitch.arc3(x(HOOP_X), 7.5, 3.05, 0.23, 0.0, 1.0);
        let mouth = pitch.arc3(x(HOOP_X), 7.5, 2.62, 0.13, 0.0, 1.0);
        for i in (0..48).step_by(6) {
            painter.line_segment([rim[i], mouth[i]], Stroke::new(1.0, theme::TEXT_MUTE));
        }
        painter.add(Shape::line(rim, Stroke::new(2.0, BALL)));
    }
}

/// Positions at `time`, blended between the two frames around it.
/// A point `u` of the way from `p1` to `p2` on the smooth curve through the
/// four points (Catmull-Rom). At `u` = 0 and 1 it is exactly `p1` and `p2`.
fn curve(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, u: f32) -> Vec2 {
    let (u2, u3) = (u * u, u * u * u);
    (p1 * 2.0
        + (p2 - p0) * u
        + (p0 * 2.0 - p1 * 5.0 + p2 * 4.0 - p3) * u2
        + (p1 * 3.0 - p0 - p2 * 3.0 + p3) * u3)
        * 0.5
}

/// The frame at `time`, the frame after it with how far between them `time`
/// is (when they are one continuous movement), and everybody's position and
/// the ball blended between them.
type Blend<'a> = (
    &'a Frame,
    [[Vec2; 5]; 2],
    Vec2,
    f32,
    Option<(&'a Frame, f32)>,
);

/// Scalar form of `curve`.
fn curve1(p0: f32, p1: f32, p2: f32, p3: f32, u: f32) -> f32 {
    let (u2, u3) = (u * u, u * u * u);
    (p1 * 2.0
        + (p2 - p0) * u
        + (p0 * 2.0 - p1 * 5.0 + p2 * 4.0 - p3) * u2
        + (p1 * 3.0 - p0 - p2 * 3.0 + p3) * u3)
        * 0.5
}

fn blend(rec: &Recording, time: f32) -> Blend<'_> {
    let i = rec.frame_at(time);
    let a = &rec.frames[i];
    let Some(b) = rec.frames.get(i + 1) else {
        return (a, a.pos, a.ball, a.ball_height, None);
    };
    let span = b.time - a.time;
    // Do not glide across a cut (a timeout, the break between periods). Every
    // other frame is one tick after the one before it; the span check is only
    // a guard against recordings made at another rate.
    if b.cut || span <= 0.0 || span > 0.25 {
        return (a, a.pos, a.ball, a.ball_height, None);
    }
    let u = ((time - a.time) / span).clamp(0.0, 1.0);
    // The recorded positions before `a` and after `b`, when play ran on
    // without a whistle between them: with four points a player's path can
    // be drawn as the curve he ran, not as straight pieces with a corner
    // five times a second.
    let near = |from: &Frame, to: &Frame| {
        let gap = to.time - from.time;
        !to.cut && gap > 0.0 && gap <= 0.25
    };
    let before = i
        .checked_sub(1)
        .map(|h| &rec.frames[h])
        .filter(|f| near(f, a));
    let after = rec.frames.get(i + 2).filter(|f| near(b, f));
    let mut pos = a.pos;
    for side in 0..2 {
        for k in 0..5 {
            // Match players by who they are: court indices move on a
            // substitution.
            let id = a.who[side][k];
            let spot = |f: &Frame| {
                f.who[side]
                    .iter()
                    .position(|&w| w == id)
                    .map(|j| f.pos[side][j])
            };
            if let Some(p2) = spot(b) {
                let p1 = a.pos[side][k];
                let p0 = before.and_then(spot).unwrap_or(p1);
                let p3 = after.and_then(spot).unwrap_or(p2);
                pos[side][k] = curve(p0, p1, p2, p3, u);
            }
        }
    }
    // A ball in the air follows a parabola: the curve through four frames
    // draws it rounder than straight pieces. In a hand it is the hand's.
    let (ball, height) = if let Some((p, z)) = rec.ball_at(time) {
        // The ball's own recorded path: a shot, a block, a tip. Drawn as it
        // was, not glided between frames.
        (p, z)
    } else if a.holder.is_some() || b.holder.is_some() {
        (
            a.ball.lerp(b.ball, u),
            a.ball_height + (b.ball_height - a.ball_height) * u,
        )
    } else {
        let (p0, p3) = (
            before.map_or(a.ball, |f| f.ball),
            after.map_or(b.ball, |f| f.ball),
        );
        let (h0, h3) = (
            before.map_or(a.ball_height, |f| f.ball_height),
            after.map_or(b.ball_height, |f| f.ball_height),
        );
        (
            curve(p0, a.ball, b.ball, p3, u),
            curve1(h0, a.ball_height, b.ball_height, h3, u).max(0.12),
        )
    };
    (a, pos, ball, height, Some((b, u)))
}

fn clock(secs: f32) -> String {
    let s = secs.ceil().max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

fn initials(full: &str) -> String {
    full.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect()
}

/// The court with both teams on it as they stood at `time`: the picture the
/// match screen and the film room share. Fills the width it is given;
/// `picked` is ringed.
pub(crate) fn paint_floor(
    ui: &mut Ui,
    rec: &Recording,
    time: f32,
    look: &mut Look,
    players: &[Player],
    result: &GameResult,
    picked: Option<PlayerId>,
) {
    // Where there is a picture at all, commentary mode shows the 3D one.
    if matches!(look.mode, ViewMode::Three | ViewMode::Commentary) {
        return paint_3d(ui, rec, time, &mut look.cam, players, result, picked);
    }
    let angled = look.mode == ViewMode::Angled;
    let (frame, pos, ball, ball_height, _) = blend(rec, time);
    let colors = [theme::ACCENT, theme::TEXT];
    let player = |side: usize, slot: u8| &players[result.boxes[side][slot as usize].0 as usize];
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, Pitch::height_for(w, angled)), Sense::hover());
    let pitch = Pitch::new(rect, angled);
    let painter = ui.painter_at(rect.expand(2.0));
    paint_court(&painter, &pitch);
    let hover = ui.input(|i| i.pointer.hover_pos());

    // Everything standing on the floor, far side first so the
    // near side is drawn over it. `None` is the ball.
    let mut order: Vec<(f32, Option<(usize, usize)>)> = (0..2)
        .flat_map(|side| (0..5).map(move |k| (side, k)))
        .map(|(side, k)| (pos[side][k].y, Some((side, k))))
        .collect();
    order.push((ball.y + 0.01, None));
    if angled {
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    let mut label: Option<(Pos2, String)> = None;
    for (_, item) in order {
        let Some((side, k)) = item else {
            // The ball, and its shadow on the floor under it.
            if angled {
                let m = pitch.metre(ball.y);
                painter.add(Shape::ellipse_filled(
                    pitch.of(ball),
                    vec2(0.17 * m, 0.08 * m),
                    Color32::from_black_alpha(55),
                ));
                let at = pitch.at3(ball.x, ball.y, ball_height);
                painter.circle_filled(at, 0.17 * m, BALL);
                painter.circle_stroke(at, 0.17 * m, Stroke::new(1.0, theme::TEXT));
            } else {
                // From above the ball grows a little and casts
                // a longer shadow as it rises.
                let at = pitch.of(ball);
                let lift = (ball_height - 1.0).max(0.0);
                painter.circle_filled(
                    at + vec2(lift * 3.0, lift * 4.0),
                    0.2 * pitch.unit,
                    Color32::from_black_alpha(50),
                );
                painter.circle_filled(at, (0.2 + lift * 0.04) * pitch.unit, BALL);
            }
            continue;
        };
        let spot = pos[side][k];
        let at = pitch.of(spot);
        let p = player(side, frame.who[side][k]);
        let has_ball = frame.holder == Some((side as u8, k as u8));
        let color = colors[side];
        // Where he can be pointed at and clicked.
        let target = if angled {
            // A figure as tall and as broad as the player is.
            let m = pitch.metre(spot.y);
            // Drawn a quarter over life size, or they are lost on a full court.
            let tall = p.body.height_cm / 100.0 * m * 1.2;
            let broad = p.body.shoulder_width_cm / 100.0 * m * 1.6;
            painter.add(Shape::ellipse_filled(
                at,
                vec2(broad * 0.75, broad * 0.3),
                Color32::from_black_alpha(45),
            ));
            let ring = |r: f32, c: Color32| {
                painter.add(Shape::ellipse_stroke(
                    at,
                    vec2(broad * r, broad * r * 0.4),
                    Stroke::new(2.0, c),
                ));
            };
            if has_ball {
                ring(1.0, BALL);
            }
            if picked == Some(p.id) {
                ring(1.3, theme::WARN);
            }
            let up = |share: f32| at - vec2(0.0, tall * share);
            let legs = Stroke::new((broad * 0.26).max(1.5), theme::TEXT_DIM);
            for dx in [-0.22, 0.22] {
                painter.line_segment(
                    [at + vec2(broad * dx, 0.0), up(0.50) + vec2(broad * dx, 0.0)],
                    legs,
                );
            }
            let torso = Rect::from_min_max(
                up(0.86) - vec2(broad / 2.0, 0.0),
                up(0.47) + vec2(broad / 2.0, 0.0),
            );
            painter.rect_filled(torso, broad * 0.22, color);
            painter.circle_filled(up(0.93), tall * 0.068, color);
            painter.circle_stroke(
                up(0.93),
                tall * 0.068,
                Stroke::new(1.0, Color32::from_white_alpha(170)),
            );
            let size = torso.width() * 0.55;
            if size >= 7.0 {
                painter.text(
                    torso.center(),
                    Align2::CENTER_CENTER,
                    initials(&p.name),
                    FontId::new(size, theme::mono_semibold()),
                    Color32::WHITE,
                );
            }
            Rect::from_min_max(
                pos2(at.x - broad * 0.6, at.y - tall),
                pos2(at.x + broad * 0.6, at.y),
            )
        } else {
            let radius = (0.5 * pitch.unit).max(8.0);
            painter.circle_filled(at + vec2(1.0, 2.0), radius, Color32::from_black_alpha(40));
            painter.circle_filled(at, radius, color);
            if has_ball {
                painter.circle_stroke(at, radius + 3.0, Stroke::new(2.0, BALL));
            }
            if picked == Some(p.id) {
                painter.circle_stroke(at, radius + 6.0, Stroke::new(2.0, theme::WARN));
            }
            painter.text(
                at,
                Align2::CENTER_CENTER,
                initials(&p.name),
                FontId::new(radius * 0.85, theme::mono_semibold()),
                Color32::WHITE,
            );
            Rect::from_center_size(at, vec2(radius * 2.0, radius * 2.0))
        };
        // A player on the floor is a link like his name is.
        let id = ui.scope_id().with(("disc", side, k));
        if ui
            .interact(target, id, Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            follow(ui, Link::Player(p.id));
        }
        if hover.is_some_and(|h| target.expand(2.0).contains(h)) {
            let text = format!("{} ({})", p.name, p.position.label());
            label = Some((target.center_top() - vec2(0.0, 4.0), text));
        }
    }
    if angled {
        paint_baskets(&painter, &pitch);
    }
    if let Some((at, text)) = label {
        painter.text(
            at,
            Align2::CENTER_BOTTOM,
            text,
            FontId::new(13.0, theme::semibold()),
            theme::TEXT,
        );
    }
}

/// The newest ball through a net in the last moments before `time`: which
/// basket (0 the low-x one), how long ago, and the points it was worth.
fn recent_swish(rec: &Recording, time: f32) -> Option<(usize, f32, u16)> {
    let i = rec.frame_at(time);
    for j in (i.saturating_sub(8)..=i).rev() {
        let f = &rec.frames[j];
        if time - f.time > 0.6 {
            break;
        }
        if f.swish {
            let o = f.offense as usize;
            let before = j.checked_sub(1).map_or(f.score[o], |h| rec.frames[h].score[o]);
            let points = f.score[o].saturating_sub(before).max(1);
            // The offense shoots at the far end: home (0) at x = 28.
            return Some((1 - o, time - f.time, points));
        }
    }
    None
}

/// A ring that spreads from the basket and a "+2" that rises and fades.
fn paint_points(painter: &egui::Painter, cam: &view3d::Camera, rect: Rect, end: usize, age: f32, points: u16) {
    let cx = if end == 0 { HOOP_X } else { LENGTH - HOOP_X };
    let fade = (1.0 - age / 1.0).clamp(0.0, 1.0);
    if age < 0.6 {
        let r = 0.3 + 1.1 * (age / 0.6);
        let a = ((1.0 - age / 0.6) * 150.0) as u8;
        let pts: Vec<Pos2> = (0..=28)
            .filter_map(|n| {
                let t = std::f32::consts::TAU * n as f32 / 28.0;
                cam.project(v3(cx + r * t.cos(), 7.5 + r * t.sin(), 3.05), rect).map(|p| p.0)
            })
            .collect();
        painter.add(Shape::line(pts, Stroke::new(2.0, Color32::from_rgba_unmultiplied(255, 230, 160, a))));
    }
    if let Some((at, depth)) = cam.project(v3(cx, 7.5, 3.9 + 0.6 * age), rect) {
        let size = (cam.scale(depth, rect) * 0.5).clamp(14.0, 34.0);
        let a = (fade * 255.0) as u8;
        painter.text(
            at + vec2(1.0, 1.0),
            Align2::CENTER_CENTER,
            format!("+{points}"),
            FontId::new(size, theme::mono_semibold()),
            Color32::from_black_alpha(a / 2),
        );
        painter.text(
            at,
            Align2::CENTER_CENTER,
            format!("+{points}"),
            FontId::new(size, theme::mono_semibold()),
            Color32::from_rgba_unmultiplied(255, 236, 170, a),
        );
    }
}

/// What the recording says player `k` of `side` is doing, as the figure needs
/// it: the action with its progress and lift blended to the next frame, and the
/// recorded ball in his own frame when nobody holds it.
#[allow(clippy::too_many_arguments)]
fn recorded_act(
    frame: &Frame,
    next: Option<(&Frame, f32)>,
    (side, k): (usize, usize),
    p: &Player,
    pos: Vec2,
    face: Vec2,
    ball: Vec3,
    holds: bool,
    hoop: Vec2,
) -> Option<view3d::Act> {
    let a = &frame.pose[side][k];
    let (mut u, mut lift) = (a.progress as f32 / 100.0, a.lift());
    if let Some((b, t)) = next {
        let pb = &b.pose[side][k];
        // Same player in the same movement: glide the progress and the height.
        if b.who[side][k] == frame.who[side][k] && pb.action == a.action && pb.stage == a.stage {
            u += (pb.progress as f32 / 100.0 - u) * t;
        }
        if b.who[side][k] == frame.who[side][k] {
            lift += (pb.lift() - lift) * t;
        }
    }
    let local_hoop = view3d::to_body(pos, face, v3(hoop.x, hoop.y, 0.0));
    Some(view3d::Act {
        action: a.action,
        style: a.style,
        stage: a.stage,
        u,
        lift,
        reach: p.body.standing_reach_cm / 100.0,
        holds,
        ball: (frame.holder.is_none()).then(|| view3d::to_body(pos, face, ball)),
        outside_left: local_hoop.y <= 0.0,
    })
}

/// The court in 3D: a flat-shaded mesh built and sorted on the CPU, with the
/// player's names and the carrier's ring on top, as in the other views.
#[allow(clippy::needless_range_loop)]
fn paint_3d(
    ui: &mut Ui,
    rec: &Recording,
    time: f32,
    cam3: &mut Cam3,
    players: &[Player],
    result: &GameResult,
    picked: Option<PlayerId>,
) {
    let _perf = crate::perf::scope("3d: whole view");
    let (frame, pos, ball, ball_height, next) = blend(rec, time);
    let colors = [theme::ACCENT, theme::TEXT_DIM];
    let w = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        vec2(w, view_height(w, ViewMode::Three)),
        Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);

    // Drag to turn the orbit camera, scroll to zoom.
    if cam3.preset == Preset::Orbit && response.dragged() {
        let d = response.drag_delta();
        cam3.yaw -= d.x * 0.008;
        cam3.pitch = (cam3.pitch + d.y * 0.006).clamp(0.12, 1.45);
    }
    if cam3.preset == Preset::Orbit {
        response.clone().on_hover_cursor(egui::CursorIcon::Grab);
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            cam3.zoom = (cam3.zoom * (scroll * 0.0015).exp()).clamp(0.5, 4.0);
            // The page does not scroll while the wheel is zooming.
            ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
        }
    }

    let ball3 = v3(ball.x, ball.y, ball_height);
    let dt = ui.input(|i| i.stable_dt).min(0.1);
    let cam = cam3.update(ball3, dt);
    if cam3.settling(ball3) {
        ui.ctx().request_repaint();
    }

    // Gait of everybody at this time, from tables kept per recording.
    let stride = |side: usize, slot: u8| {
        let p = &players[result.boxes[side][slot as usize].0 as usize];
        let d = Dims::new(&p.body, &Stats::of(p));
        (d.leg, d.stride_factor)
    };
    let gaits = {
        let _p = crate::perf::scope("3d: gait");
        view3d::gait::sample(ui.ctx(), rec, time, &stride)
    };
    let mut scene = Scene::new(cam.eye);
    let hall = {
        let _p = crate::perf::scope("3d: arena build");
        view3d::arena::begin(&mut scene, ui.ctx(), &cam)
    };
    let _figs = crate::perf::scope("3d: court+figures");
    view3d::court(&mut scene);
    hall.decals(&mut scene);
    let mut who: Vec<(usize, usize, &Player)> = Vec::with_capacity(10);
    for side in 0..2 {
        for k in 0..5 {
            let slot = frame.who[side][k] as usize;
            who.push((side, k, &players[result.boxes[side][slot].0 as usize]));
        }
    }
    for &(side, k, p) in &who {
        let at = (pos[side][k].x, pos[side][k].y);
        let broad = p.body.shoulder_width_cm / 100.0;
        scene.ground_disc(at, broad * 0.9, broad * 0.75, Color32::from_black_alpha(60));
        if frame.holder == Some((side as u8, k as u8)) {
            view3d::floor_ring(&mut scene, at, 0.62, 0.07, BALL);
        }
        if picked == Some(p.id) {
            view3d::floor_ring(&mut scene, at, 0.85, 0.08, theme::WARN);
        }
    }
    // A ball that has just gone through pulls the net of that basket.
    let swish = recent_swish(rec, time);
    let mut pull = [0.0f32; 2];
    if let Some((end, age, _)) = swish {
        pull[end] = (std::f32::consts::PI * (age / 0.5).clamp(0.0, 1.0)).sin();
    }
    view3d::baskets_with(&mut scene, pull);
    // The ball is in the carrier's hands while the recording has him holding
    // it; otherwise it is where it was recorded, on the path the engine gave it.
    let mut ball_at = v3(ball.x, ball.y, ball_height);
    let hoop = |off: u8| {
        if off == 0 {
            Vec2 { x: LENGTH - HOOP_X, y: 7.5 }
        } else {
            Vec2 { x: HOOP_X, y: 7.5 }
        }
    };
    for &(side, k, p) in &who {
        let holds = frame.holder == Some((side as u8, k as u8));
        let act = recorded_act(frame, next, (side, k), p, pos[side][k], gaits[side][k].face, ball3, holds, hoop(frame.offense));
        let at = view3d::emit_figure(
            &mut scene,
            &FigureSpec {
                body: &p.body,
                stats: Stats::of(p),
                id: p.id,
                pos: pos[side][k],
                gait: gaits[side][k],
                team: colors[side],
                holds_ball: holds,
                act,
            },
        );
        if let Some(at) = at {
            ball_at = at;
        }
    }
    view3d::ball(&mut scene, (ball_at.x, ball_at.y), ball_at.z);
    drop(_figs);
    painter.rect_filled(rect, 4.0, hall.backdrop);
    {
        let _p = crate::perf::scope("3d: project+sort");
        painter.add(Shape::mesh(scene.mesh(&cam, rect)));
    }
    if let Some((end, age, points)) = swish {
        paint_points(&painter, &cam, rect, end, age, points);
    }

    // Names over heads, and the players as links.
    let hover = ui.input(|i| i.pointer.hover_pos());
    let mut label: Option<(Pos2, String)> = None;
    for &(side, k, p) in &who {
        let (x, y) = (pos[side][k].x, pos[side][k].y);
        let tall = p.body.height_cm / 100.0;
        let (Some((head, depth)), Some((foot, _))) = (
            cam.project(v3(x, y, tall + 0.3), rect),
            cam.project(v3(x, y, 0.0), rect),
        ) else {
            continue;
        };
        let scale = cam.scale(depth, rect);
        let size = (scale * 0.32).min(15.0);
        if size >= 7.0 && rect.contains(head) {
            let text = initials(&p.name);
            let font = FontId::new(size, theme::mono_semibold());
            painter.text(
                head + vec2(1.0, 1.0),
                Align2::CENTER_BOTTOM,
                &text,
                font.clone(),
                Color32::from_black_alpha(200),
            );
            painter.text(head, Align2::CENTER_BOTTOM, text, font, Color32::WHITE);
        }
        let top = cam.project(v3(x, y, tall), rect).map_or(head, |t| t.0);
        let target = Rect::from_two_pos(top, foot)
            .expand2(vec2(scale * 0.35, 2.0))
            .intersect(rect);
        if target.is_positive() {
            let id = ui.scope_id().with(("disc", side, k));
            if ui
                .interact(target, id, Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                follow(ui, Link::Player(p.id));
            }
            if hover.is_some_and(|h| target.contains(h)) {
                let text = format!("{} ({})", p.name, p.position.label());
                label = Some((head - vec2(0.0, size + 4.0), text));
            }
        }
    }
    if let Some((at, text)) = label {
        let galley =
            painter.layout_no_wrap(text, FontId::new(13.0, theme::semibold()), theme::TEXT);
        let tag = Rect::from_center_size(
            pos2(at.x, at.y - galley.size().y / 2.0),
            galley.size() + vec2(12.0, 6.0),
        );
        let shift = vec2(
            (rect.left() + 2.0 - tag.left()).max(0.0) + (rect.right() - 2.0 - tag.right()).min(0.0),
            (rect.top() + 2.0 - tag.top()).max(0.0),
        );
        let tag = tag.translate(shift);
        painter.rect_filled(tag, 4.0, Color32::from_white_alpha(235));
        painter.galley(tag.center() - galley.size() / 2.0, galley, theme::TEXT);
    }

    // Camera buttons over the picture.
    let bar = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(rect.width() - 16.0, 26.0));
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bar)
            .layout(egui::Layout::left_to_right(egui::Align::Min)),
    );
    row.set_clip_rect(rect);
    for preset in Preset::ALL {
        let on = cam3.preset == preset;
        let mut button = egui::Button::selectable(on, preset.label());
        if !on {
            // Readable over the dark picture.
            button = button.fill(Color32::from_rgba_unmultiplied(250, 248, 243, 215));
        }
        if row.add(button).clicked() {
            cam3.preset = preset;
        }
    }
    let hint = if cam3.preset == Preset::Orbit {
        "Drag to turn, scroll to zoom"
    } else {
        "Scroll to zoom"
    };
    painter.text(
        rect.left_bottom() + vec2(10.0, -8.0),
        Align2::LEFT_BOTTOM,
        hint,
        FontId::new(11.0, theme::semibold()),
        Color32::from_white_alpha(150),
    );
}

impl App {
    pub(crate) fn match_screen_shell(&mut self, ui: &mut Ui, bd: &Backdrop, stage: Rect, tier: crate::shell::Tier) {
        let _perf = crate::perf::scope("match page");
        if let Some(mut live) = self.live.take() {
            {
                let _p = crate::perf::scope("live: step engine");
                self.run_live(ui, &mut live);
            }
            let result = {
                let _p = crate::perf::scope("live: result()");
                live.game.result()
            };
            let mut commands = Vec::new();
            self.match_hud(
                ui,
                bd,
                stage,
                tier,
                live.fixture,
                live.game.recording(),
                &result,
                Some((&live, &mut commands)),
            );
            for c in commands {
                self.command(&mut live, c);
            }
            if live.game.is_over() {
                // Book it; from here on this screen is the replay.
                self.last_game = Some(live.fixture);
                self.touchline = Touchline::default();
                self.world.full_time(live);
            } else {
                self.live = Some(live);
            }
            return;
        }

        // The recording is taken out while it is on screen, so the screen
        // can borrow the rest of the world freely.
        let replay = self.world.replay.take();
        let shown = replay
            .as_ref()
            .filter(|(i, rec)| Some(*i) == self.last_game && !rec.frames.is_empty())
            .and_then(|(i, rec)| Some((*i, rec, self.world.fixtures[*i].result.clone()?)));
        if let Some((fixture, rec, result)) = shown {
            let duration = rec.duration();
            let pb = &mut self.playback;
            if pb.playing {
                let dt = ui.input(|i| i.stable_dt).min(0.1);
                pb.time += dt * pb.speed;
                ui.ctx().request_repaint();
            }
            if pb.time >= duration {
                pb.time = duration;
                pb.playing = false;
            }
            self.match_hud(ui, bd, stage, tier, fixture, rec, &result, None);
        } else {
            // No game to show yet: the hall, and a word about what to do.
            let full = ui.ctx().content_rect();
            self.sh.stage.hall(ui, crate::scenes::Kind::Home, &self.world, self.team, full, full);
            let w = stage.width().min(460.0);
            let r = Rect::from_min_size(stage.min, vec2(w, 150.0));
            crate::shell::panel(ui, r, "match_none", |ui| {
                bd.card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    theme::eyebrow(ui, "Match");
                    ui.label(
                        RichText::new("Play your next match to coach it from here.")
                            .color(theme::TEXT_DIM),
                    );
                });
            });
        }
        self.world.replay = replay;
    }

    /// Let the live game catch up with the clock on screen, stopping where
    /// the coach gets a say.
    fn run_live(&mut self, ui: &Ui, live: &mut LiveRound) {
        let pb = &mut self.playback;
        if pb.playing {
            let dt = ui.input(|i| i.stable_dt).min(0.1);
            pb.time += dt * pb.speed;
            self.touchline.notice = None;
            ui.ctx().request_repaint();
        }
        while !live.game.is_over()
            && (live.game.time() < pb.time || live.game.recording().frames.is_empty())
        {
            let notice = match live.step() {
                Some(Pause::Timeout(side)) if side == live.side => {
                    "Timeout. Make your changes, then resume.".to_string()
                }
                Some(Pause::PeriodEnd) => {
                    let n = live.game.result().periods.len();
                    let periods = self.world.rules.periods as usize;
                    if n >= periods {
                        "Level at the end. Overtime is next.".to_string()
                    } else if n * 2 == periods {
                        "Half time. Make your changes, then resume.".to_string()
                    } else {
                        format!("End of period {n}. Make your changes, then resume.")
                    }
                }
                _ => continue,
            };
            self.touchline.notice = Some(notice);
            pb.playing = false;
            pb.time = live.game.time();
            break;
        }
        if pb.time >= live.game.time() {
            pb.time = live.game.time();
            if live.game.is_over() {
                pb.playing = false;
            }
        }
    }

    fn command(&mut self, live: &mut LiveRound, command: Command) {
        match command {
            Command::Timeout => {
                live.input(LiveInput::Timeout);
            }
            Command::AutoSubs(on) => {
                live.input(LiveInput::AutoSubs(on));
            }
            Command::Swap(off, on) => {
                live.input(LiveInput::Swap { off, on });
            }
            Command::CancelSwap(n) => {
                live.input(LiveInput::CancelSwap(n));
            }
            Command::Shout(shout, target) => {
                let reactions = live.input(LiveInput::Shout { shout, target });
                self.touchline.show(shout, reactions);
            }
            Command::Orders(o) => {
                let familiarity =
                    self.world.teams[self.team].familiarity_with(o.offense, o.defense, o.press);
                live.input(LiveInput::Orders {
                    orders: o,
                    familiarity,
                });
            }
            Command::SkipPeriod => {
                while !live.game.is_over() {
                    if matches!(live.step(), Some(Pause::PeriodEnd | Pause::Final)) {
                        break;
                    }
                }
                self.playback.time = live.game.time();
                self.playback.playing = false;
                self.touchline.notice = (!live.game.is_over())
                    .then(|| "Skipped to the break. Make your changes, then resume.".to_string());
            }
            Command::SkipAll => {
                live.input(LiveInput::AutoSubs(true));
                while !live.game.is_over() {
                    live.step();
                }
                self.playback.time = live.game.time();
                self.playback.playing = false;
            }
        }
    }

    /// The user's twelve: who is on, how they are holding up, and the
    /// buttons to change it.
    fn bench_card(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        live: &LiveRound,
        bench: &[LivePlayer],
        commands: &mut Vec<Command>,
    ) {
        let side = live.side;
        let limit = self.world.rules.foul_limit;
        let queued = live.game.queued(side);
        let who = |pid: PlayerId| self.world.players[pid as usize].name.as_str();
        let off = &mut self.touchline.off;
        if off.is_some_and(|id| !bench.iter().any(|p| p.id == id && p.on_court)) {
            *off = None;
        }
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                theme::eyebrow(ui, "Your bench");
                ui.add_space(12.0);
                let mut auto = live.game.auto_subs(side);
                if ui
                    .checkbox(&mut auto, "Assistant makes the substitutions")
                    .changed()
                {
                    commands.push(Command::AutoSubs(auto));
                }
            });
            ui.label(
                RichText::new(if live.game.auto_subs(side) {
                    "Make a swap yourself and the assistant leaves the substitutions to you."
                } else {
                    "Take off, then Send on. The swap is made at the next dead ball."
                })
                .color(theme::TEXT_DIM),
            );
            ui.add_space(6.0);
            egui::ScrollArea::horizontal()
                .id_salt("bench_scroll")
                .show(ui, |ui| {
                    egui::Grid::new("bench")
                        .striped(true)
                        .spacing(vec2(10.0, 7.0))
                        .show(ui, |ui| {
                            for on_court in [true, false] {
                                // The swap button comes first in the row, so
                                // it is on screen however narrow the panel:
                                // the numbers can scroll, the button cannot.
                                ui.label("");
                                ui.label(theme::eyebrow_text(if on_court {
                                    "On the floor"
                                } else {
                                    "Bench"
                                }));
                                if on_court {
                                    header_row(
                                        ui,
                                        &[
                                            "Pos", "Energy", "PF", "Pts", "Min", "Reb", "Ast",
                                            "Rtg", "",
                                        ],
                                    );
                                }
                                ui.end_row();
                                for p in bench.iter().filter(|p| p.on_court == on_court) {
                                    let info = &self.world.players[p.id as usize];
                                    let going = queued.iter().any(|q| q.0 == p.id || q.1 == p.id);
                                    if going {
                                        ui.label(
                                            RichText::new(if on_court { "off next" } else { "on next" })
                                                .color(theme::ACCENT),
                                        );
                                    } else if on_court {
                                        let picked = *off == Some(p.id);
                                        let button = egui::Button::selectable(picked, "Take off");
                                        if ui.add(button).clicked() {
                                            *off = (!picked).then_some(p.id);
                                        }
                                    } else if let Some(leaving) = *off {
                                        let button = egui::Button::new("Send on");
                                        if ui.add_enabled(!p.fouled_out, button).clicked() {
                                            commands.push(Command::Swap(leaving, p.id));
                                            *off = None;
                                        }
                                    } else {
                                        ui.label("");
                                    }
                                    let text = name(&info.name);
                                    crate::profile::player_link(
                                        ui,
                                        p.id,
                                        if on_court {
                                            text
                                        } else {
                                            text.color(theme::TEXT_DIM)
                                        },
                                    );
                                    ui.label(
                                        theme::mono(info.position.label()).color(theme::ACCENT),
                                    );
                                    ui.add(
                                        egui::ProgressBar::new(p.energy)
                                            .desired_width(56.0)
                                            .desired_height(8.0)
                                            .fill(match p.energy {
                                                e if e >= 0.65 => theme::POS,
                                                e if e < 0.4 => theme::WARN,
                                                _ => theme::TEXT_MUTE,
                                            }),
                                    )
                                    .on_hover_text(format!("{:.0}%", p.energy * 100.0));
                                    let fouls = theme::mono(p.line.pf.to_string());
                                    ui.label(if p.line.pf + 1 >= limit {
                                        fouls.color(theme::WARN)
                                    } else {
                                        fouls
                                    });
                                    ui.label(theme::mono(p.line.pts.to_string()));
                                    ui.label(theme::mono((p.line.secs / 60).to_string()));
                                    for v in [p.line.reb(), p.line.ast] {
                                        ui.label(theme::mono(v.to_string()));
                                    }
                                    if p.line.secs > 0 {
                                        ui.label(crate::pages::match_rating(p.line.rating()));
                                    } else {
                                        ui.label(theme::mono("-").color(theme::TEXT_MUTE));
                                    }
                                    let (text, color) = mood(p);
                                    ui.label(RichText::new(text).color(color));
                                    ui.end_row();
                                }
                            }
                        });
                    ui.add_space(14.0);
                });
            for (n, (leaving, coming)) in queued.iter().enumerate() {
                // Cancel first, for the same reason as the swap buttons.
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button("Cancel").clicked() {
                        commands.push(Command::CancelSwap(n));
                    }
                    ui.label(format!(
                        "Next dead ball: {} on for {}",
                        who(*coming),
                        who(*leaving)
                    ));
                });
            }
        });
    }

    /// Timeouts and shouts.
    fn shout_card(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        live: &LiveRound,
        bench: &[LivePlayer],
        commands: &mut Vec<Command>,
    ) {
        let side = live.side;
        let who = |pid: PlayerId| self.world.players[pid as usize].name.as_str();
        let bonus = self.world.rules.bonus_after;
        let target = &mut self.touchline.target;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Touchline");
            ui.add_space(4.0);
            let left = live.game.timeouts_left(side);
            ui.horizontal_wrapped(|ui| {
                let label = if live.game.timeout_wanted(side) {
                    "Wave the timeout off"
                } else {
                    "Call timeout"
                };
                if theme::primary_button(ui, left > 0, label) {
                    commands.push(Command::Timeout);
                }
                ui.label(theme::mono(format!("{left} left")).color(theme::TEXT_DIM));
            });
            ui.label(
                RichText::new(if live.game.timeout_wanted(side) {
                    "Taken at the next dead ball."
                } else {
                    "A minute's rest for the five on the floor, and it settles a team \
                     that is being run over."
                })
                .color(theme::TEXT_DIM),
            );
            let fouls = [live.game.team_fouls(side), live.game.team_fouls(1 - side)];
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    theme::mono(format!("team fouls {} / {}", fouls[0], fouls[1]))
                        .color(theme::TEXT_DIM),
                );
                if fouls[0] >= bonus {
                    ui.label(RichText::new("your next foul is two shots").color(theme::WARN));
                } else if fouls[1] >= bonus {
                    ui.label(RichText::new("their next foul is two shots").color(theme::POS));
                }
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label("Shout at");
                egui::ComboBox::from_id_salt("shout_target")
                    .width(210.0)
                    .selected_text(target.map_or("The five on the floor", who))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(target, None, "The five on the floor");
                        for on_court in [true, false] {
                            for p in bench.iter().filter(|p| p.on_court == on_court) {
                                let label = if on_court {
                                    who(p.id).to_string()
                                } else {
                                    format!("{} (bench)", who(p.id))
                                };
                                ui.selectable_value(target, Some(p.id), label);
                            }
                        }
                    });
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for shout in Shout::ALL {
                    if ui
                        .button(shout.label())
                        .on_hover_text(shout.description())
                        .clicked()
                    {
                        commands.push(Command::Shout(shout, *target));
                    }
                }
            });
            if live.game.voice(side) > 0.55 {
                ui.label(
                    RichText::new(
                        "You have been shouting a lot. It lands softer until you give it \
                         a few minutes.",
                    )
                    .color(theme::WARN),
                );
            }
            if let Some((shout, reactions)) = &self.touchline.said {
                ui.add_space(6.0);
                ui.label(theme::eyebrow_text(shout.label()));
                for (pid, reaction) in reactions {
                    let color = match reaction {
                        r if r.is_bad() => theme::WARN,
                        Reaction::Unmoved => theme::TEXT_MUTE,
                        _ => theme::POS,
                    };
                    ui.horizontal(|ui| {
                        ui.label(who(*pid));
                        ui.label(RichText::new(reaction.label()).color(color));
                    });
                }
            }
        });
    }

    /// The system, changeable from one dead ball to the next.
    fn orders_card(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        live: &LiveRound,
        commands: &mut Vec<Command>,
    ) {
        let before = live.game.orders(live.side);
        let mut o = before;
        let team = &self.world.teams[self.team];
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Orders");
            ui.add_space(4.0);
            egui::Grid::new("live_orders")
                .num_columns(2)
                .spacing(vec2(14.0, 8.0))
                .show(ui, |ui| {
                    ui.label("Offense");
                    egui::ComboBox::from_id_salt("live_offense")
                        .width(170.0)
                        .selected_text(o.offense.label())
                        .show_ui(ui, |ui| {
                            for v in OffenseSet::ALL {
                                ui.selectable_value(&mut o.offense, v, v.label());
                            }
                        });
                    ui.end_row();
                    ui.label("Defense");
                    egui::ComboBox::from_id_salt("live_defense")
                        .width(170.0)
                        .selected_text(o.defense.label())
                        .show_ui(ui, |ui| {
                            for v in DefenseScheme::ALL {
                                ui.selectable_value(&mut o.defense, v, v.label());
                            }
                        });
                    ui.end_row();
                    ui.label("Press");
                    egui::ComboBox::from_id_salt("live_press")
                        .width(170.0)
                        .selected_text(o.press.label())
                        .show_ui(ui, |ui| {
                            for v in Press::ALL {
                                ui.selectable_value(&mut o.press, v, v.label());
                            }
                        });
                    ui.end_row();
                    ui.spacing_mut().slider_width = 150.0;
                    for (label, value) in [
                        ("Pace", &mut o.pace),
                        ("Threes", &mut o.three_bias),
                        ("Crash boards", &mut o.crash_boards),
                        ("Aggression", &mut o.aggression),
                    ] {
                        ui.label(label);
                        ui.add(egui::Slider::new(value, 1..=20));
                        ui.end_row();
                    }
                });
            let known = team.familiarity_with(o.offense, o.defense, o.press);
            let text = format!("The squad knows this system {:.0}%.", known * 100.0);
            ui.label(RichText::new(text).color(if known < 0.5 {
                theme::WARN
            } else {
                theme::TEXT_DIM
            }));
            ui.label(
                RichText::new("Changes come in at the next dead ball.").color(theme::TEXT_DIM),
            );
        });
        if o != before {
            commands.push(Command::Orders(o));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use basketman_core::court::v;

    #[test]
    fn the_curve_passes_through_the_recorded_points() {
        let (p0, p1, p2, p3) = (v(0.0, 0.0), v(1.0, 2.0), v(3.0, 3.0), v(6.0, 2.0));
        assert!(curve(p0, p1, p2, p3, 0.0).dist(p1) < 1e-5);
        assert!(curve(p0, p1, p2, p3, 1.0).dist(p2) < 1e-5);
    }

    #[test]
    fn a_straight_run_stays_straight_and_even() {
        let p = |t: f32| v(2.0 * t, 1.0 + t);
        for u in [0.25, 0.5, 0.8] {
            let got = curve(p(0.0), p(1.0), p(2.0), p(3.0), u);
            assert!(got.dist(p(1.0 + u)) < 1e-4, "{got:?}");
        }
    }

    /// A player running an arc is drawn nearer the arc than a straight
    /// line between two recorded positions would put him.
    #[test]
    fn a_curved_run_is_followed_more_closely_than_by_straight_pieces() {
        // Four points on a circle of radius 5, about a quarter turn apart
        // in total: (cos, sin) written out, since the sim avoids trig.
        let on = [v(5.0, 0.0), v(4.33, 2.5), v(2.5, 4.33), v(0.0, 5.0)];
        let mid = curve(on[0], on[1], on[2], on[3], 0.5);
        let chord = on[1].lerp(on[2], 0.5);
        let off = |p: Vec2| (p.len() - 5.0).abs();
        assert!(
            off(mid) < off(chord) * 0.3,
            "{} vs {}",
            off(mid),
            off(chord)
        );
    }
}
