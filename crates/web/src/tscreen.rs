//! The tactics screen: the board in the middle, the cards beside it (or
//! under it on a phone), the coach's help, and the first-visit tour.

use basketman_core::command::Command;
use basketman_core::fit::{Choice, Group, Knob};
use basketman_core::tactics::{DefenseScheme, OffenseSet};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, Order, Rect, RichText, Sense, Ui, UiBuilder,
    pos2, vec2,
};

use crate::app::App;
use crate::pages::dim;
use crate::scenes::Kind;
use crate::screens::{cols, phone_split};
use crate::shell::{Tier, panel, panel_with, tab_row};
use crate::tactic_room::Tool;
use crate::theme::{self, Backdrop};

/// The tabs of the cards panel. The coach's own column stands beside the
/// board on a wide screen; elsewhere it is the first tab.
fn tabs(wide: bool) -> Vec<&'static str> {
    let mut t = vec!["Offense", "Defense", "Style", "Squad", "Drill"];
    if !wide {
        t.insert(0, "Coach");
    }
    t
}

const STEPS: [(&str, &str); 5] = [
    (
        "The board",
        "This is your half court. Your five stand where the offense puts them. The ring on each player shows how well he fits his job.",
    ),
    (
        "The play",
        "The lines are the play: arrows are cuts, the bar is a screen, the zig-zag is a dribble, dashed lines are passes. Pick another offense and they redraw.",
    ),
    (
        "The cards",
        "Every offense, defense and style is a card. It says what it does, what it needs, how well your squad fits it and what it is good against. Point at one to preview it on the board.",
    ),
    (
        "Run it",
        "Press Run it to watch your five play a few possessions against the opponent, played by the real match engine, with the reason for each result.",
    ),
    (
        "Help me choose",
        "Not sure? The assistant coach proposes a whole setup for this squad and this opponent and explains it. Press ? any time to see this tour again.",
    ),
];

impl App {
    pub(crate) fn tactics_screen(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        stage: Rect,
        tier: Tier,
        g: f32,
        full: Rect,
    ) {
        self.tactics_prepare();
        self.retire_stale_run();
        self.harness_tactics();
        if !self.room.tour_checked {
            self.room.tour_checked = true;
            let seen = ui
                .ctx()
                .data_mut(|d| d.get_persisted::<bool>(egui::Id::unique("tactics_tour_seen")))
                .unwrap_or(false);
            if !seen && self.shot.is_none() {
                self.room.tour = Some(0);
            }
        }
        self.room.hover = self.room.hover_next.take().or(self.room.pinned);
        self.room.spots = Default::default();
        self.sh
            .stage
            .hall(ui, Kind::Data, &self.world, self.team, full, full);

        let wide = tier == Tier::Wide;
        let narrow = tier == Tier::Narrow;
        let (left, centre, right) = match tier {
            Tier::Wide => {
                let c = cols(stage, &[300.0, -1.0, 410.0], g);
                (Some(c[0]), c[1], c[2])
            }
            Tier::Medium => {
                let c = cols(stage, &[-1.0, 400.0], g);
                (None, c[0], c[1])
            }
            Tier::Narrow => {
                let want = (stage.width() - 24.0) * 0.84 + 132.0;
                let (a, b) = phone_split(stage, want.min(stage.height() * 0.6), g);
                (None, a, b)
            }
        };

        if let Some(l) = left {
            panel(ui, l, "tactics_coach", |ui| self.coach_tab(ui, bd, false));
        }
        self.centre_panel(ui, bd, centre, narrow);
        self.cards_panel(ui, bd, right, wide, narrow);
        self.room.spots.cards = Some(right);
        self.tour_overlay(ui);
    }

    fn centre_panel(&mut self, ui: &mut Ui, bd: &Backdrop, rect: Rect, narrow: bool) {
        let ctx = ui.ctx().clone();
        let _ = ctx;
        panel(ui, rect, "tactics_centre", |ui| {
            let w = ui.available_width();
            self.board_toolbar(ui, narrow);
            ui.add_space(4.0);
            let overhead = match (narrow, self.room.play.is_some()) {
                (true, true) => 100.0,
                (true, false) => 62.0,
                _ => 120.0,
            };
            let avail = (rect.height() - 20.0 - ui.cursor().top() + rect.top() - overhead).max(200.0);
            let h = (w * 12.6 / 15.0).min(avail);
            self.tactics_board(ui, vec2(w, h));
            ui.add_space(4.0);
            if self.room.play.is_some() {
                self.playback_bar(ui);
            } else {
                let text = self.board_caption();
                ui.label(RichText::new(text).size(if narrow { 12.5 } else { 13.5 }).color(theme::TEXT));
            }
            if !narrow {
                ui.add_space(8.0);
                self.result_strip(ui, bd);
            }
        });
    }

