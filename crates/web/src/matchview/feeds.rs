//! The data tabs of the match drawer: the other games and the live table,
//! the shot chart, and how the game is flowing. Everything shown is read
//! from the simulation through `basketman_core` (`live_feed`, `shotchart`,
//! `game_flow`, `observations`); the tabs only draw it. The heavy counting
//! is cached and redone when the game has moved on, not every frame.

use basketman_core::engine::{GameResult, Orders};
use basketman_core::game_flow::{self, Factors, Flow, Form, Heat, MarkKind, Matchup, Read};
use basketman_core::league::{LiveRound, World};
use basketman_core::live_feed::{GameNow, LiveRow, Now, Status, Zone as TableZone};
use basketman_core::observations::{Note, Observer, Setting, SuggestedInput};
use basketman_core::player::PlayerId;
use basketman_core::replay::Recording;
use basketman_core::shotchart::{self, Shot, Zone};
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, pos2, vec2,
};

use super::Command;
use crate::nav::{Link, follow};
use crate::theme::{self, Backdrop};

/// Which team's shots the chart shows.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub(crate) enum Team {
    #[default]
    Both,
    Us,
    Them,
}

/// What the data tabs remember between frames.
#[derive(Default)]
pub(crate) struct Feeds {
    /// What the assistant has said this game, oldest first.
    pub notes: Vec<Note>,
    observer: Observer,
    observed_at: f32,
    fixture: Option<usize>,
    cache: Option<Cache>,
    pub team: Team,
    pub period: Option<u8>,
    pub player: Option<PlayerId>,
    /// The shot picked on the chart (event index).
    pub pick: Option<usize>,
    /// A replay shows the shots so far rather than the whole game.
    pub to_here: bool,
    /// Which division's table is shown; `None` is the user's own.
    pub division: Option<usize>,
    /// Scores last seen per fixture and the change since, for the marks.
    seen: Vec<Seen>,
    /// Replay time to jump to, asked for by a tap on the chart.
    pub jump: Option<f32>,
}

struct Seen {
    fixture: usize,
    score: [u16; 2],
    /// Real time of the last change and what it was.
    at: f64,
    delta: [u16; 2],
}

/// The counted state of the game at a moment.
struct Cache {
    fixture: usize,
    events: usize,
    bucket: i32,
    me: usize,
    shots: Vec<Shot>,
    flow: Flow,
    factors: [Factors; 2],
    forms: Vec<Form>,
    matchups: Vec<Matchup>,
}

/// What a tab needs to know about the game on screen.
#[derive(Clone, Copy)]
pub(crate) struct Ctx<'a> {
    pub world: &'a World,
    pub rec: &'a Recording,
    pub result: &'a GameResult,
    pub fixture: usize,
    /// The user's side, 0 = home.
    pub me: usize,
    /// Replay time on screen.
    pub time: f32,
    pub live: Option<&'a LiveRound>,
    pub colors: [Color32; 2],
    pub teams: &'a [String; 2],
    /// Wall-clock seconds, for marks that fade.
    pub wall: f64,
    /// Shown in the film room, where a tap sets the replay but does not
    /// play it.
    pub film: bool,
}

const OURS: Color32 = theme::ACCENT;

impl Ctx<'_> {
    fn ids(&self) -> [Vec<PlayerId>; 2] {
        [0, 1].map(|s| self.result.boxes[s].iter().map(|b| b.0).collect())
    }

    fn name(&self, p: PlayerId) -> &str {
        &self.world.players[p as usize].name
    }

    fn short(&self, p: PlayerId) -> String {
        // First initial and surname: two Carters on one squad are common.
        let n = self.name(p);
        match n.split_once(' ') {
            Some((first, rest)) => format!("{}. {}", first.chars().next().unwrap_or('?'), rest),
            None => n.to_string(),
        }
    }

    /// The clock on screen.
    fn now(&self) -> Now {
        let f = self.rec.frames.get(self.rec.frame_at(self.time));
        Now {
            period: f.map_or(0, |f| f.period),
            clock: f.map_or(0.0, |f| f.clock),
            score: f.map_or([0, 0], |f| f.score),
            over: self.live.is_none_or(|l| l.game.is_over()),
        }
    }
}

impl Feeds {
    fn reset_for(&mut self, c: &Ctx) {
        if self.fixture != Some(c.fixture) || self.observed_at > c.rec.duration() + 5.0 {
            *self = Feeds {
                fixture: Some(c.fixture),
                team: self.team,
                to_here: self.to_here,
                ..Feeds::default()
            };
        }
    }

    /// Forget what belongs to another game: the screen is on `c`'s now.
    pub fn sync(&mut self, c: &Ctx) {
        self.reset_for(c);
    }

    /// Count the game up to the time on screen, when it has moved on.
    fn counted(&mut self, c: &Ctx) -> &Cache {
        self.reset_for(c);
        let events = c.rec.events.partition_point(|e| e.time <= c.time);
        let bucket = c.time as i32;
        let fresh = self.cache.as_ref().is_some_and(|k| {
            k.fixture == c.fixture
                && k.me == c.me
                && k.events == events
                && (k.bucket == bucket || c.live.is_none())
        });
        if !fresh {
            let ids = c.ids();
            let live = c
                .live
                .map(|l| [l.game.players(0), l.game.players(1)]);
            let read = Read {
                rec: c.rec,
                ids: &ids,
                me: c.me,
                time: c.time,
                live: live.as_ref(),
            };
            let upto = read.upto();
            let mut shots = shotchart::shots(c.rec, &ids);
            shots.retain(|s| s.event < upto);
            self.cache = Some(Cache {
                fixture: c.fixture,
                events,
                bucket,
                me: c.me,
                shots,
                flow: read.flow(),
                factors: read.factors(),
                forms: read.forms(),
                matchups: read.matchups(),
            });
        }
        self.cache.as_ref().expect("counted above")
    }

