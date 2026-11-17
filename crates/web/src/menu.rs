//! The front of the game: the main menu, loading and saving careers, the
//! settings, the guide and the credits; and the small menu that opens over
//! the game. Everything that touches storage is queued as an `Op` and run
//! at the end of the frame, where the storage is at hand.

use basketman_core::league::World;
use eframe::egui::{
    self, Align2, Color32, FontId, Key, Modifiers, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2,
};

use crate::app::{App, Tab};
use crate::matchview::ViewMode;
use crate::persist::{self, Loaded, SlotMeta};
use crate::settings::{Autosave, SCALES, SPEEDS};
use crate::shell::{Tier, tier_for};
use crate::theme::{self, Backdrop};

/// Where the player is, in the front end or in the menu over the game.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Page {
    Main,
    New,
    Load,
    Settings,
    HowTo,
    Credits,
    /// Over the game: the small menu.
    Pause,
    /// Over the game: name a new slot.
    SaveAs,
    /// Over the game: leaving would give up a match in progress.
    ConfirmLeave,
}

impl Page {
    pub fn from_key(k: &str) -> Option<Page> {
        Some(match k {
            "main" => Page::Main,
            "load" => Page::Load,
            "settings" => Page::Settings,
            "howto" => Page::HowTo,
            "credits" => Page::Credits,
            "pause" => Page::Pause,
            "saveas" => Page::SaveAs,
            "leave" => Page::ConfirmLeave,
            _ => return None,
        })
    }
}

/// Work for the end of the frame, when the storage is at hand.
pub(crate) enum Op {
    /// Resume the latest career.
    Continue,
    Load(u32),
    Save,
    SaveNew(String),
    Delete(u32),
    Rename(u32, String),
    /// Save, then leave for the main menu.
    ExitToMenu,
    /// Start the career the flow has put together.
    Start,
    Export(u32),
    Import(Vec<u8>),
    Quit,
    /// For the screenshot harness: a career put in a slot as it is.
    Stash(Box<World>, String),
}

/// What the menus remember between frames.
#[derive(Default)]
pub(crate) struct MenuState {
    pub sel: usize,
    pub howto: usize,
    /// Slot being renamed, and the text so far.
    pub renaming: Option<(u32, String)>,
    pub confirm_delete: Option<u32>,
    pub save_name: String,
    /// Files in the exports folder, for the native import.
    pub files: Vec<std::path::PathBuf>,
    pub files_read: bool,
    /// A page to open once the game has been left.
    pub want: Option<Page>,
}

// ---- small widgets --------------------------------------------------------

fn setting_row(ui: &mut Ui, label: &str, hint: &str, add: &mut dyn FnMut(&mut Ui)) {
    ui.add_space(2.0);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(RichText::new(label).family(theme::semibold()));
        add(ui);
        if !hint.is_empty() {
            ui.label(RichText::new(hint).size(12.0).color(theme::TEXT_MUTE));
        }
    });
    ui.add_space(4.0);
    ui.separator();
}

/// A small label in a column of fixed width, left aligned.
pub(crate) fn key_label(ui: &mut Ui, width: f32, text: &str) {
    ui.allocate_ui_with_layout(vec2(width, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.add(egui::Label::new(theme::eyebrow_text(text)).wrap_mode(egui::TextWrapMode::Extend));
    });
}

/// A row of choices, one of them on.
pub(crate) fn segmented<T: PartialEq + Clone>(ui: &mut Ui, options: &[(T, String)], value: &mut T) -> bool {
    let mut changed = false;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.spacing_mut().button_padding = vec2(12.0, 6.0);
        ui.horizontal_wrapped(|ui| {
            for (v, label) in options {
                let on = value == v;
                let text = if on {
                    RichText::new(label).family(theme::semibold()).color(theme::BG)
                } else {
                    RichText::new(label)
                };
                let mut b = egui::Button::new(text).min_size(vec2(44.0, 36.0));
                if on {
                    b = b.fill(theme::TEXT).stroke(Stroke::NONE);
                }
                if ui.add(b).clicked() && !on {
                    *value = v.clone();
                    changed = true;
                }
            }
        });
    });
    changed
}

#[derive(Copy, Clone, PartialEq)]
enum Look {
    Primary,
    Plain,
}

/// A big button for a menu: title, optional lines under it. True when
/// clicked; `on` is the key selection.
fn big_button(ui: &mut Ui, width: f32, title: &str, lines: &[String], look: Look, on: bool) -> (egui::Response, f32) {
    let side = theme::side();
    let line_h = if side { 15.0 } else { 17.0 };
    let base: f32 = if side { 36.0 } else { 46.0 };
    let h = if lines.is_empty() {
        base
    } else {
        (base - 6.0).max(26.0) + lines.len() as f32 * line_h + 6.0
    };
    let (rect, r) = ui.allocate_exact_size(vec2(width, h), Sense::click());
    let hot = on || r.hovered();
    let p = ui.painter();
    let (fill, text, dim) = match look {
        Look::Primary => (
            if hot { theme::ACCENT } else { theme::TEXT },
            theme::BG,
            theme::white(210),
        ),
        Look::Plain => (
            if hot { theme::white(250) } else { theme::white(185) },
            theme::TEXT,
            theme::TEXT_DIM,
        ),
    };
    p.rect_filled(rect, 9.0, fill);
    let stroke = if on && look == Look::Plain {
        Stroke::new(2.0, theme::ACCENT)
    } else if look == Look::Plain {
        Stroke::new(1.0, if hot { theme::ACCENT } else { theme::LINE_2 })
    } else {
        Stroke::new(if on { 2.0 } else { 0.0 }, theme::ACCENT)
    };
    p.rect_stroke(rect, 9.0, stroke, egui::StrokeKind::Inside);
    let top = if lines.is_empty() { rect.center().y } else { rect.top() + (base - 6.0).max(26.0) * 0.5 + 3.0 };
    p.text(
        pos2(rect.left() + 18.0, top),
        Align2::LEFT_CENTER,
        title,
        FontId::new(if side { 15.0 } else { 17.0 }, theme::semibold()),
        text,
    );
    for (i, l) in lines.iter().enumerate() {
        p.text(
            pos2(rect.left() + 18.0, top + 11.0 + line_h * (i as f32 + 0.6)),
            Align2::LEFT_CENTER,
            l,
            FontId::new(if side { 11.5 } else { 12.5 }, egui::FontFamily::Proportional),
            dim,
        );
    }
    p.text(
        pos2(rect.right() - 16.0, rect.center().y),
        Align2::RIGHT_CENTER,
        ">",
        FontId::new(15.0, egui::FontFamily::Monospace),
        dim,
    );
    (r.on_hover_cursor(egui::CursorIcon::PointingHand), h)
}

