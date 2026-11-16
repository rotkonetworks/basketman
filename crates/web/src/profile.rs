//! A player's own page and the two-player comparison.
//!
//! Any player name in the app is a link to his profile (`player_link`). The
//! comparison puts two players either side of a shared list of attributes,
//! with bars growing out from the middle.

use basketman_core::assistant;
use basketman_core::command::Command;
use basketman_core::engine::BoxLine;
use basketman_core::player::{Attr, Hand, Player, PlayerId, Position};
use basketman_core::training::{Intensity, TrainingFocus};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, RichText, Sense, Shape, Stroke, Ui, pos2, vec2,
};

use crate::app::{App, GAP, Tab, header_row, money, rating};
use crate::pages::{dim, estimate, match_rating, pct};
use crate::theme::{self, Backdrop};

/// Attribute groups, laid out as three columns on the profile.
type Group = (&'static str, &'static [Attr]);
const COLUMNS: [&[Group]; 3] = [
    &[
        (
            "Scoring",
            &[
                Attr::CloseShot,
                Attr::MidRange,
                Attr::ThreePoint,
                Attr::FreeThrow,
                Attr::PostMoves,
            ],
        ),
        ("Playmaking", &[Attr::Handling, Attr::Passing]),
    ],
    &[
        (
            "Defense",
            &[
                Attr::PerimeterD,
                Attr::InteriorD,
                Attr::Steal,
                Attr::Block,
                Attr::Footwork,
            ],
        ),
        ("Rebounding", &[Attr::OffRebound, Attr::DefRebound]),
    ],
    &[
        (
            "Physical",
            &[Attr::Speed, Attr::Strength, Attr::Vertical, Attr::Stamina],
        ),
        ("Mental", &[Attr::Iq, Attr::Composure, Attr::WorkRate]),
    ],
];

/// The eight spokes of the radar chart.
const AXES: [Group; 8] = [
    ("Inside", &[Attr::CloseShot, Attr::PostMoves]),
    (
        "Shooting",
        &[Attr::MidRange, Attr::ThreePoint, Attr::FreeThrow],
    ),
    ("Playmaking", &[Attr::Handling, Attr::Passing]),
    ("Mental", &[Attr::Iq, Attr::Composure, Attr::WorkRate]),
    ("Athletic", &[Attr::Speed, Attr::Vertical, Attr::Strength]),
    (
        "Perimeter D",
        &[Attr::PerimeterD, Attr::Steal, Attr::Footwork],
    ),
    ("Interior D", &[Attr::InteriorD, Attr::Block]),
    ("Rebounding", &[Attr::OffRebound, Attr::DefRebound]),
];

/// An attribute, its value, and the gap to the average at the position.
type Standout = (Attr, f32, f32);

/// Colours of the left and right player in a comparison.
const SIDES: [Color32; 2] = [theme::ACCENT, Color32::from_rgb(0x1F, 0x8A, 0x8A)];

/// A player's name as a link to his profile.
pub(crate) fn player_link(ui: &mut Ui, pid: PlayerId, text: RichText) -> egui::Response {
    crate::nav::link(ui, crate::nav::Link::Player(pid), text)
}

fn position_name(p: Position) -> &'static str {
    match p {
        Position::PG => "Point guard",
        Position::SG => "Shooting guard",
        Position::SF => "Small forward",
        Position::PF => "Power forward",
        Position::C => "Centre",
    }
}

/// One labelled figure in a row of headline facts.
fn fact(ui: &mut Ui, label: &str, value: RichText, note: &str) {
    ui.vertical(|ui| {
        ui.set_min_width(104.0);
        ui.spacing_mut().item_spacing.y = 3.0;
        theme::eyebrow(ui, label);
        ui.label(value.font(FontId::new(22.0, theme::semibold())));
        ui.label(RichText::new(note).size(12.0).color(theme::TEXT_DIM));
    });
}

fn years(n: u8) -> String {
    format!("{n} year{} left", if n == 1 { "" } else { "s" })
}

fn range_text(lo: f32, hi: f32) -> String {
    if hi - lo < 0.8 {
        format!("{:.0}", (lo + hi) / 2.0)
    } else {
        format!("{lo:.0}-{hi:.0}")
    }
}

/// Bar on a 0..=`max` scale. Solid up to `lo`, faint up to `hi`. With
/// `from_right` it grows leftwards, for the left-hand side of a comparison.
fn bar(ui: &mut Ui, width: f32, (lo, hi): (f32, f32), max: f32, color: Color32, from_right: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 8.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, theme::LINE);
    let span = |v: f32| {
        let w = rect.width() * (v / max).clamp(0.0, 1.0);
        if from_right {
            egui::Rect::from_min_max(pos2(rect.right() - w, rect.top()), rect.max)
        } else {
            egui::Rect::from_min_max(rect.min, pos2(rect.left() + w, rect.bottom()))
        }
    };
    if hi - lo >= 0.8 {
        painter.rect_filled(span(hi), 4.0, color.gamma_multiply(0.3));
    }
    painter.rect_filled(span(lo), 4.0, color);
}