    /// Let the assistant look at the game now and then, in a live game.
    pub fn tick(&mut self, c: &Ctx) {
        let Some(live) = c.live else { return };
        self.reset_for(c);
        if c.time < self.observed_at {
            // Looking back: the assistant speaks of the present only.
            return;
        }
        if c.time - self.observed_at < 2.0 {
            return;
        }
        self.observed_at = c.time;
        let ids = c.ids();
        let players = [live.game.players(0), live.game.players(1)];
        let read = Read {
            rec: c.rec,
            ids: &ids,
            me: c.me,
            time: c.time,
            live: Some(&players),
        };
        let name = |p: PlayerId| c.name(p).to_string();
        let set = Setting {
            name: &name,
            orders: [live.game.orders(0), live.game.orders(1)],
            timeouts_left: live.game.timeouts_left(c.me),
            timeout_wanted: live.game.timeout_wanted(c.me),
            foul_limit: c.world.rules.foul_limit,
        };
        if let Some(note) = self.observer.observe(&read, &set) {
            self.notes.push(note);
        }
    }
}

/// The live input a note asks for, as the touchline's command.
pub(crate) fn command_of(live: &LiveRound, s: &SuggestedInput) -> Command {
    match s {
        SuggestedInput::Defense(d) => Command::Orders(Orders {
            defense: *d,
            ..live.game.orders(live.side)
        }),
        other => command_of_kind(other).expect("only a defense needs the game's orders"),
    }
}

/// The commands that need nothing from the game.
fn command_of_kind(s: &SuggestedInput) -> Option<Command> {
    Some(match s {
        SuggestedInput::Timeout => Command::Timeout,
        SuggestedInput::Swap { off, on } => Command::Swap(*off, *on),
        SuggestedInput::Shout { shout, target } => Command::Shout(*shout, *target),
        SuggestedInput::Defense(_) => return None,
    })
}

// ---- small drawing helpers

fn mono(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Monospace)
}

fn semi(size: f32) -> FontId {
    FontId::new(size, theme::semibold())
}

fn triangle(p: &egui::Painter, at: Pos2, r: f32, up: bool, color: Color32) {
    let s = if up { -1.0 } else { 1.0 };
    p.add(Shape::convex_polygon(
        vec![
            pos2(at.x, at.y + s * r),
            pos2(at.x - r, at.y - s * r * 0.8),
            pos2(at.x + r, at.y - s * r * 0.8),
        ],
        color,
        Stroke::NONE,
    ));
}

fn diamond(p: &egui::Painter, at: Pos2, r: f32, fill: Color32, line: Color32) {
    p.add(Shape::convex_polygon(
        vec![
            pos2(at.x, at.y - r),
            pos2(at.x + r, at.y),
            pos2(at.x, at.y + r),
            pos2(at.x - r, at.y),
        ],
        fill,
        Stroke::new(1.0, line),
    ));
}

/// Text that fits `room`, or the initials of it.
fn fit(p: &egui::Painter, text: &str, font: FontId, color: Color32, room: f32) -> std::sync::Arc<egui::Galley> {
    let g = p.layout_no_wrap(text.to_string(), font.clone(), color);
    if g.size().x <= room {
        return g;
    }
    let short: String = text
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(3)
        .collect::<String>()
        .to_uppercase();
    p.layout_no_wrap(short, font, color)
}

/// A small toggle: a filter chip.
fn chip(ui: &mut Ui, text: &str, on: bool) -> bool {
    let font = if on { semi(12.0) } else { FontId::new(12.0, egui::FontFamily::Proportional) };
    let g = ui.painter().layout_no_wrap(text.to_string(), font, theme::TEXT);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 16.0, 26.0), Sense::click());
    let p = ui.painter();
    if on {
        p.rect_filled(rect, 8.0, theme::ink(26));
        p.rect_stroke(rect, 8.0, Stroke::new(1.0, theme::ink(70)), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        p.rect_filled(rect, 8.0, theme::ink(12));
    }
    p.galley(rect.center() - g.size() / 2.0, g, theme::TEXT);
    resp.clicked()
}

fn pct(x: Option<f32>) -> String {
    x.map_or("-".into(), |x| format!("{:.0}%", x * 100.0))
}

// ---- the assistant's notes in the feed

impl Feeds {
    /// One note as a line of the feed, with its button.
    pub fn note_line(
        &self,
        ui: &mut Ui,
        c: &Ctx,
        note: &Note,
        commands: &mut Option<&mut Vec<Command>>,
        big: bool,
    ) {
        let stale = c.time - note.time > 90.0;
        ui.horizontal_top(|ui| {
            ui.label(theme::mono(super::clock(note.clock)).color(theme::TEXT_MUTE));
            ui.vertical(|ui| {
                let t = RichText::new(format!("Assistant: {}", note.text))
                    .color(theme::ACCENT)
                    .italics();
                ui.add(egui::Label::new(if big { t.size(16.0) } else { t }).wrap());
                if let (Some(s), Some(cmds), Some(live)) =
                    (&note.suggestion, commands.as_deref_mut(), c.live)
                {
                    let ok = !stale && !live.game.is_over();
                    if ui
                        .add_enabled(ok, egui::Button::new(&s.label).small())
                        .clicked()
                    {
                        cmds.push(command_of(live, &s.input));
                    }
                }
            });
        });
    }
}

// ---- Scores

