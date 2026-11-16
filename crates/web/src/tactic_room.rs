//! The Tactics tab, laid out like a coach's board: team instructions on the
//! left, the board in the middle, the squad on the right, and the video
//! room as a second view of the same page.

use basketman_core::command::Command;
use basketman_core::player::{Attr, Player, PlayerId, Position};
use basketman_core::fit::{Choice, Proposal, place_needs};
use basketman_core::scrimmage::{Row, Scrimmage};
use basketman_core::tactics::{
    BOARD_ROLES, Board, Brief, DefenseScheme, MAX_TACTICS, OffenseSet, Press, Role, RotationSlot,
    Tactics,
};
use eframe::egui::{
    self, Align, Align2, FontId, Layout, Rect, RichText, Sense, Ui, UiBuilder, vec2,
};

use crate::app::{App, name, rating};
use crate::pages::dim;
use crate::theme::{self, Backdrop};

/// Which view of the Tactics tab is open.
#[derive(Copy, Clone, PartialEq, Default)]
pub(crate) enum RoomView {
    #[default]
    Board,
    Film,
}

/// How a drag on the board is read.
#[derive(Copy, Clone, PartialEq, Default)]
pub(crate) enum Tool {
    /// Drag a player to where he starts.
    #[default]
    Move,
    /// Drag from a player to where he runs.
    Cut,
    /// Tap the screener, then the man he screens for.
    Screen,
}

/// A scrimmage being watched, possession by possession.
pub(crate) struct Playback {
    pub scrim: Scrimmage,
    /// The possession on the board and the seconds into it.
    pub idx: usize,
    pub t: f32,
    pub paused: bool,
    pub speed: f32,
    /// Seconds the end of a possession has been held.
    pub hold: f32,
    /// What it was run with, so a change of tactics retires it.
    pub with: Tactics,
    pub rows: Vec<Row>,
}

/// Rectangles of the parts of the screen the tour points at, as last drawn.
#[derive(Copy, Clone, Default)]
pub(crate) struct Spots {
    pub board: Option<Rect>,
    pub lines: Option<Rect>,
    pub cards: Option<Rect>,
    pub run: Option<Rect>,
    pub help: Option<Rect>,
    pub question: Option<Rect>,
}

/// What the Tactics tab remembers between frames.
pub(crate) struct Room {
    pub view: RoomView,
    /// The board shows the offense (true) or how the defense sets up.
    pub with_ball: bool,
    /// The player picked out on the board or in the squad list.
    pub selected: Option<PlayerId>,
    pub film: crate::film::FilmState,
    pub tool: Tool,
    /// Boards before each edit, newest last, for Undo.
    pub history: Vec<Board>,
    /// The first player tapped for a screen.
    pub tap: Option<usize>,
    /// The card under the pointer, shown on the board before it is chosen.
    pub hover: Option<Choice>,
    pub hover_next: Option<Choice>,
    /// A choice shown on the board until cleared (the harness).
    pub pinned: Option<Choice>,
    /// The drawing tools are open.
    pub edit: bool,
    /// The offense the defense is shown against, when it is not the
    /// opponent's own.
    pub sample: Option<OffenseSet>,
    /// Test against a sample club rather than the next opponent.
    pub vs_sample: bool,
    /// Show the opponent's defense on the board.
    pub show_them: bool,
    pub play: Option<Playback>,
    pub proposal: Option<Proposal>,
    /// The tour step open, if any.
    pub tour: Option<usize>,
    pub tour_checked: bool,
    pub spots: Spots,
}

impl Default for Room {
    fn default() -> Self {
        Room {
            view: RoomView::Board,
            with_ball: true,
            selected: None,
            film: Default::default(),
            tool: Tool::Move,
            history: Vec::new(),
            tap: None,
            hover: None,
            hover_next: None,
            pinned: None,
            edit: false,
            sample: None,
            vs_sample: false,
            show_them: true,
            play: None,
            proposal: None,
            tour: None,
            tour_checked: false,
            spots: Spots::default(),
        }
    }
}

const ROW: f32 = 28.0;

/// A tiny meter: `fraction` of the way up, green when high, amber when low.
fn meter(ui: &Ui, rect: Rect, fraction: f32) {
    let f = fraction.clamp(0.0, 1.0);
    let color = if f >= 0.7 {
        theme::POS
    } else if f >= 0.4 {
        theme::TEXT_MUTE
    } else {
        theme::WARN
    };
    ui.painter().rect_filled(rect, 2.0, theme::LINE);
    let mut fill = rect;
    fill.min.y = rect.max.y - rect.height() * f.max(0.08);
    ui.painter().rect_filled(fill, 2.0, color);
}

