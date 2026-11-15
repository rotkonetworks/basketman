//! The coach's board, the staff room and the medical file.

use basketman_core::command::Command;
use basketman_core::court::{CORNER_X, CORNER_Y, FT_RADIUS, HOOP, KEY_DEPTH, KEY_HALF_WIDTH};
use basketman_core::court::{THREE_RADIUS, Vec2, v};
use basketman_core::health::{SERIOUS, medical};
use basketman_core::player::{PlayerId, Position};
use basketman_core::staff::{Job, Staffer};
use basketman_core::tactics::{BOARD_ROLES, Board, DefenseScheme, Press, Tactics};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, vec2};

use crate::app::{App, GAP, header_row, money, name, rating};
use crate::pages::dim;
use crate::theme::{self, Backdrop};

/// Metres of court shown from the baseline out.
const DEPTH: f32 = 12.6;
const BOARD: Color32 = Color32::from_rgb(0xF4, 0xEE, 0xDC);
const CHALK: Color32 = Color32::from_rgb(0x8C, 0x86, 0x78);

pub(crate) fn surname(full: &str) -> &str {
    full.rsplit(' ').next().unwrap_or(full)
}

fn arrow(painter: &egui::Painter, from: Pos2, to: Pos2, color: Color32) {
    let d = to - from;
    let len = d.length();
    if len < 6.0 {
        return;
    }
    let u = d / len;
    let n = vec2(-u.y, u.x);
    painter.line_segment([from, to - u * 6.0], Stroke::new(2.5, color));
    painter.add(egui::Shape::convex_polygon(
        vec![to, to - u * 11.0 + n * 5.5, to - u * 11.0 - n * 5.5],
        color,
        Stroke::NONE,
    ));
}

/// The half court a board is drawn on: where it is on screen and how many
/// points a metre is.
pub(crate) struct Half {
    pub rect: Rect,
    pub scale: f32,
}

impl Half {
    pub fn at(&self, p: Vec2) -> Pos2 {
        self.rect.left_top() + vec2((p.x + 7.5) * self.scale, p.y * self.scale)
    }

    pub fn back(&self, p: Pos2) -> Vec2 {
        let q = p - self.rect.left_top();
        Board::clamp(v(q.x / self.scale - 7.5, q.y / self.scale))
    }

    /// Radius of a player's token.
    pub fn token(&self) -> f32 {
        (self.scale * 0.42).clamp(12.0, 19.0)
    }
}

/// Lay out and draw the half court, `width` points wide.
pub(crate) fn half_court(ui: &mut Ui, width: f32) -> (Half, egui::Painter) {
    let scale = width / 15.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, DEPTH * scale), Sense::hover());
    let half = Half { rect, scale };
    let at = |p: Vec2| half.at(p);
    let painter = ui.painter_at(rect.expand(2.0));
    let line = Stroke::new(1.5, CHALK);
    painter.rect_filled(rect, 6.0, BOARD);
    painter.rect_stroke(rect, 6.0, line, egui::StrokeKind::Inside);
    // Lane, free-throw circle, basket.
    painter.rect_stroke(
        Rect::from_min_max(
            at(v(-KEY_HALF_WIDTH, 0.0)),
            at(v(KEY_HALF_WIDTH, KEY_DEPTH)),
        ),
        0.0,
        line,
        egui::StrokeKind::Middle,
    );
    painter.circle_stroke(at(v(0.0, KEY_DEPTH)), FT_RADIUS * scale, line);
    painter.line_segment([at(v(-0.9, 1.2)), at(v(0.9, 1.2))], Stroke::new(3.0, CHALK));
    painter.circle_stroke(at(HOOP), 0.23 * scale, Stroke::new(2.0, theme::WARN));
    // Three-point line: straight in the corners, then the arc.
    let top = (CORNER_X / THREE_RADIUS).asin();
    let mut arc = vec![at(v(-CORNER_X, 0.0))];
    for i in 0..=40 {
        let a = -top + 2.0 * top * i as f32 / 40.0;
        arc.push(at(v(
            THREE_RADIUS * a.sin(),
            HOOP.y + THREE_RADIUS * a.cos(),
        )));
    }
    arc.push(at(v(CORNER_X, 0.0)));
    let _ = CORNER_Y;
    painter.add(egui::Shape::line(arc, line));
    (half, painter)
}

