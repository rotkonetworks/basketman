//! What goes on each screen: which place is behind it and how the panels
//! are laid over it, for the three sizes. The pages themselves (the cards
//! and tables) are the ones in the other files; this only decides where they
//! are put and which of them share a panel.

use basketman_core::command::Command;
use eframe::egui::{self, Align, Layout, Rect, Ui, pos2, vec2};

use crate::app::{App, Tab};
use crate::scenes::Kind;
use crate::shell::{Tier, panel, panel_wide, panel_with, tab_row};
use crate::theme::{self, Backdrop};

/// Split `r` into columns left to right. A positive width is fixed, a
/// negative one is a share of what is left.
pub(crate) fn cols(r: Rect, spec: &[f32], gap: f32) -> Vec<Rect> {
    let fixed: f32 = spec.iter().filter(|w| **w > 0.0).sum();
    let share: f32 = spec.iter().filter(|w| **w < 0.0).map(|w| -w).sum();
    let room = (r.width() - fixed - gap * (spec.len() as f32 - 1.0)).max(0.0);
    let mut x = r.left();
    spec.iter()
        .map(|&w| {
            let width = if w > 0.0 {
                w
            } else {
                room * -w / share.max(1e-3)
            };
            let out = Rect::from_min_size(pos2(x, r.top()), vec2(width, r.height()));
            x += width + gap;
            out
        })
        .collect()
}

/// A phone's scene and panel: the scene on top, or beside the panel when
/// the phone is on its side.
pub(crate) fn phone_split(r: Rect, top_h: f32, gap: f32) -> (Rect, Rect) {
    if r.width() > r.height() * 1.3 {
        let c = cols(r, &[-0.9, -1.1], gap);
        (c[0], c[1])
    } else {
        split_top(r, top_h, gap)
    }
}

/// The top `h` of a rect, and the rest.
pub(crate) fn split_top(r: Rect, h: f32, gap: f32) -> (Rect, Rect) {
    let h = h.clamp(0.0, r.height());
    (
        Rect::from_min_size(r.min, vec2(r.width(), h)),
        Rect::from_min_max(pos2(r.left(), r.top() + h + gap), r.max),
    )
}

impl App {
    fn inset_backdrop(&self) -> Backdrop {
        let mut bd = self.backdrop.clone();
        bd.inset = true;
        bd
    }