impl Feeds {
    pub fn scores_tab(&mut self, ui: &mut Ui, bd: &Backdrop, c: &Ctx) {
        let Some(live) = c.live else {
            ui.label(RichText::new("Other games are shown while a game is live.").color(theme::TEXT_DIM));
            return;
        };
        self.reset_for(c);
        let now = c.now();
        let games = live.games_now(c.world, now);
        let mine = c.world.my_division();
        // Marks for scores that moved.
        for g in &games {
            match self.seen.iter_mut().find(|s| s.fixture == g.fixture) {
                Some(s) => {
                    if s.score != g.score {
                        let d = [g.score[0].saturating_sub(s.score[0]), g.score[1].saturating_sub(s.score[1])];
                        if d != [0, 0] {
                            s.at = c.wall;
                            s.delta = d;
                        }
                        s.score = g.score;
                    }
                }
                None => self.seen.push(Seen {
                    fixture: g.fixture,
                    score: g.score,
                    at: f64::NEG_INFINITY,
                    delta: [0, 0],
                }),
            }
        }
        let mut divisions: Vec<usize> = games.iter().map(|g| g.division).collect();
        divisions.sort_by_key(|&d| (d != mine, d));
        divisions.dedup();
        let seen = &self.seen;
        let show = |ui: &mut Ui, d: usize, label: &str| {
            theme::eyebrow(ui, label);
            ui.add_space(2.0);
            for g in games.iter().filter(|g| g.division == d) {
                game_row(ui, c, g, seen.iter().find(|s| s.fixture == g.fixture));
            }
        };
        if let Some(&first) = divisions.first() {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                show(ui, first, &format!("Around the grounds / {}", c.world.division_name(first)));
            });
        }
        ui.add_space(6.0);
        let division = self.division.unwrap_or(mine);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                theme::eyebrow(ui, "Table as it stands");
            });
            if c.world.division_count() > 1 {
                ui.horizontal_wrapped(|ui| {
                    for d in 0..c.world.division_count() {
                        let label = if d == mine {
                            format!("{} (ours)", c.world.division_name(d))
                        } else {
                            c.world.division_name(d)
                        };
                        if chip(ui, &label, d == division) {
                            self.division = Some(d);
                        }
                    }
                });
            }
            ui.add_space(4.0);
            let rows = live.table_now(c.world, now, division);
            table(ui, c, &rows, c.world.table_lines(division));
        });
        for &d in divisions.iter().skip(1) {
            ui.add_space(6.0);
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                show(ui, d, &format!("Elsewhere / {}", c.world.division_name(d)));
            });
        }
    }
}

fn game_row(ui: &mut Ui, c: &Ctx, g: &GameNow, seen: Option<&Seen>) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 38.0), Sense::hover());
    let p = ui.painter_at(rect);
    if g.ours {
        p.rect_filled(rect, 8.0, theme::ink(10));
    }
    let cx = rect.center().x;
    let flash = seen
        .map(|s| ((c.wall - s.at) / 7.0) as f32)
        .filter(|a| *a < 1.0)
        .map(|a| 1.0 - a);
    let delta = seen.map_or([0, 0], |s| s.delta);
    let font = semi(12.5);
    let room = (w / 2.0 - 44.0).max(30.0);
    let home = fit(&p, &g.home_name, font.clone(), theme::TEXT, room);
    let away = fit(&p, &g.away_name, font, theme::TEXT, room);
    let y = rect.top() + 14.0;
    p.galley(pos2(cx - 36.0 - home.size().x, y - home.size().y / 2.0), home, theme::TEXT);
    p.galley(pos2(cx + 36.0, y - away.size().y / 2.0), away, theme::TEXT);
    for side in 0..2 {
        let x = cx + if side == 0 { -17.0 } else { 17.0 };
        let hot = flash.is_some() && delta[side] > 0;
        p.text(
            pos2(x, y),
            Align2::CENTER_CENTER,
            g.score[side].to_string(),
            FontId::new(15.0, if hot { theme::mono_semibold() } else { egui::FontFamily::Monospace }),
            if hot { OURS } else { theme::TEXT },
        );
    }
    p.text(pos2(cx, y), Align2::CENTER_CENTER, "-", mono(12.0), theme::TEXT_MUTE);
    // A changed score: the points, and a mark that fades.
    if let Some(a) = flash {
        for side in 0..2 {
            if delta[side] > 0 {
                let x = if side == 0 { rect.left() + 8.0 } else { rect.right() - 8.0 };
                let col = OURS.gamma_multiply(0.35 + 0.65 * a);
                triangle(&p, pos2(x, y), 4.5, true, col);
                p.text(
                    pos2(x + if side == 0 { 12.0 } else { -12.0 }, y),
                    if side == 0 { Align2::LEFT_CENTER } else { Align2::RIGHT_CENTER },
                    format!("+{}", delta[side]),
                    mono(10.5),
                    col,
                );
            }
        }
    }
    let status = match g.status {
        Status::Final => "FINAL".to_string(),
        Status::Live => format!("{} {}", period_name(c, g.period), super::clock(g.clock)),
    };
    p.text(
        pos2(cx, rect.top() + 29.0),
        Align2::CENTER_CENTER,
        status,
        mono(10.0),
        if g.status == Status::Live { theme::TEXT_DIM } else { theme::TEXT_MUTE },
    );
}

fn period_name(c: &Ctx, period: u8) -> String {
    let regular = c.world.rules.periods;
    if period < regular {
        format!("Q{}", period + 1)
    } else if period == regular {
        "OT".into()
    } else {
        format!("{}OT", period - regular + 1)
    }
}

