//! Getting around: every player, club and game named on screen is a link,
//! and Back retraces the steps. Also the two pages only links lead to: a
//! club's page and a played game's page.

use basketman_core::inbox::{MATCH_DAY, date};
use basketman_core::league::TeamId;
use basketman_core::player::PlayerId;
use eframe::egui::{self, FontId, RichText, Sense, Stroke, Ui, vec2};

use crate::app::{App, GAP, Tab, header_row, name, rating};
use crate::pages::{dim, estimate, graded, match_rating};
use crate::theme::{self, Backdrop};
use basketman_core::engine::BoxLine;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Link {
    Player(PlayerId),
    Club(TeamId),
    /// Index into `World::fixtures`.
    Game(usize),
}

/// Where the user was: the screen and what was open on it.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Place {
    tab: Tab,
    profile: Option<PlayerId>,
    club: Option<TeamId>,
    game: Option<usize>,
}

fn link_id() -> egui::Id {
    egui::Id::unique("follow_link")
}

/// Send the app to `to` at the end of this frame. Works while the world is
/// borrowed, which is when most links are drawn.
pub(crate) fn follow(ui: &Ui, to: Link) {
    ui.ctx().data_mut(|d| d.insert_temp(link_id(), Some(to)));
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect::<String>() + ".",
        None => String::new(),
    }
}