    /// One screen: its place, then its panels.
    pub(crate) fn screen(&mut self, ui: &mut Ui, stage: Rect, tier: Tier) {
        let full = ui.ctx().content_rect();
        let bd = self.inset_backdrop();
        let narrow = tier == Tier::Narrow;
        let g = if narrow { 8.0 } else { 12.0 };
        let tab = self.tab;
        // The match paints its own place: the court.
        if tab == Tab::Match {
            self.match_screen_shell(ui, &bd, stage, tier);
            return;
        }
        match tab {
            Tab::Inbox => {
                self.sh
                    .stage
                    .hall(ui, Kind::Home, &self.world, self.team, full, full);
                self.screen_inbox(ui, &bd, stage, tier, g);
            }
            Tab::Squad => self.screen_squad(ui, &bd, stage, tier, g, full),
            Tab::Player => self.screen_player(ui, &bd, stage, tier, g, full),
            Tab::Tactics => self.screen_tactics(ui, &bd, stage, tier, g, full),
            Tab::Training | Tab::Reserves => {
                // A panel on the left and the practice floor beside it.
                let (scene, rest) = match tier {
                    Tier::Wide => {
                        let c = cols(stage, &[-1.0, 700.0_f32.min(stage.width() * 0.6)], g);
                        (c[0], c[1])
                    }
                    _ => {
                        let h = if narrow {
                            (stage.height() * 0.28).max(140.0)
                        } else {
                            stage.height() * 0.36
                        };
                        if narrow {
                            phone_split(stage, h, g)
                        } else {
                            split_top(stage, h, g)
                        }
                    }
                };
                let order =
                    crate::lineup::order(&self.world.players, &self.world.teams[self.team].roster);
                self.sh.stage.squad(
                    ui,
                    Kind::Practice,
                    &self.world,
                    self.team,
                    &order,
                    None,
                    full,
                    scene.intersect(full),
                    scene,
                );
                let salt = if tab == Tab::Training {
                    "training"
                } else {
                    "reserves"
                };
                let min_w = if narrow { Self::narrow_min(tab) } else { 0.0 };
                panel_wide(ui, rest, salt, min_w, |ui| self.page_body(ui, &bd, tab));
            }
            Tab::Film => {
                self.sh
                    .stage
                    .hall(ui, Kind::Data, &self.world, self.team, full, full);
                let min_w = if narrow {
                    Self::narrow_min(Tab::Film)
                } else {
                    0.0
                };
                panel_wide(ui, stage, "film", min_w, |ui| self.film_room(ui, &bd));
            }
            Tab::League => self.screen_league(ui, &bd, stage, tier, g, full),
            Tab::Finances => self.screen_money(ui, &bd, stage, tier, g, full),
            _ => {
                // Everything else is data: the hall from the concourse,
                // dimmed, and one panel of the page.
                self.sh
                    .stage
                    .hall(ui, Kind::Data, &self.world, self.team, full, full);
                let detail = matches!(tab, Tab::Club | Tab::Game | Tab::Compare);
                let rect = stage;
                let salt = tab.label();
                let min_w = if narrow { Self::narrow_min(tab) } else { 0.0 };
                panel_wide(ui, rect, salt, min_w, |ui| {
                    if narrow && detail {
                        self.back_button(ui);
                    }
                    self.page_body(ui, &bd, tab)
                });
            }
        }
    }

    /// How wide a page wants to be on a phone before it scrolls sideways.
    pub(crate) fn narrow_min(tab: Tab) -> f32 {
        match tab {
            Tab::Staff
            | Tab::Scouting
            | Tab::Trade
            | Tab::Draft
            | Tab::Stats
            | Tab::Finances
            | Tab::Compare
            | Tab::Game
            | Tab::Film => 640.0,
            Tab::Reserves => 600.0,
            _ => 420.0,
        }
    }

    /// A "Back" button for a detail page on a phone.
    pub(crate) fn back_button(&mut self, ui: &mut Ui) {
        if self.can_go_back()
            && ui
                .add(egui::Button::new(format!(
                    "< Back to {}",
                    self.back_label()
                )))
                .clicked()
        {
            self.go_back();
        }
        ui.add_space(4.0);
    }

    /// The body of a page, where one page is one panel.
    pub(crate) fn page_body(&mut self, ui: &mut Ui, bd: &Backdrop, tab: Tab) {
        match tab {
            Tab::Inbox => {}
            Tab::Squad => self.squad(ui, bd),
            Tab::Tactics => {}
            Tab::Training => self.training(ui, bd),
            Tab::Staff => self.staff_page(ui, bd),
            Tab::Reserves => self.reserves_page(ui, bd),
            Tab::Scouting => self.scouting(ui, bd),
            Tab::Compare => self.compare(ui, bd),
            Tab::Finances => self.finances(ui, bd),
            Tab::Stats => self.stats(ui, bd),
            Tab::League => {}
            Tab::Match => {}
            Tab::Player => self.profile(ui, bd),
            Tab::Club => self.club_page(ui, bd),
            Tab::Arena => {
                self.club = Some(self.team);
                self.club_page(ui, bd)
            }
            Tab::Trade => self.trade_page(ui, bd),
            Tab::Draft => self.draft_page(ui, bd),
            Tab::Game => self.game_page(ui, bd),
            Tab::Film => self.film_room(ui, bd),
        }
    }

    // ---- inbox -----------------------------------------------------------