fn table(ui: &mut Ui, c: &Ctx, rows: &[LiveRow], lines: basketman_core::live_feed::Lines) {
    let w = ui.available_width();
    let h = 22.0;
    let head = 16.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, head + h * rows.len() as f32 + 4.0), Sense::hover());
    let p = ui.painter_at(rect);
    let (c_place, c_move, c_name, c_wl, c_pd) = (
        rect.left() + 12.0,
        rect.left() + 38.0,
        rect.left() + 54.0,
        rect.right() - 66.0,
        rect.right() - 6.0,
    );
    for (x, a, t) in [
        (c_name, Align2::LEFT_CENTER, "CLUB"),
        (c_wl, Align2::CENTER_CENTER, "W-L"),
        (c_pd, Align2::RIGHT_CENTER, "+/-"),
    ] {
        p.text(pos2(x, rect.top() + 8.0), a, t, mono(9.5), theme::TEXT_MUTE);
    }
    let mut last_zone = TableZone::None;
    for (i, r) in rows.iter().enumerate() {
        let top = rect.top() + head + i as f32 * h;
        let mid = top + h / 2.0;
        let band = Rect::from_min_max(pos2(rect.left(), top), pos2(rect.right(), top + h));
        // A line between the zones, named.
        if r.zone != last_zone && i > 0 {
            p.line_segment(
                [pos2(rect.left(), top), pos2(rect.right(), top)],
                Stroke::new(1.0, theme::LINE_2),
            );
        }
        last_zone = r.zone;
        if r.ours {
            p.rect_filled(band.shrink2(vec2(0.0, 1.0)), 6.0, theme::ink(12));
        }
        // A tick at the left edge for the zone, drawn dashed when relegation
        // so it does not rest on colour.
        let (zc, tag) = match r.zone {
            TableZone::Promotion => (theme::POS, "UP"),
            TableZone::Playoffs => (OURS, "PO"),
            TableZone::Relegation => (theme::WARN, "DN"),
            TableZone::None => (Color32::TRANSPARENT, ""),
        };
        if !tag.is_empty() {
            p.rect_filled(Rect::from_min_max(pos2(rect.left(), top + 3.0), pos2(rect.left() + 3.0, top + h - 3.0)), 1.5, zc);
        }
        p.text(pos2(c_place, mid), Align2::CENTER_CENTER, r.place.to_string(), mono(12.0), theme::TEXT_DIM);
        // Places gained or lost today.
        if r.was != r.place {
            let up = r.place < r.was;
            let col = if up { theme::POS } else { theme::WARN };
            triangle(&p, pos2(c_move - 6.0, mid), 3.6, up, col);
            p.text(
                pos2(c_move + 1.0, mid),
                Align2::LEFT_CENTER,
                r.was.abs_diff(r.place).to_string(),
                mono(10.0),
                col,
            );
        }
        let live_tag = r.playing.map(|s| format!("  {}-{}", s[0], s[1]));
        let label = format!("{}{}", r.name, live_tag.unwrap_or_default());
        let room = c_wl - 22.0 - c_name;
        let g = fit(&p, &label, if r.ours { semi(12.5) } else { FontId::new(12.5, egui::FontFamily::Proportional) }, theme::TEXT, room);
        p.galley(pos2(c_name, mid - g.size().y / 2.0), g, theme::TEXT);
        if r.playing.is_some() {
            p.circle_filled(pos2(c_wl - 24.0, mid), 2.5, OURS);
        }
        p.text(pos2(c_wl, mid), Align2::CENTER_CENTER, format!("{}-{}", r.wins, r.losses), mono(11.5), theme::TEXT);
        let pd = r.points_for as i64 - r.points_against as i64;
        p.text(pos2(c_pd, mid), Align2::RIGHT_CENTER, format!("{pd:+}"), mono(11.5), theme::TEXT_DIM);
    }
    let _ = c;
    // The lines, in words.
    ui.add_space(2.0);
    let mut key = Vec::new();
    if lines.playoffs > 0 {
        key.push(format!("PO top {} play off", lines.playoffs));
    }
    if lines.up > 0 {
        key.push(format!("UP top {} promoted", lines.up));
    }
    if lines.down > 0 {
        key.push(format!("DN bottom {} relegated", lines.down));
    }
    if !key.is_empty() {
        ui.label(RichText::new(key.join("   ")).font(mono(10.0)).color(theme::TEXT_MUTE));
    }
    ui.label(RichText::new("A dot marks a game on now; its score is counted as if it ended.").size(crate::theme::fs(11.0)).color(theme::TEXT_MUTE));
}

// ---- Shots

/// The half court, attack frame: x across, y from the baseline.
struct Half {
    rect: Rect,
    scale: f32,
}

const HALF_LEN: f32 = 14.0;
const HALF_W: f32 = 15.0;

impl Half {
    fn new(rect: Rect) -> Half {
        let scale = (rect.width() / HALF_W).min(rect.height() / HALF_LEN);
        Half { rect, scale }
    }

    /// Screen point of court point `(x, y)`: baseline at the bottom.
    fn at(&self, x: f32, y: f32) -> Pos2 {
        let size = vec2(HALF_W, HALF_LEN) * self.scale;
        let min = self.rect.center() - size / 2.0;
        pos2(min.x + (x + HALF_W / 2.0) * self.scale, min.y + (HALF_LEN - y) * self.scale)
    }

    fn paint(&self, p: &egui::Painter) {
        use basketman_core::court::{CORNER_X, CORNER_Y, FT_RADIUS, HOOP, KEY_DEPTH, KEY_HALF_WIDTH, THREE_RADIUS};
        let line = Stroke::new(1.2, theme::ink(90));
        let soft = Stroke::new(1.0, theme::ink(45));
        let floor = Rect::from_two_pos(self.at(-7.5, 0.0), self.at(7.5, HALF_LEN));
        p.rect_filled(floor, 6.0, theme::white(120));
        p.rect_stroke(floor, 6.0, line, egui::StrokeKind::Inside);
        // The key.
        let key = Rect::from_two_pos(self.at(-KEY_HALF_WIDTH, 0.0), self.at(KEY_HALF_WIDTH, KEY_DEPTH));
        p.rect_filled(key, 0.0, theme::ink(8));
        p.rect_stroke(key, 0.0, line, egui::StrokeKind::Inside);
        // Free-throw circle: the top half solid, the bottom dashed.
        let ft: Vec<Pos2> = arc(self, 0.0, KEY_DEPTH, FT_RADIUS, 0.0, std::f32::consts::PI);
        p.add(Shape::line(ft, line));
        let ft_low: Vec<Pos2> = arc(self, 0.0, KEY_DEPTH, FT_RADIUS, std::f32::consts::PI, std::f32::consts::TAU);
        p.add(Shape::dashed_line(&ft_low, soft, 3.0, 3.0));
        // Restricted area, backboard and hoop.
        p.add(Shape::line(arc(self, 0.0, HOOP.y, 1.25, 0.0, std::f32::consts::PI), soft));
        p.line_segment([self.at(-0.9, 1.2), self.at(0.9, 1.2)], Stroke::new(2.0, theme::ink(120)));
        p.circle_stroke(self.at(0.0, HOOP.y), 0.225 * self.scale, Stroke::new(1.5, Color32::from_rgb(0xE8, 0x6A, 0x1C)));
        // The three-point line.
        let ang = ((CORNER_Y - HOOP.y) / THREE_RADIUS).asin();
        let mut three = vec![self.at(-CORNER_X, 0.0), self.at(-CORNER_X, CORNER_Y)];
        three.extend(arc(self, 0.0, HOOP.y, THREE_RADIUS, std::f32::consts::PI - ang, ang));
        three.push(self.at(CORNER_X, 0.0));
        p.add(Shape::line(three, line));
    }
}

