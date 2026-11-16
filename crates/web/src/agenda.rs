//! The agenda: today and the next weeks as a list of days, each entry with
//! the actions that belong to it. It sits beside the mail on the Inbox
//! screen and as the day panel of the Calendar. Everything it does goes
//! through `World::apply(Command)`; the list itself is `World::agenda`.

use basketman_core::agenda::{AgendaGame, AgendaItem, TrainingDay, WatchGame};
use basketman_core::command::Command;
use basketman_core::training::{Preset, Schedule, Session};
use eframe::egui::{self, RichText, Ui};

use crate::app::{App, Tab};
use crate::nav::{Link, link};
use crate::theme::{self, Backdrop};

/// Days the agenda looks ahead.
const AHEAD: u32 = 21;

/// Something clicked in the agenda, done once the list is drawn.
enum Act {
    Scout { team: usize, day: u32 },
    Unscout { day: u32 },
    Mail(u32),
    Tactics,
    SetSession { slot: usize, session: Session },
    AddPrep { game_day: u32, lost: Option<u32> },
    RemovePrep { game_day: u32 },
}

fn dim(text: impl Into<String>) -> RichText {
    RichText::new(text).size(12.5).color(theme::TEXT_DIM)
}

fn wrapped(ui: &mut Ui, text: RichText) {
    ui.add(egui::Label::new(text).wrap());
}