/// Where the five defenders of a scheme stand against a given offensive
/// set. The zones are the engine's resting shapes; man to man, switching
/// and the pack line follow the five attackers.
fn shape(t: &Tactics, spots: &[Vec2; 5]) -> [Vec2; 5] {
    let man = |on: f32, off: f32, near: f32| {
        let mut out = [Vec2::ZERO; 5];
        for (k, a) in spots.iter().enumerate() {
            let dh = a.dist(HOOP);
            out[k] = if k == 0 {
                a.toward(HOOP, on.min(dh * 0.8))
            } else if dh < 4.0 {
                a.toward(HOOP, near.min(dh * 0.6))
            } else {
                a.lerp(HOOP, off)
            };
        }
        out
    };
    match t.defense {
        DefenseScheme::Zone23 => [
            v(-2.3, 6.9),
            v(2.3, 6.9),
            v(-3.7, 2.8),
            v(3.7, 2.8),
            v(0.0, 3.2),
        ],
        DefenseScheme::Zone32 => [
            v(0.0, 7.6),
            v(4.4, 5.9),
            v(-4.4, 5.9),
            v(-2.7, 2.6),
            v(2.7, 2.6),
        ],
        DefenseScheme::ManToMan | DefenseScheme::SwitchEverything => man(1.05, 0.30, 0.6),
        DefenseScheme::PackLine => man(1.1, 0.50, 0.6),
    }
}

/// Radius of the pack line, metres from the basket.
const PACK: f32 = 5.6;