/// Points of a circle of radius `r` around court point `(cx, cy)` from
/// angle `from` to `to` (either way round), in court angles: 0 is along +x.
fn arc(h: &Half, cx: f32, cy: f32, r: f32, from: f32, to: f32) -> Vec<Pos2> {
    let n = 40;
    (0..=n)
        .map(|i| {
            let a = from + (to - from) * i as f32 / n as f32;
            h.at(cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

fn mark(p: &egui::Painter, at: Pos2, made: bool, color: Color32, picked: bool) {
    if picked {
        p.circle_stroke(at, 9.0, Stroke::new(1.5, theme::TEXT));
    }
    if made {
        p.circle_filled(at, 4.6, color);
        p.circle_stroke(at, 4.6, Stroke::new(1.0, theme::white(220)));
    } else {
        let d = 3.8;
        let s = Stroke::new(1.8, color);
        p.line_segment([at + vec2(-d, -d), at + vec2(d, d)], s);
        p.line_segment([at + vec2(-d, d), at + vec2(d, -d)], s);
    }
}

impl Feeds {
    pub fn shots_tab(&mut self, ui: &mut Ui, bd: &Backdrop, c: &Ctx) {
        // A replay shows the whole game unless asked for the shots so far.
        let whole = Ctx {
            time: if c.live.is_none() && !self.to_here { f32::MAX } else { c.time },
            ..*c
        };
        let ids = c.ids();
        let c = &whole;
        let (me, avg) = (c.me, c.world.league_shooting());
        let periods = c.now().period;
        let team = self.team;
        let (period, player) = (self.period, self.player);
        let shots: Vec<Shot> = self
            .counted(c)
            .shots
            .iter()
            .filter(|s| match team {
                Team::Both => true,
                Team::Us => s.side as usize == me,
                Team::Them => s.side as usize != me,
            })
            .filter(|s| period.is_none_or(|p| s.period == p))
            .filter(|s| player.is_none_or(|p| s.who == p))
            .cloned()
            .collect();
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
                for (t, label) in [(Team::Both, "Both"), (Team::Us, "Us"), (Team::Them, "Them")] {
                    if chip(ui, label, self.team == t) {
                        self.team = t;
                        self.player = None;
                        self.pick = None;
                    }
                }
                if c.live.is_none() && chip(ui, "To here", self.to_here) {
                    self.to_here = !self.to_here;
                }
                ui.add_space(8.0);
                if chip(ui, "All", self.period.is_none()) {
                    self.period = None;
                }
                for q in 0..=periods {
                    if chip(ui, &period_name(c, q), self.period == Some(q)) {
                        self.period = Some(q);
                    }
                }
            });
            ui.horizontal(|ui| {
                let pool: Vec<PlayerId> = match team {
                    Team::Us => ids[me].clone(),
                    Team::Them => ids[1 - me].clone(),
                    Team::Both => ids[me].iter().chain(&ids[1 - me]).copied().collect(),
                };
                egui::ComboBox::from_id_salt("shots_player")
                    .width(150.0)
                    .selected_text(self.player.map_or("All players".to_string(), |p| c.name(p).to_string()))
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(self.player.is_none(), "All players").clicked() {
                            self.player = None;
                        }
                        for p in pool {
                            if ui.selectable_label(self.player == Some(p), c.name(p)).clicked() {
                                self.player = Some(p);
                            }
                        }
                    });
                ui.label(RichText::new(format!("{} shots", shots.len())).font(mono(11.0)).color(theme::TEXT_MUTE));
            });
            ui.add_space(4.0);
            // The court.
            let w = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(vec2(w, (w * HALF_LEN / HALF_W).min(if ui.ctx().content_rect().width() < 500.0 { 250.0 } else { 360.0 })), Sense::click());
            let half = Half::new(rect);
            let p = ui.painter_at(rect);
            half.paint(&p);
            let me_color = |side: u8| c.colors[side as usize];
            // Misses first so makes stay on top.
            for s in shots.iter().filter(|s| !s.made).chain(shots.iter().filter(|s| s.made)) {
                mark(&p, half.at(s.x, s.y), s.made, me_color(s.side), self.pick == Some(s.event));
            }
            if resp.clicked()
                && let Some(at) = resp.interact_pointer_pos()
            {
                let near = shots
                    .iter()
                    .map(|s| (half.at(s.x, s.y).distance(at), s))
                    .filter(|(d, _)| *d < 14.0)
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((_, s)) = near {
                    self.pick = Some(s.event);
                    self.jump = Some((s.time - 4.0).max(0.0));
                }
            }
            // Legend: shape as well as colour.
            ui.add_space(2.0);
            let (lg, _) = ui.allocate_exact_size(vec2(w, 18.0), Sense::hover());
            let lp = ui.painter_at(lg);
            let mut x = lg.left() + 8.0;
            mark(&lp, pos2(x, lg.center().y), true, theme::TEXT_DIM, false);
            x += 10.0;
            lp.text(pos2(x, lg.center().y), Align2::LEFT_CENTER, "made", mono(10.5), theme::TEXT_DIM);
            x += 50.0;
            mark(&lp, pos2(x, lg.center().y), false, theme::TEXT_DIM, false);
            x += 10.0;
            lp.text(pos2(x, lg.center().y), Align2::LEFT_CENTER, "missed", mono(10.5), theme::TEXT_DIM);
            x += 60.0;
            lp.circle_filled(pos2(x, lg.center().y), 4.0, c.colors[0]);
            x += 8.0;
            lp.text(pos2(x, lg.center().y), Align2::LEFT_CENTER, abbreviate(&c.teams[0]), mono(10.5), theme::TEXT_DIM);
            x += 54.0;
            lp.circle_filled(pos2(x, lg.center().y), 4.0, c.colors[1]);
            x += 8.0;
            lp.text(pos2(x, lg.center().y), Align2::LEFT_CENTER, abbreviate(&c.teams[1]), mono(10.5), theme::TEXT_DIM);
            // The shot picked.
            if let Some(s) = self.pick.and_then(|e| shots.iter().find(|s| s.event == e)) {
                ui.add_space(4.0);
                let text = format!(
                    "{} {} {}, {}, {:.1} m, {}{}.",
                    period_name(c, s.period),
                    super::clock(s.clock),
                    c.name(s.who),
                    s.zone.label().to_lowercase(),
                    s.distance(),
                    if s.made { "made" } else if s.blocked { "blocked" } else { "missed" },
                    if s.contest > 0.66 { ", tightly guarded" } else if s.contest < 0.25 { ", wide open" } else { "" },
                );
                ui.add(egui::Label::new(RichText::new(text).size(12.0).color(theme::TEXT)).wrap());
                let note = if c.film {
                    "The replay on the Match screen is set to just before it."
                } else {
                    "The replay has jumped to just before it."
                };
                ui.label(RichText::new(note).size(crate::theme::fs(11.0)).color(theme::TEXT_MUTE));
            }
        });
        ui.add_space(6.0);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "By zone");
            ui.add_space(4.0);
            let lines = shotchart::zones(shots.iter());
            egui::Grid::new("zones").num_columns(4).spacing([10.0, 4.0]).show(ui, |ui| {
                for h in ["", "Made", "FG%", "League"] {
                    ui.label(theme::eyebrow_text(h));
                }
                ui.end_row();
                for (z, l) in Zone::ALL.iter().zip(lines) {
                    ui.label(RichText::new(z.label()).size(12.5));
                    ui.label(theme::mono(format!("{}/{}", l.made, l.att)));
                    ui.label(theme::mono(pct(l.pct())));
                    let lg = avg.map(|a| shotchart::zone_average(a, *z));
                    let col = match (l.pct(), lg) {
                        (Some(a), Some(b)) if l.att >= 4 && a > b + 0.08 => theme::POS,
                        (Some(a), Some(b)) if l.att >= 4 && a < b - 0.08 => theme::WARN,
                        _ => theme::TEXT_MUTE,
                    };
                    ui.label(theme::mono(pct(lg)).color(col));
                    ui.end_row();
                }
            });
            ui.label(
                RichText::new("League: the season's two-point and three-point averages; no finer split is kept.")
                    .size(crate::theme::fs(10.5))
                    .color(theme::TEXT_MUTE),
            );
        });
    }
}