    fn screen_inbox(&mut self, ui: &mut Ui, bd: &Backdrop, stage: Rect, tier: Tier, g: f32) {
        let shown = self.inbox_shown();
        match tier {
            Tier::Narrow => {
                if self.sh.mail_open {
                    panel(ui, stage, "mail_read", |ui| {
                        if ui.add(egui::Button::new("< Inbox")).clicked() {
                            self.sh.mail_open = false;
                        }
                        ui.add_space(4.0);
                        self.mail_pane(ui, bd, &shown);
                    });
                } else {
                    panel(ui, stage, "mail_list", |ui| self.mail_list(ui, bd, &shown));
                }
            }
            Tier::Medium => {
                let c = cols(stage, &[330.0, -1.0], g);
                panel(ui, c[0], "mail_list", |ui| self.mail_list(ui, bd, &shown));
                panel(ui, c[1], "mail_read", |ui| self.mail_pane(ui, bd, &shown));
            }
            Tier::Wide => {
                let c = cols(stage, &[410.0, 640.0_f32.min(stage.width() - 410.0 - g)], g);
                panel(ui, c[0], "mail_list", |ui| self.mail_list(ui, bd, &shown));
                panel(ui, c[1], "mail_read", |ui| self.mail_pane(ui, bd, &shown));
            }
        }
    }

    // ---- league ----------------------------------------------------------

    fn screen_league(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        stage: Rect,
        tier: Tier,
        g: f32,
        full: Rect,
    ) {
        self.sh
            .stage
            .hall(ui, Kind::Data, &self.world, self.team, full, full);
        let playoffs = !self.world.playoffs.is_empty();
        let mut more: Vec<&str> = Vec::new();
        if playoffs {
            more.push("Playoffs");
        }
        more.extend(["Names", "Hall of fame", "Seasons"]);
        let narrow = tier == Tier::Narrow;
        match tier {
            Tier::Wide => {
                let c = cols(stage, &[-1.0, -1.1, -1.0], g);
                panel(ui, c[0], "standings", |ui| self.standings(ui, bd));
                panel(ui, c[1], "results", |ui| self.results(ui, bd));
                self.league_more(ui, bd, c[2], &more, false);
            }
            Tier::Medium => {
                let c = cols(stage, &[-1.0, -1.0], g);
                panel(ui, c[0], "standings", |ui| self.standings(ui, bd));
                let mut tabs = vec!["Fixtures"];
                tabs.extend(more.iter().copied());
                self.league_tabs(ui, bd, c[1], &tabs, narrow, true);
            }
            Tier::Narrow => {
                let mut tabs = vec!["Table", "Fixtures"];
                tabs.extend(more.iter().copied());
                self.league_tabs(ui, bd, stage, &tabs, narrow, false);
            }
        }
    }

    fn league_more(&mut self, ui: &mut Ui, bd: &Backdrop, rect: Rect, more: &[&str], narrow: bool) {
        self.league_tabs(ui, bd, rect, more, narrow, false);
    }

