//! The first screen: the logo, and the choice of club to take over.

use basketman_core::league::World;
use eframe::egui::{self, Color32, FontId, Pos2, RichText, Sense, Shape, Stroke, Ui, vec2};

use crate::app::{App, DIVISIONS, TEAMS};
use crate::theme::{self, Backdrop};

const DIVISION_NAMES: [&str; 3] = ["First Division", "Second Division", "Third Division"];

const BALL: Color32 = Color32::from_rgb(0xE8, 0x6A, 0x1C);

/// A club as the title screen lists it: name and how strong its best eight
/// are (mean overall).
pub(crate) struct Candidate {
    pub name: String,
    pub strength: f32,
    /// 0 is the First Division.
    pub division: usize,
}

pub(crate) fn candidates(seed: u64) -> Vec<Candidate> {
    let w = World::generate_tiers(seed, TEAMS, DIVISIONS);
    w.teams
        .iter()
        .map(|t| {
            let mut ov: Vec<f32> = t
                .roster
                .iter()
                .map(|&p| w.players[p as usize].overall())
                .collect();
            ov.sort_by(|a, b| b.total_cmp(a));
            Candidate {
                name: t.name.clone(),
                strength: ov.iter().take(8).sum::<f32>() / 8.0,
                division: t.division,
            }
        })
        .collect()
}

/// The mark: a basketball, seams and all.
pub(crate) fn ball(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let (c, r) = (rect.center(), size / 2.0);
    let painter = ui.painter_at(rect.expand(1.0));
    painter.circle_filled(c, r, BALL);
    let seam = Stroke::new((size / 22.0).max(1.0), theme::TEXT);
    painter.line_segment([c - vec2(r, 0.0), c + vec2(r, 0.0)], seam);
    painter.line_segment([c - vec2(0.0, r), c + vec2(0.0, r)], seam);
    for side in [-1.0, 1.0f32] {
        let curve: Vec<Pos2> = (0..=24)
            .map(|i| {
                let t = i as f32 / 12.0 - 1.0;
                c + vec2(side * r * (0.62 - 0.30 * (1.0 - t * t)), t * r * 0.79)
            })
            .collect();
        painter.add(Shape::line(curve, seam));
    }
    painter.circle_stroke(c, r, seam);
}

impl App {
    pub(crate) fn title(&mut self, ui: &mut Ui, bd: &Backdrop) {
        if self.candidates.0 != self.seed || self.candidates.1.is_empty() {
            self.candidates = (self.seed, candidates(self.seed));
            self.pick = self.pick.min(self.candidates.1.len() - 1);
        }
        let mut order: Vec<usize> = (0..self.candidates.1.len()).collect();
        order.sort_by(|&a, &b| {
            let (x, y) = (&self.candidates.1[a], &self.candidates.1[b]);
            x.division
                .cmp(&y.division)
                .then(y.strength.total_cmp(&x.strength))
        });
        let mut start = false;

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space((ui.available_height() * 0.10).clamp(24.0, 110.0));
                    ball(ui, 104.0);
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new("basketman")
                            .font(FontId::new(60.0, theme::mono_semibold()))
                            .color(theme::TEXT),
                    );
                    ui.label(
                        RichText::new("Run the club. Coach the game.")
                            .size(17.0)
                            .color(theme::TEXT_DIM),
                    );
                    ui.add_space(30.0);

