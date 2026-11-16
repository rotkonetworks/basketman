//! The match as a place: the court fills the window and everything else is
//! a panel over it. A scoreboard hangs at the top, the transport sits at the
//! bottom, the play-by-play runs as a ticker, and the bench, the touchline,
//! the orders, the other bench and the box score slide in from the right
//! edge when asked for and slide out again. On a phone the court is on top
//! and one tabbed panel is under it.

use super::*;
use crate::shell::{Tier, glass, panel_with, tab_row};

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
            self.phone = Some(d);
            self.chosen = true;
        }
    }
}

impl Hud {
    /// The phone is turned: the tab that was open stays open, whichever of
    /// the two fields the new layout reads.
    pub fn carry(&mut self, _from: Tier, to: Tier) {
        if to == Tier::Narrow {
            if let Some(d) = self.drawer {
                self.phone = Some(d);
            }
        } else if let Some(d) = self.phone {
            self.drawer = Some(d);
            // Out and about on a wide screen it is the user's now.
            self.chosen = true;
        }
    }
}

impl Drawer {
    /// The first five are what is happening, the rest what the coach does.
    const LIVE: [Drawer; 9] = [
        Drawer::Feed,
        Drawer::Scores,
        Drawer::Shots,
        Drawer::Flow,
        Drawer::Box,
        Drawer::Bench,
        Drawer::Shout,
        Drawer::Orders,
        Drawer::Rival,
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

/// What the match screen remembers about its panels.
#[derive(Default)]
pub(crate) struct Hud {
    /// The drawer that is out, on a wide screen.
    pub drawer: Option<Drawer>,
    /// The user has opened or closed one, so it is theirs to decide.
    pub chosen: bool,
    /// The last one out, to slide it away.
    last: Option<Drawer>,
    /// The open tab of the panel under the court on a phone.
    pub phone: Option<Drawer>,
    /// The phone on its side: when a finger or a pointer was last on the
    /// screen (seconds of the UI clock), to let the controls fade.
    pub last_input: f64,
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

/// Tabs in rows of five at most: the first row is what is happening, the
/// second what the coach does. Returns the tab picked.
fn tab_rows(ui: &mut Ui, tabs: &[Drawer], cur: Drawer, narrow: bool) -> Option<Drawer> {
    let mut pick = None;
    for (r, row) in tabs.chunks(5).enumerate() {
        let labels: Vec<&str> = row.iter().map(|d| d.label()).collect();
        let idx = row.iter().position(|d| *d == cur).unwrap_or(usize::MAX);
        if let Some(i) = tab_row(ui, &labels, idx, narrow) {
            pick = Some(tabs[r * 5 + i]);
        }
    }
    pick
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

    /// The match, as a place. `stage` is where panels may go; the court
    /// itself runs under the bars as well.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn match_hud(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        stage: Rect,
        tier: Tier,
        fixture: usize,
        rec: &Recording,
        result: &GameResult,
        live: Option<(&LiveRound, &mut Vec<Command>)>,
    ) {
        let (live, mut commands) = match live {
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
        let round = f.round;
        {
            // Whatever the feeds hold belongs to the game on screen.
            let ctx = ctx_of(&self.world, ui, &g, self.playback.time, live);
            self.sh.hud.feeds.sync(&ctx);
            if live.is_some() {
                self.sh.hud.feeds.tick(&ctx);
            }
        }
        let crowd = self
            .world
            .econ
            .crowds
            .get(fixture)
            .filter(|c| c.0 > 0)
            .map(|c| {
                format!(
                    " / {} at {}",
                    crate::app::money(c.0 as i64),
                    self.world.arena(f.home).name
                )
            })
            .unwrap_or_default();
        let (time, playing, speed) = (
            self.playback.time,
            self.playback.playing,
            self.playback.speed,
        );
        let frame = blend(rec, time).0;
        let at_head = time >= duration - 1.0;
        let picked = self.touchline.off;
        let mut look = self.look;
        let screen = ui.ctx().content_rect();
        if tier == Tier::Side {
            self.match_hud_side(ui, bd, &g, live, commands);
            return;
        }
        let narrow = tier == Tier::Narrow;
        let landscape = narrow && screen.width() > screen.height() * 1.15;
        let pad = if narrow { 8.0 } else { 12.0 };

        // Where the court is, and where the panel under or beside it is.
        let court: Rect;
        let phone_panel: Option<Rect>;
        if narrow {
            let area = stage.expand2(vec2(pad, 0.0));
            if landscape {
                let w = (area.width() * 0.56).round();
                court = Rect::from_min_size(area.min, vec2(w, area.height() + pad));
                phone_panel = Some(Rect::from_min_max(
                    pos2(court.right() + 6.0, stage.top()),
                    stage.max,
                ));
            } else {
                // The charts of the data tabs need the room more than the
                // court does while they are open.
                let data = matches!(
                    self.sh.hud.phone,
                    Some(Drawer::Scores | Drawer::Shots | Drawer::Flow)
                );
                let share = if data { 0.34 } else { 0.46 };
                let h = (area.width() * 0.78).min(area.height() * share).round();
                court = Rect::from_min_size(area.min, vec2(area.width(), h));
                phone_panel = Some(Rect::from_min_max(
                    pos2(stage.left(), court.bottom() + 6.0),
                    stage.max,
                ));
            }
        } else {
            court = Rect::from_min_max(
                pos2(stage.left() - pad, stage.top()),
                pos2(stage.right() + pad, stage.bottom() + pad),
            );
            phone_panel = None;
        }
        let court = court.intersect(screen);

        // Time moves on screen; a live game is drawn a moment behind.
        let court_time = if live.is_some() && playing {
            (time - LIVE_DELAY).max(0.0)
        } else {
            time
        };
        ui.painter().rect_filled(screen, 0.0, view3d::SURROUND);
        {
            let mode = look.mode;
            let three = matches!(mode, ViewMode::Three | ViewMode::Commentary);
            let area = if three {
                set_fill_height(Some(court.height()));
                court
            } else {
                let angled = mode == ViewMode::Angled;
                let ratio = Pitch::height_for(1.0, angled);
                let w = court.width().min(court.height() / ratio);
                Rect::from_center_size(court.center(), vec2(w, w * ratio))
            };
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(area)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(court);
            paint_floor(
                &mut child,
                rec,
                court_time,
                &mut look,
                &self.world.players,
                result,
                picked,
            );
            set_fill_height(None);
        }

        // The drawer: on by default on a wide screen, once.
        if !self.sh.hud.chosen {
            self.sh.hud.drawer = match (tier, live.is_some()) {
                (Tier::Wide, true) => Some(Drawer::Bench),
                (Tier::Wide, false) => Some(Drawer::Feed),
                _ => None,
            };
        }
        let tabs: &[Drawer] = if live.is_some() {
            &Drawer::LIVE
        } else {
            &Drawer::REPLAY
        };
        let commentary = look.mode == ViewMode::Commentary;
        if commentary && !narrow {
            self.sh.hud.drawer = Some(Drawer::Feed);
        }
        if let Some(d) = self.sh.hud.drawer {
            self.sh.hud.last = Some(d);
        }
        let open = self.sh.hud.drawer.is_some() && !narrow;
        let t = ui
            .ctx()
            .animate_bool_with_time(egui::Id::unique("hud_drawer"), open, 0.2);
        let dw = if commentary {
            (court.width() * 0.5).min(600.0)
        } else {
            (court.width() * 0.34).clamp(330.0, 420.0)
        };
        let drawer_left = court.right() - (dw + pad) * t;
        let right_edge = if narrow {
            court.right()
        } else {
            drawer_left - if t > 0.0 { pad } else { 0.0 }
        };

        // ---- scoreboard
        // Clear of the camera buttons the court draws at its top left.
        let sb_left = if narrow {
            court.left() + pad
        } else {
            court.left() + 300.0
        };
        let sb_room = right_edge - sb_left - pad;
        let sb_w = sb_room.min(if narrow { 380.0 } else { 560.0 });
        let sb_h = if narrow { 52.0 } else { 70.0 };
        let sb_x = if narrow {
            (court.left() + right_edge) / 2.0 - sb_w / 2.0
        } else {
            (sb_left + sb_room / 2.0 - sb_w / 2.0).max(sb_left)
        };
        let sb = Rect::from_min_size(
            pos2(sb_x, court.top() + if narrow { 40.0 } else { 8.0 }),
            vec2(sb_w, sb_h),
        );
        glass(ui, sb, 14, true);
        {
            let state = match live {
                Some(_) if at_head => " / live",
                Some(_) => " / looking back",
                None => " / replay",
            };
            let eye = if narrow {
                format!(
                    "P{} {} {}",
                    frame.period + 1,
                    clock(frame.clock),
                    state.trim_start_matches(" / ")
                )
            } else {
                format!(
                    "Round {} / period {} / {}{state}{crowd}",
                    round + 1,
                    frame.period + 1,
                    clock(frame.clock)
                )
            };
            let score = if at_head { result.score } else { frame.score };
            let p = ui.painter().with_clip_rect(sb);
            p.text(
                pos2(sb.center().x, sb.top() + 12.0),
                Align2::CENTER_CENTER,
                eye.to_uppercase(),
                FontId::new(if narrow { 9.5 } else { 10.0 }, egui::FontFamily::Monospace),
                theme::TEXT_MUTE,
            );
            let big = FontId::new(if narrow { 20.0 } else { 28.0 }, theme::semibold());
            let mid = pos2(sb.center().x, sb.top() + if narrow { 34.0 } else { 40.0 });
            let score_text = format!("{} - {}", score[0], score[1]);
            let sg = p.layout_no_wrap(score_text, big.clone(), theme::TEXT);
            let sw = sg.size().x;
            p.galley(mid - vec2(sw / 2.0, sg.size().y / 2.0), sg, theme::TEXT);
            let nm = FontId::new(if narrow { 12.5 } else { 16.0 }, theme::semibold());
            for side in 0..2 {
                let text = &g.teams[side];
                let gal = p.layout_no_wrap(text.clone(), nm.clone(), g.colors[side]);
                let room = sb.width() / 2.0 - sw / 2.0 - 22.0;
                let text = if gal.size().x > room {
                    initials_of(text)
                } else {
                    text.clone()
                };
                let gal = p.layout_no_wrap(text, nm.clone(), g.colors[side]);
                let at = if side == 0 {
                    pos2(
                        mid.x - sw / 2.0 - 12.0 - gal.size().x,
                        mid.y - gal.size().y / 2.0,
                    )
                } else {
                    pos2(mid.x + sw / 2.0 + 12.0, mid.y - gal.size().y / 2.0)
                };
                let r = Rect::from_min_size(at, gal.size()).expand(4.0);
                let resp = ui
                    .interact(r, egui::Id::unique(("sb_club", side)), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                p.galley(at, gal, g.colors[side]);
                if resp.clicked() {
                    follow(ui, Link::Club(g.clubs[side]));
                }
            }
            if !narrow {
                p.text(
                    pos2(sb.right() - 10.0, sb.top() + 12.0),
                    Align2::RIGHT_CENTER,
                    format!("SHOT CLOCK {:.0}", frame.shot_clock.ceil()),
                    FontId::new(10.0, egui::FontFamily::Monospace),
                    theme::TEXT_DIM,
                );
            }
        }
        if let Some(notice) = &self.touchline.notice {
            let n = Rect::from_center_size(
                pos2(sb.center().x, sb.bottom() + 20.0),
                vec2(sb_w.min(520.0), 30.0),
            );
            glass(ui, n, 10, true);
            ui.painter().with_clip_rect(n).text(
                n.center(),
                Align2::CENTER_CENTER,
                notice,
                FontId::new(13.0, theme::semibold()),
                theme::ACCENT,
            );
        }

        // ---- transport
        let mut st = Transport {
            time,
            playing,
            speed,
        };
        let transport_w = (right_edge - court.left() - 2.0 * pad).min(920.0);
        let transport_h = if narrow { 0.0 } else { 78.0 };
        let tr = Rect::from_min_size(
            pos2(
                (court.left() + right_edge) / 2.0 - transport_w / 2.0,
                court.bottom() - pad - transport_h,
            ),
            vec2(transport_w, transport_h),
        );
        if !narrow {
            glass(ui, tr, 14, true);
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(tr.shrink2(vec2(12.0, 6.0)))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(tr);
            transport(
                &mut child,
                tr.width() - 24.0,
                &mut st,
                &mut look,
                &g,
                at_head,
                commands.as_deref_mut(),
            );
        }

        // ---- ticker
        let showing_feed = self.sh.hud.drawer == Some(Drawer::Feed);
        if !narrow && !showing_feed {
            let tw = (right_edge - court.left() - 2.0 * pad).min(460.0);
            let th = 74.0;
            let tk =
                Rect::from_min_size(pos2(court.left() + pad, tr.top() - 8.0 - th), vec2(tw, th));
            let upto = rec.events.partition_point(|e| e.time <= st.time);
            glass(ui, tk, 12, true);
            let players = &self.world.players;
            let p = ui.painter().with_clip_rect(tk);
            for (n, e) in rec.events[..upto].iter().rev().take(3).enumerate() {
                let y = tk.top() + 14.0 + n as f32 * 20.0;
                p.text(
                    pos2(tk.left() + 10.0, y),
                    Align2::LEFT_CENTER,
                    clock(e.clock),
                    FontId::new(11.0, egui::FontFamily::Monospace),
                    theme::TEXT_MUTE,
                );
                p.text(
                    pos2(tk.left() + 56.0, y),
                    Align2::LEFT_CENTER,
                    event_line(e, &g.teams, players),
                    FontId::new(
                        if n == 0 { 13.5 } else { 12.5 },
                        if n == 0 {
                            theme::semibold()
                        } else {
                            egui::FontFamily::Proportional
                        },
                    ),
                    if n == 0 {
                        event_color(e, &g.colors)
                    } else {
                        theme::TEXT_DIM
                    },
                );
            }
            let resp = ui
                .interact(tk, egui::Id::unique("ticker"), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if resp.clicked() {
                self.sh.hud.drawer = Some(Drawer::Feed);
                self.sh.hud.chosen = true;
            }
        }

        // ---- the drawer, or the handles that open it
        let bench_data = live.map(|l| l.game.players(l.side));
        let mut pick_tab: Option<Drawer> = None;
        let mut close = false;
        if narrow {
            if let Some(rect) = phone_panel {
                let cur = self
                    .sh
                    .hud
                    .phone
                    .filter(|d| tabs.contains(d))
                    .unwrap_or(Drawer::Feed);
                let mut tab_pick = None;
                panel_with(ui, rect, "hud_phone", false, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    let w = ui.available_width();
                    transport(
                        ui,
                        w,
                        &mut st,
                        &mut look,
                        &g,
                        at_head,
                        commands.as_deref_mut(),
                    );
                    ui.add_space(2.0);
                    tab_pick = tab_rows(ui, tabs, cur, false);
                    egui::ScrollArea::vertical()
                        .id_salt(("hud_phone_body", cur.label()))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.hud_body(
                                ui,
                                bd,
                                &g,
                                cur,
                                live,
                                commands.as_deref_mut(),
                                bench_data.as_deref(),
                                st.time,
                                false,
                            );
                        });
                });
                if let Some(d) = tab_pick {
                    self.sh.hud.phone = Some(d);
                }
            }
        } else if t > 0.001 {
            let which = self
                .sh
                .hud
                .drawer
                .or(self.sh.hud.last)
                .unwrap_or(Drawer::Feed);
            let rect = Rect::from_min_max(
                pos2(drawer_left, court.top() + 8.0),
                pos2(court.right() - pad, court.bottom() - pad),
            );
            panel_with(ui, rect, "hud_panel", false, |ui| {
                pick_tab = tab_rows(ui, tabs, which, false);
                egui::ScrollArea::vertical()
                    .id_salt(("hud_body", which.label()))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        self.hud_body(
                            ui,
                            bd,
                            &g,
                            which,
                            live,
                            commands,
                            bench_data.as_deref(),
                            st.time,
                            commentary,
                        );
                    });
            });
            if ui
                .put(
                    Rect::from_min_size(
                        pos2(rect.left() - 26.0, rect.top() + 4.0),
                        vec2(22.0, 28.0),
                    ),
                    egui::Button::new(">"),
                )
                .on_hover_text("Hide the panel")
                .clicked()
            {
                close = true;
            }
        } else {
            // Handles on the right edge: one per panel.
            let (w, h) = (70.0, 32.0);
            let col = Rect::from_min_size(
                pos2(court.right() - pad - w, court.top() + 8.0),
                vec2(w, (h + 4.0) * tabs.len() as f32 + 4.0),
            );
            glass(ui, col, 12, true);
            for (i, d) in tabs.iter().enumerate() {
                let r = Rect::from_min_size(
                    pos2(col.left() + 4.0, col.top() + 4.0 + i as f32 * (h + 4.0)),
                    vec2(w - 8.0, h),
                );
                let resp = ui
                    .interact(r, egui::Id::unique(("handle", i)), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.hovered() {
                    ui.painter().rect_filled(r, 8.0, theme::ink(14));
                }
                ui.painter().text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    d.label(),
                    FontId::new(12.5, theme::semibold()),
                    theme::TEXT_DIM,
                );
                if resp.clicked() {
                    pick_tab = Some(*d);
                }
            }
        }
        if let Some(d) = pick_tab {
            self.sh.hud.drawer = Some(d);
            self.sh.hud.chosen = true;
        }
        if close {
            self.sh.hud.drawer = None;
            self.sh.hud.chosen = true;
        }

        self.playback.time = st.time;
        self.playback.playing = st.playing;
        self.playback.speed = st.speed;
        self.look = look;
        // A tap on the shot chart: play from just before that shot.
        if let Some(t) = self.sh.hud.feeds.jump.take() {
            self.playback.time = t;
            self.playback.playing = true;
        }
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

/// Play and pause, the speeds, the jumps of a live game, the view and the
/// timeline.
fn transport(
    ui: &mut Ui,
    w: f32,
    st: &mut Transport,
    look: &mut Look,
    g: &Game,
    at_head: bool,
    commands: Option<&mut Vec<Command>>,
) {
    // A phone has no room for a button per speed and per view: one button
    // each, stepping through the choices, keeps it to a single row.
    if w < 430.0 {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            let label = match (st.playing, g.live) {
                (true, _) => "Pause",
                (false, true) => "Resume",
                (false, false) => "Play",
            };
            if theme::primary_button(ui, true, label) {
                if !g.live && st.time >= g.duration {
                    st.time = 0.0;
                }
                st.playing = !st.playing;
            }
            if ui
                .button(format!("{:.0}x", st.speed))
                .on_hover_text("Speed")
                .clicked()
            {
                let at = SPEEDS.iter().position(|s| *s == st.speed).unwrap_or(0);
                st.speed = SPEEDS[(at + 1) % SPEEDS.len()];
            }
            let view = match look.mode {
                ViewMode::Flat => "2D",
                ViewMode::Commentary => "Text",
                _ => "3D",
            };
            if ui.button(view).on_hover_text("View").clicked() {
                look.mode = match look.mode {
                    ViewMode::Flat => ViewMode::Commentary,
                    ViewMode::Commentary => ViewMode::Three,
                    _ => ViewMode::Flat,
                };
            }
            if let Some(commands) = commands {
                if !at_head {
                    if ui.button("Live").clicked() {
                        st.time = g.duration;
                    }
                } else if ui.button("Break").on_hover_text("Play on to the next break").clicked() {
                    commands.push(Command::SkipPeriod);
                }
                if ui.button("Finish").clicked() {
                    commands.push(Command::SkipAll);
                }
            } else if ui.button("End").clicked() {
                st.time = g.duration;
            }
        });
        ui.spacing_mut().slider_width = (w - 30.0).max(100.0);
        ui.spacing_mut().interact_size.y = 14.0;
        let slider = egui::Slider::new(&mut st.time, 0.0..=g.duration).show_value(false);
        if ui.add(slider).dragged() {
            st.playing = false;
        }
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let label = match (st.playing, g.live) {
            (true, _) => "Pause",
            (false, true) => "Resume",
            (false, false) => "Play",
        };
        if theme::primary_button(ui, true, label) {
            if !g.live && st.time >= g.duration {
                st.time = 0.0;
            }
            st.playing = !st.playing;
        }
        for s in SPEEDS {
            if ui
                .add(egui::Button::selectable(st.speed == s, format!("{s:.0}x")))
                .clicked()
            {
                st.speed = s;
            }
        }
        if let Some(commands) = commands {
            if !at_head
                && ui
                    .button("Live")
                    .on_hover_text("Back to the live game")
                    .clicked()
            {
                st.time = g.duration;
            }
            if ui
                .button("Next break")
                .on_hover_text("Play on to the end of the period without watching.")
                .clicked()
            {
                commands.push(Command::SkipPeriod);
            }
            if ui
                .button("Finish")
                .on_hover_text("Your assistant takes the bench and the game is played out at once.")
                .clicked()
            {
                commands.push(Command::SkipAll);
            }
        } else if ui.button("Skip to end").clicked() {
            st.time = g.duration;
        }
        view_buttons(ui, &mut look.mode, true);
    });
    ui.spacing_mut().slider_width = (w - 30.0).clamp(100.0, 900.0);
    let slider = egui::Slider::new(&mut st.time, 0.0..=g.duration).show_value(false);
    if ui.add(slider).dragged() {
        st.playing = false;
    }
}

