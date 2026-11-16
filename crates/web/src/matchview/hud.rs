//! The match as a place: the court fills the window and everything else is
//! a panel over it. A scoreboard hangs at the top, the transport sits at the
//! bottom, the play-by-play runs as a ticker, and the bench, the touchline,
//! the orders, the other bench and the box score slide in from the right
//! edge when asked for and slide out again. On a phone the court is on top
//! and one tabbed panel is under it.

use super::*;
use crate::shell::{Tier, glass_alpha};

/// What the drawer on the right shows.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Drawer {
    Feed,
    Scores,
    Shots,
    Flow,
    Bench,
    Shout,
    Orders,
    Rival,
    Box,
}

impl Hud {
    /// Open the tab called `key` (the screenshot harness).
    pub fn pick(&mut self, key: &str) {
        let d = Drawer::LIVE.iter().find(|d| d.label().eq_ignore_ascii_case(key));
        if let Some(&d) = d {
            self.drawer = Some(d);
            self.chosen = true;
        }
    }
}

impl Drawer {
    /// What the coach does first, then what is happening.
    const LIVE: [Drawer; 9] = [
        Drawer::Bench,
        Drawer::Shout,
        Drawer::Orders,
        Drawer::Rival,
        Drawer::Feed,
        Drawer::Scores,
        Drawer::Shots,
        Drawer::Flow,
        Drawer::Box,
    ];
    const REPLAY: [Drawer; 4] = [Drawer::Feed, Drawer::Shots, Drawer::Flow, Drawer::Box];

    fn label(self) -> &'static str {
        match self {
            Drawer::Feed => "Feed",
            Drawer::Scores => "Scores",
            Drawer::Shots => "Shots",
            Drawer::Flow => "Flow",
            Drawer::Bench => "Bench",
            Drawer::Shout => "Shout",
            Drawer::Orders => "Orders",
            Drawer::Rival => "Rival",
            Drawer::Box => "Box",
        }
    }
}

/// What the user chose about the match screen's overlays, kept between
/// sessions.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct HudPrefs {
    /// 0 full, 1 minimal, 2 clean.
    pub level: u8,
    /// The side panel was left open, on this tab, this wide (0: default).
    pub open: bool,
    pub tab: Option<Drawer>,
    pub width: f32,
    /// How solid the panel, the ticker and the score are (0.35 to 1).
    pub alpha: f32,
    pub score: bool,
    pub ticker: bool,
    pub names: bool,
    pub cams: bool,
    pub swish: bool,
    pub hint: bool,
    /// The user has turned or zoomed the court once: the hint is done.
    pub hint_done: bool,
}

impl Default for HudPrefs {
    fn default() -> Self {
        HudPrefs {
            level: 0,
            open: false,
            tab: None,
            width: 0.0,
            alpha: 0.93,
            score: true,
            ticker: true,
            names: true,
            cams: true,
            swish: true,
            hint: true,
            hint_done: false,
        }
    }
}

impl HudPrefs {
    pub fn encode(&self) -> String {
        let b = |v: bool| u8::from(v);
        format!(
            "level={};open={};tab={};w={:.0};a={:.2};score={};ticker={};names={};cams={};swish={};hint={};done={}",
            self.level,
            b(self.open),
            self.tab.map_or("", Drawer::label),
            self.width,
            self.alpha,
            b(self.score),
            b(self.ticker),
            b(self.names),
            b(self.cams),
            b(self.swish),
            b(self.hint),
            b(self.hint_done),
        )
    }

    pub fn decode(text: &str) -> Self {
        let mut p = HudPrefs::default();
        for kv in text.split(';') {
            let Some((k, v)) = kv.split_once('=') else { continue };
            let on = v == "1";
            match k {
                "level" => p.level = v.parse::<u8>().unwrap_or(0).min(2),
                "open" => p.open = on,
                "tab" => p.tab = Drawer::LIVE.iter().copied().find(|d| d.label() == v),
                "w" => p.width = v.parse().unwrap_or(0.0),
                "a" => p.alpha = v.parse::<f32>().unwrap_or(0.93).clamp(0.35, 1.0),
                "score" => p.score = on,
                "ticker" => p.ticker = on,
                "names" => p.names = on,
                "cams" => p.cams = on,
                "swish" => p.swish = on,
                "hint" => p.hint = on,
                "done" => p.hint_done = on,
                _ => {}
            }
        }
        p
    }
}

/// What the match screen remembers about its panels.
#[derive(Default)]
pub(crate) struct Hud {
    /// The drawer that is out, on a wide screen.
    pub drawer: Option<Drawer>,
    /// The user has opened or closed one, so it is theirs to decide.
    pub chosen: bool,
    /// The last one out, to slide it away.
    last: Option<Drawer>,
    /// The phone on its side: when a finger or a pointer was last on the
    /// screen (seconds of the UI clock), to let the controls fade.
    pub last_input: f64,
    /// What the user chose to see, kept between sessions.
    pub prefs: HudPrefs,
    /// What the data tabs remember: the assistant's notes, filters, marks.
    pub feeds: super::feeds::Feeds,
}