    /// One panel whose tab row picks the card. A tab named "Table" is the
    /// standings and "Fixtures" the results; the others are the cards of the
    /// league page.
    fn league_tabs(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        rect: Rect,
        tabs: &[&str],
        narrow: bool,
        _fixtures_first: bool,
    ) {
        let cur = self.sh.league_tab.min(tabs.len() - 1);
        let tabs_owned: Vec<String> = tabs.iter().map(|s| s.to_string()).collect();
        let mut pick = None;
        panel_with(ui, rect, "league_tabs_outer", false, |ui| {
            // Tab row fixed on top; the card scrolls under it.
            let labels: Vec<&str> = tabs_owned.iter().map(String::as_str).collect();
            // Many tabs: smaller labels on a phone.
            pick = tab_row(ui, &labels, cur, narrow);
            let name = labels[cur];
            let compact_table = name == "Table" && narrow;
            if compact_table {
                self.standings_header(ui);
            }
            egui::ScrollArea::vertical()
                .id_salt(("league_card", name))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    match name {
                        "Table" => self.standings_rows(ui),
                        "Fixtures" => self.results(ui, bd),
                        "Playoffs" => self.playoffs_card(ui, bd),
                        "Names" => self.names_card(ui, bd),
                        "Hall of fame" => self.hall_card(ui, bd),
                        _ => self.seasons_card(ui, bd),
                    }
                });
        });
        if let Some(p) = pick {
            self.sh.league_tab = p;
        }
    }

    /// The header of the phone's table, which stays while the rows scroll.
    fn standings_header(&self, ui: &mut Ui) {
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), egui::Sense::hover());
        let p = ui.painter();
        let f = egui::FontId::new(10.0, egui::FontFamily::Monospace);
        for (x, a, t) in [
            (r.left() + 8.0, egui::Align2::LEFT_CENTER, "#"),
            (r.left() + 40.0, egui::Align2::LEFT_CENTER, "TEAM"),
            (r.right() - 78.0, egui::Align2::RIGHT_CENTER, "W-L"),
            (r.right() - 8.0, egui::Align2::RIGHT_CENTER, "DIFF"),
        ] {
            p.text(pos2(x, r.center().y), a, t, f.clone(), theme::TEXT_MUTE);
        }
        p.hline(
            r.x_range(),
            r.bottom(),
            egui::Stroke::new(1.0, theme::LINE_2),
        );
    }

    /// The league table as rows of four figures that fit a phone; the rest
    /// is on the club's page, a tap away.
    fn standings_rows(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (rank, t) in self.world.table().into_iter().enumerate() {
            let s = &self.world.standings[t];
            let (r, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), 46.0), egui::Sense::click());
            let p = ui.painter().with_clip_rect(r);
            if t == self.team {
                p.rect_filled(
                    r.shrink2(vec2(1.0, 2.0)),
                    8.0,
                    theme::ACCENT.gamma_multiply(0.12),
                );
            }
            let cy = r.center().y;
            p.text(
                pos2(r.left() + 8.0, cy),
                egui::Align2::LEFT_CENTER,
                (rank + 1).to_string(),
                egui::FontId::new(13.0, egui::FontFamily::Monospace),
                theme::TEXT_MUTE,
            );
            p.text(
                pos2(r.left() + 40.0, cy),
                egui::Align2::LEFT_CENTER,
                &self.world.teams[t].name,
                egui::FontId::new(15.0, theme::semibold()),
                if t == self.team {
                    theme::ACCENT
                } else {
                    theme::TEXT
                },
            );
            p.text(
                pos2(r.right() - 78.0, cy),
                egui::Align2::RIGHT_CENTER,
                format!("{}-{}", s.wins, s.losses),
                egui::FontId::new(14.0, egui::FontFamily::Monospace),
                theme::TEXT,
            );
            let diff = s.points_for as i64 - s.points_against as i64;
            p.text(
                pos2(r.right() - 8.0, cy),
                egui::Align2::RIGHT_CENTER,
                format!("{diff:+}"),
                egui::FontId::new(14.0, egui::FontFamily::Monospace),
                if diff >= 0 { theme::POS } else { theme::WARN },
            );
            p.hline(r.x_range(), r.bottom(), egui::Stroke::new(1.0, theme::LINE));
            if resp.clicked() {
                crate::nav::follow(ui, crate::nav::Link::Club(t));
            }
        }
    }

    // ---- money -----------------------------------------------------------

    fn screen_money(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        stage: Rect,
        tier: Tier,
        g: f32,
        full: Rect,
    ) {
        self.sh
            .stage
            .hall(ui, Kind::Data, &self.world, self.team, full, full);
        let _ = (tier, g);
        let min_w = if tier == Tier::Narrow {
            Self::narrow_min(Tab::Finances)
        } else {
            0.0
        };
        panel_wide(ui, stage, "finances", min_w, |ui| self.finances(ui, bd));
    }

    // ---- player ----------------------------------------------------------

    fn screen_player(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        stage: Rect,
        tier: Tier,
        g: f32,
        full: Rect,
    ) {
        let mine = self
            .profile
            .is_some_and(|p| self.world.teams[self.team].roster.contains(&p));
        let narrow = tier == Tier::Narrow;
        if narrow || !mine {
            self.sh
                .stage
                .hall(ui, Kind::Data, &self.world, self.team, full, full);
            let min_w = if narrow {
                Self::narrow_min(Tab::Player)
            } else {
                0.0
            };
            panel_wide(ui, stage, "player", min_w, |ui| {
                if narrow {
                    self.back_button(ui);
                }
                self.profile(ui, bd)
            });
            return;
        }
        // One of ours: his profile over the squad scene he stands in.
        let width = (stage.width() * 0.62).clamp(420.0, 780.0);
        let c = cols(stage, &[-1.0, width], g);
        let order = crate::lineup::order(&self.world.players, &self.world.teams[self.team].roster);
        let touched = self.sh.stage.squad(
            ui,
            Kind::Squad,
            &self.world,
            self.team,
            &order,
            self.profile,
            full,
            c[0],
            c[0],
        );
        if let Some(p) = touched.clicked {
            self.profile = Some(p);
            self.sh.pick = Some(p);
        }
        panel(ui, c[1], "player", |ui| self.profile(ui, bd));
    }

    // ---- tactics ---------------------------------------------------------

    fn screen_tactics(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        stage: Rect,
        tier: Tier,
        g: f32,
        full: Rect,
    ) {
        let slot = self.tactics_prepare();
        let narrow = tier == Tier::Narrow;
        // Where the half court is seen, and where the panels go.
        let (scene, left, right) = match tier {
            Tier::Wide => {
                let c = cols(stage, &[320.0, -1.0, 450.0], g);
                (c[1], Some(c[0]), c[2])
            }
            Tier::Medium => {
                let (top, rest) = split_top(stage, stage.height() * 0.44, g);
                let c = cols(rest, &[300.0, -1.0], g);
                (top, Some(c[0]), c[1])
            }
            Tier::Narrow => {
                let (top, rest) = phone_split(stage, (stage.height() * 0.42).min(340.0), g);
                (top, None, rest)
            }
        };
        let tactics = self.world.teams[self.team].playbook[slot].tactics.clone();
        let cast = tactics.cast(&self.world.players);
        let board = tactics.layout();
        let selected = self.room.selected;
        let drawing = self.room.drawing;
        // The strip of controls over the picture.
        let strip_h = if narrow { 40.0 } else { 34.0 };
        let strip = Rect::from_min_size(
            pos2(scene.left() + 8.0, scene.top() + 6.0),
            vec2(
                (scene.width() - 16.0).min(if narrow { 360.0 } else { 520.0 }),
                strip_h,
            ),
        );
        let free = Rect::from_min_max(pos2(scene.left(), strip.bottom() + 4.0), scene.max);
        let out = self.sh.stage.tactics(
            ui,
            &self.world,
            self.team,
            &board,
            &cast,
            selected,
            drawing,
            full,
            scene,
            free,
        );
        if let Some(i) = out.picked
            && cast[i].is_some()
        {
            self.room.selected = cast[i];
        }
        if out.board != board {
            let mut t = tactics.clone();
            t.draw(out.board);
            let _ = self.world.apply(Command::SetTactics { slot, tactics: t });
        }
        // Controls.
        crate::shell::glass(ui, strip, 12, true);
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(strip.shrink2(vec2(8.0, 3.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        row.set_clip_rect(strip);
        let mut drawing_now = self.room.drawing;
        row.selectable_value(&mut drawing_now, false, "Move");
        row.selectable_value(&mut drawing_now, true, "Draw runs");
        let some_runs = board.runs.iter().any(Option::is_some);
        let mut clear = false;
        if row
            .add_enabled(some_runs, egui::Button::new("Clear"))
            .clicked()
        {
            clear = true;
        }
        if !narrow {
            row.label(
                theme::mono("drag a player on the floor")
                    .size(11.0)
                    .color(theme::TEXT_MUTE),
            );
        }
        self.room.drawing = drawing_now;
        if clear {
            let mut b = board.clone();
            b.runs = [None; 5];
            let mut t = tactics.clone();
            t.draw(b);
            let _ = self.world.apply(Command::SetTactics { slot, tactics: t });
        }

        if let Some(l) = left {
            panel(ui, l, "tactics_left", |ui| {
                self.playbook(ui, bd);
                ui.add_space(8.0);
                self.instructions(ui, bd);
            });
        }
        let mut tabs = vec!["Squad", "Board", "Drill"];
        if narrow {
            tabs.insert(0, "Plays");
        }
        let cur = self.sh.tactics_tab.min(tabs.len() - 1);
        let mut pick = None;
        panel_with(ui, right, "tactics_right", false, |ui| {
            pick = tab_row(ui, &tabs, cur, narrow);
            egui::ScrollArea::vertical()
                .id_salt(("tactics_body", tabs[cur]))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    match tabs[cur] {
                        "Plays" => {
                            self.playbook(ui, bd);
                            ui.add_space(8.0);
                            self.instructions(ui, bd);
                        }
                        "Squad" => {
                            self.squad_card(ui, bd);
                            ui.add_space(8.0);
                            self.selected_card(ui, bd);
                        }
                        "Board" => self.board_card(ui, bd, slot),
                        _ => self.rehearsal_card(ui, bd),
                    }
                });
        });
        if let Some(p) = pick {
            self.sh.tactics_tab = p;
        }
    }

    // ---- squad -----------------------------------------------------------

    fn screen_squad(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        stage: Rect,
        tier: Tier,
        g: f32,
        full: Rect,
    ) {
        let roster = self.world.teams[self.team].roster.clone();
        let order = crate::lineup::order(&self.world.players, &roster);
        if self.sh.pick.is_none_or(|p| !roster.contains(&p)) {
            self.sh.pick = order.first().copied();
        }
        if self.sh.squad_full {
            self.sh
                .stage
                .hall(ui, Kind::Data, &self.world, self.team, full, full);
            panel(ui, stage, "squad_full", |ui| {
                if ui.add(egui::Button::new("< Back to the squad")).clicked() {
                    self.sh.squad_full = false;
                }
                ui.add_space(6.0);
                self.squad(ui, bd);
            });
            return;
        }
        let (scene, roster_r, detail_r) = match tier {
            Tier::Wide => {
                let c = cols(stage, &[430.0, -1.0, 390.0], g);
                (c[1], c[0], c[2])
            }
            Tier::Medium => {
                let (top, rest) = split_top(stage, stage.height() * 0.42, g);
                let c = cols(rest, &[-1.2, -1.0], g);
                (top, c[0], c[1])
            }
            Tier::Narrow => {
                let (top, rest) = phone_split(stage, (stage.height() * 0.36).min(300.0), g);
                (top, rest, rest)
            }
        };
        let touched = self.sh.stage.squad(
            ui,
            Kind::Squad,
            &self.world,
            self.team,
            &order,
            self.sh.pick,
            full,
            scene,
            scene,
        );
        if let Some(p) = touched.clicked {
            self.sh.pick = Some(p);
            self.sh.squad_detail = true;
        }
        let pick = self.sh.pick;
        match tier {
            Tier::Narrow => {
                if self.sh.squad_detail {
                    panel(ui, roster_r, "squad_detail", |ui| {
                        if ui.add(egui::Button::new("< Squad")).clicked() {
                            self.sh.squad_detail = false;
                        }
                        ui.add_space(4.0);
                        if let Some(p) = pick {
                            self.squad_detail(ui, bd, p, true);
                        }
                    });
                } else {
                    panel_with(ui, roster_r, "squad_list", false, |ui| {
                        self.squad_roster(ui, &order, true)
                    });
                }
            }
            _ => {
                panel_with(ui, roster_r, "squad_list", false, |ui| {
                    self.squad_roster(ui, &order, false)
                });
                panel(ui, detail_r, "squad_detail", |ui| {
                    if let Some(p) = pick {
                        self.squad_detail(ui, bd, p, false);
                    }
                });
            }
        }
    }
}
