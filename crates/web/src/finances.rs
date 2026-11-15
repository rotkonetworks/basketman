//! The Finances screen: the club's money in one place. Balance and debt,
//! the board's budgets, the books by category, a chart of the season week by
//! week, a projection to the end of it, the ticket price, the facilities, the
//! payroll against the cap, past seasons, and a look at the other clubs.
//!
//! Drawing only reads the world; what the user asks for comes back as
//! `Command`s that `finances` applies once the drawing is done.

use basketman_core::books::FacilityKind;
use basketman_core::command::{Command, CommandError};
use basketman_core::facility::{BUILD_ROUNDS, MAX_LEVEL, build_cost, effect_words, running_cost};
use basketman_core::finance::{Cat, Ledger};
use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Shape, Stroke, Ui, vec2};

use crate::app::{App, GAP, header_row, money};
use crate::pages::dim;
use crate::theme::{self, Backdrop};

/// Commands asked for while drawing, with the note slot each one's answer
/// goes to.
type Cmds = std::cell::RefCell<Vec<(&'static str, Command)>>;

/// Wide enough for two columns of cards.
const WIDE: f32 = 940.0;

fn note_id(key: &str) -> egui::Id {
    egui::Id::unique(("finance_note", key))
}

/// The Arena card asks for this screen; the club page opens it.
fn open_id() -> egui::Id {
    egui::Id::unique("open_finances")
}

pub(crate) fn request_open(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(open_id(), true));
}

pub(crate) fn take_open_request(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.remove_temp::<bool>(open_id())) == Some(true)
}

fn note(ui: &Ui, key: &str) -> String {
    ui.data(|d| d.get_temp::<String>(note_id(key)))
        .unwrap_or_default()
}

fn signed(v: i64) -> RichText {
    let color = match v {
        v if v < 0 => theme::WARN,
        v if v > 0 => theme::POS,
        _ => theme::TEXT_MUTE,
    };
    theme::mono(money(v)).color(color)
}

/// "1.2m", "340k", "-80k": axis labels.
fn short(v: i64) -> String {
    let a = v.abs();
    let s = if a >= 1_000_000 {
        format!("{:.1}m", a as f32 / 1_000_000.0)
    } else if a >= 1000 {
        format!("{}k", a / 1000)
    } else {
        a.to_string()
    };
    if v < 0 { format!("-{s}") } else { s }
}

/// The next round figure above `v` of the form 1, 2, 5 times a power of ten.
fn nice_max(v: i64) -> i64 {
    let v = v.max(1000);
    let mut unit = 1;
    while unit * 10 <= v {
        unit *= 10;
    }
    for m in [1, 2, 5, 10] {
        if unit * m >= v {
            return unit * m;
        }
    }
    unit * 10
}

/// A thin horizontal meter: `frac` of the width filled, a mark at `mark`.
fn meter(ui: &mut Ui, frac: f32, color: Color32, mark: Option<f32>) {
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width().min(420.0), 8.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, theme::LINE);
    let mut fill = rect;
    fill.set_width(rect.width() * frac.clamp(0.0, 1.0));
    p.rect_filled(fill, 4.0, color);
    if let Some(m) = mark {
        let x = rect.left() + rect.width() * m.clamp(0.0, 1.0);
        p.vline(x, rect.y_range().expand(3.0), Stroke::new(2.0, theme::TEXT));
    }
}

fn figure(ui: &mut Ui, label: &str, value: String) {
    ui.vertical(|ui| {
        ui.label(theme::eyebrow_text(label));
        ui.label(RichText::new(value).font(FontId::new(18.0, theme::semibold())));
    });
}

/// Run `f` on two columns when there is room, one under the other if not.
fn two(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    if ui.available_width() > WIDE {
        ui.columns(2, |cols| {
            left(&mut cols[0]);
            right(&mut cols[1]);
        });
    } else {
        left(ui);
        ui.add_space(GAP);
        right(ui);
    }
}

