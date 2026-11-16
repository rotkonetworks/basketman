//! The squad screen's two panels: the roster as a list that fits any width
//! (the header stays where it is while the rows scroll), and the key facts
//! of the player picked in the scene.

use basketman_core::command::Command;
use basketman_core::player::{Attr, PlayerId};
use basketman_core::training::{Intensity, TrainingFocus};
use eframe::egui::{
    self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};

use crate::app::{App, name, rating};
use crate::theme::{self, Backdrop};

struct Col {
    label: &'static str,
    width: f32,
    right: bool,
}

fn columns(narrow: bool) -> Vec<Col> {
    let c = |label, width, right| Col {
        label,
        width,
        right,
    };
    if narrow {
        vec![
            c("Player", 0.0, false),
            c("Pos", 40.0, false),
            c("Ovr", 38.0, true),
            c("Fit", 52.0, true),
        ]
    } else {
        vec![
            c("Player", 0.0, false),
            c("Pos", 42.0, false),
            c("Age", 34.0, true),
            c("Ovr", 38.0, true),
            c("Fit", 56.0, true),
            c("Morale", 66.0, false),
        ]
    }
}

/// Where the cells of a row start, from the left edge of `row`.
fn cell_x(row: Rect, cols: &[Col]) -> Vec<f32> {
    let fixed: f32 = cols.iter().map(|c| c.width).sum();
    let first = (row.width() - fixed - 16.0).max(60.0);
    let mut x = row.left() + 10.0;
    cols.iter()
        .map(|c| {
            let at = x;
            x += if c.width == 0.0 { first } else { c.width };
            at
        })
        .collect()
}