/// A club by its nickname: the last word of its name.
fn abbreviate(team: &str) -> String {
    team.rsplit(' ').next().unwrap_or(team).chars().take(9).collect()
}

// ---- Flow

impl Feeds {
    pub fn flow_tab(&mut self, ui: &mut Ui, bd: &Backdrop, c: &Ctx) {
        self.counted(c);
        let k = self.cache.as_ref().expect("counted");
        let (me, them) = (c.me, 1 - c.me);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Score margin");
            ui.add_space(2.0);
            flow_chart(ui, c, &k.flow);
            ui.add_space(2.0);
            for r in k.flow.runs.iter().rev().take(3) {
                let who = if r.side as usize == me { "We" } else { "They" };
                ui.label(
                    RichText::new(format!(
                        "{} {} {} ({} {}).",
                        period_name(c, r.period),
                        super::clock(r.clock),
                        format!("{who} went on a {}", r.label()),
                        "ongoing",
                        if r.on { "" } else { "ended" }
                    ).replace(" (ongoing ended)", " (ended)").replace(" (ongoing )", " (still going)"))
                    .size(12.0)
                    .color(theme::TEXT_DIM),
                );
            }
        });
        ui.add_space(6.0);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Four factors");
            ui.add_space(4.0);
            let f = &k.factors;
            let (a, b) = (&f[me], &f[them]);
            egui::Grid::new("factors").num_columns(3).spacing([14.0, 5.0]).show(ui, |ui| {
                ui.label("");
                ui.label(theme::eyebrow_text(&abbreviate(&c.teams[me])));
                ui.label(theme::eyebrow_text(&abbreviate(&c.teams[them])));
                ui.end_row();
                let row = |ui: &mut Ui, label: &str, x: Option<f32>, y: Option<f32>, high_good: bool| {
                    ui.label(RichText::new(label).size(12.5));
                    let better = match (x, y) {
                        (Some(x), Some(y)) if (x - y).abs() > 0.005 => Some(if high_good { x > y } else { x < y }),
                        _ => None,
                    };
                    for (v, mine) in [(x, better), (y, better.map(|b| !b))] {
                        let t = theme::mono(pct(v));
                        ui.label(if mine == Some(true) { t.strong().color(theme::TEXT) } else { t.color(theme::TEXT_DIM) });
                    }
                    ui.end_row();
                };
                row(ui, "Shooting (eFG%)", a.efg(), b.efg(), true);
                row(ui, "Turnovers per play", a.tov_rate(), b.tov_rate(), false);
                row(ui, "Offensive rebounds", game_flow::orb_rate(a, b), game_flow::orb_rate(b, a), true);
                row(ui, "Free throws per shot", a.ft_rate(), b.ft_rate(), true);
            });
            ui.label(RichText::new("Bold is the better of the two.").size(crate::theme::fs(10.5)).color(theme::TEXT_MUTE));
        });
        ui.add_space(6.0);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Hot and cold");
            ui.add_space(4.0);
            let mut rows: Vec<&Form> = k
                .forms
                .iter()
                .filter(|f| f.on_court == Some(true) || f.heat.is_some() || f.fouls >= 3)
                .filter(|f| f.side as usize == me || f.heat.is_some() || f.on_court == Some(true))
                .collect();
            rows.sort_by_key(|f| (f.side as usize != me, f.on_court != Some(true), f.id));
            if rows.is_empty() {
                ui.label(RichText::new("Nobody stands out yet.").color(theme::TEXT_DIM));
            }
            for f in rows {
                form_row(ui, c, f);
            }
        });
        if c.live.is_some() {
            ui.add_space(6.0);
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Matchups");
                ui.add_space(4.0);
                for m in &k.matchups {
                    let Some(g) = m.guards else {
                        ui.label(RichText::new(format!("{} is not on a man.", c.short(m.mine))).size(12.5).color(theme::TEXT_MUTE));
                        continue;
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(name(&c.short(m.mine)));
                        ui.label(RichText::new("on").color(theme::TEXT_MUTE));
                        if ui.add(egui::Label::new(name(&c.short(g)).color(theme::TEXT)).sense(Sense::click())).clicked() {
                            follow(ui, Link::Player(g));
                        }
                    });
                    let verdict = m.verdict().map_or(String::new(), |v| format!(", {v}"));
                    ui.label(
                        RichText::new(format!(
                            "He is {}/{} against him ({} pts), beaten {}x; {} has {} in all{verdict}.",
                            m.made_against, m.shots_against, m.points_against, m.beaten, c.short(g), m.his_points
                        ))
                        .size(crate::theme::fs(11.5))
                        .color(theme::TEXT_DIM),
                    );
                    ui.add_space(2.0);
                }
                ui.label(RichText::new("Who guards whom is where the bodies were over the last forty seconds.").size(crate::theme::fs(10.5)).color(theme::TEXT_MUTE));
            });
        }
    }
}

