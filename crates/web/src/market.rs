//! The trade desk and the draft room.

use basketman_core::command::Command;
use basketman_core::inbox::Deal;
use basketman_core::league::TeamId;
use basketman_core::player::PlayerId;
use basketman_core::season::SALARY_CAP;
use eframe::egui::{self, FontId, RichText, Ui, vec2};

use crate::app::{App, GAP, Tab, header_row, money, name, rating};
use crate::nav::{Link, link};
use crate::pages::{dim, estimate};
use crate::theme::{self, Backdrop};

impl App {
    /// Open the trade desk with `club`, optionally asking for one of its
    /// players straight away.
    pub(crate) fn open_trade(&mut self, club: TeamId, want: Option<PlayerId>) {
        if self.deal.club != club {
            self.deal = Deal {
                club,
                ..Deal::default()
            };
        }
        if let Some(p) = want
            && !self.deal.get.contains(&p)
        {
            self.deal.get.push(p);
        }
        self.deal_note = None;
        self.go(Tab::Trade);
    }

    pub(crate) fn trade_page(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let me = self.team;
        if self.deal.club == me || self.deal.club >= self.world.teams.len() {
            self.deal.club = (me + 1) % self.world.teams.len();
        }
        // Drop anything that is no longer where the deal says it is.
        let club = self.deal.club;
        let w = &self.world;
        self.deal.give.retain(|p| w.teams[me].roster.contains(p));
        self.deal.get.retain(|p| w.teams[club].roster.contains(p));
        self.deal.give_picks.retain(|&i| w.picks[i].owner == me);
        self.deal.get_picks.retain(|&i| w.picks[i].owner == club);

        let mut deal = self.deal.clone();
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Trade desk");
            ui.label(
                RichText::new(
                    "Players, draft picks and cash, in any mix. Tick what goes each way; the \
                     other club's answer is shown as you build the deal.",
                )
                .color(theme::TEXT_DIM),
            );
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("Trade with");
                egui::ComboBox::from_id_salt("trade_club")
                    .width(220.0)
                    .selected_text(&w.teams[deal.club].name)
                    .show_ui(ui, |ui| {
                        for t in w.table() {
                            if t != me {
                                ui.selectable_value(&mut deal.club, t, &w.teams[t].name);
                            }
                        }
                    });
                ui.add_space(16.0);
                for t in [me, club] {
                    let pay = w.payroll(t);
                    let text = format!("{} payroll {}", w.teams[t].name, money(pay));
                    ui.label(theme::mono(text).color(if pay > SALARY_CAP {
                        theme::WARN
                    } else {
                        theme::TEXT_DIM
                    }));
                }
                ui.label(theme::mono(format!("cap {}", money(SALARY_CAP))).color(theme::TEXT_MUTE));
            });
            let gm = &w.teams[club].office.gm;
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(dim("Across the table:"));
                link(ui, Link::Club(club), name(&gm.name));
                ui.label(dim(format!("{}.", gm.traits().join("; "))));
            });
            ui.label(dim(
                "Values in the tables are the market's. He has his own, and they decide the answer.",
            ));
        });
        if deal.club != club {
            deal = Deal {
                club: deal.club,
                give: deal.give,
                give_picks: deal.give_picks,
                ..Deal::default()
            };
        }
        ui.add_space(GAP);

        let side = |ui: &mut Ui,
                    team: TeamId,
                    players: &mut Vec<PlayerId>,
                    picks: &mut Vec<usize>| {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(
                    ui,
                    &if team == me {
                        "We send".to_string()
                    } else {
                        format!("{} send", w.teams[team].name)
                    },
                );
                ui.add_space(6.0);
                egui::ScrollArea::horizontal()
                    .id_salt(("trade_side", team == me))
                    .show(ui, |ui| {
                        egui::Grid::new(("trade_grid", team == me))
                            .striped(true)
                            .spacing(vec2(14.0, 7.0))
                            .show(ui, |ui| {
                                header_row(
                                    ui,
                                    &["", "Player", "Pos", "Age", "Ovr", "Salary", "Yrs", "Value"],
                                );
                                ui.end_row();
                                let mut roster = w.teams[team].roster.clone();
                                roster.sort_by_key(|&p| std::cmp::Reverse(w.value(p)));
                                for pid in roster {
                                    let p = &w.players[pid as usize];
                                    let mut on = players.contains(&pid);
                                    if ui.checkbox(&mut on, "").changed() {
                                        if on {
                                            players.push(pid);
                                        } else {
                                            players.retain(|&q| q != pid);
                                        }
                                    }
                                    link(ui, Link::Player(pid), name(&p.name));
                                    ui.label(theme::mono(p.position.label()).color(theme::ACCENT));
                                    ui.label(theme::mono(p.age.to_string()));
                                    if team == me {
                                        ui.label(rating(p.overall(), false));
                                    } else {
                                        let (lo, hi) = self.known_overall(p);
                                        ui.label(estimate(lo, hi));
                                    }
                                    ui.label(theme::mono(money(p.contract.salary as i64)));
                                    ui.label(theme::mono(p.contract.years_left.to_string()));
                                    ui.label(
                                        theme::mono(money(w.value(pid))).color(theme::TEXT_DIM),
                                    );
                                    ui.end_row();
                                }
                            });
                        ui.add_space(10.0);
                    });
                theme::eyebrow(ui, "Draft picks");
                let mut any = false;
                for (i, pick) in w.picks.iter().enumerate() {
                    if pick.owner != team {
                        continue;
                    }
                    any = true;
                    ui.horizontal(|ui| {
                        let mut on = picks.contains(&i);
                        if ui.checkbox(&mut on, w.pick_label(i)).changed() {
                            if on {
                                picks.push(i);
                            } else {
                                picks.retain(|&q| q != i);
                            }
                        }
                        ui.label(theme::mono(money(w.pick_value(i))).color(theme::TEXT_DIM));
                    });
                }
                if !any {
                    ui.label(dim("None left."));
                }
            });
        };
        if ui.available_width() > 1100.0 {
            ui.columns(2, |cols| {
                side(&mut cols[0], me, &mut deal.give, &mut deal.give_picks);
                side(&mut cols[1], deal.club, &mut deal.get, &mut deal.get_picks);
            });
        } else {
            side(ui, me, &mut deal.give, &mut deal.give_picks);
            ui.add_space(GAP);
            side(ui, deal.club, &mut deal.get, &mut deal.get_picks);
        }
        ui.add_space(GAP);

        let verdict = w.trade_verdict(&deal);
        let mut propose = false;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "The deal");
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("Cash we add");
                let most = w.teams[me].budget.max(0);
                let least = -w.teams[deal.club].budget.max(0);
                ui.add(
                    egui::DragValue::new(&mut deal.cash)
                        .speed(1000)
                        .range(least..=most),
                );
                ui.label(dim("(below zero: they pay us)"));
            });
            ui.label(dim(format!(
                "Transfer budget left: {}. Wage budget left: {}.",
                money(w.transfer_left(me).max(0)),
                money(w.wage_left(me).max(0))
            )));
            let tax = w.tax_view(me);
            if tax.excess > 0 {
                ui.label(
                    RichText::new(format!(
                        "Our payroll is {} over the cap. The luxury tax is {} a year at this \
                         payroll, {} still to pay this season.",
                        money(tax.excess),
                        money(tax.annual),
                        money(tax.to_pay())
                    ))
                    .color(theme::WARN),
                );
            }
            ui.add_space(6.0);
            match &verdict {
                Ok(text) => ui.label(RichText::new(text).color(theme::POS)),
                Err(text) => ui.label(RichText::new(text).color(theme::WARN)),
            };
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                propose = theme::primary_button(ui, verdict.is_ok(), "Make the trade");
                if ui.button("Clear").clicked() {
                    deal = Deal {
                        club: deal.club,
                        ..Deal::default()
                    };
                }
            });
            if let Some(note) = &self.deal_note {
                ui.add_space(6.0);
                ui.label(RichText::new(note).family(theme::semibold()));
            }
        });
        if deal != self.deal {
            self.deal_note = None;
        }
        self.deal = deal;
        if propose {
            self.deal_note = Some(
                match self.world.apply(Command::ProposeTrade(self.deal.clone())) {
                    Ok(_) => "Done. The details are in your inbox.".to_string(),
                    Err(why) => why.to_string(),
                },
            );
            self.deal = Deal {
                club: self.deal.club,
                ..Deal::default()
            };
        }
    }

    pub(crate) fn draft_page(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let w = &self.world;
        let me = self.team;
        let live = w.draft.as_ref();
        let Some(draft) = live.or(w.last_draft.as_ref()) else {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Draft");
                ui.label(dim("The draft is held when the season ends."));
            });
            return;
        };
        let at = draft.at();
        let ours = live.is_some() && draft.on_clock() == Some(me);
        let mut take = None;
        let mut auto = None;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(
                ui,
                &format!("Draft / after season {}", w.season + live.is_some() as u16),
            );
            let title = match (live.is_some(), at) {
                (true, Some(i)) if ours => format!("You are on the clock: pick {}", i + 1),
                (true, Some(i)) => format!("Pick {}", i + 1),
                _ => "The draft is over".to_string(),
            };
            ui.label(RichText::new(title).font(FontId::new(26.0, theme::semibold())));
            if ours {
                ui.label(
                    RichText::new(
                        "Figures are your scouts' ranges. Open a prospect for the full picture.",
                    )
                    .color(theme::TEXT_DIM),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Let the assistant make this pick").clicked() {
                        auto = Some(false);
                    }
                    if ui.button("Let him make all our picks").clicked() {
                        auto = Some(true);
                    }
                });
            } else if live.is_none() {
                ui.label(dim("The new season has begun. Press Continue."));
            }
        });
        ui.add_space(GAP);

        let mut board = |ui: &mut Ui| {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, &format!("Prospects / {} left", draft.pool.len()));
                ui.add_space(6.0);
                let mid = |r: (f32, f32)| (r.0 + r.1) / 2.0;
                let mut pool: Vec<PlayerId> = draft.pool.clone();
                let like = |p: PlayerId| {
                    let p = &w.players[p as usize];
                    mid(self.known_overall(p)) * 0.45 + mid(w.scouting.estimate_potential(p)) * 0.55
                };
                pool.sort_by(|&a, &b| like(b).total_cmp(&like(a)));
                egui::ScrollArea::vertical()
                    .id_salt("draft_board")
                    .max_height(620.0)
                    .min_scrolled_height(620.0)
                    .show(ui, |ui| {
                        egui::Grid::new("draft_grid")
                            .striped(true)
                            .spacing(vec2(16.0, 7.0))
                            .show(ui, |ui| {
                                header_row(
                                    ui,
                                    &["Prospect", "Pos", "Age", "Ht", "Wing", "Now", "Ceiling", ""],
                                );
                                ui.end_row();
                                for pid in pool {
                                    let p = &w.players[pid as usize];
                                    link(ui, Link::Player(pid), name(&p.name));
                                    ui.label(theme::mono(p.position.label()).color(theme::ACCENT));
                                    ui.label(theme::mono(p.age.to_string()));
                                    ui.label(theme::mono(format!("{:.0}", p.body.height_cm)));
                                    ui.label(theme::mono(format!("{:.0}", p.body.wingspan_cm)));
                                    let (lo, hi) = self.known_overall(p);
                                    ui.label(estimate(lo, hi));
                                    let (lo, hi) = w.scouting.estimate_potential(p);
                                    ui.label(estimate(lo, hi));
                                    if ours {
                                        if ui.button("Draft").clicked() {
                                            take = Some(pid);
                                        }
                                    } else {
                                        ui.label("");
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            });
        };
        let order = |ui: &mut Ui| {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Order");
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .id_salt("draft_order")
                    .max_height(620.0)
                    .min_scrolled_height(620.0)
                    .show(ui, |ui| {
                        egui::Grid::new("draft_order_grid")
                            .striped(true)
                            .spacing(vec2(12.0, 7.0))
                            .show(ui, |ui| {
                                for (i, s) in draft.slots.iter().enumerate() {
                                    let now = live.is_some() && at == Some(i);
                                    let number = theme::mono(format!("{:>2}", i + 1));
                                    ui.label(if now {
                                        number.color(theme::ACCENT)
                                    } else {
                                        number.color(theme::TEXT_MUTE)
                                    });
                                    self.club_link(ui, s.owner);
                                    match s.taken {
                                        Some(p) => {
                                            let text = name(&w.players[p as usize].name);
                                            link(ui, Link::Player(p), text);
                                        }
                                        None if now => {
                                            ui.label(
                                                RichText::new("on the clock").color(theme::ACCENT),
                                            );
                                        }
                                        None => {
                                            ui.label(dim("-"));
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            });
        };
        if ui.available_width() > 1100.0 {
            let right = 440.0;
            let left = ui.available_width() - right - GAP;
            let down = egui::Layout::top_down(egui::Align::Min);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(left, 0.0), down, board);
                ui.add_space(GAP - ui.spacing().item_spacing.x);
                ui.allocate_ui_with_layout(vec2(right, 0.0), down, order);
            });
        } else {
            board(ui);
            ui.add_space(GAP);
            order(ui);
        }
        if let Some(p) = take {
            let _ = self.world.apply(Command::DraftPick(p));
        }
        if let Some(all) = auto {
            let _ = self.world.apply(Command::DraftAuto { all });
        }
    }
}
