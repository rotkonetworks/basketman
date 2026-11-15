//! The Arena card on a club's page: the hall, the crowds, the boards and
//! who holds them, and for the user's own club the work on the hall and
//! the season's books.

use basketman_core::arena::{EXPANSION_SEATS, MAX_CAPACITY};
use basketman_core::command::Command;
use basketman_core::finance::Cat;
use basketman_core::league::TeamId;
use basketman_core::sponsor::{SLOTS, slot_name};
use eframe::egui::{self, Color32, FontId, RichText, Sense, Ui, vec2};

use crate::app::{App, money};
use crate::pages::dim;
use crate::theme::{self, Backdrop};

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn figure(ui: &mut Ui, label: &str, value: String) {
    ui.vertical(|ui| {
        ui.label(theme::eyebrow_text(label));
        ui.label(RichText::new(value).font(FontId::new(18.0, theme::semibold())));
    });
}

impl App {
    /// The card for club `t`. Returns what the user asked for, if anything;
    /// the caller applies it once the world is no longer borrowed.
    pub(crate) fn arena_card(&self, ui: &mut Ui, bd: &Backdrop, t: TeamId) -> Option<Command> {
        let w = &self.world;
        let a = w.arena(t);
        let ledger = w.ledger(t);
        let mine = t == self.team;
        let mut command = None;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Arena");
            ui.horizontal(|ui| {
                ui.label(RichText::new(&a.name).font(FontId::new(22.0, theme::semibold())));
                // Floor, seats and trim, as the 3D view paints them.
                for c in [a.look.floor, a.look.seats, a.look.trim] {
                    let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    ui.painter().rect_filled(r, 3.0, rgb(c));
                }
            });
            let stands = format!(
                "{} tier{}{}",
                if a.look.two_tiers { "Two" } else { "One" },
                if a.look.two_tiers { "s" } else { "" },
                if a.look.behind_baskets {
                    ", seats behind the baskets"
                } else {
                    ", sides only"
                }
            );
            ui.label(dim(stands));
            ui.add_space(6.0);
            let full =
                |att: u32| format!("{} ({}%)", money(att as i64), att * 100 / a.capacity.max(1));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 34.0;
                figure(ui, "Capacity", money(a.capacity as i64));
                figure(
                    ui,
                    "Condition",
                    format!("{} ({:.0}%)", a.condition_words(), a.condition * 100.0),
                );
                figure(ui, "Ticket", money(w.ticket_price(t)));
                if ledger.games > 0 {
                    figure(ui, "Average crowd", full(ledger.average_attendance()));
                    figure(ui, "Last crowd", full(ledger.last_attendance));
                } else {
                    figure(ui, "Expected crowd", full(w.expected_attendance(t)));
                }
                figure(ui, "Gate this season", money(ledger.total(Cat::Gate)));
                figure(
                    ui,
                    "Sponsors this season",
                    money(ledger.total(Cat::Sponsorship)),
                );
            });
            if a.pending_seats > 0 {
                ui.label(dim(format!(
                    "{} more seats are being built and open next season.",
                    money(a.pending_seats as i64)
                )));
            }

            ui.add_space(10.0);
            theme::eyebrow(
                ui,
                &format!(
                    "Boards / {} of {SLOTS} sold, {} a season",
                    SLOTS - w.free_slots(t).len(),
                    money(w.sponsor_income(t))
                ),
            );
            ui.add_space(4.0);
            let mut deals: Vec<_> = w.econ.deals.iter().filter(|d| d.club == t).collect();
            deals.sort_by_key(|d| d.slots.first().copied());
            if deals.is_empty() {
                ui.label(dim(
                    "No sponsor has a board. The league's own board runs everywhere.",
                ));
            }
            for d in deals {
                let s = &w.econ.sponsors[d.sponsor];
                ui.horizontal_wrapped(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                    ui.painter().rect_filled(r, 2.0, rgb(s.bg));
                    ui.painter().rect_stroke(
                        r,
                        2.0,
                        egui::Stroke::new(1.0, rgb(s.fg)),
                        egui::StrokeKind::Inside,
                    );
                    ui.label(RichText::new(&s.name).family(theme::semibold()));
                    let names: Vec<String> =
                        d.slots.iter().map(|&i| slot_name(i as usize)).collect();
                    ui.label(dim(format!(
                        "{} a season until season {}. {}",
                        money(d.fee),
                        d.ends + 1,
                        names.join(", ")
                    )));
                });
            }
            let free = w.free_slots(t);
            if !free.is_empty() {
                let names: Vec<String> = free.iter().map(|&i| slot_name(i as usize)).collect();
                ui.label(dim(format!("{} boards show the league board.", free.len())))
                    .on_hover_text(names.join("\n"));
            }

            if mine {
                ui.add_space(10.0);
                theme::eyebrow(ui, "The hall");
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    let blocked = w.works_blocked(t, false);
                    let label = format!("Renovate for {}", money(a.renovation_cost()));
                    let hint = blocked.clone().unwrap_or_else(|| {
                        "Restores the hall by 40 points of condition. Better condition fills \
                         more seats and lets the club charge more."
                            .to_string()
                    });
                    if ui
                        .add_enabled(blocked.is_none(), egui::Button::new(label))
                        .on_hover_text(hint)
                        .on_disabled_hover_text(blocked.unwrap_or_default())
                        .clicked()
                    {
                        command = Some(Command::Renovate);
                    }
                    let blocked = w.works_blocked(t, true);
                    let label = format!(
                        "Expand by {} seats for {}",
                        money(EXPANSION_SEATS as i64),
                        money(a.expansion_cost())
                    );
                    let hint = format!(
                        "Takes effect next season. The site tops out at {} seats.",
                        money(MAX_CAPACITY as i64)
                    );
                    if ui
                        .add_enabled(blocked.is_none(), egui::Button::new(label))
                        .on_hover_text(hint)
                        .on_disabled_hover_text(blocked.unwrap_or_default())
                        .clicked()
                    {
                        command = Some(Command::Expand);
                    }
                });
                if w.spending_frozen(t) {
                    ui.label(
                        RichText::new(
                            "The club is past the bank's overdraft limit: the board has frozen \
                             bids, hires and work on the hall.",
                        )
                        .color(theme::WARN),
                    );
                }

                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    let tax = w.tax_view(t);
                    let mut line = format!(
                        "Season {} books: budget {}, net {} so far, ticket {}.",
                        w.season + 1,
                        money(w.teams[t].budget),
                        money(ledger.net()),
                        money(w.ticket_price(t)),
                    );
                    if tax.excess > 0 {
                        line +=
                            &format!(" Payroll is over the cap: {} tax due.", money(tax.to_pay()));
                    }
                    ui.label(dim(line));
                    if ui.link("Open the Finances page").clicked() {
                        crate::finances::request_open(ui.ctx());
                    }
                });
            }
        });
        command
    }
}