fn name(t: &str) -> RichText {
    crate::app::name(t)
}

fn form_row(ui: &mut Ui, c: &Ctx, f: &Form) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
    let p = ui.painter_at(rect);
    let mid = rect.center().y;
    let ours = f.side as usize == c.me;
    p.text(
        pos2(rect.left() + 2.0, mid),
        Align2::LEFT_CENTER,
        c.short(f.id),
        if ours { semi(12.5) } else { FontId::new(12.5, egui::FontFamily::Proportional) },
        if ours { theme::TEXT } else { theme::TEXT_DIM },
    );
    // Last five shots: a filled square made, a hollow one missed.
    let x0 = rect.left() + (w * 0.34).min(120.0);
    for (i, &made) in f.last.iter().enumerate() {
        let r = Rect::from_center_size(pos2(x0 + i as f32 * 10.0, mid), vec2(7.0, 7.0));
        if made {
            p.rect_filled(r, 1.5, theme::TEXT);
        } else {
            p.rect_stroke(r, 1.5, Stroke::new(1.2, theme::TEXT_MUTE), egui::StrokeKind::Inside);
        }
    }
    let tag = match f.heat {
        Some(Heat::Hot) => "HOT",
        Some(Heat::Cold) => "COLD",
        None => "",
    };
    let tx = x0 + 58.0;
    p.text(pos2(tx, mid), Align2::LEFT_CENTER, tag, mono(10.5), if f.heat == Some(Heat::Hot) { theme::POS } else { theme::WARN });
    // Fouls and energy on the right.
    let mut x = rect.right() - 4.0;
    if let Some(e) = f.energy.filter(|_| ours) {
        let bar = Rect::from_min_size(pos2(x - 44.0, mid - 3.0), vec2(44.0, 6.0));
        p.rect_filled(bar, 3.0, theme::ink(20));
        let fill = Rect::from_min_size(bar.min, vec2(bar.width() * e.clamp(0.0, 1.0), 6.0));
        p.rect_filled(fill, 3.0, if e < 0.3 { theme::WARN } else { theme::TEXT_DIM });
        x -= 52.0;
    }
    p.text(
        pos2(x, mid),
        Align2::RIGHT_CENTER,
        format!("{}F {}p", f.fouls, f.points),
        mono(10.5),
        if f.fouls >= 4 { theme::WARN } else { theme::TEXT_DIM },
    );
    if resp.clicked() {
        follow(ui, Link::Player(f.id));
    }
}

/// Position of a replay time on the chart's axis: whole periods.
fn game_x(c: &Ctx, time: f32) -> f32 {
    let Some(f) = c.rec.frames.get(c.rec.frame_at(time)) else { return 0.0 };
    let regular = c.world.rules.periods;
    let len = if f.period < regular { c.world.rules.period_secs } else { c.world.rules.overtime_secs } as f32;
    f.period as f32 + ((len - f.clock) / len).clamp(0.0, 1.0)
}

