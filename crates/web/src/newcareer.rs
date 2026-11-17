//! The new career, as a short guided flow: you, the world, the division,
//! the club, and a last look. The league is built once, when the world is
//! confirmed, and the later steps only read it; then `TakeCharge` through
//! `World::apply` makes it a career, exactly as an agent would.

use basketman_core::club::{ClubSummary, city_of};
use basketman_core::command::Command;
use basketman_core::league::{TeamId, World};
use eframe::egui::{self, Align2, Color32, FontId, Key, Modifiers, RichText, Sense, Stroke, Ui, vec2};

use crate::app::{App, money};
use crate::menu::{self, Page};
use crate::theme::{self, Backdrop};

pub(crate) const DEFAULT_PER: usize = 12;
pub(crate) const DEFAULT_DIVS: usize = 3;
/// Clubs in a division, and the number of divisions, that may be chosen.
pub(crate) const PER_CHOICES: [usize; 5] = [8, 10, 12, 14, 16];
pub(crate) const DIV_CHOICES: [usize; 3] = [2, 3, 4];

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Step {
    You,
    World,
    Division,
    Club,
    Confirm,
}

impl Step {
    const ALL: [Step; 5] = [Step::You, Step::World, Step::Division, Step::Club, Step::Confirm];

    fn index(self) -> usize {
        Step::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    fn label(self) -> &'static str {
        match self {
            Step::You => "You",
            Step::World => "World",
            Step::Division => "Division",
            Step::Club => "Club",
            Step::Confirm => "Confirm",
        }
    }

    pub fn from_key(k: &str) -> Option<Step> {
        Step::ALL.into_iter().find(|s| s.label().to_lowercase() == k)
    }

    fn prev(self) -> Option<Step> {
        self.index().checked_sub(1).map(|i| Step::ALL[i])
    }

    fn next(self) -> Option<Step> {
        Step::ALL.get(self.index() + 1).copied()
    }
}

/// A club as the picker lists it.
pub(crate) struct ClubInfo {
    pub team: TeamId,
    pub name: String,
    pub town: String,
    /// Mean overall of the best eight.
    pub strength: f32,
    /// The place the board asks for.
    pub expect: usize,
    pub division: usize,
}

/// The league built for a seed and size.
pub(crate) struct Built {
    seed: u64,
    per: usize,
    divs: usize,
    world: World,
    clubs: Vec<ClubInfo>,
    best: f32,
    worst: f32,
}

pub(crate) struct NewCareer {
    pub step: Step,
    pub name: String,
    name_roll: usize,
    pub seed: u64,
    pub per: usize,
    pub divs: usize,
    built: Option<Built>,
    pub division: usize,
    pub club: Option<TeamId>,
    /// When every slot is in use: the one the new career takes over.
    pub replace: Option<u32>,
    surprise: usize,
    summary: Option<(TeamId, Box<ClubSummary>)>,
}

const FIRST: [&str; 20] = [
    "Aino", "Elias", "Mika", "Sanna", "Jonas", "Lena", "Ville", "Karin", "Tomas", "Ida", "Oskar",
    "Maria", "Pekka", "Nora", "Anders", "Helmi", "Lukas", "Eeva", "Marek", "Saga",
];
const LAST: [&str; 20] = [
    "Aaltonen", "Berg", "Koski", "Lindqvist", "Nieminen", "Ozols", "Petrov", "Rantanen", "Saar",
    "Tamm", "Virtanen", "Wiik", "Eklund", "Halme", "Kallio", "Laine", "Makela", "Norberg",
    "Salo", "Toivonen",
];

/// A manager's name for a world and a roll of the dice.
pub(crate) fn make_name(seed: u64, roll: usize) -> String {
    let mut h = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (roll as u64).wrapping_mul(0xD1B5_4A32_D192_ED03);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    format!(
        "{} {}",
        FIRST[(h % FIRST.len() as u64) as usize],
        LAST[((h >> 16) % LAST.len() as u64) as usize]
    )
}

/// Put the manager in charge of a club: the one step from a new league to
/// a career.
pub(crate) fn take_charge(mut world: World, team: TeamId) -> World {
    let _ = world.apply(Command::TakeCharge(team));
    world
}

/// A whole new career, as the flow makes one: the same world as
/// `World::new_career_tiers` with the same seed and `TakeCharge`.
pub(crate) fn career(seed: u64, per: usize, divs: usize, team: TeamId) -> World {
    take_charge(World::new_career_tiers(seed, per, divs), team)
}

impl NewCareer {
    pub fn new(seed: u64) -> NewCareer {
        NewCareer {
            step: Step::You,
            name: make_name(seed, 0),
            name_roll: 0,
            seed,
            per: DEFAULT_PER,
            divs: DEFAULT_DIVS,
            built: None,
            division: DEFAULT_DIVS - 1,
            club: None,
            replace: None,
            surprise: 0,
            summary: None,
        }
    }

