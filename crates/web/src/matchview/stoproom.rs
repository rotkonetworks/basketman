//! The coaching room: the screen that opens over the court when play stops
//! (a timeout of either team, the end of a period, half-time, the tip).
//! Tactics, instructions and talk are changed here and nowhere else: while
//! play is live the coach can only call a timeout and queue substitutions.
//!
//! What can be told to a player is what the engine reads: his brief
//! (`Brief`: look for his shot, move the ball, attack the rim). Who a man
//! guards is shown but not set: the engine matches up by position.

use super::*;
use crate::pages::dim;
use crate::shell::Tier;
use basketman_core::briefing;
use basketman_core::fit::{Choice, Group, Knob};
use basketman_core::game_flow::{Form, Matchup, Read};
use basketman_core::league::Stoppage;
use basketman_core::tactics::{Brief, DefenseScheme, OffenseSet, Press, Tactics};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum RoomTab {
    Read,
    Tactics,
    Players,
    Talk,
}

impl RoomTab {
    const ALL: [RoomTab; 4] = [RoomTab::Read, RoomTab::Tactics, RoomTab::Players, RoomTab::Talk];

    fn label(self) -> &'static str {
        match self {
            RoomTab::Read => "Read",
            RoomTab::Tactics => "Tactics",
            RoomTab::Players => "Players",
            RoomTab::Talk => "Talk",
        }
    }
}

/// The room, from the moment play stops until it resumes.
pub(crate) struct StopRoom {
    /// On screen. Esc puts it away without resuming.
    pub open: bool,
    pub stoppage: Stoppage,
    pub tab: RoomTab,
    /// The right half of a wide screen: tactics or players.
    wide_right: RoomTab,
    start: Orders,
    start_briefs: Vec<(PlayerId, Brief)>,
    start_five: Vec<PlayerId>,
    /// The assistant's read, three or four lines.
    read: Vec<String>,
    matchups: Vec<Matchup>,
    forms: Vec<Form>,
    /// The player whose row is open.
    picked: Option<PlayerId>,
    /// Who comes off, for the swap.
    off: Option<PlayerId>,
    /// What was said here, for the summary.
    pub said: Vec<String>,
    /// Who a word is for.
    word_to: Option<PlayerId>,
}

impl StopRoom {
    pub fn new(live: &LiveRound, world: &World, stoppage: Stoppage) -> StopRoom {
        let side = live.side;
        let result = live.game.result();
        let ids: [Vec<PlayerId>; 2] = [0, 1].map(|s| result.boxes[s].iter().map(|b| b.0).collect());
        let players = [live.game.players(0), live.game.players(1)];
        let read = Read {
            rec: live.game.recording(),
            ids: &ids,
            me: side,
            time: live.game.time(),
            live: Some(&players),
        };
        let name = |p: PlayerId| world.players[p as usize].name.clone();
        let their = live.game.orders(1 - side);
        let set = briefing::Setting {
            name: &name,
            since: live.previous_stop(),
            foul_limit: world.rules.foul_limit,
            theirs: their,
        };
        let lines = if stoppage == Stoppage::Tipoff {
            vec![
                format!(
                    "They run {} and defend in {}.",
                    their.offense.label().to_lowercase(),
                    their.defense.label().to_lowercase()
                ),
                format!(
                    "Your five: {}.",
                    live.game.five(side).iter().map(|&p| surname(&name(p))).collect::<Vec<_>>().join(", ")
                ),
                "Set the plan now: once the ball is up it takes a timeout to change it.".to_string(),
            ]
        } else {
            briefing::briefing(&read, &set)
        };
        let frames = !live.game.recording().frames.is_empty();
        StopRoom {
            open: true,
            stoppage,
            tab: RoomTab::Read,
            wide_right: RoomTab::Tactics,
            start: live.game.orders(side),
            start_briefs: players[side]
                .iter()
                .filter_map(|p| live.game.brief(side, p.id).map(|b| (p.id, b)))
                .collect(),
            start_five: live.game.five(side),
            read: lines,
            matchups: if frames { read.matchups() } else { Vec::new() },
            forms: read.forms(),
            picked: None,
            off: None,
            said: Vec::new(),
            word_to: None,
        }
    }
}

