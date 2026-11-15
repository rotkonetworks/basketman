//! Training, scouting, comparison and stats screens.

use basketman_core::assistant;
use basketman_core::command::Command;
use basketman_core::engine::BoxLine;
use basketman_core::league::TeamStats;
use basketman_core::player::{Attr, Player, PlayerId, Position};
use basketman_core::scouting::SHORTLIST_MAX;
use basketman_core::training::{DAYS, Intensity, Preset, Schedule, Session, TrainingFocus};
use eframe::egui::{self, RichText, Ui, vec2};

use crate::app::{App, GAP, header_row, name, rating};
use crate::theme::{self, Backdrop};

const COMPARE_MAX: usize = 2;
/// Scouting knowledge needed before a player's character is known.
const CHARACTER_KNOWN_AT: f32 = 0.6;

pub(crate) fn dim(text: impl Into<String>) -> RichText {
    RichText::new(text).color(theme::TEXT_DIM)
}

fn cm(v: f32) -> RichText {
    theme::mono(format!("{v:.0}"))
}

pub(crate) fn pct(made: u16, attempts: u16) -> f32 {
    if attempts == 0 {
        0.0
    } else {
        100.0 * made as f32 / attempts as f32
    }
}

fn two_pct(l: &BoxLine) -> f32 {
    pct(l.fgm - l.tpm, l.fga - l.tpa)
}

/// An attribute estimate: one number when known, a range when not.
pub(crate) fn estimate(lo: f32, hi: f32) -> RichText {
    if hi - lo < 0.8 {
        rating((lo + hi) / 2.0, false)
    } else {
        theme::mono(format!("{lo:.0}-{hi:.0}")).color(theme::TEXT_DIM)
    }
}

/// Colour for the weakest figures in a column: there, but out of the way.
const FADED: egui::Color32 = egui::Color32::from_rgb(0xAE, 0xA9, 0x9E);

/// A table figure coloured by where it stands among the others in its
/// column, so the eye lands on the good ones: the best in green and bold,
/// above the middle in ink, below it grey, the bottom faded. Pass
/// `low_is_good` for columns like turnovers or points allowed.
pub(crate) fn graded(text: String, v: f32, column: &[f32], low_is_good: bool) -> RichText {
    let (below, above) = column.iter().fold((0, 0), |(b, a), &x| {
        (b + (x < v) as usize, a + (x > v) as usize)
    });
    let text = theme::mono(text);
    if below + above == 0 {
        return text.color(theme::TEXT_DIM);
    }
    let (worse, better) = if low_is_good {
        (above, below)
    } else {
        (below, above)
    };
    // Share of the column this figure beats, ties split.
    let n = column.len().max(2) as f32 - 1.0;
    let share = (worse as f32 + (n - worse as f32 - better as f32) / 2.0) / n;
    match share {
        s if s >= 0.85 => text.family(theme::mono_semibold()).color(theme::POS),
        s if s >= 0.55 => text.color(theme::TEXT),
        s if s >= 0.25 => text.color(theme::TEXT_MUTE),
        _ => text.color(FADED),
    }
}

pub(crate) fn faded(text: &str) -> RichText {
    theme::mono(text).color(FADED)
}

/// `graded` for counting stats: nothing is nothing, however many others
/// also have none.
pub(crate) fn graded_count(v: u16, column: &[f32]) -> RichText {
    if v == 0 {
        theme::mono("0").color(FADED)
    } else {
        graded(v.to_string(), v as f32, column, false)
    }
}

pub(crate) fn match_rating(v: f32) -> RichText {
    let text = theme::mono(format!("{v:.1}"));
    match v {
        v if v >= 7.5 => text.color(theme::POS),
        v if v < 5.0 => text.color(theme::WARN),
        _ => text,
    }
}