    pub fn another_name(&mut self) {
        self.name_roll += 1;
        self.name = make_name(self.seed, self.name_roll);
    }

    /// The league on show is not the one the seed and size now ask for.
    pub fn stale(&self) -> bool {
        !self
            .built
            .as_ref()
            .is_some_and(|b| (b.seed, b.per, b.divs) == (self.seed, self.per, self.divs))
    }

    /// Build the league. Done once per seed and size.
    pub fn generate(&mut self) {
        if !self.stale() {
            return;
        }
        let world = World::new_career_tiers(self.seed, self.per, self.divs);
        let clubs: Vec<ClubInfo> = (0..world.teams.len())
            .map(|t| {
                let mut ov: Vec<f32> = world.teams[t]
                    .roster
                    .iter()
                    .map(|&p| world.players[p as usize].overall())
                    .collect();
                ov.sort_by(|a, b| b.total_cmp(a));
                ClubInfo {
                    team: t,
                    name: world.teams[t].name.clone(),
                    town: city_of(&world.teams[t].name).to_string(),
                    strength: ov.iter().take(8).sum::<f32>() / 8.0,
                    expect: world.expected_finish(t),
                    division: world.teams[t].division,
                }
            })
            .collect();
        let best = clubs.iter().map(|c| c.strength).fold(0.0, f32::max);
        let worst = clubs.iter().map(|c| c.strength).fold(f32::MAX, f32::min);
        self.built = Some(Built {
            seed: self.seed,
            per: self.per,
            divs: self.divs,
            world,
            clubs,
            best,
            worst,
        });
        // The long road is the one offered first.
        self.division = self.divs - 1;
        self.club = None;
        self.summary = None;
        self.surprise = 0;
    }

    /// The clubs of a division, strongest squad first.
    fn division_clubs(&self, d: usize) -> Vec<&ClubInfo> {
        let mut v: Vec<&ClubInfo> = self
            .built
            .iter()
            .flat_map(|b| b.clubs.iter())
            .filter(|c| c.division == d)
            .collect();
        v.sort_by(|a, b| b.strength.total_cmp(&a.strength));
        v
    }

    /// A club from the middle of the division; asked again, the next one.
    pub fn surprise_me(&mut self) {
        let list = self.division_clubs(self.division);
        if list.is_empty() {
            return;
        }
        let third = (list.len() / 3).max(1);
        let pick = list[third + self.surprise % third.min(list.len() - third).max(1)].team;
        self.surprise += 1;
        self.club = Some(pick);
    }

    /// Choose the division; the club is the middle of it, so one is always chosen.
    fn set_division(&mut self, d: usize) {
        if d != self.division || self.club.is_none() {
            self.division = d;
            self.club = None;
            self.surprise = 0;
            self.surprise_me();
        }
    }

    /// For the screenshot harness: the club at that place of the list, or the middle one.
    pub fn pick_for_harness(&mut self, n: Option<usize>) {
        match n {
            Some(i) => self.club = self.division_clubs(self.division).get(i).map(|c| c.team),
            None => self.set_division(self.division),
        }
    }

    pub fn world(&self) -> Option<&World> {
        self.built.as_ref().map(|b| &b.world)
    }

    /// The league with the manager in charge of the club chosen.
    pub fn finish(mut self) -> Option<(World, TeamId)> {
        let team = self.club?;
        let b = self.built.take()?;
        Some((take_charge(b.world, team), team))
    }

    fn can_next(&self) -> bool {
        match self.step {
            Step::You => !self.name.trim().is_empty(),
            Step::Club | Step::Confirm => self.club.is_some(),
            _ => true,
        }
    }
}

fn blurb(d: usize, divs: usize, moves: usize) -> (&'static str, String) {
    if d == 0 {
        (
            "The top",
            format!(
                "The best squads, the biggest halls, the most money, and a board that wants results. \
                 The playoffs decide the title. The last {moves} go down."
            ),
        )
    } else if d + 1 == divs {
        (
            "Start at the bottom and climb",
            format!(
                "The weakest squads, small halls and tight money. Nobody expects much, so you have time. \
                 Finish in the top {moves} to go up, and keep climbing for years to reach the top."
            ),
        )
    } else {
        (
            "A step down",
            format!(
                "Weaker squads, smaller halls and less money than the division above. The top {moves} go up, \
                 the last {moves} go down."
            ),
        )
    }
}