impl App {
    pub(crate) fn finances(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let cmds: Cmds = Default::default();

        self.debt_banner(ui, bd);
        self.balance_tiles(ui, bd);
        ui.add_space(GAP);
        two(
            ui,
            |ui| self.budgets_card(ui, bd, &cmds),
            |ui| self.payroll_card(ui, bd),
        );
        ui.add_space(GAP);
        self.chart_card(ui, bd);
        ui.add_space(GAP);
        two(
            ui,
            |ui| self.merch_card(ui, bd),
            |ui| self.fans_card(ui, bd),
        );
        ui.add_space(GAP);
        two(
            ui,
            |ui| self.books_card(ui, bd),
            |ui| self.projection_card(ui, bd),
        );
        ui.add_space(GAP);
        two(
            ui,
            |ui| self.ticket_card(ui, bd, &cmds),
            |ui| self.facilities_card(ui, bd, &cmds),
        );
        ui.add_space(GAP);
        self.history_card(ui, bd);
        ui.add_space(GAP);
        self.league_money_card(ui, bd);

        for (key, cmd) in cmds.into_inner() {
            let text = match self.world.apply(cmd) {
                Ok(out) => out.note.unwrap_or_default(),
                Err(CommandError::Refused(why)) => why,
                Err(CommandError::NothingToDo) => String::new(),
                Err(other) => other.to_string(),
            };
            ui.data_mut(|d| d.insert_temp(note_id(key), text));
        }
    }

