//! The film room: the user's last game cut into clips, with the best
//! attacks, the turnovers and the breakdowns listed beside a court that
//! plays whichever one is picked, over and over.

use basketman_core::film::{Breakdown, Clip, ClipKind, Film, Summary};
use basketman_core::player::PlayerId;
use basketman_core::replay::{EventKind, Mistake, Source, TurnoverKind};
use eframe::egui::{self, Align, Layout, RichText, Sense, Ui, vec2};

use crate::app::{App, GAP};
use crate::board::surname;
use crate::matchview::{paint_floor, view_buttons};
use crate::pages::dim;
use crate::theme::{self, Backdrop};

/// The three lists, in the order they are shown.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum List {
    Best,
    Turnovers,
    Breakdowns,
}

impl List {
    const ALL: [List; 3] = [List::Best, List::Turnovers, List::Breakdowns];

    fn title(self) -> &'static str {
        match self {
            List::Best => "Our best attacks",
            List::Turnovers => "Our turnovers",
            List::Breakdowns => "Breakdowns",
        }
    }

    fn note(self) -> &'static str {
        match self {
            List::Best => {
                "Baskets ranked by how well the offense worked: a pass beats a lone effort, a cut \
                 or a roll beats a late-clock shot, an open man beats a smothered one."
            }
            List::Turnovers => "Every time we gave it away, in the order it happened.",
            List::Breakdowns => {
                "Our defenders losing their man, and our own sets falling apart. A defensive \
                 mistake is marked if they scored on it."
            }
        }
    }
}

/// What the film room remembers between frames.
pub(crate) struct FilmState {
    /// The game the clips below were cut from, and the clips.
    film: Option<(usize, Film)>,
    /// The clip on show: which list it was picked from, and its index in
    /// `Film::clips`.
    clip: Option<(List, usize)>,
    pub time: f32,
    pub playing: bool,
    pub speed: f32,
    /// Screenshots: start each clip just before its event, and ask for a
    /// list and a place in it when the film is first built.
    pub freeze: bool,
    pub want: Option<(List, usize)>,
}

impl Default for FilmState {
    fn default() -> Self {
        FilmState {
            film: None,
            clip: None,
            time: 0.0,
            playing: true,
            speed: 1.0,
            freeze: false,
            want: None,
        }
    }
}

impl FilmState {
    fn show(&mut self, film: &Film, list: List, i: usize) {
        self.clip = Some((list, i));
        let c = &film.clips[i];
        self.time = if self.freeze {
            (c.time - 0.6).max(c.start)
        } else {
            c.start
        };
    }
}

/// Plural, in the words of a coach.
fn source_phrase(s: Source) -> &'static str {
    match s {
        Source::Transition => "the fast break",
        Source::Drive => "drives",
        Source::PullUp => "pull-up jumpers",
        Source::CatchAndShoot => "catch-and-shoot",
        Source::Cut => "cuts",
        Source::Roll => "the roll man",
        Source::Post => "post ups",
        Source::Putback => "putbacks",
        Source::Forced => "late-clock shots",
    }
}

fn turnover_label(k: TurnoverKind) -> &'static str {
    match k {
        TurnoverKind::BadPass => "Bad pass",
        TurnoverKind::Stripped => "Stripped",
        TurnoverKind::OffensiveFoul => "Offensive foul",
        TurnoverKind::ShotClock => "Shot clock",
        TurnoverKind::Pressed => "Lost to the press",
        TurnoverKind::ThreeSeconds => "Three seconds",
    }
}

fn mistake_label(m: Mistake) -> &'static str {
    match m {
        Mistake::BallWatching => "Ball watching",
        Mistake::LateCloseout => "Late closeout",
        Mistake::BlownBy => "Blown by",
        Mistake::LostCutter => "Lost his cutter",
        Mistake::OutOfPosition => "Out of position",
    }
}