/// How a defense sets up: the five defenders drawn in the shape of the
/// scheme. A picture of the scheme, not of anything the engine played.
pub(crate) fn defense_diagram(ui: &mut Ui, t: &Tactics, width: f32) {
    let board = t.layout();
    let guards_men = !matches!(t.defense, DefenseScheme::Zone23 | DefenseScheme::Zone32);
    let defenders = shape(t, &board.spots);
    let (half, painter) = half_court(ui, width);
    let r = half.token();
    let at = |p: Vec2| half.at(p);
    let chalk = Stroke::new(1.5, CHALK);

    // The ground each defender is responsible for, in a zone.
    if !guards_men {
        for d in defenders {
            painter.circle_filled(at(d), 1.9 * half.scale, theme::ACCENT.gamma_multiply(0.08));
            painter.circle_stroke(
                at(d),
                1.9 * half.scale,
                Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.35)),
            );
        }
    }
    if t.defense == DefenseScheme::PackLine {
        let pts: Vec<Pos2> = (0..=64)
            .map(|i| {
                let a = std::f32::consts::PI * i as f32 / 64.0;
                at(v(PACK * a.cos(), HOOP.y + PACK * a.sin()))
            })
            .collect();
        painter.extend(egui::Shape::dashed_line(
            &pts,
            Stroke::new(2.0, theme::ACCENT),
            7.0,
            5.0,
        ));
        painter.text(
            at(v(PACK * 0.72, HOOP.y + PACK * 0.72)) + vec2(8.0, 0.0),
            Align2::LEFT_CENTER,
            "pack line",
            FontId::proportional(12.0),
            theme::ACCENT,
        );
    }

    // The attackers, faint: who they are set up against.
    for (i, a) in board.spots.iter().enumerate() {
        let c = at(*a);
        if guards_men {
            painter.extend(egui::Shape::dashed_line(
                &[at(defenders[i]), c],
                chalk,
                4.0,
                4.0,
            ));
        }
        painter.circle_filled(c, r * 0.85, BOARD);
        painter.circle_stroke(c, r * 0.85, Stroke::new(1.5, theme::TEXT_MUTE));
        painter.text(
            c,
            Align2::CENTER_CENTER,
            (i + 1).to_string(),
            FontId::new(12.0, theme::mono_semibold()),
            theme::TEXT_MUTE,
        );
        if i == 0 {
            painter.circle_filled(c + vec2(r * 0.8, -r * 0.8), 5.0, theme::WARN);
        }
    }
    // Switching: the men on the screen swap.
    if t.defense == DefenseScheme::SwitchEverything {
        let (a, b) = (at(defenders[0]), at(defenders[4]));
        let u = (b - a).normalized();
        arrow(&painter, a + u * (r + 2.0), b - u * (r + 2.0), theme::WARN);
        arrow(&painter, b - u * (r + 2.0), a + u * (r + 2.0), theme::WARN);
    }
    for (i, d) in defenders.iter().enumerate() {
        let c = at(*d);
        painter.circle_filled(c, r, theme::ACCENT);
        painter.circle_stroke(c, r, Stroke::new(1.5, Color32::WHITE));
        painter.text(
            c,
            Align2::CENTER_CENTER,
            (i + 1).to_string(),
            FontId::new(13.0, theme::mono_semibold()),
            Color32::WHITE,
        );
    }
    if t.press != Press::None {
        let text = format!("{} press", t.press.label().to_lowercase());
        painter.text(
            half.rect.right_bottom() - vec2(10.0, 8.0),
            Align2::RIGHT_BOTTOM,
            text,
            FontId::new(12.0, theme::semibold()),
            theme::WARN,
        );
    }

    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        ui.label(theme::eyebrow_text("How the scheme sets up"));
        ui.label(
            RichText::new("Purple: your defenders.")
                .color(theme::ACCENT)
                .strong(),
        );
        ui.label(dim("Grey: five attackers, ball in orange."));
    });
    ui.label(
        RichText::new(format!(
            "{}. {}",
            t.defense.label(),
            scheme_shape(t.defense)
        ))
        .color(theme::TEXT),
    );
    match t.press {
        Press::None => {}
        Press::ThreeQuarter => {
            ui.label(dim(
                "Press on: they pick the ball up before half court, then fall back into this shape.",
            ));
        }
        Press::FullCourt => {
            ui.label(dim(
                "Press on: they pick the ball up from the baseline, then fall back into this shape.",
            ));
        }
    }
}

/// What the picture of a scheme shows.
fn scheme_shape(d: DefenseScheme) -> &'static str {
    match d {
        DefenseScheme::ManToMan => {
            "Each defender stays between his own man and the basket. The man with the ball is \
             guarded tight, the others drop off toward the lane."
        }
        DefenseScheme::SwitchEverything => {
            "The same shape as man to man, but when a screen comes the two defenders swap men \
             instead of fighting through, so a big man can end up on a guard."
        }
        DefenseScheme::Zone23 => {
            "Two guards up top, three along the baseline. Each guards an area, not a man, and \
             slides with the ball. The corners and the arc are the open ground."
        }
        DefenseScheme::Zone32 => {
            "Three across the top, two underneath. The arc is covered; the baseline and the \
             high post are the soft spots."
        }
        DefenseScheme::PackLine => {
            "Man to man, but everyone away from the ball sags inside the dashed line, so a drive \
             meets a wall. Shooters get more room."
        }
    }
}

