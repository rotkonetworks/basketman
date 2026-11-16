//! The choices of the tactics screen as cards: a small drawing, one plain
//! sentence, what it needs, how well this squad fits it, what it is good
//! and bad against and how well the squad knows it. Also the assistant's
//! proposal, the opponent card and the strip of results after a run.

use basketman_core::command::Command;
use basketman_core::court::{Vec2, v};
use basketman_core::fit::{
    Choice, Group, Knob, Mark, next_opponent, play, propose, scout, term,
};
use basketman_core::scrimmage::Focus;
use basketman_core::tactics::{Board, DefenseScheme, OffenseSet, Press, Tactics};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, vec2};

use crate::app::App;
use crate::board::{Half, paint_court, shape};
use crate::pages::dim;
use crate::tboard::fit_color;
use crate::theme::{self, Backdrop};

/// A small arrow for the card drawings.
fn mini_arrow(painter: &egui::Painter, a: Pos2, b: Pos2, color: Color32, dashed: bool) {
    let d = b - a;
    let len = d.length();
    if len < 3.0 {
        return;
    }
    let u = d / len;
    let n = vec2(-u.y, u.x);
    if dashed {
        painter.extend(Shape::dashed_line(&[a, b - u * 3.0], Stroke::new(1.3, color), 3.0, 2.0));
    } else {
        painter.line_segment([a, b - u * 3.0], Stroke::new(1.6, color));
    }
    painter.add(Shape::convex_polygon(
        vec![b, b - u * 5.0 + n * 2.6, b - u * 5.0 - n * 2.6],
        color,
        Stroke::NONE,
    ));
}

fn dot(painter: &egui::Painter, c: Pos2, color: Color32) {
    painter.circle_filled(c, 3.4, color);
    painter.circle_stroke(c, 3.4, Stroke::new(0.8, Color32::WHITE));
}

const GREY: Color32 = Color32::from_rgb(0x8A, 0x86, 0x96);