/// Radar chart of the eight `AXES`, 1..=20 from the centre out.
fn radar(ui: &mut Ui, size: f32, series: &[([f32; 8], Color32)]) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size * 0.86), Sense::hover());
    let c = rect.center();
    let r = size * 0.5 - 62.0;
    let dir = |i: usize| {
        let a = std::f32::consts::TAU * i as f32 / 8.0 - std::f32::consts::FRAC_PI_2;
        vec2(a.cos(), a.sin())
    };
    let painter = ui.painter();
    for ring in [5.0, 10.0, 15.0, 20.0] {
        let points = (0..8).map(|i| c + dir(i) * r * ring / 20.0).collect();
        painter.add(Shape::closed_line(points, Stroke::new(1.0, theme::LINE_2)));
    }
    for (i, (label, _)) in AXES.iter().enumerate() {
        let d = dir(i);
        painter.line_segment([c, c + d * r], Stroke::new(1.0, theme::LINE));
        let align = match (d.x, d.y) {
            (x, _) if x > 0.3 => Align2::LEFT_CENTER,
            (x, _) if x < -0.3 => Align2::RIGHT_CENTER,
            (_, y) if y < 0.0 => Align2::CENTER_BOTTOM,
            _ => Align2::CENTER_TOP,
        };
        painter.text(
            c + d * (r + 8.0),
            align,
            label.to_uppercase(),
            FontId::new(crate::theme::fs(10.0), egui::FontFamily::Monospace),
            theme::TEXT_MUTE,
        );
    }
    for (values, color) in series {
        let points: Vec<egui::Pos2> = (0..8)
            .map(|i| c + dir(i) * r * (values[i] / 20.0).clamp(0.05, 1.0))
            .collect();
        // Fan from the centre: the outline need not be convex.
        let mut mesh = egui::Mesh::default();
        let fill = color.gamma_multiply(0.16);
        mesh.colored_vertex(c, fill);
        for p in &points {
            mesh.colored_vertex(*p, fill);
        }
        for i in 0..8u32 {
            mesh.add_triangle(0, 1 + i, 1 + (i + 1) % 8);
        }
        painter.add(Shape::mesh(mesh));
        painter.add(Shape::closed_line(points.clone(), Stroke::new(1.8, *color)));
        for p in points {
            painter.circle_filled(p, 2.5, *color);
        }
    }
}

fn stat_cells(ui: &mut Ui, l: &BoxLine, per: f32, decimals: usize) {
    let f = |v: u16| theme::mono(format!("{:.*}", decimals, v as f32 / per));
    ui.label(theme::mono(format!(
        "{:.*}",
        decimals,
        l.secs as f32 / 60.0 / per
    )));
    for v in [l.pts, l.reb(), l.ast, l.stl, l.blk, l.tov] {
        ui.label(f(v));
    }
    ui.label(theme::mono(format!("{:.1}", pct(l.fgm, l.fga))));
    ui.label(theme::mono(format!("{:.1}", pct(l.tpm, l.tpa))));
    ui.label(theme::mono(format!("{:.1}", pct(l.ftm, l.fta))));
}

impl App {
    /// Follow a player link clicked anywhere this frame.
    /// Make an offer for another club's player; the answer comes by mail.
    fn bid_controls(&mut self, ui: &mut Ui, pid: PlayerId) {
        if let Some(bid) = self.world.bid_pending(pid) {
            ui.label(
                RichText::new(format!(
                    "Offer of {} made, answer due {}",
                    crate::app::money(bid.fee),
                    basketman_core::inbox::date(bid.answer_on)
                ))
                .color(theme::ACCENT),
            );
            return;
        }
        match self.world.team_of(pid) {
            None => {
                // Nobody's player: he signs if asked.
                let id = egui::Id::unique(("sign", pid));
                if ui.button("Sign as a free agent").clicked() {
                    let note = match self.world.apply(Command::Bid {
                        player: pid,
                        fee: 0,
                    }) {
                        Ok(_) => String::new(),
                        Err(why) => why.to_string(),
                    };
                    ui.data_mut(|d| d.insert_temp(id, note));
                }
                let note: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
                if !note.is_empty() {
                    ui.label(RichText::new(&note).color(theme::WARN));
                }
                return;
            }
            Some(club) => {
                if ui.button("Trade for him").clicked() {
                    self.open_trade(club, Some(pid));
                }
            }
        }
        let value = self.world.value(pid);
        let id = egui::Id::unique(("bid", pid));
        let mut fee: i64 = ui.data(|d| d.get_temp(id)).unwrap_or(value);
        ui.add_space(12.0);
        ui.label(crate::pages::dim(format!(
            "Valued at {}",
            crate::app::money(value)
        )));
        let left = self.world.transfer_left(self.team);
        ui.add(
            egui::DragValue::new(&mut fee)
                .speed(1000)
                .range(1000..=self.world.teams[self.team].budget.max(1000)),
        );
        ui.label(crate::pages::dim(format!(
            "Transfer budget left: {}",
            crate::app::money(left.max(0))
        )));
        ui.data_mut(|d| d.insert_temp(id, fee));
        if ui.button("Make offer").clicked() {
            let note = match self.world.apply(Command::Bid { player: pid, fee }) {
                Ok(_) => String::new(),
                Err(why) => why.to_string(),
            };
            ui.data_mut(|d| d.insert_temp(id.with("note"), note));
        }
        let note: String = ui.data(|d| d.get_temp(id.with("note"))).unwrap_or_default();
        if !note.is_empty() {
            ui.label(RichText::new(&note).color(theme::WARN));
        }
    }

    fn axes(&self, p: &Player) -> [f32; 8] {
        std::array::from_fn(|i| {
            let attrs = AXES[i].1;
            attrs
                .iter()
                .map(|&a| {
                    let (lo, hi) = self.known(p, a);
                    (lo + hi) / 2.0
                })
                .sum::<f32>()
                / attrs.len() as f32
        })
    }

    /// Where a player stands out from others at his position, best first,
    /// and where he falls short, worst first. Only attributes known well
    /// enough to say.
    fn standouts(&self, p: &Player) -> (Vec<Standout>, Vec<Standout>) {
        let peers: Vec<&Player> = self
            .world
            .players
            .iter()
            .filter(|q| q.position == p.position)
            .collect();
        let mut diffs: Vec<Standout> = Attr::ALL
            .into_iter()
            .filter_map(|a| {
                let (lo, hi) = self.known(p, a);
                if hi - lo > 2.5 {
                    return None;
                }
                let avg = peers.iter().map(|q| q.attrs[a]).sum::<f32>() / peers.len() as f32;
                let v = (lo + hi) / 2.0;
                Some((a, v, v - avg))
            })
            .collect();
        diffs.sort_by(|a, b| b.2.total_cmp(&a.2));
        let strengths = diffs
            .iter()
            .copied()
            .filter(|d| d.2 >= 1.5)
            .take(4)
            .collect();
        let weaknesses = diffs
            .iter()
            .rev()
            .copied()
            .filter(|d| d.2 <= -1.5)
            .take(4)
            .collect();
        (strengths, weaknesses)
    }