/// What one frame of the match screen knows about the game.
struct Game<'a> {
    fixture: usize,
    /// The side the user's club plays, 0 = home.
    me: usize,
    teams: [String; 2],
    clubs: [usize; 2],
    rec: &'a Recording,
    result: &'a GameResult,
    duration: f32,
    live: bool,
    colors: [Color32; 2],
}

/// What the data tabs need to know about the game on screen.
fn ctx_of<'a>(
    world: &'a basketman_core::league::World,
    ui: &Ui,
    g: &'a Game,
    time: f32,
    live: Option<&'a LiveRound>,
) -> super::feeds::Ctx<'a> {
    super::feeds::Ctx {
        world,
        rec: g.rec,
        result: g.result,
        fixture: g.fixture,
        me: g.me,
        time,
        live,
        colors: g.colors,
        teams: &g.teams,
        wall: ui.input(|i| i.time),
        film: false,
    }
}

fn event_line(
    e: &basketman_core::replay::Event,
    teams: &[String; 2],
    players: &[Player],
) -> String {
    match &e.kind {
        EventKind::Timeout { side } => format!("Timeout, {}.", teams[*side as usize]),
        EventKind::Adjust {
            side,
            offense,
            defense,
            press,
        } => format!(
            "{} change it: {} on offense, {} on defense, {}.",
            teams[*side as usize],
            offense.label().to_lowercase(),
            defense.label().to_lowercase(),
            press.label().to_lowercase()
        ),
        _ => e.text(|p: PlayerId| players[p as usize].name.as_str()),
    }
}

fn event_color(e: &basketman_core::replay::Event, colors: &[Color32; 2]) -> Color32 {
    match &e.kind {
        EventKind::Mistake { .. } => theme::WARN,
        EventKind::Shot { made: true, .. } => colors[e.offense as usize],
        EventKind::Timeout { side } => colors[*side as usize],
        EventKind::Adjust { .. } => theme::WARN,
        EventKind::Shout { .. } => theme::ACCENT,
        EventKind::PeriodStart | EventKind::PeriodEnd | EventKind::Substitution { .. } => {
            theme::TEXT_MUTE
        }
        _ => theme::TEXT_DIM,
    }
}

