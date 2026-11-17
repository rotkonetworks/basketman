//! The inbox: mail on the left, the open message on the right, and the
//! Continue button that moves the calendar on to the next thing that needs
//! the manager.

use basketman_core::command::Command;
use basketman_core::inbox::{Kind, Message, OfferState, Stop};
use eframe::egui::{self, Align2, FontId, RichText, Sense, Stroke, Ui, vec2};

use crate::app::{App, Tab, money};
use crate::theme::{self, Backdrop};

const ROW: f32 = 54.0;

impl App {
    /// The one button that moves the game forward. What it says is what
    /// pressing it will do.
    pub(crate) fn continue_button(&mut self, ui: &mut Ui, compact: bool) {
        if self.live.is_some() {
            // The round is not over until the game on the Match tab is.
            if theme::primary_button(ui, self.tab != Tab::Match, "Back to the match") {
                self.tab = Tab::Match;
            }
            return;
        }
        if let Some(id) = self.world.needs_answer() {
            if theme::primary_button(ui, true, "Answer the offer") {
                self.open_mail(id);
            }
            return;
        }
        if self.world.match_today() {
            let opponent = self
                .world
                .user_fixture()
                .map(|i| &self.world.fixtures[i])
                .map(|f| if f.home == self.team { f.away } else { f.home });
            let hint = match opponent {
                Some(t) => format!(
                    "Coach the game against {} yourself.",
                    self.world.teams[t].name
                ),
                None => "No game for us this round; the others play.".to_string(),
            };
            let play = egui::Button::new(
                RichText::new("Play match")
                    .family(theme::semibold())
                    .color(theme::BG),
            )
            .fill(theme::TEXT)
            .stroke(Stroke::NONE);
            if ui.add(play).on_hover_text(hint).clicked() {
                self.live = self.world.kickoff_today(self.team);
                if self.live.is_none() {
                    let _ = self.world.apply(Command::PlayToday);
                }
                self.touchline = Default::default();
                self.playback = self.fresh_playback();
                self.tab = Tab::Match;
            }
            if ui
                .button(if compact { "Instant" } else { "Instant result" })
                .on_hover_text("Your assistant coaches the game and you get the result.")
                .clicked()
            {
                let team = self.team;
                let played = self
                    .world
                    .apply(Command::PlayToday)
                    .map(|o| o.played)
                    .unwrap_or_default();
                self.last_game = played.into_iter().find(|&i| {
                    self.world.fixtures[i].home == team || self.world.fixtures[i].away == team
                });
                // Straight to the final score; the replay is there to rewind.
                let end = self.world.replay.as_ref().map_or(0.0, |r| r.1.duration());
                self.playback = crate::matchview::Playback {
                    time: end,
                    playing: false,
                    ..self.fresh_playback()
                };
                self.tab = Tab::Match;
            }
            return;
        }
        if self.world.draft.is_some() {
            if theme::primary_button(ui, self.tab != Tab::Draft, "Go to the draft") {
                self.go(Tab::Draft);
            }
            return;
        }
        let friendly = self.world.friendly_today().is_some();
        let label = if friendly { "Play friendly" } else { "Continue" };
        let hint = if friendly {
            "A friendly is on today. It is played when the day passes; your assistant picks the side."
        } else {
            "Pass one day. Other clubs play their games on their days."
        };
        if ui
            .add(
                egui::Button::new(
                    RichText::new(label)
                        .family(theme::semibold())
                        .color(theme::BG),
                )
                .fill(theme::TEXT)
                .stroke(Stroke::NONE),
            )
            .on_hover_text(hint)
            .clicked()
        {
            if let Some(Stop::Draft) = self
                .world
                .apply(Command::Continue)
                .ok()
                .and_then(|o| o.stop)
            {
                self.go(Tab::Draft);
            }
        }
        if ui
            .button(if compact { "Skip" } else { "Next event" })
            .on_hover_text("Jump to the next thing that needs you: a game, mail or the draft.")
            .clicked()
        {
            match self.world.apply(Command::Advance).ok().and_then(|o| o.stop) {
                Some(Stop::Mail) => {
                    let newest = self.world.inbox().iter().find(|m| !m.read).map(|m| m.id);
                    if let Some(id) = newest {
                        self.open_mail(id);
                    }
                }
                Some(Stop::Draft) => self.go(Tab::Draft),
                Some(Stop::MatchDay) | None => {}
            }
        }
    }

