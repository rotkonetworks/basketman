//! The club as a place with a past and with people: the arrival (a
//! sequence of five short pages the manager steps through or skips), the
//! permanent Club page, and the manager's career: his record, the offers
//! that come, and the deal with a new club.

use basketman_core::board::{sack_line, warn_line};
use basketman_core::club::{ClubSummary, PersonView, YearLine};
use basketman_core::command::Command;
use basketman_core::inbox::OfferState;
use basketman_core::staff::Job;
use eframe::egui::{self, Color32, FontId, Rect, RichText, Sense, Ui, pos2, vec2};

use crate::app::{App, GAP, Tab, money, name};
use crate::pages::dim;
use crate::scenes::Kind;
use crate::screens::{cols, split_top};
use crate::shell::{Tier, panel_wide};
use crate::theme::{self, Backdrop};

/// The introduction's pages.
const STEPS: [&str; 5] = ["The club", "The people", "The arena", "The squad", "What next"];

#[derive(Default)]
pub(crate) struct ClubUi {
    /// Page of the introduction.
    pub step: usize,
    /// Offer being looked at, the people asked to come, the new club's
    /// people who stay.
    pub offer: Option<usize>,
    pub bring: Vec<Job>,
    pub keep: Vec<Job>,
    pub keep_set: bool,
    pub note: Option<String>,
    pub confirm_resign: bool,
}

fn para(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(14.5).color(theme::TEXT)).wrap());
}

fn heading(ui: &mut Ui, eyebrow: &str, text: &str) {
    theme::eyebrow(ui, eyebrow);
    ui.add(
        egui::Label::new(
            RichText::new(text)
                .font(FontId::new(26.0, theme::semibold()))
                .color(theme::TEXT),
        )
        .wrap(),
    );
}

fn figure(ui: &mut Ui, label: &str, value: String) {
    ui.vertical(|ui| {
        ui.label(theme::eyebrow_text(label));
        ui.label(RichText::new(value).font(FontId::new(18.0, theme::semibold())));
    });
}

fn years(list: &[u16]) -> String {
    if list.is_empty() {
        "none".to_string()
    } else {
        list.iter()
            .map(|y| y.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn honours_text(s: &ClubSummary) -> String {
    let top = match s.titles.len() {
        0 => "Never champions of the top division.".to_string(),
        1 => format!("Champions of the top division once, in {}.", s.titles[0]),
        n => format!("Champions of the top division {n} times: {}.", years(&s.titles)),
    };
    if s.lower_titles.is_empty() {
        top
    } else {
        let lower: Vec<u16> = s.lower_titles.iter().map(|t| t.0).collect();
        format!("{top} Won a lower division in {}.", years(&lower))
    }
}

/// "Best finish: 2nd, in 2011." and, below the top, the division.
fn best_text(s: &ClubSummary) -> Option<String> {
    let (p, y) = s.best?;
    let place = basketman_core::budget::ordinal(p as usize);
    Some(if s.best_division == 0 {
        format!("Best finish: {place}, in {y}.")
    } else {
        let n = ["First", "Second", "Third", "Fourth", "Fifth"]
            .get(s.best_division as usize)
            .map_or("lower".to_string(), |n| n.to_string());
        format!("Best finish: {place} in the {n} Division, in {y}.")
    })
}

fn person_card(ui: &mut Ui, bd: &Backdrop, p: &PersonView, speaks: bool) {
    bd.card(ui, |ui| {
        ui.set_width(ui.available_width());
        theme::eyebrow(ui, p.role);
        ui.horizontal_wrapped(|ui| {
            ui.label(name(&p.name));
            ui.label(dim(format!("{}", p.age)));
        });
        para(ui, &p.background);
        ui.add_space(4.0);
        for t in &p.traits {
            ui.label(dim(format!("- {t}")));
        }
        if speaks && !p.says.is_empty() {
            ui.add_space(8.0);
            for line in &p.says {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("\"{line}\""))
                            .italics()
                            .size(14.0)
                            .color(theme::TEXT),
                    )
                    .wrap(),
                );
            }
        }
    });
}