impl App {
    /// The play-by-play, newest first, each line a link to the player it is
    /// about.
    #[allow(clippy::too_many_arguments)]
    fn feed_list(
        &self,
        ui: &mut Ui,
        g: &Game,
        time: f32,
        lines: usize,
        big: bool,
        live: Option<&LiveRound>,
        commands: &mut Option<&mut Vec<Command>>,
    ) {
        let upto = g.rec.events.partition_point(|e| e.time <= time);
        let players = &self.world.players;
        let ctx = ctx_of(&self.world, ui, g, time, live);
        let notes = &self.sh.hud.feeds.notes;
        // The assistant's notes newest first, spoken in between the events.
        let mut pending = notes.iter().rev().filter(|n| n.time <= time).peekable();
        for (n, e) in g.rec.events[..upto].iter().rev().take(lines).enumerate() {
            while let Some(note) = pending.next_if(|note| note.time >= e.time) {
                self.sh.hud.feeds.note_line(ui, &ctx, note, commands, big);
            }
            let color = event_color(e, &g.colors);
            let line = event_line(e, &g.teams, players);
            ui.horizontal_top(|ui| {
                ui.label(theme::mono(clock(e.clock)).color(theme::TEXT_MUTE));
                let text = RichText::new(line).color(color);
                let text = if big { text.size(16.0) } else { text };
                let text = if n == 0 { text.strong() } else { text };
                match e.actor() {
                    Some(p) => {
                        let label = egui::Label::new(text)
                            .wrap()
                            .selectable(false)
                            .sense(Sense::click());
                        if ui
                            .add(label)
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            follow(ui, Link::Player(p));
                        }
                    }
                    None => {
                        ui.add(egui::Label::new(text).wrap());
                    }
                }
            });
        }
    }

    /// The final or running score by period, and both box scores.
    fn box_panel(&self, ui: &mut Ui, bd: &Backdrop, g: &Game) {
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, if g.live { "So far" } else { "Final" });
            ui.label(name(&format!(
                "{} {} - {} {}",
                g.teams[0], g.result.score[0], g.result.score[1], g.teams[1]
            )));
            let periods: Vec<String> = g
                .result
                .periods
                .iter()
                .map(|p| format!("{}-{}", p[0], p[1]))
                .collect();
            ui.label(theme::mono(periods.join("   ")).color(theme::TEXT_DIM));
        });
        for side in 0..2 {
            ui.add_space(6.0);
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, &g.teams[side]);
                ui.add_space(6.0);
                egui::ScrollArea::horizontal()
                    .id_salt(("box_scroll", side))
                    .show(ui, |ui| {
                        self.box_score(ui, g.result, side);
                        ui.add_space(14.0);
                    });
            });
        }
    }

    /// The match, as a place: the court is the whole window and everything
    /// else floats over it, as much or as little as the user chose.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn match_hud(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        _stage: Rect,
        tier: Tier,
        fixture: usize,
        rec: &Recording,
        result: &GameResult,
        live: Option<(&LiveRound, &mut Vec<Command>)>,
    ) {
        let (live, commands) = match live {
            Some((round, commands)) => (Some(round), Some(commands)),
            None => (None, None),
        };
        view3d::arena::ArenaView::of(&self.world, fixture).publish(ui.ctx());
        let f = &self.world.fixtures[fixture];
        let duration = live.map_or(rec.duration(), |l| l.game.time());
        let g = Game {
            fixture,
            me: usize::from(f.home != self.team),
            teams: [
                self.world.teams[f.home].name.clone(),
                self.world.teams[f.away].name.clone(),
            ],
            clubs: [f.home, f.away],
            rec,
            result,
            duration,
            live: live.is_some(),
            colors: [theme::ACCENT, theme::TEXT],
        };
        {
            // Whatever the feeds hold belongs to the game on screen.
            let ctx = ctx_of(&self.world, ui, &g, self.playback.time, live);
            self.sh.hud.feeds.sync(&ctx);
            if live.is_some() {
                self.sh.hud.feeds.tick(&ctx);
            }
        }
        self.match_overlay(ui, bd, tier, &g, live, commands);
    }

    /// What is in the panel for `which`.
    #[allow(clippy::too_many_arguments)]
    fn hud_body(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        g: &Game,
        which: Drawer,
        live: Option<&LiveRound>,
        commands: Option<&mut Vec<Command>>,
        bench: Option<&[LivePlayer]>,
        time: f32,
        big: bool,
    ) {
        match (which, live, commands, bench) {
            (Drawer::Feed, live, mut commands, _) => {
                self.feed_list(ui, g, time, if big { 200 } else { 80 }, big, live, &mut commands)
            }
            (Drawer::Scores | Drawer::Shots | Drawer::Flow, live, ..) => {
                let ctx = super::feeds::Ctx {
                    world: &self.world,
                    rec: g.rec,
                    result: g.result,
                    fixture: g.fixture,
                    me: g.me,
                    time,
                    live,
                    colors: g.colors,
                    teams: &g.teams,
                    wall: ui.input(|i| i.time),
                    film: false,
                };
                let feeds = &mut self.sh.hud.feeds;
                match which {
                    Drawer::Scores => feeds.scores_tab(ui, bd, &ctx),
                    Drawer::Shots => feeds.shots_tab(ui, bd, &ctx),
                    _ => feeds.flow_tab(ui, bd, &ctx),
                }
            }
            (Drawer::Box, ..) => self.box_panel(ui, bd, g),
            (Drawer::Bench, Some(l), Some(c), Some(b)) => self.bench_card(ui, bd, l, b, c),
            (Drawer::Shout, Some(l), Some(c), Some(b)) => self.shout_card(ui, bd, l, b, c),
            (Drawer::Orders, Some(l), Some(c), _) => self.orders_card(ui, bd, l, c),
            (Drawer::Rival, Some(l), ..) => self.opponent_card(ui, bd, l),
            _ => {}
        }
    }
}

/// The clock of the picture: where it is, whether it runs, how fast.
struct Transport {
    time: f32,
    playing: bool,
    speed: f32,
}

fn initials_of(text: &str) -> String {
    text.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(3)
        .collect::<String>()
        .to_uppercase()
}


// ---- the overlays ----------------------------------------------------------

/// How long the controls stay out after the last touch while the game runs.
const IDLE_SECS: f64 = 3.0;
/// Room for the corner buttons (menu, eye, view, camera).
const CLUSTER_W: f32 = 290.0;

/// A line of text that stays readable over anything: dark ink with a light
/// edge.
fn halo_text(p: &egui::Painter, at: Pos2, anchor: Align2, text: &str, font: FontId, ink: Color32) {
    for d in [vec2(-1.0, 0.0), vec2(1.0, 0.0), vec2(0.0, -1.0), vec2(0.0, 1.0)] {
        p.text(at + d, anchor, text, font.clone(), Color32::from_white_alpha(170));
    }
    p.text(at, anchor, text, font, ink);
}