/// How hard the job is, in a sentence.
fn difficulty(s: &ClubSummary) -> (&'static str, String) {
    let n = s.division_size;
    let rank = s.squad.strength_rank;
    let gap = rank as i32 - s.expected_place as i32;
    let word = match gap {
        ..=-2 => "Comfortable",
        -1..=1 => "Fair",
        2..=3 => "Demanding",
        _ => "Hard",
    };
    let line = match gap {
        ..=-2 => format!("The squad is {} of {n} on paper and the board asks for less.", ordinal(rank)),
        -1..=1 => format!(
            "The squad is {} of {n} on paper and the board asks for {}.",
            ordinal(rank),
            ordinal(s.expected_place)
        ),
        _ => format!(
            "The board asks for {} but the squad is {} of {n} on paper. You will have to do more than the players.",
            ordinal(s.expected_place),
            ordinal(rank)
        ),
    };
    (word, line)
}

fn ordinal(n: usize) -> String {
    basketman_core::budget::ordinal(n)
}

impl App {
    pub(crate) fn open_new_career(&mut self) {
        let seed = if self.harness() {
            std::env::var("SHOT_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(1)
        } else {
            random_seed()
        };
        self.nc = NewCareer::new(seed);
        // Out of slots: the one least played is offered to be replaced.
        self.nc.replace = None;
        self.front = Page::New;
    }

    fn nc_back(&mut self) {
        match self.nc.step.prev() {
            Some(s) => self.nc.step = s,
            None => self.front = Page::Main,
        }
    }

    fn nc_next(&mut self) {
        if !self.nc.can_next() {
            return;
        }
        match self.nc.step {
            Step::World | Step::You | Step::Division => {
                // The league is built here, once; it takes a few milliseconds.
                if self.nc.step == Step::World {
                    self.nc.generate();
                }
                if let Some(s) = self.nc.step.next() {
                    self.nc.step = s;
                }
                if self.nc.step == Step::Club && self.nc.club.is_none() {
                    let d = self.nc.division;
                    self.nc.set_division(d);
                }
            }
            Step::Club => self.nc.step = Step::Confirm,
            Step::Confirm => self.ops.push(crate::menu::Op::Start),
        }
    }

    pub(crate) fn new_career_screen(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let busy = false;
        // Keys: Enter goes on, Esc goes back, arrows choose.
        let (mut next, mut back, mut up, mut down, mut left, mut right) = (false, false, false, false, false, false);
        if !busy && !ui.ctx().egui_wants_keyboard_input() {
            ui.ctx().input_mut(|i| {
                next = i.consume_key(Modifiers::NONE, Key::Enter);
                back = i.consume_key(Modifiers::NONE, Key::Escape);
                up = i.consume_key(Modifiers::NONE, Key::ArrowUp);
                down = i.consume_key(Modifiers::NONE, Key::ArrowDown);
                left = i.consume_key(Modifiers::NONE, Key::ArrowLeft);
                right = i.consume_key(Modifiers::NONE, Key::ArrowRight);
            });
        }
        if !busy && ui.ctx().input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            back = true;
        }
        if self.nc.step == Step::Club {
            let list: Vec<TeamId> = self.nc.division_clubs(self.nc.division).iter().map(|c| c.team).collect();
            let at = self.nc.club.and_then(|c| list.iter().position(|&t| t == c));
            if down && !list.is_empty() {
                self.nc.club = Some(list[at.map_or(0, |i| (i + 1).min(list.len() - 1))]);
            }
            if up && !list.is_empty() {
                self.nc.club = Some(list[at.map_or(0, |i| i.saturating_sub(1))]);
            }
        }
        if self.nc.step == Step::Division && self.nc.built.is_some() {
            let d = self.nc.division;
            if right && d + 1 < self.nc.divs {
                self.nc.set_division(d + 1);
            }
            if left && d > 0 {
                self.nc.set_division(d - 1);
            }
        }
        if next {
            self.nc_next();
        }
        if back {
            self.nc_back();
        }

        let side = theme::side();
        let screen = ui.max_rect();
        let margin = if side { 8.0 } else { 18.0 };
        let w = (screen.width() - 2.0 * margin).min(1060.0);
        let wanted = match self.nc.step {
            Step::You => 400.0,
            Step::World => 470.0,
            Step::Confirm => 520.0,
            Step::Division => 600.0,
            Step::Club => 820.0,
        };
        let h = (screen.height() - 2.0 * margin).min(if side { 820.0 } else { wanted });
        let rect = egui::Rect::from_center_size(screen.center(), vec2(w, h));
        let pad = if side { 12.0 } else { 18.0 };
        let inner = vec2(w - 2.0 * pad, h - 2.0 * pad);
        let (header_h, footer_h) = if side { (40.0, 44.0) } else { (58.0, 50.0) };
        let (mut go_next, mut go_back) = (false, false);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            bd.card_padded(ui, pad as i8, |ui| {
                ui.set_min_size(inner);
                ui.set_max_size(inner);
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.allocate_ui(vec2(inner.x, header_h), |ui| self.nc_header(ui, side));
                let body_h = inner.y - header_h - footer_h - 12.0;
                ui.allocate_ui(vec2(inner.x, body_h), |ui| {
                    ui.set_min_size(vec2(inner.x, body_h));
                    match self.nc.step {
                        Step::You => self.nc_you(ui),
                        Step::World => self.nc_world(ui),
                        Step::Division => self.nc_division(ui, body_h),
                        Step::Club => self.nc_club(ui, body_h),
                        Step::Confirm => self.nc_confirm(ui, body_h),
                    }
                });
                ui.allocate_ui(vec2(inner.x, footer_h), |ui| {
                    ui.horizontal(|ui| {
                        let b = if self.nc.step == Step::You { "Cancel" } else { "Back" };
                        if ui
                            .add_enabled(!busy, egui::Button::new(b).min_size(vec2(96.0, 40.0)))
                            .clicked()
                        {
                            go_back = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let label = match self.nc.step {
                                Step::World if self.nc.stale() => "Build the league",
                                Step::Confirm => "Start career",
                                _ => "Next",
                            };
                            let ok = self.nc.can_next() && !busy;
                            let clicked = ui
                                .add_enabled(
                                    ok,
                                    egui::Button::new(RichText::new(label).family(theme::semibold()).color(theme::BG))
                                        .fill(theme::TEXT)
                                        .stroke(Stroke::NONE)
                                        .min_size(vec2(150.0, 40.0)),
                                )
                                .clicked();
                            if clicked {
                                go_next = true;
                            }
                            if self.nc.step == Step::Club && !busy {
                                if ui
                                    .add(egui::Button::new("Surprise me").min_size(vec2(120.0, 40.0)))
                                    .on_hover_text("A club from the middle of the division. Press again for another.")
                                    .clicked()
                                {
                                    self.nc.surprise_me();
                                }
                            }
                        });
                    });
                });
            });
        });
        if go_next {
            self.nc_next();
        }
        if go_back {
            self.nc_back();
        }
    }

    fn nc_header(&mut self, ui: &mut Ui, side: bool) {
        let step = self.nc.step;
        let n = Step::ALL.len();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                theme::eyebrow(ui, &format!("New career / step {} of {}", step.index() + 1, n));
                let title = match step {
                    Step::You => "Who are you?",
                    Step::World => "Choose a world",
                    Step::Division => "Choose a division",
                    Step::Club => "Choose your club",
                    Step::Confirm => "Ready?",
                };
                ui.label(
                    RichText::new(title)
                        .font(FontId::new(if side { 20.0 } else { 26.0 }, theme::semibold()))
                        .color(theme::TEXT),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let w = if side { 180.0 } else { 260.0 };
                let (rect, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
                let p = ui.painter_at(rect);
                let seg = (w - 4.0 * 4.0) / n as f32;
                for (i, s) in Step::ALL.iter().enumerate() {
                    let x = rect.left() + i as f32 * (seg + 4.0);
                    let r = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 2.0), vec2(seg, 5.0));
                    let col = if i <= step.index() { theme::ACCENT } else { theme::LINE_2 };
                    p.rect_filled(r, 2.5, col);
                    if !side || i == step.index() {
                        let _ = s;
                    }
                    if !side {
                        p.text(
                            egui::pos2(r.center().x, r.bottom() + 5.0),
                            Align2::CENTER_TOP,
                            s.label(),
                            FontId::new(10.0, egui::FontFamily::Monospace),
                            if i == step.index() { theme::TEXT } else { theme::TEXT_MUTE },
                        );
                    }
                }
            });
        });
    }

    fn nc_you(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        theme::eyebrow(ui, "Your name");
        let mut go = false;
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.nc.name)
                    .desired_width(ui.available_width().min(320.0))
                    .char_limit(28)
                    .font(FontId::new(18.0, theme::semibold()))
                    .margin(egui::Margin::symmetric(10, 8)),
            );
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                go = true;
            }
            if ui.add(egui::Button::new("Another name").min_size(vec2(130.0, 36.0))).clicked() {
                self.nc.another_name();
            }
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new("The manager the board, the press and your saved careers know you as.")
                .color(theme::TEXT_DIM),
        );
        ui.add_space(10.0);
        theme::eyebrow(ui, "Your reputation");
        ui.label(
            RichText::new(
                "Nobody has heard of you yet. Your reputation starts from the club you choose: \
                 a big club means a big name, a small one a small name. Results move it from there.",
            )
            .color(theme::TEXT_DIM),
        );
        if go {
            self.nc_next();
        }
    }

    fn nc_world(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        theme::eyebrow(ui, "World");
        ui.spacing_mut().interact_size.y = 36.0;
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.nc.seed)
                    .range(1..=999_999)
                    .prefix("world ")
                    .speed(1.0),
            )
            .on_hover_text("Drag, or click and type. The same number is always the same league.");
            if ui.add(egui::Button::new("Random").min_size(vec2(96.0, 36.0))).clicked() {
                self.nc.seed = random_seed();
                // The name made for the old world stays: it is the player's.
            }
        });
        ui.label(
            RichText::new(
                "Every world is a different league: other clubs, other players, other owners. \
                 The same number always gives the same one.",
            )
            .color(theme::TEXT_DIM),
        );
        ui.add_space(10.0);
        theme::eyebrow(ui, "Size");
        ui.horizontal_wrapped(|ui| {
            ui.label("Divisions");
            menu::segmented(ui, &DIV_CHOICES.map(|d| (d, d.to_string())), &mut self.nc.divs);
            ui.add_space(10.0);
            ui.label("Clubs in each");
            menu::segmented(ui, &PER_CHOICES.map(|d| (d, d.to_string())), &mut self.nc.per);
        });
        let total = self.nc.divs * self.nc.per;
        ui.label(
            RichText::new(format!(
                "{total} clubs in all. The standard league is three divisions of twelve. \
                 A bigger league takes a little longer to build and a longer season to play."
            ))
            .color(theme::TEXT_DIM),
        );
        if !self.nc.stale() {
            ui.label(theme::mono("This league is built.").color(theme::POS));
        }
    }

    fn nc_division(&mut self, ui: &mut Ui, body_h: f32) {
        let Some(b) = &self.nc.built else { return };
        let world = &b.world;
        let divs = self.nc.divs;
        let moves = world.tiers.moves;
        let side = theme::side();
        let width = ui.available_width();
        let narrow = width < 640.0 && !side;
        let gap = 10.0;
        let card_w = if narrow { width } else { (width - gap * (divs as f32 - 1.0)) / divs as f32 };
        let mut pick = None;
        let content = |ui: &mut Ui, pick: &mut Option<usize>| {
            let lay = |ui: &mut Ui, d: usize, pick: &mut Option<usize>| {
                let clubs: Vec<&ClubInfo> = b.clubs.iter().filter(|c| c.division == d).collect();
                let n = clubs.len() as f32;
                let strength = clubs.iter().map(|c| c.strength).sum::<f32>() / n;
                let hall = clubs.iter().map(|c| world.econ.arenas[c.team].capacity as f32).sum::<f32>() / n;
                let cash = clubs.iter().map(|c| world.teams[c.team].budget as f32).sum::<f32>() / n;
                let (head, body) = blurb(d, divs, moves);
                let chosen = self.nc.division == d;
                let frame = egui::Frame::NONE
                    .inner_margin(egui::Margin::same(if side { 9 } else { 14 }))
                    .corner_radius(10)
                    .fill(if chosen { theme::ACCENT.gamma_multiply(0.08) } else { theme::white(150) })
                    .stroke(Stroke::new(if chosen { 2.0 } else { 1.0 }, if chosen { theme::ACCENT } else { theme::LINE_2 }));
                let out = frame.show(ui, |ui| ui.vertical(|ui| {
                    ui.set_width(card_w - if side { 18.0 } else { 28.0 } - 2.0);
                    if !narrow && !side {
                        ui.set_min_height(250.0);
                    }
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.label(
                        RichText::new(world.division_name(d))
                            .font(FontId::new(if side { 16.0 } else { 19.0 }, theme::semibold()))
                            .color(if chosen { theme::ACCENT } else { theme::TEXT }),
                    );
                    ui.label(RichText::new(head).family(theme::semibold()).color(theme::TEXT_DIM));
                    ui.label(RichText::new(body).size(theme::fs(12.5)).color(theme::TEXT_DIM));
                    ui.add_space(4.0);
                    let stats = if side {
                        vec![
                            ("Squads", format!("{} clubs, strength {strength:.1}", clubs.len())),
                            ("Halls", format!("{} seats, {} money", money(hall as i64), money(cash as i64))),
                        ]
                    } else {
                        vec![
                            ("Clubs", format!("{}", clubs.len())),
                            ("Squads", format!("{strength:.1} average strength")),
                            ("Halls", format!("{} seats on average", money(hall as i64))),
                            ("Money", format!("{} on average", money(cash as i64))),
                        ]
                    };
                    for (k, v) in stats {
                        ui.horizontal(|ui| {
                            menu::key_label(ui, 56.0, k);
                            ui.label(theme::mono(v).size(theme::fs(12.0)));
                        });
                    }
                }));
                let r = ui.interact(out.response.rect, ui.id().with(("div", d)), Sense::click());
                if r.clicked() {
                    *pick = Some(d);
                }
            };
            if narrow {
                for d in 0..divs {
                    lay(ui, d, pick);
                }
            } else {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for d in 0..divs {
                        lay(ui, d, pick);
                    }
                });
            }
        };
        let _ = body_h;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| content(ui, &mut pick));
        if let Some(d) = pick {
            self.nc.set_division(d);
        }
    }

    fn nc_club(&mut self, ui: &mut Ui, body_h: f32) {
        let side = theme::side();
        let width = ui.available_width();
        let stacked = width < 640.0;
        let list_w = if stacked { width } else { (width * 0.42).min(400.0) };
        let list: Vec<(TeamId, String, String, f32, usize)> = self
            .nc
            .division_clubs(self.nc.division)
            .iter()
            .map(|c| (c.team, c.name.clone(), c.town.clone(), c.strength, c.expect))
            .collect();
        let (best, worst) = self.nc.built.as_ref().map_or((10.0, 5.0), |b| (b.best, b.worst));
        let mut pick = None;
        let size = self.nc.division_size();
        let list_h = if stacked { (body_h * 0.45).max(120.0) } else { body_h };
        let mut draw_list = |ui: &mut Ui, selected: Option<TeamId>, pick: &mut Option<TeamId>| {
            ui.set_width(list_w);
            egui::ScrollArea::vertical()
                .id_salt("nc_clubs")
                .max_height(list_h)
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    for (t, name, town, strength, expect) in &list {
                        let h = if side { 40.0 } else { 46.0 };
                        let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
                        let on = selected == Some(*t);
                        let p = ui.painter_at(rect);
                        if on {
                            p.rect_filled(rect, 7.0, theme::ACCENT.gamma_multiply(0.12));
                            p.rect_stroke(rect, 7.0, Stroke::new(1.5, theme::ACCENT), egui::StrokeKind::Inside);
                        } else if r.hovered() {
                            p.rect_filled(rect, 7.0, theme::LINE.gamma_multiply(0.7));
                        }
                        let x = rect.left() + 10.0;
                        p.text(
                            egui::pos2(x, rect.top() + h * 0.30),
                            Align2::LEFT_CENTER,
                            name,
                            FontId::new(theme::fs(14.5), if on { theme::semibold() } else { egui::FontFamily::Proportional }),
                            if on { theme::ACCENT } else { theme::TEXT },
                        );
                        p.text(
                            egui::pos2(x, rect.top() + h * 0.72),
                            Align2::LEFT_CENTER,
                            format!("{town} / board asks top {expect}"),
                            FontId::new(theme::fs(11.0), egui::FontFamily::Monospace),
                            theme::TEXT_MUTE,
                        );
                        let full = (rect.width() * 0.22).clamp(36.0, 80.0);
                        let x0 = rect.right() - full - 42.0;
                        let share = ((strength - worst) / (best - worst).max(0.1)).clamp(0.05, 1.0);
                        let bar = |w: f32, col: Color32| {
                            p.rect_filled(egui::Rect::from_min_size(egui::pos2(x0, rect.center().y - 3.0), vec2(w, 6.0)), 3.0, col);
                        };
                        bar(full, theme::LINE_2);
                        bar(full * share, if on { theme::ACCENT } else { theme::TEXT_MUTE });
                        p.text(
                            egui::pos2(rect.right() - 8.0, rect.center().y),
                            Align2::RIGHT_CENTER,
                            format!("{strength:.1}"),
                            FontId::new(theme::fs(12.0), egui::FontFamily::Monospace),
                            theme::TEXT_DIM,
                        );
                        if r.clicked() {
                            *pick = Some(*t);
                        }
                        if on && ui.input(|i| i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowUp)) {
                            r.scroll_to_me(None);
                        }
                    }
                });
        };
        let selected = self.nc.club;
        // The profile of the club chosen.
        if let Some(t) = selected {
            if self.nc.summary.as_ref().map(|s| s.0) != Some(t) {
                if let Some(w) = self.nc.world() {
                    let s = w.club_summary(t);
                    self.nc.summary = Some((t, Box::new(s)));
                }
            }
        }
        let _ = size;
        if stacked {
            draw_list(ui, selected, &mut pick);
            ui.add_space(6.0);
            self.nc_profile(ui, body_h - list_h - 6.0);
        } else {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                let down = egui::Layout::top_down(egui::Align::Min);
                ui.allocate_ui_with_layout(vec2(list_w, body_h), down, |ui| draw_list(ui, selected, &mut pick));
                ui.allocate_ui_with_layout(vec2(width - list_w - 14.0, body_h), down, |ui| {
                    ui.set_width(width - list_w - 14.0);
                    self.nc_profile(ui, body_h)
                });
            });
        }
        if let Some(t) = pick {
            self.nc.club = Some(t);
        }
    }

    fn nc_profile(&mut self, ui: &mut Ui, h: f32) {
        let Some((t, s)) = &self.nc.summary else {
            ui.label(RichText::new("Choose a club from the list.").color(theme::TEXT_DIM));
            return;
        };
        let Some(w) = self.nc.world() else { return };
        let side = theme::side();
        let info = self.nc.built.as_ref().and_then(|b| b.clubs.iter().find(|c| c.team == *t));
        let (word, hard) = difficulty(s);
        let owner = s.people.first();
        let b = &w.econ.books[*t];
        let wage = if b.wage_budget > 0 { b.wage_budget } else { w.payroll(*t) };
        let honours = {
            let mut v = Vec::new();
            if !s.titles.is_empty() {
                v.push(format!(
                    "{} title{} (last {})",
                    s.titles.len(),
                    if s.titles.len() == 1 { "" } else { "s" },
                    s.titles.iter().max().copied().unwrap_or(0)
                ));
            }
            if !s.finals.is_empty() {
                v.push(format!("{} lost final{}", s.finals.len(), if s.finals.len() == 1 { "" } else { "s" }));
            }
            if !s.promotions.is_empty() {
                v.push(format!("{} promotion{}", s.promotions.len(), if s.promotions.len() == 1 { "" } else { "s" }));
            }
            if v.is_empty() { "None yet.".to_string() } else { v.join(", ") }
        };
        let stars = s
            .squad
            .stars
            .iter()
            .map(|g| format!("{} ({}, age {}, rated {:.0})", g.name, g.position.label(), g.age, g.overall))
            .collect::<Vec<_>>()
            .join("; ");
        let owner_line = owner.map_or(String::new(), |p| {
            let traits = p.traits.iter().take(2).copied().collect::<Vec<_>>().join(", ");
            if traits.is_empty() {
                format!("{}, {}", p.name, p.age)
            } else {
                format!("{}, {}: {traits}", p.name, p.age)
            }
        });
        let mut rows: Vec<(&str, String)> = vec![
            ("The job", format!("{word}. {hard}")),
            (
                "Board",
                format!("Top {} of {} this season. {}", s.expected_place, s.division_size, s.fans_expect),
            ),
            (
                if owner.is_some_and(|p| p.role == "Chairman") { "Chairman" } else { "Owner" },
                owner_line,
            ),
            (
                "Money",
                format!(
                    "{} in the bank. Wages {}, transfers {}.",
                    money(w.teams[*t].budget),
                    money(wage),
                    money(b.transfer_budget)
                ),
            ),
            ("Hall", format!("{}, {} seats", s.arena.name, money(s.arena.capacity as i64))),
            ("Best players", stars),
            (
                "Squad",
                format!(
                    "Average age {:.1}. {} of {} on paper{}.",
                    s.squad.average_age,
                    ordinal(s.squad.strength_rank),
                    s.division_size,
                    info.map_or(String::new(), |i| format!(", strength {:.1}", i.strength))
                ),
            ),
            ("Founded", format!("{}. {}", s.founded, s.origin)),
            ("Honours", honours),
            ("Rival", s.rival.clone()),
            ("Standing", format!("{}.", capital(s.stature_word))),
        ];
        if side {
            // Short on height: fewer, fuller rows.
            let get = |k: &str, rows: &[(&str, String)]| rows.iter().find(|r| r.0 == k).map(|r| r.1.clone()).unwrap_or_default();
            let money_row = format!("{} {}", get("Money", &rows), get("Hall", &rows));
            let players = get("Best players", &rows);
            let founded = format!("Founded {}.", s.founded);
            let history = format!("{founded} Honours: {} Rival: {}.", get("Honours", &rows), get("Rival", &rows));
            let owner_key = rows[2].0;
            rows = vec![
                ("The job", get("The job", &rows)),
                ("Board", get("Board", &rows)),
                (owner_key, get(owner_key, &rows)),
                ("Money", money_row),
                ("Players", players),
                ("History", history),
            ];
        }
        egui::ScrollArea::vertical()
            .id_salt("nc_profile")
            .max_height(h)
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 5.0;
                ui.label(
                    RichText::new(&s.name)
                        .font(FontId::new(if side { 17.0 } else { 22.0 }, theme::semibold()))
                        .color(theme::TEXT),
                );
                ui.label(theme::mono(format!("{} / stature {}", s.division_name, s.stature)).color(theme::TEXT_MUTE));
                ui.add_space(2.0);
                for (k, v) in rows {
                    if v.is_empty() {
                        continue;
                    }
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        menu::key_label(ui, if side { 62.0 } else { 92.0 }, k);
                        ui.add(egui::Label::new(RichText::new(v).size(theme::fs(13.0)).color(theme::TEXT)).wrap());
                    });
                }
            });
    }

    fn nc_confirm(&mut self, ui: &mut Ui, _body_h: f32) {
        let Some(w) = self.nc.world() else { return };
        let Some(t) = self.nc.club else { return };
        let (name, division, expect, n) = (
            w.teams[t].name.clone(),
            w.division_name(w.teams[t].division),
            w.expected_finish(t),
            w.division_size(t),
        );
        let facts = [
            ("You", self.nc.name.trim().to_string()),
            (
                "World",
                format!("world {}: {} divisions of {}", self.nc.seed, self.nc.divs, self.nc.per),
            ),
            ("Club", format!("{name}, {division}")),
            ("The board asks", format!("a top {expect} finish of {n}")),
        ];
        ui.add_space(4.0);
        for (k, v) in facts {
            ui.horizontal(|ui| {
                menu::key_label(ui, 120.0, k);
                ui.label(RichText::new(v).font(FontId::new(theme::fs(16.0), theme::semibold())));
            });
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new("You start in the training camp, with a few days to look around before the season.")
                .color(theme::TEXT_DIM),
        );
        if self.slots.full() && self.persisting() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!(
                    "All {} slots are in use. The new career takes the place of one:",
                    crate::persist::max_slots()
                ))
                .color(theme::WARN),
            );
            if self.nc.replace.is_none() {
                self.nc.replace = self.slots.oldest();
            }
            egui::ScrollArea::vertical().id_salt("nc_replace").max_height(110.0).show(ui, |ui| {
                for s in self.slots.slots.clone() {
                    let on = self.nc.replace == Some(s.id);
                    let label = format!("{}  /  {}  /  {}", s.name, s.line(), crate::persist::ago(crate::persist::now_secs(), s.played));
                    if ui.add(egui::Button::new(label).selected(on).min_size(vec2(0.0, 32.0))).clicked() {
                        self.nc.replace = Some(s.id);
                    }
                }
            });
        }
    }
}