impl StopRoom {
    /// The screenshot harness: pick a tab and open a player's row.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn pick_for_harness(&mut self, tab: Option<&str>, row: Option<usize>, live: &LiveRound) {
        self.tab = match tab {
            Some("tactics") => RoomTab::Tactics,
            Some("players") => RoomTab::Players,
            Some("talk") => RoomTab::Talk,
            _ => RoomTab::Read,
        };
        if matches!(self.tab, RoomTab::Tactics | RoomTab::Players) {
            self.wide_right = self.tab;
        }
        if let Some(n) = row {
            self.picked = live.game.five(live.side).get(n).copied();
        }
    }
}

fn top_down() -> egui::Layout {
    egui::Layout::top_down(egui::Align::Min)
}

/// First initial and surname: two Carters on one squad are common.
fn surname(full: &str) -> String {
    match full.split_once(' ') {
        Some((first, rest)) => format!("{}. {}", first.chars().next().unwrap_or('?'), rest),
        None => full.to_string(),
    }
}

/// The tactics a live game is running: the playbook's, with the orders.
fn tactics_with(base: &Tactics, o: &Orders) -> Tactics {
    let mut t = base.clone();
    t.offense = o.offense;
    t.defense = o.defense;
    t.press = o.press;
    t.pace = o.pace;
    t.three_bias = o.three_bias;
    t.crash_boards = o.crash_boards;
    t.aggression = o.aggression;
    t
}

fn brief_text(b: Brief) -> &'static str {
    match b {
        Brief::Free => "Plays his own game.",
        Brief::Shoot => "Takes the shot when it is there, even an ordinary one.",
        Brief::Pass => "Moves the ball and takes only the good shots.",
        Brief::Attack => "Puts it on the floor and goes at the basket.",
    }
}

fn brief_short(b: Brief) -> &'static str {
    match b {
        Brief::Free => "Free",
        Brief::Shoot => "Shoot",
        Brief::Pass => "Pass",
        Brief::Attack => "Attack",
    }
}

fn length(secs: u32) -> String {
    if secs >= 120 && secs.is_multiple_of(60) {
        format!("{} min", secs / 60)
    } else {
        format!("{secs} s")
    }
}

/// A chip that is a button and can be selected, tall enough for a finger.
fn chip(ui: &mut Ui, on: bool, text: &str) -> egui::Response {
    ui.add(egui::Button::selectable(on, text).min_size(vec2(0.0, 36.0)))
}

impl App {
    /// Open the room for the stoppage the game is in.
    pub(crate) fn open_room(&mut self, live: &LiveRound) {
        let Some(stop) = live.stoppage() else { return };
        if stop == Stoppage::Final {
            return;
        }
        self.touchline.stopped = true;
        self.touchline.tip_seen |= stop == Stoppage::Tipoff;
        self.touchline.notice = Some(match stop {
            Stoppage::Tipoff => "Before the tip. Space to start.".to_string(),
            _ => "Play is stopped. Space to resume.".to_string(),
        });
        self.touchline.room = Some(StopRoom::new(live, &self.world, stop));
    }

    /// The title of a stoppage, in words.
    fn stoppage_title(&self, live: &LiveRound, stop: Stoppage) -> String {
        let n = |side: usize| {
            let f = &self.world.fixtures[live.fixture];
            self.world.teams[[f.home, f.away][side]].name.clone()
        };
        match stop {
            Stoppage::Tipoff => "Before the tip".to_string(),
            Stoppage::Timeout(s) if s == live.side => "Timeout: ours".to_string(),
            Stoppage::Timeout(s) => format!("Timeout: {}", n(s)),
            Stoppage::Break { half: true, .. } => "Half-time".to_string(),
            Stoppage::Break { period, .. } if period >= self.world.rules.periods => {
                "End of regulation. Level: overtime".to_string()
            }
            Stoppage::Break { period, .. } => format!("End of period {period}"),
            Stoppage::Final => "Full time".to_string(),
        }
    }