/// The tiny drawing on a card.
pub(crate) fn glyph(painter: &egui::Painter, rect: Rect, c: Choice, sample: OffenseSet) {
    let half = Half::fit(rect);
    paint_court(painter, &half);
    let at = |p: Vec2| half.at(p);
    let accent = theme::ACCENT;
    match c {
        Choice::Offense(o) => {
            let spots = Board::stock(o).spots;
            for m in play(o) {
                let line = |path: &[Vec2], who: usize, col: Color32| {
                    let mut p = at(spots[who]);
                    for q in path {
                        mini_arrow(painter, p, at(*q), col, false);
                        p = at(*q);
                    }
                };
                match &m {
                    Mark::Cut { who, path } => line(path, *who, accent),
                    Mark::Dribble { who, path } => line(path, *who, Color32::from_rgb(0xE0, 0x7A, 0x1F)),
                    Mark::Screen { at: p, .. } => {
                        let b = at(*p);
                        painter.line_segment([b - vec2(4.0, 0.0), b + vec2(4.0, 0.0)], Stroke::new(3.0, accent));
                    }
                    Mark::Pass { from, to, option } => {
                        mini_arrow(painter, at(spots[*from]), at(spots[*to]), if *option { GREY } else { theme::TEXT }, true)
                    }
                }
            }
            for s in spots {
                dot(painter, at(s), accent);
            }
        }
        Choice::Defense(d) => {
            let att = Board::stock(sample).spots;
            let def = shape(d, &att);
            if matches!(d, DefenseScheme::Zone23 | DefenseScheme::Zone32) {
                for p in def {
                    painter.circle_filled(at(p), 1.6 * half.scale, accent.gamma_multiply(0.10));
                }
            }
            for a in att {
                painter.circle_filled(at(a), 2.6, GREY);
            }
            if d == DefenseScheme::PackLine {
                let pts: Vec<Pos2> = (0..=24)
                    .map(|i| {
                        let a = std::f32::consts::PI * i as f32 / 24.0;
                        at(v(5.6 * a.cos(), 1.575 + 5.6 * a.sin()))
                    })
                    .collect();
                painter.extend(Shape::dashed_line(&pts, Stroke::new(1.3, accent), 3.0, 2.0));
            }
            if d == DefenseScheme::SwitchEverything {
                mini_arrow(painter, at(def[0]) + vec2(3.0, 0.0), at(def[4]), theme::WARN, false);
                mini_arrow(painter, at(def[4]), at(def[0]) + vec2(3.0, 0.0), theme::WARN, false);
            }
            for p in def {
                dot(painter, at(p), accent);
            }
        }
        Choice::Press(p) => {
            let col = theme::WARN;
            let rows: &[f32] = match p {
                Press::None => &[],
                Press::ThreeQuarter => &[9.5],
                Press::FullCourt => &[9.5, 5.5, 1.8],
            };
            for &y in rows {
                for x in [-4.5f32, -1.5, 1.5, 4.5] {
                    painter.circle_filled(at(v(x, y)), 2.2, col);
                }
            }
            if p == Press::None {
                for x in [-4.0f32, -1.3, 1.3, 4.0] {
                    dot(painter, at(v(x, 3.2)), accent);
                }
                dot(painter, at(v(0.0, 1.2)), accent);
            }
        }
        Choice::Knob(k, level) => {
            let n = level + 1;
            match k {
                Knob::Pace => {
                    for i in 0..n {
                        let x = -3.0 + i as f32 * 3.0;
                        let (a, b) = (at(v(x, 3.0)), at(v(x + 2.0, 6.0)));
                        mini_arrow(painter, a, b, accent, false);
                        mini_arrow(painter, b, at(v(x, 9.0)), accent, false);
                    }
                }
                Knob::Threes => {
                    let ys: &[(f32, f32)] = match level {
                        0 => &[(-1.0, 3.0), (1.0, 3.0), (0.0, 2.2), (-2.0, 4.5)],
                        1 => &[(-1.0, 3.0), (6.0, 2.0), (2.5, 8.0), (-3.0, 5.0)],
                        _ => &[(-6.5, 2.0), (6.5, 2.0), (3.0, 9.0), (-3.0, 9.0), (0.0, 10.0)],
                    };
                    for &(x, y) in ys {
                        painter.circle_filled(at(v(x, y)), 3.0, Color32::from_rgb(0xE0, 0x7A, 0x1F));
                    }
                }
                Knob::Boards => {
                    let ys: &[(f32, f32)] = match level {
                        0 => &[(-4.0, 10.0), (4.0, 10.0), (0.0, 10.5), (-2.0, 9.0), (2.0, 9.0)],
                        1 => &[(-2.0, 3.5), (2.0, 3.5), (-4.0, 9.0), (4.0, 9.0), (0.0, 10.0)],
                        _ => &[(-1.6, 2.8), (1.6, 2.8), (-3.0, 4.2), (3.0, 4.2), (0.0, 5.0)],
                    };
                    for &(x, y) in ys {
                        dot(painter, at(v(x, y)), accent);
                    }
                }
                Knob::Gamble => {
                    dot(painter, at(v(-2.0, 7.0)), GREY);
                    dot(painter, at(v(2.0, 4.0)), accent);
                    if level == 2 {
                        mini_arrow(painter, at(v(2.0, 4.0)), at(v(-1.5, 6.6)), theme::WARN, false);
                    } else if level == 0 {
                        painter.line_segment([at(v(-2.0, 7.0)), at(v(2.0, 4.0))], Stroke::new(1.0, crate::board::CHALK));
                    }
                }
            }
        }
    }
}

/// How a verdict reads, in a colour.
fn verdict_text(score: u8, verdict: &str) -> RichText {
    RichText::new(format!("{verdict}  {score}"))
        .color(fit_color(score as f32 / 100.0))
        .strong()
}

impl App {
    fn slot_tactics(&self) -> Tactics {
        self.world.teams[self.team].playbook[self.slot].tactics.clone()
    }

    pub(crate) fn apply_choice(&mut self, c: Choice) {
        let mut t = self.slot_tactics();
        c.apply(&mut t);
        let _ = self.world.apply(Command::SetTactics {
            slot: self.slot,
            tactics: t,
        });
    }

    /// One choice as a card. Returns nothing: a click on Use applies it, and
    /// the pointer over it shows it on the board.
    pub(crate) fn choice_card(&mut self, ui: &mut Ui, c: Choice, width: f32, compact: bool) {
        let t = self.slot_tactics();
        let slot = &self.world.teams[self.team].playbook[self.slot];
        let fam = match c {
            Choice::Offense(o) => Some(slot.familiarity_with(o, t.defense, t.press)),
            Choice::Defense(d) => Some(slot.familiarity_with(t.offense, d, t.press)),
            Choice::Press(p) => Some(slot.familiarity_with(t.offense, t.defense, p)),
            Choice::Knob(..) => None,
        };
        if self.choice_card_for(ui, c, width, compact, &t, fam) {
            self.apply_choice(c);
        }
    }

