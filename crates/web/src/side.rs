//! The frame of a phone held sideways. There is no bar across the top or
//! the bottom: a slim rail of the six groups stands on the left edge, one
//! thin strip over the content holds the pages of the group, the club and
//! the one button that moves the game on, and the rest is the page. On the
//! match the court takes the whole screen and the rail comes out over it
//! when asked for.

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, RichText, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, pos2, vec2,
};

use crate::app::{App, Tab};
use crate::shell::{Group, Tier, glass, page_label, reset_id, vitals};
use crate::theme::{self, ink, white};

/// Width of the rail.
pub(crate) const RAIL_W: f32 = 58.0;
/// Height of the strip over the content.
pub(crate) const STRIP_H: f32 = 34.0;
/// Clear of the short edges (the notch may sit there).
pub(crate) const EDGE: f32 = 8.0;
/// Clear of the left and right ends, where only the rail may have controls.
pub(crate) const SAFE: f32 = 20.0;

impl App {
    /// The whole screen of a phone on its side.
    pub(crate) fn shell_side(&mut self, ui: &mut Ui, screen: Rect) {
        {
            // Big enough to touch, small enough to leave room.
            let style = ui.style_mut();
            style.spacing.interact_size.y = 36.0;
            style.spacing.button_padding = vec2(12.0, 6.0);
            style.spacing.item_spacing = vec2(8.0, 6.0);
            style.spacing.combo_height = 220.0;
        }
        let reset = ui
            .ctx()
            .data_mut(|d| d.remove_temp::<bool>(egui::Id::unique("page_top_flag")))
            .is_some();
        ui.ctx().data_mut(|d| d.insert_temp(reset_id(), reset));
        self.shell_keys(ui.ctx());

        if self.tab == Tab::Match {
            self.shell_match(ui, screen, Tier::Side);
            return;
        } else {
            self.sh.rail_open = false;
            let rail = Rect::from_min_max(
                pos2(screen.left() + 6.0, screen.top() + EDGE),
                pos2(screen.left() + 6.0 + RAIL_W, screen.bottom() - EDGE),
            );
            let left = rail.right() + 6.0;
            let strip = Rect::from_min_size(
                pos2(left, screen.top() + 5.0),
                vec2(screen.right() - SAFE - left, STRIP_H),
            );
            let stage = Rect::from_min_max(
                pos2(left, strip.bottom() + 5.0),
                pos2(screen.right() - SAFE, screen.bottom() - EDGE),
            );
            let group = Group::of(self.tab);
            let pages = group.pages(self.world.draft.is_some() || self.tab == Tab::Draft);
            self.screen(ui, stage, Tier::Side);
            self.side_strip(ui, strip, &pages);
            self.side_rail(ui, rail, false);
        }
        self.take_link(ui.ctx());
    }