    /// Draw the room over the court, when it is open.
    pub(crate) fn stop_room(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        tier: Tier,
        live: &LiveRound,
        commands: &mut Vec<Command>,
    ) {
        let Some(mut room) = self.touchline.room.take() else { return };
        if !room.open || live.stoppage() != Some(room.stoppage) {
            // Play went on (or the room was put away): nothing to show.
            if live.stoppage().is_none() {
                return;
            }
            self.touchline.room = Some(room);
            return;
        }
        use crate::side::{EDGE, SAFE};
        let screen = ui.ctx().content_rect();
        let phone = tier.phone();
        let wide = screen.width() >= 1000.0;
        let margin_x = if phone { SAFE.min(12.0) } else { 28.0 };
        let top = screen.top() + if phone { 6.0 } else { 20.0 };
        let bottom = screen.bottom() - if phone { 6.0f32.max(EDGE.min(10.0)) } else { 20.0 };
        let max_w = if wide { 1240.0 } else { screen.width() };
        let w = (screen.width() - 2.0 * margin_x).min(max_w);
        let rect = Rect::from_center_size(
            pos2(screen.center().x, (top + bottom) / 2.0),
            vec2(w, bottom - top),
        );
        // Dim the court and catch the pointer, so nothing under it is
        // touched.
        ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha(110));
        let _ = ui.interact(screen, egui::Id::unique("room_block"), Sense::click_and_drag());
        ui.painter().rect_filled(rect, 14.0, Color32::from_rgb(250, 248, 243));
        ui.painter().rect_stroke(rect, 14.0, Stroke::new(1.0, theme::LINE_2), egui::StrokeKind::Inside);
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink(if phone { 8.0 } else { 14.0 }))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect.shrink(1.0));
        let ui = &mut child;
        ui.spacing_mut().item_spacing.y = 6.0;
        let width = ui.available_width();
        let compact_head = width < 640.0;
        // A phone on its side has no height to spare: the resume button
        // goes up beside the title and the footer is one line.
        let short = screen.height() < 520.0;

        self.room_header(ui, live, &mut room, compact_head, short, commands);
        ui.add_space(2.0);
        if !wide {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let each = ((ui.available_width() - 12.0) / 4.0).max(60.0);
                for t in RoomTab::ALL {
                    let r = ui.add(
                        egui::Button::selectable(room.tab == t, t.label()).min_size(vec2(each, if short { 36.0 } else { 40.0 })),
                    );
                    if r.clicked() {
                        room.tab = t;
                    }
                }
            });
        }
        // Room for the footer: the summary and the resume button.
        let footer_h = if short { 26.0 } else if compact_head { 112.0 } else { 74.0 };
        let body_h = (ui.available_height() - footer_h).max(80.0);
        let body = ui.allocate_ui(vec2(ui.available_width(), body_h), |ui| {
            if wide {
                let gap = 14.0;
                let left_w = ((ui.available_width() - gap) * 0.42).round();
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.allocate_ui_with_layout(vec2(left_w, body_h), top_down(), |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("room_left")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(left_w - 6.0);
                                self.room_read(ui, bd, live, &room);
                                self.room_talk(ui, bd, live, &mut room, commands);
                            });
                    });
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), body_h), top_down(), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            for t in [RoomTab::Tactics, RoomTab::Players] {
                                if chip(ui, room.wide_right == t, t.label()).clicked() {
                                    room.wide_right = t;
                                }
                            }
                        });
                        egui::ScrollArea::vertical()
                            .id_salt(("room_right", room.wide_right.label()))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width() - 6.0);
                                match room.wide_right {
                                    RoomTab::Players => self.room_players(ui, bd, live, &mut room, commands, true),
                                    _ => self.room_tactics(ui, bd, live, commands, 2),
                                }
                            });
                    });
                });
            } else {
                egui::ScrollArea::vertical()
                    .id_salt(("room_body", room.tab.label()))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width() - 4.0);
                        match room.tab {
                            RoomTab::Read => self.room_read(ui, bd, live, &room),
                            RoomTab::Tactics => self.room_tactics(ui, bd, live, commands, 1),
                            RoomTab::Players => self.room_players(ui, bd, live, &mut room, commands, false),
                            RoomTab::Talk => self.room_talk(ui, bd, live, &mut room, commands),
                        }
                    });
            }
        });
        let _ = body;

        // The footer: what was changed, and the way back into the game.
        ui.add_space(2.0);
        let summary = self.room_summary(live, &room);
        let resume_label = if room.stoppage == Stoppage::Tipoff { "Tip off" } else { "Resume play" };
        if short {
            ui.label(RichText::new(&summary).size(12.5).color(theme::TEXT_DIM));
        } else if compact_head {
            ui.label(RichText::new(&summary).size(13.0).color(theme::TEXT_DIM));
            if ui
                .add_sized(
                    [ui.available_width(), 46.0],
                    egui::Button::new(RichText::new(resume_label).family(theme::semibold()).color(theme::BG))
                        .fill(theme::TEXT),
                )
                .clicked()
            {
                commands.push(Command::Resume);
            }
        } else {
            ui.horizontal(|ui| {
                let btn_w = 190.0;
                ui.allocate_ui_with_layout(vec2((ui.available_width() - btn_w - 12.0).max(100.0), 46.0), top_down(), |ui| {
                    ui.label(RichText::new(&summary).size(13.5).color(theme::TEXT_DIM));
                });
                if ui
                    .add_sized(
                        [btn_w, 46.0],
                        egui::Button::new(RichText::new(resume_label).family(theme::semibold()).color(theme::BG))
                            .fill(theme::TEXT),
                    )
                    .on_hover_text("Space")
                    .clicked()
                {
                    commands.push(Command::Resume);
                }
            });
        }
        self.touchline.room = Some(room);
    }

    fn room_header(
        &self,
        ui: &mut Ui,
        live: &LiveRound,
        room: &mut StopRoom,
        compact: bool,
        short: bool,
        commands: &mut Vec<Command>,
    ) {
        let side = live.side;
        let result = live.game.result();
        let f = &self.world.fixtures[live.fixture];
        let teams = [f.home, f.away].map(|t| self.world.teams[t].name.clone());
        let frame = live.game.recording().frames.last();
        let (period, clk) = frame.map_or((0u8, self.world.rules.period_secs as f32), |f| (f.period, f.clock));
        let title = self.stoppage_title(live, room.stoppage);
        let secs = room.stoppage.seconds();
        let before = room.stoppage == Stoppage::Tipoff;
        let mut close = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(&title).family(theme::semibold()).size(if compact { 18.0 } else { 22.0 }).color(theme::TEXT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if short {
                    let label = if room.stoppage == Stoppage::Tipoff { "Tip off" } else { "Resume play" };
                    if ui
                        .add(
                            egui::Button::new(RichText::new(label).family(theme::semibold()).color(theme::BG))
                                .fill(theme::TEXT)
                                .min_size(vec2(0.0, 36.0)),
                        )
                        .clicked()
                    {
                        commands.push(Command::Resume);
                    }
                }
                if !(before && live.game.recording().frames.is_empty())
                    && ui
                        .add(egui::Button::new(if short { "Hide" } else { "Back to the game" }).min_size(vec2(0.0, 36.0)))
                        .on_hover_text("Esc. Play stays stopped; Space resumes it.")
                        .clicked()
                {
                    close = true;
                }
                if !compact {
                    ui.label(theme::mono(format!("{} break", length(secs))).color(theme::TEXT_MUTE));
                }
            });
        });
        let score = format!(
            "{} {} - {} {}",
            teams[0], result.score[0], result.score[1], teams[1]
        );
        // They are dealt at the start of the first period.
        let (mine, theirs) = if before {
            (self.world.rules.timeouts[0], self.world.rules.timeouts[0])
        } else {
            (live.game.timeouts_left(side), live.game.timeouts_left(1 - side))
        };
        let to_color = if mine == 0 { theme::WARN } else { theme::TEXT_DIM };
        if short {
            // One row: nothing here may cost height.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                ui.label(RichText::new(score).strong().size(14.0).color(theme::TEXT));
                ui.label(theme::mono(format!("P{} {}", period + 1, clock(clk))).color(theme::TEXT_DIM));
                ui.label(theme::mono(format!("timeouts {mine} v {theirs}")).color(to_color))
                    .on_hover_text("Ours v theirs");
            });
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                ui.label(RichText::new(score).strong().size(15.0).color(theme::TEXT));
                ui.label(theme::mono(format!("P{} {}", period + 1, clock(clk))).color(theme::TEXT_DIM));
                ui.label(theme::mono(format!("timeouts: ours {mine}, theirs {theirs}")).color(to_color));
                if compact {
                    ui.label(theme::mono(format!("{} break", length(secs))).color(theme::TEXT_MUTE));
                }
            });
        }
        if close {
            room.open = false;
        }
    }

    /// What the assistant sees, and what they are running.
    fn room_read(&self, ui: &mut Ui, bd: &Backdrop, live: &LiveRound, room: &StopRoom) {
        let them = 1 - live.side;
        let o = live.game.orders(them);
        let changes = live.game.adjustments(them);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "What is happening");
            for line in &room.read {
                ui.add(egui::Label::new(RichText::new(line).size(14.5)).wrap());
            }
        });
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "What they are running");
            ui.horizontal_wrapped(|ui| {
                ui.label(name(o.offense.label())).on_hover_text(o.offense.describe());
                ui.label(dim("offense,"));
                ui.label(name(o.defense.label())).on_hover_text(o.defense.describe());
                ui.label(dim("defense,"));
                ui.label(name(o.press.label())).on_hover_text(o.press.describe());
                ui.label(dim(format!("press, pace {}.", o.pace)));
            });
            ui.label(match changes {
                0 => dim("Their bench has not changed anything yet."),
                n => RichText::new(format!("Their bench has changed its system {n} time{}.", if n == 1 { "" } else { "s" }))
                    .color(theme::WARN),
            });
        });
    }

    /// Offense, defense, press and the sliders as the tactics screen's
    /// cards, for what the game is running now.
    fn room_tactics(
        &mut self,
        ui: &mut Ui,
        _bd: &Backdrop,
        live: &LiveRound,
        commands: &mut Vec<Command>,
        cols: usize,
    ) {
        let side = live.side;
        let cur = live.game.orders(side);
        let team = &self.world.teams[self.team];
        let base = team.tactics().clone();
        let t = tactics_with(&base, &cur);
        let known = team.familiarity_with(cur.offense, cur.defense, cur.press);
        ui.label(
            RichText::new(format!(
                "The squad knows this system {:.0}%. Changing offense, defense or press costs what they have built up.",
                known * 100.0
            ))
            .size(13.5)
            .color(if known < 0.5 { theme::WARN } else { theme::TEXT_DIM }),
        );
        let fam_of = |c: Choice| match c {
            Choice::Offense(o) => Some(team.familiarity_with(o, t.defense, t.press)),
            Choice::Defense(d) => Some(team.familiarity_with(t.offense, d, t.press)),
            Choice::Press(p) => Some(team.familiarity_with(t.offense, t.defense, p)),
            Choice::Knob(..) => None,
        };
        let fams: Vec<(Group, Vec<(Choice, Option<f32>)>)> = [Group::Offense, Group::Defense, Group::Press]
            .into_iter()
            .map(|g| (g, g.choices().into_iter().map(|c| (c, fam_of(c))).collect()))
            .collect();
        let width = ui.available_width();
        let mut chosen: Option<Choice> = None;
        for (g, choices) in fams {
            ui.add_space(4.0);
            ui.label(theme::eyebrow_text(g.label()));
            let col_w = ((width - 8.0 * (cols as f32 - 1.0)) / cols as f32).floor();
            for row in choices.chunks(cols) {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for (c, fam) in row {
                        ui.allocate_ui_with_layout(vec2(col_w, 10.0), top_down(), |ui| {
                            if self.choice_card_for(ui, *c, col_w, true, &t, *fam) {
                                chosen = Some(*c);
                            }
                        });
                    }
                });
            }
        }
        ui.add_space(4.0);
        ui.label(theme::eyebrow_text("Sliders"));
        for k in Knob::ALL {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(k.label()).strong().color(theme::TEXT));
                for l in 0..3 {
                    let c = Choice::Knob(k, l);
                    if chip(ui, c.is_on(&t), c.title()).clicked() {
                        chosen = Some(c);
                    }
                }
            });
            if let Some(c) = (0..3).map(|l| Choice::Knob(k, l)).find(|c| c.is_on(&t)) {
                ui.label(dim(c.line()).size(13.0));
            }
        }
        if let Some(c) = chosen {
            let mut t2 = t.clone();
            c.apply(&mut t2);
            let o = Orders::of(&t2);
            if o != cur {
                commands.push(Command::Orders(o));
            }
        }
    }

    /// The five on the floor and the bench: how each is doing, his
    /// instruction, a word to him, the substitution.
    fn room_players(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        live: &LiveRound,
        room: &mut StopRoom,
        commands: &mut Vec<Command>,
        _wide: bool,
    ) {
        let side = live.side;
        let limit = self.world.rules.foul_limit;
        let all = live.game.players(side);
        let five = live.game.five(side);
        let opp = live.game.five(1 - side);
        let queued = live.game.queued(side);
        let (_, words) = live.talks_left();
        let man_to_man = matches!(
            live.game.orders(side).defense,
            DefenseScheme::ManToMan | DefenseScheme::SwitchEverything | DefenseScheme::PackLine
        );
        ui.label(
            RichText::new(
                "Tap a player for his instruction, a word, or a substitution. The instruction is what he \
                 does with the ball; who he guards is set by his position.",
            )
            .size(13.0)
            .color(theme::TEXT_DIM),
        );
        let mut order: Vec<&basketman_core::engine::LivePlayer> = five
            .iter()
            .filter_map(|id| all.iter().find(|p| p.id == *id))
            .collect();
        let on_ids: Vec<PlayerId> = order.iter().map(|p| p.id).collect();
        order.extend(all.iter().filter(|p| !on_ids.contains(&p.id)));
        for (n, p) in order.iter().enumerate() {
            let id = p.id;
            let pl = &self.world.players[id as usize];
            let on = n < 5;
            if n == 0 {
                ui.label(theme::eyebrow_text("On the floor"));
            } else if n == 5 {
                ui.add_space(6.0);
                ui.label(theme::eyebrow_text("Bench"));
            }
            let brief = live.game.brief(side, id).unwrap_or_default();
            let form = room.forms.iter().find(|f| f.id == id);
            let picked = room.picked == Some(id);
            let marked = queued.iter().any(|(a, b)| *a == id || *b == id);
            let resp = egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(10, 6))
                .corner_radius(10)
                .fill(if picked { theme::ACCENT.gamma_multiply(0.08) } else { theme::white(150) })
                .stroke(Stroke::new(if picked { 2.0 } else { 1.0 }, if picked { theme::ACCENT } else { theme::LINE_2 }))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.spacing_mut().interact_size.y = 22.0;
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&pl.name).strong().size(15.0).color(theme::TEXT));
                        ui.label(dim(pl.position.label()));
                        let (m, c) = mood(p);
                        if !m.is_empty() {
                            ui.label(RichText::new(m).color(c).small());
                        }
                        if marked {
                            ui.label(RichText::new("swap queued").color(theme::ACCENT).small());
                        }
                        ui.label(RichText::new(brief_short(brief)).color(theme::ACCENT).small().strong());
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let (bar, _) = ui.allocate_exact_size(vec2(56.0, 8.0), Sense::hover());
                        ui.painter().rect_filled(bar, 4.0, theme::LINE);
                        let mut fill = bar;
                        fill.set_width(bar.width() * p.energy.clamp(0.04, 1.0));
                        ui.painter().rect_filled(fill, 4.0, crate::tboard::fit_color(p.energy));
                        ui.label(theme::mono(format!("{:.0}%", p.energy * 100.0)).color(
                            if p.energy < 0.5 { theme::WARN } else { theme::TEXT_DIM },
                        ));
                        ui.label(theme::mono(format!("PF {}/{}", p.line.pf, limit)).color(
                            if p.line.pf + 2 >= limit { theme::WARN } else { theme::TEXT_DIM },
                        ));
                        ui.label(theme::mono(format!(
                            "{} pts {}/{}",
                            p.line.pts, p.line.fgm, p.line.fga
                        )).color(theme::TEXT_DIM));
                        if let Some(f) = form
                            && let Some(h) = f.heat
                        {
                            ui.label(
                                RichText::new(match h {
                                    basketman_core::game_flow::Heat::Hot => "hot",
                                    basketman_core::game_flow::Heat::Cold => "cold",
                                })
                                .small()
                                .color(match h {
                                    basketman_core::game_flow::Heat::Hot => theme::POS,
                                    basketman_core::game_flow::Heat::Cold => theme::WARN,
                                }),
                            );
                        }
                    });
                    if on && man_to_man {
                        // The man he is matched with, and how it is going.
                        let m = room.matchups.iter().find(|m| m.mine == id);
                        let k = five.iter().position(|&x| x == id).unwrap_or(0);
                        let guards = m.and_then(|m| m.guards).or_else(|| opp.get(k).copied());
                        if let Some(g) = guards {
                            let how = m
                                .map(|m| format!("{} of {} against him", m.made_against, m.shots_against))
                                .unwrap_or_default();
                            let verdict = m.and_then(|m| m.verdict()).unwrap_or("");
                            ui.label(
                                RichText::new(format!(
                                    "Guards {}. {how} {verdict}",
                                    self.world.players[g as usize].name
                                ))
                                .size(12.5)
                                .color(if verdict == "getting hurt" { theme::WARN } else { theme::TEXT_DIM }),
                            );
                        }
                    }
                });
            let click = ui.interact(resp.response.rect, ui.scope_id().with(("room_row", id)), Sense::click());
            if click.clicked() {
                room.picked = if picked { None } else { Some(id) };
            }
            if picked {
                bd.card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(theme::eyebrow_text("With the ball"));
                    ui.horizontal_wrapped(|ui| {
                        for b in Brief::ALL {
                            if chip(ui, brief == b, b.label()).clicked() && brief != b {
                                commands.push(Command::Brief(id, b));
                            }
                        }
                    });
                    ui.label(dim(brief_text(brief)).size(13.0));
                    if on && !man_to_man {
                        ui.label(dim("In a zone he guards an area, not a man.").size(12.5));
                    }
                    ui.add_space(2.0);
                    if on {
                        let mut label = "Take him off".to_string();
                        if room.off == Some(id) {
                            label = "Cancel".to_string();
                        }
                        if ui.add(egui::Button::new(label).min_size(vec2(0.0, 36.0))).clicked() {
                            room.off = if room.off == Some(id) { None } else { Some(id) };
                        }
                    } else if let Some(off) = room.off {
                        if !p.fouled_out
                            && ui
                                .add(
                                    egui::Button::new(format!(
                                        "Send on for {}",
                                        surname(&self.world.players[off as usize].name)
                                    ))
                                    .min_size(vec2(0.0, 36.0)),
                                )
                                .clicked()
                        {
                            commands.push(Command::Swap(off, id));
                            room.off = None;
                        }
                    } else if p.fouled_out {
                        ui.label(RichText::new("Fouled out.").color(theme::WARN));
                    } else {
                        ui.label(dim("To bring him on, first take a man off.").size(13.0));
                    }
                    ui.add_space(2.0);
                    ui.label(theme::eyebrow_text(&format!("A word to him ({words} left)")));
                    ui.horizontal_wrapped(|ui| {
                        for s in Shout::ALL {
                            if ui
                                .add_enabled(words > 0, egui::Button::new(s.label()).min_size(vec2(0.0, 36.0)))
                                .on_hover_text(s.description())
                                .clicked()
                            {
                                commands.push(Command::Shout(s, Some(id)));
                                room.said.push(format!("{} to {}", s.label(), surname(&pl.name)));
                            }
                        }
                    });
                });
            }
        }
        ui.add_space(6.0);
        let auto = live.game.auto_subs(side);
        let mut a = auto;
        if ui.checkbox(&mut a, "The assistant makes the other substitutions").changed() {
            commands.push(Command::AutoSubs(a));
        }
        ui.label(dim("A swap you make turns the assistant's off, so he does not undo it.").size(12.5));
    }

    /// Team talk and a word to a player.
    fn room_talk(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        live: &LiveRound,
        room: &mut StopRoom,
        commands: &mut Vec<Command>,
    ) {
        let (team, words) = live.talks_left();
        let all = live.game.players(live.side);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, &format!("To the team ({team} left)"));
            ui.horizontal_wrapped(|ui| {
                for s in Shout::ALL {
                    if ui
                        .add_enabled(team > 0, egui::Button::new(s.label()).min_size(vec2(0.0, 36.0)))
                        .on_hover_text(s.description())
                        .clicked()
                    {
                        commands.push(Command::Shout(s, None));
                        room.said.push(format!("{} to the team", s.label()));
                    }
                }
            });
            ui.label(
                dim(match room.stoppage {
                    Stoppage::Break { half: true, .. } => {
                        "Half-time: two team talks and four words to a player. What is said fades in about five minutes."
                    }
                    Stoppage::Timeout(_) => {
                        "A timeout is short: one team talk and one word to a player. What is said fades in about five minutes."
                    }
                    _ => "One team talk and two words to a player. What is said fades in about five minutes.",
                })
                .size(13.0),
            );
        });
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, &format!("A word to one player ({words} left)"));
            let who = |id: PlayerId| self.world.players[id as usize].name.clone();
            egui::ComboBox::from_id_salt("room_word_to")
                .width(ui.available_width().min(260.0))
                .selected_text(room.word_to.map_or("Pick a player".to_string(), who))
                .show_ui(ui, |ui| {
                    for p in &all {
                        let label = if p.on_court { who(p.id) } else { format!("{} (bench)", who(p.id)) };
                        ui.selectable_value(&mut room.word_to, Some(p.id), label);
                    }
                });
            ui.horizontal_wrapped(|ui| {
                for s in Shout::ALL {
                    let can = words > 0 && room.word_to.is_some();
                    if ui
                        .add_enabled(can, egui::Button::new(s.label()).min_size(vec2(0.0, 36.0)))
                        .on_hover_text(s.description())
                        .clicked()
                        && let Some(id) = room.word_to
                    {
                        commands.push(Command::Shout(s, Some(id)));
                        room.said.push(format!("{} to {}", s.label(), surname(&who(id))));
                    }
                }
            });
            if live.game.voice(live.side) > 0.55 {
                ui.label(
                    RichText::new("Your voice is worn from talking. It lands softer for a few minutes.")
                        .color(theme::WARN),
                );
            }
        });
        if let Some((shout, reactions)) = &self.touchline.said {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, &format!("How it was taken: {}", shout.label()));
                for (pid, reaction) in reactions {
                    let color = match reaction {
                        r if r.is_bad() => theme::WARN,
                        Reaction::Unmoved => theme::TEXT_MUTE,
                        _ => theme::POS,
                    };
                    ui.horizontal(|ui| {
                        ui.label(&self.world.players[*pid as usize].name);
                        ui.label(RichText::new(reaction.label()).color(color));
                    });
                }
            });
        }
    }

    /// One or two plain lines on what has been changed here.
    fn room_summary(&self, live: &LiveRound, room: &StopRoom) -> String {
        let side = live.side;
        let now = live.game.orders(side);
        let mut parts: Vec<String> = Vec::new();
        let s = &room.start;
        if now.offense != s.offense {
            parts.push(format!("offense {} -> {}", s.offense.label(), now.offense.label()));
        }
        if now.defense != s.defense {
            parts.push(format!("defense {} -> {}", s.defense.label(), now.defense.label()));
        }
        if now.press != s.press {
            parts.push(format!("press {} -> {}", s.press.label(), now.press.label()));
        }
        for (name, a, b) in [
            ("tempo", s.pace, now.pace),
            ("threes", s.three_bias, now.three_bias),
            ("boards", s.crash_boards, now.crash_boards),
            ("risk", s.aggression, now.aggression),
        ] {
            if a != b {
                parts.push(format!("{name} {a} -> {b}"));
            }
        }
        if (now.offense, now.defense, now.press) != (s.offense, s.defense, s.press) {
            let team = &self.world.teams[self.team];
            parts.push(format!(
                "the squad will know it {:.0}%",
                team.familiarity_with(now.offense, now.defense, now.press) * 100.0
            ));
        }
        for (id, b) in &room.start_briefs {
            let nb = live.game.brief(side, *id).unwrap_or_default();
            if nb != *b {
                parts.push(format!(
                    "{}: {}",
                    surname(&self.world.players[*id as usize].name),
                    brief_short(nb).to_lowercase()
                ));
            }
        }
        let q = live.game.queued(side);
        if !q.is_empty() {
            let who = |id: PlayerId| surname(&self.world.players[id as usize].name);
            parts.push(
                q.iter()
                    .map(|(a, b)| format!("{} on for {}", who(*b), who(*a)))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        let _ = &room.start_five;
        parts.extend(room.said.iter().cloned());
        if parts.is_empty() {
            "Nothing changed yet.".to_string()
        } else {
            format!("Changed: {}.", parts.join("; "))
        }
    }
}

#[allow(dead_code)]
fn _keep(_: OffenseSet, _: Press) {}

#[cfg(test)]
mod tests {
    use super::*;
    use basketman_core::engine::Pause;

    /// Whatever stops the game (either bench's timeout, a period end) is a
    /// stoppage the room opens for, with a read of three or four lines
    /// from the real numbers; and the game never stops without one.
    #[test]
    fn every_stop_of_a_game_has_a_room() {
        let mut w = World::generate(3, 8);
        let mut live = w.kickoff(0).unwrap();
        let tip = StopRoom::new(&live, &w, live.stoppage().unwrap());
        assert!(!tip.read.is_empty() && tip.read.len() <= 4);
        for _ in 0..40 {
            live.step();
        }
        live.input(LiveInput::Timeout);
        let (mut ours, mut theirs, mut breaks) = (0, 0, 0);
        while !live.game.is_over() {
            let Some(pause) = live.step() else { continue };
            if pause == Pause::Final {
                break;
            }
            let stop = live.stoppage().expect("a pause is a stoppage");
            let room = StopRoom::new(&live, &w, stop);
            assert!((1..=4).contains(&room.read.len()), "{:?}", room.read);
            match stop {
                Stoppage::Timeout(s) if s == live.side => ours += 1,
                Stoppage::Timeout(_) => theirs += 1,
                Stoppage::Break { .. } => breaks += 1,
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(ours, 1, "the timeout we asked for");
        assert!(breaks >= 3, "{breaks} breaks");
        let _ = theirs;
    }
}
