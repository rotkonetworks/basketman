//! The tactics board as the screen: the half court with our five on it, the
//! play drawn over them, the defense against a sample, and the scrimmage
//! played back on it as the real engine ran it.

use basketman_core::command::Command;
use basketman_core::court::{HOOP, Vec2, v};
use basketman_core::fit::{Mark, next_opponent, place_fit, sample_opponent, scout};
use basketman_core::player::{Attr, PlayerId};
use basketman_core::scrimmage::{self, Focus, Outcome, Setup};
use basketman_core::tactics::{BOARD_ROLES, Board, DefenseScheme};
#[cfg(test)]
use basketman_core::tactics::OffenseSet;
use eframe::egui::{self, Align2, pos2, Color32, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, vec2};

use crate::app::App;
use crate::board::{Half, PACK, arrow, paint_court, scheme_shape, shape};
use crate::pages::dim;
use crate::tactic_room::{Playback, Tool};
use crate::theme;

const BALL: Color32 = Color32::from_rgb(0xE0, 0x7A, 0x1F);
const THEM: Color32 = Color32::from_rgb(0x8A, 0x86, 0x96);
const AMBER: Color32 = Color32::from_rgb(0xC9, 0x8A, 0x10);
const RED: Color32 = Color32::from_rgb(0xC0, 0x39, 0x2B);

pub(crate) fn fit_color(f: f32) -> Color32 {
    if f >= 0.6 {
        theme::POS
    } else if f >= 0.4 {
        AMBER
    } else {
        RED
    }
}