fn flow_chart(ui: &mut Ui, c: &Ctx, flow: &Flow) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 168.0), Sense::hover());
    let p = ui.painter_at(rect);
    let plot = Rect::from_min_max(pos2(rect.left() + 26.0, rect.top() + 14.0), pos2(rect.right() - 6.0, rect.bottom() - 36.0));
    let regular = c.world.rules.periods as f32;
    let now_x = game_x(c, c.time);
    let units = regular.max(now_x.ceil());
    let peak = flow.margin.iter().map(|m| m.1.unsigned_abs()).max().unwrap_or(0).max(10);
    let r = (peak.div_ceil(5) * 5) as f32;
    let x_of = |u: f32| plot.left() + plot.width() * u / units;
    let y_of = |m: f32| plot.center().y - plot.height() / 2.0 * m / r;
    // Periods.
    for q in 0..=(units as i32) {
        let x = x_of(q as f32);
        p.line_segment([pos2(x, plot.top()), pos2(x, plot.bottom())], Stroke::new(1.0, theme::LINE_2));
        if (q as f32) < units {
            p.text(pos2(x_of(q as f32 + 0.5), plot.bottom() + 9.0), Align2::CENTER_CENTER, period_name(c, q as u8), mono(9.5), theme::TEXT_MUTE);
        }
    }
    // Runs behind the line.
    for run in &flow.runs {
        let (x0, x1) = (x_of(game_x(c, run.from)), x_of(game_x(c, run.to)));
        let ours = run.side as usize == c.me;
        let band = Rect::from_min_max(pos2(x0, plot.top()), pos2(x1.max(x0 + 2.0), plot.bottom()));
        p.rect_filled(band, 0.0, if ours { OURS.gamma_multiply(0.14) } else { theme::WARN.gamma_multiply(0.14) });
        p.text(pos2((x0 + x1) / 2.0, plot.top() + 6.0), Align2::CENTER_CENTER, format!("{}-{}", run.points, run.against), mono(10.0), if ours { OURS } else { theme::WARN });
    }
    // Zero, and the axis in words.
    p.line_segment([pos2(plot.left(), plot.center().y), pos2(plot.right(), plot.center().y)], Stroke::new(1.0, theme::ink(70)));
    p.text(pos2(rect.left() + 2.0, plot.top()), Align2::LEFT_CENTER, format!("+{}", r as i32), mono(9.5), theme::TEXT_MUTE);
    p.text(pos2(rect.left() + 2.0, plot.bottom()), Align2::LEFT_CENTER, format!("-{}", r as i32), mono(9.5), theme::TEXT_MUTE);
    // The margin as a step line, ahead above the zero line.
    let mut pts: Vec<Pos2> = Vec::new();
    let mut last = 0.0f32;
    for &(t, m) in &flow.margin {
        let x = x_of(game_x(c, t));
        if t > 0.0 {
            pts.push(pos2(x, y_of(last)));
        } else {
            pts.push(pos2(x_of(0.0), y_of(0.0)));
        }
        pts.push(pos2(x, y_of(m as f32)));
        last = m as f32;
    }
    pts.push(pos2(x_of(now_x), y_of(last)));
    for pair in pts.windows(2) {
        if (pair[0].x - pair[1].x).abs() > 0.1 {
            let y = pair[0].y;
            let fill = Rect::from_min_max(pos2(pair[0].x, y.min(plot.center().y)), pos2(pair[1].x, y.max(plot.center().y)));
            p.rect_filled(fill, 0.0, if y < plot.center().y { theme::POS.gamma_multiply(0.18) } else { theme::WARN.gamma_multiply(0.18) });
        }
    }
    p.add(Shape::line(pts, Stroke::new(1.8, theme::TEXT)));
    // Timeouts on the top edge, changes of five on the bottom.
    for m in &flow.marks {
        let x = x_of(game_x(c, m.time));
        let col = c.colors[m.side as usize];
        match m.kind {
            MarkKind::Timeout => diamond(&p, pos2(x, plot.top() - 6.0), 3.6, col, theme::TEXT),
            MarkKind::Substitution => {
                p.line_segment([pos2(x, plot.bottom() - 5.0), pos2(x, plot.bottom())], Stroke::new(1.5, col));
            }
            MarkKind::Adjust => {
                p.line_segment([pos2(x - 3.0, plot.top() - 3.0), pos2(x + 3.0, plot.top() - 9.0)], Stroke::new(1.5, theme::WARN));
            }
        }
    }
    // The legend.
    let ly = rect.bottom() - 3.0;
    diamond(&p, pos2(plot.left() + 4.0, ly - 4.0), 3.0, theme::TEXT_DIM, theme::TEXT);
    p.text(pos2(plot.left() + 12.0, ly - 4.0), Align2::LEFT_CENTER, "timeout", mono(9.5), theme::TEXT_MUTE);
    p.line_segment([pos2(plot.left() + 74.0, ly - 8.0), pos2(plot.left() + 74.0, ly - 2.0)], Stroke::new(1.5, theme::TEXT_DIM));
    p.text(pos2(plot.left() + 80.0, ly - 4.0), Align2::LEFT_CENTER, "substitution", mono(9.5), theme::TEXT_MUTE);
    p.text(pos2(plot.right(), ly - 4.0), Align2::RIGHT_CENTER, "up: we lead", mono(9.5), theme::TEXT_MUTE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_half_court_puts_the_basket_low_and_centred_and_keeps_to_its_rect() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(300.0, 300.0));
        let h = Half::new(rect);
        let hoop = h.at(0.0, basketman_core::court::HOOP.y);
        assert!((hoop.x - rect.center().x).abs() < 0.01);
        assert!(hoop.y > rect.center().y, "the basket is at the bottom");
        for (x, y) in [(-7.5, 0.0), (7.5, 0.0), (-7.5, HALF_LEN), (7.5, HALF_LEN)] {
            assert!(rect.expand(0.01).contains(h.at(x, y)));
        }
        // Left of the basket for the shooter is left on the page.
        assert!(h.at(-3.0, 5.0).x < h.at(3.0, 5.0).x);
    }

    #[test]
    fn a_note_asks_for_the_same_live_input_the_agents_get() {
        use basketman_core::observations::SuggestedInput;
        assert!(matches!(
            super::command_of_kind(&SuggestedInput::Timeout),
            Some(Command::Timeout)
        ));
    }
}