    fn debt_banner(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        let me = self.team;
        if !w.in_overdraft(me) {
            return;
        }
        let text = if w.spending_frozen(me) {
            format!(
                "The club owes {}, which is past the {} the bank allows. The board has frozen \
                 bids, hires, work on the hall and new facilities, and expects a sale. \
                 Interest is {} a round.",
                money(-w.teams[me].budget),
                money(w.overdraft_limit(me)),
                money(w.interest_due(me)),
            )
        } else {
            format!(
                "The club is overdrawn by {}. The bank allows up to {}. Interest is {} a \
                 round until the balance is back above zero.",
                money(-w.teams[me].budget),
                money(w.overdraft_limit(me)),
                money(w.interest_due(me)),
            )
        };
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Overdraft");
            ui.label(RichText::new(text).color(theme::WARN));
        });
        ui.add_space(GAP);
    }

    fn balance_tiles(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        let me = self.team;
        let l = w.ledger(me);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            theme::stat(
                ui,
                bd,
                "Wage budget left",
                &money(w.wage_left(me)),
                &format!("of {}", money(w.books(me).wage_budget)),
            );
            theme::stat(
                ui,
                bd,
                "Transfer budget left",
                &money(w.transfer_left(me)),
                &format!("of {}", money(w.books(me).transfer_budget)),
            );
            let tax = w.tax_view(me);
            theme::stat(
                ui,
                bd,
                "Payroll",
                &money(tax.payroll),
                &if tax.excess > 0 {
                    format!("{} over the cap", money(tax.excess))
                } else {
                    format!("{} under the cap", money(tax.cap - tax.payroll))
                },
            );
            theme::stat(
                ui,
                bd,
                "Ticket price",
                &money(w.ticket_price(me)),
                &if l.games > 0 {
                    format!("{} a game on average", money(l.average_attendance() as i64))
                } else {
                    "no home game yet".to_string()
                },
            );
        });
    }

    // ---- budgets ---------------------------------------------------------

    fn budgets_card(&self, ui: &mut Ui, bd: &Backdrop, cmds: &Cmds) {
        let w = &self.world;
        let me = self.team;
        let b = w.books(me);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Board budgets / this season");
            ui.add_space(6.0);
            let committed = w.wage_committed(me);
            ui.label(RichText::new("Wages").family(theme::semibold()));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 30.0;
                figure(ui, "Budget", money(b.wage_budget));
                figure(ui, "Bill for the season", money(committed));
                figure(ui, "Paid so far", money(w.wages_paid(me)));
                figure(ui, "Left", money(w.wage_left(me)));
            });
            ui.add_space(4.0);
            let over = w.wage_left(me) < 0;
            meter(
                ui,
                committed as f32 / b.wage_budget.max(1) as f32,
                if over { theme::WARN } else { theme::ACCENT },
                None,
            );
            if over {
                ui.label(
                    RichText::new(
                        "The wage bill is over its budget: no new signings or raises until it \
                         is back inside. Nobody has to go.",
                    )
                    .color(theme::WARN),
                );
            }
            ui.add_space(12.0);
            ui.label(RichText::new("Transfers").family(theme::semibold()));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 30.0;
                figure(ui, "Budget", money(b.transfer_budget));
                figure(ui, "Spent", money(b.transfer_spent.max(0)));
                figure(ui, "Offers waiting", money(w.pending_bids(me)));
                figure(ui, "Left", money(w.transfer_left(me)));
            });
            ui.add_space(4.0);
            let used = b.transfer_spent.max(0) + w.pending_bids(me);
            meter(
                ui,
                used as f32 / b.transfer_budget.max(1) as f32,
                theme::ACCENT,
                None,
            );
            ui.add_space(12.0);
            ui.label(RichText::new("Move money between them").family(theme::semibold()));
            let to_wages = w.budget_move_limit(me, true);
            let to_transfers = w.budget_move_limit(me, false);
            let amount_id = egui::Id::unique("finance_move_amount");
            let mut amount: i64 = ui.data(|d| d.get_temp(amount_id)).unwrap_or(10_000);
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::DragValue::new(&mut amount)
                        .speed(1000)
                        .range(1000..=to_wages.max(to_transfers).max(1000)),
                );
                for (label, towards, most) in [
                    ("Move to wages", true, to_wages),
                    ("Move to transfers", false, to_transfers),
                ] {
                    let ok = amount <= most && most > 0;
                    if ui
                        .add_enabled(ok, egui::Button::new(label))
                        .on_disabled_hover_text(format!(
                            "The board lets you move at most {} that way.",
                            money(most)
                        ))
                        .clicked()
                    {
                        cmds.borrow_mut().push((
                            "budget",
                            Command::MoveBudget {
                                to_wages: towards,
                                amount,
                            },
                        ));
                    }
                }
            });
            ui.data_mut(|d| d.insert_temp(amount_id, amount));
            ui.label(dim(format!(
                "Up to {} can go to wages and {} to transfers. The board keeps half of what it \
                 first set, and what is already spent or promised stays where it is.",
                money(to_wages),
                money(to_transfers)
            )));
            let n = note(ui, "budget");
            if !n.is_empty() {
                ui.label(RichText::new(n).color(theme::WARN));
            }
        });
    }

    // ---- payroll and tax -------------------------------------------------

    fn payroll_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        let me = self.team;
        let tax = w.tax_view(me);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Payroll and the salary cap");
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 30.0;
                figure(ui, "Payroll", money(tax.payroll));
                figure(ui, "Cap", money(tax.cap));
                figure(
                    ui,
                    if tax.excess > 0 {
                        "Over by"
                    } else {
                        "Room under"
                    },
                    money(if tax.excess > 0 {
                        tax.excess
                    } else {
                        tax.cap - tax.payroll
                    }),
                );
            });
            ui.add_space(4.0);
            let top = (tax.payroll.max(tax.cap) as f32 * 1.15).max(1.0);
            meter(
                ui,
                tax.payroll as f32 / top,
                if tax.excess > 0 {
                    theme::WARN
                } else {
                    theme::ACCENT
                },
                Some(tax.cap as f32 / top),
            );
            ui.label(dim("The dark mark is the cap."));
            ui.add_space(8.0);
            if tax.excess > 0 {
                ui.label(
                    RichText::new(format!(
                        "At this payroll the luxury tax is {} a year, paid in two halves of {}. \
                         {} halves are still to come this season, {} in all. The next unit over \
                         the cap costs {}%.",
                        money(tax.annual),
                        money(tax.per_checkpoint),
                        tax.checkpoints_left,
                        money(tax.to_pay()),
                        tax.marginal,
                    ))
                    .color(theme::WARN),
                );
                ui.label(dim(
                    "The tax is shared equally among the clubs at or under the cap. Getting \
                     under it before a charge saves the whole of that charge.",
                ));
            } else {
                ui.label(
                    "No luxury tax at this payroll. Clubs over the cap pay tax on the excess \
                     twice a season, at rates that climb in bands, and the clubs under it \
                     share the money out.",
                );
                if tax.checkpoints_left > 0 {
                    ui.label(dim(format!(
                        "{} charges still to come this season.",
                        tax.checkpoints_left
                    )));
                }
            }
            ui.add_space(6.0);
            ui.label(dim(format!(
                "The wage bill is higher than the payroll: reserves are paid too ({} a year \
                 for the whole squad).",
                money(w.squad_wages(me))
            )));
        });
    }

    // ---- chart -----------------------------------------------------------

    fn chart_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        let me = self.team;
        let now = w.ledger(me);
        let current = !now.weeks().is_empty();
        let (weeks, start, slots, caption) = if current {
            (
                now.weeks(),
                now.start,
                w.rounds().max(now.weeks().len()),
                format!("Season {}, round by round", w.season + 1),
            )
        } else if let Some(last) = w.econ.history.last().map(|h| &h[me]) {
            (
                last.weeks(),
                last.start,
                last.weeks().len(),
                format!(
                    "Season {} (no games yet this season)",
                    last.season as usize + 1
                ),
            )
        } else {
            (
                Vec::new(),
                now.start,
                w.rounds(),
                "No games yet".to_string(),
            )
        };
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, &format!("The season in weeks / {caption}"));
            ui.add_space(4.0);
            if weeks.is_empty() {
                ui.label(dim("The chart fills in as rounds are played."));
                return;
            }
            chart(ui, &weeks, start, slots);
        });
    }

    // ---- books -----------------------------------------------------------

    fn books_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        let me = self.team;
        let now = w.ledger(me);
        let last = w.econ.history.last().map(|h| &h[me]);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Income and spending by category");
            ui.add_space(6.0);
            egui::Grid::new("fin_books")
                .spacing(vec2(28.0, 5.0))
                .show(ui, |ui| {
                    ui.label("");
                    ui.label(theme::eyebrow_text(&format!("Season {}", w.season + 1)));
                    ui.label(theme::eyebrow_text(&match last {
                        Some(l) => format!("Season {}", l.season + 1),
                        None => "Last season".to_string(),
                    }));
                    ui.end_row();
                    let part = |ui: &mut Ui, title: &str, incomes: bool| {
                        let rows: Vec<Cat> = Cat::ALL
                            .into_iter()
                            .filter(|&c| {
                                (c.is_income() == incomes)
                                    && (now.total(c) != 0 || last.is_some_and(|l| l.total(c) != 0))
                            })
                            .collect();
                        if rows.is_empty() {
                            return;
                        }
                        ui.label(RichText::new(title).family(theme::semibold()));
                        ui.label("");
                        ui.label("");
                        ui.end_row();
                        for c in rows {
                            ui.label(dim(c.label()));
                            ui.label(signed(now.total(c)));
                            match last {
                                Some(l) => ui.label(signed(l.total(c))),
                                None => ui.label(dim("none yet")),
                            };
                            ui.end_row();
                        }
                    };
                    part(ui, "Money in", true);
                    part(ui, "Spending and other movements", false);
                    ui.label(RichText::new("Net").family(theme::semibold()));
                    ui.label(signed(now.net()));
                    match last {
                        Some(l) => ui.label(signed(l.net())),
                        None => ui.label(""),
                    };
                    ui.end_row();
                });
            ui.add_space(4.0);
            ui.label(dim(format!(
                "Earned so far this season: {}. Spent: {}.",
                money(now.income()),
                money(now.spending())
            )));
        });
    }

    fn projection_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        let p = w.projection(self.team);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Projection to the end of the season");
            ui.label(dim(
                "A forecast, not a result. It assumes the table position, ticket price and \
                 payroll stay as they are today, and it leaves out the playoffs.",
            ));
            ui.add_space(6.0);
            ui.label(dim(format!(
                "{} regular rounds and {} home games to go.",
                p.rounds_left, p.home_games_left
            )));
            ui.add_space(4.0);
            egui::Grid::new("fin_projection")
                .spacing(vec2(28.0, 5.0))
                .show(ui, |ui| {
                    ui.label(dim("Balance now"));
                    ui.label(signed(p.now));
                    ui.end_row();
                    for (label, v) in p.rows() {
                        ui.label(dim(label));
                        ui.label(signed(v));
                        ui.end_row();
                    }
                    ui.label(
                        RichText::new("Expected balance at the end").family(theme::semibold()),
                    );
                    ui.label(signed(p.closing));
                    ui.end_row();
                });
            ui.add_space(4.0);
            ui.label(dim(
                "Prize money is for the current place and is paid when the season ends. \
                 Luxury tax is what the next charges would be at today's payroll.",
            ));
        });
    }

    // ---- ticket price ----------------------------------------------------

    fn ticket_card(&self, ui: &mut Ui, bd: &Backdrop, cmds: &Cmds) {
        let w = &self.world;
        let me = self.team;
        let (low, high) = w.price_range(me);
        let fair = w.fair_price(me);
        let mut price = w.ticket_price(me);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Ticket price");
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("Price of a seat");
                ui.spacing_mut().slider_width = 240.0;
                let before = price;
                ui.add(egui::Slider::new(&mut price, low..=high).integer());
                if price != before {
                    cmds.borrow_mut()
                        .push(("price", Command::SetTicketPrice(price)));
                }
                if ui
                    .button(format!("Reset to {}", money(fair)))
                    .on_hover_text("The price the hall's standing and condition suggest.")
                    .clicked()
                    && price != fair
                {
                    cmds.borrow_mut()
                        .push(("price", Command::SetTicketPrice(fair)));
                }
            });
            let n = note(ui, "price");
            if !n.is_empty() {
                ui.label(RichText::new(n).color(theme::WARN));
            }
            ui.add_space(8.0);
            match w.price_preview(me) {
                None => {
                    ui.label(dim("There are no home games left this season."));
                }
                Some(pv) => {
                    let a = w.arena(me);
                    ui.label(format!(
                        "Next home game: {}{}.",
                        w.teams[pv.opponent].name,
                        if pv.playoff { " (playoff)" } else { "" }
                    ));
                    ui.add_space(4.0);
                    egui::Grid::new("fin_price")
                        .spacing(vec2(28.0, 5.0))
                        .show(ui, |ui| {
                            header_row(ui, &["Price", "Expected crowd", "Full", "Gate", ""]);
                            ui.end_row();
                            for (i, pt) in pv.points.iter().enumerate() {
                                let now = i == 1;
                                let mono = |s: String| {
                                    if now {
                                        theme::mono(s).color(theme::ACCENT)
                                    } else {
                                        theme::mono(s)
                                    }
                                };
                                ui.label(mono(money(pt.price)));
                                ui.label(mono(money(pt.attendance as i64)));
                                ui.label(mono(format!(
                                    "{}%",
                                    pt.attendance as u64 * 100 / a.capacity.max(1) as u64
                                )));
                                ui.label(mono(money(pt.gate)));
                                ui.label(dim(match i {
                                    0 => "a little lower",
                                    1 => "the price now",
                                    _ => "a little higher",
                                }));
                                ui.end_row();
                            }
                        });
                    ui.add_space(6.0);
                    ui.label(format!(
                        "The crowd would pay about {} a seat for this game. The price that takes \
                         the most at the gate is {}.",
                        money(pv.willing.round() as i64),
                        money(pv.best)
                    ));
                    ui.horizontal(|ui| {
                        if ui
                            .button(format!("Use {}", money(pv.best)))
                            .on_hover_text("Best for the gate at this game. It moves with results.")
                            .clicked()
                            && pv.best != price
                        {
                            cmds.borrow_mut()
                                .push(("price", Command::SetTicketPrice(pv.best)));
                        }
                    });
                }
            }
            ui.add_space(6.0);
            ui.label(dim(
                "A winning club with a loud hall can charge more and still fill it. A losing \
                 club that charges too much plays to empty rows. Charging less than the crowd \
                 would pay builds support over the season, and charging more wears it down.",
            ));
        });
    }

    // ---- facilities ------------------------------------------------------

    fn facilities_card(&self, ui: &mut Ui, bd: &Backdrop, cmds: &Cmds) {
        let w = &self.world;
        let me = self.team;
        let b = w.books(me);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Facilities");
            ui.label(dim(format!(
                "Level 3 is ordinary. Each level costs more to run, and a new level takes \
                 {BUILD_ROUNDS} rounds to build. Running all three costs {} a season.",
                money(b.facilities.yearly_running())
            )));
            ui.add_space(6.0);
            for kind in FacilityKind::ALL {
                let level = b.facilities.level(kind);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(kind.label()).family(theme::semibold()));
                    dots(ui, level);
                    ui.label(theme::mono(format!("level {level} of {MAX_LEVEL}")));
                });
                ui.label(dim(effect_words(kind, level)));
                ui.horizontal_wrapped(|ui| {
                    let refused = w.works_refused(me, kind);
                    if level < MAX_LEVEL {
                        let label = format!(
                            "Build level {} for {}",
                            level + 1,
                            money(build_cost(level + 1))
                        );
                        if ui
                            .add_enabled(refused.is_none(), egui::Button::new(label))
                            .on_hover_text(effect_words(kind, level + 1))
                            .on_disabled_hover_text(refused.clone().unwrap_or_default())
                            .clicked()
                        {
                            cmds.borrow_mut()
                                .push(("facility", Command::UpgradeFacility(kind)));
                        }
                        ui.label(dim(format!(
                            "Running cost {} a season, now {}.",
                            money(running_cost(level + 1)),
                            money(running_cost(level))
                        )));
                    } else {
                        ui.label(dim(format!(
                            "As good as it gets. Running cost {} a season.",
                            money(running_cost(level))
                        )));
                    }
                });
                if let Some(wk) = b.works.filter(|k| k.kind == kind) {
                    let left = if wk.season == w.season {
                        wk.round as i64 - w.round as i64
                    } else {
                        0
                    };
                    ui.label(
                        RichText::new(format!(
                            "Being built up to level {}: {}.",
                            wk.to,
                            if left > 0 {
                                format!("ready in {left} rounds")
                            } else {
                                "ready when the season turns".to_string()
                            }
                        ))
                        .color(theme::ACCENT),
                    );
                }
                ui.add_space(8.0);
            }
            let n = note(ui, "facility");
            if !n.is_empty() {
                ui.label(RichText::new(n).color(theme::WARN));
            }
        });
    }

    // ---- past seasons ----------------------------------------------------

    fn history_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        let me = self.team;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Past seasons");
            ui.add_space(6.0);
            if w.econ.history.is_empty() {
                ui.label(dim("The first season's books are filed when it ends."));
                return;
            }
            let years: Vec<&Ledger> = w.econ.history.iter().map(|h| &h[me]).collect();
            let rows: Vec<Cat> = Cat::ALL
                .into_iter()
                .filter(|&c| years.iter().any(|l| l.total(c) != 0))
                .collect();
            egui::ScrollArea::horizontal()
                .id_salt("fin_history_scroll")
                .show(ui, |ui| {
                    egui::Grid::new("fin_history")
                        .striped(true)
                        .spacing(vec2(26.0, 6.0))
                        .show(ui, |ui| {
                            ui.label("");
                            for l in &years {
                                ui.label(theme::eyebrow_text(&format!("Season {}", l.season + 1)));
                            }
                            ui.end_row();
                            for c in rows {
                                ui.label(dim(c.label()));
                                for l in &years {
                                    ui.label(signed(l.total(c)));
                                }
                                ui.end_row();
                            }
                            ui.label(RichText::new("Net").family(theme::semibold()));
                            for l in &years {
                                ui.label(signed(l.net()));
                            }
                            ui.end_row();
                            ui.label(RichText::new("Closing balance").family(theme::semibold()));
                            for l in &years {
                                ui.label(theme::mono(money(l.closing)));
                            }
                            ui.end_row();
                            ui.label(dim("Average crowd"));
                            for l in &years {
                                ui.label(theme::mono(money(l.average_attendance() as i64)));
                            }
                            ui.end_row();
                        });
                    ui.add_space(14.0);
                });
        });
    }

    // ---- the other clubs -------------------------------------------------

    fn league_money_card(&self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "The league's money");
            ui.label(dim(
                "What the published accounts show: payrolls and crowds are public, the size of \
                 another club's balance is not, so only how it looks is given.",
            ));
            ui.add_space(6.0);
            egui::ScrollArea::horizontal()
                .id_salt("fin_league_scroll")
                .show(ui, |ui| {
                    egui::Grid::new("fin_league")
                        .striped(true)
                        .spacing(vec2(26.0, 7.0))
                        .show(ui, |ui| {
                            header_row(
                                ui,
                                &[
                                    "Club",
                                    "Money",
                                    "Payroll",
                                    "Against the cap",
                                    "Average crowd",
                                    "Hall",
                                    "Ticket",
                                ],
                            );
                            ui.end_row();
                            for t in w.table() {
                                self.club_link(ui, t);
                                let state = w.money_state(t);
                                let text = RichText::new(state.label());
                                ui.label(match state.label() {
                                    "Sound" => text.color(theme::POS),
                                    "Tight" => text,
                                    _ => text.color(theme::WARN),
                                });
                                let tax = w.tax_view(t);
                                ui.label(theme::mono(money(tax.payroll)));
                                if tax.excess > 0 {
                                    ui.label(
                                        theme::mono(format!("{} over", money(tax.excess)))
                                            .color(theme::WARN),
                                    );
                                } else {
                                    ui.label(dim(format!(
                                        "{} under",
                                        money(tax.cap - tax.payroll)
                                    )));
                                }
                                let l = w.ledger(t);
                                let a = w.arena(t);
                                if l.games > 0 {
                                    ui.label(theme::mono(format!(
                                        "{} ({}%)",
                                        money(l.average_attendance() as i64),
                                        l.average_attendance() as u64 * 100
                                            / a.capacity.max(1) as u64
                                    )));
                                } else {
                                    ui.label(dim("no games yet"));
                                }
                                ui.label(theme::mono(money(a.capacity as i64)));
                                ui.label(theme::mono(money(w.ticket_price(t))));
                                ui.end_row();
                            }
                        });
                    ui.add_space(14.0);
                });
        });
    }
}