/// What the arrow keys, Enter and Escape did this frame, unless a text
/// field has the keyboard.
#[derive(Default, Clone, Copy)]
pub(crate) struct Keys {
    pub up: bool,
    pub down: bool,
    pub enter: bool,
    pub esc: bool,
}

pub(crate) fn take_keys(ctx: &egui::Context) -> Keys {
    let mut k = Keys::default();
    let typing = ctx.egui_wants_keyboard_input();
    ctx.input_mut(|i| {
        if !typing {
            k.up = i.consume_key(Modifiers::NONE, Key::ArrowUp);
            k.down = i.consume_key(Modifiers::NONE, Key::ArrowDown);
            k.enter = i.consume_key(Modifiers::NONE, Key::Enter);
        }
        k.esc = i.consume_key(Modifiers::NONE, Key::Escape);
    });
    k
}

const HOWTO: [(&str, &[&str]); 6] = [
    (
        "What a manager does",
        &[
            "You run a basketball club. You choose the players, the tactics, the staff and how the money is spent.",
            "The board watches the results and decides if you keep the job. Good seasons raise your reputation, and bigger clubs call.",
            "You start in the training camp. The season begins when the camp is over.",
        ],
    ),
    (
        "A normal week",
        &[
            "Read the Mail. The board, your assistant and the clubs write to you there.",
            "Look after the squad: training, the lineup, scouting, and the market for new players.",
            "Press Continue to pass the day. The game stops by itself when something needs you.",
            "A match day brings a game. Win more than you lose.",
        ],
    ),
    (
        "Match day",
        &[
            "Coach the game live, watch it, or let your assistant play it and give you the result.",
            "Watch it in 3D, from above in 2D, or as plain commentary. Change this in Settings.",
            "In a live game you can call a timeout and make substitutions. Tactics and talks change only when play is stopped: in a timeout, at a break, or before the tip.",
        ],
    ),
    (
        "Money and the board",
        &[
            "The Finances pages show what comes in (gate, sponsors) and what goes out (wages, the hall). Ticket prices are yours to set.",
            "There is a salary cap, a wage budget and a transfer budget. The board's confidence rises and falls with results.",
            "Doing better than the board asked raises your reputation. Doing worse can cost you the job.",
        ],
    ),
    (
        "Seasons and divisions",
        &[
            "Each season is a league of home and away games, then playoffs, then the draft.",
            "The best clubs of a division go up, the worst go down. Start in the Third Division and climb for years, or take a top club and defend it.",
            "Careers go on for many seasons. Players age, retire and are replaced.",
        ],
    ),
    (
        "Keys and touch",
        &[
            "Ctrl+K searches players, clubs and pages. 1 to 6 open the groups of pages. Alt+Left goes back. F11 is full screen.",
            "In a match: Space plays and pauses, T calls a timeout, Esc puts the coaching room away.",
            "In menus: arrows choose, Enter selects, Esc goes back.",
            "On a phone, hold it sideways. Everything can be touched; nothing needs a mouse.",
        ],
    ),
];

impl App {
    /// Whether saving to storage is switched on in this run (not in the
    /// screenshot harness, unless asked).
    pub(crate) fn persisting(&self) -> bool {
        self.persist
    }

    pub(crate) fn open_pause(&mut self) {
        self.pause = Some(Page::Pause);
        self.menu.sel = 0;
    }

    /// Ask to do something that leaves the game. With a match in progress
    /// that cannot be saved, and the player's wish to be asked, it waits
    /// for an answer.
    pub(crate) fn request(&mut self, op: Op) {
        if self.started && self.live.is_some() && self.settings.confirm_leave {
            self.leaving = Some(op);
            self.pause = Some(Page::ConfirmLeave);
        } else {
            self.ops.push(op);
        }
    }

    /// The page around the game chose where the visitor goes (browser only).
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn follow_hash(&mut self, hash: &str) {
        let want = match hash {
            "#new" => Page::New,
            "#load" => Page::Load,
            "#play" => Page::Main,
            "#continue" => {
                if self.slots.latest().is_some() {
                    if self.started {
                        self.pause = None;
                    } else {
                        self.request(Op::Continue);
                    }
                }
                return;
            }
            _ => return,
        };
        if self.started {
            self.menu.want = Some(want);
            self.request(Op::ExitToMenu);
        } else if want == Page::New {
            self.open_new_career();
        } else {
            self.front = want;
        }
    }

    // ---- the front screens ------------------------------------------------

    pub(crate) fn front_ui(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let side = tier_for(ui.ctx().content_rect().size()) == Tier::Side;
        theme::set_side(side);
        match self.front {
            Page::New => self.new_career_screen(ui, bd),
            Page::Load | Page::Settings | Page::HowTo | Page::Credits => {
                if take_keys(ui.ctx()).esc {
                    self.front = Page::Main;
                }
                self.page(ui, bd, self.front);
            }
            _ => self.main_menu(ui, bd),
        }
    }