/// Fitness, sharpness and morale as three small meters, or the injury.
fn condition(ui: &mut Ui, rect: Rect, p: &Player) {
    if let Some(inj) = &p.injury {
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.label(theme::mono(format!("OUT {}w", inj.weeks_left)).color(theme::WARN))
                    .on_hover_text(format!(
                        "Out {} more weeks: {}",
                        inj.weeks_left,
                        inj.what.label()
                    ));
            },
        );
        return;
    }
    let bars = [(p.condition - 0.7) / 0.3, (p.sharp - 0.5) / 0.5, p.morale];
    for (k, f) in bars.into_iter().enumerate() {
        let x = rect.left() + k as f32 * 9.0;
        meter(
            ui,
            Rect::from_min_size(egui::pos2(x, rect.center().y - 8.0), vec2(6.0, 16.0)),
            f,
        );
    }
    ui.interact(rect, ui.scope_id().with(("cond", p.id)), Sense::hover())
        .on_hover_text(format!(
            "Fitness {:.0}%, sharpness {:.0}%, morale {}",
            p.condition * 100.0,
            p.sharp * 100.0,
            p.mood().to_lowercase()
        ));
}

/// A drop-down for a choice with a one-line explanation of the choice made.
fn pick<T: Copy + PartialEq>(
    ui: &mut Ui,
    salt: &str,
    title: &str,
    value: &mut T,
    all: &[T],
    label: fn(T) -> &'static str,
    describe: fn(T) -> &'static str,
) {
    ui.label(theme::eyebrow_text(title));
    egui::ComboBox::from_id_salt(salt)
        .width(ui.available_width() - 28.0)
        .selected_text(label(*value))
        .show_ui(ui, |ui| {
            for &o in all {
                ui.selectable_value(value, o, label(o))
                    .on_hover_text(describe(o));
            }
        });
    ui.add_space(2.0);
    ui.label(dim(describe(*value)));
}

/// A 1..=20 slider with what more and less of it does.
fn slider(ui: &mut Ui, title: &str, value: &mut u8, hint: &str) {
    ui.label(theme::eyebrow_text(title));
    ui.spacing_mut().slider_width = (ui.available_width() - 64.0).max(80.0);
    ui.add(egui::Slider::new(value, 1..=20));
    ui.label(dim(hint));
}

fn role_note(r: Role) -> &'static str {
    match r {
        Role::PrimaryOption => "The offense runs through him: the most shots.",
        Role::Secondary => "A good share of the shots, behind the first option.",
        Role::RolePlayer => "Takes what the play gives him.",
        Role::Limited => "Rarely shoots. There to defend, rebound and pass.",
    }
}

fn brief_note(b: Brief) -> &'static str {
    match b {
        Brief::Free => "Plays his own game.",
        Brief::Shoot => "Takes the shot when it is there, even an ordinary one.",
        Brief::Pass => "Keeps the ball moving and shoots only the good ones.",
        Brief::Attack => "Puts it on the floor and goes at the basket.",
    }
}

fn place_note(place: usize) -> (&'static str, &'static [Attr]) {
    place_needs(place)
}

fn nominal_place(p: Position) -> usize {
    Position::ALL.iter().position(|&x| x == p).unwrap_or(0)
}

impl App {
    /// The playbook slot open, with somebody always picked out: the first
    /// starter, until the user picks.
    pub(crate) fn tactics_prepare(&mut self) -> usize {
        let team = &self.world.teams[self.team];
        self.slot = self.slot.min(team.playbook.len() - 1);
        let slot = self.slot;
        let rotation = &team.playbook[slot].tactics.rotation;
        if self
            .room
            .selected
            .is_none_or(|s| !rotation.iter().any(|r| r.player == s))
        {
            self.room.selected = rotation.first().map(|r| r.player);
        }
        slot
    }

