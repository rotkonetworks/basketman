//! The frame of the game: the window is always exactly full, a scene fills
//! it, and everything else is a panel of glass floating over the scene.
//! This file holds the three layouts, the groups the destinations are
//! sorted into, the top bar, the rail and the bottom bar, full screen, and
//! the glass panel every screen is made of. What goes in the panels is
//! composed per destination in `screens.rs`.

use basketman_core::league::World;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, RichText, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, pos2, vec2,
};
use eframe::epaint::{RectShape, Shadow, Shape};

use crate::app::{App, Tab, money};
use crate::theme::{self, ink, white};

/// How much room there is. Chosen from the size of the window, not from
/// what it runs on.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Tier {
    /// Rail and two or three panels side by side.
    Wide,
    /// Rail of short labels, two panels, or a scene over a row of panels.
    Medium,
    /// A bottom bar and one panel at a time.
    Narrow,
    /// A phone held sideways: short and wide. A slim rail on the left, a
    /// thin strip of pages on top, the rest is content.
    Side,
}

impl Tier {
    /// A phone in either hand: touch sizes, and pages that scroll sideways
    /// rather than squeeze.
    pub(crate) fn phone(self) -> bool {
        matches!(self, Tier::Narrow | Tier::Side)
    }
}

/// Window width in points above which the layout is wide.
const WIDE_FROM: f32 = 1100.0;
/// Below this width it is narrow.
const MEDIUM_FROM: f32 = 700.0;
/// Shorter than this, and wider than tall, it is a phone on its side.
const SIDE_BELOW: f32 = 500.0;

pub(crate) fn tier_for(size: egui::Vec2) -> Tier {
    if size.y < SIDE_BELOW && size.x > size.y {
        Tier::Side
    } else if size.x < MEDIUM_FROM {
        Tier::Narrow
    } else if size.x < WIDE_FROM {
        Tier::Medium
    } else {
        Tier::Wide
    }
}

/// The places of the game, in groups of a few pages each.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Group {
    Home,
    Team,
    Club,
    Market,
    League,
    Match,
}

impl Group {
    pub(crate) const ALL: [Group; 6] = [
        Group::Home,
        Group::Team,
        Group::Club,
        Group::Market,
        Group::League,
        Group::Match,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Group::Home => "Home",
            Group::Team => "Team",
            Group::Club => "Club",
            Group::Market => "Market",
            Group::League => "League",
            Group::Match => "Match",
        }
    }

    /// The page a group opens on.
    pub(crate) fn first(self) -> Tab {
        match self {
            Group::Home => Tab::Inbox,
            Group::Team => Tab::Squad,
            Group::Club => Tab::About,
            Group::Market => Tab::Scouting,
            Group::League => Tab::League,
            Group::Match => Tab::Match,
        }
    }

    /// The pages of a group. The draft is one only while it is held.
    pub(crate) fn pages(self, draft: bool) -> Vec<Tab> {
        match self {
            Group::Home => vec![Tab::Inbox, Tab::Calendar, Tab::Career],
            Group::Team => vec![Tab::Squad, Tab::Tactics, Tab::Training, Tab::Reserves],
            Group::Club => vec![Tab::About, Tab::Staff, Tab::Finances, Tab::Arena],
            Group::Market if draft => vec![Tab::Scouting, Tab::Trade, Tab::Draft],
            Group::Market => vec![Tab::Scouting, Tab::Trade],
            Group::League => vec![Tab::League, Tab::Stats],
            Group::Match => vec![Tab::Match, Tab::Film],
        }
    }

    /// The group a page belongs to, links and detail pages included.
    pub(crate) fn of(tab: Tab) -> Group {
        match tab {
            Tab::Inbox | Tab::Calendar | Tab::Career | Tab::Offer | Tab::Welcome => Group::Home,
            Tab::Squad
            | Tab::Tactics
            | Tab::Training
            | Tab::Reserves
            | Tab::Player
            | Tab::Compare => Group::Team,
            Tab::About | Tab::Staff | Tab::Finances | Tab::Arena => Group::Club,
            Tab::Scouting | Tab::Trade | Tab::Draft => Group::Market,
            Tab::League | Tab::Stats | Tab::Club | Tab::Game => Group::League,
            Tab::Match | Tab::Film => Group::Match,
        }
    }
}

/// Short names of the pages, for the strip under the groups.
pub(crate) fn page_label(tab: Tab) -> &'static str {
    match tab {
        Tab::Arena => "Arena",
        Tab::About => "Club",
        Tab::Career => "Career",
        Tab::League => "Table",
        Tab::Film => "Film room",
        t => t.label(),
    }
}