    fn main_menu(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let _ = bd;
        #[derive(Clone, Copy, PartialEq)]
        enum Item {
            Continue,
            New,
            Load,
            Settings,
            HowTo,
            Credits,
            Quit,
        }
        let latest = self.slots.latest().cloned();
        let mut items: Vec<(Item, String, Vec<String>)> = Vec::new();
        if let Some(m) = &latest {
            items.push((
                Item::Continue,
                "Continue".into(),
                vec![
                    format!("{}, {}", m.club, m.division),
                    format!("Season {}, {}, record {}", m.season, m.date, m.record),
                ],
            ));
        }
        items.push((Item::New, "New career".into(), vec![]));
        let n = self.slots.slots.len();
        if n > 0 {
            items.push((
                Item::Load,
                "Load career".into(),
                vec![format!("{n} saved career{}", if n == 1 { "" } else { "s" })],
            ));
        } else {
            items.push((Item::Load, "Load career".into(), vec![]));
        }
        items.push((Item::Settings, "Settings".into(), vec![]));
        items.push((Item::HowTo, "How to play".into(), vec![]));
        items.push((Item::Credits, "Credits".into(), vec![]));
        #[cfg(not(target_arch = "wasm32"))]
        items.push((Item::Quit, "Quit".into(), vec![]));

        let k = take_keys(ui.ctx());
        let count = items.len();
        self.menu.sel = self.menu.sel.min(count - 1);
        if k.down {
            self.menu.sel = (self.menu.sel + 1) % count;
        }
        if k.up {
            self.menu.sel = (self.menu.sel + count - 1) % count;
        }
        let mut chosen = k.enter.then(|| items[self.menu.sel].0);

        let screen = ui.max_rect();
        let side = theme::side();
        // The brand and the list: stacked, or side by side on a phone held sideways.
        let col_w = if side { 320.0 } else { 380.0_f32.min(screen.width() - 32.0) };
        let fade = if crate::settings::reduced() {
            1.0
        } else {
            ui.ctx().animate_value_with_time(egui::Id::unique("menu_fade"), 1.0, 0.35)
        };
        let brand = |ui: &mut Ui, big: bool| {
            ui.vertical_centered(|ui| {
                crate::title::ball(ui, if big { 92.0 } else { 52.0 });
                ui.add_space(if big { 12.0 } else { 6.0 });
                ui.label(
                    RichText::new("basketman")
                        .font(FontId::new(if big { 54.0 } else { 34.0 }, theme::mono_semibold()))
                        .color(theme::TEXT),
                );
                ui.label(RichText::new("Run the club. Coach the game.").size(if big { 17.0 } else { 14.0 }).color(theme::TEXT_DIM));
            });
        };
        let list = |this: &mut App, ui: &mut Ui, chosen: &mut Option<Item>| {
            ui.spacing_mut().item_spacing.y = if side { 5.0 } else { 8.0 };
            for (i, (item, title, lines)) in items.iter().enumerate() {
                let look = if i == 0 && *item == Item::Continue || (*item == Item::New && latest.is_none()) {
                    Look::Primary
                } else {
                    Look::Plain
                };
                let (r, _) = big_button(ui, col_w, title, lines, look, this.menu.sel == i);
                if r.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                    this.menu.sel = i;
                }
                if r.clicked() {
                    *chosen = Some(*item);
                }
            }
        };
        ui.set_opacity(fade);
        if side {
            // Brand on the left, list on the right.
            let left = Rect::from_min_max(screen.min, pos2(screen.center().x - 20.0, screen.max.y));
            let right = Rect::from_min_max(pos2(screen.center().x, screen.min.y), screen.max);
            ui.scope_builder(UiBuilder::new().max_rect(left), |ui| {
                ui.add_space((left.height() / 2.0 - 70.0).max(8.0));
                brand(ui, false);
            });
            ui.scope_builder(UiBuilder::new().max_rect(right.shrink2(vec2(0.0, 8.0))), |ui| {
                let total: f32 = items.iter().map(|(_, _, l)| if l.is_empty() { 36.0 } else { 30.0 + 15.0 * l.len() as f32 + 6.0 }).sum::<f32>()
                    + 5.0 * count as f32;
                ui.add_space(((right.height() - 16.0 - total) / 2.0).max(0.0));
                list(self, ui, &mut chosen);
            });
        } else {
            let est = 220.0 + items.iter().map(|(_, _, l)| if l.is_empty() { 54.0 } else { 40.0 + 17.0 * l.len() as f32 + 14.0 }).sum::<f32>();
            let top = ((screen.height() - est) * 0.45).clamp(10.0, 120.0);
            let col = Rect::from_center_size(
                pos2(screen.center().x, screen.min.y + screen.height() / 2.0),
                vec2(col_w, screen.height()),
            );
            ui.scope_builder(UiBuilder::new().max_rect(col), |ui| {
                ui.set_width(col_w);
                ui.add_space(top);
                brand(ui, true);
                ui.add_space(24.0);
                list(self, ui, &mut chosen);
                ui.add_space(14.0);
                ui.vertical_centered(|ui| {
                    ui.label(theme::mono(format!("v{}", env!("CARGO_PKG_VERSION"))).color(theme::TEXT_MUTE));
                });
            });
        }
        ui.set_opacity(1.0);
        if let Some(item) = chosen {
            match item {
                Item::Continue => self.request(Op::Continue),
                Item::New => self.open_new_career(),
                Item::Load => self.front = Page::Load,
                Item::Settings => self.front = Page::Settings,
                Item::HowTo => {
                    self.menu.howto = 0;
                    self.front = Page::HowTo;
                }
                Item::Credits => self.front = Page::Credits,
                Item::Quit => self.ops.push(Op::Quit),
            }
        }
    }

    // ---- pages that serve both the front and the pause menu --------------

    /// Draw one of the pages in a panel in the middle of the screen: over
    /// the photograph in the front, over the game when paused.
    fn page(&mut self, ui: &mut Ui, bd: &Backdrop, page: Page) {
        let (title, wide, back): (&str, f32, bool) = match page {
            Page::Load => ("Load career", 760.0, true),
            Page::Settings => ("Settings", 860.0, true),
            Page::HowTo => ("How to play", 700.0, true),
            Page::Credits => ("Credits", 600.0, true),
            Page::SaveAs => ("Save as new slot", 560.0, true),
            Page::ConfirmLeave => ("Leave the match?", 520.0, false),
            _ => ("Paused", 420.0, false),
        };
        let screen = ui.max_rect();
        let side = theme::side();
        let margin = if side { 8.0 } else { 20.0 };
        let w = wide.min(screen.width() - 2.0 * margin);
        let id = egui::Id::unique(("page_h", page as u8));
        let known: f32 = ui.ctx().data(|d| d.get_temp(id)).unwrap_or(300.0);
        let max_h = screen.height() - 2.0 * margin;
        let h = known.min(max_h);
        let top = (screen.top() + (screen.height() - h) * 0.5).max(screen.top() + margin);
        let rect = Rect::from_min_size(pos2(screen.center().x - w / 2.0, top), vec2(w, max_h));
        let overlay = self.started;
        let pad = if side { 12 } else { 18 };
        let mut go_back = false;
        let used = ui
            .scope_builder(UiBuilder::new().max_rect(rect), |ui| {
                let mut inner = |ui: &mut Ui| {
                    ui.set_width(w - 2.0 * pad as f32);
                    ui.spacing_mut().item_spacing.y = 6.0;
                    ui.horizontal(|ui| {
                        if back
                            && ui.add(egui::Button::new("< Back").min_size(vec2(80.0, 36.0))).clicked()
                        {
                            go_back = true;
                        }
                        ui.label(
                            RichText::new(title)
                                .font(FontId::new(if side { 18.0 } else { 22.0 }, theme::semibold()))
                                .color(theme::TEXT),
                        );
                    });
                    ui.add_space(4.0);
                    let body_max = max_h - 2.0 * pad as f32 - 56.0;
                    egui::ScrollArea::vertical()
                        .id_salt(("page_scroll", page as u8))
                        .max_height(body_max)
                        .auto_shrink([false, true])
                        .show(ui, |ui| match page {
                            Page::Load => self.page_load(ui),
                            Page::Settings => self.page_settings(ui),
                            Page::HowTo => self.page_howto(ui),
                            Page::Credits => page_credits(ui),
                            Page::SaveAs => self.page_save_as(ui),
                            Page::ConfirmLeave => self.page_leave(ui),
                            _ => {}
                        });
                };
                if overlay {
                    egui::Frame::NONE
                        .fill(theme::BG)
                        .stroke(Stroke::new(1.0, theme::LINE_2))
                        .corner_radius(12)
                        .shadow(egui::epaint::Shadow { offset: [0, 10], blur: 30, spread: 0, color: theme::ink(60) })
                        .inner_margin(egui::Margin::same(pad))
                        .show(ui, inner)
                        .response
                        .rect
                } else {
                    bd.card_padded(ui, pad, |ui| {
                        inner(ui);
                    });
                    ui.min_rect()
                }
            })
            .inner;
        ui.ctx().data_mut(|d| d.insert_temp(id, used.height()));
        if go_back {
            self.page_back(page);
        }
    }

    fn page_back(&mut self, _from: Page) {
        if self.started {
            self.pause = Some(Page::Pause);
        } else {
            self.front = Page::Main;
        }
    }

    fn page_load(&mut self, ui: &mut Ui) {
        let now = persist::now_secs();
        let persist_on = self.persisting();
        if !persist_on {
            ui.label(RichText::new("Saving is switched off in this run.").color(theme::WARN));
            return;
        }
        let used = self.slots.slots.len();
        ui.label(
            RichText::new(format!(
                "{used} of {} slots used. The game saves into the career you are playing.",
                persist::max_slots()
            ))
            .color(theme::TEXT_DIM),
        );
        if let Some(n) = &self.notice {
            ui.label(RichText::new(n).color(theme::WARN));
        }
        if self.slots.slots.is_empty() {
            ui.add_space(8.0);
            ui.label("No saved careers yet. Start a new career and it is saved here as you play.");
        }
        let slots: Vec<SlotMeta> = self.slots.slots.clone();
        let width = ui.available_width();
        let wide = width > 600.0;
        for s in slots {
            let current = self.slot_id == Some(s.id) && self.career;
            let frame = egui::Frame::NONE
                .inner_margin(egui::Margin::same(12))
                .corner_radius(10)
                .fill(theme::white(if current { 220 } else { 160 }))
                .stroke(Stroke::new(if current { 1.5 } else { 1.0 }, if current { theme::ACCENT } else { theme::LINE_2 }));
            frame.show(ui, |ui| {
                ui.set_width(width - 26.0);
                ui.spacing_mut().item_spacing.y = 3.0;
                let renaming = self.menu.renaming.as_ref().filter(|r| r.0 == s.id).map(|r| r.1.clone());
                if let Some(mut text) = renaming {
                    ui.horizontal(|ui| {
                        let r = ui.add(egui::TextEdit::singleline(&mut text).char_limit(40).desired_width(260.0));
                        r.request_focus();
                        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        if ui.add(egui::Button::new("Save name").min_size(vec2(100.0, 36.0))).clicked() || enter {
                            self.ops.push(Op::Rename(s.id, text.clone()));
                            self.menu.renaming = None;
                        } else if ui.add(egui::Button::new("Cancel").min_size(vec2(80.0, 36.0))).clicked() {
                            self.menu.renaming = None;
                        } else {
                            self.menu.renaming = Some((s.id, text));
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&s.name).font(FontId::new(17.0, theme::semibold())));
                        if current {
                            ui.label(theme::mono("playing now").color(theme::ACCENT));
                        }
                    });
                }
                if s.damaged {
                    ui.label(RichText::new(format!("Cannot be opened: {}", s.club)).color(theme::WARN));
                } else {
                    ui.label(RichText::new(s.line()).color(theme::TEXT));
                    ui.label(theme::mono(s.detail()).color(theme::TEXT_DIM));
                }
                let when = persist::ago(now, s.played);
                let who = if s.manager.is_empty() { String::new() } else { format!("Manager {}", s.manager) };
                let foot = [who, if when.is_empty() { when } else { format!("saved {when}") }]
                    .into_iter()
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                ui.label(theme::mono(foot).color(theme::TEXT_MUTE));
                ui.add_space(4.0);
                if self.menu.confirm_delete == Some(s.id) {
                    ui.label(
                        RichText::new(format!("Delete \"{}\"? This cannot be undone.", s.name))
                            .color(theme::WARN)
                            .family(theme::semibold()),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Delete").color(theme::BG)).fill(theme::WARN).min_size(vec2(90.0, 36.0)))
                            .clicked()
                        {
                            self.ops.push(Op::Delete(s.id));
                            self.menu.confirm_delete = None;
                        }
                        if ui.add(egui::Button::new("Keep it").min_size(vec2(90.0, 36.0))).clicked() {
                            self.menu.confirm_delete = None;
                        }
                    });
                } else {
                    let _ = wide;
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if !s.damaged
                            && ui
                                .add(
                                    egui::Button::new(RichText::new("Load").family(theme::semibold()).color(theme::BG))
                                        .fill(theme::TEXT)
                                        .min_size(vec2(84.0, 36.0)),
                                )
                                .clicked()
                        {
                            self.request(Op::Load(s.id));
                        }
                        if ui.add(egui::Button::new("Rename").min_size(vec2(84.0, 36.0))).clicked() {
                            self.menu.renaming = Some((s.id, s.name.clone()));
                        }
                        if !s.damaged && ui.add(egui::Button::new("Export").min_size(vec2(84.0, 36.0))).clicked() {
                            self.ops.push(Op::Export(s.id));
                        }
                        if ui.add(egui::Button::new("Delete").min_size(vec2(84.0, 36.0))).clicked() {
                            self.menu.confirm_delete = Some(s.id);
                        }
                    });
                }
            });
            ui.add_space(4.0);
        }
        self.page_import(ui);
    }

    /// Careers brought in from a file.
    fn page_import(&mut self, ui: &mut Ui) {
        ui.add_space(6.0);
        theme::eyebrow(ui, "Import a career");
        #[cfg(not(target_arch = "wasm32"))]
        {
            let dir = export_dir();
            if !self.menu.files_read {
                self.menu.files = std::fs::read_dir(&dir)
                    .map(|d| {
                        let mut v: Vec<_> = d
                            .filter_map(|e| e.ok().map(|e| e.path()))
                            .filter(|p| p.extension().is_some_and(|x| x == "bmsave"))
                            .collect();
                        v.sort();
                        v
                    })
                    .unwrap_or_default();
                self.menu.files_read = true;
            }
            ui.label(
                RichText::new(format!(
                    "Exported careers are written to {}. Put a file there to import it.",
                    dir.display()
                ))
                .size(12.5)
                .color(theme::TEXT_DIM),
            );
            let mut pick = None;
            for f in &self.menu.files {
                let name = f.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
                if ui.add(egui::Button::new(format!("Import {name}")).min_size(vec2(0.0, 36.0))).clicked() {
                    pick = Some(f.clone());
                }
            }
            if self.menu.files.is_empty() {
                ui.label(theme::mono("No files there yet.").color(theme::TEXT_MUTE));
            }
            if ui.add(egui::Button::new("Look again").min_size(vec2(110.0, 36.0))).clicked() {
                self.menu.files_read = false;
            }
            if let Some(f) = pick {
                match std::fs::read(&f) {
                    Ok(bytes) => self.ops.push(Op::Import(bytes)),
                    Err(e) => self.notice = Some(format!("Could not read the file: {e}")),
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            ui.label(
                RichText::new("Choose a file made by Export. It is added as a new slot.")
                    .size(12.5)
                    .color(theme::TEXT_DIM),
            );
            if ui.add(egui::Button::new("Choose a file...").min_size(vec2(150.0, 36.0))).clicked() {
                crate::webfile::pick();
            }
        }
    }

    fn page_save_as(&mut self, ui: &mut Ui) {
        if !self.persisting() {
            ui.label(RichText::new("Saving is switched off in this run.").color(theme::WARN));
            return;
        }
        if self.slots.full() {
            ui.label(
                RichText::new(format!(
                    "All {} slots are in use. Delete one in Load career to make room.",
                    persist::max_slots()
                ))
                .color(theme::WARN),
            );
            if ui.add(egui::Button::new("Open Load career").min_size(vec2(160.0, 36.0))).clicked() {
                self.pause = Some(Page::Load);
            }
            return;
        }
        ui.label(RichText::new("Keeps this moment as a slot of its own. The game then saves into the new one.").color(theme::TEXT_DIM));
        let mut go = false;
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.menu.save_name).char_limit(40).desired_width(280.0).margin(egui::Margin::symmetric(8, 7)));
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                go = true;
            }
            if ui
                .add(egui::Button::new(RichText::new("Save").family(theme::semibold()).color(theme::BG)).fill(theme::TEXT).min_size(vec2(90.0, 36.0)))
                .clicked()
            {
                go = true;
            }
        });
        if go && !self.menu.save_name.trim().is_empty() {
            self.ops.push(Op::SaveNew(self.menu.save_name.trim().to_string()));
            self.pause = Some(Page::Pause);
        }
    }

    fn page_leave(&mut self, ui: &mut Ui) {
        ui.label(
            "A match is being played. It cannot be saved. If you leave, you go back to your last save, \
             from before this game.",
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new(RichText::new("Leave the match").color(theme::BG)).fill(theme::WARN).min_size(vec2(150.0, 40.0)))
                .clicked()
                && let Some(op) = self.leaving.take()
            {
                self.live = None;
                // The world has started that round; the save has not.
                self.career = false;
                self.pause = None;
                self.ops.push(op);
            }
            if ui.add(egui::Button::new("Stay").min_size(vec2(100.0, 40.0))).clicked() {
                self.leaving = None;
                self.pause = Some(Page::Pause);
            }
        });
    }

    fn page_settings(&mut self, ui: &mut Ui) {
        // Two columns when there is room, so that all of it is in view.
        if ui.available_width() > 640.0 {
            ui.columns(2, |cols| {
                self.settings_match(&mut cols[0]);
                self.settings_screen(&mut cols[1]);
            });
        } else {
            self.settings_match(ui);
            ui.add_space(6.0);
            self.settings_screen(ui);
        }
    }

    fn settings_match(&mut self, ui: &mut Ui) {
        theme::eyebrow(ui, "The match");
        let mut mode = if self.look.mode == ViewMode::Angled { ViewMode::Three } else { self.look.mode };
        setting_row(ui, "Match view", "How a game is shown. You can also change it during a match.", &mut |ui| {
            segmented(
                ui,
                &[(ViewMode::Three, "3D".to_string()), (ViewMode::Flat, "2D".to_string()), (ViewMode::Commentary, "Commentary".to_string())],
                &mut mode,
            );
        });
        self.look.mode = mode;
        let prefs = &mut self.sh.hud.prefs;
        setting_row(ui, "Match screen panels", "Full shows everything. Clean shows only the court.", &mut |ui| {
            segmented(
                ui,
                &[(0u8, "Full".to_string()), (1, "Minimal".to_string()), (2, "Clean".to_string())],
                &mut prefs.level,
            );
        });
        setting_row(ui, "Panel opacity", "How solid the panels over the court are.", &mut |ui| {
            ui.spacing_mut().slider_width = 180.0;
            ui.spacing_mut().interact_size.y = 36.0;
            ui.add(
                egui::Slider::new(&mut prefs.alpha, 0.35..=1.0)
                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                    .custom_parser(|s| s.trim_end_matches('%').parse::<f64>().ok().map(|v| v / 100.0)),
            );
        });
        let s = &mut self.settings;
        setting_row(ui, "Game speed", "The speed a match starts at.", &mut |ui| {
            segmented(ui, &SPEEDS.map(|v| (v, format!("{v:.0}x"))), &mut s.speed);
        });
        setting_row(ui, "Ask before leaving a match", "Leaving a match in progress gives it up; it cannot be saved.", &mut |ui| {
            segmented(ui, &[(false, "Off".to_string()), (true, "On".to_string())], &mut s.confirm_leave);
        });
    }

    fn settings_screen(&mut self, ui: &mut Ui) {
        let s = &mut self.settings;
        theme::eyebrow(ui, "The screen");
        setting_row(ui, "Interface size", "Makes everything larger or smaller.", &mut |ui| {
            segmented(ui, &SCALES.map(|v| (v, format!("{v}%"))), &mut s.scale);
        });
        setting_row(ui, "Reduced motion", "No fades, and nothing that moves by itself.", &mut |ui| {
            segmented(ui, &[(false, "Off".to_string()), (true, "On".to_string())], &mut s.reduced_motion);
        });
        #[cfg(not(target_arch = "wasm32"))]
        setting_row(ui, "Full screen", "F11 does the same.", &mut |ui| {
            segmented(ui, &[(false, "Off".to_string()), (true, "On".to_string())], &mut s.fullscreen);
        });
        ui.add_space(6.0);
        theme::eyebrow(ui, "Saving");
        setting_row(
            ui,
            "Auto-save",
            "Off: the game saves only when you press Save or leave to the main menu.",
            &mut |ui| {
                segmented(ui, &Autosave::ALL.map(|a| (a, a.label().to_string())), &mut s.autosave);
            },
        );
    }

    fn page_howto(&mut self, ui: &mut Ui) {
        let n = HOWTO.len();
        let page = self.menu.howto.min(n - 1);
        let (title, lines) = HOWTO[page];
        theme::eyebrow(ui, &format!("{} of {n}", page + 1));
        ui.label(RichText::new(title).font(FontId::new(20.0, theme::semibold())));
        ui.add_space(4.0);
        ui.scope(|ui| {
            ui.set_min_height(if theme::side() { 120.0 } else { 190.0 });
            for l in lines {
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new("-").color(theme::ACCENT));
                    ui.add(egui::Label::new(RichText::new(*l).size(theme::fs(14.5))).wrap());
                });
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(page > 0, egui::Button::new("< Earlier").min_size(vec2(96.0, 38.0))).clicked() {
                self.menu.howto = page - 1;
            }
            let (rect, _) = ui.allocate_exact_size(vec2(n as f32 * 16.0, 38.0), Sense::hover());
            for i in 0..n {
                ui.painter().circle_filled(
                    pos2(rect.left() + 8.0 + i as f32 * 16.0, rect.center().y),
                    if i == page { 4.5 } else { 3.0 },
                    if i == page { theme::ACCENT } else { theme::LINE_2 },
                );
            }
            if ui.add_enabled(page + 1 < n, egui::Button::new("Next >").min_size(vec2(96.0, 38.0))).clicked() {
                self.menu.howto = page + 1;
            }
        });
        // Left and right turn the pages when no field has the keyboard.
        let (l, r) = ui.ctx().input_mut(|i| {
            (i.consume_key(Modifiers::NONE, Key::ArrowLeft), i.consume_key(Modifiers::NONE, Key::ArrowRight))
        });
        if l && page > 0 {
            self.menu.howto = page - 1;
        }
        if r && page + 1 < n {
            self.menu.howto = page + 1;
        }
    }

    // ---- the menu over the game ---------------------------------------

    /// Before the game's own keys: while the menu is up it has the keyboard.
    pub(crate) fn pause_keys(&mut self, ctx: &egui::Context) -> Keys {
        let Some(page) = self.pause else { return Keys::default() };
        let mut k = Keys::default();
        if page == Page::Pause {
            k = take_keys(ctx);
        } else {
            let typing = ctx.egui_wants_keyboard_input();
            k.esc = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
            let _ = typing;
        }
        // Nothing else of the keyboard reaches the game beneath.
        if !ctx.egui_wants_keyboard_input() {
            ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Key { .. } | egui::Event::Text(_))));
        }
        k
    }

    pub(crate) fn pause_overlay(&mut self, ui: &mut Ui, bd: &Backdrop, keys: Keys) {
        let Some(page) = self.pause else { return };
        let ctx = ui.ctx().clone();
        let screen = ctx.content_rect();
        let side = tier_for(screen.size()) == Tier::Side;
        theme::set_side(side);
        if keys.esc {
            match page {
                Page::Pause => self.pause = None,
                Page::ConfirmLeave => {
                    self.leaving = None;
                    self.pause = Some(Page::Pause);
                }
                _ => self.pause = Some(Page::Pause),
            }
            if self.pause.is_none() {
                return;
            }
        }
        let page = self.pause.unwrap_or(Page::Pause);
        egui::Area::new(egui::Id::unique("pause_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(&ctx, |ui| {
                ui.set_clip_rect(screen);
                // The scrim takes every click that would reach the game.
                let r = ui.allocate_rect(screen, Sense::click_and_drag());
                ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha(120));
                if r.clicked() && page == Page::Pause {
                    self.pause = None;
                }
                let mut sub = ui.new_child(UiBuilder::new().max_rect(screen));
                match page {
                    Page::Pause => self.pause_menu(&mut sub, keys, screen),
                    p => self.page(&mut sub, bd, p),
                }
            });
    }

    fn pause_menu(&mut self, ui: &mut Ui, keys: Keys, screen: Rect) {
        let side = theme::side();
        let w = if side { 340.0 } else { 380.0_f32.min(screen.width() - 24.0) };
        let items: Vec<(&str, &str)> = vec![
            ("Resume", "resume"),
            ("Save", "save"),
            ("Save as new slot", "saveas"),
            ("Load career", "load"),
            ("Settings", "settings"),
            ("How to play", "howto"),
            ("Exit to main menu", "exit"),
        ];
        let count = items.len();
        self.menu.sel = self.menu.sel.min(count - 1);
        if keys.down {
            self.menu.sel = (self.menu.sel + 1) % count;
        }
        if keys.up {
            self.menu.sel = (self.menu.sel + count - 1) % count;
        }
        let mut chosen = keys.enter.then(|| items[self.menu.sel].1);
        let row_h = if side { 36.0 } else { 46.0 };
        let gap = if side { 4.0 } else { 7.0 };
        let note = self.pause_note();
        let total = 40.0 + count as f32 * (row_h + gap) + if note.is_some() { 24.0 } else { 0.0 } + 36.0;
        let top = ((screen.height() - total) / 2.0).max(6.0);
        let rect = Rect::from_min_size(pos2(screen.center().x - w / 2.0, screen.top() + top), vec2(w, screen.height()));
        ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
            egui::Frame::NONE
                .fill(theme::BG)
                .stroke(Stroke::new(1.0, theme::LINE_2))
                .corner_radius(12)
                .shadow(egui::epaint::Shadow { offset: [0, 10], blur: 30, spread: 0, color: theme::ink(60) })
                .inner_margin(egui::Margin::same(if side { 12 } else { 16 }))
                .show(ui, |ui| {
                    ui.set_width(w - 32.0);
                    ui.spacing_mut().item_spacing.y = gap;
                    ui.label(RichText::new("Paused").font(FontId::new(20.0, theme::semibold())));
                    for (i, (label, key)) in items.iter().enumerate() {
                        let look = if i == 0 { Look::Primary } else { Look::Plain };
                        let (r, _) = big_button(ui, w - 32.0, label, &[], look, self.menu.sel == i);
                        if r.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                            self.menu.sel = i;
                        }
                        if r.clicked() {
                            chosen = Some(key);
                        }
                    }
                    if let Some(n) = &note {
                        ui.label(RichText::new(n).size(12.5).color(theme::TEXT_DIM));
                    }
                });
        });
        if let Some(c) = chosen {
            match c {
                "resume" => self.pause = None,
                "save" => {
                    self.ops.push(Op::Save);
                }
                "saveas" => {
                    if self.menu.save_name.is_empty() {
                        self.menu.save_name = format!("{}, season {}", self.world.teams[self.team].name, self.world.season + 1);
                    }
                    self.pause = Some(Page::SaveAs);
                }
                "load" => self.pause = Some(Page::Load),
                "settings" => self.pause = Some(Page::Settings),
                "howto" => {
                    self.menu.howto = 0;
                    self.pause = Some(Page::HowTo);
                }
                _ => self.request(Op::ExitToMenu),
            }
        }
    }

    fn pause_note(&self) -> Option<String> {
        if let Some(n) = &self.notice {
            return Some(n.clone());
        }
        if self.live.is_some() {
            return Some("A match is in progress. It cannot be saved.".into());
        }
        None
    }

    // ---- storage work, at the end of the frame ---------------------------

    pub(crate) fn run_ops(&mut self, ctx: &egui::Context, mut storage: Option<&mut (dyn eframe::Storage + '_)>) {
        if self.slots.migrating
            && let Some(st) = storage.as_mut()
        {
            self.slots.commit_migration(&mut **st);
        }
        while !self.ops.is_empty() {
            let op = self.ops.remove(0);
            match storage.as_mut() {
                Some(st) => self.run_op(ctx, op, Some(&mut **st)),
                None => self.run_op(ctx, op, None),
            }
        }
    }

    fn run_op(&mut self, ctx: &egui::Context, op: Op, storage: Option<&mut (dyn eframe::Storage + '_)>) {
        self.notice = None;
        let persist_on = self.persist;
        match op {
            Op::Quit => {
                #[cfg(not(target_arch = "wasm32"))]
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                let _ = ctx;
            }
            Op::Start => {
                let nc = std::mem::replace(&mut self.nc, crate::newcareer::NewCareer::new(1));
                let name = nc.name.trim().to_string();
                let replace = nc.replace;
                let Some((world, team)) = nc.finish() else { return };
                let mut slot = None;
                if let (true, Some(st)) = (persist_on, storage) {
                    if self.slots.full()
                        && let Some(r) = replace
                    {
                        self.slots.delete(st, r);
                    }
                    match self.slots.create(st, &world, &name, None) {
                        Ok(id) => slot = Some(id),
                        Err(e) => self.notice = Some(e),
                    }
                }
                self.begin_world(world, team, name, slot);
            }
            Op::Continue => {
                let Some(id) = self.slots.latest().map(|m| m.id) else { return };
                if self.career && self.slot_id == Some(id) {
                    self.started = true;
                    self.pause = None;
                } else {
                    self.run_op(ctx, Op::Load(id), storage);
                }
            }
            Op::Load(id) => {
                let Some(st) = storage else {
                    self.notice = Some("Saving is switched off in this run.".into());
                    return;
                };
                // What is being played is kept first.
                self.save_current(st);
                match self.slots.load(st, id) {
                    Loaded::Career(w) => {
                        let manager = self.slots.get(id).map(|m| m.manager.clone()).unwrap_or_default();
                        self.enter_world(*w, id, manager);
                    }
                    Loaded::Unreadable(why) => {
                        if let Some(m) = self.slots.slots.iter_mut().find(|m| m.id == id) {
                            m.damaged = true;
                            m.club = why.clone();
                        }
                        self.notice = Some(format!("That career cannot be opened: {why}"));
                    }
                }
            }
            Op::Save => {
                let Some(st) = storage else {
                    self.notice = Some("Saving is switched off in this run.".into());
                    return;
                };
                if self.live.is_some() {
                    self.notice = Some("A match is in progress. It cannot be saved; the career was saved before it.".into());
                } else if self.slot_id.is_some() {
                    self.save_current(st);
                    self.notice = Some("Saved.".into());
                } else {
                    let name = self.world.teams[self.team].name.clone();
                    self.run_op(ctx, Op::SaveNew(name), Some(st));
                }
            }
            Op::SaveNew(name) => {
                let Some(st) = storage else {
                    self.notice = Some("Saving is switched off in this run.".into());
                    return;
                };
                if self.live.is_some() {
                    self.notice = Some("A match is in progress. It cannot be saved.".into());
                    return;
                }
                match self.slots.create(st, &self.world, &self.manager, Some(&name)) {
                    Ok(id) => {
                        self.slot_id = Some(id);
                        self.saved_at = Some(self.save_mark());
                        self.notice = Some(format!("Saved as \"{name}\"."));
                    }
                    Err(e) => self.notice = Some(e),
                }
            }
            Op::Delete(id) => {
                if let Some(st) = storage {
                    self.slots.delete(st, id);
                }
                if self.slot_id == Some(id) {
                    self.slot_id = None;
                    if !self.started {
                        self.career = false;
                    }
                }
            }
            Op::Rename(id, name) => {
                if let Some(st) = storage {
                    self.slots.rename(st, id, &name);
                }
            }
            Op::ExitToMenu => {
                if let Some(st) = storage {
                    if self.career && self.live.is_none() {
                        self.save_current(st);
                    }
                }
                self.live = None;
                self.started = false;
                self.pause = None;
                self.front = Page::Main;
                self.menu.sel = 0;
                match self.menu.want.take() {
                    Some(Page::New) => self.open_new_career(),
                    Some(p) => self.front = p,
                    None => {}
                }
            }
            Op::Export(id) => {
                let Some(st) = storage else { return };
                match self.slots.export(st, id) {
                    Ok((file, bytes)) => self.deliver_export(&file, bytes),
                    Err(e) => self.notice = Some(e),
                }
            }
            Op::Import(bytes) => {
                let Some(st) = storage else { return };
                match self.slots.import(st, &bytes) {
                    Ok(_) => self.notice = Some("Career imported.".into()),
                    Err(e) => self.notice = Some(format!("Could not import: {e}")),
                }
            }
            Op::Stash(world, manager) => {
                if let Some(st) = storage {
                    let _ = self.slots.create(st, &world, &manager, None);
                }
            }
        }
    }

    /// Write a career file where the player can find it, or hand it to the browser.
    fn deliver_export(&mut self, file: &str, bytes: Vec<u8>) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let dir = export_dir();
            let path = dir.join(file);
            self.notice = Some(match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, bytes)) {
                Ok(()) => format!("Written to {}", path.display()),
                Err(e) => format!("Could not write the file: {e}"),
            });
            self.menu.files_read = false;
        }
        #[cfg(target_arch = "wasm32")]
        {
            crate::webfile::download(file, &bytes);
            self.notice = Some(format!("Downloading {file}"));
        }
    }

    /// Whether the calendar has moved far enough for the autosave setting.
    pub(crate) fn autosave_due(&self) -> bool {
        match self.settings.autosave {
            Autosave::Off => false,
            Autosave::Day => self.saved_at != Some(self.save_mark()),
            Autosave::Round => self.saved_at.map(|m| (m.0, m.1)) != Some((self.world.season, self.world.round)),
        }
    }

    /// Put the career into its slot.
    pub(crate) fn save_current(&mut self, storage: &mut dyn eframe::Storage) {
        if !self.may_save() {
            return;
        }
        if let Some(id) = self.slot_id {
            match self.slots.save(storage, id, &self.world, &self.manager) {
                Ok(()) => self.saved_at = Some(self.save_mark()),
                Err(e) => {
                    // Do not try again every frame.
                    self.saved_at = Some(self.save_mark());
                    self.notice = Some(e);
                }
            }
        }
    }

    /// Put the settings into effect: scale, motion, full screen.
    pub(crate) fn apply_settings(&mut self, ctx: &egui::Context) {
        let s = self.settings;
        crate::settings::set_reduced(s.reduced_motion);
        if (ctx.zoom_factor() - s.zoom()).abs() > 1e-3 {
            ctx.set_zoom_factor(s.zoom());
        }
        if self.applied_motion != Some(s.reduced_motion) {
            self.applied_motion = Some(s.reduced_motion);
            ctx.all_styles_mut(|st| st.animation_time = if s.reduced_motion { 0.0 } else { 1.0 / 12.0 });
        }
        #[cfg(not(target_arch = "wasm32"))]
        if !self.harness() && self.applied_full != Some(s.fullscreen) {
            // The first frame only follows the setting; F11 can change it after.
            if self.applied_full.is_some() || s.fullscreen {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(s.fullscreen));
            }
            self.applied_full = Some(s.fullscreen);
        }
    }

    /// A fresh playback at the speed the settings ask for.
    pub(crate) fn fresh_playback(&self) -> crate::matchview::Playback {
        crate::matchview::Playback {
            speed: self.settings.speed,
            ..crate::matchview::Playback::start()
        }
    }

    /// The tab a loaded career opens on.
    pub(crate) fn home_tab(&self) -> Tab {
        Tab::Inbox
    }
}