/// The small heading of a clip's row.
fn tag(c: &Clip) -> String {
    match &c.kind {
        ClipKind::Score { source, assisted } => format!(
            "{} / {} / {} pts",
            source.label(),
            if *assisted { "assisted" } else { "on his own" },
            c.points
        ),
        ClipKind::Turnover(k) => turnover_label(*k).to_string(),
        ClipKind::Breakdown(Breakdown::Defense { mistake, conceded }) => format!(
            "{}{}",
            mistake_label(*mistake),
            if *conceded { " / they scored" } else { "" }
        ),
        ClipKind::Breakdown(Breakdown::Set) => "Set broke down".to_string(),
        ClipKind::Breakdown(Breakdown::ShotClock) => "Shot clock".to_string(),
    }
}

fn period_clock(c: &Clip) -> String {
    let s = c.clock.ceil().max(0.0) as u32;
    format!("P{} {}:{:02}", c.period + 1, s / 60, s % 60)
}

/// "Carter 5, Tamm 3" for the first few of a ranked list.
fn naming(list: &[(PlayerId, u16)], who: &dyn Fn(PlayerId) -> String) -> String {
    list.iter()
        .take(4)
        .map(|(p, n)| format!("{} {n}", who(*p)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn plural(n: u16, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The three lines under the heading, in plain words.
fn sentences(s: &Summary, who: &dyn Fn(PlayerId) -> String) -> [Vec<String>; 3] {
    let mut score = Vec::new();
    if s.made == 0 {
        score.push("No baskets to show.".to_string());
    } else {
        score.push(format!(
            "{} points from {} baskets in {} shots.",
            s.points, s.made, s.attempts
        ));
        if let Some(best) = s.sources.first() {
            score.push(format!(
                "Best source of points: {}, {} from {} {}.",
                source_phrase(best.source),
                plural(best.points, "point", "points"),
                best.made,
                if best.made == 1 { "basket" } else { "baskets" }
            ));
        }
        score.push(format!(
            "{} of {} baskets came off a pass.",
            s.assisted, s.made
        ));
    }

    let mut tov = Vec::new();
    if s.turnovers == 0 {
        tov.push("No turnovers. A clean game.".to_string());
    } else {
        let kinds: Vec<String> = s
            .turnover_kinds
            .iter()
            .map(|(k, n)| format!("{n} {}", k.plural()))
            .collect();
        tov.push(format!(
            "{}: {}.",
            plural(s.turnovers, "turnover", "turnovers"),
            kinds.join(", ")
        ));
        tov.push(format!("By player: {}.", naming(&s.turnover_players, who)));
    }

    let mut brk = Vec::new();
    if s.breakdowns == 0 {
        brk.push("Nothing broke down that the film can pin on a player.".to_string());
    } else {
        brk.push(format!(
            "{} charged to our players. By player: {}.",
            plural(s.breakdowns, "breakdown", "breakdowns"),
            naming(&s.breakdown_players, who)
        ));
    }
    [score, tov, brk]
}

impl App {
    pub(crate) fn film_room(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let me = self.team;
        // Games on tape this session: the one loaded and the earlier ones.
        let mut tapes: Vec<usize> = self
            .world
            .tapes
            .iter()
            .map(|t| t.0)
            .chain(self.world.replay.iter().map(|r| r.0))
            .collect();
        tapes.sort_unstable();
        let loaded = self
            .world
            .replay
            .as_ref()
            .filter(|(_, rec)| !rec.frames.is_empty())
            .map(|(i, _)| *i);
        let Some(fixture) = loaded else {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Film room");
                ui.label(RichText::new("No film yet.").color(theme::TEXT).strong());
                ui.label(dim(
                    "Games are filmed as they are played and the tapes are not kept in a saved \
                     career, so there is nothing to watch after starting a new career or \
                     loading a save. Film appears after your next game.",
                ));
            });
            return;
        };

        let side = usize::from(self.world.fixtures[fixture].home != me);
        let mut load: Option<usize> = None;

        // Which game.
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                theme::eyebrow(ui, "Game");
                for &i in &tapes {
                    let f = &self.world.fixtures[i];
                    let theirs = if f.home == me { f.away } else { f.home };
                    let mut label =
                        format!("Round {} v {}", f.round + 1, self.world.teams[theirs].name);
                    if let Some(r) = &f.result {
                        let (a, b) = (r.score[side_of(f, me)], r.score[1 - side_of(f, me)]);
                        label = format!("{label}  {}{a}-{b}", if a > b { "W " } else { "L " });
                    }
                    if ui.selectable_label(i == fixture, label).clicked() && i != fixture {
                        load = Some(i);
                    }
                }
            });
        });
        ui.add_space(GAP);
        if let Some(i) = load {
            self.world.load_tape(i);
            self.room.film.clip = None;
            self.room.film.film = None;
            ui.ctx().request_repaint();
            return;
        }

        // Cut the clips once per game.
        if self
            .room
            .film
            .film
            .as_ref()
            .is_none_or(|(i, _)| *i != fixture)
        {
            let players = &self.world.players;
            let rec = &self.world.replay.as_ref().expect("loaded above").1;
            let film = Film::of(rec, side, |p| players[p as usize].name.as_str());
            self.room.film.film = Some((fixture, film));
            self.room.film.clip = None;
        }
        let film = self.room.film.film.as_ref().expect("built above").1.clone();
        let film = &film;
        let state = &mut self.room.film;
        let lists: [Vec<usize>; 3] = [film.best_looks(), film.turnovers(), film.breakdowns()];
        let index = |l: List| lists[List::ALL.iter().position(|x| *x == l).unwrap_or(0)].as_slice();
        if state.clip.is_none() {
            let (l, n) = state.want.take().unwrap_or((List::Best, 0));
            let pool = if index(l).is_empty() { List::Best } else { l };
            if let Some(&i) = index(pool).get(n).or(index(pool).first()) {
                state.show(film, pool, i);
            }
        }

        let names: Vec<String> = self.world.players.iter().map(|p| p.name.clone()).collect();
        let who = |p: PlayerId| surname(&names[p as usize]).to_string();
        let [score, tov, brk] = sentences(&film.summary, &who);

        // What the film says.
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "What the film says");
            ui.add_space(4.0);
            let wide = ui.available_width() > 900.0;
            let block = |ui: &mut Ui, title: &str, lines: &[String]| {
                ui.label(theme::eyebrow_text(title));
                for l in lines {
                    ui.label(RichText::new(l).color(theme::TEXT));
                }
            };
            if wide {
                ui.columns(3, |c| {
                    block(&mut c[0], "Scoring", &score);
                    block(&mut c[1], "Turnovers", &tov);
                    block(&mut c[2], "Breakdowns", &brk);
                });
            } else {
                block(ui, "Scoring", &score);
                ui.add_space(6.0);
                block(ui, "Turnovers", &tov);
                ui.add_space(6.0);
                block(ui, "Breakdowns", &brk);
            }
        });
        ui.add_space(GAP);

        // Where the shots were taken, for both teams.
        if let (Some((_, rec)), Some(result)) = (
            self.world.replay.as_ref(),
            self.world.fixtures[fixture].result.as_ref(),
        ) {
            let f = &self.world.fixtures[fixture];
            let teams = [
                self.world.teams[f.home].name.clone(),
                self.world.teams[f.away].name.clone(),
            ];
            let ctx = crate::matchview::FeedCtx {
                world: &self.world,
                rec,
                result,
                fixture,
                me: side,
                time: f32::MAX,
                live: None,
                colors: [theme::ACCENT, theme::TEXT],
                teams: &teams,
                wall: ui.input(|i| i.time),
                film: true,
            };
            let feeds = &mut self.sh.hud.feeds;
            feeds.sync(&ctx);
            // Closed to begin with: the clips are what the room is for.
            egui::CollapsingHeader::new(RichText::new("Shot chart and zones").strong().color(theme::TEXT))
                .default_open(false)
                .show(ui, |ui| feeds.shots_tab(ui, bd, &ctx));
            // A tap on a shot sets the replay to it.
            if let Some(t) = feeds.jump.take() {
                self.playback.time = t;
                self.playback.playing = false;
            }
        }
        ui.add_space(GAP);

        // The lists and the court.
        let mut picked: Option<(List, usize)> = None;
        let current = state.clip;
        let list_card = |ui: &mut Ui, l: List, picked: &mut Option<(List, usize)>| {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                let rows = index(l);
                theme::eyebrow(ui, &format!("{} / {}", l.title(), rows.len()));
                ui.label(dim(l.note()));
                ui.add_space(6.0);
                if rows.is_empty() {
                    ui.label(dim("None in this game."));
                }
                let shown = if l == List::Best { 8 } else { rows.len() };
                // Rows are about this tall; the list scrolls past a few.
                let room = (shown as f32 * 78.0).min(if l == List::Best { 470.0 } else { 320.0 });
                egui::ScrollArea::vertical()
                    .id_salt(("film_list", l as u8))
                    .min_scrolled_height(room)
                    .max_height(room)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for &i in rows.iter().take(shown) {
                            let c = &film.clips[i];
                            let chosen = current == Some((l, i));
                            let frame = egui::Frame::NONE
                                .inner_margin(egui::Margin::symmetric(6, 4))
                                .corner_radius(6)
                                .fill(if chosen {
                                    theme::ACCENT.gamma_multiply(0.10)
                                } else {
                                    egui::Color32::TRANSPARENT
                                });
                            frame.show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal_top(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    ui.add_sized(
                                        vec2(62.0, 18.0),
                                        egui::Label::new(
                                            theme::mono(period_clock(c)).color(theme::TEXT_MUTE),
                                        ),
                                    );
                                    ui.vertical(|ui| {
                                        let head = ui.add(
                                            egui::Label::new(theme::mono(tag(c)).color(
                                                if chosen {
                                                    theme::ACCENT
                                                } else {
                                                    theme::TEXT_DIM
                                                },
                                            ))
                                            .selectable(false)
                                            .sense(Sense::click()),
                                        );
                                        let line = ui.add(
                                            egui::Label::new(RichText::new(&c.text))
                                                .wrap()
                                                .selectable(false)
                                                .sense(Sense::click()),
                                        );
                                        if head.clicked() || line.clicked() {
                                            *picked = Some((l, i));
                                        }
                                        if head.hovered() || line.hovered() {
                                            ui.ctx()
                                                .set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                        if let Some(p) = c.who {
                                            crate::profile::player_link(
                                                ui,
                                                p,
                                                RichText::new(&names[p as usize])
                                                    .color(theme::ACCENT)
                                                    .small(),
                                            );
                                        }
                                    });
                                });
                            });
                        }
                    });
            });
        };

        let wide = ui.available_width() > 1100.0;
        let down = Layout::top_down(Align::Min);
        if wide {
            let right = (ui.available_width() * 0.56).max(560.0);
            let left = ui.available_width() - right - GAP;
            ui.horizontal_top(|ui| {
                let gap = GAP - ui.spacing().item_spacing.x;
                ui.allocate_ui_with_layout(vec2(left, 0.0), down, |ui| {
                    for l in List::ALL {
                        list_card(ui, l, &mut picked);
                        ui.add_space(GAP);
                    }
                });
                ui.add_space(gap);
                ui.allocate_ui_with_layout(vec2(right, 0.0), down, |ui| {
                    self.clip_player(ui, bd, fixture, film, &lists);
                });
            });
        } else {
            self.clip_player(ui, bd, fixture, film, &lists);
            ui.add_space(GAP);
            for l in List::ALL {
                list_card(ui, l, &mut picked);
                ui.add_space(GAP);
            }
        }
        if let Some((l, i)) = picked {
            self.room.film.show(film, l, i);
            self.room.film.playing = !self.room.film.freeze;
        }
    }

    /// The court, playing the picked clip, with its controls and the
    /// play-by-play of the moment.
    fn clip_player(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        fixture: usize,
        film: &Film,
        lists: &[Vec<usize>; 3],
    ) {
        let Some((list, i)) = self.room.film.clip else {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Clip");
                ui.label(dim("Pick a clip from the lists to watch it here."));
            });
            return;
        };
        let clip = &film.clips[i];
        let state = &mut self.room.film;
        // Run the clock and loop.
        if state.playing {
            let dt = ui.input(|i| i.stable_dt).min(0.1);
            state.time += dt * state.speed;
            ui.ctx().request_repaint();
        }
        if state.time > clip.end || state.time < clip.start {
            state.time = clip.start;
        }
        let time = state.time;
        let order = &lists[List::ALL.iter().position(|x| *x == list).unwrap_or(0)];
        let at = order.iter().position(|&k| k == i).unwrap_or(0);
        let (rec, result) = match (
            self.world.replay.as_ref(),
            self.world.fixtures[fixture].result.as_ref(),
        ) {
            (Some((_, rec)), Some(result)) => (rec, result),
            _ => return,
        };
        let players = &self.world.players;
        let mut step: Option<usize> = None;
        let mut look = self.look;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                theme::eyebrow(
                    ui,
                    &format!("{} / {} of {}", list.title(), at + 1, order.len()),
                );
                ui.label(theme::mono(period_clock(clip)).color(theme::TEXT_MUTE));
            });
            ui.label(RichText::new(tag(clip)).color(theme::ACCENT).strong());
            ui.add_space(6.0);
            paint_floor(ui, rec, time, &mut look, players, result, clip.who);
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                let state = &mut self.room.film;
                if ui
                    .add_enabled(at > 0, egui::Button::new("Previous"))
                    .clicked()
                {
                    step = Some(order[at - 1]);
                }
                if ui
                    .button(if state.playing { "Pause" } else { "Play" })
                    .clicked()
                {
                    state.playing = !state.playing;
                }
                if ui
                    .add_enabled(at + 1 < order.len(), egui::Button::new("Next"))
                    .clicked()
                {
                    step = Some(order[at + 1]);
                }
                ui.add_space(6.0);
                for speed in [0.5, 1.0, 2.0] {
                    if ui
                        .add(egui::Button::selectable(
                            state.speed == speed,
                            format!("{speed}x"),
                        ))
                        .clicked()
                    {
                        state.speed = speed;
                    }
                }
                ui.add_space(6.0);
                view_buttons(ui, &mut look.mode, false);
            });
            ui.spacing_mut().slider_width = (ui.available_width() - 20.0).max(100.0);
            let mut t = time;
            if ui
                .add(egui::Slider::new(&mut t, clip.start..=clip.end).show_value(false))
                .dragged()
            {
                self.room.film.playing = false;
            }
            self.room.film.time = t;
            ui.add_space(6.0);
            ui.label(theme::eyebrow_text("Play by play"));
            let who = |p: PlayerId| players[p as usize].name.as_str();
            for e in rec
                .events
                .iter()
                .filter(|e| e.time >= clip.start - 0.01 && e.time <= clip.end + 0.01)
                .filter(|e| !matches!(e.kind, EventKind::Shout { .. }))
            {
                let now = e.time <= t;
                let key = (e.time - clip.time).abs() < 0.01;
                let secs = e.clock.ceil().max(0.0) as u32;
                ui.horizontal_top(|ui| {
                    ui.label(
                        theme::mono(format!("{}:{:02}", secs / 60, secs % 60))
                            .color(theme::TEXT_MUTE),
                    );
                    let text = RichText::new(e.text(who)).color(if key {
                        theme::TEXT
                    } else if now {
                        theme::TEXT_DIM
                    } else {
                        theme::LINE_2
                    });
                    ui.add(egui::Label::new(if key { text.strong() } else { text }).wrap());
                });
            }
        });
        self.look = look;
        if let Some(k) = step {
            self.room.film.show(film, list, k);
        }
    }
}

fn side_of(f: &basketman_core::league::Fixture, me: usize) -> usize {
    usize::from(f.home != me)
}