/// What the screens remember about how they are laid out.
#[derive(Default)]
pub(crate) struct Shell {
    /// The player picked in the squad scene.
    pub pick: Option<basketman_core::player::PlayerId>,
    /// Phone: the squad's detail is open over the list.
    pub squad_detail: bool,
    /// The full table of the squad, with every attribute, replaces the
    /// scene's panels.
    pub squad_full: bool,
    /// Phone: a message is open over the list.
    pub mail_open: bool,
    /// The Inbox's list panel shows the mail (otherwise the agenda), where
    /// the agenda has no panel of its own.
    pub mail_tab: bool,
    /// What the last action in the agenda said.
    pub agenda_note: Option<String>,
    /// Which tab of a tabbed panel is open, per screen.
    pub tactics_tab: usize,
    pub league_tab: usize,
    /// The calendar: the month shown (year, 1..=12) and the day picked.
    pub cal_month: Option<(i32, u8)>,
    pub cal_day: Option<u32>,
    /// The draggable parts of a match screen.
    pub hud: crate::matchview::Hud,
    /// Scenes, kept as pictures until something changes.
    pub stage: crate::scenes::Stage,
    /// The size class of the last frame, to notice a change.
    pub last_tier: Option<Tier>,
    /// A phone on its side, on the match: the rail is out over the court.
    pub rail_open: bool,
}

pub(crate) const BAR_TOP: f32 = 52.0;
const SPECTRUM_H: f32 = 3.0;

pub(crate) fn reset_id() -> egui::Id {
    egui::Id::unique("basketman-reset-scroll")
}

/// Light glass: a near-opaque sheet of the page colour with a hairline,
/// a bright rim and a soft shadow.
/// `SHOT_BARE=1` leaves the panels out of a screenshot, to judge the place.
fn bare() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var("SHOT_BARE").is_ok()
    }
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
}

pub(crate) fn glass(ui: &Ui, rect: Rect, radius: u8, shadow: bool) {
    glass_alpha(ui, rect, radius, shadow, 238.0 / 255.0);
}

/// Glass of a chosen solidity (0 to 1), for what floats over the court.
pub(crate) fn glass_alpha(ui: &Ui, rect: Rect, radius: u8, shadow: bool, alpha: f32) {
    if bare() && radius > 0 && rect.height() > 120.0 {
        return;
    }
    let mut shapes: Vec<Shape> = Vec::with_capacity(4);
    if shadow {
        shapes.push(
            Shadow {
                offset: [0, 8],
                blur: 28,
                spread: 0,
                color: ink((46.0 * alpha.min(1.0)) as u8),
            }
            .as_shape(rect, radius)
            .into(),
        );
    }
    let sheet = Color32::from_rgba_unmultiplied(250, 248, 243, (alpha * 255.0) as u8);
    shapes.push(RectShape::filled(rect, radius, sheet).into());
    shapes.push(
        RectShape::stroke(
            rect,
            radius,
            Stroke::new(1.0, theme::LINE_2),
            StrokeKind::Outside,
        )
        .into(),
    );
    shapes.push(
        RectShape::stroke(
            rect,
            radius,
            Stroke::new(1.0, white(235)),
            StrokeKind::Inside,
        )
        .into(),
    );
    ui.painter().extend(shapes);
}

/// Run `add` in a panel of glass at `rect`. The body scrolls inside the
/// panel when it is longer than it; the panel itself never moves.
pub(crate) fn panel(ui: &mut Ui, rect: Rect, salt: &str, add: impl FnOnce(&mut Ui)) {
    panel_wide(ui, rect, salt, 0.0, add);
}

/// A panel whose content wants at least `min_w` points: where the panel is
/// narrower (a phone) the content scrolls sideways as well as down.
pub(crate) fn panel_wide(
    ui: &mut Ui,
    rect: Rect,
    salt: &str,
    min_w: f32,
    add: impl FnOnce(&mut Ui),
) {
    if rect.width() < 40.0 || rect.height() < 40.0 || bare() {
        return;
    }
    glass(ui, rect, 14, true);
    let inner = rect.shrink2(vec2(10.0, 10.0));
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    child.set_clip_rect(rect.shrink(1.0));
    let reset = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(reset_id()))
        .unwrap_or(false);
    let sideways = inner.width() - 14.0 < min_w;
    let mut area = if sideways {
        egui::ScrollArea::both()
    } else {
        egui::ScrollArea::vertical()
    }
    .id_salt(salt)
    .scroll_bar_visibility(if sideways {
        egui::scroll_area::ScrollBarVisibility::AlwaysVisible
    } else {
        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
    })
    .auto_shrink([false, false]);
    if reset {
        area = area.vertical_scroll_offset(0.0);
    }
    let width = (inner.width() - 14.0).max(min_w);
    area.show(&mut child, |ui| {
        ui.set_width(if sideways {
            width
        } else {
            ui.available_width()
        });
        ui.add_space(2.0);
        add(ui);
        ui.add_space(4.0);
    });
}