    // ---- Profile --------------------------------------------------------

    pub(crate) fn profile(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let Some(pid) = self.profile else {
            self.go_back();
            return;
        };
        let p = self.world.players[pid as usize].clone();
        let mine = self.is_mine(pid);
        let club = self.world.team_of(pid);
        let games = self.world.games_played[pid as usize];
        let season = self.world.season_stats[pid as usize];
        let mut go = false;
        let mut jump: Option<PlayerId> = None;
        let mut shortlist = false;
        let mut versus: Option<PlayerId> = None;

        // Back, and through the rest of his club's squad.
        ui.horizontal(|ui| {
            if ui.link(format!("Back to {}", self.back_label())).clicked() {
                go = true;
            }
            if let Some(t) = club {
                let mates = &self.world.teams[t].roster;
                if let Some(i) = mates.iter().position(|&m| m == pid)
                    && mates.len() > 1
                {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let next = mates[(i + 1) % mates.len()];
                        let prev = mates[(i + mates.len() - 1) % mates.len()];
                        let who = |q: PlayerId| self.world.players[q as usize].name.as_str();
                        if ui.button(format!("{} >", who(next))).clicked() {
                            jump = Some(next);
                        }
                        if ui.button(format!("< {}", who(prev))).clicked() {
                            jump = Some(prev);
                        }
                    });
                }
            }
        });
        ui.add_space(10.0);

        // Who he is.
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                theme::eyebrow(ui, &format!("{} /", position_name(p.position)));
                match club {
                    Some(t) => {
                        let text = theme::eyebrow_text(&self.world.teams[t].name);
                        crate::nav::link(ui, crate::nav::Link::Club(t), text);
                    }
                    None if p.is_retired() => theme::eyebrow(ui, "Retired"),
                    None => theme::eyebrow(ui, "Free agent"),
                }
            });
            theme::title(ui, &p.name);
            let hand = if p.body.hand == Hand::Left {
                "left"
            } else {
                "right"
            };
            ui.label(dim(format!(
                "{} years old / {:.0} cm / {:.0} kg / shoots {hand}-handed",
                p.age, p.body.height_cm, p.body.weight_kg
            )));
            ui.add_space(14.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(26.0, 14.0);
                let (lo, hi) = self.known_overall(&p);
                fact(
                    ui,
                    "Overall",
                    RichText::new(range_text(lo, hi)),
                    "all attributes",
                );
                let (lo, hi) = if mine {
                    (
                        (p.potential - 1.0).max(p.overall()),
                        (p.potential + 1.0).min(20.0),
                    )
                } else {
                    self.world.scouting.estimate_potential(&p)
                };
                fact(
                    ui,
                    "Potential",
                    RichText::new(range_text(lo, hi)),
                    "staff estimate",
                );
                if mine {
                    let fit = RichText::new(format!("{:.0}%", p.condition * 100.0));
                    fact(
                        ui,
                        "Condition",
                        match p.condition {
                            c if c >= 0.95 => fit.color(theme::POS),
                            c if c < 0.85 => fit.color(theme::WARN),
                            _ => fit,
                        },
                        "match fitness",
                    );
                    let slot = self.world.teams[self.team]
                        .tactics()
                        .rotation
                        .iter()
                        .find(|s| s.player == pid);
                    match slot {
                        Some(s) => fact(
                            ui,
                            "Role",
                            RichText::new(s.role.label()),
                            &format!("{} minutes planned", s.minutes),
                        ),
                        None => fact(ui, "Role", RichText::new("Not in rotation"), "match tactic"),
                    }
                } else {
                    let known = self.world.scouting.knowledge(pid) * 100.0;
                    fact(
                        ui,
                        "Scouted",
                        RichText::new(format!("{known:.0}%")),
                        if self.world.scouting.is_shortlisted(pid) {
                            "on the shortlist"
                        } else {
                            "not being followed"
                        },
                    );
                }
                fact(
                    ui,
                    "Character",
                    self.character(&p),
                    if mine || self.world.scouting.knowledge(pid) >= 0.6 {
                        p.personality.kind().description()
                    } else {
                        "needs more scouting"
                    },
                );
                fact(
                    ui,
                    "Salary",
                    RichText::new(money(p.contract.salary as i64)),
                    &years(p.contract.years_left),
                );
                if games > 0 {
                    fact(
                        ui,
                        "Season rating",
                        match_rating(season.rating_per(games)),
                        &format!("{games} games"),
                    );
                }
            });
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("versus")
                    .width(220.0)
                    .selected_text("Compare with...")
                    .show_ui(ui, |ui| {
                        for o in self.compare_options() {
                            if o != pid
                                && ui.selectable_label(false, self.option_label(o)).clicked()
                            {
                                versus = Some(o);
                            }
                        }
                    });
                if !mine && !p.is_retired() {
                    let on = self.world.scouting.is_shortlisted(pid);
                    if ui
                        .selectable_label(on, if on { "Shortlisted" } else { "Shortlist" })
                        .clicked()
                    {
                        shortlist = true;
                    }
                    self.bid_controls(ui, pid);
                }
            });
        });
        ui.add_space(GAP);
        self.signature_card(ui, bd, pid);
        self.public_life(ui, bd, pid);
        ui.add_space(GAP);

        // Attributes, with the radar beside them when there is room.
        let wide = ui.available_width() > 1150.0;
        let side = if wide { 400.0 } else { ui.available_width() };
        let main = if wide {
            ui.available_width() - side - GAP
        } else {
            ui.available_width()
        };
        let focus = if mine { p.training.attrs() } else { &[] };
        let attributes = |ui: &mut Ui| {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Attributes");
                if !mine && self.world.scouting.knowledge(pid) < 0.95 {
                    ui.label(dim(
                        "Ranges are what the scouts can say so far. Shortlist him to narrow them.",
                    ));
                }
                ui.add_space(8.0);
                let columns = if ui.available_width() > 720.0 { 3 } else { 1 };
                let width = (ui.available_width() - 24.0 * (columns as f32 - 1.0)) / columns as f32;
                let column = |ui: &mut Ui, groups: &[Group]| {
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        for (title, attrs) in groups {
                            ui.label(theme::eyebrow_text(title).color(theme::ACCENT));
                            egui::Grid::new(("attrs", *title))
                                .spacing(vec2(12.0, 7.0))
                                .show(ui, |ui| {
                                    for &a in *attrs {
                                        let (lo, hi) = self.known(&p, a);
                                        ui.allocate_ui_with_layout(
                                            vec2((width - 150.0).max(110.0), 16.0),
                                            Layout::left_to_right(Align::Center),
                                            |ui| {
                                                let l = ui.label(a.name());
                                                if a == Attr::Footwork {
                                                    l.on_hover_text(
                                                        "Balance and feet: staying in front of a \
                                                         dribbler, pivots, clean gathers.",
                                                    );
                                                }
                                            },
                                        );
                                        bar(ui, 90.0, (lo, hi), 20.0, theme::ACCENT, false);
                                        ui.label(if hi - lo < 0.8 {
                                            rating((lo + hi) / 2.0, focus.contains(&a))
                                        } else {
                                            estimate(lo, hi)
                                        });
                                        ui.end_row();
                                    }
                                });
                            ui.add_space(12.0);
                        }
                    });
                };
                if columns == 3 {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 24.0;
                        for groups in COLUMNS {
                            column(ui, groups);
                        }
                    });
                } else {
                    for groups in COLUMNS {
                        column(ui, groups);
                    }
                }
                if !focus.is_empty() {
                    ui.label(dim(format!(
                        "Underlined: what his {} training works on.",
                        p.training.label().to_lowercase()
                    )));
                }
            });
        };
        let shape = |ui: &mut Ui| {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Profile");
                let size = ui.available_width().min(380.0);
                ui.vertical_centered(|ui| radar(ui, size, &[(self.axes(&p), theme::ACCENT)]));
                let (strengths, weaknesses) = self.standouts(&p);
                let pos = p.position.label();
                if !strengths.is_empty() {
                    ui.add_space(6.0);
                    ui.label(theme::eyebrow_text("Strengths").color(theme::POS));
                    for (a, v, d) in strengths {
                        ui.label(format!("{} {v:.0}, {d:+.0} on the average {pos}", a.name()));
                    }
                }
                if !weaknesses.is_empty() {
                    ui.add_space(6.0);
                    ui.label(theme::eyebrow_text("Weaknesses").color(theme::WARN));
                    for (a, v, d) in weaknesses {
                        ui.label(format!("{} {v:.0}, {d:+.0} on the average {pos}", a.name()));
                    }
                }
                let ape = p.body.ape_index_cm();
                if ape >= 8.0 {
                    ui.add_space(6.0);
                    ui.label(dim(format!(
                        "Long arms: wingspan {ape:.0} cm more than his height."
                    )));
                }
            });
        };
        if wide {
            ui.horizontal_top(|ui| {
                let down = Layout::top_down(Align::Min);
                ui.allocate_ui_with_layout(vec2(main, 0.0), down, attributes);
                ui.add_space(GAP - ui.spacing().item_spacing.x);
                ui.allocate_ui_with_layout(vec2(side, 0.0), down, shape);
            });
        } else {
            attributes(ui);
            ui.add_space(GAP);
            shape(ui);
        }
        ui.add_space(GAP);

        // Body, development, season.
        let mut training = (p.training, p.intensity);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            bd.card(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(250.0);
                    theme::eyebrow(ui, "Body");
                    ui.add_space(6.0);
                    egui::Grid::new("body")
                        .spacing(vec2(24.0, 7.0))
                        .show(ui, |ui| {
                            let mut line = |label: &str, value: String| {
                                ui.label(label);
                                ui.label(theme::mono(value));
                                ui.end_row();
                            };
                            let b = &p.body;
                            line("Height", format!("{:.0} cm", b.height_cm));
                            line("Weight", format!("{:.0} kg", b.weight_kg));
                            line(
                                "Wingspan",
                                format!("{:.0} cm ({:+.0})", b.wingspan_cm, b.ape_index_cm()),
                            );
                            line("Standing reach", format!("{:.0} cm", b.standing_reach_cm));
                            line("Shoulder width", format!("{:.0} cm", b.shoulder_width_cm));
                            line("Hand length", format!("{:.1} cm", b.hand_length_cm));
                            if mine || self.world.scouting.knowledge(pid) >= 0.95 {
                                line("Max vertical", format!("{:.0} cm", p.max_vertical_cm()));
                                line("Max touch", format!("{:.0} cm", p.max_touch_cm()));
                            }
                        });
                });
            });
            // His build, turning, against the league average. His ratings
            // shape the figure a little, so another club's player is drawn
            // with average ones until he is known.
            bd.card(ui, |ui| {
                ui.vertical(|ui| {
                    theme::eyebrow(ui, "Build");
                    ui.add_space(6.0);
                    let known = mine || self.world.scouting.knowledge(pid) >= 0.95;
                    let stats = if known {
                        crate::view3d::gait::Stats::of(&p)
                    } else {
                        crate::view3d::gait::Stats::AVERAGE
                    };
                    let average = crate::lineup::average_body(&self.world.players);
                    crate::lineup::profile_figure(ui, &p, stats, &average, vec2(300.0, 300.0));
                });
            });
            self.health_card(ui, bd, pid);
            ui.end_row();
            if mine {
                bd.card(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_min_width(300.0);
                        theme::eyebrow(ui, "Training");
                        ui.add_space(6.0);
                        egui::Grid::new("profile_training")
                            .spacing(vec2(24.0, 7.0))
                            .show(ui, |ui| {
                                ui.label("Focus");
                                egui::ComboBox::from_id_salt("profile_focus")
                                    .width(170.0)
                                    .selected_text(training.0.label())
                                    .show_ui(ui, |ui| {
                                        for f in TrainingFocus::ALL {
                                            ui.selectable_value(&mut training.0, f, f.label());
                                        }
                                    });
                                ui.end_row();
                                ui.label("Intensity");
                                egui::ComboBox::from_id_salt("profile_intensity")
                                    .width(170.0)
                                    .selected_text(training.1.label())
                                    .show_ui(ui, |ui| {
                                        for i in Intensity::ALL {
                                            ui.selectable_value(&mut training.1, i, i.label());
                                        }
                                    });
                                ui.end_row();
                            });
                        ui.add_space(8.0);
                        let advice = assistant::plan_individual(&p);
                        if advice == training {
                            ui.label(dim("The assistant coach agrees with this plan."));
                        } else {
                            ui.label(dim(format!(
                                "The assistant coach would have him on {}, {}.",
                                advice.0.label().to_lowercase(),
                                advice.1.label().to_lowercase()
                            )));
                            if ui.button("Use his plan").clicked() {
                                training = advice;
                            }
                        }
                        let room = p.potential - p.overall();
                        ui.add_space(6.0);
                        ui.label(dim(match (p.age, room) {
                            (..=23, r) if r >= 1.5 => "Young, with a lot still to come.",
                            (..=27, r) if r >= 0.8 => "Still improving.",
                            (31.., _) => "Past his peak; the physical side will fade first.",
                            _ => "About as good as he is going to get.",
                        }));
                    });
                });
            } else {
                bd.card(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_min_width(300.0);
                        theme::eyebrow(ui, "Scout reports");
                        ui.add_space(6.0);
                        let reports: Vec<_> =
                            self.world.scouting.reports_for(pid).take(6).collect();
                        if reports.is_empty() {
                            ui.label(dim(
                                "None yet. Shortlisted players are watched every round.",
                            ));
                        } else {
                            egui::Grid::new("profile_reports")
                                .striped(true)
                                .spacing(vec2(14.0, 6.0))
                                .show(ui, |ui| {
                                    header_row(
                                        ui,
                                        &["Rd", "Opponent", "Min", "Pts", "Reb", "Ast", "Rtg"],
                                    );
                                    ui.end_row();
                                    for r in reports {
                                        let l = &r.line;
                                        ui.label(theme::mono((r.round + 1).to_string()));
                                        let place = if r.home { "vs" } else { "at" };
                                        ui.label(dim(format!(
                                            "{place} {}",
                                            self.world.teams[r.opponent].name
                                        )));
                                        ui.label(theme::mono((l.secs / 60).to_string()));
                                        ui.label(theme::mono(l.pts.to_string()));
                                        ui.label(theme::mono(l.reb().to_string()));
                                        ui.label(theme::mono(l.ast.to_string()));
                                        ui.label(match_rating(r.rating));
                                        ui.end_row();
                                    }
                                });
                        }
                    });
                });
            }
            bd.card(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_min_width(620.0f32.min(ui.available_width()));
                    theme::eyebrow(ui, "This season");
                    ui.add_space(6.0);
                    if games == 0 {
                        ui.label(dim("Has not played yet."));
                        return;
                    }
                    egui::ScrollArea::horizontal()
                        .id_salt("profile_season")
                        .show(ui, |ui| {
                            egui::Grid::new("profile_season_grid")
                                .striped(true)
                                .spacing(vec2(14.0, 7.0))
                                .show(ui, |ui| {
                                    header_row(
                                        ui,
                                        &[
                                            "", "Min", "Pts", "Reb", "Ast", "Stl", "Blk", "TO",
                                            "FG%", "3P%", "FT%",
                                        ],
                                    );
                                    ui.end_row();
                                    ui.label("Per game");
                                    stat_cells(ui, &season, games as f32, 1);
                                    ui.end_row();
                                    ui.label("Per 36 minutes");
                                    stat_cells(
                                        ui,
                                        &season,
                                        (season.secs as f32 / 2160.0).max(0.01),
                                        1,
                                    );
                                    ui.end_row();
                                    ui.label("Total");
                                    stat_cells(ui, &season, 1.0, 0);
                                    ui.end_row();
                                });
                            ui.add_space(12.0);
                        });
                });
            });
        });
        ui.add_space(GAP);

        self.career_card(ui, bd, pid);
        ui.add_space(GAP);

        // Game by game.
        if let Some(t) = club {
            bd.card(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::eyebrow(ui, "Game log");
                ui.add_space(6.0);
                let log: Vec<_> = self
                    .world
                    .fixtures
                    .iter()
                    .filter(|f| f.home == t || f.away == t)
                    .filter_map(|f| {
                        let r = f.result.as_ref()?;
                        let side = if f.home == t { 0 } else { 1 };
                        let line = r.boxes[side].iter().find(|(q, _)| *q == pid)?.1;
                        Some((f, r, side, line))
                    })
                    .collect();
                if log.is_empty() {
                    ui.label(dim("No games yet."));
                    return;
                }
                egui::ScrollArea::horizontal()
                    .id_salt("profile_log")
                    .show(ui, |ui| {
                        egui::Grid::new("profile_log_grid")
                            .striped(true)
                            .spacing(vec2(16.0, 7.0))
                            .show(ui, |ui| {
                                header_row(
                                    ui,
                                    &[
                                        "Rd", "Opponent", "Result", "Min", "Pts", "FG", "3PT",
                                        "FT", "Reb", "Ast", "Stl", "Blk", "TO", "PF", "Rtg",
                                    ],
                                );
                                ui.end_row();
                                for (f, r, side, l) in log.iter().rev() {
                                    ui.label(theme::mono((f.round + 1).to_string()));
                                    let (place, opp) = if *side == 0 {
                                        ("vs", f.away)
                                    } else {
                                        ("at", f.home)
                                    };
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 5.0;
                                        ui.label(dim(place));
                                        self.club_link(ui, opp);
                                    });
                                    let (us, them) = (r.score[*side], r.score[1 - *side]);
                                    let result = theme::mono(format!(
                                        "{} {us}-{them}",
                                        if us > them { "W" } else { "L" }
                                    ));
                                    let result = if us > them {
                                        result.color(theme::POS)
                                    } else {
                                        result.color(theme::WARN)
                                    };
                                    let game = self
                                        .world
                                        .fixtures
                                        .iter()
                                        .position(|x| std::ptr::eq(x, *f))
                                        .unwrap_or(0);
                                    crate::nav::link(ui, crate::nav::Link::Game(game), result)
                                        .on_hover_text("Box score");
                                    ui.label(theme::mono((l.secs / 60).to_string()));
                                    let pts = theme::mono(l.pts.to_string());
                                    ui.label(if l.pts >= 20 {
                                        pts.color(theme::POS)
                                    } else {
                                        pts
                                    });
                                    ui.label(theme::mono(format!("{}-{}", l.fgm, l.fga)));
                                    ui.label(theme::mono(format!("{}-{}", l.tpm, l.tpa)));
                                    ui.label(theme::mono(format!("{}-{}", l.ftm, l.fta)));
                                    for v in [l.reb(), l.ast, l.stl, l.blk, l.tov, l.pf] {
                                        ui.label(theme::mono(v.to_string()));
                                    }
                                    ui.label(match_rating(l.rating()));
                                    ui.end_row();
                                }
                            });
                        ui.add_space(12.0);
                    });
            });
        }

        if mine {
            let (focus, intensity) = training;
            let _ = self.world.apply(Command::SetTraining {
                player: pid,
                focus,
                intensity,
            });
        }
        if shortlist {
            self.toggle_shortlist(pid);
        }
        if let Some(o) = versus {
            self.compare = vec![pid, o];
            self.go(Tab::Compare);
        }
        if let Some(q) = jump {
            self.profile = Some(q);
        }
        if go {
            self.go_back();
        }
    }

    // ---- Compare --------------------------------------------------------

    /// Players offered in the pickers: own squad, shortlist, then the rest
    /// of the league club by club.
    fn compare_options(&self) -> Vec<PlayerId> {
        let w = &self.world;
        let mut out = w.teams[self.team].roster.clone();
        out.extend(w.scouting.shortlist.iter().copied());
        for (t, team) in w.teams.iter().enumerate() {
            if t != self.team {
                out.extend(
                    team.roster
                        .iter()
                        .copied()
                        .filter(|p| !w.scouting.is_shortlisted(*p)),
                );
            }
        }
        out
    }

    fn option_label(&self, p: PlayerId) -> String {
        let q = &self.world.players[p as usize];
        format!("{}  {}  {}", q.name, q.position.label(), self.club(p))
    }

    pub(crate) fn compare(&mut self, ui: &mut Ui, bd: &Backdrop) {
        self.compare.truncate(2);
        let ids = [self.compare.first().copied(), self.compare.get(1).copied()];
        let mut pick: [Option<PlayerId>; 2] = [None, None];
        let mut swap = false;

        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Head to head");
            ui.add_space(8.0);
            let avail = ui.available_width();
            let stacked = avail < 900.0;
            let chart = if stacked { avail.min(380.0) } else { 380.0 };
            let column = if stacked {
                avail
            } else {
                (avail - chart - 2.0 * GAP) / 2.0
            };
            let side = |ui: &mut Ui, s: usize, pick: &mut [Option<PlayerId>; 2]| {
                ui.set_width(column);
                egui::ComboBox::from_id_salt(("compare_pick", s))
                    .width(column.min(320.0))
                    .selected_text(match ids[s] {
                        Some(p) => self.option_label(p),
                        None => "Pick a player".to_string(),
                    })
                    .show_ui(ui, |ui| {
                        for o in self.compare_options() {
                            if Some(o) != ids[1 - s]
                                && ui
                                    .selectable_label(Some(o) == ids[s], self.option_label(o))
                                    .clicked()
                            {
                                pick[s] = Some(o);
                            }
                        }
                    });
                let Some(pid) = ids[s] else {
                    ui.add_space(8.0);
                    ui.label(dim(
                        "Or use the Compare buttons on the squad and scouting screens.",
                    ));
                    return;
                };
                let p = &self.world.players[pid as usize];
                ui.add_space(10.0);
                player_link(
                    ui,
                    pid,
                    RichText::new(&p.name)
                        .font(FontId::new(26.0, theme::semibold()))
                        .color(SIDES[s]),
                );
                ui.label(
                    theme::mono(format!(
                        "{} / {} / {}",
                        p.position.label(),
                        p.age,
                        self.club(pid)
                    ))
                    .color(theme::TEXT_DIM),
                );
                ui.add_space(8.0);
                let (lo, hi) = self.known_overall(p);
                ui.label(
                    RichText::new(range_text(lo, hi)).font(FontId::new(34.0, theme::semibold())),
                );
                ui.label(theme::eyebrow_text("Overall"));
                ui.add_space(8.0);
                ui.label(self.character(p));
                ui.label(dim(format!(
                    "{} a year, {}",
                    money(p.contract.salary as i64),
                    years(p.contract.years_left)
                )));
                if !self.is_mine(pid) {
                    let known = self.world.scouting.knowledge(pid) * 100.0;
                    ui.label(theme::mono(format!("{known:.0}% scouted")).color(theme::TEXT_MUTE));
                }
            };
            let series: Vec<([f32; 8], Color32)> = ids
                .iter()
                .enumerate()
                .filter_map(|(s, id)| {
                    let p = &self.world.players[(*id)? as usize];
                    Some((self.axes(p), SIDES[s]))
                })
                .collect();
            if stacked {
                side(ui, 0, &mut pick);
                ui.add_space(GAP);
                side(ui, 1, &mut pick);
                ui.vertical_centered(|ui| radar(ui, chart, &series));
            } else {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    ui.allocate_ui_with_layout(
                        vec2(column, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| side(ui, 0, &mut pick),
                    );
                    ui.allocate_ui_with_layout(
                        vec2(chart, 0.0),
                        Layout::top_down(Align::Center),
                        |ui| {
                            radar(ui, chart, &series);
                            if ids[0].is_some()
                                && ids[1].is_some()
                                && ui.button("Swap sides").clicked()
                            {
                                swap = true;
                            }
                        },
                    );
                    ui.allocate_ui_with_layout(
                        vec2(column, 0.0),
                        Layout::top_down(Align::Max),
                        |ui| side(ui, 1, &mut pick),
                    );
                });
            }
        });

        if let [Some(a), Some(b)] = ids {
            ui.add_space(GAP);
            self.compare_rows(ui, bd, a, b);
        }

        if swap {
            self.compare.reverse();
        }
        for s in 0..2 {
            if let Some(p) = pick[s] {
                if s < self.compare.len() {
                    self.compare[s] = p;
                } else {
                    self.compare.push(p);
                }
            }
        }
    }

    fn compare_rows(&self, ui: &mut Ui, bd: &Backdrop, a: PlayerId, b: PlayerId) {
        let players = [
            &self.world.players[a as usize],
            &self.world.players[b as usize],
        ];
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());

            // Who is ahead, attribute by attribute.
            let mut ahead = [0usize; 2];
            for attr in Attr::ALL {
                let mid = |p: &Player| {
                    let (lo, hi) = self.known(p, attr);
                    (lo + hi) / 2.0
                };
                let (x, y) = (mid(players[0]), mid(players[1]));
                if (x - y).abs() >= 0.5 {
                    ahead[if x > y { 0 } else { 1 }] += 1;
                }
            }
            let last = |p: &Player| p.name.rsplit(' ').next().unwrap_or(&p.name).to_string();
            ui.vertical_centered(|ui| {
                ui.label(dim(format!(
                    "{} is ahead on {} attributes, {} on {}.",
                    last(players[0]),
                    ahead[0],
                    last(players[1]),
                    ahead[1]
                )));
            });
            ui.add_space(10.0);

            const VALUE: f32 = 64.0;
            const LABEL: f32 = 190.0;
            const SPACE: f32 = 14.0;
            let avail = ui.available_width();
            let width = ((avail - 2.0 * VALUE - LABEL - 4.0 * SPACE) / 2.0).clamp(40.0, 380.0);
            let total = 2.0 * (VALUE + width) + LABEL + 4.0 * SPACE;
            ui.horizontal(|ui| {
                ui.add_space(((avail - total) / 2.0).max(0.0));
                egui::Grid::new("compare_rows")
                    .spacing(vec2(SPACE, 8.0))
                    .show(ui, |ui| {
                        // Fixed-width cell with the text painted where it
                        // belongs; grid cells otherwise shrink to the text.
                        let cell = |ui: &mut Ui, w: f32, align: Align2, text: RichText| {
                            let (rect, _) = ui.allocate_exact_size(vec2(w, 16.0), Sense::hover());
                            let font = FontId::new(14.0, egui::FontFamily::Proportional);
                            let job = egui::WidgetText::from(text).into_galley(
                                ui,
                                Some(egui::TextWrapMode::Extend),
                                w,
                                font,
                            );
                            let pos = align.align_size_within_rect(job.size(), rect).min;
                            ui.painter().galley(pos, job, theme::TEXT);
                        };
                        let right = Align2::RIGHT_CENTER;
                        let left = Align2::LEFT_CENTER;
                        let centre = Align2::CENTER_CENTER;
                        let section = |ui: &mut Ui, title: &str| {
                            ui.label("");
                            ui.label("");
                            cell(
                                ui,
                                LABEL,
                                centre,
                                theme::eyebrow_text(title).color(theme::ACCENT),
                            );
                            ui.label("");
                            ui.label("");
                            ui.end_row();
                        };
                        // One line of the comparison. `scale` of zero means
                        // no bars (for values where length says nothing).
                        let row = |ui: &mut Ui,
                                   label: &str,
                                   values: [(f32, f32); 2],
                                   scale: f32,
                                   text: &dyn Fn((f32, f32)) -> String,
                                   lower: bool| {
                            let mid = |v: (f32, f32)| (v.0 + v.1) / 2.0;
                            let (x, y) = (mid(values[0]), mid(values[1]));
                            let lead = if (x - y).abs() < 0.05 {
                                None
                            } else {
                                Some(usize::from((x < y) != lower))
                            };
                            let value = |s: usize| {
                                let t = RichText::new(text(values[s]));
                                if lead == Some(s) {
                                    t.font(FontId::new(13.0, theme::mono_semibold()))
                                        .color(theme::TEXT)
                                } else {
                                    t.font(FontId::new(13.0, egui::FontFamily::Monospace))
                                        .color(theme::TEXT_MUTE)
                                }
                            };
                            let color = |s: usize| {
                                if lead == Some(1 - s) {
                                    SIDES[s].gamma_multiply(0.4)
                                } else {
                                    SIDES[s]
                                }
                            };
                            cell(ui, VALUE, right, value(0));
                            if scale > 0.0 {
                                bar(ui, width, values[0], scale, color(0), true);
                            } else {
                                ui.allocate_space(vec2(width, 8.0));
                            }
                            cell(ui, LABEL, centre, RichText::new(label));
                            if scale > 0.0 {
                                bar(ui, width, values[1], scale, color(1), false);
                            } else {
                                ui.allocate_space(vec2(width, 8.0));
                            }
                            cell(ui, VALUE, left, value(1));
                            ui.end_row();
                        };
                        let range = |v: (f32, f32)| range_text(v.0, v.1);
                        let whole = |v: (f32, f32)| format!("{:.0}", v.0);
                        let one = |v: (f32, f32)| format!("{:.1}", v.0);
                        let exact = |f: &dyn Fn(&Player) -> f32| {
                            let (x, y) = (f(players[0]), f(players[1]));
                            [(x, x), (y, y)]
                        };

                        row(
                            ui,
                            "Overall",
                            [
                                self.known_overall(players[0]),
                                self.known_overall(players[1]),
                            ],
                            20.0,
                            &range,
                            false,
                        );
                        for groups in COLUMNS {
                            for (title, attrs) in groups {
                                section(ui, title);
                                for &attr in *attrs {
                                    row(
                                        ui,
                                        attr.name(),
                                        [
                                            self.known(players[0], attr),
                                            self.known(players[1], attr),
                                        ],
                                        20.0,
                                        &range,
                                        false,
                                    );
                                }
                            }
                        }

                        section(ui, "Body");
                        row(ui, "Age", exact(&|p| p.age as f32), 0.0, &whole, true);
                        row(
                            ui,
                            "Height, cm",
                            exact(&|p| p.body.height_cm),
                            0.0,
                            &whole,
                            false,
                        );
                        row(
                            ui,
                            "Weight, kg",
                            exact(&|p| p.body.weight_kg),
                            0.0,
                            &whole,
                            false,
                        );
                        row(
                            ui,
                            "Wingspan, cm",
                            exact(&|p| p.body.wingspan_cm),
                            0.0,
                            &whole,
                            false,
                        );
                        row(
                            ui,
                            "Standing reach, cm",
                            exact(&|p| p.body.standing_reach_cm),
                            0.0,
                            &whole,
                            false,
                        );
                        row(
                            ui,
                            "Shoulder width, cm",
                            exact(&|p| p.body.shoulder_width_cm),
                            0.0,
                            &whole,
                            false,
                        );
                        row(
                            ui,
                            "Hand length, cm",
                            exact(&|p| p.body.hand_length_cm),
                            0.0,
                            &one,
                            false,
                        );
                        let known = |p: &Player| {
                            self.is_mine(p.id) || self.world.scouting.knowledge(p.id) >= 0.95
                        };
                        if players.iter().all(|p| known(p)) {
                            row(
                                ui,
                                "Max touch, cm",
                                exact(&|p| p.max_touch_cm()),
                                0.0,
                                &whole,
                                false,
                            );
                        }

                        section(ui, "This season, per game");
                        let games = |p: &Player| self.world.games_played[p.id as usize];
                        let line = |p: &Player| self.world.season_stats[p.id as usize];
                        let per = |f: &dyn Fn(&BoxLine) -> u16| {
                            exact(&|p| f(&line(p)) as f32 / games(p).max(1) as f32)
                        };
                        let scaled = |ui: &mut Ui, label: &str, v: [(f32, f32); 2], lower: bool| {
                            let top = v[0].0.max(v[1].0).max(0.1);
                            row(ui, label, v, top, &one, lower);
                        };
                        row(ui, "Games", exact(&|p| games(p) as f32), 0.0, &whole, false);
                        scaled(ui, "Minutes", per(&|l| (l.secs / 60) as u16), false);
                        scaled(ui, "Points", per(&|l| l.pts), false);
                        scaled(ui, "Rebounds", per(&|l| l.reb()), false);
                        scaled(ui, "Assists", per(&|l| l.ast), false);
                        scaled(ui, "Steals", per(&|l| l.stl), false);
                        scaled(ui, "Blocks", per(&|l| l.blk), false);
                        scaled(ui, "Turnovers", per(&|l| l.tov), true);
                        let share = |f: &dyn Fn(&BoxLine) -> (u16, u16)| {
                            exact(&|p| {
                                let (made, tried) = f(&line(p));
                                pct(made, tried)
                            })
                        };
                        row(
                            ui,
                            "Field goal %",
                            share(&|l| (l.fgm, l.fga)),
                            100.0,
                            &one,
                            false,
                        );
                        row(
                            ui,
                            "Three-point %",
                            share(&|l| (l.tpm, l.tpa)),
                            100.0,
                            &one,
                            false,
                        );
                        row(
                            ui,
                            "Free throw %",
                            share(&|l| (l.ftm, l.fta)),
                            100.0,
                            &one,
                            false,
                        );
                        row(
                            ui,
                            "Match rating",
                            exact(&|p| line(p).rating_per(games(p))),
                            10.0,
                            &one,
                            false,
                        );
                    });
            });
            if players
                .iter()
                .any(|p| !self.is_mine(p.id) && self.world.scouting.knowledge(p.id) < 0.95)
            {
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    ui.label(dim(
                        "Faint bars and ranges are what the scouts are not yet sure of.",
                    ));
                });
            }
        });
    }
}