fn page_credits(ui: &mut Ui) {
    ui.label(RichText::new("basketman").font(FontId::new(20.0, theme::mono_semibold())));
    ui.label(RichText::new("A basketball manager game. Run the club, coach the game.").color(theme::TEXT_DIM));
    ui.add_space(6.0);
    for (k, v) in [
        ("Game", "Made by Rotko Networks. Written in Rust."),
        ("Interface", "egui and eframe, running on the desktop and in the browser."),
        ("Type", "Inter by Rasmus Andersson and JetBrains Mono, under the SIL Open Font License 1.1."),
        (
            "Photograph",
            "Wilt Chamberlain, 1967, from Wikimedia Commons, public domain. Used as a faint ink mask.",
        ),
        ("Names", "All clubs, players, owners and staff are made up."),
    ] {
        ui.horizontal_top(|ui| {
            key_label(ui, 90.0, k);
            ui.add(egui::Label::new(RichText::new(v).size(theme::fs(14.0))).wrap());
        });
    }
    ui.add_space(6.0);
    ui.label(theme::mono(format!("version {}", env!("CARGO_PKG_VERSION"))).color(theme::TEXT_MUTE));
}

/// Where careers are exported to on the desktop.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn export_dir() -> std::path::PathBuf {
    eframe::storage_dir("Basketman")
        .map(|d| d.join("exports"))
        .unwrap_or_else(|| std::path::PathBuf::from("basketman-exports"))
}

impl App {
    /// The screens drawn over the game when the pause menu is not the
    /// face of it: nothing. Kept so the shell has one call.
    pub(crate) fn menu_open(&self) -> bool {
        self.pause.is_some()
    }
}