pub(crate) fn panel_with(
    ui: &mut Ui,
    rect: Rect,
    salt: &str,
    scroll: bool,
    add: impl FnOnce(&mut Ui),
) {
    if scroll {
        return panel(ui, rect, salt, add);
    }
    if rect.width() < 40.0 || rect.height() < 40.0 || bare() {
        return;
    }
    glass(ui, rect, 14, true);
    let inner = rect.shrink2(vec2(10.0, 10.0));
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    child.set_clip_rect(rect.shrink(1.0));
    add(&mut child);
}

/// A row of tabs inside a panel. Returns the tab picked.
pub(crate) fn tab_row(ui: &mut Ui, tabs: &[&str], current: usize, narrow: bool) -> Option<usize> {
    let mut hit = None;
    let h = if narrow {
        40.0
    } else if theme::side() {
        36.0
    } else {
        30.0
    };
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let each = rect.width() / tabs.len().max(1) as f32;
    for (i, label) in tabs.iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(rect.left() + each * i as f32, rect.top()),
            vec2(each, h),
        );
        let resp = ui.interact(r, ui.scope_id().with(("tab", i, *label)), Sense::click());
        let on = i == current;
        let p = ui.painter();
        if on {
            p.rect_filled(r.shrink2(vec2(2.0, 1.0)), 8.0, theme::TEXT);
        } else if resp.hovered() {
            p.rect_filled(r.shrink2(vec2(2.0, 1.0)), 8.0, ink(14));
        }
        p.text(
            r.center(),
            Align2::CENTER_CENTER,
            *label,
            FontId::new(if narrow || theme::side() { 13.0 } else { 12.5 }, theme::semibold()),
            if on { theme::BG } else { theme::TEXT_DIM },
        );
        if resp.clicked() {
            hit = Some(i);
        }
    }
    ui.add_space(6.0);
    hit
}

/// Turn the window full screen or back. Where the platform refuses this
/// nothing happens.
pub(crate) fn toggle_fullscreen(ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let now = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!now));
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ctx;
        use eframe::web_sys;
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            if doc.fullscreen_element().is_some() {
                doc.exit_fullscreen();
            } else if let Some(root) = doc.document_element() {
                // A refusal (no gesture, no permission) is not an error.
                let _ = root.request_fullscreen();
            }
        }
    }
}

pub(crate) fn is_fullscreen(ctx: &egui::Context) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ctx;
        eframe::web_sys::window()
            .and_then(|w| w.document())
            .is_some_and(|d| d.fullscreen_element().is_some())
    }
}

/// 1,075,205 as 1.08M, for a phone.
pub(crate) fn short_money(v: i64) -> String {
    let a = v.abs() as f64;
    let s = if a >= 1e6 {
        format!("{:.2}M", a / 1e6)
    } else if a >= 1e3 {
        format!("{:.0}k", a / 1e3)
    } else {
        format!("{a:.0}")
    };
    if v < 0 { format!("-{s}") } else { s }
}

fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// The few numbers that matter on every screen.
pub(crate) struct Vitals {
    pub record: String,
    pub place: String,
    pub balance: String,
    pub room: String,
    pub next: String,
    pub date: String,
}

pub(crate) fn vitals(app: &App) -> Vitals {
    let w: &World = &app.world;
    let st = &w.standings[app.team];
    let pos = w
        .table()
        .iter()
        .position(|&t| t == app.team)
        .map_or(0, |p| p + 1);
    let next = w.user_fixture().map(|i| &w.fixtures[i]);
    let next = match next {
        Some(f) if f.home == app.team => format!("v {}", w.teams[f.away].name),
        Some(f) => format!("at {}", w.teams[f.home].name),
        None => "season over".to_string(),
    };
    let rounds = w.rounds();
    Vitals {
        record: format!("{}-{}", st.wins, st.losses),
        place: ordinal(pos),
        balance: money(w.teams[app.team].budget),
        room: format!("{:.0}%", w.teams[app.team].belief * 100.0),
        next,
        date: if w.in_preseason() {
            format!("{} / training camp", w.day_label(w.day))
        } else {
            format!(
                "{} / round {} of {}",
                w.day_label(w.day),
                w.round.min(rounds),
                rounds
            )
        },
    }
}

fn chip(ui: &mut Ui, label: &str, value: &str, width: f32, color: Color32) {
    ui.allocate_ui_with_layout(vec2(width, 38.0), Layout::top_down(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.label(theme::eyebrow_text(label));
        ui.add(
            egui::Label::new(
                RichText::new(value)
                    .font(FontId::new(14.5, theme::semibold()))
                    .color(color),
            )
            .truncate(),
        );
    });
}