impl App {
    /// The coach's board: the half court with the five where the set puts
    /// them, or how the defense sets up. Drag a man to move him; in draw
    /// mode drag from a man to give him a run. Click a man to select him.
    pub(crate) fn board_card(&mut self, ui: &mut Ui, bd: &Backdrop, slot: usize) {
        let tactics = self.world.teams[self.team].playbook[slot].tactics.clone();
        let cast = tactics.cast(&self.world.players);
        let mut board = tactics.layout();
        let before = board.clone();
        let drawn = tactics
            .board
            .as_ref()
            .is_some_and(|b| b.set == tactics.offense);
        let mut drawing = self.room.drawing;
        let mut with_ball = self.room.with_ball;

        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut with_ball, true, "With the ball");
                ui.selectable_value(&mut with_ball, false, "Without the ball");
                ui.add_space(8.0);
                if with_ball {
                    theme::eyebrow(ui, &format!("{}", tactics.offense.label()));
                    ui.label(dim(if drawn {
                        "your drawing"
                    } else {
                        "the stock set"
                    }));
                } else {
                    theme::eyebrow(ui, tactics.defense.label());
                }
            });
            ui.add_space(6.0);
            if !with_ball {
                let width = ui.available_width().min(720.0);
                defense_diagram(ui, &tactics, width);
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut drawing, false, "Move players");
                ui.selectable_value(&mut drawing, true, "Draw runs");
                ui.add_space(10.0);
                if ui
                    .add_enabled(
                        board.runs.iter().any(Option::is_some),
                        egui::Button::new("Clear runs"),
                    )
                    .clicked()
                {
                    board.runs = [None; 5];
                }
                if ui
                    .add_enabled(drawn, egui::Button::new("Back to the stock set"))
                    .clicked()
                {
                    board = Board::stock(tactics.offense);
                }
            });
            ui.label(dim(if drawing {
                "Drag from a player to where he should run once the ball is over. Click a player \
                 to rub his run out. A run into the lane is a cut: called on some possessions, \
                 and he clears out again if the ball does not find him."
            } else {
                "Drag a player to where he should stand when the ball comes over half court, or \
                 click him to pick him out. The team runs it to either side."
            }));
            ui.add_space(8.0);

            let (half, painter) = half_court(ui, ui.available_width().min(720.0));
            let at = |p: Vec2| half.at(p);
            let r = half.token();

            // Runs first, so the men sit on top of their arrows.
            for i in 0..5 {
                if let Some(to) = board.runs[i] {
                    let (a, b) = (at(board.spots[i]), at(to));
                    let u = (b - a).normalized();
                    arrow(&painter, a + u * (r + 2.0), b, theme::ACCENT);
                }
            }
            for i in 0..5 {
                let c = at(board.spots[i]);
                let id = ui.scope_id().with(("board_man", i));
                let hit = ui.interact(
                    Rect::from_center_size(c, vec2(r * 2.4, r * 2.4)),
                    id,
                    Sense::click_and_drag(),
                );
                let player = cast[i].map(|p| &self.world.players[p as usize]);
                let hit = hit.on_hover_text(format!(
                    "{}: {}",
                    BOARD_ROLES[i],
                    player.map_or("whoever is on the floor", |p| p.name.as_str())
                ));
                if hit.dragged() {
                    if drawing {
                        if let Some(p) = hit.interact_pointer_pos() {
                            board.runs[i] = Some(half.back(p));
                        }
                    } else {
                        board.spots[i] = half.back(c + hit.drag_delta());
                    }
                }
                if hit.clicked() && drawing {
                    board.runs[i] = None;
                }
                if (hit.clicked() || hit.drag_started()) && cast[i].is_some() {
                    self.room.selected = cast[i];
                }
                let selected = cast[i].is_some() && cast[i] == self.room.selected;
                if selected {
                    painter.circle_filled(c, r + 7.0, theme::ACCENT.gamma_multiply(0.18));
                    painter.circle_stroke(c, r + 5.0, Stroke::new(2.5, theme::ACCENT));
                }
                let fill = if selected || hit.hovered() || hit.dragged() {
                    theme::ACCENT
                } else {
                    theme::TEXT
                };
                painter.circle_filled(c, r, fill);
                // His position at the top of the token, or the number of the
                // place when nobody stands there.
                let slot_pos = player.and_then(|p| {
                    let s = tactics.rotation.iter().find(|s| s.player == p.id)?;
                    Some(s.position.unwrap_or(p.position))
                });
                painter.text(
                    c,
                    Align2::CENTER_CENTER,
                    slot_pos.map_or((i + 1).to_string(), |p| p.label().to_string()),
                    FontId::new(r * 0.78, theme::mono_semibold()),
                    Color32::WHITE,
                );
                if let Some(p) = player {
                    // A first initial when two of the five share a surname.
                    let twin = cast.iter().flatten().any(|&o| {
                        o != p.id
                            && surname(&self.world.players[o as usize].name) == surname(&p.name)
                    });
                    let shown = match p.name.split_once(' ') {
                        Some((first, _)) if twin => format!(
                            "{}. {}",
                            first.chars().next().unwrap_or(' '),
                            surname(&p.name)
                        ),
                        _ => surname(&p.name).to_string(),
                    };
                    painter.text(
                        c + vec2(0.0, r + 3.0),
                        Align2::CENTER_TOP,
                        shown,
                        if selected {
                            FontId::new(13.0, theme::semibold())
                        } else {
                            FontId::proportional(13.0)
                        },
                        if selected { theme::ACCENT } else { theme::TEXT },
                    );
                }
            }

            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                for i in 0..5 {
                    let who =
                        cast[i].map_or("-", |p| surname(&self.world.players[p as usize].name));
                    ui.label(
                        theme::mono(format!("{} {}", i + 1, BOARD_ROLES[i])).color(theme::TEXT_DIM),
                    )
                    .on_hover_text(format!("Expected at tip-off: {who}"));
                }
            });
        });

        self.room.drawing = drawing;
        self.room.with_ball = with_ball;
        if board != before {
            let mut tactics = self.world.teams[self.team].playbook[slot].tactics.clone();
            tactics.draw(board);
            let _ = self.world.apply(Command::SetTactics { slot, tactics });
        }
    }

    /// Your coaches and scouts, what each is doing for you, and who is
    /// looking for work.
    pub(crate) fn staff_page(&mut self, ui: &mut Ui, bd: &Backdrop) {
        let me = self.team;
        let mut hire: Option<(Job, usize)> = None;
        let mut dismiss: Option<Job> = None;
        let office = self.world.teams[me].office.clone();
        let catch = self.world.catch_rate(me);
        let ratings = |ui: &mut Ui, s: &Staffer, job: Option<Job>| {
            for (v, j) in [
                (s.tactics, Job::Assistant),
                (s.teaching, Job::Skills),
                (s.fitness, Job::Fitness),
                (s.judgement, Job::Scout),
                (s.media, Job::Press),
            ] {
                ui.label(rating(v as f32, job == Some(j)));
            }
        };
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(
                ui,
                &format!("Your staff / {} a year in wages", money(office.wages())),
            );
            ui.label(dim(
                "You are the head coach. Each job runs on one rating, underlined; the rest of a \
                 man's ratings only matter if you move him. A vacant job is covered at a rating of 4.",
            ));
            if let Some(note) = &self.deal_note {
                ui.label(RichText::new(note).color(theme::ACCENT));
            }
            ui.add_space(8.0);
            egui::Grid::new("staff").striped(true).spacing(vec2(16.0, 9.0)).show(ui, |ui| {
                header_row(ui, &["Job", "Name", "Age", "Tac", "Teach", "Fit", "Judge", "Media", "Wage", "What it gets you", ""]);
                ui.end_row();
                for job in Job::HIRED {
                    ui.label(job.label());
                    match office.staffer(job) {
                        Some(s) => {
                            ui.label(name(&s.name));
                            ui.label(theme::mono(s.age.to_string()));
                            ratings(ui, s, Some(job));
                            ui.label(theme::mono(money(s.wage)));
                        }
                        None => {
                            ui.label(RichText::new("Vacant").color(theme::WARN));
                            for _ in 0..7 {
                                ui.label("");
                            }
                        }
                    }
                    let pct = |x: f32| format!("{:+.0}%", (x - 1.0) * 100.0);
                    ui.label(dim(match job {
                        Job::Assistant => format!(
                            "The squad learns a tactic {} faster than under an average coach.",
                            pct(office.drilling())
                        ),
                        Job::Skills => format!(
                            "Players gain {} from training against the average.",
                            pct(office.teaching())
                        ),
                        Job::Fitness => format!(
                            "Tired legs, injury risk and time out {} against the average.",
                            pct(office.wear())
                        ),
                        Job::Press => format!(
                            "Stops about {:.0}% of careless remarks. Media work teaches faster.",
                            catch * 100.0
                        ),
                        _ => "How fast and how truly the scouts read a shortlisted player, and how \
                              well your assistant GM reads a draft."
                            .to_string(),
                    }));
                    if office.staffer(job).is_some()
                        && ui.small_button("Let go").on_hover_text("Costs a quarter of a year's wage.").clicked()
                    {
                        dismiss = Some(job);
                    }
                    ui.end_row();
                }
            });
        });
        ui.add_space(GAP);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, &format!("Looking for work / {}", self.world.staff_pool.len()));
            ui.label(dim(
                "Hiring pays off the man in the job with a quarter of his yearly wage; he joins this list. \
                 New people arrive every summer, and other clubs hire from the same list then.",
            ));
            ui.add_space(8.0);
            let mut order: Vec<usize> = (0..self.world.staff_pool.len()).collect();
            order.sort_by_key(|&i| {
                let s = &self.world.staff_pool[i];
                std::cmp::Reverse(
                    s.tactics
                        .max(s.teaching)
                        .max(s.fitness)
                        .max(s.judgement)
                        .max(s.media),
                )
            });
            egui::Grid::new("staff_pool").striped(true).spacing(vec2(16.0, 9.0)).show(ui, |ui| {
                header_row(ui, &["Name", "Age", "Tac", "Teach", "Fit", "Judge", "Media", "Wage", "Likes to run", ""]);
                ui.end_row();
                for i in order {
                    let s = &self.world.staff_pool[i];
                    ui.label(name(&s.name));
                    ui.label(theme::mono(s.age.to_string()));
                    ratings(ui, s, None);
                    ui.label(theme::mono(money(s.wage)));
                    ui.label(dim(format!("{}, {}", s.offense.label(), s.defense.label())));
                    ui.menu_button("Hire as", |ui| {
                        for job in Job::HIRED {
                            let now = office.rating(job);
                            let label = format!("{} ({} now, {} with him)", job.label(), now, s.rating(job));
                            if ui.button(label).clicked() {
                                hire = Some((job, i));
                                ui.close();
                            }
                        }
                    });
                    ui.end_row();
                }
            });
        });
        if let Some((job, i)) = hire {
            self.deal_note = Some(
                match self.world.apply(Command::Hire { job, candidate: i }) {
                    Ok(out) => out.note.unwrap_or_default(),
                    Err(why) => why.to_string(),
                },
            );
        }
        if let Some(job) = dismiss {
            let _ = self.world.apply(Command::Dismiss(job));
            self.deal_note = Some(format!(
                "The {} has been let go.",
                job.label().to_lowercase()
            ));
        }
    }

    /// A club's bench staff, on its club page.
    pub(crate) fn staff_lines(&self, ui: &mut Ui, club: usize) {
        let office = &self.world.teams[club].office;
        ui.add_space(8.0);
        egui::Grid::new(("club_staff", club))
            .spacing(vec2(16.0, 6.0))
            .show(ui, |ui| {
                for job in Job::ALL {
                    let Some(s) = office.staffer(job) else {
                        continue;
                    };
                    ui.label(theme::eyebrow_text(job.label()));
                    ui.label(name(&s.name));
                    ui.label(dim(format!("{}", s.age)));
                    ui.label(rating(s.rating(job) as f32, false))
                        .on_hover_text(format!("{} rating", job.skill()));
                    if job == Job::HeadCoach {
                        ui.label(dim(format!(
                            "runs {}, {}",
                            s.offense.label(),
                            s.defense.label()
                        )));
                    }
                    ui.end_row();
                }
            });
    }

    /// Condition, morale, positions and the medical record of one player.
    pub(crate) fn health_card(&self, ui: &mut Ui, bd: &Backdrop, pid: PlayerId) {
        let p = &self.world.players[pid as usize];
        let mine = self.is_mine(pid);
        let known = if mine {
            1.0
        } else {
            self.world.scouting.knowledge(pid)
        };
        bd.card(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_min_width(330.0);
                theme::eyebrow(ui, "Fitness and medical");
                ui.add_space(6.0);
                egui::Grid::new("health")
                    .spacing(vec2(24.0, 7.0))
                    .show(ui, |ui| {
                        ui.label("Status");
                        match &p.injury {
                            Some(inj) => ui.label(
                                RichText::new(format!(
                                    "Out {} more week{}: {}",
                                    inj.weeks_left,
                                    if inj.weeks_left == 1 { "" } else { "s" },
                                    inj.what.label()
                                ))
                                .color(theme::WARN),
                            ),
                            None => ui.label(RichText::new("Fit").color(theme::POS)),
                        };
                        ui.end_row();
                        if mine || known >= 0.6 {
                            ui.label("Condition");
                            ui.label(theme::mono(format!("{:.0}%", p.condition * 100.0)));
                            ui.end_row();
                            ui.label("Morale");
                            ui.label(RichText::new(p.mood()).color(match p.morale {
                                m if m >= 0.62 => theme::POS,
                                m if m < 0.42 => theme::WARN,
                                _ => theme::TEXT,
                            }));
                            ui.end_row();
                        }
                        ui.label("Positions");
                        ui.horizontal(|ui| {
                            for pos in Position::ALL {
                                let k = p.knows(pos);
                                if k < 0.05 {
                                    continue;
                                }
                                let text =
                                    theme::mono(format!("{} {:.0}%", pos.label(), k * 100.0));
                                ui.label(match k {
                                    k if k >= 0.95 => text.color(theme::POS),
                                    k if k >= 0.75 => text.color(theme::TEXT),
                                    _ => text.color(theme::TEXT_MUTE),
                                });
                            }
                        })
                        .response
                        .on_hover_text(
                            "How well he knows each position. Used somewhere he does not know, he \
                         plays below his attributes; he learns it by being played there.",
                        );
                        ui.end_row();
                        ui.label("Medical");
                        ui.label(medical(p, known));
                        ui.end_row();
                    });
                ui.add_space(8.0);
                if p.injuries.is_empty() {
                    ui.label(dim("Injury record: clean."));
                    return;
                }
                let serious = p.injuries.iter().filter(|r| r.weeks >= SERIOUS).count();
                ui.label(dim(format!(
                    "Injury record: {} injuries, {} weeks out, {} serious.",
                    p.injuries.len(),
                    p.weeks_lost(),
                    serious
                )));
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("injuries")
                    .max_height(170.0)
                    .show(ui, |ui| {
                        egui::Grid::new("injuries")
                            .striped(true)
                            .spacing(vec2(16.0, 6.0))
                            .show(ui, |ui| {
                                header_row(ui, &["Season", "Age", "Injury", "Out", "After"]);
                                ui.end_row();
                                for r in p.injuries.iter().rev() {
                                    ui.label(theme::mono((r.season + 1).to_string()));
                                    ui.label(theme::mono(r.age.to_string()));
                                    ui.label(r.what.label());
                                    ui.label(theme::mono(format!("{} wk", r.weeks)));
                                    ui.label(match r.lost {
                                        l if l >= 0.8 => RichText::new("never the same athlete")
                                            .color(theme::WARN),
                                        l if l > 0.0 => {
                                            RichText::new("lost a step").color(theme::WARN)
                                        }
                                        _ => dim("full recovery"),
                                    });
                                    ui.end_row();
                                }
                            });
                    });
            });
        });
    }
}