    pub(crate) fn open_mail(&mut self, id: u32) {
        self.mail = Some(id);
        self.mail_kind = None;
        let _ = self.world.apply(Command::MarkRead(id));
        self.tab = Tab::Inbox;
    }

    /// The messages in the open folder. Something is always open: the
    /// newest unread, else the newest.
    pub(crate) fn inbox_shown(&mut self) -> Vec<Message> {
        let shown: Vec<Message> = self
            .world
            .inbox()
            .into_iter()
            .filter(|m| self.mail_kind.is_none_or(|k| k == m.kind))
            .cloned()
            .collect();
        if !shown.iter().any(|m| Some(m.id) == self.mail) {
            self.mail = shown
                .iter()
                .find(|m| !m.read)
                .or(shown.first())
                .map(|m| m.id);
            if let Some(id) = self.mail {
                let _ = self.world.apply(Command::MarkRead(id));
            }
        }
        shown
    }

    pub(crate) fn mail_list(&mut self, ui: &mut Ui, bd: &Backdrop, shown: &[Message]) {
        let mut open = None;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            let unread = self.world.unread();
            theme::eyebrow(
                ui,
                &if unread > 0 {
                    format!("Inbox / {unread} unread")
                } else {
                    "Inbox".to_string()
                },
            );
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.mail_kind, None, "All");
                for kind in Kind::ALL {
                    ui.selectable_value(&mut self.mail_kind, Some(kind), kind.label());
                }
            });
            ui.add_space(6.0);
            if shown.is_empty() {
                ui.label(RichText::new("Nothing here.").color(theme::TEXT_DIM));
            }
            egui::ScrollArea::vertical()
                .id_salt("mail_list")
                .max_height(620.0)
                .min_scrolled_height(620.0)
                .show(ui, |ui| {
                    for m in shown {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                        let selected = self.mail == Some(m.id);
                        let painter = ui.painter_at(rect);
                        if selected {
                            painter.rect_filled(rect, 6.0, theme::ACCENT.gamma_multiply(0.10));
                        } else if response.hovered() {
                            painter.rect_filled(rect, 6.0, theme::LINE.gamma_multiply(0.6));
                        }
                        if !m.read {
                            painter.circle_filled(
                                rect.left_center() + vec2(9.0, 0.0),
                                3.5,
                                theme::ACCENT,
                            );
                        }
                        let waiting = m
                            .offer
                            .is_some_and(|i| self.world.desk.offers[i].state == OfferState::Open);
                        let weight = if m.read {
                            egui::FontFamily::Proportional
                        } else {
                            theme::semibold()
                        };
                        let left = rect.left() + 22.0;
                        painter.text(
                            egui::pos2(left, rect.top() + 8.0),
                            Align2::LEFT_TOP,
                            &m.from,
                            FontId::new(12.5, weight.clone()),
                            theme::TEXT_DIM,
                        );
                        painter.text(
                            egui::pos2(rect.right() - 10.0, rect.top() + 8.0),
                            Align2::RIGHT_TOP,
                            if waiting {
                                "needs an answer".to_string()
                            } else {
                                self.world.day_label(m.day)
                            },
                            FontId::new(crate::theme::fs(11.5), egui::FontFamily::Monospace),
                            if waiting {
                                theme::WARN
                            } else {
                                theme::TEXT_MUTE
                            },
                        );
                        // Long subjects are cut off by the row's clip rect.
                        painter.text(
                            egui::pos2(left, rect.top() + 28.0),
                            Align2::LEFT_TOP,
                            &m.subject,
                            FontId::new(14.0, weight),
                            if m.read { theme::TEXT_DIM } else { theme::TEXT },
                        );
                        painter.hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, theme::LINE));
                        if response
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            open = Some(m.id);
                        }
                    }
                });
        });
        if let Some(id) = open {
            self.mail = Some(id);
            self.sh.mail_open = true;
            let _ = self.world.apply(Command::MarkRead(id));
        }
    }

    pub(crate) fn mail_pane(&mut self, ui: &mut Ui, bd: &Backdrop, shown: &[Message]) {
        let Some(m) = shown.iter().find(|m| Some(m.id) == self.mail) else {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("No message selected.").color(theme::TEXT_DIM));
            });
            return;
        };
        let mut answer = None;
        let mut sponsor_answer = None;
        let mut appearance_answer = None;
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(420.0);
            theme::eyebrow(ui, &format!("{} / {}", m.kind.label(), self.world.day_label(m.day)));
            ui.add_space(2.0);
            ui.label(RichText::new(&m.subject).font(FontId::new(24.0, theme::semibold())));
            ui.label(RichText::new(format!("From {}", m.from)).color(theme::TEXT_DIM));
            ui.add_space(14.0);
            ui.scope(|ui| {
                ui.set_max_width(ui.available_width().min(720.0));
                ui.spacing_mut().item_spacing.y = 10.0;
                // Tables (results, standings) keep their columns in mono.
                for part in m.body.split("\n\n") {
                    let table = part.lines().count() > 2 && part.contains("  ");
                    let text = if table {
                        theme::mono(part)
                    } else {
                        RichText::new(part).size(14.5)
                    };
                    ui.add(egui::Label::new(text.color(theme::TEXT)).wrap());
                }
            });

            if let Some(i) = m.offer {
                let offer = &self.world.desk.offers[i];
                ui.add_space(14.0);
                ui.horizontal_wrapped(|ui| match offer.state {
                    OfferState::Open => {
                        let accept = format!("Accept {}", money(offer.fee));
                        if theme::primary_button(ui, true, &accept) {
                            answer = Some((i, true));
                        }
                        if ui.button("Turn it down").clicked() {
                            answer = Some((i, false));
                        }
                    }
                    OfferState::Accepted => {
                        ui.label(RichText::new("You accepted this offer.").color(theme::POS));
                    }
                    OfferState::Rejected => {
                        ui.label(RichText::new("You turned this down.").color(theme::TEXT_DIM));
                    }
                    OfferState::Lapsed => {
                        ui.label(RichText::new("This offer has lapsed.").color(theme::TEXT_DIM));
                    }
                });
            }
            if let Some(i) = m.sponsor
                && let Some(offer) = self.world.econ.offers.get(i)
            {
                ui.add_space(14.0);
                let open = offer.open_on(self.world.day);
                ui.horizontal_wrapped(|ui| match offer.state {
                    OfferState::Open if open => {
                        let accept = format!("Sign for {} a season", money(offer.fee));
                        if theme::primary_button(ui, true, &accept) {
                            sponsor_answer = Some((i, true));
                        }
                        if ui.button("Turn it down").clicked() {
                            sponsor_answer = Some((i, false));
                        }
                    }
                    OfferState::Accepted => {
                        ui.label(RichText::new("You signed the deal.").color(theme::POS));
                    }
                    OfferState::Rejected => {
                        ui.label(RichText::new("You turned this down.").color(theme::TEXT_DIM));
                    }
                    _ => {
                        ui.label(RichText::new("This offer has lapsed.").color(theme::TEXT_DIM));
                    }
                });
            }
            if let Some(i) = m.appearance
                && let Some(ask) = self.world.fame.asks.get(i)
            {
                ui.add_space(14.0);
                ui.horizontal_wrapped(|ui| match ask.state {
                    OfferState::Open if self.world.appearance_open(i) => {
                        let accept = format!("Accept, {} fee", money(ask.fee));
                        if theme::primary_button(ui, true, &accept) {
                            appearance_answer = Some((i, true));
                        }
                        if ui.button("Decline").clicked() {
                            appearance_answer = Some((i, false));
                        }
                    }
                    OfferState::Accepted => {
                        ui.label(RichText::new("You accepted the request.").color(theme::POS));
                    }
                    OfferState::Rejected => {
                        ui.label(RichText::new("You declined the request.").color(theme::TEXT_DIM));
                    }
                    _ => {
                        ui.label(RichText::new("This request has lapsed.").color(theme::TEXT_DIM));
                    }
                });
            }
            if let Some(i) = m.press {
                self.press_buttons(ui, i);
            }
            if let Some(i) = m.job {
                self.job_buttons(ui, i);
            }
            if !m.players.is_empty() {
                ui.add_space(14.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(theme::eyebrow_text("Players"));
                    for &p in &m.players {
                        let name = crate::app::name(&self.world.players[p as usize].name);
                        crate::profile::player_link(ui, p, name);
                    }
                });
            }
        });
        if let Some((offer, accept)) = answer {
            let _ = self.world.apply(Command::AnswerOffer { offer, accept });
        }
        if let Some((request, accept)) = appearance_answer {
            let _ = self
                .world
                .apply(Command::AnswerAppearance { request, accept });
        }
        if let Some((offer, accept)) = sponsor_answer {
            let _ = self.world.apply(Command::AnswerSponsor { offer, accept });
        }
    }
}