    /// The playbook as a strip: one tab per tactic, then what you can do
    /// with the open one.
    pub(crate) fn playbook(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let team = &self.world.teams[self.team];
        self.slot = self.slot.min(team.playbook.len() - 1);
        let (mut copy, mut use_it, mut delete) = (false, false, false);
        let mut rename = None;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(theme::eyebrow_text(&format!(
                    "Playbook / {} of {MAX_TACTICS}",
                    team.playbook.len()
                )))
                .on_hover_text(
                    "Drill time is shared between tactics, so a bigger playbook takes longer to \
                     learn. Changing the offense, defense or press sets familiarity back.",
                );
                for (i, slot) in team.playbook.iter().enumerate() {
                    let mark = if i == team.active { "  in use" } else { "" };
                    let label = format!("{}  {:.0}%{mark}", slot.name, slot.familiarity() * 100.0);
                    ui.selectable_value(&mut self.slot, i, label);
                }
                if team.playbook.len() < MAX_TACTICS && ui.button("+ Copy as new").clicked() {
                    copy = true;
                }
                ui.add_space(12.0);
                let slot = &team.playbook[self.slot];
                let mut text = slot.name.clone();
                ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(140.0)
                        .hint_text("Name"),
                );
                if text != slot.name {
                    rename = Some(text);
                }
                if ui
                    .add_enabled(
                        team.active != self.slot,
                        egui::Button::new("Use in matches"),
                    )
                    .clicked()
                {
                    use_it = true;
                }
                if ui
                    .add_enabled(team.playbook.len() > 1, egui::Button::new("Delete"))
                    .clicked()
                {
                    delete = true;
                }
                ui.add_space(8.0);
                ui.add(
                    egui::ProgressBar::new(slot.familiarity())
                        .desired_width(140.0)
                        .desired_height(8.0)
                        .fill(theme::ACCENT),
                );
                ui.label(theme::mono(format!(
                    "{:.0}% familiar",
                    slot.familiarity() * 100.0
                )));
                if slot.changed() {
                    ui.label(RichText::new("system changed, not drilled yet").color(theme::WARN));
                }
            });
        });
        if copy
            && let Ok(out) = self.world.apply(Command::CopyTactic { from: self.slot })
            && let Some(i) = out.slot
        {
            self.slot = i;
        }
        if let Some(name) = rename {
            let _ = self.world.apply(Command::RenameTactic {
                slot: self.slot,
                name,
            });
        }
        if use_it {
            let _ = self.world.apply(Command::UseTactic(self.slot));
        }
        if delete && self.world.apply(Command::DeleteTactic(self.slot)).is_ok() {
            self.slot = self
                .slot
                .min(self.world.teams[self.team].playbook.len() - 1);
        }
    }

    /// Offense, defense, press and the four sliders, each with what it
    /// means right under it.
    pub(crate) fn instructions(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let before = self.world.teams[self.team].playbook[self.slot]
            .tactics
            .clone();
        let mut t = before.clone();
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Team instructions");
            ui.add_space(8.0);
            pick(
                ui,
                "offense",
                "Offense",
                &mut t.offense,
                &OffenseSet::ALL,
                OffenseSet::label,
                OffenseSet::describe,
            );
            ui.add_space(10.0);
            pick(
                ui,
                "defense",
                "Defense",
                &mut t.defense,
                &DefenseScheme::ALL,
                DefenseScheme::label,
                DefenseScheme::describe,
            );
            ui.add_space(10.0);
            pick(
                ui,
                "press",
                "Press",
                &mut t.press,
                &Press::ALL,
                Press::label,
                Press::describe,
            );
            ui.add_space(14.0);
            ui.separator();
            ui.add_space(6.0);
            slider(
                ui,
                "Pace",
                &mut t.pace,
                "Higher: shoot earlier in the shot clock, more possessions, more tired legs. \
                 Lower: work the clock for a better shot.",
            );
            ui.add_space(8.0);
            slider(
                ui,
                "Three-point emphasis",
                &mut t.three_bias,
                "Higher: shooters let it fly more readily. Lower: more of the shots come inside.",
            );
            ui.add_space(8.0);
            slider(
                ui,
                "Crash the offensive boards",
                &mut t.crash_boards,
                "Higher: more second chances, slower getting back. Lower: get back and defend.",
            );
            ui.add_space(8.0);
            slider(
                ui,
                "Defensive aggression",
                &mut t.aggression,
                "Higher: more steals and more fouls. Lower: stay in front of your man.",
            );
        });
        if t != before {
            let _ = self.world.apply(Command::SetTactics {
                slot: self.slot,
                tactics: t,
            });
        }
    }

    /// The rotation, starters first, then the bench, then everybody else.
    pub(crate) fn squad_card(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let before = self.world.teams[self.team].playbook[self.slot]
            .tactics
            .clone();
        let mut t = before.clone();
        let players = &self.world.players;
        let team = &self.world.teams[self.team];
        let selected = self.room.selected;
        let sel_index = selected.and_then(|s| t.rotation.iter().position(|r| r.player == s));
        let mut pick: Option<PlayerId> = None;
        let mut swap: Option<(usize, usize)> = None;
        let mut remove: Option<usize> = None;
        let mut add: Option<PlayerId> = None;
        let can_remove = t.rotation.len() > 5;

        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            let total: u32 = t.rotation.iter().map(|s| s.minutes as u32).sum();
            ui.horizontal(|ui| {
                theme::eyebrow(ui, "Squad");
                let color = if total == 200 {
                    theme::POS
                } else {
                    theme::WARN
                };
                ui.label(theme::mono(format!("{total} / 200 min")).color(color))
                    .on_hover_text(
                        "Five players on the floor for 40 minutes is 200. The coach scales the \
                         minutes to fit, so they do not have to add up.",
                    );
            });
            ui.add_space(4.0);
            let width = ui.available_width();
            // Where each column starts, from the left edge of a row.
            let (x_pos, x_ovr, x_min, x_cond, x_act) = if width < 404.0 {
                (118.0, 146.0, 172.0, 204.0, width - 88.0)
            } else {
                (132.0, 160.0, 188.0, 222.0, (width - 128.0).max(276.0))
            };
            let header = |ui: &mut Ui, title: &str| {
                let (rect, _) = ui.allocate_exact_size(vec2(width, 18.0), Sense::hover());
                for (x, text) in [
                    (8.0, title),
                    (x_pos, "POS"),
                    (x_ovr, "OVR"),
                    (x_min, "MIN"),
                    (x_cond, "F S M"),
                ] {
                    ui.painter().text(
                        rect.left_center() + vec2(x, 0.0),
                        Align2::LEFT_CENTER,
                        text,
                        FontId::new(crate::theme::fs(10.0), egui::FontFamily::Monospace),
                        theme::TEXT_MUTE,
                    );
                }
            };
            let rows = t.rotation.len();
            for i in 0..rows {
                if i == 0 {
                    header(ui, "STARTING FIVE");
                }
                if i == 5 {
                    ui.add_space(8.0);
                    header(ui, "BENCH");
                }
                let slot = &t.rotation[i];
                let p = &players[slot.player as usize];
                let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW), Sense::click());
                let chosen = selected == Some(p.id);
                if chosen {
                    ui.painter()
                        .rect_filled(rect, 6.0, theme::ACCENT.gamma_multiply(0.10));
                    ui.painter().rect_filled(
                        Rect::from_min_size(rect.min + vec2(0.0, 4.0), vec2(3.0, ROW - 8.0)),
                        1.5,
                        theme::ACCENT,
                    );
                } else if resp.hovered() {
                    ui.painter().rect_filled(rect, 6.0, theme::LINE);
                }
                if resp.clicked() {
                    pick = Some(p.id);
                }
                let cell =
                    |x: f32, w: f32| Rect::from_min_size(rect.min + vec2(x, 0.0), vec2(w, ROW));
                let at = |ui: &mut Ui, r: Rect, add: &mut dyn FnMut(&mut Ui)| {
                    ui.scope_builder(
                        UiBuilder::new()
                            .max_rect(r)
                            .layout(Layout::left_to_right(Align::Center)),
                        |ui| add(ui),
                    );
                };
                at(ui, cell(8.0, x_pos - 8.0), &mut |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    crate::profile::player_link(ui, p.id, name(&p.name));
                });
                let shown = slot.position.unwrap_or(p.position);
                at(ui, cell(x_pos, 28.0), &mut |ui| {
                    let text = theme::mono(shown.label()).color(theme::ACCENT);
                    ui.label(if slot.position.is_some() {
                        text.underline()
                    } else {
                        text
                    })
                    .on_hover_text(if slot.position.is_some() {
                        "Playing out of his own position"
                    } else {
                        "Plays his own position"
                    });
                });
                at(ui, cell(x_ovr, 26.0), &mut |ui| {
                    ui.label(rating(p.overall(), false));
                });
                at(ui, cell(x_min, 34.0), &mut |ui| {
                    ui.label(theme::mono(slot.minutes.to_string()));
                });
                condition(ui, cell(x_cond, 40.0), p);
                at(ui, cell(x_act, width - x_act), &mut |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let swap_with = match sel_index {
                            Some(s) if s < 5 && i >= 5 => Some((s, i, "Swap in")),
                            Some(s) if s >= 5 && i < 5 => Some((i, s, "Swap out")),
                            _ => None,
                        };
                        // On a phone the row has room for one button.
                        if !(width < 404.0 && swap_with.is_some())
                            && ui
                                .add_enabled(can_remove, egui::Button::new("Drop").small())
                                .on_hover_text("Take him out of the rotation. At least five stay.")
                                .clicked()
                        {
                            remove = Some(i);
                        }
                        if let Some((a, b, label)) = swap_with {
                            let incoming = &players[t.rotation[b].player as usize];
                            let fit = incoming.injury.is_none();
                            if ui
                                .add_enabled(fit, egui::Button::new(label).small())
                                .on_hover_text(if fit {
                                    "The bench man takes the starter's place, minutes and role. \
                                     Both go back to their own position and to playing their game."
                                } else {
                                    "He is injured."
                                })
                                .clicked()
                            {
                                swap = Some((a, b));
                            }
                        }
                    });
                });
            }

            let bench: Vec<PlayerId> = team
                .roster
                .iter()
                .copied()
                .filter(|pid| !t.rotation.iter().any(|s| s.player == *pid))
                .collect();
            if !bench.is_empty() {
                ui.add_space(10.0);
                header(ui, "NOT IN THE ROTATION");
                for pid in bench {
                    let p = &players[pid as usize];
                    let (rect, _) = ui.allocate_exact_size(vec2(width, ROW), Sense::hover());
                    let cell =
                        |x: f32, w: f32| Rect::from_min_size(rect.min + vec2(x, 0.0), vec2(w, ROW));
                    let at = |ui: &mut Ui, r: Rect, add: &mut dyn FnMut(&mut Ui)| {
                        ui.scope_builder(
                            UiBuilder::new()
                                .max_rect(r)
                                .layout(Layout::left_to_right(Align::Center)),
                            |ui| add(ui),
                        );
                    };
                    at(ui, cell(8.0, x_pos - 8.0), &mut |ui| {
                        crate::profile::player_link(ui, p.id, name(&p.name));
                    });
                    at(ui, cell(x_pos, 28.0), &mut |ui| {
                        ui.label(theme::mono(p.position.label()).color(theme::ACCENT));
                    });
                    at(ui, cell(x_ovr, 26.0), &mut |ui| {
                        ui.label(rating(p.overall(), false));
                    });
                    condition(ui, cell(x_cond, 40.0), p);
                    at(ui, cell(x_act, width - x_act), &mut |ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add_enabled(p.injury.is_none(), egui::Button::new("Add").small())
                                .on_hover_text("Put him in the rotation for five minutes.")
                                .on_disabled_hover_text("He is injured.")
                                .clicked()
                            {
                                add = Some(pid);
                            }
                        });
                    });
                }
            }
            ui.add_space(6.0);
            ui.label(dim(
                "Click a player to pick him out here and on the board. Fitness, sharpness and \
                 morale are the three bars, the taller the better.",
            ));
        });

        if let Some(p) = pick {
            self.room.selected = Some(p);
        }
        if let Some((a, b)) = swap {
            // The slots keep their minutes and role; the men change places.
            // Neither carries his old position or brief to the new slot.
            let (pa, pb) = (t.rotation[a].player, t.rotation[b].player);
            t.rotation[a].player = pb;
            t.rotation[b].player = pa;
            for k in [a, b] {
                t.rotation[k].position = None;
                t.rotation[k].brief = Brief::Free;
            }
            self.room.selected = Some(pb);
        }
        if let Some(i) = remove {
            t.rotation.remove(i);
        }
        if let Some(pid) = add {
            t.rotation.push(RotationSlot {
                player: pid,
                minutes: 5,
                role: Role::Limited,
                brief: Brief::Free,
                position: None,
            });
        }
        if t != before {
            let _ = self.world.apply(Command::SetTactics {
                slot: self.slot,
                tactics: t,
            });
        }
    }

    /// The player picked out: his orders, and why the board wants him.
    pub(crate) fn selected_card(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let before = self.world.teams[self.team].playbook[self.slot]
            .tactics
            .clone();
        let mut t = before.clone();
        let Some(index) = self
            .room
            .selected
            .and_then(|s| t.rotation.iter().position(|r| r.player == s))
        else {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Selected player");
                ui.label(dim(
                    "Click a player on the board or in the squad list to see his instructions: \
                     minutes, role, what he is told to do and where he plays.",
                ));
            });
            return;
        };
        let cast = before.cast(&self.world.players);
        let p = &self.world.players[t.rotation[index].player as usize];
        let on_board = cast.iter().position(|c| *c == Some(p.id));
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                theme::eyebrow(ui, "Selected player");
                crate::profile::player_link(ui, p.id, name(&p.name).size(16.0));
                ui.label(theme::mono(p.position.label()).color(theme::ACCENT));
                ui.label(rating(p.overall(), false));
                ui.label(dim(match on_board {
                    Some(k) => format!("starts as {}", BOARD_ROLES[k].to_lowercase()),
                    None if index < 5 => "starts".to_string(),
                    None => "on the bench".to_string(),
                }));
            });
            ui.add_space(8.0);
            ui.columns(2, |cols| {
                let ui = &mut cols[0];
                let slot = &mut t.rotation[index];
                ui.label(theme::eyebrow_text("Minutes"));
                ui.spacing_mut().slider_width = (ui.available_width() - 64.0).max(80.0);
                ui.add(egui::Slider::new(&mut slot.minutes, 1..=40));
                ui.add_space(8.0);
                ui.label(theme::eyebrow_text("Role"));
                egui::ComboBox::from_id_salt("sel_role")
                    .width(ui.available_width() - 28.0)
                    .selected_text(slot.role.label())
                    .show_ui(ui, |ui| {
                        for r in Role::ALL {
                            ui.selectable_value(&mut slot.role, r, r.label());
                        }
                    });
                ui.label(dim(role_note(slot.role)));
                ui.add_space(8.0);
                ui.label(theme::eyebrow_text("Told to"));
                egui::ComboBox::from_id_salt("sel_brief")
                    .width(ui.available_width() - 28.0)
                    .selected_text(slot.brief.label())
                    .show_ui(ui, |ui| {
                        for b in Brief::ALL {
                            ui.selectable_value(&mut slot.brief, b, b.label());
                        }
                    });
                ui.label(dim(brief_note(slot.brief)));
                ui.add_space(8.0);
                ui.label(theme::eyebrow_text("Plays"));
                let at = slot.position.unwrap_or(p.position);
                let known = |pos: Position| format!("{} {:.0}%", pos.label(), p.knows(pos) * 100.0);
                egui::ComboBox::from_id_salt("sel_plays")
                    .width(ui.available_width() - 28.0)
                    .selected_text(known(at))
                    .show_ui(ui, |ui| {
                        for pos in Position::ALL {
                            let pick = (pos != p.position).then_some(pos);
                            ui.selectable_value(&mut slot.position, pick, known(pos));
                        }
                    });
                ui.label(dim(
                    "Where he lines up and whom he guards. Below 100% he plays under his \
                     attributes there, and learns the position week by week.",
                ));

                let ui = &mut cols[1];
                let place = on_board.unwrap_or_else(|| {
                    nominal_place(t.rotation[index].position.unwrap_or(p.position))
                });
                let (note, attrs) = place_note(place);
                ui.label(theme::eyebrow_text(&format!(
                    "What the {} needs",
                    BOARD_ROLES[place].to_lowercase()
                )));
                ui.label(RichText::new(note).color(theme::TEXT));
                ui.add_space(6.0);
                for &a in attrs {
                    let v = p.attrs[a];
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            vec2(128.0, 18.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.add(egui::Label::new(a.name()).truncate());
                            },
                        );
                        let (bar, _) = ui.allocate_exact_size(vec2(90.0, 8.0), Sense::hover());
                        ui.painter().rect_filled(bar, 4.0, theme::LINE);
                        let mut fill = bar;
                        fill.set_width(bar.width() * (v / 20.0).clamp(0.0, 1.0));
                        ui.painter().rect_filled(fill, 4.0, theme::ACCENT);
                        ui.label(rating(v, false));
                    });
                }
                ui.add_space(6.0);
                ui.label(dim(format!(
                    "Condition {:.0}%, sharpness {:.0}%, morale {}.",
                    p.condition * 100.0,
                    p.sharp * 100.0,
                    p.mood().to_lowercase()
                )));
                if on_board.is_none() {
                    ui.label(dim(
                        "He is not one of the five on the board, so these are the attributes \
                         of the place his position would fill.",
                    ));
                }
            });
        });
        if t != before {
            let _ = self.world.apply(Command::SetTactics {
                slot: self.slot,
                tactics: t,
            });
        }
    }
}