/// Five dots, the first `level` of them filled.
fn dots(ui: &mut Ui, level: u8) {
    let (rect, _) = ui.allocate_exact_size(vec2(5.0 * 14.0, 14.0), Sense::hover());
    for i in 0..MAX_LEVEL {
        let c = rect.left_center() + vec2(7.0 + i as f32 * 14.0, 0.0);
        if i < level {
            ui.painter().circle_filled(c, 4.5, theme::ACCENT);
        } else {
            ui.painter()
                .circle_stroke(c, 4.5, Stroke::new(1.2, theme::LINE_2));
        }
    }
}

/// The season's money, round by round: bars for what came in and what went
/// out each round above, the balance as a line below.
fn chart(ui: &mut Ui, weeks: &[[i64; 3]], start: i64, slots: usize) {
    let width = ui.available_width();
    let height = 300.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let p = ui.painter_at(rect);
    let axis = FontId::monospace(10.0);
    let (left, right) = (58.0, 12.0);
    let x0 = rect.left() + left;
    let x1 = rect.right() - right;
    let slots = slots.max(weeks.len()).max(1);
    let slot_w = (x1 - x0) / slots as f32;
    let centre = |i: usize| x0 + slot_w * (i as f32 + 0.5);

    // Top panel: money in and out.
    let (f_top, f_bot) = (rect.top() + 18.0, rect.top() + 128.0);
    let flow_max = nice_max(weeks.iter().map(|w| w[0].max(w[1])).max().unwrap_or(0));
    let fy = |v: i64| f_bot - (f_bot - f_top) * v as f32 / flow_max as f32;
    p.text(
        egui::pos2(x0, rect.top() + 2.0),
        Align2::LEFT_TOP,
        "MONEY IN AND OUT EACH ROUND",
        FontId::monospace(10.0),
        theme::TEXT_MUTE,
    );
    for k in 0..=2 {
        let v = flow_max * k / 2;
        let y = fy(v);
        p.hline(
            x0..=x1,
            y,
            Stroke::new(1.0, if k == 0 { theme::LINE_2 } else { theme::LINE }),
        );
        p.text(
            egui::pos2(x0 - 8.0, y),
            Align2::RIGHT_CENTER,
            short(v),
            axis.clone(),
            theme::TEXT_MUTE,
        );
    }
    let bar = (slot_w * 0.36).clamp(2.0, 16.0);
    let income = theme::POS.gamma_multiply(0.75);
    let spend = theme::WARN.gamma_multiply(0.75);
    for (i, wk) in weeks.iter().enumerate() {
        let c = centre(i);
        let a = egui::Rect::from_min_max(
            egui::pos2(c - bar - 0.5, fy(wk[0])),
            egui::pos2(c - 0.5, f_bot),
        );
        let b = egui::Rect::from_min_max(
            egui::pos2(c + 0.5, fy(wk[1])),
            egui::pos2(c + bar + 0.5, f_bot),
        );
        p.rect_filled(a, 1.5, income);
        p.rect_filled(b, 1.5, spend);
    }
    // Key.
    let key_y = rect.top() + 2.0;
    let kx = x1 - 190.0;
    p.rect_filled(
        egui::Rect::from_min_size(egui::pos2(kx, key_y + 1.0), vec2(9.0, 9.0)),
        2.0,
        income,
    );
    p.text(
        egui::pos2(kx + 14.0, key_y),
        Align2::LEFT_TOP,
        "in",
        axis.clone(),
        theme::TEXT_DIM,
    );
    p.rect_filled(
        egui::Rect::from_min_size(egui::pos2(kx + 44.0, key_y + 1.0), vec2(9.0, 9.0)),
        2.0,
        spend,
    );
    p.text(
        egui::pos2(kx + 58.0, key_y),
        Align2::LEFT_TOP,
        "out",
        axis.clone(),
        theme::TEXT_DIM,
    );
    p.rect_filled(
        egui::Rect::from_min_size(egui::pos2(kx + 96.0, key_y + 1.0), vec2(9.0, 9.0)),
        4.5,
        theme::ACCENT,
    );
    p.text(
        egui::pos2(kx + 110.0, key_y),
        Align2::LEFT_TOP,
        "balance",
        axis.clone(),
        theme::TEXT_DIM,
    );

    // Bottom panel: the balance.
    let (b_top, b_bot) = (rect.top() + 172.0, rect.top() + 268.0);
    // The range of the balance, not of zero: a club with millions in the
    // bank would otherwise draw a flat line. Zero is in view when the
    // balance is near it or below it.
    let mut lo = weeks.iter().map(|w| w[2]).chain([start]).min().unwrap_or(0);
    let mut hi = weeks.iter().map(|w| w[2]).chain([start]).max().unwrap_or(0);
    if hi - lo < 100_000 {
        let mid = (hi + lo) / 2;
        (lo, hi) = (mid - 50_000, mid + 50_000);
    }
    if lo > 0 && lo < (hi - lo) * 2 {
        lo = 0;
    }
    if hi < 0 && -hi < (hi - lo) * 2 {
        hi = 0;
    }
    let step = nice_max((hi - lo).max(1) / 3).max(10_000);
    let lo_t = (lo as f64 / step as f64).floor() as i64 * step;
    let hi_t = (hi as f64 / step as f64).ceil() as i64 * step;
    let span = (hi_t - lo_t).max(1);
    let by = |v: i64| b_bot - (b_bot - b_top) * (v - lo_t) as f32 / span as f32;
    p.text(
        egui::pos2(x0, rect.top() + 156.0),
        Align2::LEFT_TOP,
        "BALANCE AT THE END OF THE ROUND",
        FontId::monospace(10.0),
        theme::TEXT_MUTE,
    );
    let mut tick = lo_t;
    while tick <= hi_t {
        let y = by(tick);
        p.hline(
            x0..=x1,
            y,
            Stroke::new(
                1.0,
                if tick == 0 {
                    theme::TEXT_MUTE
                } else {
                    theme::LINE
                },
            ),
        );
        p.text(
            egui::pos2(x0 - 8.0, y),
            Align2::RIGHT_CENTER,
            short(tick),
            axis.clone(),
            theme::TEXT_MUTE,
        );
        tick += step;
    }
    let mut pts = vec![egui::pos2(x0, by(start))];
    pts.extend(
        weeks
            .iter()
            .enumerate()
            .map(|(i, w)| egui::pos2(centre(i), by(w[2]))),
    );
    p.add(Shape::line(pts.clone(), Stroke::new(2.0, theme::ACCENT)));
    if let Some(last) = pts.last() {
        p.circle_filled(*last, 3.5, theme::ACCENT);
    }

    // Round numbers along the bottom.
    let every = if slots > 16 { 3 } else { 1 };
    for i in (0..slots).step_by(every) {
        p.text(
            egui::pos2(centre(i), rect.bottom() - 4.0),
            Align2::CENTER_BOTTOM,
            (i + 1).to_string(),
            axis.clone(),
            theme::TEXT_MUTE,
        );
    }
    p.text(
        egui::pos2(x0 - 8.0, rect.bottom() - 4.0),
        Align2::RIGHT_BOTTOM,
        "round",
        axis.clone(),
        theme::TEXT_MUTE,
    );

    // Hover: a hairline and the figures of that round.
    if let Some(pos) = resp.hover_pos()
        && pos.x >= x0
        && pos.x <= x1
    {
        let i = (((pos.x - x0) / slot_w) as usize).min(slots - 1);
        p.vline(centre(i), rect.y_range(), Stroke::new(1.0, theme::LINE_2));
        if let Some(wk) = weeks.get(i) {
            resp.on_hover_text_at_pointer(format!(
                "Round {}\nIn {}\nOut {}\nBalance {}",
                i + 1,
                money(wk[0]),
                money(wk[1]),
                money(wk[2])
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_figures_are_round_and_short() {
        assert_eq!(nice_max(0), 1000);
        assert_eq!(nice_max(73_000), 100_000);
        assert_eq!(nice_max(100_000), 100_000);
        assert_eq!(nice_max(130_000), 200_000);
        assert_eq!(nice_max(210_000), 500_000);
        assert_eq!(short(2_300_000), "2.3m");
        assert_eq!(short(-80_000), "-80k");
        assert_eq!(short(950), "950");
    }

    #[test]
    fn every_tab_in_the_menu_has_a_page_and_compare_is_off_it() {
        use crate::app::Tab;
        assert!(Tab::ALL.contains(&Tab::Finances));
        assert!(!Tab::ALL.contains(&Tab::Compare));
        assert_eq!(Tab::Finances.label(), "Finances");
    }
}