impl App {
    /// The roster: a fixed header and the rows under it. Tapping a row picks
    /// the player; the scene rings him.
    pub(crate) fn squad_roster(&mut self, ui: &mut Ui, order: &[PlayerId], narrow: bool) {
        let cols = columns(narrow);
        let row_h = if narrow { 44.0 } else { 34.0 };
        ui.horizontal(|ui| {
            theme::eyebrow(
                ui,
                &if theme::side() {
                    format!("{} players", order.len())
                } else {
                    format!("First team / {} players", order.len())
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(egui::Button::new("Full table"))
                    .on_hover_text("Every attribute, contract and training setting")
                    .clicked()
                {
                    self.sh.squad_full = true;
                }
            });
        });
        // Header, outside the scrolling part.
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
        let xs = cell_x(head, &cols);
        for (c, &x) in cols.iter().zip(&xs) {
            let w = if c.width == 0.0 { 100.0 } else { c.width };
            let (at, anchor) = if c.right {
                (x + w - 8.0, Align2::RIGHT_CENTER)
            } else {
                (x, Align2::LEFT_CENTER)
            };
            ui.painter().text(
                pos2(at, head.center().y),
                anchor,
                c.label.to_uppercase(),
                FontId::new(crate::theme::fs(10.0), egui::FontFamily::Monospace),
                theme::TEXT_MUTE,
            );
        }
        ui.painter().hline(
            head.x_range(),
            head.bottom(),
            Stroke::new(1.0, theme::LINE_2),
        );
        egui::ScrollArea::vertical()
            .id_salt(("roster", narrow))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for &pid in order {
                    let (row, resp) =
                        ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
                    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                    let p = &self.world.players[pid as usize];
                    let on = self.sh.pick == Some(pid);
                    let paint = ui.painter().with_clip_rect(row);
                    if on {
                        paint.rect_filled(
                            row.shrink2(vec2(2.0, 1.0)),
                            8.0,
                            theme::ACCENT.gamma_multiply(0.14),
                        );
                        paint.rect_stroke(
                            row.shrink2(vec2(2.0, 1.0)),
                            8.0,
                            Stroke::new(1.0, theme::ACCENT),
                            StrokeKind::Inside,
                        );
                    } else if resp.hovered() {
                        paint.rect_filled(row.shrink2(vec2(2.0, 1.0)), 8.0, theme::ink(10));
                    }
                    let xs = cell_x(row, &cols);
                    let cy = row.center().y;
                    let fit = format!("{:.0}%", p.condition * 100.0);
                    let (fit, fit_color) = match &p.injury {
                        Some(i) => (format!("out {}w", i.weeks_left), theme::WARN),
                        None if p.condition >= 0.95 => (fit, theme::POS),
                        None if p.condition < 0.85 => (fit, theme::WARN),
                        None => (fit, theme::TEXT),
                    };
                    let mood_color = match p.morale {
                        m if m >= 0.62 => theme::POS,
                        m if m < 0.42 => theme::WARN,
                        _ => theme::TEXT_DIM,
                    };
                    for ((c, &x), n) in cols.iter().zip(&xs).zip(0..) {
                        let w = if c.width == 0.0 { 100.0 } else { c.width };
                        let (at, anchor) = if c.right {
                            (x + w - 8.0, Align2::RIGHT_CENTER)
                        } else {
                            (x, Align2::LEFT_CENTER)
                        };
                        let (text, font, color): (String, FontId, Color32) = match c.label {
                            "Player" => (
                                p.name.clone(),
                                FontId::new(14.0, theme::semibold()),
                                theme::TEXT,
                            ),
                            "Pos" => (
                                p.position.label().to_string(),
                                FontId::new(12.5, theme::mono_semibold()),
                                theme::ACCENT,
                            ),
                            "Age" => (
                                p.age.to_string(),
                                FontId::new(12.5, egui::FontFamily::Monospace),
                                theme::TEXT_DIM,
                            ),
                            "Ovr" => (
                                format!("{:.0}", p.overall()),
                                FontId::new(13.0, theme::mono_semibold()),
                                theme::TEXT,
                            ),
                            "Fit" => (
                                fit.clone(),
                                FontId::new(12.5, egui::FontFamily::Monospace),
                                fit_color,
                            ),
                            _ => (
                                p.mood().to_string(),
                                FontId::new(12.5, egui::FontFamily::Proportional),
                                mood_color,
                            ),
                        };
                        let _ = n;
                        paint.text(pos2(at, cy), anchor, text, font, color);
                    }
                    if resp.clicked() {
                        self.sh.pick = Some(pid);
                        self.sh.squad_detail = true;
                    }
                }
            });
    }

    /// The key facts of one player, his training and what can be done with
    /// him from here. The whole profile is a click away.
    pub(crate) fn squad_detail(&mut self, ui: &mut Ui, bd: &Backdrop, pid: PlayerId, narrow: bool) {
        let p = self.world.players[pid as usize].clone();
        let mut new_focus = p.training;
        let mut new_intensity = p.intensity;
        let media_now = p.renown.media;
        let mut new_media = media_now;
        let mut open_profile = false;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            let also: Vec<&str> = p.also_plays().iter().map(|x| x.label()).collect();
            let pos = std::iter::once(p.position.label())
                .chain(also)
                .collect::<Vec<_>>()
                .join("/");
            theme::eyebrow(ui, &format!("{pos} / age {}", p.age));
            ui.horizontal_wrapped(|ui| {
                crate::signature_ui::name_with_mark(
                    ui,
                    pid,
                    RichText::new(&p.name).font(FontId::new(22.0, theme::semibold())),
                    &p.signatures,
                );
                crate::fame_ui::fame_chip(ui, p.renown.fame);
            });
            let kind = p.personality.kind();
            ui.label(RichText::new(kind.label()).color(if kind.is_concern() {
                theme::WARN
            } else {
                theme::TEXT_DIM
            }));
            ui.add_space(6.0);
            egui::Grid::new(("facts", pid))
                .spacing(vec2(18.0, 5.0))
                .show(ui, |ui| {
                    let kv = |ui: &mut Ui, k: &str, v: RichText| {
                        ui.label(theme::eyebrow_text(k));
                        ui.label(v);
                    };
                    kv(ui, "Overall", rating(p.overall(), false));
                    kv(
                        ui,
                        "Height",
                        theme::mono(format!("{:.0} cm", p.body.height_cm)),
                    );
                    ui.end_row();
                    kv(
                        ui,
                        "Weight",
                        theme::mono(format!("{:.0} kg", p.body.weight_kg)),
                    );
                    kv(
                        ui,
                        "Wingspan",
                        theme::mono(format!("{:.0} cm", p.body.wingspan_cm)),
                    );
                    ui.end_row();
                    let fit = match &p.injury {
                        Some(i) => {
                            theme::mono(format!("out {} wk", i.weeks_left)).color(theme::WARN)
                        }
                        None => theme::mono(format!("{:.0}%", p.condition * 100.0)),
                    };
                    kv(ui, "Fit", fit);
                    kv(ui, "Sharp", theme::mono(format!("{:.0}%", p.sharp * 100.0)));
                    ui.end_row();
                    kv(ui, "Morale", RichText::new(p.mood()));
                    kv(
                        ui,
                        "Contract",
                        theme::mono(format!(
                            "{} x {}y",
                            crate::app::money(p.contract.salary as i64),
                            p.contract.years_left
                        )),
                    );
                    ui.end_row();
                });
        });
        ui.add_space(6.0);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Attributes");
            ui.add_space(4.0);
            let focus = p.training.attrs();
            let per_row = if narrow || ui.available_width() < 330.0 {
                2
            } else {
                3
            };
            egui::Grid::new(("attrs", pid))
                .spacing(vec2(14.0, 4.0))
                .show(ui, |ui| {
                    for (n, a) in Attr::ALL.into_iter().enumerate() {
                        ui.label(RichText::new(a.label()).size(12.5).color(theme::TEXT_DIM));
                        ui.label(rating(p.attrs[a], focus.contains(&a)));
                        if (n + 1) % per_row == 0 {
                            ui.end_row();
                        }
                    }
                });
            ui.label(
                RichText::new("Underlined: what his training focus works on.")
                    .size(crate::theme::fs(11.5))
                    .color(theme::TEXT_MUTE),
            );
        });
        ui.add_space(6.0);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Training and media");
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt(("focus", pid))
                    .width(150.0)
                    .selected_text(new_focus.label())
                    .show_ui(ui, |ui| {
                        for f in TrainingFocus::ALL {
                            ui.selectable_value(&mut new_focus, f, f.label());
                        }
                    });
                egui::ComboBox::from_id_salt(("intensity", pid))
                    .width(90.0)
                    .selected_text(new_intensity.label())
                    .show_ui(ui, |ui| {
                        for i in Intensity::ALL {
                            ui.selectable_value(&mut new_intensity, i, i.label());
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(theme::eyebrow_text("Media work"));
                new_media = crate::fame_ui::media_combo(ui, ("media", pid), media_now);
            });
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if theme::primary_button(ui, true, "Full profile") {
                open_profile = true;
            }
            crate::pages::compare_button(ui, &mut self.compare, pid);
            if ui.button("Compare players").clicked() {
                if self.compare.len() < 2 {
                    let mut best = self.world.teams[self.team].roster.clone();
                    best.sort_by(|&a, &b| {
                        let ov = |q: PlayerId| self.world.players[q as usize].overall();
                        ov(b).total_cmp(&ov(a))
                    });
                    self.compare = best.into_iter().take(2).collect();
                }
                self.go(crate::app::Tab::Compare);
            }
        });
        if (new_focus, new_intensity) != (p.training, p.intensity) {
            let _ = self.world.apply(Command::SetTraining {
                player: pid,
                focus: new_focus,
                intensity: new_intensity,
            });
        }
        if new_media != media_now {
            let _ = self.world.apply(Command::SetMedia {
                player: pid,
                level: new_media,
            });
        }
        if open_profile {
            crate::nav::follow(ui, crate::nav::Link::Player(pid));
        }
        let _ = name("");
    }
}