/// Text that goes somewhere when clicked: underlined on hover.
pub(crate) fn link(ui: &mut Ui, to: Link, text: RichText) -> egui::Response {
    let r = ui
        .add(
            egui::Label::new(text)
                .selectable(false)
                .sense(Sense::click()),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if r.hovered() {
        ui.painter().hline(
            r.rect.x_range(),
            r.rect.bottom(),
            Stroke::new(1.0, theme::ACCENT),
        );
    }
    if r.clicked() {
        follow(ui, to);
    }
    r
}

impl App {
    fn place(&self) -> Place {
        Place {
            tab: self.tab,
            profile: self.profile,
            club: self.club,
            game: self.game,
        }
    }

    /// Remember where we are before going somewhere else.
    pub(crate) fn leave(&mut self) {
        let here = self.place();
        if self.history.last() != Some(&here) {
            self.history.push(here);
            if self.history.len() > 60 {
                self.history.remove(0);
            }
        }
    }

    pub(crate) fn can_go_back(&self) -> bool {
        !self.history.is_empty()
    }

    pub(crate) fn go_back(&mut self) {
        if let Some(p) = self.history.pop() {
            self.tab = p.tab;
            self.profile = p.profile.or(self.profile);
            self.club = p.club.or(self.club);
            self.game = p.game.or(self.game);
        }
    }

    /// Name of the screen Back leads to.
    pub(crate) fn back_label(&self) -> String {
        let Some(p) = self.history.last() else {
            return "squad".to_string();
        };
        match p.tab {
            Tab::Player => p
                .profile
                .map(|id| self.world.players[id as usize].name.clone()),
            Tab::Club => p.club.map(|t| self.world.teams[t].name.clone()),
            _ => None,
        }
        .unwrap_or_else(|| p.tab.label().to_lowercase())
    }

    pub(crate) fn go(&mut self, tab: Tab) {
        if self.tab != tab {
            self.leave();
            self.tab = tab;
        }
    }

    /// Act on a link clicked this frame.
    pub(crate) fn take_link(&mut self, ctx: &egui::Context) {
        let Some(to) = ctx
            .data_mut(|d| d.remove_temp::<Option<Link>>(link_id()))
            .flatten()
        else {
            return;
        };
        self.leave();
        match to {
            Link::Player(p) => {
                self.profile = Some(p);
                self.tab = Tab::Player;
            }
            Link::Club(t) => {
                self.club = Some(t);
                self.tab = Tab::Club;
            }
            Link::Game(i) => {
                self.game = Some(i);
                self.tab = Tab::Game;
            }
        }
        // A new page starts at its top.
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique("page_top"), true));
    }

    pub(crate) fn club_link(&self, ui: &mut Ui, team: TeamId) -> egui::Response {
        let text = &self.world.teams[team].name;
        let text = if team == self.team {
            name(text).color(theme::ACCENT)
        } else {
            RichText::new(text)
        };
        link(ui, Link::Club(team), text)
    }

    fn back_row(&mut self, ui: &mut Ui) {
        if ui.link(format!("Back to {}", self.back_label())).clicked() {
            self.go_back();
        }
        ui.add_space(8.0);
    }

    /// Every fixture of the season, or of one club, round by round. Played
    /// games link to their box score.
    pub(crate) fn fixture_list(&self, ui: &mut Ui, only: Option<TeamId>, height: f32) {
        let w = &self.world;
        egui::ScrollArea::vertical()
            .id_salt(("fixtures", only))
            .max_height(height)
            .min_scrolled_height(height)
            .show(ui, |ui| {
                egui::Grid::new(("fixture_grid", only))
                    .striped(true)
                    .spacing(vec2(14.0, 7.0))
                    .show(ui, |ui| {
                        let mut round = usize::MAX;
                        for (i, f) in w.fixtures.iter().enumerate() {
                            if only.is_some_and(|t| f.home != t && f.away != t) {
                                continue;
                            }
                            let day = f.round as u32 * 7 + MATCH_DAY;
                            if f.round != round && only.is_none() {
                                round = f.round;
                                let label = if f.playoff {
                                    format!("Playoffs / {}", date(day))
                                } else {
                                    format!("Round {} / {}", f.round + 1, date(day))
                                };
                                ui.label(theme::eyebrow_text(&label));
                                ui.end_row();
                            }
                            if only.is_some() {
                                ui.label(theme::mono(date(day)).color(theme::TEXT_MUTE));
                            }
                            self.club_link(ui, f.home);
                            match &f.result {
                                Some(r) => {
                                    let text = format!("{} - {}", r.score[0], r.score[1]);
                                    let won = only.map(|t| {
                                        let side = if f.home == t { 0 } else { 1 };
                                        r.score[side] > r.score[1 - side]
                                    });
                                    let text = theme::mono(text).color(match won {
                                        Some(true) => theme::POS,
                                        Some(false) => theme::WARN,
                                        None => theme::ACCENT,
                                    });
                                    link(ui, Link::Game(i), text).on_hover_text("Box score");
                                }
                                None if f.round == w.round => {
                                    ui.label(theme::mono("next").color(theme::TEXT));
                                }
                                None => {
                                    ui.label(theme::mono("-").color(theme::TEXT_MUTE));
                                }
                            }
                            self.club_link(ui, f.away);
                            ui.end_row();
                        }
                    });
            });
    }

    // ---- a club's page ---------------------------------------------------

    pub(crate) fn club_page(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let Some(t) = self.club else {
            self.go_back();
            return;
        };
        self.back_row(ui);
        let w = &self.world;
        let team = &w.teams[t];
        let s = &w.standings[t];
        let place = w.table().iter().position(|&x| x == t).map_or(0, |p| p + 1);
        let tactics = team.tactics();
        let games = (s.wins + s.losses).max(1) as f32;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, if t == self.team { "Your club" } else { "Club" });
            ui.label(RichText::new(&team.name).font(FontId::new(30.0, theme::semibold())));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 34.0;
                for (label, value) in [
                    ("Record", format!("{}-{}", s.wins, s.losses)),
                    ("League position", place.to_string()),
                    ("Scores", format!("{:.1}", s.points_for as f32 / games)),
                    ("Allows", format!("{:.1}", s.points_against as f32 / games)),
                    ("Offense", tactics.offense.label().to_string()),
                    ("Defense", tactics.defense.label().to_string()),
                    ("Press", tactics.press.label().to_string()),
                ] {
                    ui.vertical(|ui| {
                        ui.label(theme::eyebrow_text(label));
                        ui.label(RichText::new(value).font(FontId::new(18.0, theme::semibold())));
                    });
                }
            });
        });
        ui.add_space(GAP);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            let office = &team.office;
            let gm = &office.gm;
            theme::eyebrow(ui, "Front office");
            ui.horizontal_wrapped(|ui| {
                let role = if t == self.team {
                    "Assistant general manager"
                } else {
                    "General manager"
                };
                ui.label(name(&gm.name));
                ui.label(dim(format!("{role}, {}", gm.age)));
            });
            ui.label(RichText::new(capitalise(&gm.traits().join("; "))).color(theme::TEXT));
            if t == self.team {
                ui.label(dim(
                    "You make the deals. He runs the draft for you if you let him, and \
                     his department decides how fast the scouts learn a player.",
                ));
            }
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 30.0;
                for (label, value, hint) in [
                    (
                        "Judgement",
                        gm.judgement,
                        "How near his valuation of a player is to the market's. Low: some of \
                         his players can be had cheaply, others he will overpay for.",
                    ),
                    (
                        "Negotiation",
                        gm.negotiation,
                        "How far a deal has to favour him before he agrees.",
                    ),
                    (
                        "Patience",
                        gm.patience,
                        "High: values young players and draft picks. Low: values veterans \
                         who help now.",
                    ),
                    (
                        "Activity",
                        gm.activity,
                        "How often he makes offers for other clubs' players.",
                    ),
                    (
                        "Scouting dept",
                        office.scouting(),
                        "How well the club reads a draft class.",
                    ),
                ] {
                    ui.vertical(|ui| {
                        ui.label(theme::eyebrow_text(label)).on_hover_text(hint);
                        ui.label(rating(value as f32, false).size(18.0))
                            .on_hover_text(hint);
                    });
                }
            });
            self.staff_lines(ui, t);
        });
        ui.add_space(GAP);
        let arena_command = self.arena_card(ui, bd, t);
        let go_finances = crate::finances::take_open_request(ui.ctx());
        let mut trade = false;
        if t != self.team {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                trade = ui.button("Propose a trade").clicked();
                let picks: Vec<String> = (0..w.picks.len())
                    .filter(|&i| w.picks[i].owner == t)
                    .map(|i| w.pick_label(i))
                    .collect();
                ui.label(dim(format!("{} draft picks held", picks.len())))
                    .on_hover_text(picks.join("\n"));
            });
        }
        ui.add_space(GAP);

        let mut shortlist = None;
        let mut squad = |ui: &mut Ui| {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, &format!("Squad / {} players", team.roster.len()));
                ui.add_space(6.0);
                egui::ScrollArea::horizontal()
                    .id_salt("club_squad")
                    .show(ui, |ui| {
                        egui::Grid::new("club_squad_grid")
                            .striped(true)
                            .spacing(vec2(14.0, 8.0))
                            .show(ui, |ui| {
                                header_row(
                                    ui,
                                    &[
                                        "Player", "Pos", "Age", "Ht", "Ovr", "G", "Min", "Pts",
                                        "Reb", "Ast", "+/-", "Rtg", "",
                                    ],
                                );
                                ui.end_row();
                                let mut roster = team.roster.clone();
                                roster.sort_by(|&a, &b| {
                                    w.players[b as usize]
                                        .overall()
                                        .total_cmp(&w.players[a as usize].overall())
                                });
                                let rate = |pid: PlayerId, f: &dyn Fn(&BoxLine) -> f32| {
                                    f(&w.season_stats[pid as usize])
                                        / w.games_played[pid as usize].max(1) as f32
                                };
                                type Pick = fn(&BoxLine) -> f32;
                                let picks: [Pick; 5] = [
                                    |l| l.secs as f32 / 60.0,
                                    |l| l.pts as f32,
                                    |l| l.reb() as f32,
                                    |l| l.ast as f32,
                                    |l| l.pm as f32,
                                ];
                                let columns: Vec<Vec<f32>> = picks
                                    .iter()
                                    .map(|f| {
                                        roster
                                            .iter()
                                            .filter(|&&q| w.games_played[q as usize] > 0)
                                            .map(|&q| rate(q, f))
                                            .collect()
                                    })
                                    .collect();
                                for pid in roster.clone() {
                                    let p = &w.players[pid as usize];
                                    let g = w.games_played[pid as usize];
                                    let l = &w.season_stats[pid as usize];
                                    link(ui, Link::Player(pid), name(&p.name));
                                    ui.label(theme::mono(p.position.label()).color(theme::ACCENT));
                                    ui.label(theme::mono(p.age.to_string()));
                                    ui.label(theme::mono(format!("{:.0}", p.body.height_cm)));
                                    if t == self.team {
                                        ui.label(rating(p.overall(), false));
                                    } else {
                                        let (lo, hi) = self.known_overall(p);
                                        ui.label(estimate(lo, hi));
                                    }
                                    ui.label(theme::mono(g.to_string()));
                                    for (f, column) in picks.iter().zip(&columns) {
                                        ui.label(if g == 0 {
                                            crate::pages::faded("-")
                                        } else {
                                            let v = rate(pid, f);
                                            graded(format!("{v:.1}"), v, column, false)
                                        });
                                    }
                                    if g > 0 {
                                        ui.label(match_rating(l.rating_per(g)));
                                    } else {
                                        ui.label(dim("-"));
                                    }
                                    if t != self.team {
                                        let on = w.scouting.is_shortlisted(pid);
                                        let label = if on { "Shortlisted" } else { "Shortlist" };
                                        if ui.selectable_label(on, label).clicked() {
                                            shortlist = Some(pid);
                                        }
                                    } else {
                                        ui.label("");
                                    }
                                    ui.end_row();
                                }
                            });
                        ui.add_space(14.0);
                    });
            });
        };
        let schedule = |ui: &mut Ui| {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Schedule");
                ui.add_space(6.0);
                self.fixture_list(ui, Some(t), 520.0);
            });
        };
        if ui.available_width() > 1150.0 {
            let side = 470.0;
            let main = ui.available_width() - side - GAP;
            let down = egui::Layout::top_down(egui::Align::Min);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(main, 0.0), down, squad);
                ui.add_space(GAP - ui.spacing().item_spacing.x);
                ui.allocate_ui_with_layout(vec2(side, 0.0), down, schedule);
            });
        } else {
            squad(ui);
            ui.add_space(GAP);
            schedule(ui);
        }
        ui.add_space(GAP);
        self.club_history_card(ui, bd, t);
        if let Some(pid) = shortlist {
            self.toggle_shortlist(pid);
        }
        if let Some(command) = arena_command {
            let _ = self.world.apply(command);
        }
        if trade {
            self.open_trade(t, None);
        }
        if go_finances {
            self.go(Tab::Finances);
        }
    }

    // ---- a played game's page --------------------------------------------

    pub(crate) fn game_page(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let Some(i) = self.game.filter(|&i| i < self.world.fixtures.len()) else {
            self.go_back();
            return;
        };
        self.back_row(ui);
        let f = self.world.fixtures[i].clone();
        let day = f.round as u32 * 7 + MATCH_DAY;
        let taped = self.world.has_tape(i);
        let mut watch = false;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, &format!("Round {} / {}", f.round + 1, date(day)));
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                let big =
                    |text: &str| RichText::new(text).font(FontId::new(28.0, theme::semibold()));
                let home = big(&self.world.teams[f.home].name);
                let away = big(&self.world.teams[f.away].name);
                link(ui, Link::Club(f.home), home.color(theme::ACCENT));
                match &f.result {
                    Some(r) => ui.label(big(&format!("{} - {}", r.score[0], r.score[1]))),
                    None => ui.label(big("v")),
                };
                link(ui, Link::Club(f.away), away);
            });
            match &f.result {
                Some(r) => {
                    let periods: Vec<String> = r
                        .periods
                        .iter()
                        .map(|p| format!("{}-{}", p[0], p[1]))
                        .collect();
                    ui.label(theme::mono(periods.join("   ")).color(theme::TEXT_DIM));
                    if let Some(&(att, gate)) = self.world.econ.crowds.get(i).filter(|c| c.0 > 0) {
                        let hall = self.world.arena(f.home);
                        let gate = if f.home == self.team {
                            format!(", gate {}", crate::app::money(gate))
                        } else {
                            String::new()
                        };
                        ui.label(dim(format!(
                            "{} at {}, {}% full{gate}.",
                            crate::app::money(att as i64),
                            hall.name,
                            att as u64 * 100 / hall.capacity.max(1) as u64
                        )));
                    }
                }
                None => {
                    ui.label(dim("Not played yet."));
                }
            }
            if taped {
                ui.add_space(6.0);
                watch = ui.button("Watch the replay").clicked();
            } else if f.result.is_some() {
                ui.add_space(6.0);
                ui.label(dim(
                    "Only your own games are filmed. This one has its box score.",
                ));
            }
        });
        if watch {
            self.world.load_tape(i);
            self.last_game = Some(i);
            self.playback = crate::matchview::Playback {
                time: 0.0,
                playing: true,
                ..crate::matchview::Playback::start()
            };
            self.go(Tab::Match);
        }
        let Some(r) = &f.result else { return };
        ui.add_space(GAP);
        let team_box = |ui: &mut Ui, side: usize| {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, &self.world.teams[[f.home, f.away][side]].name);
                ui.add_space(6.0);
                egui::ScrollArea::horizontal()
                    .id_salt(("game_box", side))
                    .show(ui, |ui| {
                        self.box_score(ui, r, side);
                        ui.add_space(14.0);
                    });
            });
        };
        if ui.available_width() > 1300.0 {
            ui.columns(2, |cols| {
                team_box(&mut cols[0], 0);
                team_box(&mut cols[1], 1);
            });
        } else {
            team_box(ui, 0);
            ui.add_space(GAP);
            team_box(ui, 1);
        }
    }
}