impl App {
    /// The agenda panel: why time is waiting, the buttons that move it,
    /// then the days.
    pub(crate) fn agenda_panel(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let narrow = ui.available_width() < 380.0;
        let agenda = self.world.agenda(AHEAD);
        theme::eyebrow(ui, "Agenda");
        ui.add_space(2.0);
        wrapped(ui, RichText::new(&agenda.date).font(egui::FontId::new(18.0, theme::semibold())));
        ui.add_space(4.0);
        wrapped(ui, dim(agenda.hold.reason.clone()));
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| self.continue_button(ui, narrow));
        if let Some(note) = self.sh.agenda_note.clone() {
            ui.add_space(4.0);
            wrapped(ui, RichText::new(note).size(12.5).color(theme::ACCENT));
        }
        ui.add_space(8.0);
        let mut acts = Vec::new();
        for day in &agenda.days {
            if day.items.is_empty() && !day.today {
                continue;
            }
            self.day_block(ui, bd, day.day, &day.label, day.today, &day.items, &mut acts);
        }
        self.do_acts(acts);
    }

    /// One day's entries.
    #[allow(clippy::too_many_arguments)]
    fn day_block(
        &self,
        ui: &mut Ui,
        bd: &Backdrop,
        _day: u32,
        label: &str,
        today: bool,
        items: &[AgendaItem],
        acts: &mut Vec<Act>,
    ) {
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(label)
                        .family(theme::semibold())
                        .size(14.0)
                        .color(if today { theme::ACCENT } else { theme::TEXT }),
                );
                if today {
                    ui.label(theme::eyebrow_text("Today"));
                }
            });
            if items.is_empty() {
                wrapped(ui, dim("Nothing on."));
            }
            for item in items {
                ui.add_space(4.0);
                match item {
                    AgendaItem::Game(g) => self.game_entry(ui, g, acts),
                    AgendaItem::Training(t) => self.training_entry(ui, t, acts),
                    AgendaItem::Scout {
                        day,
                        team_name,
                        vs_name,
                        home,
                        ..
                    } => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(theme::eyebrow_text("Scout"));
                            wrapped(
                                ui,
                                RichText::new(format!(
                                    "At {team_name} {} {vs_name}",
                                    if *home { "v" } else { "at" }
                                ))
                                .size(13.5),
                            );
                        });
                        if ui.small_button("Call him back").clicked() {
                            acts.push(Act::Unscout { day: *day });
                        }
                    }
                    AgendaItem::Deadline { text, message } => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(theme::eyebrow_text("Deadline"));
                            wrapped(ui, RichText::new(text).size(13.5).color(theme::WARN));
                            if let Some(m) = message
                                && ui.small_button("Open").clicked()
                            {
                                acts.push(Act::Mail(*m));
                            }
                        });
                    }
                    AgendaItem::Answer { message, subject } => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(theme::eyebrow_text("Answer"));
                            wrapped(ui, RichText::new(subject).size(13.5).color(theme::WARN));
                            if ui.small_button("Open").clicked() {
                                acts.push(Act::Mail(*message));
                            }
                        });
                    }
                    AgendaItem::Marker { text } => {
                        wrapped(ui, RichText::new(text).size(13.5).color(theme::POS));
                    }
                }
            }
        });
        ui.add_space(6.0);
    }

    fn game_entry(&self, ui: &mut Ui, g: &AgendaGame, acts: &mut Vec<Act>) {
        let kind = match g.competition {
            "friendly" => "Friendly",
            "playoff" => "Playoff",
            _ => "League",
        };
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            ui.label(theme::eyebrow_text(kind));
            ui.label(RichText::new(if g.home { "Home v" } else { "Away at" }).size(13.5));
            link(
                ui,
                Link::Club(g.opponent),
                RichText::new(&g.opponent_name)
                    .family(theme::semibold())
                    .size(14.0),
            );
        });
        if let Some(s) = g.score {
            let (txt, col) = if s[0] > s[1] {
                (format!("Won {}-{}", s[0], s[1]), theme::POS)
            } else {
                (format!("Lost {}-{}", s[0], s[1]), theme::WARN)
            };
            wrapped(ui, RichText::new(txt).size(13.0).color(col));
            return;
        }
        let mut place = format!("{}", g.city);
        if g.competition != "friendly" {
            place += &format!(", {} in the table, {}", basketman_core::budget::ordinal(g.rank), g.record);
            if !g.form.is_empty() {
                place += &format!(", form {}", g.form);
            }
        }
        wrapped(ui, dim(place));
        if let Some(t) = &g.trip {
            let text = format!("{} km: {}.", t.km, t.kind);
            wrapped(ui, if t.long { RichText::new(text).size(12.5).color(theme::WARN) } else { dim(text) });
            if t.long {
                wrapped(ui, dim("A long trip costs the day before's training and some energy."));
            }
        }
        if g.competition == "friendly" {
            return;
        }
        // The scout's report.
        match &g.report {
            Some(r) => {
                let mut line = format!("Report from {}, confidence {}.", r.date, r.confidence);
                if let (Some(o), Some(d)) = (r.offense, r.defense) {
                    line += &format!(" Likely setup: {}, {}.", o.to_lowercase(), d.to_lowercase());
                }
                wrapped(ui, RichText::new(line).size(13.0));
                ui.horizontal_wrapped(|ui| {
                    if let Some(m) = r.message
                        && ui.small_button("Open report").clicked()
                    {
                        acts.push(Act::Mail(m));
                    }
                });
            }
            None => wrapped(ui, RichText::new("No scout report on them yet.").size(13.0).color(theme::WARN)),
        }
        // Games of theirs the scout can still watch.
        let before: Vec<&WatchGame> = g.watch.iter().filter(|w| w.before_meeting).collect();
        if before.is_empty() {
            if g.report.is_none() {
                wrapped(ui, dim("None of their games is left to watch before we meet."));
            }
        } else {
            wrapped(ui, dim("Their games the scout can watch before ours:"));
            for w in before {
                self.watch_row(ui, g.opponent, w, acts);
            }
        }
        // Preparation.
        ui.horizontal_wrapped(|ui| {
            ui.label(theme::eyebrow_text("Preparation"));
            wrapped(
                ui,
                RichText::new(format!("{} session{} set", g.prep_sessions, if g.prep_sessions == 1 { "" } else { "s" }))
                    .size(13.0),
            );
        });
        let game_day = self.game_day_of(g);
        ui.horizontal_wrapped(|ui| {
            if ui
                .small_button("Add a prep session")
                .on_hover_text("Turns the latest free training session before the game into match preparation. It helps only as far as the scout's report is right; it replaces a normal session.")
                .clicked()
            {
                acts.push(Act::AddPrep { game_day, lost: g.trip.as_ref().and_then(|t| t.leave_day) });
            }
            if g.prep_sessions > 0 && ui.small_button("Remove one").clicked() {
                acts.push(Act::RemovePrep { game_day });
            }
            if ui.small_button("Tactics").on_hover_text("Open the tactics screen").clicked() {
                acts.push(Act::Tactics);
            }
        });
    }

    fn watch_row(&self, ui: &mut Ui, team: usize, w: &WatchGame, acts: &mut Vec<Act>) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            wrapped(
                ui,
                RichText::new(format!("{}: {} {}", w.label, if w.home { "home to" } else { "away at" }, w.vs_name)).size(12.5),
            );
            if w.assigned {
                ui.label(RichText::new("scout going").size(12.0).color(theme::POS));
                if ui.small_button("Cancel").clicked() {
                    acts.push(Act::Unscout { day: w.day });
                }
            } else if let Some(other) = &w.busy_with {
                if ui
                    .small_button("Switch")
                    .on_hover_text(format!("He is to watch {other} that day. Send him here instead."))
                    .clicked()
                {
                    acts.push(Act::Scout { team, day: w.day });
                }
            } else if ui.small_button("Send scout").clicked() {
                acts.push(Act::Scout { team, day: w.day });
            }
        });
    }

    /// The day of an agenda game, from the calendar.
    fn game_day_of(&self, g: &AgendaGame) -> u32 {
        match g.fixture {
            Some(i) => self.world.fixtures[i].day,
            None => self.world.day,
        }
    }

    fn training_entry(&self, ui: &mut Ui, t: &TrainingDay, acts: &mut Vec<Act>) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(theme::eyebrow_text("Training"));
            if t.lost_to_travel {
                ui.label(RichText::new("lost to the trip").size(13.0).color(theme::WARN));
                return;
            }
            for k in 0..2 {
                let prep = t.prep[k];
                let text = if prep { "Match prep" } else { t.sessions[k] };
                let r = ui
                    .selectable_label(prep, RichText::new(text).size(13.0))
                    .on_hover_text(if prep {
                        "Match preparation against the next opponent. Click for a normal session again."
                    } else {
                        "Click to make this session match preparation."
                    });
                if r.clicked() {
                    let session = if prep {
                        Schedule::preset(Preset::Balanced).sessions[t.slots[k]]
                    } else {
                        Session::MatchPrep
                    };
                    acts.push(Act::SetSession { slot: t.slots[k], session });
                }
            }
        });
    }

    fn do_acts(&mut self, acts: Vec<Act>) {
        for act in acts {
            let res = match act {
                Act::Scout { team, day } => self.world.apply(Command::AssignScout { team, day }),
                Act::Unscout { day } => self.world.apply(Command::UnassignScout { day }),
                Act::Mail(id) => {
                    self.open_mail(id);
                    self.sh.mail_tab = true;
                    self.sh.mail_open = true;
                    continue;
                }
                Act::Tactics => {
                    self.go(Tab::Tactics);
                    continue;
                }
                Act::SetSession { slot, session } => self.world.apply(Command::SetSession { slot, session }),
                Act::AddPrep { game_day, lost } => {
                    let me = self.team;
                    let s = self.world.teams[me].schedule.sessions;
                    let last = ((game_day % 7) as usize).min(5);
                    let lost_wd = lost.map(|d| (d % 7) as usize);
                    let slot = (0..last)
                        .rev()
                        .filter(|w| Some(*w) != lost_wd)
                        .flat_map(|w| [w * 2 + 1, w * 2])
                        .find(|&k| s[k] != Session::MatchPrep);
                    match slot {
                        Some(slot) => self.world.apply(Command::SetSession { slot, session: Session::MatchPrep }),
                        None => {
                            self.sh.agenda_note = Some("No free training session left before the game.".into());
                            continue;
                        }
                    }
                }
                Act::RemovePrep { game_day } => {
                    let me = self.team;
                    let s = self.world.teams[me].schedule.sessions;
                    let last = ((game_day % 7) as usize).min(5);
                    let slot = (0..last * 2).find(|&k| s[k] == Session::MatchPrep);
                    match slot {
                        Some(slot) => self.world.apply(Command::SetSession {
                            slot,
                            session: Schedule::preset(Preset::Balanced).sessions[slot],
                        }),
                        None => continue,
                    }
                }
            };
            self.sh.agenda_note = match res {
                Ok(o) => o.note,
                Err(e) => Some(e.to_string()),
            };
        }
    }

    /// The calendar's day panel: the same entries for one day, with their
    /// actions.
    pub(crate) fn day_actions(&mut self, ui: &mut Ui, bd: &Backdrop, day: u32) {
        let today = self.world.day;
        if day < today {
            return;
        }
        let agenda = self.world.agenda(day - today + 1);
        let Some(d) = agenda.days.into_iter().find(|d| d.day == day) else {
            return;
        };
        if d.items.is_empty() {
            return;
        }
        ui.add_space(6.0);
        theme::eyebrow(ui, "Our day");
        let mut acts = Vec::new();
        self.day_block(ui, bd, d.day, &d.label, d.today, &d.items, &mut acts);
        self.do_acts(acts);
        if let Some(note) = self.sh.agenda_note.clone() {
            wrapped(ui, RichText::new(note).size(12.5).color(theme::ACCENT));
        }
    }
}