    /// The card for `c` against tactics `t` (the playbook's, or what a live
    /// game is running) and the squad's familiarity `fam` with it. Returns
    /// true when "Use this" is clicked.
    pub(crate) fn choice_card_for(
        &mut self,
        ui: &mut Ui,
        c: Choice,
        width: f32,
        compact: bool,
        t: &Tactics,
        fam: Option<f32>,
    ) -> bool {
        let on = c.is_on(t);
        let fit = c.fit(t, &self.world.players);
        let opp = next_opponent(&self.world, self.team).map(|o| scout(&self.world, o));
        let vs = opp.as_ref().and_then(|o| match c {
            Choice::Offense(x) if x.good_vs().contains(&o.defense) => Some((
                true,
                format!("Good against {}: they defend in {}.", o.name, o.defense.label().to_lowercase()),
            )),
            Choice::Offense(x) if x.bad_vs().contains(&o.defense) => Some((
                false,
                format!("Weak against {}: they defend in {}.", o.name, o.defense.label().to_lowercase()),
            )),
            Choice::Defense(x) if o.offense.bad_vs().contains(&x) => Some((
                true,
                format!("Handles {}: they run {}.", o.name, o.offense.label().to_lowercase()),
            )),
            Choice::Defense(x) if o.offense.good_vs().contains(&x) => Some((
                false,
                format!("{} run {}, which beats it.", o.name, o.offense.label().to_lowercase()),
            )),
            _ => None,
        });
        let sample = self
            .room
            .sample
            .or(opp.as_ref().map(|o| o.offense))
            .unwrap_or(OffenseSet::Motion);
        let against = c.against();
        let terms: Vec<&'static str> = c.terms().to_vec();
        let mut use_it = false;
        let fill = if on {
            theme::ACCENT.gamma_multiply(0.07)
        } else {
            theme::white(150)
        };
        let frame = egui::Frame::NONE
            .inner_margin(egui::Margin::same(10))
            .corner_radius(10)
            .fill(fill)
            .stroke(Stroke::new(
                if on { 2.0 } else { 1.0 },
                if on { theme::ACCENT } else { theme::LINE_2 },
            ));
        let out = frame.show(ui, |ui| {
            ui.set_width(width - 22.0);
            ui.horizontal_top(|ui| {
                let gw = if compact { 66.0 } else { 92.0 };
                let (g, _) = ui.allocate_exact_size(vec2(gw, gw * 0.84), Sense::hover());
                glyph(&ui.painter_at(g), g, c, sample);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if let Choice::Knob(k, _) = c {
                            ui.label(theme::eyebrow_text(k.label()));
                        }
                        ui.label(RichText::new(c.title()).strong().size(15.0).color(theme::TEXT));
                        if on {
                            ui.label(RichText::new("In use").color(theme::ACCENT).small().strong());
                        }
                    });
                    ui.label(dim(c.line()).size(13.0));
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let (bar, _) = ui.allocate_exact_size(vec2(70.0, 8.0), Sense::hover());
                ui.painter().rect_filled(bar, 4.0, theme::LINE);
                let mut f = bar;
                f.set_width(bar.width() * (fit.score as f32 / 100.0).max(0.05));
                ui.painter().rect_filled(f, 4.0, fit_color(fit.score as f32 / 100.0));
                ui.label(verdict_text(fit.score, fit.verdict()));
            });
            ui.label(dim(&fit.reason).size(12.5));
            if !compact {
                ui.label(dim(format!("Needs: {}", c.needs())).size(12.5));
                if let Some((good, bad)) = &against {
                    ui.label(dim(format!("Good against: {good}.")).size(12.5));
                    ui.label(dim(format!("Poor against: {bad}.")).size(12.5));
                }
            }
            if let Some((good, text)) = &vs {
                ui.label(
                    RichText::new(text)
                        .size(12.5)
                        .color(if *good { theme::POS } else { theme::WARN })
                        .strong(),
                );
            }
            ui.horizontal_wrapped(|ui| {
                if let Some(f) = fam {
                    let known = theme::mono(format!("Squad knows it {:.0}%", f * 100.0)).size(12.0);
                    ui.label(if f < 0.5 { known.color(theme::WARN) } else { known.color(theme::TEXT_DIM) })
                        .on_hover_text(term("familiarity").unwrap_or(""));
                }
                if !compact {
                    for w in &terms {
                        ui.label(RichText::new(*w).size(12.0).color(theme::ACCENT).underline())
                            .on_hover_text(term(w).unwrap_or(""));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !on && ui.button("Use this").clicked() {
                        use_it = true;
                    }
                });
            });
        });
        if ui.rect_contains_pointer(out.response.rect) && !on {
            self.room.hover_next = Some(c);
        }
        ui.add_space(6.0);
        use_it
    }

    /// The cards of one group, with the group's name over them.
    pub(crate) fn card_group(&mut self, ui: &mut Ui, g: Group, width: f32, compact: bool) {
        ui.label(theme::eyebrow_text(g.label()));
        ui.add_space(4.0);
        for c in g.choices() {
            self.choice_card(ui, c, width, compact);
        }
    }

    /// The cards laid out in a row for a phone, to be swiped.
    pub(crate) fn card_row(&mut self, ui: &mut Ui, groups: &[Group]) {
        egui::ScrollArea::horizontal()
            .id_salt(("card_row", groups.len(), groups.first().map(|g| g.label())))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for &g in groups {
                        for c in g.choices() {
                            ui.vertical(|ui| {
                                ui.set_width(286.0);
                                self.choice_card(ui, c, 286.0, true);
                            });
                        }
                    }
                });
            });
    }

    // ---- help me choose ------------------------------------------------------

    pub(crate) fn help_card(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let mut ask = false;
        let mut take = false;
        let mut drop = false;
        let now = self.slot_tactics();
        bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Help me choose");
                ui.label(dim(
                    "Your assistant coach looks at the five who start and at the next opponent, and proposes a full setup.",
                ));
                ui.add_space(4.0);
                if self.room.proposal.is_none() && theme::primary_button(ui, true, "Ask the assistant") {
                    ask = true;
                }
                if let Some(p) = &self.room.proposal {
                    if let Some(o) = &p.against {
                        ui.label(dim(format!("Against {o}, with {} test possessions.", p.tested)));
                    }
                    ui.add_space(4.0);
                    for line in &p.why {
                        ui.label(RichText::new(line).size(13.5).color(theme::TEXT));
                        ui.add_space(3.0);
                    }
                    if p.changes.is_empty() || p.tactics == now {
                        ui.label(RichText::new("This is what you already have.").color(theme::POS).strong());
                    } else {
                        ui.label(theme::eyebrow_text("What changes"));
                        for c in &p.changes {
                            ui.label(dim(c).size(12.5));
                        }
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.add_enabled(p.tactics != now, egui::Button::new("Use this setup")).clicked() {
                            take = true;
                        }
                        if ui.button("Not now").clicked() {
                            drop = true;
                        }
                    });
                }
        });
        if ask {
            self.room.proposal = Some(propose(&self.world, self.team, self.slot, true));
        }
        if take && let Some(p) = self.room.proposal.take() {
            let _ = self.world.apply(Command::SetTactics {
                slot: self.slot,
                tactics: p.tactics,
            });
        }
        if drop {
            self.room.proposal = None;
        }
    }

    // ---- the opponent --------------------------------------------------------

    pub(crate) fn scout_card(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let (opp_id, real) = self.test_opponent();
        let o = scout(&self.world, opp_id);
        let t = self.slot_tactics();
        let mut vs = self.room.vs_sample;
        let mut show = self.room.show_them;
        let known = next_opponent(&self.world, self.team).is_some();
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(
                ui,
                &if real {
                    format!("Next opponent / {}", o.name)
                } else {
                    format!("Sample club / {}", o.name)
                },
            );
            if known {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut vs, false, "Next opponent");
                    ui.selectable_value(&mut vs, true, "Sample club");
                });
            } else {
                ui.label(dim("No game is coming, so the board tests against a sample club."));
            }
            if !o.source.is_empty() {
                ui.label(RichText::new(&o.source).size(12.5).color(theme::ACCENT));
            }
            ui.add_space(4.0);
            for line in &o.tendencies {
                ui.label(RichText::new(line).size(13.0).color(theme::TEXT));
            }
            ui.add_space(4.0);
            let names = |xs: Vec<&str>| {
                if xs.is_empty() {
                    "none stands out".to_string()
                } else {
                    xs.join(", ")
                }
            };
            ui.label(dim(format!(
                "Offenses that beat their defense: {}.",
                names(o.we_attack_with.iter().map(|x| x.label()).collect())
            )));
            ui.label(dim(format!(
                "Defenses that stop their offense: {}.",
                names(o.we_defend_with.iter().map(|x| x.label()).collect())
            )));
            if o.we_attack_with.contains(&t.offense) && !o.we_attack_with.is_empty() {
                ui.label(RichText::new("What you run beats their defense.").color(theme::POS).strong());
            }
            ui.checkbox(&mut show, "Show their defense on the board");
        });
        self.room.vs_sample = vs;
        self.room.show_them = show;
    }

    // ---- after a run ---------------------------------------------------------

    pub(crate) fn result_strip(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let Some(pb) = &self.room.play else {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(dim(
                    "Press Run it to watch your five play a few possessions against the opponent with what is on the board. The result lands here, next to the other plays.",
                ));
            });
            return;
        };
        let attack = pb.scrim.focus == Focus::Attack;
        let cur = pb.idx;
        let mut jump = None;
        let mut pick: Option<(OffenseSet, DefenseScheme)> = None;
        let n = pb.scrim.possessions.len();
        let (pts, ppp, tov) = (pb.scrim.points(), pb.scrim.ppp(), pb.scrim.turnovers());
        let opp = self.world.teams[pb.scrim.opponent].name.clone();
        let best = if attack {
            pb.rows.iter().map(|r| r.ppp()).fold(0.0f32, f32::max)
        } else {
            pb.rows.iter().map(|r| r.ppp()).fold(f32::MAX, f32::min)
        };
        let chips: Vec<(String, Color32)> = pb
            .scrim
            .possessions
            .iter()
            .map(|p| {
                use basketman_core::scrimmage::Outcome::*;
                let good = |b: bool| if b == p.we_attack { theme::POS } else { Color32::from_rgb(0xC0, 0x39, 0x2B) };
                match p.outcome {
                    Scored => (format!("+{}", p.points), good(true)),
                    Missed => ("miss".to_string(), good(false)),
                    Turnover => ("TO".to_string(), good(false)),
                    Fouled => ("foul".to_string(), Color32::from_rgb(0xC9, 0x8A, 0x10)),
                }
            })
            .collect();
        let rows: Vec<_> = pb
            .rows
            .iter()
            .map(|r| (r.label, r.ppp(), r.familiarity, r.current, r.offense, r.defense, r.possessions))
            .collect();
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, if attack { "What our possessions produced" } else { "What they scored on us" });
            ui.label(RichText::new(format!(
                "{pts} points in {n} possessions against {opp}: {ppp:.2} a possession, {tov} turnover{}.",
                if tov == 1 { "" } else { "s" }
            )).size(14.0).color(theme::TEXT));
            ui.horizontal_wrapped(|ui| {
                for (i, (text, col)) in chips.iter().enumerate() {
                    let b = egui::Button::new(RichText::new(text).color(Color32::WHITE).size(12.5))
                        .fill(if i == cur { *col } else { col.gamma_multiply(0.65) })
                        .small();
                    if ui.add(b).on_hover_text("Watch this possession again").clicked() {
                        jump = Some(i);
                    }
                }
            });
            ui.add_space(6.0);
            ui.label(theme::eyebrow_text(if attack {
                "The same possessions with each offense"
            } else {
                "The same possessions with each defense"
            }));
            for (label, p, fam, current, o, d, _) in &rows {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(vec2(118.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let text = RichText::new(*label).size(13.0);
                        ui.label(if *current { text.strong().color(theme::ACCENT) } else { text });
                    });
                    let (bar, _) = ui.allocate_exact_size(vec2(90.0, 10.0), Sense::hover());
                    ui.painter().rect_filled(bar, 5.0, theme::LINE);
                    let mut f = bar;
                    f.set_width(bar.width() * (p / 1.6).clamp(0.04, 1.0));
                    let is_best = (p - best).abs() < 0.001;
                    ui.painter().rect_filled(f, 5.0, if is_best { theme::POS } else { theme::TEXT_MUTE });
                    ui.label(theme::mono(format!("{p:.2}")));
                    ui.label(dim(format!("knows {:.0}%", fam * 100.0)).size(crate::theme::fs(11.5)));
                    if !*current && ui.small_button("Use").clicked() {
                        pick = Some((*o, *d));
                    }
                });
            }
            ui.label(dim(format!(
                "{} possessions each. Small samples: gaps under about 0.15 a possession are noise. A system the squad barely knows is played worse, which is part of its number.",
                rows.first().map_or(0, |r| r.6)
            )).size(12.0));
        });
        if let Some(i) = jump
            && let Some(pb) = self.room.play.as_mut()
        {
            pb.idx = i;
            pb.t = 0.0;
            pb.hold = 0.0;
            pb.paused = false;
        }
        if let Some((o, d)) = pick {
            let c = if attack { Choice::Offense(o) } else { Choice::Defense(d) };
            self.apply_choice(c);
        }
    }
}