fn faded(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// A ring around a token filled `fit` of the way round, from the top.
fn ring(painter: &egui::Painter, c: Pos2, r: f32, fit: f32) {
    painter.circle_stroke(c, r + 4.0, Stroke::new(3.0, theme::LINE_2));
    let n = 36;
    let upto = ((fit.clamp(0.04, 1.0)) * n as f32).ceil() as usize;
    let pts: Vec<Pos2> = (0..=upto)
        .map(|i| {
            let f = (i as f32 / n as f32).min(fit.clamp(0.04, 1.0));
            let a = -std::f32::consts::FRAC_PI_2 + f * std::f32::consts::TAU;
            c + vec2(a.cos(), a.sin()) * (r + 4.0)
        })
        .collect();
    painter.add(Shape::line(pts, Stroke::new(3.0, fit_color(fit))));
}

fn token(painter: &egui::Painter, c: Pos2, r: f32, fill: Color32, text: &str) {
    painter.circle_filled(c, r, fill);
    painter.circle_stroke(c, r, Stroke::new(1.5, Color32::WHITE));
    painter.text(
        c,
        Align2::CENTER_CENTER,
        text,
        FontId::new((r * 0.78).max(9.0), theme::mono_semibold()),
        Color32::WHITE,
    );
}

fn label(painter: &egui::Painter, c: Pos2, r: f32, text: &str, color: Color32, bold: bool, within: Rect) {
    let font = if bold {
        FontId::new(12.5, theme::semibold())
    } else {
        FontId::proportional(12.5)
    };
    let g = painter.layout_no_wrap(text.to_string(), font, color);
    let w = g.size().x;
    let x = c.x.clamp(within.left() + w / 2.0 + 3.0, (within.right() - w / 2.0 - 3.0).max(within.left()));
    painter.galley(pos2(x - w / 2.0, c.y + r + 6.0), g, color);
}

/// A zig-zag line, the coach's mark for a man dribbling.
fn zigzag(painter: &egui::Painter, from: Pos2, to: Pos2, color: Color32) {
    let d = to - from;
    let len = d.length();
    if len < 8.0 {
        return;
    }
    let u = d / len;
    let n = vec2(-u.y, u.x);
    let mut pts = vec![from];
    let steps = (len / 9.0).max(2.0) as usize;
    for i in 1..steps {
        let side = if i % 2 == 0 { 3.5 } else { -3.5 };
        pts.push(from + u * (len * i as f32 / steps as f32) + n * side);
    }
    pts.push(to - u * 6.0);
    painter.add(Shape::line(pts, Stroke::new(2.2, color)));
    painter.add(Shape::convex_polygon(
        vec![to, to - u * 10.0 + n * 5.0, to - u * 10.0 - n * 5.0],
        color,
        Stroke::NONE,
    ));
}

fn dashed_arrow(painter: &egui::Painter, from: Pos2, to: Pos2, color: Color32) {
    let d = to - from;
    let len = d.length();
    if len < 8.0 {
        return;
    }
    let u = d / len;
    let n = vec2(-u.y, u.x);
    painter.extend(Shape::dashed_line(
        &[from, to - u * 6.0],
        Stroke::new(2.0, color),
        6.0,
        4.0,
    ));
    painter.add(Shape::convex_polygon(
        vec![to, to - u * 10.0 + n * 5.0, to - u * 10.0 - n * 5.0],
        color,
        Stroke::NONE,
    ));
}

/// Draw the movement of a play from where the five stand: cuts as arrows,
/// the ball handler's dribble as a squiggle, a screen as a bar, passes
/// dashed.
pub(crate) fn draw_marks(
    painter: &egui::Painter,
    half: &Half,
    marks: &[Mark],
    spots: &[Vec2; 5],
    r: f32,
    color: Color32,
) {
    let at = |p: Vec2| half.at(p);
    let path = |who: usize, path: &[Vec2]| -> Vec<Pos2> {
        let mut out = vec![at(spots[who])];
        out.extend(path.iter().map(|&p| at(p)));
        out
    };
    for m in marks {
        match m {
            Mark::Cut { who, path: p } => {
                let pts = path(*who, p);
                for (k, w) in pts.windows(2).enumerate() {
                    let (a, b) = (w[0], w[1]);
                    let u = (b - a).normalized();
                    let a = if k == 0 { a + u * (r + 2.0) } else { a };
                    if k + 2 == pts.len() {
                        arrow(painter, a, b, color);
                    } else {
                        painter.line_segment([a, b], Stroke::new(2.5, color));
                    }
                }
            }
            Mark::Dribble { who, path: p } => {
                let pts = path(*who, p);
                for (k, w) in pts.windows(2).enumerate() {
                    let (a, b) = (w[0], w[1]);
                    let u = (b - a).normalized();
                    let a = if k == 0 { a + u * (r + 2.0) } else { a };
                    if k + 2 == pts.len() {
                        zigzag(painter, a, b, BALL);
                    } else {
                        painter.line_segment([a, b], Stroke::new(2.2, BALL));
                    }
                }
            }
            Mark::Screen { who, at: p, for_who } => {
                let s = at(spots[*who]);
                let b = at(*p);
                let toward = at(spots[*for_who]) - b;
                let u = if toward.length() > 1.0 {
                    toward.normalized()
                } else {
                    vec2(0.0, 1.0)
                };
                let n = vec2(-u.y, u.x);
                painter.line_segment([s + (b - s).normalized() * (r + 2.0), b], Stroke::new(2.0, color));
                painter.line_segment(
                    [b - n * (r * 0.9), b + n * (r * 0.9)],
                    Stroke::new(5.0, color),
                );
            }
            Mark::Pass { from, to, option } => {
                let (a, b) = (at(spots[*from]), at(spots[*to]));
                let u = (b - a).normalized();
                let c = if *option { faded(THEM, 220) } else { faded(theme::TEXT, 200) };
                dashed_arrow(painter, a + u * (r + 3.0), b - u * (r + 3.0), c);
            }
        }
    }
}

/// Where a screen is set: the screener beside the man, between him and the
/// basket, and the man's run off his shoulder. A body in the way is the one
/// thing the engine understands as a screen.
fn set_screen(board: &mut Board, screener: usize, user: usize) {
    let me = board.spots[user];
    let d = (HOOP - me).norm();
    let at = Board::clamp(v(me.x + d.x * 1.1, me.y + d.y * 1.1));
    board.spots[screener] = at;
    let side = if me.x > 0.0 { -1.0 } else { 1.0 };
    let go = at.toward(HOOP, 2.6);
    board.runs[user] = Some(Board::clamp(v(go.x + side * 1.0, go.y)));
    board.runs[screener] = Some(Board::clamp(v(HOOP.x + 0.4 * -side, HOOP.y + 1.8)));
}

/// A chip of text on the board, top left.
fn chip(painter: &egui::Painter, rect: Rect, text: &str) {
    if rect.width() < 420.0 {
        return;
    }
    let font = FontId::new(12.5, theme::semibold());
    let galley = painter.layout_no_wrap(text.to_string(), font, theme::TEXT);
    let size = galley.size() + vec2(18.0, 10.0);
    let r = Rect::from_min_size(rect.left_bottom() + vec2(8.0, -size.y - 8.0), size);
    painter.rect_filled(r, 8.0, Color32::from_rgba_unmultiplied(255, 255, 255, 215));
    painter.rect_stroke(r, 8.0, Stroke::new(1.0, theme::LINE_2), egui::StrokeKind::Inside);
    painter.galley(r.min + vec2(9.0, 5.0), galley, theme::TEXT);
}

impl App {
    fn twin_names(&self, ids: &[PlayerId]) -> Vec<(PlayerId, String)> {
        let last = |id: PlayerId| {
            let n = &self.world.players[id as usize].name;
            n.rsplit(' ').next().unwrap_or(n).to_string()
        };
        ids.iter()
            .map(|&id| {
                let l = last(id);
                let twin = ids.iter().any(|&o| o != id && last(o) == l);
                let n = &self.world.players[id as usize].name;
                let same = ids.iter().any(|&o| {
                    o != id && self.world.players[o as usize].name == *n
                });
                let shown = if same {
                    format!(
                        "{}. {l} ({})",
                        n.chars().next().unwrap_or(' '),
                        self.world.players[id as usize].position.label()
                    )
                } else if twin {
                    format!("{}. {l}", n.chars().next().unwrap_or(' '))
                } else {
                    l
                };
                (id, shown)
            })
            .collect()
    }

    /// The club we test against: the next opponent, unless a sample club
    /// was asked for.
    pub(crate) fn test_opponent(&self) -> (usize, bool) {
        match next_opponent(&self.world, self.team) {
            Some(o) if !self.room.vs_sample => (o, true),
            _ => (sample_opponent(&self.world, self.team), false),
        }
    }

    /// Play the scrimmage for what the board is showing and start watching.
    pub(crate) fn run_it(&mut self) {
        let focus = if self.room.with_ball {
            Focus::Attack
        } else {
            Focus::Defend
        };
        let mut s = Setup::for_slot(&self.world, self.team, self.slot, focus);
        s.opponent = self.test_opponent().0;
        if focus == Focus::Defend {
            s.their_offense = self.room.sample;
        }
        let scrim = scrimmage::run(&self.world, &s);
        let rows = scrimmage::compare(&self.world, &s, self.slot, 4);
        self.room.play = Some(Playback {
            with: s.tactics.clone(),
            scrim,
            idx: 0,
            t: 0.0,
            paused: false,
            speed: 2.0,
            hold: 0.0,
            rows,
        });
    }

    /// Drop a scrimmage that was run with something other than what is now
    /// in force.
    pub(crate) fn retire_stale_run(&mut self) {
        let now = &self.world.teams[self.team].playbook[self.slot].tactics;
        if self.room.play.as_ref().is_some_and(|p| p.with != *now) {
            self.room.play = None;
        }
    }

    /// The board, `size` points of it, with the caption under it.
    pub(crate) fn tactics_board(&mut self, ui: &mut Ui, size: egui::Vec2) {
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let half = Half::fit(rect);
        let painter = ui.painter_at(rect.expand(2.0));
        paint_court(&painter, &half);
        self.room.spots.board = Some(half.rect);
        if self.room.play.is_some() {
            self.paint_playback(ui, &painter, &half);
        } else if self.room.with_ball {
            self.paint_attack(ui, &painter, &half);
        } else {
            self.paint_defense(&painter, &half);
        }
    }

    fn paint_attack(&mut self, ui: &mut Ui, painter: &egui::Painter, half: &Half) {
        let slot = self.slot;
        let mut tactics = self.world.teams[self.team].playbook[slot].tactics.clone();
        let preview = self.room.hover.filter(|c| !c.is_on(&tactics));
        if let Some(c) = preview {
            c.apply(&mut tactics);
        }
        let drawn = tactics
            .board
            .as_ref()
            .is_some_and(|b| b.set == tactics.offense);
        let mut board = tactics.layout();
        let before = board.clone();
        let cast = tactics.cast(&self.world.players);
        let r = half.token();
        let at = |p: Vec2| half.at(p);
        let tool = self.room.tool;

        // Their defense, faint, against our spots.
        let opp = {
            let (o, _) = self.test_opponent();
            scout(&self.world, o)
        };
        if self.room.show_them {
            let d = shape(opp.defense, &board.spots);
            let man = !matches!(opp.defense, DefenseScheme::Zone23 | DefenseScheme::Zone32);
            for (k, p) in d.iter().enumerate() {
                let c = at(*p);
                if !man {
                    painter.circle_filled(c, 1.9 * half.scale, faded(RED, 14));
                    painter.circle_stroke(c, 1.9 * half.scale, Stroke::new(1.0, faded(RED, 70)));
                }
                painter.circle_filled(c, r * 0.8, faded(THEM, 120));
                painter.circle_stroke(c, r * 0.8, Stroke::new(1.0, faded(THEM, 200)));
                painter.text(
                    c,
                    Align2::CENTER_CENTER,
                    "D",
                    FontId::new(r * 0.7, theme::mono_semibold()),
                    Color32::WHITE,
                );
                if man {
                    painter.extend(Shape::dashed_line(
                        &[c, at(board.spots[k])],
                        Stroke::new(1.0, faded(THEM, 110)),
                        3.0,
                        3.0,
                    ));
                }
            }
        }

        // The play.
        if drawn {
            for i in 0..5 {
                if let Some(to) = board.runs[i] {
                    let (a, b) = (at(board.spots[i]), at(to));
                    let u = (b - a).normalized();
                    arrow(painter, a + u * (r + 2.0), b, theme::ACCENT);
                }
            }
        } else {
            draw_marks(
                painter,
                half,
                &basketman_core::fit::play(tactics.offense),
                &board.spots,
                r,
                theme::ACCENT,
            );
        }

        let mut push = false;
        for i in 0..5 {
            let c = at(board.spots[i]);
            let id = ui.scope_id().with(("tb_man", i));
            let sense = if preview.is_some() {
                Sense::hover()
            } else {
                Sense::click_and_drag()
            };
            let hit = ui.interact(Rect::from_center_size(c, vec2(r * 2.6, r * 2.6)), id, sense);
            if preview.is_none() {
                match tool {
                    Tool::Move => {
                        if hit.drag_started() {
                            push = true;
                        }
                        if hit.dragged()
                            && let Some(p) = hit.interact_pointer_pos()
                        {
                            board.spots[i] = half.back(p);
                        }
                    }
                    Tool::Cut => {
                        if hit.drag_started() {
                            push = true;
                        }
                        if hit.dragged()
                            && let Some(p) = hit.interact_pointer_pos()
                        {
                            board.runs[i] = Some(half.back(p));
                        }
                        if hit.clicked() && board.runs[i].is_some() {
                            push = true;
                            board.runs[i] = None;
                        }
                    }
                    Tool::Screen => {
                        if hit.clicked() {
                            match self.room.tap {
                                None => self.room.tap = Some(i),
                                Some(a) if a != i => {
                                    push = true;
                                    set_screen(&mut board, a, i);
                                    self.room.tap = None;
                                }
                                Some(_) => self.room.tap = None,
                            }
                        }
                    }
                }
                if (hit.clicked() || hit.drag_started()) && cast[i].is_some() && tool != Tool::Screen
                {
                    self.room.selected = cast[i];
                }
            }
            let tip = {
                let player = cast[i].map(|p| &self.world.players[p as usize]);
                let (note, attrs) = basketman_core::fit::place_needs(i);
                match player {
                    Some(p) => {
                        let f = place_fit(tactics.offense, i, p);
                        let weak = attrs
                            .iter()
                            .min_by(|a, b| p.attrs[**a].total_cmp(&p.attrs[**b]))
                            .map(|a: &Attr| a.name())
                            .unwrap_or("");
                        format!(
                            "{} as {}: {:.0}% fit for the job. {note} His weakest part of it: {}.",
                            p.name,
                            BOARD_ROLES[i].to_lowercase(),
                            f * 100.0,
                            weak.to_lowercase()
                        )
                    }
                    None => format!("{}: {note}", BOARD_ROLES[i]),
                }
            };
            if preview.is_none() {
                hit.clone().on_hover_text(tip);
            }
            let player = cast[i].map(|p| &self.world.players[p as usize]);
            let selected = cast[i].is_some() && cast[i] == self.room.selected;
            let tapped = self.room.tap == Some(i);
            if selected || tapped {
                painter.circle_filled(c, r + 9.0, theme::ACCENT.gamma_multiply(0.16));
            }
            let fit = player.map(|p| place_fit(tactics.offense, i, p));
            if let Some(f) = fit {
                ring(painter, c, r, f);
            }
            let pos = player.and_then(|p| {
                let s = tactics.rotation.iter().find(|s| s.player == p.id)?;
                Some(s.position.unwrap_or(p.position))
            });
            token(
                painter,
                c,
                r,
                if hit.hovered() || hit.dragged() || tapped {
                    theme::TEXT
                } else {
                    theme::ACCENT
                },
                &pos.map_or((i + 1).to_string(), |p| p.label().to_string()),
            );
            if i == 0 {
                painter.circle_filled(c + vec2(r * 0.85, -r * 0.85), 5.5, BALL);
            }
            if let Some(p) = player {
                let ids: Vec<PlayerId> = cast.iter().flatten().copied().collect();
                let names = self.twin_names(&ids);
                let shown = names
                    .iter()
                    .find(|(id, _)| *id == p.id)
                    .map_or("", |(_, n)| n.as_str());
                label(
                    painter,
                    c,
                    r + 4.0,
                    shown,
                    if selected { theme::ACCENT } else { theme::TEXT },
                    selected,
                    half.rect,
                );
            }
        }
        // The caption chip.
        let title = match preview {
            Some(c) => format!("Preview: {}", c.title()),
            None => format!(
                "{}, with the ball{}",
                tactics.offense.label(),
                if drawn { " (your drawing)" } else { "" }
            ),
        };
        chip(painter, half.rect, &title);
        if self.room.show_them && half.rect.width() >= 420.0 {
            painter.text(
                half.rect.right_bottom() + vec2(-10.0, -8.0),
                Align2::RIGHT_BOTTOM,
                format!("grey: {} defend in {}", opp.name, opp.defense.label().to_lowercase()),
                FontId::proportional(12.0),
                theme::TEXT_MUTE,
            );
        }

        if board != before {
            if push {
                self.room.history.push(before);
                if self.room.history.len() > 60 {
                    self.room.history.remove(0);
                }
            }
            let mut t = self.world.teams[self.team].playbook[slot].tactics.clone();
            t.draw(board);
            let _ = self.world.apply(Command::SetTactics { slot, tactics: t });
        } else if push {
            self.room.history.push(before);
        }
    }

    fn paint_defense(&mut self, painter: &egui::Painter, half: &Half) {
        let slot = self.slot;
        let mut tactics = self.world.teams[self.team].playbook[slot].tactics.clone();
        let preview = self.room.hover.filter(|c| !c.is_on(&tactics));
        if let Some(c) = preview {
            c.apply(&mut tactics);
        }
        let opp = scout(&self.world, self.test_opponent().0);
        let sample = self.room.sample.unwrap_or(opp.offense);
        let attack = Board::stock(sample);
        let defenders = shape(tactics.defense, &attack.spots);
        let man = !matches!(tactics.defense, DefenseScheme::Zone23 | DefenseScheme::Zone32);
        let r = half.token();
        let at = |p: Vec2| half.at(p);
        let cast = tactics.cast(&self.world.players);

        if !man {
            for d in defenders {
                painter.circle_filled(at(d), 1.9 * half.scale, theme::ACCENT.gamma_multiply(0.08));
                painter.circle_stroke(
                    at(d),
                    1.9 * half.scale,
                    Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.35)),
                );
            }
        }
        if tactics.defense == DefenseScheme::PackLine {
            let pts: Vec<Pos2> = (0..=64)
                .map(|i| {
                    let a = std::f32::consts::PI * i as f32 / 64.0;
                    at(v(PACK * a.cos(), HOOP.y + PACK * a.sin()))
                })
                .collect();
            painter.extend(Shape::dashed_line(&pts, Stroke::new(2.0, theme::ACCENT), 7.0, 5.0));
            painter.text(
                at(v(PACK * 0.72, HOOP.y + PACK * 0.72)) + vec2(8.0, 0.0),
                Align2::LEFT_CENTER,
                "pack line",
                FontId::proportional(12.0),
                theme::ACCENT,
            );
        }
        // The sample offense: where it stands and what it does.
        draw_marks(
            painter,
            half,
            &basketman_core::fit::play(sample),
            &attack.spots,
            r,
            faded(THEM, 230),
        );
        for (i, a) in attack.spots.iter().enumerate() {
            let c = at(*a);
            if man {
                painter.extend(Shape::dashed_line(
                    &[at(defenders[i]), c],
                    Stroke::new(1.2, crate::board::CHALK),
                    4.0,
                    4.0,
                ));
            }
            token(painter, c, r * 0.9, THEM, &(i + 1).to_string());
            if i == 0 {
                painter.circle_filled(c + vec2(r * 0.8, -r * 0.8), 5.0, BALL);
            }
        }
        if tactics.defense == DefenseScheme::SwitchEverything {
            let (a, b) = (at(defenders[0]), at(defenders[4]));
            let u = (b - a).normalized();
            arrow(painter, a + u * (r + 2.0), b - u * (r + 2.0), theme::WARN);
            arrow(painter, b - u * (r + 2.0), a + u * (r + 2.0), theme::WARN);
        }
        let ids: Vec<PlayerId> = cast.iter().flatten().copied().collect();
        let names = self.twin_names(&ids);
        for (i, d) in defenders.iter().enumerate() {
            let c = at(*d);
            let player = cast[i].map(|p| &self.world.players[p as usize]);
            if let Some(p) = player {
                let f = ((p.attrs[Attr::PerimeterD] + p.attrs[Attr::InteriorD]) / 2.0 - 4.0) / 12.0;
                ring(painter, c, r, f);
            }
            token(painter, c, r, theme::ACCENT, &(i + 1).to_string());
            if let Some(p) = player {
                let shown = names
                    .iter()
                    .find(|(id, _)| *id == p.id)
                    .map_or("", |(_, n)| n.as_str());
                label(painter, c, r + 4.0, shown, theme::TEXT, false, half.rect);
            }
        }
        if tactics.press != basketman_core::tactics::Press::None {
            painter.text(
                half.rect.right_bottom() - vec2(10.0, 8.0),
                Align2::RIGHT_BOTTOM,
                format!("{} press first", tactics.press.label().to_lowercase()),
                FontId::new(12.0, theme::semibold()),
                theme::WARN,
            );
        }
        let title = match preview {
            Some(c) => format!("Preview: {}", c.title()),
            None => format!(
                "{}, against {}",
                tactics.defense.label(),
                sample.label().to_lowercase()
            ),
        };
        chip(painter, half.rect, &title);
        if half.rect.width() >= 420.0 {
            painter.text(
            half.rect.right_bottom() + vec2(-10.0, -8.0),
            Align2::RIGHT_BOTTOM,
            "purple: your defenders, grey: the attack",
            FontId::proportional(12.0),
            theme::TEXT_MUTE,
        );
        }
    }

    /// The plain words under the board when nothing is being played.
    pub(crate) fn board_caption(&self) -> String {
        let t = &self.world.teams[self.team].playbook[self.slot].tactics;
        let t = match self.room.hover {
            Some(c) => {
                let mut x = t.clone();
                c.apply(&mut x);
                x
            }
            None => t.clone(),
        };
        if self.room.hover.is_some_and(|c| !c.is_on(&self.world.teams[self.team].playbook[self.slot].tactics)) {
            return format!(
                "Preview of {}. {} Press Use this on the card to keep it.",
                self.room.hover.map_or("", |c| c.title()),
                self.room.hover.map_or("", |c| c.line())
            );
        }
        if self.room.with_ball {
            let drawn = t.board.as_ref().is_some_and(|b| b.set == t.offense);
            format!(
                "{} {}",
                t.offense.line(),
                if drawn {
                    "The positions and cuts are the ones you drew."
                } else {
                    "Purple is your five, with the ring showing how well each man fits his job."
                }
            )
        } else {
            scheme_shape(t.defense).to_string()
        }
    }

    // ---- playback ------------------------------------------------------------

    fn paint_playback(&mut self, ui: &mut Ui, painter: &egui::Painter, half: &Half) {
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        let Some(pb) = self.room.play.as_mut() else {
            return;
        };
        if pb.idx >= pb.scrim.possessions.len() {
            pb.idx = pb.scrim.possessions.len().saturating_sub(1);
        }
        let Some(poss) = pb.scrim.possessions.get(pb.idx) else {
            return;
        };
        // Start where the ball comes into the half court.
        let start = poss
            .moments
            .iter()
            .position(|m| m.ball.y <= 11.5)
            .unwrap_or(0)
            .saturating_sub(6);
        let t0 = poss.moments[start].t;
        let dur = poss.moments.last().map_or(0.0, |m| m.t) - t0;
        if !pb.paused {
            if pb.t < dur {
                pb.t = (pb.t + dt * pb.speed).min(dur);
            } else {
                pb.hold += dt;
                if pb.hold > 2.2 / pb.speed.max(1.0).sqrt() {
                    if pb.idx + 1 < pb.scrim.possessions.len() {
                        pb.idx += 1;
                        pb.t = 0.0;
                        pb.hold = 0.0;
                    } else {
                        pb.paused = true;
                    }
                }
            }
            ui.ctx().request_repaint();
        }
        let poss = &pb.scrim.possessions[pb.idx];
        let now = t0 + pb.t;
        let k = poss
            .moments
            .partition_point(|m| m.t <= now)
            .saturating_sub(1)
            .max(start);
        let m = poss.moments[k].clone();
        let m = &m;
        let ended = pb.t >= dur;
        let (idx, total) = (pb.idx, pb.scrim.possessions.len());
        let (outcome, points, we_attack) = (poss.outcome, poss.points, poss.we_attack);
        let attackers_ids = poss.attackers;
        let defenders_ids = poss.defenders;
        let moments = poss.moments.clone();
        let _ = pb;

        let r = half.token();
        let at = |p: Vec2| half.at(p);
        let mut ids: Vec<PlayerId> = attackers_ids.to_vec();
        ids.extend(defenders_ids);
        let names = self.twin_names(&ids);
        let nm = |id: PlayerId| {
            names
                .iter()
                .find(|(i, _)| *i == id)
                .map_or("", |(_, n)| n.as_str())
        };

        // Trails of the attack, and the passes so far.
        for a in 0..5 {
            let from = k.saturating_sub(22).max(start);
            let pts: Vec<Pos2> = (from..=k).map(|j| at(moments[j].attackers[a])).collect();
            if pts.len() > 1 {
                painter.add(Shape::line(
                    pts,
                    Stroke::new(
                        2.0,
                        faded(if we_attack { theme::ACCENT } else { THEM }, 90),
                    ),
                ));
            }
        }
        let mut last: Option<(usize, Vec2)> = None;
        let mut passes: Vec<(Vec2, Vec2)> = Vec::new();
        let mut gone_from: Option<(usize, Vec2)> = None;
        for mm in &moments[start..=k] {
            match mm.holder {
                Some(h) => {
                    if let Some((p, pos)) = gone_from
                        && p != h
                    {
                        passes.push((pos, mm.attackers[h]));
                    }
                    gone_from = None;
                    last = Some((h, mm.attackers[h]));
                }
                None => {
                    if gone_from.is_none() {
                        gone_from = last;
                    }
                }
            }
        }
        let n = passes.len();
        for (j, (a, b)) in passes.iter().enumerate().skip(n.saturating_sub(3)) {
            let alpha = if j + 1 == n { 230 } else { 120 };
            let (a, b) = (at(*a), at(*b));
            let u = (b - a).normalized();
            dashed_arrow(painter, a + u * (r + 2.0), b - u * (r + 3.0), faded(theme::TEXT, alpha));
        }

        // Defenders, then attackers, then the ball.
        let (dcol, acol) = if we_attack {
            (THEM, theme::ACCENT)
        } else {
            (theme::ACCENT, THEM)
        };
        for d in 0..5 {
            let c = at(m.defenders[d]);
            token(painter, c, r * 0.9, dcol, "D");
            label(painter, c, r * 0.9, nm(defenders_ids[d]), faded(theme::TEXT, 190), false, half.rect);
        }
        for a in 0..5 {
            let c = at(m.attackers[a]);
            if m.holder == Some(a) {
                painter.circle_filled(c, r + 6.0, faded(BALL, 60));
            }
            let pos = self.world.players[attackers_ids[a] as usize].position.label();
            token(painter, c, r, acol, pos);
            label(painter, c, r, nm(attackers_ids[a]), theme::TEXT, m.holder == Some(a), half.rect);
        }
        let ball = match m.holder {
            Some(h) => at(m.attackers[h]) + vec2(r * 0.9, -r * 0.9),
            None => at(m.ball) + vec2(0.0, -m.ball_height * half.scale * 0.35),
        };
        painter.circle_filled(ball, 5.5, BALL);
        painter.circle_stroke(ball, 5.5, Stroke::new(1.0, Color32::WHITE));

        chip(
            painter,
            half.rect,
            &format!(
                "{} {}/{}: {}",
                if we_attack { "Our possession" } else { "Their possession" },
                idx + 1,
                total,
                if we_attack { "we attack" } else { "we defend" }
            ),
        );
        painter.text(
            half.rect.right_bottom() + vec2(-10.0, -8.0),
            Align2::RIGHT_BOTTOM,
            format!("shot clock {:.0}", m.shot_clock.max(0.0)),
            FontId::new(12.0, theme::mono_semibold()),
            theme::TEXT_MUTE,
        );
        if ended {
            let (text, color) = match (outcome, points) {
                (Outcome::Scored, p) => (
                    format!("{}{}", if we_attack { "+" } else { "They score " }, p),
                    if we_attack { theme::POS } else { RED },
                ),
                (Outcome::Turnover, _) => (
                    if we_attack { "Turnover".to_string() } else { "They turn it over".to_string() },
                    if we_attack { RED } else { theme::POS },
                ),
                (Outcome::Fouled, _) => ("Fouled".to_string(), AMBER),
                (Outcome::Missed, _) => (
                    if we_attack { "Miss".to_string() } else { "They miss".to_string() },
                    if we_attack { RED } else { theme::POS },
                ),
            };
            let c = half.rect.center_bottom() + vec2(0.0, -26.0);
            let font = FontId::new(18.0, theme::semibold());
            let g = painter.layout_no_wrap(text, font, Color32::WHITE);
            let rr = Rect::from_center_size(c, g.size() + vec2(24.0, 12.0));
            painter.rect_filled(rr, 10.0, color);
            painter.galley(rr.min + vec2(12.0, 6.0), g, Color32::WHITE);
        }
    }

    /// Buttons under the board while a scrimmage is up, and its caption.
    pub(crate) fn playback_bar(&mut self, ui: &mut Ui) {
        let Some(pb) = self.room.play.as_mut() else {
            return;
        };
        let total = pb.scrim.possessions.len();
        if total == 0 {
            ui.label(dim("No possession was played."));
            return;
        }
        let poss = &pb.scrim.possessions[pb.idx.min(total - 1)];
        let start = poss
            .moments
            .iter()
            .position(|m| m.ball.y <= 11.5)
            .unwrap_or(0)
            .saturating_sub(6);
        let dur = poss.moments.last().map_or(0.0, |m| m.t) - poss.moments[start].t;
        let reveal = pb.t >= dur * 0.8 || pb.paused;
        let caption = if reveal {
            poss.caption.clone()
        } else {
            "Watch the possession...".to_string()
        };
        let mut stop = false;
        ui.label(RichText::new(caption).color(theme::TEXT).size(14.0));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if ui.add_enabled(pb.idx > 0, egui::Button::new("Prev")).clicked() {
                pb.idx -= 1;
                pb.t = 0.0;
                pb.hold = 0.0;
                pb.paused = false;
            }
            if ui
                .button(if pb.paused { "Play" } else { "Pause" })
                .clicked()
            {
                pb.paused = !pb.paused;
                if pb.paused && pb.t >= dur && pb.idx + 1 == total {
                    pb.idx = 0;
                    pb.t = 0.0;
                    pb.hold = 0.0;
                    pb.paused = false;
                }
            }
            if ui
                .add_enabled(pb.idx + 1 < total, egui::Button::new("Next"))
                .clicked()
            {
                pb.idx += 1;
                pb.t = 0.0;
                pb.hold = 0.0;
                pb.paused = false;
            }
            if ui
                .button(format!("{:.0}x", pb.speed))
                .on_hover_text("Playback speed")
                .clicked()
            {
                pb.speed = if pb.speed >= 4.0 { 1.0 } else { pb.speed * 2.0 };
            }
            if ui.button("Close").on_hover_text("Back to the board").clicked() {
                stop = true;
            }
        });
        if stop {
            self.room.play = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screen_puts_a_body_between_the_man_and_the_basket() {
        let mut b = Board::stock(OffenseSet::Motion);
        let (user, screener) = (1, 4);
        let me = b.spots[user];
        set_screen(&mut b, screener, user);
        assert!(b.spots[screener].dist(HOOP) < me.dist(HOOP));
        assert!(b.spots[screener].dist(me) < 1.5);
        assert!(b.runs[user].is_some() && b.runs[screener].is_some());
        // Still on the floor.
        assert_eq!(Board::clamp(b.spots[screener]), b.spots[screener]);
    }

    #[test]
    fn the_board_fits_its_area_and_keeps_the_court_shape() {
        let h = Half::fit(Rect::from_min_size(pos2(0.0, 0.0), vec2(500.0, 300.0)));
        assert!(h.rect.height() <= 300.0 + 0.01 && h.rect.width() <= 500.0 + 0.01);
        let ratio = h.rect.width() / h.rect.height();
        assert!((ratio - 15.0 / 12.6).abs() < 0.01);
        let p = v(1.5, 4.0);
        assert!(h.back(h.at(p)).dist(p) < 0.01);
    }
}