/// A bar from 0 to 100 with the lines that matter marked on it.
fn confidence_bar(ui: &mut Ui, value: f32, lines: &[f32]) {
    let w = ui.available_width().min(520.0);
    let (r, _) = ui.allocate_exact_size(vec2(w, 14.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 7.0, theme::ink(18));
    let colour = if value >= 60.0 {
        theme::POS
    } else if value >= 35.0 {
        Color32::from_rgb(214, 160, 40)
    } else {
        Color32::from_rgb(200, 70, 60)
    };
    let fill = Rect::from_min_size(r.min, vec2(r.width() * value.clamp(0.0, 100.0) / 100.0, r.height()));
    p.rect_filled(fill, 7.0, colour);
    for &l in lines {
        let x = r.left() + r.width() * l / 100.0;
        p.line_segment(
            [pos2(x, r.top() - 2.0), pos2(x, r.bottom() + 2.0)],
            egui::Stroke::new(1.5, theme::TEXT),
        );
    }
}

/// Every season of the club's life as a small square.
fn record_strip(ui: &mut Ui, record: &[YearLine]) {
    if record.is_empty() {
        return;
    }
    let per_row = 20usize;
    let cell = ((ui.available_width() - 4.0) / per_row as f32).clamp(8.0, 20.0);
    let rows = record.len().div_ceil(per_row);
    let (r, _) = ui.allocate_exact_size(vec2(cell * per_row as f32, cell * rows as f32), Sense::hover());
    let p = ui.painter();
    for (i, l) in record.iter().enumerate() {
        let x = r.left() + (i % per_row) as f32 * cell;
        let y = r.top() + (i / per_row) as f32 * cell;
        let sq = Rect::from_min_size(pos2(x + 1.0, y + 1.0), vec2(cell - 2.0, cell - 2.0));
        let colour = if l.place == 0 {
            theme::ink(10)
        } else if l.champion {
            Color32::from_rgb(230, 180, 40)
        } else if l.runner_up {
            Color32::from_rgb(170, 175, 185)
        } else if l.place <= 4 {
            Color32::from_rgb(90, 150, 200)
        } else {
            theme::ink(60)
        };
        p.rect_filled(sq, 2.0, colour);
        if l.played {
            p.rect_stroke(
                sq,
                2.0,
                egui::Stroke::new(1.5, theme::TEXT),
                egui::StrokeKind::Inside,
            );
        }
    }
    let (first, last) = (record[0].year, record[record.len() - 1].year);
    ui.label(dim(format!(
        "{first} to {last}, {per_row} to a row. Gold: won the division. Grey: runners-up in the top division. Blue: top four. \
         Outlined: seasons you have managed."
    )));
}

impl App {
    /// Follow the club the manager runs: after a move the screens must not
    /// keep showing the old one.
    pub(crate) fn follow_watch(&mut self) {
        if let Some(w) = self.world.watch
            && w != self.team
        {
            self.team = w;
            self.club = None;
            self.profile = None;
            self.mail = None;
            self.slot = 0;
            self.compare.clear();
            self.deal = Default::default();
            self.history.clear();
        }
        // Out of work: the pages of a club you do not run are closed.
        if self.world.watch.is_none()
            && self.world.jobless()
            && !matches!(
                self.tab,
                Tab::Inbox
                    | Tab::Career
                    | Tab::Offer
                    | Tab::League
                    | Tab::Stats
                    | Tab::Club
                    | Tab::Game
                    | Tab::Welcome
            )
        {
            self.tab = Tab::Career;
        }
    }

    /// Start the arrival sequence at its first page.
    pub(crate) fn start_intro(&mut self) {
        self.cu.step = 0;
        self.tab = Tab::Welcome;
    }

    // ---- the arrival -------------------------------------------------------

    pub(crate) fn screen_welcome(
        &mut self,
        ui: &mut Ui,
        bd: &Backdrop,
        stage: Rect,
        tier: Tier,
        g: f32,
        full: Rect,
    ) {
        let step = self.cu.step.min(STEPS.len() - 1);
        // The arena page shows the hall itself behind the panel.
        let kind = if step == 2 { Kind::Home } else { Kind::Data };
        self.sh
            .stage
            .hall(ui, kind, &self.world, self.team, full, full);
        let narrow = tier == Tier::Narrow;
        let rect = if step == 2 && !narrow {
            cols(stage, &[stage.width().min(470.0), -1.0], g)[0]
        } else if step == 2 {
            split_top(stage, stage.height() * 0.3, g).1
        } else if narrow {
            stage
        } else {
            cols(stage, &[stage.width().min(840.0), -1.0], g)[0]
        };
        panel_wide(ui, rect, "welcome", 0.0, |ui| self.welcome_body(ui, bd, step));
    }

    fn welcome_body(&mut self, ui: &mut Ui, bd: &Backdrop, step: usize) {
        let s = self.world.club_summary(self.team);
        // Where we are, and the way out.
        let mut go: Option<isize> = None;
        let mut skip = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(theme::eyebrow_text(&format!(
                "Welcome {} / {}",
                step + 1,
                STEPS.len()
            )));
            for (i, label) in STEPS.iter().enumerate() {
                let on = i == step;
                let text = RichText::new(*label).size(12.5);
                let text = if on { text.color(theme::TEXT).strong() } else { text.color(theme::TEXT_DIM) };
                if ui.add(egui::Button::new(text).frame(false)).clicked() {
                    go = Some(i as isize - step as isize);
                }
            }
            if ui.button("Skip").clicked() {
                skip = true;
            }
            if step + 1 < STEPS.len() && theme::primary_button(ui, true, "Next") {
                go = Some(1);
            }
        });
        ui.add_space(6.0);
        match step {
            0 => self.intro_club(ui, bd, &s),
            1 => self.intro_people(ui, bd, &s),
            2 => self.intro_arena(ui, bd, &s),
            3 => self.intro_squad(ui, bd, &s),
            _ => self.intro_next(ui, bd, &s),
        }
        ui.add_space(GAP);
        ui.horizontal(|ui| {
            if step > 0 && ui.button("Back").clicked() {
                go = Some(-1);
            }
            let last = step + 1 >= STEPS.len();
            let label = if last { "Start the season" } else { "Next" };
            if theme::primary_button(ui, true, label) {
                if last {
                    skip = true;
                } else {
                    go = Some(1);
                }
            }
        });
        if let Some(d) = go {
            self.cu.step = (self.cu.step as isize + d).clamp(0, STEPS.len() as isize - 1) as usize;
        }
        if skip {
            self.cu.step = 0;
            self.go(Tab::Inbox);
        }
    }

    fn intro_club(&self, ui: &mut Ui, bd: &Backdrop, s: &ClubSummary) {
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            heading(ui, "The club", &s.name);
            ui.label(dim(format!(
                "Founded {}. {}. Plays in the {}.",
                s.founded,
                capital(s.stature_word),
                s.division_name
            )));
            ui.add_space(6.0);
            para(ui, &s.origin);
            ui.add_space(6.0);
            para(ui, &honours_text(s));
            if !s.finals.is_empty() {
                para(ui, &format!("Lost the final in {}.", years(&s.finals)));
            }
            if let Some(text) = best_text(s)
                && s.titles.is_empty()
            {
                para(ui, &text);
            }
            ui.add_space(6.0);
            record_strip(ui, &s.record);
        });
        ui.add_space(GAP);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Legends");
            for l in &s.legends {
                ui.horizontal_wrapped(|ui| {
                    ui.label(name(&l.name));
                    ui.label(dim(format!("{}, {} to {}", l.position.label(), l.from, l.to)));
                });
                para(ui, &l.known_for);
                ui.add_space(4.0);
            }
        });
        ui.add_space(GAP);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Rival and fans");
            ui.label(name(&s.rival));
            para(ui, "Our oldest rival. Every game against them counts double in the stands.");
            ui.add_space(4.0);
            para(ui, s.fans_expect);
        });
    }

    fn intro_people(&self, ui: &mut Ui, bd: &Backdrop, s: &ClubSummary) {
        ui.label(
            RichText::new("The people above you")
                .font(FontId::new(26.0, theme::semibold())),
        );
        ui.add_space(6.0);
        for (i, p) in s.people.iter().enumerate() {
            person_card(ui, bd, p, i == 0);
            ui.add_space(GAP);
        }
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Your staff");
            ui.horizontal_wrapped(|ui| {
                ui.label(name(&s.gm.name));
                ui.label(dim("General manager. He makes the deals for you."));
            });
            ui.add_space(4.0);
            for l in &s.staff {
                ui.horizontal_wrapped(|ui| {
                    ui.label(name(&l.name));
                    ui.label(dim(format!("{}, {} {}", l.job, l.skill.to_lowercase(), l.rating)));
                });
            }
            ui.add_space(4.0);
            ui.label(dim("They came with the club. You can change them on the Staff page."));
        });
    }

    fn intro_arena(&self, ui: &mut Ui, bd: &Backdrop, s: &ClubSummary) {
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            heading(ui, "The arena", &s.arena.name);
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 30.0;
                figure(ui, "Capacity", money(s.arena.capacity as i64));
                figure(ui, "Condition", format!("{:.0}%", s.arena.condition * 100.0));
            });
            ui.add_space(6.0);
            for line in &s.arena.special {
                para(ui, line);
            }
        });
    }

    fn intro_squad(&self, ui: &mut Ui, bd: &Backdrop, s: &ClubSummary) {
        let q = &s.squad;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            heading(ui, "The squad", &format!("Number {} on paper", q.strength_rank));
            ui.add_space(6.0);
            theme::eyebrow(ui, "Stars");
            for g in &q.stars {
                ui.horizontal_wrapped(|ui| {
                    ui.label(name(&g.name));
                    ui.label(dim(format!(
                        "{}, {}, rating {:.1}, {} for {} more year{}",
                        g.position.label(),
                        g.age,
                        g.overall,
                        money(g.salary as i64),
                        g.years_left,
                        if g.years_left == 1 { "" } else { "s" }
                    )));
                });
            }
        });
        ui.add_space(GAP);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Positions");
            for p in &q.positions {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(p.position.label()).family(theme::semibold()));
                    let colour = match p.verdict {
                        "strong" => theme::POS,
                        "weak" | "nobody there" => Color32::from_rgb(200, 70, 60),
                        _ => theme::TEXT_DIM,
                    };
                    ui.label(RichText::new(p.verdict).color(colour));
                    ui.label(dim(format!("best {:.1}, {} players", p.best, p.players)));
                });
            }
            ui.add_space(4.0);
            ui.label(dim(format!("Average age {:.1}.", q.average_age)));
            if !q.injured.is_empty() {
                ui.label(dim(format!("Injured: {}.", q.injured.join(", "))));
            }
            if !q.expiring.is_empty() {
                ui.label(dim(format!("Contracts ending: {}.", q.expiring.join(", "))));
            }
        });
        ui.add_space(GAP);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Your assistant says");
            for line in &q.assistant {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("\"{line}\"")).italics().size(14.0),
                    )
                    .wrap(),
                );
            }
        });
    }

    fn intro_next(&self, ui: &mut Ui, bd: &Backdrop, s: &ClubSummary) {
        let w = &self.world;
        let me = self.team;
        let versus = |f: &basketman_core::league::Fixture| {
            if f.home == me {
                format!("home against {}", w.teams[f.away].name)
            } else {
                format!("away at {}", w.teams[f.home].name)
            }
        };
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            heading(ui, "What happens next", "Your first dates");
            ui.add_space(6.0);
            if w.in_preseason() {
                para(
                    ui,
                    &format!(
                        "{}: the training camp is on. It runs until the league opens, {} days of \
                         training. Press Continue to pass a day.",
                        w.day_long(w.day),
                        w.preseason_days
                    ),
                );
                let friendlies: Vec<_> = w
                    .friendlies
                    .iter()
                    .filter(|f| f.home == me || f.away == me)
                    .collect();
                if !friendlies.is_empty() {
                    ui.add_space(4.0);
                    theme::eyebrow(ui, "Friendlies");
                    for f in friendlies {
                        para(ui, &format!("{}: {}.", w.day_long(f.day), versus(f)));
                    }
                    para(ui, "Friendlies do not count in the table. They are for sharpness.");
                }
            }
            let next = w
                .fixtures
                .iter()
                .filter(|f| !f.playoff && (f.home == me || f.away == me) && f.result.is_none())
                .min_by_key(|f| f.day);
            if let Some(f) = next {
                ui.add_space(4.0);
                theme::eyebrow(
                    ui,
                    if w.round == 0 { "Opening day" } else { "Next league game" },
                );
                para(ui, &format!("{}: {}.", w.day_long(f.day), versus(f)));
                let other = if f.home == me { f.away } else { f.home };
                para(
                    ui,
                    &format!(
                        "The day before, the assistant sends a preview of {}.",
                        w.teams[other].name
                    ),
                );
            }
            ui.add_space(4.0);
            para(
                ui,
                &format!(
                    "You play in the {}. The season is {} rounds, one a week. The board has \
                     asked for a place in the top {}.",
                    s.division_name,
                    w.rounds(),
                    s.expected_place
                ),
            );
            para(
                ui,
                "Your inbox has the chairman's letter, the budgets and the hall. Read them, set \
                 your lineup on the Tactics page, then press Continue to move through the week.",
            );
        });
    }

    // ---- the club page -----------------------------------------------------

    pub(crate) fn club_story(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let s = self.world.club_summary(self.team);
        let mut intro = false;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            heading(ui, "Your club", &s.name);
            ui.label(dim(format!(
                "Founded {}. {}. Plays in the {}.",
                s.founded,
                capital(s.stature_word),
                s.division_name
            )));
            ui.add_space(4.0);
            para(ui, &s.origin);
            ui.add_space(6.0);
            para(ui, &honours_text(&s));
            if !s.finals.is_empty() {
                para(ui, &format!("Runners-up: {}.", years(&s.finals)));
            }
            if let Some(text) = best_text(&s) {
                ui.label(dim(text));
            }
            ui.add_space(6.0);
            record_strip(ui, &s.record);
            ui.add_space(6.0);
            if ui.button("Show the introduction again").clicked() {
                intro = true;
            }
        });
        ui.add_space(GAP);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "The board's confidence in you");
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("{:.0}", s.confidence))
                        .font(FontId::new(26.0, theme::semibold())),
                );
                ui.label(dim(format!("The board is {}.", s.confidence_word)));
            });
            let p = self.world.boardroom(self.team).patience();
            confidence_bar(ui, s.confidence, &[warn_line(p), sack_line(p)]);
            ui.label(dim(format!(
                "A warning below {:.0}. Dismissal below {:.0}, after a warning.",
                warn_line(p),
                sack_line(p)
            )));
            ui.add_space(4.0);
            ui.label(dim(format!(
                "They expect a top-{} finish. {}",
                s.expected_place, s.fans_expect
            )));
            if s.warned {
                ui.label(RichText::new("You have been warned.").color(Color32::from_rgb(200, 70, 60)));
            }
            ui.add_space(6.0);
            theme::eyebrow(ui, "Why");
            if s.reasons.is_empty() {
                ui.label(dim("Nothing yet."));
            }
            for r in s.reasons.iter().rev().take(8) {
                let colour = if r.delta > 0.0 {
                    theme::POS
                } else if r.delta < 0.0 {
                    Color32::from_rgb(200, 70, 60)
                } else {
                    theme::TEXT_DIM
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{:+.1}", r.delta)).color(colour).family(egui::FontFamily::Monospace));
                    ui.add(egui::Label::new(RichText::new(&r.text).size(13.5)).wrap());
                });
            }
        });
        ui.add_space(GAP);
        for (i, p) in s.people.iter().enumerate() {
            person_card(ui, bd, p, i == 0);
            ui.add_space(GAP);
        }
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Legends");
            for l in &s.legends {
                ui.horizontal_wrapped(|ui| {
                    ui.label(name(&l.name));
                    ui.label(dim(format!("{}, {} to {}", l.position.label(), l.from, l.to)));
                });
                para(ui, &l.known_for);
            }
            ui.add_space(6.0);
            theme::eyebrow(ui, "Rival");
            ui.label(name(&s.rival));
        });
        if intro {
            self.start_intro();
        }
    }

    // ---- the manager's career ---------------------------------------------

    pub(crate) fn career_page(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let v = self.world.view_career();
        let mut look: Option<usize> = None;
        let mut resign = false;
        if v.jobless {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Out of work");
                para(
                    ui,
                    "You have no club. Time goes on. Clubs that lose their manager may call. \
                     Press Continue to let the weeks pass.",
                );
            });
            ui.add_space(GAP);
        }
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            heading(ui, "Your career", &format!("Reputation {}", v.reputation));
            ui.label(dim(format!("You are {}.", v.reputation_word)));
            confidence_bar(ui, v.reputation as f32, &[]);
            ui.add_space(4.0);
            ui.label(dim(
                "It grows when you beat what the club was asked for, and falls when you do not. \
                 Bigger clubs call when it is high.",
            ));
        });
        ui.add_space(GAP);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Jobs held");
            for sp in v.spells.iter().rev() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(name(&sp.club_name));
                    let to = sp.to_year.map_or("now".to_string(), |y| y.to_string());
                    ui.label(dim(format!("{} to {to}, {}", sp.from_year, sp.ended)));
                });
                ui.label(dim(format!(
                    "{} wins, {} losses{}{}",
                    sp.wins,
                    sp.losses,
                    sp.best_place.map_or(String::new(), |p| format!(", best finish {p} in division {}", sp.best_division + 1)),
                    {
                        let mut t = String::new();
                        if sp.titles > 0 { t += &format!(", {} title(s)", sp.titles); }
                        if sp.promotions > 0 { t += &format!(", {} promotion(s)", sp.promotions); }
                        if sp.relegations > 0 { t += &format!(", {} relegation(s)", sp.relegations); }
                        t
                    }
                )));
                ui.add_space(4.0);
            }
        });
        ui.add_space(GAP);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Offers");
            let open: Vec<_> = v.offers.iter().filter(|o| o.open).collect();
            if open.is_empty() {
                ui.label(dim("No club has called."));
            }
            for o in open {
                ui.horizontal_wrapped(|ui| {
                    ui.label(name(&o.club_name));
                    ui.label(dim(format!("{}, stature {}", o.division_name, o.stature)));
                    if ui.button("Look at the offer").clicked() {
                        look = Some(o.offer);
                    }
                });
                ui.label(dim(&o.why));
            }
            let old: Vec<_> = v.offers.iter().filter(|o| !o.open).collect();
            if !old.is_empty() {
                ui.add_space(6.0);
                theme::eyebrow(ui, "Past offers");
                for o in old.iter().rev().take(6) {
                    ui.label(dim(format!("{}: {}", o.club_name, o.state)));
                }
            }
            if !v.vacancies.is_empty() {
                ui.add_space(6.0);
                theme::eyebrow(ui, "Looking for a manager");
                ui.label(dim(
                    v.vacancies
                        .iter()
                        .map(|&c| self.world.teams[c].name.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
            }
        });
        if !v.jobless {
            ui.add_space(GAP);
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Leave");
                if self.cu.confirm_resign {
                    para(ui, "Resign from the club? You will be out of work until somebody calls.");
                    ui.horizontal(|ui| {
                        if ui.button("Yes, resign").clicked() {
                            resign = true;
                        }
                        if ui.button("No").clicked() {
                            self.cu.confirm_resign = false;
                        }
                    });
                } else if ui.button("Resign").clicked() {
                    self.cu.confirm_resign = true;
                }
            });
        }
        if let Some(n) = &self.cu.note {
            ui.add_space(GAP);
            ui.label(dim(n.clone()));
        }
        if let Some(i) = look {
            self.open_offer(i);
        }
        if resign {
            self.cu.confirm_resign = false;
            match self.world.apply(Command::Resign) {
                Ok(o) => self.cu.note = o.note,
                Err(e) => self.cu.note = Some(e.to_string()),
            }
            self.follow_watch();
        }
    }

    pub(crate) fn open_offer(&mut self, i: usize) {
        self.cu.offer = Some(i);
        self.cu.bring.clear();
        self.cu.keep_set = false;
        self.cu.note = None;
        self.go(Tab::Offer);
    }

    /// The job offer: the two clubs side by side, and who comes and who stays.
    pub(crate) fn offer_page(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let Some(i) = self.cu.offer else {
            self.go(Tab::Career);
            return;
        };
        if !self.cu.keep_set
            && let Some(o) = self.world.career.offers.get(i)
        {
            // Everybody stays unless you say otherwise.
            self.cu.keep = Job::HIRED
                .iter()
                .copied()
                .filter(|&j| self.world.teams[o.club].office.staffer(j).is_some())
                .collect();
            self.cu.keep_set = true;
        }
        let (bring, keep) = (self.cu.bring.clone(), self.cu.keep.clone());
        let view = match self.world.view_job_offer(i, &bring, &keep) {
            Ok(v) => v,
            Err(e) => {
                bd.card(ui, |ui| {
                    ui.label(dim(e));
                    if ui.button("Back").clicked() {
                        self.cu.offer = None;
                    }
                });
                return;
            }
        };
        let open = self.world.open_job_offers().contains(&i);
        let new = &view.summary;
        let mine = self.world.watch.map(|m| self.world.club_summary(m));
        if ui.button("< Career").clicked() {
            self.go(Tab::Career);
        }
        ui.add_space(4.0);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            heading(ui, "A job offer", &new.name);
            ui.label(dim(&view.terms.why));
        });
        ui.add_space(GAP);
        let wide = ui.available_width() > 700.0;
        let facts = |ui: &mut Ui, s: &ClubSummary, title: &str| {
            ui.label(theme::eyebrow_text(title));
            ui.label(name(&s.name));
            ui.label(dim(format!(
                "{}. Founded {}. Plays in the {}.",
                capital(s.stature_word),
                s.founded,
                s.division_name
            )));
            ui.label(dim(honours_text(s)));
            ui.label(dim(format!("Rival: {}", s.rival)));
            for p in &s.people {
                ui.label(dim(format!("{}: {}", p.role, p.name)));
                for t in &p.traits {
                    ui.label(RichText::new(format!("   {t}")).size(12.5).color(theme::TEXT_DIM));
                }
            }
            ui.label(dim(format!("Asks for a top-{} finish.", s.expected_place)));
            ui.label(dim(format!(
                "Squad: number {} on paper. Hall: {}, holds {}.",
                s.squad.strength_rank,
                s.arena.name,
                money(s.arena.capacity as i64)
            )));
        };
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            if wide {
                ui.columns(2, |c| {
                    if let Some(m) = &mine {
                        facts(&mut c[0], m, "Where you are");
                    }
                    facts(&mut c[1], new, "The offer");
                });
            } else {
                if let Some(m) = &mine {
                    facts(ui, m, "Where you are");
                    ui.separator();
                }
                facts(ui, new, "The offer");
            }
        });
        ui.add_space(GAP);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "The money");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 30.0;
                figure(ui, "In the bank", money(view.cash));
                figure(ui, "Wage budget", money(view.wage_budget));
                figure(ui, "Transfer budget", money(view.transfer_budget));
            });
        });
        ui.add_space(GAP);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Your people: who comes with you");
            if view.your_staff.is_empty() {
                ui.label(dim("You have no staff to bring."));
            }
            for l in &view.your_staff {
                let Some(job) = Job::HIRED.into_iter().find(|j| j.label() == l.job) else {
                    continue;
                };
                let verdict = self
                    .world
                    .watch
                    .and_then(|m| self.world.teams[m].office.staffer(job))
                    .map(|s| self.world.staff_will(new.team, job, s));
                let mut on = self.cu.bring.contains(&job);
                ui.horizontal_wrapped(|ui| {
                    if ui.checkbox(&mut on, "").changed() {
                        self.cu.bring.retain(|&j| j != job);
                        if on {
                            self.cu.bring.push(job);
                        }
                    }
                    ui.label(name(&l.name));
                    ui.label(dim(format!("{}, {} {}", l.job, l.skill.to_lowercase(), l.rating)));
                });
                if let Some(v) = verdict {
                    let (txt, col) = if v.willing {
                        ("would come", theme::POS)
                    } else {
                        ("would not come", Color32::from_rgb(200, 70, 60))
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(26.0);
                        ui.label(RichText::new(txt).color(col).size(13.0));
                        if !v.reasons.is_empty() {
                            ui.label(dim(format!("({})", v.reasons.join(", "))));
                        }
                    });
                }
            }
        });
        ui.add_space(GAP);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Their people: who stays");
            for l in &view.their_staff {
                let Some(job) = Job::HIRED.into_iter().find(|j| j.label() == l.job) else {
                    continue;
                };
                let mut on = self.cu.keep.contains(&job);
                ui.horizontal_wrapped(|ui| {
                    if ui.checkbox(&mut on, "").changed() {
                        self.cu.keep.retain(|&j| j != job);
                        if on {
                            self.cu.keep.push(job);
                        }
                    }
                    ui.label(name(&l.name));
                    ui.label(dim(format!("{}, {} {}", l.job, l.skill.to_lowercase(), l.rating)));
                });
            }
            ui.add_space(4.0);
            ui.label(dim(format!(
                "General manager {} stays.",
                new.gm.name
            )));
            if !view.terms.release.is_empty() {
                ui.label(dim(format!("Paid off: {}.", view.terms.release.join(", "))));
            }
        });
        ui.add_space(GAP);
        let mut accept = false;
        let mut decline = false;
        ui.horizontal_wrapped(|ui| {
            if let Some(r) = &view.terms.refused {
                ui.label(RichText::new(r).color(Color32::from_rgb(200, 70, 60)));
            }
        });
        ui.horizontal_wrapped(|ui| {
            if theme::primary_button(ui, open && view.terms.refused.is_none(), "Take the job") {
                accept = true;
            }
            if ui.add_enabled(open, egui::Button::new("Turn it down")).clicked() {
                decline = true;
            }
        });
        if !open {
            ui.label(dim("This offer is no longer open."));
        }
        if let Some(n) = &self.cu.note {
            ui.label(dim(n.clone()));
        }
        if accept {
            let cmd = Command::AcceptJob {
                offer: i,
                bring: self.cu.bring.clone(),
                keep: self.cu.keep.clone(),
            };
            match self.world.apply(cmd) {
                Ok(o) => {
                    self.cu.note = o.note;
                    self.cu.offer = None;
                    self.follow_watch();
                    self.start_intro();
                }
                Err(e) => self.cu.note = Some(e.to_string()),
            }
        }
        if decline {
            let _ = self.world.apply(Command::DeclineJob(i));
            self.cu.offer = None;
            self.go(Tab::Career);
        }
    }

    /// The buttons under a message that offers a job.
    pub(crate) fn job_buttons(&mut self, ui: &mut Ui, i: usize) {
        let Some(o) = self.world.career.offers.get(i) else {
            return;
        };
        ui.add_space(14.0);
        match o.state {
            OfferState::Open if self.world.open_job_offers().contains(&i) => {
                if theme::primary_button(ui, true, "Look at the offer") {
                    self.open_offer(i);
                }
            }
            OfferState::Accepted => {
                ui.label(RichText::new("You took the job.").color(theme::POS));
            }
            OfferState::Rejected => {
                ui.label(RichText::new("You turned this down.").color(theme::TEXT_DIM));
            }
            _ => {
                ui.label(RichText::new("The club has found somebody else.").color(theme::TEXT_DIM));
            }
        }
    }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