/// The eye: open for everything, half shut for the minimum, struck through
/// for the clean court.
fn eye_button(ui: &mut Ui, level: u8) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(40.0, 36.0), Sense::click());
    let c = r.center();
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(r.shrink(2.0), 8.0, theme::ink(14));
    }
    let col = theme::TEXT;
    p.add(Shape::ellipse_stroke(c, vec2(11.0, 6.5), Stroke::new(1.8, col)));
    match level {
        0 => {
            p.circle_filled(c, 3.6, col);
        }
        1 => {
            p.circle_stroke(c, 3.4, Stroke::new(1.6, col));
        }
        _ => {
            p.line_segment([c + vec2(-10.0, 9.0), c + vec2(10.0, -9.0)], Stroke::new(2.0, col));
        }
    }
    resp
}

impl App {
    /// The match on every size: the court is the window, and what floats
    /// over it is the user's to choose. Full shows everything; Minimal a
    /// small score, with the controls fading while the game runs; Clean
    /// nothing at all.
    fn match_overlay(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        tier: Tier,
        g: &Game,
        live: Option<&LiveRound>,
        mut commands: Option<&mut Vec<Command>>,
    ) {
        use crate::side::{EDGE, SAFE};
        let screen = ui.ctx().content_rect();
        let phone = tier.phone();
        let stacked = tier == Tier::Narrow;
        let mut prefs = self.sh.hud.prefs;
        let (rec, result) = (g.rec, g.result);
        let (time, playing, speed) = (
            self.playback.time,
            self.playback.playing,
            self.playback.speed,
        );
        let frame = blend(rec, time).0;
        let at_head = time >= g.duration - 1.0;
        let picked = self.touchline.off;
        let mut look = self.look;
        let commentary = look.mode == ViewMode::Commentary;
        let court_time = if live.is_some() && playing {
            (time - LIVE_DELAY).max(0.0)
        } else {
            time
        };
        let now = ui.input(|i| i.time);
        let level = prefs.level;

        // ---- keys and touches
        let touched = ui.input(|i| {
            !i.events.is_empty() || i.pointer.any_down() || i.multi_touch().is_some()
        });
        if touched {
            self.sh.hud.last_input = now;
        }
        if !ui.ctx().egui_wants_keyboard_input()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::H))
        {
            prefs.level = (prefs.level + 1) % 3;
            self.sh.hud.last_input = now;
        }
        let idle = now - self.sh.hud.last_input;
        let notice = self.touchline.notice.is_some();
        let shot_idle = self.harness() && std::env::var("SHOT_IDLE").is_ok();
        let level = prefs.level;

        // ---- the panel's state
        let tabs: &[Drawer] = if live.is_some() { &Drawer::LIVE } else { &Drawer::REPLAY };
        if commentary {
            self.sh.hud.drawer = Some(Drawer::Feed);
        }
        if let Some(d) = self.sh.hud.drawer
            && !tabs.contains(&d)
        {
            self.sh.hud.drawer = Some(tabs[0]);
        }
        if let Some(d) = self.sh.hud.drawer {
            self.sh.hud.last = Some(d);
        }
        let open = self.sh.hud.drawer.is_some() && level < 2;
        let t = ui
            .ctx()
            .animate_bool_with_time(egui::Id::unique("hud_sheet"), open, 0.18);
        let top = screen.top() + if phone { EDGE } else { 12.0 };
        let gap = if phone { SAFE } else { 12.0 };
        let max_w = (screen.width() * 0.45).round();
        let default_w = if phone {
            max_w
        } else {
            (screen.width() * 0.34).clamp(330.0, 420.0).min(max_w)
        };
        let sheet_w = if prefs.width > 0.0 {
            prefs.width.clamp(280.0f32.min(max_w), max_w)
        } else {
            default_w
        };
        let sheet = if stacked {
            let h = (screen.height() * 0.55).round();
            let y = screen.bottom() - (h + EDGE) * t;
            Rect::from_min_size(pos2(screen.left() + 8.0, y), vec2(screen.width() - 16.0, h))
        } else {
            let x = screen.right() - (sheet_w + gap) * t;
            Rect::from_min_max(pos2(x, top), pos2(x + sheet_w, screen.bottom() - EDGE))
        };
        // Where the free court is, for the score and the controls.
        let free_right = if open && !stacked { sheet.left() - 8.0 } else { screen.right() - SAFE };
        let free_bottom = if open && stacked { sheet.top() - 6.0 } else { screen.bottom() - EDGE };

        // ---- the court: the whole window
        ui.painter().rect_filled(screen, 0.0, view3d::SURROUND);
        // Upright, the court keeps the top while the panel is out.
        let court_r = if stacked {
            Rect::from_min_max(screen.min, pos2(screen.right(), (sheet.top() - 6.0).max(screen.top() + 220.0)))
        } else {
            screen
        };
        {
            let three = matches!(look.mode, ViewMode::Three | ViewMode::Commentary);
            let area = if three {
                set_fill_height(Some(court_r.height()));
                court_r
            } else {
                let ratio = Pitch::height_for(1.0, false);
                let w = court_r.width().min(court_r.height() / ratio);
                Rect::from_center_size(court_r.center(), vec2(w, w * ratio))
            };
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(area)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(court_r);
            set_overlays(Overlays {
                names: prefs.names && level < 2,
                swish: prefs.swish && level < 2,
                hint: prefs.hint && !prefs.hint_done && level == 0,
                cams: false,
            });
            let before = (look.cam.zoom, look.cam.yaw);
            paint_floor(&mut child, rec, court_time, &mut look, &self.world.players, result, picked);
            if (look.cam.zoom, look.cam.yaw) != before {
                prefs.hint_done = true;
            }
            set_overlays(Overlays { names: true, swish: true, hint: true, cams: true });
            set_fill_height(None);
        }

        // On a phone a tap on the court puts the panel away.
        if open && phone && !commentary {
            let court = if stacked {
                Rect::from_min_max(screen.min, pos2(screen.right(), sheet.top()))
            } else {
                Rect::from_min_max(screen.min, pos2(sheet.left(), screen.bottom()))
            };
            let tap = ui.interact(court, egui::Id::unique("hud_court_tap"), Sense::click());
            if tap.clicked() {
                self.sh.hud.drawer = None;
            }
        }

        // ---- what is visible now
        let controls_on = !shot_idle
            && match level {
                2 => false,
                0 if !phone => true,
                _ => !playing || open || notice || idle < IDLE_SECS,
            };
        let corner_on = !shot_idle
            && match level {
                2 => idle < IDLE_SECS && self.sh.hud.last_input > 0.0,
                0 if !phone => true,
                _ => controls_on,
            };
        let ctl = ui
            .ctx()
            .animate_bool_with_time(egui::Id::unique("hud_controls"), controls_on, 0.25);
        let corner = ui
            .ctx()
            .animate_bool_with_time(egui::Id::unique("hud_corner"), corner_on, 0.25);
        if (ctl > 0.0 && ctl < 1.0) || (corner > 0.0 && corner < 1.0) || (corner_on && level == 2) {
            ui.ctx().request_repaint();
        }
        let solid = prefs.alpha;

        // ---- the score
        let narrow_top = screen.width() < 700.0
            || free_right - (screen.left() + SAFE + CLUSTER_W + 10.0) < 300.0;
        if prefs.score && level < 2 {
            let w = 280.0f32.min(free_right - screen.left() - 2.0 * SAFE);
            let (cx, y) = if narrow_top {
                ((screen.left() + free_right) / 2.0, top + 52.0)
            } else {
                let from = screen.left() + SAFE + CLUSTER_W + 10.0;
                ((from + free_right) / 2.0, top)
            };
            let sb = Rect::from_min_size(pos2(cx - w / 2.0, y), vec2(w, 46.0));
            glass_alpha(ui, sb, 12, true, solid);
            let state = match live {
                Some(_) if at_head => "live",
                Some(_) => "looking back",
                None => "replay",
            };
            let eye = if level == 0 {
                format!("P{} {} {state}", frame.period + 1, clock(frame.clock))
            } else {
                format!("P{} {}", frame.period + 1, clock(frame.clock))
            };
            let score = if at_head { result.score } else { frame.score };
            let p = ui.painter().with_clip_rect(sb);
            halo_text(
                &p,
                pos2(sb.center().x, sb.top() + 10.0),
                Align2::CENTER_CENTER,
                &eye.to_uppercase(),
                FontId::new(12.0, egui::FontFamily::Monospace),
                theme::TEXT_DIM,
            );
            let mid = pos2(sb.center().x, sb.top() + 30.0);
            let big = FontId::new(19.0, theme::semibold());
            let sg = p.layout_no_wrap(format!("{} - {}", score[0], score[1]), big.clone(), theme::TEXT);
            let sw = sg.size().x;
            halo_text(
                &p,
                mid,
                Align2::CENTER_CENTER,
                &format!("{} - {}", score[0], score[1]),
                big,
                theme::TEXT,
            );
            let nm = FontId::new(13.0, theme::semibold());
            for side in 0..2 {
                let room = sb.width() / 2.0 - sw / 2.0 - 16.0;
                let full = p.layout_no_wrap(g.teams[side].clone(), nm.clone(), g.colors[side]);
                let text = if full.size().x > room { initials_of(&g.teams[side]) } else { g.teams[side].clone() };
                let (anchor, at) = if side == 0 {
                    (Align2::RIGHT_CENTER, pos2(mid.x - sw / 2.0 - 10.0, mid.y))
                } else {
                    (Align2::LEFT_CENTER, pos2(mid.x + sw / 2.0 + 10.0, mid.y))
                };
                halo_text(&p, at, anchor, &text, nm.clone(), g.colors[side]);
            }
            if let Some(n) = &self.touchline.notice {
                let nr = Rect::from_center_size(
                    pos2(sb.center().x, sb.bottom() + 20.0),
                    vec2(w.max(330.0).min(free_right - screen.left() - 2.0 * SAFE), 32.0),
                );
                glass_alpha(ui, nr, 10, true, solid.max(0.8));
                ui.painter().with_clip_rect(nr).text(
                    nr.center(),
                    Align2::CENTER_CENTER,
                    n,
                    FontId::new(13.0, theme::semibold()),
                    theme::ACCENT,
                );
            }
        } else if notice && level == 2 {
            // Nothing but a pulsing dot asks for the coach.
            let at = pos2(screen.right() - SAFE - 14.0, top + 16.0);
            let r = 6.0 + 2.5 * ((now * 5.0).sin() as f32).abs();
            ui.painter().circle_filled(at, r, theme::ACCENT);
            let hit = ui.interact(
                Rect::from_center_size(at, vec2(40.0, 40.0)),
                egui::Id::unique("hud_dot"),
                Sense::click(),
            );
            if hit.clicked() {
                prefs.level = 1;
            }
            ui.ctx().request_repaint();
        }

        // ---- the corner: menu, eye, view, camera
        if corner > 0.02 {
            let bar = Rect::from_min_size(
                pos2(screen.left() + SAFE, top),
                vec2(CLUSTER_W - 20.0, 44.0),
            );
            let mut row = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(bar)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            row.set_opacity(corner);
            let backing = row.painter().add(Shape::Noop);
            row.spacing_mut().item_spacing.x = 2.0;
            row.add_space(4.0);
            if row.add(egui::Button::new("Menu").frame(false).min_size(vec2(0.0, 36.0))).clicked() {
                self.sh.rail_open = true;
            }
            let eye = eye_button(&mut row, level).on_hover_text("Cycle what is shown (H)");
            if eye.clicked() {
                prefs.level = (prefs.level + 1) % 3;
            }
            row.menu_button("View", |ui| {
                ui.set_min_width(210.0);
                ui.label("Panel opacity");
                ui.add(egui::Slider::new(&mut prefs.alpha, 0.35..=1.0).show_value(false));
                ui.separator();
                ui.checkbox(&mut prefs.score, "Score");
                ui.checkbox(&mut prefs.ticker, "Feed ticker");
                ui.checkbox(&mut prefs.names, "Names over heads");
                ui.checkbox(&mut prefs.cams, "Camera button");
                ui.checkbox(&mut prefs.swish, "+N on a basket");
                if ui.checkbox(&mut prefs.hint, "Zoom hint").changed() && prefs.hint {
                    prefs.hint_done = false;
                }
            });
            if prefs.cams && look.mode == ViewMode::Three {
                let cam = look.cam.preset.label();
                row.menu_button(cam, |ui| {
                    for p in Preset::ALL {
                        if ui
                            .selectable_label(look.cam.preset == p, p.label())
                            .clicked()
                        {
                            look.cam.preset = p;
                            ui.close();
                        }
                    }
                });
            }
            let used = row.min_rect().expand2(vec2(4.0, 0.0));
            let r = (used.height() / 2.0) as u8;
            let sheet_c = Color32::from_rgba_unmultiplied(250, 248, 243, (solid.max(0.8) * 255.0) as u8);
            row.painter().set(
                backing,
                Shape::Vec(vec![
                    egui::epaint::RectShape::filled(used, r, sheet_c).into(),
                    egui::epaint::RectShape::stroke(
                        used,
                        r,
                        Stroke::new(1.0, theme::LINE_2),
                        egui::StrokeKind::Outside,
                    )
                    .into(),
                ]),
            );
        }

        // ---- the transport
        let mut st = Transport { time, playing, speed };
        let mut open_panel = false;
        let avail = free_right - (screen.left() + SAFE);
        let bar_w = avail.min(760.0);
        let bx = if phone { screen.left() + SAFE } else { screen.left() + SAFE + (avail - bar_w) / 2.0 };
        let bar = Rect::from_min_max(
            pos2(bx, free_bottom - 46.0),
            pos2(bx + bar_w, free_bottom),
        );
        if ctl > 0.02 {
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(bar)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            child.set_clip_rect(bar.expand(6.0));
            child.set_opacity(ctl);
            glass_alpha(&child, bar, 14, true, solid.max(0.8));
            if ctl > 0.6 {
                child.spacing_mut().item_spacing.x = 6.0;
                child.add_space(6.0);
                open_panel = side_transport(
                    &mut child,
                    bar.width() - 12.0,
                    &mut st,
                    &mut look,
                    g,
                    at_head,
                    commands.as_deref_mut(),
                    open,
                );
            }
        }

        // ---- the ticker: the last two things that happened
        let feed_shown = open && self.sh.hud.drawer == Some(Drawer::Feed);
        if prefs.ticker && level == 0 && !feed_shown && (ctl > 0.02 || !phone) {
            let tw = avail.min(440.0);
            let th = 56.0;
            let tk = Rect::from_min_size(
                pos2(screen.left() + SAFE, (if ctl > 0.02 { bar.top() } else { free_bottom }) - 6.0 - th),
                vec2(tw, th),
            );
            glass_alpha(ui, tk, 12, true, solid);
            let upto = rec.events.partition_point(|e| e.time <= st.time);
            let players = &self.world.players;
            let p = ui.painter().with_clip_rect(tk);
            for (n, e) in rec.events[..upto].iter().rev().take(2).enumerate() {
                let y = tk.top() + 16.0 + n as f32 * 24.0;
                halo_text(
                    &p,
                    pos2(tk.left() + 10.0, y),
                    Align2::LEFT_CENTER,
                    &clock(e.clock),
                    FontId::new(12.0, egui::FontFamily::Monospace),
                    theme::TEXT_DIM,
                );
                halo_text(
                    &p,
                    pos2(tk.left() + 56.0, y),
                    Align2::LEFT_CENTER,
                    &event_line(e, &g.teams, players),
                    FontId::new(if n == 0 { 13.5 } else { 12.5 }, theme::semibold()),
                    if n == 0 { event_color(e, &g.colors) } else { theme::TEXT_DIM },
                );
            }
            let resp = ui.interact(tk, egui::Id::unique("ticker"), Sense::click());
            if resp.clicked() {
                self.sh.hud.drawer = Some(Drawer::Feed);
            }
        }

        // ---- a slim handle on the right edge, where the panel is not
        if !phone && !open && ctl > 0.02 && level < 2 {
            let h = Rect::from_center_size(
                pos2(screen.right() - 12.0, screen.center().y),
                vec2(24.0, 84.0),
            );
            let resp = ui
                .interact(h, egui::Id::unique("hud_handle"), Sense::click())
                .on_hover_text("Open the panel");
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(h));
            c.set_opacity(ctl);
            glass_alpha(&c, h, 8, true, solid.max(0.8));
            c.painter().text(h.center(), Align2::CENTER_CENTER, "<", FontId::new(16.0, theme::mono_semibold()), theme::TEXT);
            if resp.clicked() {
                open_panel = true;
            }
        }

        // ---- the panel
        let mut sheet_pick: Option<Drawer> = None;
        if t > 0.001 {
            let which = self.sh.hud.drawer.or(self.sh.hud.last).unwrap_or(tabs[0]);
            let bench_data = live.map(|l| l.game.players(l.side));
            let mut close = false;
            glass_alpha(ui, sheet, 14, true, solid);
            let inner = sheet.shrink2(vec2(10.0, 10.0));
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(sheet.shrink(1.0));
            child.spacing_mut().item_spacing.y = 4.0;
            {
                let ui = &mut child;
                // One row of tabs that scrolls, and the way out.
                ui.horizontal(|ui| {
                    let w = ui.available_width() - 44.0;
                    egui::ScrollArea::horizontal()
                        .id_salt("hud_sheet_tabs")
                        .max_width(w)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                for d in tabs {
                                    let r = ui.add(
                                        egui::Button::selectable(*d == which, d.label())
                                            .min_size(vec2(0.0, 36.0)),
                                    );
                                    if r.clicked() {
                                        sheet_pick = Some(*d);
                                    }
                                }
                            });
                        });
                    if ui
                        .add(egui::Button::new("X").min_size(vec2(36.0, 36.0)))
                        .clicked()
                    {
                        close = true;
                    }
                });
                egui::ScrollArea::vertical()
                    .id_salt(("hud_sheet_body", which.label()))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        self.hud_body(
                            ui,
                            bd,
                            g,
                            which,
                            live,
                            commands.as_deref_mut(),
                            bench_data.as_deref(),
                            st.time,
                            commentary,
                        );
                    });
            }
            if close {
                self.sh.hud.drawer = None;
            }
            // The inner edge drags the width, with a mouse.
            if !phone && t > 0.99 {
                let edge = Rect::from_min_max(
                    pos2(sheet.left() - 5.0, sheet.top()),
                    pos2(sheet.left() + 5.0, sheet.bottom()),
                );
                let resp = ui
                    .interact(edge, egui::Id::unique("hud_sheet_edge"), Sense::drag())
                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                if resp.dragged() {
                    prefs.width = (sheet_w - resp.drag_delta().x).clamp(280.0f32.min(max_w), max_w);
                }
            }
        }
        if open_panel {
            let d = self
                .sh
                .hud
                .last
                .filter(|d| tabs.contains(d))
                .or(prefs.tab.filter(|d| tabs.contains(d)))
                .unwrap_or(tabs[0]);
            sheet_pick = Some(d);
        }
        if let Some(d) = sheet_pick {
            self.sh.hud.drawer = Some(d);
            self.sh.hud.chosen = true;
        }

        // What is to be remembered.
        if !commentary {
            prefs.open = self.sh.hud.drawer.is_some();
        }
        if let Some(d) = self.sh.hud.drawer.or(self.sh.hud.last) {
            prefs.tab = Some(d);
        }
        self.sh.hud.prefs = prefs;
        self.playback.time = st.time;
        self.playback.playing = st.playing;
        self.playback.speed = st.speed;
        self.look = look;
        if let Some(t) = self.sh.hud.feeds.jump.take() {
            self.playback.time = t;
            self.playback.playing = true;
        }
    }
}