impl App {
    /// The whole screen: scene, bars and the panels of the page.
    pub(crate) fn shell(&mut self, ui: &mut Ui) {
        let screen = ui.ctx().content_rect();
        let tier = tier_for(screen.size());
        let narrow = tier == Tier::Narrow;
        // A film room left open by the harness or an older session is the
        // match group's page now.
        if self.tab == Tab::Tactics && self.room.view == crate::tactic_room::RoomView::Film {
            self.room.view = crate::tactic_room::RoomView::Board;
            self.tab = Tab::Film;
        }
        if self.sh.last_tier != Some(tier) {
            // Turning the phone changes only how things are laid out: what
            // is open (message, player, tab, replay time) lives elsewhere.
            self.sh.last_tier = Some(tier);
        }
        theme::set_side(tier == Tier::Side);
        if tier == Tier::Side {
            self.shell_side(ui, screen);
            return;
        }
        {
            // Big enough to touch on a phone.
            let style = ui.style_mut();
            if narrow {
                style.spacing.interact_size.y = 40.0;
                style.spacing.button_padding = vec2(14.0, 9.0);
                style.spacing.item_spacing = vec2(10.0, 8.0);
                style.spacing.combo_height = 300.0;
            }
        }
        let reset = ui
            .ctx()
            .data_mut(|d| d.remove_temp::<bool>(egui::Id::unique("page_top_flag")))
            .is_some();
        ui.ctx().data_mut(|d| d.insert_temp(reset_id(), reset));

        self.shell_keys(ui.ctx());

        // The match is the court and nothing else; its own controls float.
        if self.tab == Tab::Match {
            self.shell_match(ui, screen, tier);
            return;
        }
        // Where everything goes.
        let portrait_bar = narrow && screen.height() >= 560.0;
        let top_h = if portrait_bar { 88.0 } else { BAR_TOP };
        let rail_w = match tier {
            Tier::Wide => 104.0,
            Tier::Medium => 74.0,
            Tier::Narrow | Tier::Side => 0.0,
        };
        let short = screen.height() < 480.0;
        let bottom_h = match (narrow, short) {
            (false, _) => 0.0,
            (true, true) => 44.0,
            (true, false) => 58.0,
        };
        let top = Rect::from_min_max(screen.min, pos2(screen.right(), screen.top() + top_h));
        let rail = Rect::from_min_max(
            pos2(screen.left() + 8.0, top.bottom() + 8.0),
            pos2(screen.left() + rail_w, screen.bottom() - 8.0),
        );
        let bottom =
            Rect::from_min_max(pos2(screen.left(), screen.bottom() - bottom_h), screen.max);
        let g = if narrow { 8.0 } else { 12.0 };
        let mut stage = Rect::from_min_max(
            pos2(
                screen.left() + rail_w + if narrow { 0.0 } else { 4.0 },
                top.bottom(),
            ),
            pos2(screen.right(), bottom.top()),
        );
        stage = stage.shrink2(vec2(g, g));
        let group = Group::of(self.tab);
        let pages = group.pages(self.world.draft.is_some() || self.tab == Tab::Draft);
        // The strip of the group's pages: above the panels, or above the
        // bottom bar on a phone.
        let strip_h = if narrow && !short { 44.0 } else { 36.0 };
        let has_strip =
            pages.len() > 1 || Group::of(self.tab) == Group::League && !pages.contains(&self.tab);
        let detail = matches!(self.tab, Tab::Player | Tab::Club | Tab::Game | Tab::Compare);
        let strip = if has_strip || detail {
            if narrow {
                let r = Rect::from_min_max(pos2(stage.left(), stage.bottom() - strip_h), stage.max);
                stage.max.y -= strip_h + 6.0;
                r
            } else {
                let r = Rect::from_min_size(stage.min, vec2(stage.width(), strip_h));
                stage.min.y += strip_h + 8.0;
                r
            }
        } else {
            Rect::NOTHING
        };

        // The pages, then the bars over them.
        self.screen(ui, stage, tier);
        if strip.is_positive() {
            self.page_strip(ui, strip, &pages, tier);
        }
        self.top_bar(ui, top, tier, portrait_bar);
        match tier {
            Tier::Narrow | Tier::Side => self.bottom_bar(ui, bottom),
            _ => self.rail(ui, rail, tier),
        }
        if tier == Tier::Narrow && screen.height() > screen.width() {
            self.turn_hint(ui, bottom, bottom.top() - 58.0);
        }
        self.take_link(ui.ctx());
    }