fn initials_of(text: &str) -> String {
    text.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(3)
        .collect::<String>()
        .to_uppercase()
}

// ---- a phone held sideways ------------------------------------------------

/// How long the controls stay out after the last touch while the game runs.
const IDLE_SECS: f64 = 3.0;

impl App {
    /// The match for a phone on its side: the court is the whole screen,
    /// the score hangs at the top centre, a slim set of controls fades when
    /// nothing is touched, and the data tabs come in as a sheet from the
    /// right over part of the court.
    fn match_hud_side(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        g: &Game,
        live: Option<&LiveRound>,
        mut commands: Option<&mut Vec<Command>>,
    ) {
        use crate::side::{EDGE, SAFE};
        let screen = ui.ctx().content_rect();
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

        // The court: everything.
        ui.painter().rect_filled(screen, 0.0, view3d::SURROUND);
        {
            let three = matches!(look.mode, ViewMode::Three | ViewMode::Commentary);
            let area = if three {
                set_fill_height(Some(screen.height()));
                screen
            } else {
                let ratio = Pitch::height_for(1.0, false);
                let w = screen.width().min(screen.height() / ratio);
                Rect::from_center_size(screen.center(), vec2(w, w * ratio))
            };
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(area)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(screen);
            set_cam_buttons(false);
            paint_floor(
                &mut child,
                rec,
                court_time,
                &mut look,
                &self.world.players,
                result,
                picked,
            );
            set_cam_buttons(true);
            set_fill_height(None);
        }

        // The sheet's state: a tab out or not. In commentary the feed is
        // always out.
        let tabs: &[Drawer] = if live.is_some() { &Drawer::LIVE } else { &Drawer::REPLAY };
        if commentary {
            self.sh.hud.drawer = Some(Drawer::Feed);
        }
        if let Some(d) = self.sh.hud.drawer
            && !tabs.contains(&d)
        {
            self.sh.hud.drawer = Some(Drawer::Feed);
        }
        if let Some(d) = self.sh.hud.drawer {
            self.sh.hud.last = Some(d);
        }
        let open = self.sh.hud.drawer.is_some();
        let t = ui
            .ctx()
            .animate_bool_with_time(egui::Id::unique("side_sheet"), open, 0.18);
        let sheet_w = (screen.width() * 0.45).round();
        let sheet_left = screen.right() - (sheet_w + SAFE) * t;
        let sheet = Rect::from_min_max(
            pos2(sheet_left, screen.top() + EDGE),
            pos2(sheet_left + sheet_w, screen.bottom() - EDGE),
        );

        // A tap on the court puts the sheet away, and wakes the controls.
        let now = ui.input(|i| i.time);
        let touched = ui.input(|i| {
            i.pointer.any_down() || i.pointer.is_moving() || i.pointer.any_released()
                || i.multi_touch().is_some() || i.smooth_scroll_delta != egui::Vec2::ZERO
        });
        if touched {
            self.sh.hud.last_input = now;
        }
        if open && !commentary {
            let court = Rect::from_min_max(screen.min, pos2(sheet.left(), screen.bottom()));
            let tap = ui.interact(court, egui::Id::unique("side_court_tap"), Sense::click());
            if tap.clicked() {
                self.sh.hud.drawer = None;
                self.sh.hud.chosen = true;
            }
        }

        // The score.
        let sb = Rect::from_center_size(
            pos2(
                ((screen.left() + SAFE + 60.0) + (screen.right() - SAFE)) / 2.0 - 30.0,
                screen.top() + EDGE + 23.0,
            ),
            vec2(290.0, 46.0),
        );
        let sb = if open { sb.translate(vec2(-(sb.right() - (sheet.left() - 8.0)).max(0.0), 0.0)) } else { sb };
        glass(ui, sb, 12, true);
        {
            let state = match live {
                Some(_) if at_head => "live",
                Some(_) => "looking back",
                None => "replay",
            };
            let eye = format!("P{} {} {state}", frame.period + 1, clock(frame.clock));
            let score = if at_head { result.score } else { frame.score };
            let p = ui.painter().with_clip_rect(sb);
            p.text(
                pos2(sb.center().x, sb.top() + 10.0),
                Align2::CENTER_CENTER,
                eye.to_uppercase(),
                FontId::new(12.0, egui::FontFamily::Monospace),
                theme::TEXT_MUTE,
            );
            let big = FontId::new(19.0, theme::semibold());
            let mid = pos2(sb.center().x, sb.top() + 30.0);
            let sg = p.layout_no_wrap(format!("{} - {}", score[0], score[1]), big, theme::TEXT);
            let sw = sg.size().x;
            p.galley(mid - vec2(sw / 2.0, sg.size().y / 2.0), sg, theme::TEXT);
            let nm = FontId::new(13.0, theme::semibold());
            for side in 0..2 {
                let room = sb.width() / 2.0 - sw / 2.0 - 16.0;
                let full = p.layout_no_wrap(g.teams[side].clone(), nm.clone(), g.colors[side]);
                let text = if full.size().x > room { initials_of(&g.teams[side]) } else { g.teams[side].clone() };
                let gal = p.layout_no_wrap(text, nm.clone(), g.colors[side]);
                let at = if side == 0 {
                    pos2(mid.x - sw / 2.0 - 10.0 - gal.size().x, mid.y - gal.size().y / 2.0)
                } else {
                    pos2(mid.x + sw / 2.0 + 10.0, mid.y - gal.size().y / 2.0)
                };
                p.galley(at, gal, g.colors[side]);
            }
        }
        if let Some(notice) = &self.touchline.notice {
            let n = Rect::from_center_size(
                pos2(sb.center().x, sb.bottom() + 20.0),
                vec2(sb.width().max(320.0).min(sheet.left() - 20.0), 32.0),
            );
            glass(ui, n, 10, true);
            ui.painter().with_clip_rect(n).text(
                n.center(),
                Align2::CENTER_CENTER,
                notice,
                FontId::new(13.0, theme::semibold()),
                theme::ACCENT,
            );
        }

        // The controls: out while there is something to do, or touch.
        let mut st = Transport { time, playing, speed };
        let idle_forced = self.harness() && std::env::var("SHOT_IDLE").is_ok();
        let awake = !idle_forced
            && (!playing || open || self.touchline.notice.is_some()
                || now - self.sh.hud.last_input < IDLE_SECS);
        if playing && !idle_forced && !awake {
            ui.ctx().request_repaint();
        } else if playing && awake && !open {
            // Come back to fade them.
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(400));
        }
        let alpha = ui
            .ctx()
            .animate_bool_with_time(egui::Id::unique("side_controls"), awake, 0.25);
        let mut sheet_pick: Option<Drawer> = None;
        let mut open_sheet = false;
        if alpha > 0.02 {
            let right_limit = if open { sheet.left() - 8.0 } else { screen.right() - SAFE };
            let bar = Rect::from_min_max(
                pos2(screen.left() + SAFE, screen.bottom() - EDGE - 46.0),
                pos2(right_limit, screen.bottom() - EDGE),
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(bar)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            child.set_clip_rect(bar.expand(6.0));
            child.set_opacity(alpha);
            glass(&child, bar, 14, true);
            // The controls are only live while they are visible enough to see.
            if alpha > 0.6 {
                child.spacing_mut().item_spacing.x = 6.0;
                child.add_space(6.0);
                open_sheet = side_transport(
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

        // The sheet.
        if t > 0.001 {
            let which = self.sh.hud.drawer.or(self.sh.hud.last).unwrap_or(Drawer::Feed);
            let bench_data = live.map(|l| l.game.players(l.side));
            let mut close = false;
            let mut commands = commands;
            panel_with(ui, sheet, "hud_sheet", false, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
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
                                    if *d == which && sheet_pick.is_none() && ui.is_rect_visible(r.rect) {
                                        // Keep the open tab in sight when the row is first shown.
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
            });
            if close {
                self.sh.hud.drawer = None;
                self.sh.hud.chosen = true;
            }
        }
        if open_sheet {
            let d = self.sh.hud.last.filter(|d| tabs.contains(d)).unwrap_or(Drawer::Feed);
            sheet_pick = Some(d);
        }
        if let Some(d) = sheet_pick {
            // Both fields, so turning the phone keeps the tab.
            self.sh.hud.drawer = Some(d);
            self.sh.hud.phone = Some(d);
            self.sh.hud.chosen = true;
        }

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
    sheet_open: bool,
) -> bool {
    let mut panels = false;
    let mut left = room;
    let mut fits = |ui: &Ui, text: &str, left: &mut f32| -> bool {
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
    if look.mode == ViewMode::Three {
        let cam = look.cam.preset.label();
        if fits(ui, cam, &mut left) && ui.add(egui::Button::new(cam).min_size(size)).clicked() {
            let at = Preset::ALL.iter().position(|p| *p == look.cam.preset).unwrap_or(0);
            look.cam.preset = Preset::ALL[(at + 1) % Preset::ALL.len()];
        }
    }
    if !sheet_open && fits(ui, "Panels", &mut left) {
        let b = ui.add(egui::Button::new("Panels").min_size(size));
        panels = b.clicked();
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
