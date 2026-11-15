//! The coach's side of the game: the reserves, the press, the assistant's
//! rehearsal of the next opponent, and the other bench during a match.

use basketman_core::assistant::{Rehearsal, Trial, rehearse};
use basketman_core::command::Command;
use basketman_core::engine::BoxLine;
use basketman_core::league::LiveRound;
use basketman_core::player::PlayerId;
use eframe::egui::{self, RichText, Ui, vec2};

use crate::app::{App, GAP, header_row, name, rating};
use crate::nav::{Link, link};
use crate::pages::dim;
use crate::theme::{self, Backdrop};

/// Games the assistant plays on paper for each system.
const REHEARSAL_GAMES: u32 = 24;

fn per(line: &BoxLine, games: u16, f: impl Fn(&BoxLine) -> f32) -> String {
    format!("{:.1}", f(line) / games.max(1) as f32)
}

impl App {
    /// The buttons under a question from the press.
    pub(crate) fn press_buttons(&mut self, ui: &mut Ui, i: usize) {
        let Some(p) = self.world.desk.press.get(i).cloned() else {
            return;
        };
        ui.add_space(14.0);
        match p.answer {
            Some(a) => {
                ui.label(dim(format!("You said: {}", p.options[a as usize])));
                ui.label(RichText::new(&p.outcome).color(theme::ACCENT));
            }
            None if self.world.press_open(i) => {
                let mut pick = None;
                ui.horizontal_wrapped(|ui| {
                    for (k, option) in p.options.iter().enumerate() {
                        if ui.button(option).clicked() {
                            pick = Some(k as u8);
                        }
                    }
                });
                ui.label(dim(
                    "How it lands depends on who is in the squad and on how far they trust you.",
                ));
                if let Some(k) = pick {
                    let _ = self.world.apply(Command::AnswerPress {
                        question: i,
                        choice: k,
                    });
                }
            }
            None => {
                ui.label(dim("You gave no answer. The press moved on."));
            }
        }
    }