    /// The match screen on every size: the court has the whole window, the
    /// match draws its own controls, and the navigation comes out over it
    /// from the Menu button or Esc.
    pub(crate) fn shell_match(&mut self, ui: &mut Ui, screen: Rect, tier: Tier) {
        let stage = Rect::from_min_max(
            pos2(screen.left() + SAFE, screen.top() + EDGE),
            pos2(screen.right() - SAFE, screen.bottom() - EDGE),
        );
        let ctx = ui.ctx().clone();
        if !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.sh.rail_open = !self.sh.rail_open;
        }
        self.screen(ui, stage, tier);
        self.side_menu(ui, screen);
        if tier == Tier::Narrow && screen.height() > screen.width() {
            let bottom = Rect::from_min_max(pos2(screen.left(), screen.bottom() - 8.0), screen.max);
            self.turn_hint(ui, bottom, 112.0);
        }
        self.take_link(&ctx);
    }

    /// The navigation, out over the match when asked for.
    fn side_menu(&mut self, ui: &mut Ui, screen: Rect) {
        let t = ui.ctx().animate_bool_with_time(
            egui::Id::unique("side_menu"),
            self.sh.rail_open,
            if crate::settings::reduced() { 0.0 } else { 0.15 },
        );
        if t > 0.01 {
            // A tap anywhere else puts it away.
            let scrim = ui.interact(screen, egui::Id::unique("side_scrim"), Sense::click());
            ui.painter()
                .rect_filled(screen, 0.0, Color32::from_black_alpha((90.0 * t) as u8));
            let w = RAIL_W + 14.0;
            let x = screen.left() + 6.0 - (w + 6.0) * (1.0 - t);
            let rail = Rect::from_min_max(
                pos2(x, screen.top() + EDGE),
                pos2(x + w, screen.bottom() - EDGE),
            );
            self.side_rail(ui, rail, true);
            if scrim.clicked() {
                self.sh.rail_open = false;
            }
        }
    }

    /// The six groups down the left edge. Over the court (`over`) a seventh
    /// item leads to the film room, which has no strip of its own there.
    fn side_rail(&mut self, ui: &mut Ui, rect: Rect, over: bool) {
        glass(ui, rect, 14, true);
        let unread = self.world.unread();
        let here = Group::of(self.tab);
        let n_items = if over { 7 } else { 6 };
        let gap = 3.0;
        let item_h = ((rect.height() - 12.0 - gap * (n_items as f32 - 1.0)) / n_items as f32)
            .min(54.0);
        let mut open = None;
        let mut film = false;
        for n in 0..n_items {
            let r = Rect::from_min_size(
                pos2(rect.left() + 4.0, rect.top() + 6.0 + n as f32 * (item_h + gap)),
                vec2(rect.width() - 8.0, item_h),
            );
            let g = Group::ALL.get(n).copied();
            let resp = ui
                .interact(r, egui::Id::unique(("side_rail", n, over)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            let on = g == Some(here) && !(over && g == Some(Group::Match) && false);
            let p = ui.painter();
            if on {
                p.rect_filled(r, 10.0, white(255));
                p.rect_stroke(r, 10.0, Stroke::new(1.0, theme::LINE_2), StrokeKind::Inside);
                p.rect_filled(
                    Rect::from_min_size(
                        pos2(r.left(), r.top() + 10.0),
                        vec2(3.0, r.height() - 20.0),
                    ),
                    CornerRadius::same(2),
                    theme::ACCENT,
                );
            } else if resp.hovered() {
                p.rect_filled(r, 10.0, ink(12));
            }
            let label = g.map_or("Film", |g| g.label());
            p.text(
                r.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::new(12.5, theme::semibold()),
                if on { theme::TEXT } else { theme::TEXT_DIM },
            );
            if g == Some(Group::Home) && unread > 0 {
                let at = pos2(r.right() - 9.0, r.top() + 9.0);
                p.circle_filled(at, 8.0, theme::ACCENT);
                p.text(
                    at,
                    Align2::CENTER_CENTER,
                    unread.min(99).to_string(),
                    FontId::new(crate::theme::fs(10.0), theme::mono_semibold()),
                    Color32::WHITE,
                );
            }
            if resp.clicked() {
                match g {
                    Some(g) => open = Some(g),
                    None => film = true,
                }
            }
        }
        if open.is_some() || film {
            self.sh.rail_open = false;
        }
        if let Some(g) = open {
            // Pressing the group you are in goes to its first page.
            if Group::of(self.tab) == g && self.tab != g.first() {
                self.go(g.first());
            } else {
                self.open_group(g);
            }
        }
        if film {
            self.go(Tab::Film);
        }
    }

    /// The pages of the group, the club, and the one button that moves the
    /// game on, in one thin line.
    fn side_strip(&mut self, ui: &mut Ui, rect: Rect, pages: &[Tab]) {
        let mut v = vitals(self);
        let jobless = self.world.watch.is_none();
        let mut club = self.world.teams[self.team].name.clone();
        if jobless {
            club = "Out of work".to_string();
            v.record = "-".into();
            v.place = String::new();
        }
        let mid = format!("{club}  {}  {} {}", v.date.split(" / ").next().unwrap_or(""), v.record, v.place);

        // The right end first: more, and the button that moves on.
        let mut bar = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .layout(Layout::right_to_left(Align::Center)),
        );
        bar.set_clip_rect(rect.expand2(vec2(0.0, 4.0)));
        glass(ui, rect, (rect.height() / 2.0) as u8, true);
        let used = {
            bar.spacing_mut().item_spacing.x = 6.0;
            bar.spacing_mut().interact_size.y = STRIP_H - 4.0;
            bar.spacing_mut().button_padding = vec2(12.0, 5.0);
            bar.scope(|ui| {
                self.continue_button(ui, true);
                ui.menu_button("More", |ui| {
                    if ui.button("Search").clicked() {
                        self.search.show();
                        ui.close();
                    }
                    let unread = self.world.unread();
                    let mail = if unread > 0 { format!("Mail ({unread})") } else { "Mail".to_string() };
                    if ui.button(mail).clicked() {
                        self.go(Tab::Inbox);
                        ui.close();
                    }
                    if ui.button("Full screen").clicked() {
                        crate::shell::toggle_fullscreen(ui.ctx());
                        ui.close();
                    }
                    if ui.button("Main menu").clicked() {
                        self.open_pause();
                        ui.close();
                    }
                });
            })
            .response
            .rect
        };
        // The left: back, and the pages as pills.
        let mut labels: Vec<(String, Tab)> = pages
            .iter()
            .map(|&t| (page_label(t).to_string(), t))
            .collect();
        let detail = match self.tab {
            Tab::Player => self.profile.map(|p| self.world.players[p as usize].name.clone()),
            Tab::Club => self.club.map(|t| self.world.teams[t].name.clone()),
            Tab::Game => Some("Box score".to_string()),
            Tab::Compare => Some("Compare".to_string()),
            _ => None,
        };
        if let Some(d) = detail {
            let surname = d.split_whitespace().next_back().unwrap_or(&d).to_string();
            labels.push((surname, self.tab));
        }
        let font = FontId::new(13.0, theme::semibold());
        let mut x = rect.left();
        let y0 = rect.top() + 2.0;
        let h = rect.height() - 4.0;
        let mut go: Option<Tab> = None;
        let mut back = false;
        if self.can_go_back() {
            let r = Rect::from_min_size(pos2(x, y0), vec2(40.0, h));
            let resp = ui.interact(r.expand2(vec2(0.0, 3.0)), egui::Id::unique("side_back"), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, r.height() / 2.0, ink(14));
            }
            ui.painter().text(
                r.center(),
                Align2::CENTER_CENTER,
                "<",
                FontId::new(15.0, theme::mono_semibold()),
                theme::TEXT,
            );
            back = resp.clicked();
            x += 44.0;
        }
        let widths: Vec<f32> = labels
            .iter()
            .map(|(l, _)| {
                ui.painter()
                    .layout_no_wrap(l.clone(), font.clone(), Color32::BLACK)
                    .size()
                    .x
                    + 26.0
            })
            .collect();
        let total: f32 = widths.iter().sum::<f32>() + 3.0 * labels.len() as f32 + 6.0;
        let bar_r = Rect::from_min_size(pos2(x, y0), vec2(total, h));
        let x = x + 4.0;
        let mut px = x;
        for (i, (label, tab)) in labels.iter().enumerate() {
            let r = Rect::from_min_size(pos2(px, y0 + 2.0), vec2(widths[i], h - 4.0));
            px += widths[i] + 3.0;
            // The touch target reaches into the gaps above and below.
            let resp = ui
                .interact(r.expand2(vec2(1.0, 4.0)), egui::Id::unique(("side_page", i)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            let on = *tab == self.tab;
            let p = ui.painter();
            if on {
                p.rect_filled(r, r.height() / 2.0, theme::TEXT);
            } else if resp.hovered() {
                p.rect_filled(r, r.height() / 2.0, ink(14));
            }
            p.text(
                r.center(),
                Align2::CENTER_CENTER,
                label,
                font.clone(),
                if on { theme::BG } else { theme::TEXT_DIM },
            );
            if resp.clicked() {
                go = Some(*tab);
            }
        }
        // The club and the date, where there is room for them.
        let room = used.left() - bar_r.right() - 20.0;
        if room > 110.0 {
            let mid_r = Rect::from_min_max(
                pos2(bar_r.right() + 12.0, rect.top()),
                pos2(used.left() - 8.0, rect.bottom()),
            );
            ui.put(
                mid_r,
                egui::Label::new(
                    RichText::new(mid)
                        .font(FontId::new(12.0, theme::semibold()))
                        .color(theme::TEXT_DIM),
                )
                .truncate(),
            );
        }
        if back {
            self.go_back();
        }
        if let Some(t) = go
            && t != self.tab
        {
            if t == Tab::Arena {
                self.club = Some(self.team);
            }
            self.go(t);
        }
    }
}