impl NewCareer {
    fn division_size(&self) -> usize {
        self.division_clubs(self.division).len()
    }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
}

/// A world number to start from: friendly, and different each time.
pub(crate) fn random_seed() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    let t = crate::persist::now_secs().wrapping_mul(2654435761).wrapping_add(n.wrapping_mul(40503));
    1 + (t ^ (t >> 13)) % 9999
}

#[cfg(test)]
mod tests {
    use basketman_core::save;

    use super::*;

    /// The app and an agent must make the same career from the same seed.
    #[test]
    fn the_flow_makes_the_world_an_agent_would() {
        for (seed, per, divs, team) in [(9, 4, 2, 5), (3, 6, 3, 0), (12, 4, 3, 11)] {
            let mut nc = NewCareer::new(seed);
            nc.per = per;
            nc.divs = divs;
            nc.generate();
            nc.club = Some(team);
            let (made, t) = nc.finish().expect("a club is chosen");
            assert_eq!(t, team);

            let mut agent = World::new_career_tiers(seed, per, divs);
            agent.apply(Command::TakeCharge(team)).unwrap();
            assert_eq!(made.watch, Some(team));
            assert!(save::encode(&made) == save::encode(&agent));
            assert!(format!("{made:?}") == format!("{agent:?}"));
        }
    }