                    if self.career {
                        let club = self.world.teams[self.team].name.clone();
                        let label = format!("Continue with {club}");
                        if theme::primary_button(ui, true, &label) {
                            self.started = true;
                        }
                        let w = &self.world;
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(format!(
                                "Season {} / round {} of {}",
                                w.season + 1,
                                w.round.min(w.rounds()),
                                w.rounds()
                            ))
                            .color(theme::TEXT_DIM),
                        );
                        ui.add_space(22.0);
                    }
                    if let Some(why) = &self.load_failed {
                        ui.label(
                            RichText::new(
                                "Your saved career could not be loaded (older version?). \
                                 Starting a new career replaces it.",
                            )
                            .color(theme::WARN),
                        )
                        .on_hover_text(why);
                        ui.add_space(22.0);
                    }

                    ui.allocate_ui(
                        vec2(560.0_f32.min(ui.available_width() - 32.0), 0.0),
                        |ui| {
                            bd.card(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                    ui.horizontal(|ui| {
                                        theme::eyebrow(ui, "New career / pick your club");
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.add(
                                                    egui::DragValue::new(&mut self.seed)
                                                        .prefix("world "),
                                                )
                                                .on_hover_text(
                                                    "Each number is a different league: other \
                                                 clubs, other players.",
                                                );
                                            },
                                        );
                                    });
                                    ui.label(
                                        RichText::new(
                                            "Three divisions. Strongest squad first in each. A \
                                         weaker club means a more patient board.",
                                        )
                                        .color(theme::TEXT_DIM),
                                    );
                                    ui.add_space(8.0);
                                    let best = self.candidates.1[order[0]].strength;
                                    let mut shown_division = usize::MAX;
                                    let mut rank = 0;
                                    for &t in order.iter() {
                                        let c = &self.candidates.1[t];
                                        if c.division != shown_division {
                                            shown_division = c.division;
                                            rank = 0;
                                            ui.add_space(8.0);
                                            ui.label(theme::eyebrow_text(DIVISION_NAMES[c.division.min(2)]));
                                            let note = match c.division {
                                                0 => "The top. The best squads, the biggest halls, the playoffs.",
                                                1 => "A step down: weaker squads, smaller halls and less money. Two clubs go up each year.",
                                                _ => "Starting at the bottom is the long road. The squads are weak, the halls small and the money tight. Finish in the top two to go up, and keep climbing for years to reach the First Division.",
                                            };
                                            ui.label(RichText::new(note).size(12.5).color(theme::TEXT_DIM));
                                            ui.add_space(4.0);
                                        }
                                        rank += 1;
                                        let (rect, response) = ui.allocate_exact_size(
                                            vec2(ui.available_width(), 34.0),
                                            Sense::click(),
                                        );
                                        let painter = ui.painter_at(rect);
                                        let picked = self.pick == t;
                                        if picked {
                                            painter.rect_filled(
                                                rect,
                                                6.0,
                                                theme::ACCENT.gamma_multiply(0.12),
                                            );
                                        } else if response.hovered() {
                                            painter.rect_filled(
                                                rect,
                                                6.0,
                                                theme::LINE.gamma_multiply(0.6),
                                            );
                                        }
                                        let mid = rect.center().y;
                                        painter.text(
                                            egui::pos2(rect.left() + 10.0, mid),
                                            egui::Align2::LEFT_CENTER,
                                            format!("{:>2}", rank),
                                            FontId::new(12.0, egui::FontFamily::Monospace),
                                            theme::TEXT_MUTE,
                                        );
                                        painter.text(
                                            egui::pos2(rect.left() + 42.0, mid),
                                            egui::Align2::LEFT_CENTER,
                                            &c.name,
                                            FontId::new(
                                                15.0,
                                                if picked {
                                                    theme::semibold()
                                                } else {
                                                    egui::FontFamily::Proportional
                                                },
                                            ),
                                            if picked { theme::ACCENT } else { theme::TEXT },
                                        );
                                        // Squad strength as a bar against the best.
                                        let full = (rect.width() - 235.0).clamp(30.0, 150.0);
                                        let x0 = rect.right() - full - 54.0;
                                        let share =
                                            ((c.strength - 6.0) / (best - 6.0)).clamp(0.05, 1.0);
                                        let bar = |w: f32, color| {
                                            painter.rect_filled(
                                                egui::Rect::from_min_size(
                                                    egui::pos2(x0, mid - 3.0),
                                                    vec2(w, 6.0),
                                                ),
                                                3.0,
                                                color,
                                            );
                                        };
                                        bar(full, theme::LINE);
                                        bar(
                                            full * share,
                                            if picked {
                                                theme::ACCENT
                                            } else {
                                                theme::TEXT_MUTE
                                            },
                                        );
                                        painter.text(
                                            egui::pos2(rect.right() - 10.0, mid),
                                            egui::Align2::RIGHT_CENTER,
                                            format!("{:.1}", c.strength),
                                            FontId::new(12.5, egui::FontFamily::Monospace),
                                            theme::TEXT_DIM,
                                        );
                                        if response
                                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                                            .clicked()
                                        {
                                            self.pick = t;
                                            self.confirm_new = false;
                                        }
                                    }
                                    ui.add_space(12.0);
                                    let label = format!(
                                        "Take charge of {}",
                                        self.candidates.1[self.pick].name
                                    );
                                    if !(self.career || self.load_failed.is_some()) {
                                        start = theme::primary_button(ui, true, &label);
                                    } else if self.confirm_new {
                                        // Two steps, so a stray click cannot
                                        // end the career that is saved.
                                        let sure = format!(
                                            "This replaces your saved career. Take charge of {}?",
                                            self.candidates.1[self.pick].name
                                        );
                                        start = theme::primary_button(ui, true, &sure);
                                        if ui.button("Cancel").clicked() {
                                            self.confirm_new = false;
                                        }
                                    } else if theme::primary_button(ui, true, &label) {
                                        self.confirm_new = true;
                                    }
                                });
                            });
                        },
                    );
                    ui.add_space(40.0);
                });
            });
        if start {
            self.begin(self.pick);
        }
    }
}
