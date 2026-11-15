//! What is left of finished seasons: every player's year-by-year line,
//! every club's finishes, and the roll of champions.

use basketman_core::engine::BoxLine;
use basketman_core::league::{TeamId, TeamStats};
use basketman_core::player::PlayerId;
use eframe::egui::{self, RichText, Ui, vec2};

use crate::app::{App, header_row, name, rating};
use crate::nav::{Link, link};
use crate::pages::{dim, faded, graded, match_rating, pct};
use crate::theme::{self, Backdrop};

/// A season in the career table: label, club, age, overall if known,
/// games, totals.
type Row = (String, Option<TeamId>, u8, Option<f32>, u16, BoxLine);
/// A per-game team figure and whether low is good.
type TeamFigure = (fn(&TeamStats, f32) -> f32, bool);

impl App {
    /// Season by season, with the current one last and a career line under
    /// it. Figures are per game and graded against his own other seasons.
    pub(crate) fn career_card(&self, ui: &mut Ui, bd: &Backdrop, pid: PlayerId) {
        let w = &self.world;
        let p = &w.players[pid as usize];
        // (label, club, age, overall, games, totals)
        let mut rows: Vec<Row> = w
            .career(pid)
            .iter()
            .map(|c| {
                let mvp = w
                    .history
                    .iter()
                    .any(|r| r.season == c.season && r.mvp == Some(pid));
                let label = format!("Season {}{}", c.season + 1, if mvp { "  MVP" } else { "" });
                (label, c.team, c.age, Some(c.overall), c.games, c.totals)
            })
            .collect();
        let now = w.games_played[pid as usize];
        if !p.is_retired() {
            rows.push((
                format!("Season {} (now)", w.season + 1),
                w.team_of(pid),
                p.age,
                self.is_mine(pid).then(|| p.overall()),
                now,
                w.season_stats[pid as usize],
            ));
        }
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Career");
            ui.add_space(6.0);
            type Pick = fn(&BoxLine, f32) -> f32;
            let picks: [Pick; 9] = [
                |l, g| l.secs as f32 / 60.0 / g,
                |l, g| l.pts as f32 / g,
                |l, g| l.reb() as f32 / g,
                |l, g| l.ast as f32 / g,
                |l, g| l.stl as f32 / g,
                |l, g| l.blk as f32 / g,
                |l, _| pct(l.fgm, l.fga),
                |l, _| pct(l.tpm, l.tpa),
                |l, _| pct(l.ftm, l.fta),
            ];
            let columns: Vec<Vec<f32>> = picks
                .iter()
                .map(|f| {
                    rows.iter()
                        .filter(|r| r.4 > 0)
                        .map(|r| f(&r.5, r.4 as f32))
                        .collect()
                })
                .collect();
            egui::ScrollArea::horizontal()
                .id_salt("career_scroll")
                .show(ui, |ui| {
                    egui::Grid::new("career_grid")
                        .striped(true)
                        .spacing(vec2(16.0, 7.0))
                        .show(ui, |ui| {
                            header_row(
                                ui,
                                &[
                                    "Season", "Club", "Age", "Ovr", "G", "Min", "Pts", "Reb",
                                    "Ast", "Stl", "Blk", "FG%", "3P%", "FT%", "Rtg",
                                ],
                            );
                            ui.end_row();
                            let mut career = BoxLine::default();
                            let mut games = 0u16;
                            for (label, club, age, overall, g, line) in &rows {
                                ui.label(theme::mono(label.as_str()).color(theme::TEXT_DIM));
                                match club {
                                    Some(t) => {
                                        self.club_link(ui, *t);
                                    }
                                    None => {
                                        ui.label(dim("Free agent"));
                                    }
                                }
                                ui.label(theme::mono(age.to_string()));
                                match overall {
                                    Some(v) => ui.label(rating(*v, false)),
                                    None => ui.label(faded("-")),
                                };
                                ui.label(theme::mono(g.to_string()));
                                if *g == 0 {
                                    for _ in 0..picks.len() + 1 {
                                        ui.label(faded("-"));
                                    }
                                    ui.end_row();
                                    continue;
                                }
                                for (f, column) in picks.iter().zip(&columns) {
                                    let v = f(line, *g as f32);
                                    ui.label(graded(format!("{v:.1}"), v, column, false));
                                }
                                ui.label(match_rating(line.rating_per(*g)));
                                ui.end_row();
                                career.add(line);
                                games += g;
                            }
                            if rows.len() > 1 && games > 0 {
                                ui.label(RichText::new("Career").family(theme::semibold()));
                                ui.label("");
                                ui.label("");
                                ui.label("");
                                ui.label(theme::mono(games.to_string()));
                                for f in picks {
                                    let v = f(&career, games as f32);
                                    ui.label(theme::mono(format!("{v:.1}")));
                                }
                                ui.label(match_rating(career.rating_per(games)));
                                ui.end_row();
                            }
                        });
                    ui.add_space(12.0);
                });
        });
    }

    /// A club's finished seasons.
    pub(crate) fn club_history_card(&self, ui: &mut Ui, bd: &Backdrop, t: TeamId) {
        let w = &self.world;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Past seasons");
            ui.add_space(6.0);
            if w.history.is_empty() {
                ui.label(dim("This is the first season on record."));
                return;
            }
            egui::ScrollArea::horizontal()
                .id_salt("club_history")
                .show(ui, |ui| {
                    egui::Grid::new("club_history_grid")
                        .striped(true)
                        .spacing(vec2(16.0, 7.0))
                        .show(ui, |ui| {
                            header_row(
                                ui,
                                &[
                                    "Season",
                                    "Finish",
                                    "W",
                                    "L",
                                    "Pts",
                                    "Opp",
                                    "FG%",
                                    "3P%",
                                    "Reb",
                                    "Ast",
                                    "TO",
                                    "Top scorer",
                                    "",
                                ],
                            );
                            ui.end_row();
                            for r in w.history.iter().rev() {
                                let place = r.place(t);
                                let (_, wins, losses) = r.table[place.max(1) - 1];
                                let s = &r.stats[t];
                                let g = s.games.max(1) as f32;
                                ui.label(
                                    theme::mono(format!("Season {}", r.season + 1))
                                        .color(theme::TEXT_DIM),
                                );
                                let won = r.champion == t;
                                let finish = theme::mono(if won {
                                    format!("{place}, champions")
                                } else if r.runner_up == Some(t) {
                                    format!("{place}, lost the final")
                                } else {
                                    place.to_string()
                                });
                                ui.label(match place {
                                    _ if won => {
                                        finish.family(theme::mono_semibold()).color(theme::POS)
                                    }
                                    2..=3 => finish.color(theme::TEXT),
                                    _ => finish.color(theme::TEXT_MUTE),
                                });
                                ui.label(theme::mono(wins.to_string()));
                                ui.label(theme::mono(losses.to_string()));
                                // Graded against the rest of the league
                                // that year.
                                let figures: [TeamFigure; 7] = [
                                    (|s, g| s.own.pts as f32 / g, false),
                                    (|s, g| s.opp.pts as f32 / g, true),
                                    (|s, _| pct(s.own.fgm, s.own.fga), false),
                                    (|s, _| pct(s.own.tpm, s.own.tpa), false),
                                    (|s, g| s.own.reb() as f32 / g, false),
                                    (|s, g| s.own.ast as f32 / g, false),
                                    (|s, g| s.own.tov as f32 / g, true),
                                ];
                                for (f, low) in figures {
                                    let column: Vec<f32> = r
                                        .stats
                                        .iter()
                                        .map(|x| f(x, x.games.max(1) as f32))
                                        .collect();
                                    let v = f(s, g);
                                    ui.label(graded(format!("{v:.1}"), v, &column, low));
                                }
                                match r.scorers.get(t).copied().flatten() {
                                    Some((p, per)) => {
                                        link(
                                            ui,
                                            Link::Player(p),
                                            name(&w.players[p as usize].name),
                                        );
                                        ui.label(theme::mono(format!("{per:.1}")));
                                    }
                                    None => {
                                        ui.label(dim("-"));
                                        ui.label("");
                                    }
                                }
                                ui.end_row();
                            }
                        });
                    ui.add_space(12.0);
                });
        });
    }

    /// The playoff bracket as it stands.
    pub(crate) fn playoffs_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Playoffs / best of three");
            ui.add_space(6.0);
            egui::Grid::new("playoffs_grid")
                .spacing(vec2(16.0, 7.0))
                .show(ui, |ui| {
                    for s in &w.playoffs {
                        ui.label(theme::eyebrow_text(if s.is_final {
                            "Final"
                        } else {
                            "Semifinal"
                        }));
                        self.club_link(ui, s.high);
                        ui.label(
                            theme::mono(format!("{} - {}", s.wins[0], s.wins[1]))
                                .family(theme::mono_semibold()),
                        );
                        self.club_link(ui, s.low);
                        match s.winner() {
                            Some(t) if s.is_final => ui.label(
                                RichText::new(format!("{} are champions", w.teams[t].name))
                                    .color(theme::POS),
                            ),
                            Some(t) => ui.label(dim(format!("{} go through", w.teams[t].name))),
                            None => ui.label(dim("in progress")),
                        };
                        ui.end_row();
                    }
                });
        });
    }

    /// Who has the best case for most valuable player right now.
    pub(crate) fn mvp_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        let race = w.mvp_race();
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "MVP race");
            ui.label(dim(
                "Plus/minus leads the case, the box score backs it, and it has to come with \
                 winning: only clubs in the top half count, and a playoff run adds to it.",
            ));
            ui.add_space(6.0);
            if race.is_empty() {
                ui.label(dim("No games played yet."));
                return;
            }
            let top = &race[..race.len().min(10)];
            let col = |f: &dyn Fn(&basketman_core::season::MvpCase) -> f32| {
                race.iter().map(f).collect::<Vec<f32>>()
            };
            let (pm, rt, pts, win) = (
                col(&|c| c.plus_minus),
                col(&|c| c.rating),
                col(&|c| c.points),
                col(&|c| c.winning),
            );
            egui::Grid::new("mvp_grid")
                .striped(true)
                .spacing(vec2(16.0, 7.0))
                .show(ui, |ui| {
                    header_row(
                        ui,
                        &["#", "Player", "Club", "G", "+/-", "Rtg", "Pts", "Team wins"],
                    );
                    ui.end_row();
                    for (n, c) in top.iter().enumerate() {
                        ui.label(theme::mono((n + 1).to_string()).color(theme::TEXT_MUTE));
                        let text = name(&w.players[c.player as usize].name);
                        link(
                            ui,
                            Link::Player(c.player),
                            if self.is_mine(c.player) {
                                text.color(theme::ACCENT)
                            } else {
                                text
                            },
                        );
                        self.club_link(ui, c.team);
                        ui.label(theme::mono(c.games.to_string()));
                        let plus = format!("{:+.1}", c.plus_minus);
                        ui.label(graded(plus, c.plus_minus, &pm, false));
                        ui.label(graded(format!("{:.1}", c.rating), c.rating, &rt, false));
                        ui.label(graded(format!("{:.1}", c.points), c.points, &pts, false));
                        let wins = format!("{:.0}%", c.winning * 100.0);
                        ui.label(graded(wins, c.winning, &win, false));
                        ui.end_row();
                    }
                });
        });
    }

    /// The roll of champions, with where the user's club came each year.
    pub(crate) fn seasons_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        if w.history.is_empty() {
            return;
        }
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Past seasons");
            ui.add_space(6.0);
            egui::Grid::new("seasons_grid")
                .striped(true)
                .spacing(vec2(16.0, 7.0))
                .show(ui, |ui| {
                    header_row(
                        ui,
                        &[
                            "Season",
                            "Champions",
                            "Runners-up",
                            "Best record",
                            "MVP",
                            "We finished",
                        ],
                    );
                    ui.end_row();
                    for r in w.history.iter().rev() {
                        ui.label(
                            theme::mono(format!("Season {}", r.season + 1)).color(theme::TEXT_DIM),
                        );
                        self.club_link(ui, r.champion);
                        match r.runner_up {
                            Some(t) => {
                                self.club_link(ui, t);
                            }
                            None => {
                                ui.label(dim("-"));
                            }
                        }
                        let (first, wins, losses) = r.table[0];
                        ui.horizontal(|ui| {
                            self.club_link(ui, first);
                            ui.label(
                                theme::mono(format!("{wins}-{losses}")).color(theme::TEXT_DIM),
                            );
                        });
                        match r.mvp {
                            Some(p) => {
                                link(ui, Link::Player(p), name(&w.players[p as usize].name));
                            }
                            None => {
                                ui.label(dim("-"));
                            }
                        }
                        ui.label(theme::mono(r.place(self.team).to_string()));
                        ui.end_row();
                    }
                });
        });
    }
}