    /// The reserve squad, who is turning out for it, and its league.
    pub(crate) fn reserves_page(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let me = self.team;
        let mut act: Option<(PlayerId, u8)> = None;
        let side = self.world.reserve_side(me);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Reserve team");
            ui.label(dim(
                "The reserves play the other clubs' reserves every week the first team plays. A \
                 game there builds match sharpness without the strain of a real one, which is \
                 what a man back from injury needs, and it is where the young ones grow. \
                 First-team players marked \"plays for reserves\" turn out for them whenever they \
                 are not in your first-team rotation.",
            ));
            if let Some(note) = &self.deal_note {
                ui.label(RichText::new(note).color(theme::WARN));
            }
        });
        ui.add_space(GAP);

        let stats = |w: &basketman_core::league::World, p: PlayerId| {
            let g = w.reserve_games.get(p as usize).copied().unwrap_or(0);
            let l = w.reserve_stats.get(p as usize).copied().unwrap_or_default();
            (g, l)
        };
        let row = |this: &App, ui: &mut Ui, p: PlayerId| {
            let w = &this.world;
            let pl = &w.players[p as usize];
            crate::profile::player_link(ui, p, name(&pl.name));
            ui.label(theme::mono(pl.position.label()).color(theme::ACCENT));
            ui.label(theme::mono(pl.age.to_string()));
            ui.label(rating(pl.overall(), false));
            let (lo, hi) = (pl.potential - 0.5, pl.potential + 0.5);
            ui.label(theme::mono(format!("{lo:.0}-{hi:.0}")).color(theme::TEXT_DIM));
            match &pl.injury {
                Some(inj) => {
                    ui.label(theme::mono(format!("out {} wk", inj.weeks_left)).color(theme::WARN))
                }
                None => ui.label(theme::mono(format!("{:.0}%", pl.condition * 100.0))),
            };
            let sharp = theme::mono(format!("{:.0}%", pl.sharp * 100.0));
            ui.label(match pl.sharp {
                s if s >= 0.9 => sharp.color(theme::POS),
                s if s < 0.75 => sharp.color(theme::WARN),
                _ => sharp,
            });
            let (g, l) = stats(w, p);
            ui.label(theme::mono(g.to_string()));
            ui.label(theme::mono(per(&l, g, |l| l.secs as f32 / 60.0)));
            ui.label(theme::mono(per(&l, g, |l| l.pts as f32)));
            ui.label(theme::mono(per(&l, g, |l| l.reb() as f32)));
            ui.label(theme::mono(per(&l, g, |l| l.ast as f32)));
        };
        let heads = [
            "Player", "Pos", "Age", "Ovr", "Pot", "Fit", "Sharp", "G", "Min", "Pts", "Reb", "Ast",
            "", "",
        ];

        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            let t = &self.world.teams[me];
            theme::eyebrow(ui, &format!("Reserve squad / {} players", t.reserves.len()));
            ui.add_space(6.0);
            egui::ScrollArea::horizontal()
                .id_salt("reserves_scroll")
                .show(ui, |ui| {
                    egui::Grid::new("reserves")
                        .striped(true)
                        .spacing(vec2(14.0, 8.0))
                        .show(ui, |ui| {
                            header_row(ui, &heads);
                            ui.end_row();
                            for p in t.reserves.clone() {
                                row(self, ui, p);
                                ui.label(if side.contains(&p) {
                                    dim("plays this week")
                                } else {
                                    dim("")
                                });
                                if ui
                                    .small_button("Promote")
                                    .on_hover_text("Move him to the first-team squad.")
                                    .clicked()
                                {
                                    act = Some((p, 0));
                                }
                                ui.end_row();
                            }
                        });
                    ui.add_space(12.0);
                });
        });
        ui.add_space(GAP);

        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "First team");
            ui.label(dim(
                "Sharpness falls every week a player does not play and rises with minutes, here or \
                 in the first team. Below about 75% it shows in his game.",
            ));
            ui.add_space(6.0);
            let t = &self.world.teams[me];
            let rotation: Vec<PlayerId> = t.tactics().rotation.iter().map(|s| s.player).collect();
            egui::ScrollArea::horizontal().id_salt("first_scroll").show(ui, |ui| {
                egui::Grid::new("first_down").striped(true).spacing(vec2(14.0, 8.0)).show(ui, |ui| {
                    header_row(ui, &heads);
                    ui.end_row();
                    for p in t.roster.clone() {
                        row(self, ui, p);
                        let mut on = self.world.teams[me].down.contains(&p);
                        let label = if rotation.contains(&p) {
                            "plays for reserves (in your rotation now)"
                        } else {
                            "plays for reserves"
                        };
                        if ui.checkbox(&mut on, label).changed() {
                            act = Some((p, if on { 2 } else { 3 }));
                        }
                        if ui
                            .small_button("Send down")
                            .on_hover_text("Move him to the reserve squad for good. He will not like it.")
                            .clicked()
                        {
                            act = Some((p, 1));
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(12.0);
            });
        });
        ui.add_space(GAP);

        let wide = ui.available_width() > 940.0;
        let table = |this: &App, ui: &mut Ui| {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Reserve league");
                ui.add_space(6.0);
                let w = &this.world;
                let mut order: Vec<usize> = (0..w.teams.len()).collect();
                order.sort_by_key(|&t| {
                    let s = w.reserve_table.get(t).copied().unwrap_or_default();
                    (
                        std::cmp::Reverse(s.wins),
                        std::cmp::Reverse(s.points_for as i64 - s.points_against as i64),
                    )
                });
                egui::Grid::new("reserve_table")
                    .striped(true)
                    .spacing(vec2(18.0, 7.0))
                    .show(ui, |ui| {
                        header_row(ui, &["", "Club", "W", "L", "Diff"]);
                        ui.end_row();
                        for (i, t) in order.into_iter().enumerate() {
                            let s = w.reserve_table.get(t).copied().unwrap_or_default();
                            ui.label(theme::mono((i + 1).to_string()).color(theme::TEXT_MUTE));
                            this.club_link(ui, t);
                            ui.label(theme::mono(s.wins.to_string()));
                            ui.label(theme::mono(s.losses.to_string()));
                            ui.label(theme::mono(format!(
                                "{:+}",
                                s.points_for as i64 - s.points_against as i64
                            )));
                            ui.end_row();
                        }
                    });
            });
        };
        let results = |this: &App, ui: &mut Ui| {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Our reserve results");
                ui.add_space(6.0);
                let w = &this.world;
                let ours: Vec<_> = w
                    .reserve_log
                    .iter()
                    .rev()
                    .filter(|g| g.home == me || g.away == me)
                    .take(10)
                    .collect();
                if ours.is_empty() {
                    ui.label(dim("No reserve games played yet."));
                }
                for g in ours {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            theme::mono(format!("R{:<2}", g.round + 1)).color(theme::TEXT_MUTE),
                        );
                        ui.label(format!(
                            "{} {} - {} {}",
                            w.teams[g.home].name, g.score[0], g.score[1], w.teams[g.away].name
                        ));
                        if let Some((p, l)) = &g.star {
                            link(
                                ui,
                                Link::Player(*p),
                                RichText::new(&w.players[*p as usize].name).color(theme::TEXT_DIM),
                            );
                            ui.label(dim(format!(
                                "{} pts, {} reb, {} ast",
                                l.pts,
                                l.reb(),
                                l.ast
                            )));
                        }
                    });
                }
            });
        };
        if wide {
            ui.columns(2, |cols| {
                table(self, &mut cols[0]);
                results(self, &mut cols[1]);
            });
        } else {
            table(self, ui);
            ui.add_space(GAP);
            results(self, ui);
        }

        if let Some((p, what)) = act {
            let r = match what {
                0 => self.world.apply(Command::Promote(p)),
                1 => self.world.apply(Command::Demote(p)),
                _ => self.world.apply(Command::SetDown {
                    player: p,
                    on: what == 2,
                }),
            };
            self.deal_note = r.err().map(|why| why.to_string());
        }
    }

    /// The next opponent, and what the assistant found by playing the game
    /// on paper with each system.
    pub(crate) fn rehearsal_card(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let id = egui::Id::unique("rehearsal");
        let w = &self.world;
        let next = w
            .fixtures
            .iter()
            .find(|f| f.result.is_none() && (f.home == self.team || f.away == self.team))
            .map(|f| if f.home == self.team { f.away } else { f.home });
        let Some(opp) = next else { return };
        let mut stored: Option<Rehearsal> = ui.data(|d| d.get_temp(id));
        // Stale once the round is played or the opponent changes.
        if stored
            .as_ref()
            .is_some_and(|r| r.opponent != opp || r.round != w.round)
        {
            stored = None;
        }
        let mut run = false;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            let them = &w.teams[opp];
            let t = them.tactics();
            theme::eyebrow(ui, &format!("Next opponent / {}", them.name));
            ui.horizontal_wrapped(|ui| {
                ui.label("They run");
                ui.label(name(t.offense.label())).on_hover_text(t.offense.describe());
                ui.label("and defend");
                ui.label(name(t.defense.label())).on_hover_text(t.defense.describe());
                ui.label(format!("({}).", t.press.label().to_lowercase()));
            });
            if let Some(head) = them.office.staffer(basketman_core::staff::Job::HeadCoach) {
                ui.label(dim(format!(
                    "{} is on their bench, rated {} for tactics: {}.",
                    head.name,
                    head.tactics,
                    match head.tactics {
                        15.. => "expect him to change things quickly if he is losing",
                        9..=14 => "he will change things if the game is getting away",
                        _ => "he tends to stick with what he started with",
                    }
                )));
            }
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Have the assistant run the game on paper").clicked() {
                    run = true;
                }
                ui.label(dim(format!(
                    "He plays it {REHEARSAL_GAMES} times with each offense and each defense, your \
                     rotation and sliders as they are, at the familiarity the squad has today."
                )));
            });
            let Some(r) = &stored else { return };
            ui.add_space(8.0);
            let table = |ui: &mut Ui, salt: &str, title: &str, trials: &[Trial]| {
                ui.vertical(|ui| {
                    ui.label(theme::eyebrow_text(title));
                    let best = trials.iter().map(|t| t.wins).fold(0.0f32, f32::max);
                    egui::Grid::new(salt).striped(true).spacing(vec2(16.0, 6.0)).show(ui, |ui| {
                        header_row(ui, &["System", "Won", "Margin", "Known"]);
                        ui.end_row();
                        for t in trials {
                            let label = if t.current { format!("{} (now)", t.label) } else { t.label.to_string() };
                            ui.label(if t.current { name(&label) } else { RichText::new(label) });
                            let won = theme::mono(format!("{:.0}%", t.wins * 100.0));
                            ui.label(if t.wins >= best - 0.001 { won.color(theme::POS).strong() } else { won });
                            ui.label(theme::mono(format!("{:+.1}", t.margin)));
                            let known = theme::mono(format!("{:.0}%", t.familiarity * 100.0));
                            ui.label(if t.familiarity < 0.5 { known.color(theme::WARN) } else { known.color(theme::TEXT_DIM) });
                            ui.end_row();
                        }
                    });
                });
            };
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 36.0;
                table(ui, "rehearse_o", "Your offense against them", &r.offense);
                table(ui, "rehearse_d", "Your defense against them", &r.defense);
            });
            ui.add_space(4.0);
            ui.label(dim(format!(
                "{} games each is a small sample: differences of a few points in a hundred are noise. \
                 A system the squad barely knows is played badly, which is part of its number. \
                 Their coach reacts during the game; you can too.",
                r.games
            )));
        });
        if run {
            stored = rehearse(&self.world, self.team, REHEARSAL_GAMES);
        }
        ui.data_mut(|d| match stored {
            Some(r) => {
                d.insert_temp(id, r);
            }
            None => d.remove::<Rehearsal>(id),
        });
    }

    /// The other bench during a live game: what they are running now.
    pub(crate) fn opponent_card(&self, ui: &mut Ui, bd: &Backdrop, live: &LiveRound) {
        let them = 1 - live.side;
        let f = &self.world.fixtures[live.fixture];
        let club = [f.home, f.away][them];
        let o = live.game.orders(them);
        let changes = live.game.adjustments(them);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(
                ui,
                &format!("Their bench / {}", self.world.teams[club].name),
            );
            egui::Grid::new("their_bench")
                .spacing(vec2(16.0, 6.0))
                .show(ui, |ui| {
                    ui.label(dim("Offense"));
                    ui.label(name(o.offense.label()))
                        .on_hover_text(o.offense.describe());
                    ui.end_row();
                    ui.label(dim("Defense"));
                    ui.label(name(o.defense.label()))
                        .on_hover_text(o.defense.describe());
                    ui.end_row();
                    ui.label(dim("Press"));
                    ui.label(name(o.press.label()))
                        .on_hover_text(o.press.describe());
                    ui.end_row();
                    ui.label(dim("Pace"));
                    ui.label(theme::mono(format!("{} / 20", o.pace)));
                    ui.end_row();
                });
            ui.label(match changes {
                0 => dim("They have not changed anything yet."),
                n => RichText::new(format!(
                    "Their coach has changed it {n} time{}. Hover a system to see what beats it.",
                    if n == 1 { "" } else { "s" }
                ))
                .color(theme::WARN),
            });
        });
    }

}