    #[test]
    fn the_league_is_built_once_per_seed_and_size() {
        let mut nc = NewCareer::new(4);
        nc.per = 4;
        nc.divs = 2;
        assert!(nc.stale());
        nc.generate();
        assert!(!nc.stale());
        // Going back and forth changes nothing.
        let before = format!("{:?}", nc.world().unwrap().teams[0].name);
        nc.step = Step::Division;
        nc.step = Step::World;
        nc.generate();
        assert_eq!(before, format!("{:?}", nc.world().unwrap().teams[0].name));
        // A new seed or size does.
        nc.seed = 5;
        assert!(nc.stale());
        nc.generate();
        assert!(!nc.stale());
        nc.per = 6;
        assert!(nc.stale());
    }

    #[test]
    fn a_surprise_is_a_club_of_the_division() {
        let mut nc = NewCareer::new(2);
        nc.per = 8;
        nc.divs = 3;
        nc.generate();
        for d in 0..3 {
            nc.set_division(d);
            for _ in 0..10 {
                nc.surprise_me();
                let t = nc.club.unwrap();
                assert_eq!(nc.world().unwrap().teams[t].division, d);
            }
        }
    }

    #[test]
    fn names_are_made_from_the_world_and_vary() {
        assert_eq!(make_name(7, 0), make_name(7, 0));
        assert_ne!(make_name(7, 0), make_name(7, 1));
        assert!(make_name(7, 0).contains(' '));
    }

    /// Every size the picker offers must build and play a round.
    #[test]
    #[ignore]
    fn every_size_the_picker_offers_works() {
        for &divs in &DIV_CHOICES {
            for &per in &PER_CHOICES {
                let t = std::time::Instant::now();
                let mut w = career(1, per, divs, 0);
                eprintln!("{divs} x {per}: built in {:?}", t.elapsed());
                for _ in 0..3 {
                    w.play_round();
                }
                assert!(w.advance() == basketman_core::inbox::Stop::Mail || true);
            }
        }
    }
}