/// Toggle button that puts a player in the comparison, dropping the older
/// of the two picks when it is full.
pub(crate) fn compare_button(ui: &mut Ui, list: &mut Vec<PlayerId>, p: PlayerId) {
    let on = list.contains(&p);
    if ui
        .selectable_label(on, if on { "Comparing" } else { "Compare" })
        .clicked()
    {
        if on {
            list.retain(|&x| x != p);
        } else {
            if list.len() >= COMPARE_MAX {
                list.remove(0);
            }
            list.push(p);
        }
    }
}

impl App {
    /// Put a player on the shortlist, or take him off.
    pub(crate) fn toggle_shortlist(&mut self, player: PlayerId) {
        let on = !self.world.scouting.is_shortlisted(player);
        let _ = self.world.apply(Command::Shortlist { player, on });
    }

    pub(crate) fn is_mine(&self, p: PlayerId) -> bool {
        self.world.teams[self.team].roster.contains(&p)
    }

    /// Attribute as the user knows it: exact for his own players.
    pub(crate) fn known(&self, p: &Player, a: Attr) -> (f32, f32) {
        if self.is_mine(p.id) {
            (p.attrs[a], p.attrs[a])
        } else {
            self.world.scouting.estimate(p, a)
        }
    }

    pub(crate) fn known_overall(&self, p: &Player) -> (f32, f32) {
        if self.is_mine(p.id) {
            (p.overall(), p.overall())
        } else {
            self.world.scouting.estimate_overall(p)
        }
    }

    pub(crate) fn character(&self, p: &Player) -> RichText {
        if self.is_mine(p.id) || self.world.scouting.knowledge(p.id) >= CHARACTER_KNOWN_AT {
            let kind = p.personality.kind();
            RichText::new(kind.label()).color(if kind.is_concern() {
                theme::WARN
            } else {
                theme::TEXT_DIM
            })
        } else {
            RichText::new("Unknown").color(theme::LINE_2)
        }
    }

    pub(crate) fn club(&self, p: PlayerId) -> &str {
        self.world
            .team_of(p)
            .map_or("Free agent", |t| self.world.teams[t].name.as_str())
    }

    fn body_cells(ui: &mut Ui, p: &Player) {
        ui.label(theme::mono(p.position.label()).color(theme::ACCENT));
        ui.label(theme::mono(p.age.to_string()));
        for v in [
            p.body.height_cm,
            p.body.wingspan_cm,
            p.body.standing_reach_cm,
            p.body.weight_kg,
        ] {
            ui.label(cm(v));
        }
    }

    // ---- Training -------------------------------------------------------