/// The slim controls over the court, in a single row. What does not fit
/// the room is left out, least important last. Returns true when the
/// panels button was pressed.
#[allow(clippy::too_many_arguments)]
fn side_transport(
    ui: &mut Ui,
    room: f32,
    st: &mut Transport,
    look: &mut Look,
    g: &Game,
    at_head: bool,
    commands: Option<&mut Vec<Command>>,
    panel_open: bool,
) -> bool {
    let mut panels = false;
    let mut left = room;
    let fits = |ui: &Ui, text: &str, left: &mut f32| -> bool {
        let w = ui
            .painter()
            .layout_no_wrap(text.to_string(), FontId::new(14.0, theme::semibold()), Color32::BLACK)
            .size()
            .x
            + 26.0
            + 6.0;
        if *left >= w {
            *left -= w;
            true
        } else {
            false
        }
    };
    let play_label = match (st.playing, g.live) {
        (true, _) => "Pause",
        (false, true) => "Resume",
        (false, false) => "Play",
    };
    let size = vec2(0.0, 36.0);
    if fits(ui, play_label, &mut left)
        && ui
            .add(
                egui::Button::new(RichText::new(play_label).family(theme::semibold()).color(theme::BG))
                    .fill(theme::TEXT)
                    .min_size(size),
            )
            .clicked()
    {
        if !g.live && st.time >= g.duration {
            st.time = 0.0;
        }
        st.playing = !st.playing;
    }
    let speed = format!("{:.0}x", st.speed);
    if fits(ui, &speed, &mut left) && ui.add(egui::Button::new(speed).min_size(size)).clicked() {
        let at = SPEEDS.iter().position(|s| *s == st.speed).unwrap_or(0);
        st.speed = SPEEDS[(at + 1) % SPEEDS.len()];
    }
    let mut commands = commands;
    if let Some(commands) = commands.as_deref_mut() {
        let (text, live_back) = if at_head { ("Break", false) } else { ("Live", true) };
        if fits(ui, text, &mut left) && ui.add(egui::Button::new(text).min_size(size)).clicked() {
            if live_back {
                st.time = g.duration;
            } else {
                commands.push(Command::SkipPeriod);
            }
        }
        if fits(ui, "Finish", &mut left)
            && ui.add(egui::Button::new("Finish").min_size(size)).clicked()
        {
            commands.push(Command::SkipAll);
        }
    } else if fits(ui, "End", &mut left) && ui.add(egui::Button::new("End").min_size(size)).clicked() {
        st.time = g.duration;
    }
    let view = match look.mode {
        ViewMode::Flat => "2D",
        ViewMode::Commentary => "Text",
        _ => "3D",
    };
    if fits(ui, view, &mut left) && ui.add(egui::Button::new(view).min_size(size)).clicked() {
        look.mode = match look.mode {
            ViewMode::Flat => ViewMode::Commentary,
            ViewMode::Commentary => ViewMode::Three,
            _ => ViewMode::Flat,
        };
    }
    if !panel_open && fits(ui, "Panels", &mut left) {
        panels = ui.add(egui::Button::new("Panels").min_size(size)).clicked();
    }
    if left >= 90.0 {
        ui.spacing_mut().slider_width = left - 24.0;
        let slider = egui::Slider::new(&mut st.time, 0.0..=g.duration).show_value(false);
        if ui.add(slider).dragged() {
            st.playing = false;
        }
    }
    panels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_user_chose_is_remembered() {
        let p = HudPrefs {
            level: 2,
            open: true,
            tab: Some(Drawer::Orders),
            width: 360.0,
            alpha: 0.4,
            score: false,
            ticker: false,
            names: false,
            cams: false,
            swish: false,
            hint: false,
            hint_done: true,
        };
        assert_eq!(HudPrefs::decode(&p.encode()), p);
        // Nonsense falls back to the defaults.
        assert_eq!(HudPrefs::decode("level=9;a=7"), HudPrefs { level: 2, alpha: 1.0, ..HudPrefs::default() });
        assert_eq!(HudPrefs::decode(""), HudPrefs::default());
    }
}