    /// The buttons over the board.
    fn board_toolbar(&mut self, ui: &mut Ui, narrow: bool) {
        let mut with_ball = self.room.with_ball;
        let mut run = false;
        let mut undo = false;
        let mut reset = false;
        let mut edit = self.room.edit;
        let mut tool = self.room.tool;
        let mut sample = self.room.sample;
        let drawn = {
            let t = &self.world.teams[self.team].playbook[self.slot].tactics;
            t.board.as_ref().is_some_and(|b| b.set == t.offense)
        };
        let can_undo = !self.room.history.is_empty();
        let mut help = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.selectable_value(&mut with_ball, true, if narrow { "Attack" } else { "With the ball" });
            ui.selectable_value(&mut with_ball, false, if narrow { "Defend" } else { "Without the ball" });
            ui.add_space(4.0);
            let r = ui.add(
                egui::Button::new(RichText::new("Run it").color(theme::BG).strong())
                    .fill(theme::TEXT)
                    .corner_radius(8),
            );
            self.room.spots.run = Some(r.rect);
            run = r
                .on_hover_text("Play a few possessions on this board with the real engine.")
                .clicked();
            if with_ball {
                ui.toggle_value(&mut edit, if narrow { "Draw" } else { "Draw a play" })
                    .on_hover_text("Move players, draw cuts and set screens, then press Run it to test the play.");
            }
            let q = ui.add(egui::Button::new("?").min_size(vec2(24.0, 0.0)));
            help = Some(q.rect);
            if q.on_hover_text("Show the tour").clicked() {
                self.room.tour = Some(0);
            }
        });
        self.room.spots.question = help;
        if with_ball && edit {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.selectable_value(&mut tool, Tool::Move, "Move players")
                    .on_hover_text("Drag a player to where he stands when the ball comes over half court.");
                ui.selectable_value(&mut tool, Tool::Cut, "Draw cuts")
                    .on_hover_text("Drag from a player to where he runs. Click him to rub the run out.");
                ui.selectable_value(&mut tool, Tool::Screen, "Set a screen")
                    .on_hover_text("Tap the screener, then the man he screens for.");
                if ui.add_enabled(can_undo, egui::Button::new("Undo")).clicked() {
                    undo = true;
                }
                if ui.add_enabled(drawn, egui::Button::new("Stock set")).clicked() {
                    reset = true;
                }
            });
            ui.label(dim(match tool {
                Tool::Move => "Drag a player to where he starts. Then press Run it.",
                Tool::Cut => "Drag from a player to where he runs once the ball is over.",
                Tool::Screen => match self.room.tap {
                    None => "Tap the player who sets the screen.",
                    Some(_) => "Now tap the man he screens for.",
                },
            }).size(12.5));
        }
        if !with_ball {
            let shown = sample.map_or("their offense", |o| o.label());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(dim("Against").size(12.5));
                if narrow {
                    egui::ComboBox::from_id_salt("sample_offense")
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut sample, None, "their offense");
                            for o in OffenseSet::ALL {
                                ui.selectable_value(&mut sample, Some(o), o.label());
                            }
                        });
                } else {
                    ui.selectable_value(&mut sample, None, "their offense");
                    for o in OffenseSet::ALL {
                        ui.selectable_value(&mut sample, Some(o), o.label());
                    }
                }
            });
        }
        self.room.with_ball = with_ball;
        self.room.edit = edit;
        self.room.tool = if edit { tool } else { Tool::Move };
        self.room.sample = sample;
        if !edit {
            self.room.tap = None;
        }
        if run {
            self.run_it();
        }
        if undo {
            while let Some(b) = self.room.history.pop() {
                let mut t = self.world.teams[self.team].playbook[self.slot].tactics.clone();
                if t.layout() != b {
                    t.draw(b);
                    let _ = self.world.apply(Command::SetTactics {
                        slot: self.slot,
                        tactics: t,
                    });
                    break;
                }
            }
        }
        if reset {
            let mut t = self.world.teams[self.team].playbook[self.slot].tactics.clone();
            self.room.history.push(t.layout());
            t.board = None;
            let _ = self.world.apply(Command::SetTactics {
                slot: self.slot,
                tactics: t,
            });
        }
    }

    /// The coach's own column: the playbook, the assistant and the
    /// opponent.
    pub(crate) fn coach_tab(&mut self, ui: &mut Ui, bd: &Backdrop, with_result: bool) {
        self.playbook(ui, bd);
        ui.add_space(8.0);
        let r = ui.scope(|ui| self.help_card(ui, bd)).response.rect;
        self.room.spots.help = Some(r);
        ui.add_space(8.0);
        self.scout_card(ui, bd);
        if with_result {
            ui.add_space(8.0);
            self.result_strip(ui, bd);
        }
    }

    fn cards_panel(&mut self, ui: &mut Ui, bd: &Backdrop, rect: Rect, wide: bool, narrow: bool) {
        let names = tabs(wide);
        let cur = self.sh.tactics_tab.min(names.len() - 1);
        let mut pick = None;
        panel_with(ui, rect, "tactics_cards", false, |ui| {
            pick = tab_row(ui, &names, cur, narrow);
            let name = names[cur];
            let body = |this: &mut App, ui: &mut Ui| {
                let w = ui.available_width();
                match name {
                    "Coach" => this.coach_tab(ui, bd, narrow),
                    "Offense" if narrow => this.card_row(ui, &[Group::Offense]),
                    "Defense" if narrow => this.card_row(ui, &[Group::Defense]),
                    "Style" if narrow => this.card_row(
                        ui,
                        &[
                            Group::Press,
                            Group::Knob(Knob::Pace),
                            Group::Knob(Knob::Threes),
                            Group::Knob(Knob::Boards),
                            Group::Knob(Knob::Gamble),
                        ],
                    ),
                    "Offense" => this.card_group(ui, Group::Offense, w, false),
                    "Defense" => this.card_group(ui, Group::Defense, w, false),
                    "Style" => {
                        for g in [
                            Group::Press,
                            Group::Knob(Knob::Pace),
                            Group::Knob(Knob::Threes),
                            Group::Knob(Knob::Boards),
                            Group::Knob(Knob::Gamble),
                        ] {
                            this.card_group(ui, g, w, false);
                            ui.add_space(6.0);
                        }
                        egui::CollapsingHeader::new("Fine-tune with sliders")
                            .default_open(false)
                            .show(ui, |ui| this.instructions(ui, bd));
                    }
                    "Squad" => {
                        this.squad_card(ui, bd);
                        ui.add_space(8.0);
                        this.selected_card(ui, bd);
                    }
                    _ => this.rehearsal_card(ui, bd),
                }
            };
            let scroll = if narrow && matches!(name, "Offense" | "Defense" | "Style") {
                egui::ScrollArea::vertical()
            } else {
                egui::ScrollArea::vertical()
            };
            scroll
                .id_salt(("tactics_body", name))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    body(self, ui);
                });
        });
        if let Some(p) = pick {
            self.sh.tactics_tab = p;
            match names[p] {
                "Offense" => self.room.with_ball = true,
                "Defense" => self.room.with_ball = false,
                _ => {}
            }
        }
    }

    // ---- the tour ------------------------------------------------------------

    fn tour_overlay(&mut self, ui: &mut Ui) {
        let Some(step) = self.room.tour else { return };
        let ctx = ui.ctx().clone();
        let screen = ctx.content_rect();
        let target = match step {
            0 => self.room.spots.board,
            1 => self.room.spots.lines,
            2 => self.room.spots.cards,
            3 => self.room.spots.run,
            _ => self.room.spots.help.or(self.room.spots.question),
        }
        .or(self.room.spots.board);
        let target = if step == 1 {
            self.room.spots.board.map(|r| r.shrink2(vec2(r.width() * 0.15, r.height() * 0.12)))
        } else {
            target
        };
        let hole = target.map(|r| r.expand(6.0).intersect(screen));
        let painter = ctx.layer_painter(egui::LayerId::new(Order::Foreground, egui::Id::unique("tour_dim")));
        let dim_c = Color32::from_rgba_unmultiplied(20, 16, 30, 150);
        match hole {
            Some(h) => {
                painter.rect_filled(Rect::from_min_max(screen.min, pos2(screen.max.x, h.min.y)), 0.0, dim_c);
                painter.rect_filled(Rect::from_min_max(pos2(screen.min.x, h.max.y), screen.max), 0.0, dim_c);
                painter.rect_filled(Rect::from_min_max(pos2(screen.min.x, h.min.y), pos2(h.min.x, h.max.y)), 0.0, dim_c);
                painter.rect_filled(Rect::from_min_max(pos2(h.max.x, h.min.y), pos2(screen.max.x, h.max.y)), 0.0, dim_c);
                painter.rect_stroke(h, 10.0, egui::Stroke::new(2.5, theme::ACCENT), egui::StrokeKind::Outside);
            }
            None => {
                painter.rect_filled(screen, 0.0, dim_c);
            }
        }
        let w = 310.0f32.min(screen.width() - 24.0);
        let pos = match hole {
            Some(h) if h.center().y < screen.center().y => pos2(h.center().x - w / 2.0, h.max.y + 12.0),
            Some(h) => pos2(h.center().x - w / 2.0, (h.min.y - 190.0).max(8.0)),
            None => pos2(screen.center().x - w / 2.0, screen.center().y - 90.0),
        };
        let pos = pos2(pos.x.clamp(12.0, (screen.width() - w - 12.0).max(12.0)), pos.y.clamp(8.0, (screen.height() - 200.0).max(8.0)));
        let (mut next, mut back, mut done) = (false, false, false);
        // Nothing underneath is touched while the tour is open.
        egui::Area::new(egui::Id::unique("tour_block"))
            .order(Order::Foreground)
            .fixed_pos(screen.min)
            .show(&ctx, |ui| {
                ui.allocate_rect(screen, Sense::click_and_drag());
            });
        egui::Area::new(egui::Id::unique("tour_box"))
            .order(Order::Tooltip)
            .fixed_pos(pos)
            .show(&ctx, |ui| {
                egui::Frame::NONE
                    .fill(theme::BG)
                    .corner_radius(12)
                    .inner_margin(egui::Margin::same(14))
                    .stroke(egui::Stroke::new(1.5, theme::ACCENT))
                    .show(ui, |ui| {
                        ui.set_width(w - 28.0);
                        ui.label(theme::mono(format!("{} / {}", step + 1, STEPS.len())).color(theme::TEXT_MUTE));
                        ui.label(RichText::new(STEPS[step].0).strong().size(16.0).color(theme::TEXT));
                        ui.add_space(2.0);
                        ui.label(RichText::new(STEPS[step].1).size(13.5));
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.add_enabled(step > 0, egui::Button::new("Back")).clicked() {
                                back = true;
                            }
                            if step + 1 < STEPS.len() {
                                if ui.button("Next").clicked() {
                                    next = true;
                                }
                            } else if ui.button("Done").clicked() {
                                done = true;
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button("Skip").clicked() {
                                    done = true;
                                }
                            });
                        });
                    });
            });
        let _ = (FontId::default(), Align2::CENTER_CENTER, Sense::hover(), UiBuilder::new());
        if next {
            self.room.tour = Some(step + 1);
        } else if back {
            self.room.tour = Some(step.saturating_sub(1));
        } else if done {
            self.room.tour = None;
            ctx.data_mut(|d| d.insert_persisted(egui::Id::unique("tactics_tour_seen"), true));
        }
    }

    // ---- the harness ---------------------------------------------------------

    /// `SHOT_TACTICS=run,defense,help,edit,tour=2,tab=3,preview=fiveout` sets
    /// the screen up for a screenshot. `SHOT_POSS=n` and `SHOT_RUNT=secs`
    /// place the playback.
    fn harness_tactics(&mut self) {
        if self.shot.is_none() || self.room.tour_checked {
            return;
        }
        let Ok(spec) = std::env::var("SHOT_TACTICS") else {
            return;
        };
        for item in spec.split(',') {
            let (k, v) = item.split_once('=').unwrap_or((item, ""));
            match k {
                "defense" => self.room.with_ball = false,
                "edit" => self.room.edit = true,
                "tool" => {
                    self.room.edit = true;
                    self.room.tool = match v {
                        "cut" => Tool::Cut,
                        "screen" => Tool::Screen,
                        _ => Tool::Move,
                    }
                }
                "help" => {
                    self.room.proposal = Some(basketman_core::fit::propose(&self.world, self.team, self.slot, true))
                }
                "tour" => self.room.tour = v.parse().ok().or(Some(0)),
                "tab" => self.sh.tactics_tab = v.parse().unwrap_or(0),
                "sample" => self.room.vs_sample = true,
                "preview" => {
                    self.room.pinned = match v {
                        "fiveout" => Some(Choice::Offense(OffenseSet::FiveOut)),
                        "pnr" => Some(Choice::Offense(OffenseSet::PickAndRoll)),
                        "post" => Some(Choice::Offense(OffenseSet::PostUp)),
                        "iso" => Some(Choice::Offense(OffenseSet::Isolation)),
                        "zone23" => Some(Choice::Defense(DefenseScheme::Zone23)),
                        "zone32" => Some(Choice::Defense(DefenseScheme::Zone32)),
                        "pack" => Some(Choice::Defense(DefenseScheme::PackLine)),
                        "switch" => Some(Choice::Defense(DefenseScheme::SwitchEverything)),
                        _ => None,
                    }
                }
                "run" => {
                    self.run_it();
                    let idx = std::env::var("SHOT_POSS").ok().and_then(|n| n.parse().ok()).unwrap_or(0);
                    let t = std::env::var("SHOT_RUNT").ok().and_then(|n| n.parse().ok()).unwrap_or(9.0);
                    if let Some(pb) = self.room.play.as_mut() {
                        pb.idx = idx;
                        pb.t = t;
                        pb.paused = true;
                    }
                }
                _ => {}
            }
        }
    }
}