    pub(crate) fn training(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let mut ask = false;
        let mut schedule = self.world.teams[self.team].schedule.clone();
        let mut delegation = (false, false);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            let playbook = self.world.teams[self.team].playbook.len();
            let team = &self.world.teams[self.team];
            let was_delegated = team.delegate_training;
            let mut delegate = was_delegated;
            theme::eyebrow(ui, "Training week");
            ui.label(dim(
                "Ten team sessions, Monday to Friday. Saturday is match day and Sunday is off. \
                 Each player's own focus, below, comes on top.",
            ));
            ui.add_space(6.0);

            let before = team.schedule.clone();
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut delegate, "Leave training to the assistant coach");
                if ui.button("Ask the assistant for a plan").clicked() {
                    ask = true;
                }
                ui.add_space(12.0);
                ui.label(theme::eyebrow_text("Presets"));
                for p in Preset::ALL {
                    if ui.button(p.label()).clicked() {
                        schedule = Schedule::preset(p);
                    }
                }
            });
            ui.add_space(8.0);

            egui::Grid::new("week")
                .spacing(vec2(10.0, 10.0))
                .show(ui, |ui| {
                    ui.label("");
                    header_row(ui, &DAYS);
                    header_row(ui, &["Sat", "Sun"]);
                    ui.end_row();
                    for (half, label) in ["Morning", "Afternoon"].into_iter().enumerate() {
                        ui.label(theme::eyebrow_text(label));
                        for day in 0..DAYS.len() {
                            let i = day * 2 + half;
                            let session = &mut schedule.sessions[i];
                            egui::ComboBox::from_id_salt(("session", i))
                                .width(150.0)
                                .selected_text(session.label())
                                .show_ui(ui, |ui| {
                                    for s in Session::ALL {
                                        ui.selectable_value(session, s, s.label())
                                            .on_hover_text(s.purpose());
                                    }
                                });
                        }
                        ui.label(dim(if half == 0 { "Match" } else { "" }));
                        ui.label(dim(if half == 0 { "Day off" } else { "" }));
                        ui.end_row();
                    }
                });
            // Editing by hand takes the week back from the assistant.
            if schedule != before {
                delegate = false;
            }
            delegation = (was_delegated, delegate);

            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                let fitness = schedule.condition_delta() * 100.0;
                ui.label(theme::eyebrow_text("This week"));
                ui.label(
                    theme::mono(format!("fitness {fitness:+.1}%")).color(if fitness < 0.0 {
                        theme::WARN
                    } else {
                        theme::POS
                    }),
                );
                let drill = schedule.familiarity_gain() * 100.0 / playbook as f32;
                ui.label(theme::mono(format!("familiarity +{drill:.1}% per tactic")));
            });
        });
        if schedule != self.world.teams[self.team].schedule {
            let _ = self.world.apply(Command::SetSchedule(schedule));
        }
        if delegation.0 != delegation.1 {
            let _ = self.world.apply(Command::DelegateTraining(delegation.1));
        }
        if ask {
            let _ = self.world.apply(Command::AskAssistantToPlan);
        }

        ui.add_space(GAP);
        if ui.available_width() > 940.0 {
            ui.columns(2, |cols| {
                self.assistant_card(&mut cols[0], bd);
                self.numbers_card(&mut cols[1], bd);
            });
        } else {
            self.assistant_card(ui, bd);
            ui.add_space(GAP);
            self.numbers_card(ui, bd);
        }
        ui.add_space(GAP);
        self.individual_card(ui, bd);
    }

    fn assistant_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let team = &self.world.teams[self.team];
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Assistant coach");
            if team.assistant_notes.is_empty() {
                ui.label(dim("No plan from the assistant yet. Ask for one, or play a round with training delegated."));
            }
            for note in &team.assistant_notes {
                ui.label(note.to_string());
            }
            if !team.delegate_training {
                ui.add_space(4.0);
                ui.label(dim("You are running training yourself; the notes above are from his last plan."));
            }
        });
    }

    fn numbers_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let mine = self.world.team_stats[self.team];
        let league = self.world.league_totals();
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Team numbers the plan is based on");
            if mine.games == 0 {
                ui.label(dim("No games played yet."));
                return;
            }
            let per = |s: &TeamStats, v: u16| v as f32 / s.games.max(1) as f32;
            // (label, ours, league, true if higher is better)
            let rows = [
                (
                    "Points",
                    per(&mine, mine.own.pts),
                    per(&league, league.own.pts),
                    true,
                ),
                (
                    "Points allowed",
                    per(&mine, mine.opp.pts),
                    per(&league, league.own.pts),
                    false,
                ),
                (
                    "3P%",
                    pct(mine.own.tpm, mine.own.tpa),
                    pct(league.own.tpm, league.own.tpa),
                    true,
                ),
                ("2P%", two_pct(&mine.own), two_pct(&league.own), true),
                (
                    "FT%",
                    pct(mine.own.ftm, mine.own.fta),
                    pct(league.own.ftm, league.own.fta),
                    true,
                ),
                (
                    "Rebounds",
                    per(&mine, mine.own.reb()),
                    per(&league, league.own.reb()),
                    true,
                ),
                (
                    "Turnovers",
                    per(&mine, mine.own.tov),
                    per(&league, league.own.tov),
                    false,
                ),
                (
                    "Opponent 3P%",
                    pct(mine.opp.tpm, mine.opp.tpa),
                    pct(league.own.tpm, league.own.tpa),
                    false,
                ),
                (
                    "Opponent 2P%",
                    two_pct(&mine.opp),
                    two_pct(&league.own),
                    false,
                ),
            ];
            ui.add_space(6.0);
            egui::Grid::new("numbers")
                .striped(true)
                .spacing(vec2(22.0, 7.0))
                .show(ui, |ui| {
                    header_row(ui, &["Per game", "Us", "League", "Gap"]);
                    ui.end_row();
                    for (label, ours, theirs, higher_is_better) in rows {
                        let gap = ours - theirs;
                        let good = (gap >= 0.0) == higher_is_better;
                        ui.label(label);
                        ui.label(theme::mono(format!("{ours:.1}")));
                        ui.label(theme::mono(format!("{theirs:.1}")).color(theme::TEXT_MUTE));
                        ui.label(theme::mono(format!("{gap:+.1}")).color(if good {
                            theme::POS
                        } else {
                            theme::WARN
                        }));
                        ui.end_row();
                    }
                });
        });
    }

    fn individual_card(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let roster = self.world.teams[self.team].roster.clone();
        let mut edits = Vec::new();
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Individual plans");
            ui.label(dim("The assistant suggests the weakest part of each player's job; veterans work on conditioning."));
            ui.add_space(6.0);
            egui::ScrollArea::horizontal().id_salt("individual_scroll").show(ui, |ui| {
                egui::Grid::new("individual").striped(true).spacing(vec2(16.0, 8.0)).show(ui, |ui| {
                    header_row(
                        ui,
                        &["Player", "Pos", "Age", "Ovr", "Pot", "Character", "Fit", "Focus", "Intensity", "Assistant suggests"],
                    );
                    ui.end_row();
                    for pid in roster {
                        let p = &self.world.players[pid as usize];
                        let before = (p.training, p.intensity);
                        let (mut training, mut level) = before;
                        let (focus, intensity) = assistant::plan_individual(p);
                        crate::profile::player_link(ui, pid, name(&p.name));
                        ui.label(theme::mono(p.position.label()).color(theme::ACCENT));
                        ui.label(theme::mono(p.age.to_string()));
                        ui.label(rating(p.overall(), false));
                        ui.label(rating(p.potential, false));
                        let kind = p.personality.kind();
                        ui.label(RichText::new(kind.label()).color(if kind.is_concern() {
                            theme::WARN
                        } else {
                            theme::TEXT_DIM
                        }))
                        .on_hover_text(kind.description());
                        let fit = theme::mono(format!("{:.0}%", p.condition * 100.0));
                        ui.label(if p.condition < 0.85 { fit.color(theme::WARN) } else { fit });
                        egui::ComboBox::from_id_salt(("ifocus", pid)).width(150.0).selected_text(training.label()).show_ui(
                            ui,
                            |ui| {
                                for f in TrainingFocus::ALL {
                                    ui.selectable_value(&mut training, f, f.label());
                                }
                            },
                        );
                        egui::ComboBox::from_id_salt(("iintensity", pid))
                            .width(80.0)
                            .selected_text(level.label())
                            .show_ui(ui, |ui| {
                                for i in Intensity::ALL {
                                    ui.selectable_value(&mut level, i, i.label());
                                }
                            });
                        let suggestion = format!("{}, {}", focus.label(), intensity.label().to_lowercase());
                        let same = (focus, intensity) == (training, level);
                        ui.label(if same { dim(suggestion) } else { RichText::new(suggestion).color(theme::ACCENT) });
                        if before != (training, level) {
                            edits.push(Command::SetTraining { player: pid, focus: training, intensity: level });
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(14.0);
            });
        });
        // Editing by hand takes the week back from the assistant.
        if !edits.is_empty() {
            for cmd in edits {
                let _ = self.world.apply(cmd);
            }
            let _ = self.world.apply(Command::DelegateTraining(false));
        }
    }

    // ---- Scouting -------------------------------------------------------

    pub(crate) fn scouting(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let mut toggle = None;
        self.shortlist_card(ui, bd, &mut toggle);
        ui.add_space(GAP);
        self.reports_card(ui, bd);
        ui.add_space(GAP);
        self.pool_card(ui, bd, &mut toggle);
        if let Some(p) = toggle {
            self.toggle_shortlist(p);
        }
    }

    fn shortlist_card(&mut self, ui: &mut Ui, bd: &Backdrop, toggle: &mut Option<PlayerId>) {
        let shortlist = self.world.scouting.shortlist.clone();
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, &format!("Shortlist / {} of {SHORTLIST_MAX}", shortlist.len()));
            ui.label(dim(
                "A scout attends every game a shortlisted player appears in. Each game adds a report \
                 and narrows the attribute ranges.",
            ));
            if shortlist.is_empty() {
                ui.label("Nobody shortlisted yet. Add players from the pool below.");
                return;
            }
            ui.add_space(6.0);
            egui::ScrollArea::horizontal().id_salt("shortlist_scroll").show(ui, |ui| {
                egui::Grid::new("shortlist").striped(true).spacing(vec2(16.0, 8.0)).show(ui, |ui| {
                    header_row(
                        ui,
                        &[
                            "Player", "Club", "Pos", "Age", "Ht", "Wing", "Reach", "Wt", "Character", "Known", "Ovr",
                            "Pot", "Seen", "Avg", "", "",
                        ],
                    );
                    ui.end_row();
                    for pid in shortlist {
                        let p = &self.world.players[pid as usize];
                        let s = &self.world.scouting;
                        crate::signature_ui::name_with_mark(
                            ui,
                            pid,
                            name(&p.name),
                            &self.world.known_signatures(pid),
                        );
                        match self.world.team_of(pid) {
                            Some(t) => self.club_link(ui, t),
                            None => ui.label(dim("Free agent")),
                        };
                        Self::body_cells(ui, p);
                        ui.label(self.character(p));
                        ui.label(theme::mono(format!("{:.0}%", s.knowledge(pid) * 100.0)));
                        let (lo, hi) = s.estimate_overall(p);
                        ui.label(estimate(lo, hi));
                        let (lo, hi) = s.estimate_potential(p);
                        ui.label(estimate(lo, hi));
                        ui.label(theme::mono(s.reports_for(pid).count().to_string()));
                        match s.average_rating(pid) {
                            Some(avg) => ui.label(match_rating(avg)),
                            None => ui.label(dim("-")),
                        };
                        compare_button(ui, &mut self.compare, pid);
                        if ui.button("Remove").clicked() {
                            *toggle = Some(pid);
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(14.0);
            });
        });
    }

    fn reports_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(
                ui,
                &format!("Scout reports / {} filed", w.scouting.reports.len()),
            );
            if w.scouting.reports.is_empty() {
                ui.label(dim(
                    "Reports arrive after each round a shortlisted player takes part in.",
                ));
                return;
            }
            ui.add_space(6.0);
            egui::ScrollArea::both()
                .id_salt("reports_scroll")
                .max_height(330.0)
                .show(ui, |ui| {
                    egui::Grid::new("reports")
                        .striped(true)
                        .spacing(vec2(16.0, 8.0))
                        .show(ui, |ui| {
                            header_row(
                                ui,
                                &[
                                    "Round",
                                    "Player",
                                    "Game",
                                    "Rating",
                                    "Min",
                                    "Pts",
                                    "Reb",
                                    "Ast",
                                    "Scout's notes",
                                ],
                            );
                            ui.end_row();
                            for r in w.scouting.reports.iter().rev().take(40) {
                                let l = &r.line;
                                ui.label(
                                    theme::mono((r.round + 1).to_string()).color(theme::TEXT_MUTE),
                                );
                                crate::profile::player_link(
                                    ui,
                                    r.player,
                                    name(&w.players[r.player as usize].name),
                                );
                                let place = if r.home { "vs" } else { "at" };
                                ui.label(dim(format!("{place} {}", w.teams[r.opponent].name)));
                                ui.label(match_rating(r.rating));
                                for v in [(l.secs / 60) as u16, l.pts, l.reb(), l.ast] {
                                    ui.label(theme::mono(v.to_string()));
                                }
                                ui.horizontal(|ui| {
                                    if r.observations.is_empty() {
                                        ui.label(dim("solid, nothing to add"));
                                    }
                                    for (i, o) in r.observations.iter().enumerate() {
                                        let text = if i + 1 < r.observations.len() {
                                            format!("{o};")
                                        } else {
                                            o.to_string()
                                        };
                                        ui.label(RichText::new(text).color(if o.is_concern() {
                                            theme::WARN
                                        } else if matches!(
                                            o,
                                            basketman_core::scouting::Observation::SignatureSeen { .. }
                                        ) {
                                            theme::ACCENT
                                        } else {
                                            theme::TEXT
                                        }));
                                    }
                                });
                                ui.end_row();
                            }
                        });
                });
        });
    }

    fn pool_card(&mut self, ui: &mut Ui, bd: &Backdrop, toggle: &mut Option<PlayerId>) {
        // Edited inside the card while the rest of the app is only read.
        let mut scout_pos = self.scout_pos;
        let mut compare = std::mem::take(&mut self.compare);
        let this = &*self;
        let s = &this.world.scouting;
        let mid = |p: &Player| {
            let (lo, hi) = s.estimate_overall(p);
            (lo + hi) / 2.0
        };
        let mut pool: Vec<&Player> = this
            .world
            .players
            .iter()
            .filter(|p| !this.is_mine(p.id) && scout_pos.is_none_or(|pos| pos == p.position))
            .collect();
        pool.sort_by(|a, b| mid(b).total_cmp(&mid(a)));
        let full = s.shortlist.len() >= SHORTLIST_MAX;

        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(theme::eyebrow_text(&format!("Player pool / {} players", pool.len())));
                ui.add_space(10.0);
                ui.selectable_value(&mut scout_pos, None, "All");
                for pos in Position::ALL {
                    ui.selectable_value(&mut scout_pos, Some(pos), pos.label());
                }
            });
            ui.label(dim("Measurements are public. Ability is an estimate until a scout has watched the player."));
            ui.add_space(6.0);
            egui::ScrollArea::both().id_salt("pool_scroll").max_height(460.0).show(ui, |ui| {
                egui::Grid::new("pool").striped(true).spacing(vec2(16.0, 8.0)).show(ui, |ui| {
                    header_row(
                        ui,
                        &[
                            "Player", "Club", "Pos", "Age", "Ht", "Wing", "Reach", "Wt", "Ovr", "Pot", "Pts", "Reb",
                            "Ast", "", "",
                        ],
                    );
                    ui.end_row();
                    for p in pool {
                        crate::signature_ui::name_with_mark(
                            ui,
                            p.id,
                            name(&p.name),
                            &this.known_signatures(p.id),
                        );
                        ui.label(dim(this.club(p.id)));
                        Self::body_cells(ui, p);
                        let (lo, hi) = s.estimate_overall(p);
                        ui.label(estimate(lo, hi));
                        let (lo, hi) = s.estimate_potential(p);
                        ui.label(estimate(lo, hi));
                        let (line, games) = (&this.world.season_stats[p.id as usize], this.world.games_played[p.id as usize]);
                        for v in [line.pts, line.reb(), line.ast] {
                            ui.label(theme::mono(format!("{:.1}", v as f32 / games.max(1) as f32)));
                        }
                        let listed = s.is_shortlisted(p.id);
                        let label = if listed { "Shortlisted" } else { "Shortlist" };
                        if ui.add_enabled(listed || !full, egui::Button::selectable(listed, label)).clicked() {
                            *toggle = Some(p.id);
                        }
                        compare_button(ui, &mut compare, p.id);
                        ui.end_row();
                    }
                });
            });
        });
        self.scout_pos = scout_pos;
        self.compare = compare;
    }

    // ---- Stats ----------------------------------------------------------

    pub(crate) fn stats(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Teams, per game");
            ui.add_space(6.0);
            egui::ScrollArea::horizontal()
                .id_salt("team_stats_scroll")
                .show(ui, |ui| {
                    egui::Grid::new("team_stats")
                        .striped(true)
                        .spacing(vec2(18.0, 8.0))
                        .show(ui, |ui| {
                            header_row(
                                ui,
                                &[
                                    "Team", "G", "Pts", "Opp", "FG%", "3P%", "FT%", "Reb", "Ast",
                                    "Stl", "Blk", "TO", "Opp FG%", "Opp 3P%",
                                ],
                            );
                            ui.end_row();
                            let figures = |t: usize| {
                                let s = &w.team_stats[t];
                                let g = s.games.max(1) as f32;
                                [
                                    s.own.pts as f32 / g,
                                    s.opp.pts as f32 / g,
                                    pct(s.own.fgm, s.own.fga),
                                    pct(s.own.tpm, s.own.tpa),
                                    pct(s.own.ftm, s.own.fta),
                                    s.own.reb() as f32 / g,
                                    s.own.ast as f32 / g,
                                    s.own.stl as f32 / g,
                                    s.own.blk as f32 / g,
                                    s.own.tov as f32 / g,
                                    pct(s.opp.fgm, s.opp.fga),
                                    pct(s.opp.tpm, s.opp.tpa),
                                ]
                            };
                            let all: Vec<[f32; 12]> = (0..w.teams.len()).map(figures).collect();
                            // Points allowed, turnovers and what opponents
                            // shoot are better low.
                            let low = [1, 9, 10, 11];
                            for t in w.table() {
                                let s = &w.team_stats[t];
                                self.club_link(ui, t);
                                ui.label(theme::mono(s.games.to_string()));
                                for (c, v) in all[t].into_iter().enumerate() {
                                    let column: Vec<f32> = all.iter().map(|row| row[c]).collect();
                                    let text = format!("{v:.1}");
                                    ui.label(graded(text, v, &column, low.contains(&c)));
                                }
                                ui.end_row();
                            }
                        });
                    ui.add_space(14.0);
                });
        });

        ui.add_space(GAP);
        self.mvp_card(ui, bd);
        ui.add_space(GAP);
        type Pick = fn(&BoxLine) -> u16;
        let categories: [(&str, Pick); 5] = [
            ("Points", |l| l.pts),
            ("Rebounds", |l| l.reb()),
            ("Assists", |l| l.ast),
            ("Steals", |l| l.stl),
            ("Blocks", |l| l.blk),
        ];
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            for (title, pick) in categories {
                let per =
                    |p: usize| pick(&w.season_stats[p]) as f32 / w.games_played[p].max(1) as f32;
                let mut leaders: Vec<usize> = (0..w.players.len())
                    .filter(|&p| w.games_played[p] > 0)
                    .collect();
                leaders.sort_by(|&a, &b| per(b).total_cmp(&per(a)));
                bd.card(ui, |ui| {
                    ui.set_min_width(250.0);
                    ui.vertical(|ui| {
                        theme::eyebrow(ui, &format!("{title} per game"));
                        if leaders.is_empty() {
                            ui.label(dim("No games played yet."));
                        }
                        egui::Grid::new(("leaders", title))
                            .spacing(vec2(14.0, 6.0))
                            .show(ui, |ui| {
                                for &p in leaders.iter().take(6) {
                                    let player = &w.players[p];
                                    let mine = self.is_mine(player.id);
                                    let text = name(&player.name);
                                    crate::profile::player_link(
                                        ui,
                                        player.id,
                                        if mine {
                                            text.color(theme::ACCENT)
                                        } else {
                                            text
                                        },
                                    );
                                    ui.label(
                                        theme::mono(player.position.label())
                                            .color(theme::TEXT_MUTE),
                                    );
                                    ui.label(theme::mono(format!("{:.1}", per(p))));
                                    ui.end_row();
                                }
                            });
                    });
                });
            }
        });
    }
}