    /// Once, on the match and the tactics board of a phone held upright: the
    /// game is better sideways. Dismissed for good with a tap.
    pub(crate) fn turn_hint(&mut self, ui: &mut Ui, bottom: Rect, y: f32) {
        let text = match self.tab {
            Tab::Match => "Turn the phone sideways for the full court.",
            Tab::Tactics => "Turn the phone sideways for a bigger board.",
            _ => return,
        };
        let ctx = ui.ctx().clone();
        let id = egui::Id::unique("turn_hint_seen");
        let seen = ctx.data_mut(|d| d.get_persisted::<bool>(id)).unwrap_or(false);
        let forced = self.harness() && std::env::var("SHOT_HINT").is_ok();
        if (seen && !forced) || (self.harness() && !forced) {
            return;
        }
        let w = (bottom.width() - 24.0).min(420.0);
        let mut done = false;
        egui::Area::new(egui::Id::unique("turn_hint"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(bottom.center().x - w / 2.0, y))
            .show(&ctx, |ui| {
                egui::Frame::NONE
                    .fill(theme::TEXT)
                    .corner_radius(12)
                    .inner_margin(egui::Margin::symmetric(14, 6))
                    .show(ui, |ui| {
                        ui.set_width(w - 28.0);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(text).color(theme::BG).size(13.5),
                                )
                                .wrap(),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button("OK").clicked() {
                                    done = true;
                                }
                            });
                        });
                    });
            });
        if done {
            ctx.data_mut(|d| d.insert_persisted(id, true));
        }
    }

    /// 1 to 6 open the groups, Alt+Left goes back, F11 is full screen.
    pub(crate) fn shell_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.search.open {
            return;
        }
        use egui::{Key, Modifiers};
        let keys = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
        ];
        let mut go = None;
        let (back, full) = ctx.input_mut(|i| {
            for (n, k) in keys.into_iter().enumerate() {
                if i.consume_key(Modifiers::NONE, k) {
                    go = Some(Group::ALL[n]);
                }
            }
            (
                i.consume_key(Modifiers::ALT, Key::ArrowLeft),
                i.consume_key(Modifiers::NONE, Key::F11),
            )
        });
        if let Some(g) = go {
            self.open_group(g);
        }
        if back && self.can_go_back() {
            self.go_back();
        }
        if full {
            toggle_fullscreen(ctx);
        }
    }

    /// Open a group on the page it was last on, or its first.
    pub(crate) fn open_group(&mut self, g: Group) {
        if Group::of(self.tab) == g {
            return;
        }
        if g == Group::Club {
            self.club = Some(self.team);
        }
        self.go(g.first());
    }

    fn top_bar(&mut self, ui: &mut Ui, rect: Rect, tier: Tier, portrait: bool) {
        glass(ui, rect, 0, false);
        let spectrum = Rect::from_min_size(rect.min, vec2(rect.width(), SPECTRUM_H));
        ui.painter()
            .add(Shape::Mesh(std::sync::Arc::new(theme::spectrum(spectrum))));
        let mut v = vitals(self);
        let jobless = self.world.watch.is_none();
        let mut club = self.world.teams[self.team].name.clone();
        if jobless {
            // Nothing of the old club is his any more.
            club = "Out of work".to_string();
            v.record = "-".into();
            v.place = String::new();
            v.next = "no club".into();
            v.balance = "-".into();
            v.room = "-".into();
        }
        let narrow = tier == Tier::Narrow;
        let pad = if narrow { 10.0 } else { 16.0 };
        let inner = Rect::from_min_max(
            pos2(rect.left() + pad, rect.top() + SPECTRUM_H),
            pos2(rect.right() - pad, rect.bottom()),
        );
        let mut bar = ui.new_child(
            UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::top_down(Align::Min)),
        );
        bar.set_clip_rect(rect);
        let row_h = if portrait {
            inner.height() / 2.0
        } else {
            inner.height()
        };
        let row1 = Rect::from_min_size(inner.min, vec2(inner.width(), row_h));
        let row2 = Rect::from_min_max(pos2(inner.left(), inner.top() + row_h), inner.max);

        // Right side of the first row: the actions.
        let unread = self.world.unread();
        let actions_rect = if portrait { row1 } else { inner };
        let wide = tier == Tier::Wide;
        let used = bar
            .scope_builder(
                UiBuilder::new()
                    .max_rect(actions_rect)
                    .layout(Layout::right_to_left(Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if !portrait {
                        self.continue_button(ui, !wide);
                        ui.add_space(6.0);
                    }
                    if narrow {
                        // Two rare things behind one touch target.
                        ui.menu_button("More", |ui| {
                            if ui.button("Full screen").clicked() {
                                toggle_fullscreen(ui.ctx());
                                ui.close();
                            }
                            if ui.button("Main menu").clicked() {
                                self.started = false;
                                ui.close();
                            }
                        });
                    } else {
                        let menu = if wide { "Main menu" } else { "Menu" };
                        if ui.add(egui::Button::new(menu)).clicked() {
                            self.started = false;
                        }
                        let fs = match (wide, is_fullscreen(ui.ctx())) {
                            (true, true) => "Exit full screen",
                            (true, false) => "Full screen",
                            (false, _) => "Full",
                        };
                        if ui
                            .add(egui::Button::new(fs))
                            .on_hover_text("Fill the whole screen (F11)")
                            .clicked()
                        {
                            toggle_fullscreen(ui.ctx());
                        }
                    }
                    if crate::search::nav_button(ui) {
                        self.search.show();
                    }
                    let mail = if unread > 0 {
                        format!("Mail {unread}")
                    } else {
                        "Mail".to_string()
                    };
                    let mut b = egui::Button::new(RichText::new(mail).family(theme::semibold()));
                    if unread > 0 {
                        b = b.fill(theme::ACCENT.gamma_multiply(0.16));
                    }
                    if ui.add(b).clicked() {
                        self.go(Tab::Inbox);
                    }
                },
            )
            .response
            .rect;

        // Left side: back, the club, then the figures that fit.
        let left_room = (used.left() - inner.left() - 10.0).max(60.0);
        let left = Rect::from_min_size(
            if portrait { row1.min } else { inner.min },
            vec2(left_room, row_h),
        );
        let mut l = bar.new_child(
            UiBuilder::new()
                .max_rect(left)
                .layout(Layout::left_to_right(Align::Center)),
        );
        l.set_clip_rect(left.expand2(vec2(0.0, 2.0)));
        l.spacing_mut().item_spacing.x = 8.0;
        if self.can_go_back() {
            let b = egui::Button::new(RichText::new("<").family(theme::mono_semibold()));
            if l.add_sized(
                vec2(
                    if narrow { 40.0 } else { 30.0 },
                    if narrow { 36.0 } else { 28.0 },
                ),
                b,
            )
            .on_hover_text(format!("Back to {}", self.back_label()))
            .clicked()
            {
                self.go_back();
            }
        } else {
            crate::title::ball(&mut l, 20.0);
        }
        let name_w = match tier {
            Tier::Wide => 190.0,
            Tier::Medium => 150.0,
            Tier::Narrow | Tier::Side => (left_room - 52.0).clamp(80.0, 190.0),
        };
        l.allocate_ui_with_layout(vec2(name_w, 40.0), Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add(
                egui::Label::new(RichText::new(&club).font(FontId::new(
                    if narrow { 16.0 } else { 17.0 },
                    theme::semibold(),
                )))
                .truncate(),
            );
            ui.add(
                egui::Label::new(
                    RichText::new(&v.date)
                        .font(FontId::new(crate::theme::fs(10.5), egui::FontFamily::Monospace))
                        .color(theme::TEXT_MUTE),
                )
                .truncate(),
            );
        });
        let rec = format!("{} / {}", v.record, v.place);
        if portrait {
            // Second row: the figures, two short lines, and Continue.
            let mut r2 = bar.new_child(
                UiBuilder::new()
                    .max_rect(row2)
                    .layout(Layout::right_to_left(Align::Center)),
            );
            r2.spacing_mut().item_spacing.x = 6.0;
            self.continue_button(&mut r2, true);
            let room = r2.available_width();
            r2.allocate_ui_with_layout(vec2(room, row_h), Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add(
                    egui::Label::new(
                        theme::mono(format!(
                            "{rec}  {}",
                            if jobless { "-".to_string() } else { short_money(self.world.teams[self.team].budget) }
                        ))
                        .size(12.0)
                        .color(theme::TEXT),
                    )
                    .truncate(),
                );
                ui.add(
                    egui::Label::new(
                        theme::mono(format!("{}  room {}", v.next, v.room))
                            .size(crate::theme::fs(11.0))
                            .color(theme::TEXT_DIM),
                    )
                    .truncate(),
                );
            });
        } else {
            let mut room = l.available_width() - 4.0;
            let chips: [(&str, String, f32); 4] = if wide {
                [
                    ("Record", rec, 110.0),
                    ("Next", v.next.clone(), 170.0),
                    ("Balance", v.balance.clone(), 104.0),
                    ("Room", v.room.clone(), 64.0),
                ]
            } else {
                [
                    ("Record", rec, 92.0),
                    ("Next", v.next.clone(), 130.0),
                    ("Balance", v.balance.clone(), 88.0),
                    ("Room", v.room.clone(), 54.0),
                ]
            };
            // In the order the window can spare them.
            let mut shown = [false; 4];
            for (i, c) in chips.iter().enumerate() {
                if room >= c.2 + 12.0 {
                    shown[i] = true;
                    room -= c.2 + 12.0;
                } else {
                    break;
                }
            }
            l.add_space(6.0);
            for (i, (label, value, w)) in chips.iter().enumerate() {
                if shown[i] {
                    chip(&mut l, label, value, *w, theme::TEXT);
                }
            }
        }
    }

    /// The groups down the side of the screen.
    fn rail(&mut self, ui: &mut Ui, rect: Rect, tier: Tier) {
        glass(ui, rect, 14, true);
        let wide = tier == Tier::Wide;
        let item_h = if wide { 64.0 } else { 58.0 };
        let unread = self.world.unread();
        let here = Group::of(self.tab);
        let mut open = None;
        for (n, g) in Group::ALL.iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(
                    rect.left() + 6.0,
                    rect.top() + 8.0 + n as f32 * (item_h + 4.0),
                ),
                vec2(rect.width() - 12.0, item_h),
            );
            if r.bottom() > rect.bottom() {
                break;
            }
            let resp = ui
                .interact(r, egui::Id::unique(("rail", n)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            let on = *g == here;
            let p = ui.painter();
            if on {
                p.rect_filled(r, 10.0, white(255));
                p.rect_stroke(r, 10.0, Stroke::new(1.0, theme::LINE_2), StrokeKind::Inside);
                p.rect_filled(
                    Rect::from_min_size(
                        pos2(r.left(), r.top() + 12.0),
                        vec2(3.0, r.height() - 24.0),
                    ),
                    CornerRadius::same(2),
                    theme::ACCENT,
                );
            } else if resp.hovered() {
                p.rect_filled(r, 10.0, ink(12));
            }
            let c = if wide {
                r.center() - vec2(0.0, 6.0)
            } else {
                r.center()
            };
            p.text(
                c,
                Align2::CENTER_CENTER,
                g.label(),
                FontId::new(if wide { 14.5 } else { 12.5 }, theme::semibold()),
                if on { theme::TEXT } else { theme::TEXT_DIM },
            );
            if wide {
                p.text(
                    r.center() + vec2(0.0, 15.0),
                    Align2::CENTER_CENTER,
                    format!("{}", n + 1),
                    FontId::new(crate::theme::fs(10.5), egui::FontFamily::Monospace),
                    theme::TEXT_MUTE,
                );
            }
            if *g == Group::Home && unread > 0 {
                let at = pos2(r.right() - 12.0, r.top() + 12.0);
                p.circle_filled(at, 8.0, theme::ACCENT);
                p.text(
                    at,
                    Align2::CENTER_CENTER,
                    unread.min(99).to_string(),
                    FontId::new(crate::theme::fs(9.5), theme::mono_semibold()),
                    Color32::WHITE,
                );
            }
            if resp.clicked() {
                open = Some(*g);
            }
        }
        if let Some(g) = open {
            self.open_group(g);
        }
    }

    /// The groups along the bottom of a phone.
    fn bottom_bar(&mut self, ui: &mut Ui, rect: Rect) {
        glass(ui, rect, 0, true);
        let each = rect.width() / Group::ALL.len() as f32;
        let unread = self.world.unread();
        let here = Group::of(self.tab);
        let mut open = None;
        for (n, g) in Group::ALL.iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(rect.left() + each * n as f32, rect.top()),
                vec2(each, rect.height()),
            );
            let resp = ui.interact(r, egui::Id::unique(("bottom", n)), Sense::click());
            let on = *g == here;
            let p = ui.painter();
            if on {
                p.rect_filled(
                    Rect::from_min_size(
                        pos2(r.left() + 10.0, r.top()),
                        vec2(r.width() - 20.0, 3.0),
                    ),
                    CornerRadius::same(2),
                    theme::ACCENT,
                );
            }
            p.text(
                r.center() - vec2(0.0, 6.0),
                Align2::CENTER_CENTER,
                g.label(),
                FontId::new(13.0, theme::semibold()),
                if on { theme::TEXT } else { theme::TEXT_DIM },
            );
            p.text(
                r.center() + vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                format!("{}", n + 1),
                FontId::new(crate::theme::fs(9.5), egui::FontFamily::Monospace),
                theme::TEXT_MUTE,
            );
            if *g == Group::Home && unread > 0 {
                let at = pos2(r.center().x + 22.0, r.top() + 14.0);
                p.circle_filled(at, 7.5, theme::ACCENT);
                p.text(
                    at,
                    Align2::CENTER_CENTER,
                    unread.min(99).to_string(),
                    FontId::new(crate::theme::fs(9.0), theme::mono_semibold()),
                    Color32::WHITE,
                );
            }
            if resp.clicked() {
                open = Some(*g);
            }
        }
        if let Some(g) = open {
            self.open_group(g);
        }
    }

    /// The pages of the open group as pills, and the detail page open over
    /// them, if any.
    fn page_strip(&mut self, ui: &mut Ui, rect: Rect, pages: &[Tab], tier: Tier) {
        let narrow = tier == Tier::Narrow;
        let mut labels: Vec<(String, Tab)> = pages
            .iter()
            .map(|&t| (page_label(t).to_string(), t))
            .collect();
        let detail = match self.tab {
            Tab::Player => self
                .profile
                .map(|p| self.world.players[p as usize].name.clone()),
            Tab::Club => self.club.map(|t| self.world.teams[t].name.clone()),
            Tab::Game => Some("Box score".to_string()),
            Tab::Compare => Some("Compare".to_string()),
            _ => None,
        };
        if let Some(d) = detail {
            let surname = d.split_whitespace().next_back().unwrap_or(&d).to_string();
            labels.push((if narrow { surname } else { d }, self.tab));
        }
        let font = FontId::new(if narrow { 13.5 } else { 13.0 }, theme::semibold());
        let widths: Vec<f32> = labels
            .iter()
            .map(|(l, _)| {
                ui.painter()
                    .layout_no_wrap(l.clone(), font.clone(), Color32::BLACK)
                    .size()
                    .x
                    + 28.0
            })
            .collect();
        let total: f32 = widths.iter().sum::<f32>() + 4.0 * labels.len() as f32;
        let bar = if narrow {
            rect
        } else {
            Rect::from_min_size(rect.min, vec2(total + 8.0, rect.height()))
        };
        glass(ui, bar, (bar.height() / 2.0) as u8, true);
        let each = if narrow {
            (bar.width() - 8.0) / labels.len() as f32
        } else {
            0.0
        };
        let mut x = bar.left() + 4.0;
        let mut go = None;
        for (i, (label, tab)) in labels.iter().enumerate() {
            let w = if narrow { each } else { widths[i] };
            let r = Rect::from_min_size(pos2(x, bar.top() + 3.0), vec2(w, bar.height() - 6.0));
            x += w + if narrow { 0.0 } else { 4.0 };
            let resp = ui
                .interact(r, egui::Id::unique(("page", i)), Sense::click())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_follows_the_space_not_the_platform() {
        assert_eq!(tier_for(vec2(1440.0, 900.0)), Tier::Wide);
        assert_eq!(tier_for(vec2(1024.0, 768.0)), Tier::Medium);
        assert_eq!(tier_for(vec2(390.0, 844.0)), Tier::Narrow);
        assert_eq!(tier_for(vec2(430.0, 932.0)), Tier::Narrow);
        // A phone on its side is its own layout, however wide it is.
        assert_eq!(tier_for(vec2(800.0, 380.0)), Tier::Side);
        assert_eq!(tier_for(vec2(844.0, 390.0)), Tier::Side);
        assert_eq!(tier_for(vec2(932.0, 430.0)), Tier::Side);
        assert_eq!(tier_for(vec2(667.0, 375.0)), Tier::Side);
        assert_eq!(tier_for(vec2(1100.0, 499.0)), Tier::Side);
        assert_eq!(tier_for(vec2(1100.0, 500.0)), Tier::Wide);
        assert_eq!(tier_for(vec2(360.0, 480.0)), Tier::Narrow);
        assert_eq!(tier_for(vec2(1100.0, 700.0)), Tier::Wide);
        assert_eq!(tier_for(vec2(1099.0, 700.0)), Tier::Medium);
        assert_eq!(tier_for(vec2(700.0, 600.0)), Tier::Medium);
        assert_eq!(tier_for(vec2(699.0, 900.0)), Tier::Narrow);
    }

    #[test]
    fn every_page_is_in_exactly_one_group_and_found_there() {
        let mut seen = 0;
        for g in Group::ALL {
            for t in g.pages(true) {
                assert_eq!(Group::of(t), g, "{}", t.label());
                seen += 1;
            }
        }
        // Twelve destinations, Film and Arena added, Draft too, and the
        // calendar, the club's story and the manager's career under Home.
        assert_eq!(seen, 18);
        for t in Tab::ALL {
            assert!(
                Group::ALL.iter().any(|g| g.pages(true).contains(&t)),
                "{}",
                t.label()
            );
        }
    }

    #[test]
    fn the_groups_open_on_one_of_their_own_pages() {
        for g in Group::ALL {
            assert!(g.pages(false).contains(&g.first()));
        }
    }
}
