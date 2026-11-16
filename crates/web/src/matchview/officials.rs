//! The officials on the match screen: the three on the floor, the whistle,
//! the coach's challenge and what the film showed afterwards.
//!
//! Everything here is read from the recording (`Frame::officials`,
//! `Frame::whistler`, `Recording::calls`, the review events). During play
//! only what was CALLED is shown; what really happened is shown after the
//! game, in the box score.

use super::*;
use crate::shell::glass_alpha;
use basketman_core::officials::{CallRecord, Opinion, ReviewPrompt, Ruling, What};
use basketman_core::replay::Event;

/// How long, in replay seconds, a challenge is "under review".
pub(super) const UNDER: f32 = 1.5;
/// How long the result stays up after that.
pub(super) const SHOWN: f32 = 3.0;
/// How long a whistle stays on screen after it blew.
const WHISTLE: f32 = 3.0;

const SHIRT: Color32 = Color32::from_rgb(52, 52, 58);
const STRIPE: Color32 = Color32::from_rgb(190, 190, 196);

// ---- pure helpers ------------------------------------------------------

fn side_name(side: u8) -> &'static str {
    if side == 0 { "Home" } else { "Away" }
}

/// What a ruling says, short, for the lists.
pub(super) fn ruling_text(r: Ruling) -> String {
    match r {
        Ruling::Nothing => "no foul".to_string(),
        Ruling::Foul { kind, by } | Ruling::Marginal { kind, by } => {
            format!("{} ({})", kind.label().to_lowercase(), side_name(by))
        }
        Ruling::Ball(s) => format!("ball to {}", side_name(s)),
        Ruling::Value(n) => format!("{n} points"),
    }
}

/// The whistle as it was called, for the chip near the score.
pub(super) fn whistle_text(c: &CallRecord, who: &str) -> String {
    match c.called {
        Ruling::Foul { kind, by } | Ruling::Marginal { kind, by } => {
            if kind.is_offensive() {
                format!("Whistle: {}, {who} ({})", kind.label(), side_name(by))
            } else {
                format!("Whistle: {} foul, {who} ({})", kind.label(), side_name(by))
            }
        }
        Ruling::Ball(s) => format!("Whistle: out of bounds, ball to {}", side_name(s)),
        Ruling::Value(n) => format!("Whistle: basket counts {n}"),
        Ruling::Nothing => "Whistle".to_string(),
    }
}

/// The call the whistling official (1 to 3) has just made, if the record
/// has it.
pub(super) fn whistle_call(calls: &[CallRecord], whistler: u8, time: f32) -> Option<&CallRecord> {
    if whistler == 0 {
        return None;
    }
    calls
        .iter()
        .rev()
        .filter(|c| c.time <= time)
        .take_while(|c| time - c.time < WHISTLE)
        .find(|c| c.by + 1 == whistler && c.called != Ruling::Nothing)
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Under,
    Overturned,
    Stands,
}

impl Stage {
    pub fn text(self) -> &'static str {
        match self {
            Stage::Under => "Under review",
            Stage::Overturned => "Overturned",
            Stage::Stands => "Call stands",
        }
    }
}

/// Where a challenge is at `time`: under review for `UNDER` seconds from
/// the moment it was made, then its result for `SHOWN`.
pub(super) fn review_stage(events: &[Event], time: f32) -> Option<(u8, What, Stage)> {
    events
        .iter()
        .rev()
        .filter(|e| e.time <= time)
        .take_while(|e| time - e.time < UNDER + SHOWN)
        .find_map(|e| match &e.kind {
            EventKind::Review {
                side,
                overturned,
                what,
            } => {
                let age = time - e.time;
                let stage = if age < UNDER {
                    Stage::Under
                } else if *overturned {
                    Stage::Overturned
                } else {
                    Stage::Stands
                };
                Some((*side, *what, stage))
            }
            _ => None,
        })
}

/// The recording carries officials (old saves do not).
pub(super) fn present(f: &Frame) -> bool {
    f.officials.iter().any(|p| p.x != 0.0 || p.y != 0.0)
}

/// Where the three stand, glided to the next frame the way players are.
pub(super) fn spots(a: &Frame, next: Option<(&Frame, f32)>) -> [Vec2; 3] {
    let mut out = a.officials;
    if let Some((b, u)) = next {
        for i in 0..3 {
            let (p, q) = (a.officials[i], b.officials[i]);
            // A jump is a cut, not a run.
            if p.dist(q) < 4.0 {
                out[i] = p.lerp(q, u);
            }
        }
    }
    out
}

/// How fast each is moving, metres per second.
pub(super) fn speeds(a: &Frame, next: Option<(&Frame, f32)>) -> [Vec2; 3] {
    let mut out = [Vec2::ZERO; 3];
    if let Some((b, _)) = next {
        let span = b.time - a.time;
        if span > 0.0 {
            for i in 0..3 {
                let d = b.officials[i] - a.officials[i];
                if d.len() < 4.0 {
                    out[i] = d * (1.0 / span);
                }
            }
        }
    }
    out
}

// ---- the court ---------------------------------------------------------

/// The three on a flat court: dark discs with two light stripes, nothing
/// like a player, and an arm up on the one who blew the whistle.
pub(super) fn paint_flat(painter: &egui::Painter, pitch: &Pitch, frame: &Frame, next: Option<(&Frame, f32)>) {
    if !present(frame) {
        return;
    }
    let r = (0.4 * pitch.unit).max(6.0);
    for (i, spot) in spots(frame, next).into_iter().enumerate() {
        let at = pitch.of(spot);
        painter.circle_filled(at + vec2(1.0, 2.0), r, Color32::from_black_alpha(40));
        painter.circle_filled(at, r, SHIRT);
        for dx in [-0.38f32, 0.38] {
            let h = r * (1.0 - dx * dx).sqrt() * 0.9;
            painter.line_segment(
                [at + vec2(dx * r, -h), at + vec2(dx * r, h)],
                Stroke::new((r * 0.16).max(1.0), STRIPE),
            );
        }
        painter.circle_stroke(at, r, Stroke::new(1.0, STRIPE));
        if frame.whistler as usize == i + 1 {
            let hand = at + vec2(r * 1.5, -r * 1.9);
            painter.line_segment(
                [at + vec2(r * 0.75, -r * 0.65), hand],
                Stroke::new((r * 0.3).max(2.0), Color32::from_rgb(240, 200, 60)),
            );
            painter.circle_filled(hand, (r * 0.3).max(2.5), Color32::from_rgb(240, 200, 60));
        }
    }
}

/// The three as figures: where the recording has them, walking as they
/// moved, facing the way they went or, standing, the ball.
pub(super) fn emit_3d(scene: &mut Scene, frame: &Frame, next: Option<(&Frame, f32)>, ball: Vec2) {
    if !present(frame) {
        return;
    }
    let at = spots(frame, next);
    let vel = speeds(frame, next);
    for i in 0..3 {
        let (p, v) = (at[i], vel[i]);
        let speed = v.len();
        let face = if speed > 0.4 {
            v.norm()
        } else if p.dist(ball) > 0.1 {
            (ball - p).norm()
        } else {
            Vec2 { x: 1.0, y: 0.0 }
        };
        // Phase advances with the distance run along the way he faces.
        let phase = (p.x * face.x + p.y * face.y) / 1.4 * std::f32::consts::TAU;
        let gait = view3d::gait::Gait {
            vel: v,
            face,
            stance: 0.0,
            phase,
            dribble: 0.0,
            // He walks or jogs where he is going and stands facing the ball; the
            // recording has no legwork for him, so the plain run decides.
            legs: view3d::gait::Legs {
                foot: if speed > 0.4 {
                    basketman_core::replay::Footwork::Run
                } else {
                    basketman_core::replay::Footwork::Stand
                },
                ..Default::default()
            },
        };
        scene.ground_disc((p.x, p.y), 0.4, 0.32, Color32::from_black_alpha(60));
        view3d::emit_official(scene, i as u8, p, gait, frame.whistler as usize == i + 1);
    }
}

// ---- the screen --------------------------------------------------------

fn chip(ui: &Ui, top_centre: Pos2, text: &str, color: Color32, solid: f32, max_w: f32) {
    let font = FontId::new(13.0, theme::semibold());
    let galley = ui.painter().layout_no_wrap(text.to_string(), font.clone(), color);
    let w = (galley.size().x + 26.0).min(max_w).max(80.0);
    let r = Rect::from_min_size(pos2(top_centre.x - w / 2.0, top_centre.y), vec2(w, 30.0));
    glass_alpha(ui, r, 10, true, solid.max(0.8));
    ui.painter()
        .with_clip_rect(r)
        .text(r.center(), Align2::CENTER_CENTER, text, font, color);
}

impl App {
    /// The upcoming league game of the user's club, for the crew strip
    /// before kickoff.
    pub(super) fn next_fixture(&self) -> Option<usize> {
        let w = &self.world;
        (0..w.fixtures.len()).find(|&i| {
            let f = &w.fixtures[i];
            f.round == w.round && f.result.is_none() && (f.home == self.team || f.away == self.team)
        })
    }

    /// The crew, names and what each is known for.
    pub(super) fn crew_card(&self, ui: &mut Ui, bd: &Backdrop, ids: [basketman_core::officials::RefId; 3]) {
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Officials");
            for r in self.world.referees_of(ids) {
                ui.horizontal_wrapped(|ui| {
                    ui.label(name(&r.name));
                    let tags = r.tags();
                    if !tags.is_empty() {
                        ui.label(RichText::new(tags.join(", ")).color(theme::TEXT_DIM));
                    }
                });
            }
        });
    }

    /// Everything the officials add over the court: the challenge prompt
    /// (at every level, a live game waits on it), then, unless the screen
    /// is Clean, the challenge sequence, the whistle, and the crew's names
    /// at the start of a game.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn officials_layer(
        &mut self,
        ui: &mut Ui,
        rec: &Recording,
        live: Option<&LiveRound>,
        commands: Option<&mut Vec<Command>>,
        court_time: f32,
        level: u8,
        screen: Rect,
        chip_at: Pos2,
        solid: f32,
    ) {
        if let (Some(l), Some(c)) = (live, commands)
            && self.touchline.review_open
        {
            self.review_prompt(ui, l, c, screen, chip_at, solid);
        }
        if level >= 2 || rec.frames.is_empty() {
            return;
        }
        let frame = &rec.frames[rec.frame_at(court_time)];
        let max_w = (screen.right() - screen.left() - 24.0).min(520.0);
        let players = &self.world.players;
        if let Some((_, _, stage)) = review_stage(&rec.events, court_time) {
            let color = match stage {
                Stage::Under => theme::WARN,
                Stage::Overturned => theme::POS,
                Stage::Stands => theme::TEXT,
            };
            chip(ui, chip_at, stage.text(), color, solid, max_w);
            ui.ctx().request_repaint();
        } else if frame.whistle || frame.whistler > 0 {
            let text = whistle_call(&rec.calls, frame.whistler, court_time)
                .map(|c| whistle_text(c, &players[c.who as usize].name))
                .unwrap_or_else(|| "Whistle".to_string());
            chip(ui, chip_at, &text, theme::TEXT, solid, max_w);
        } else if level == 0 && live.is_some() && court_time < 30.0 {
            if let Some(ids) = self.next_crew_ids(live, rec) {
                let names: Vec<&str> = self
                    .world
                    .referees_of(ids)
                    .iter()
                    .map(|r| r.name.as_str())
                    .collect();
                chip(ui, chip_at, &format!("Officials: {}", names.join(", ")), theme::TEXT_DIM, solid, max_w);
            }
        }
    }

    /// The crew of the game on screen.
    fn next_crew_ids(&self, live: Option<&LiveRound>, _rec: &Recording) -> Option<[basketman_core::officials::RefId; 3]> {
        let i = live.map(|l| l.fixture).or(self.last_game)?;
        self.world.crew_of_fixture(i)
    }

    /// The question at a call that went against the user's bench. The game
    /// does not move until it is answered.
    fn review_prompt(
        &self,
        ui: &mut Ui,
        live: &LiveRound,
        commands: &mut Vec<Command>,
        screen: Rect,
        chip_at: Pos2,
        solid: f32,
    ) {
        let Some(p) = live.game.review() else {
            commands.push(Command::PlayOn);
            return;
        };
        let players = &self.world.players;
        let call = p.call_text(|id| players[id as usize].name.as_str());
        let by = live
            .game
            .crew()
            .and_then(|c| c.refs.get(p.by as usize))
            .and_then(|o| self.world.officials.get(o.id))
            .map(|r| r.name.clone());
        let can = prompt_can_challenge(p);
        let w = (screen.width() - 24.0).min(380.0);
        let compact = screen.height() < 500.0;
        let h = if compact { 156.0 } else { 196.0 };
        let y = (screen.center().y - h / 2.0).max(chip_at.y + 8.0);
        let r = Rect::from_min_size(pos2(screen.center().x - w / 2.0, y), vec2(w, h));
        glass_alpha(ui, r, 14, true, solid.max(0.9));
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(r.shrink(12.0))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(r);
        let ui = &mut child;
        ui.spacing_mut().item_spacing.y = 4.0;
        theme::eyebrow(ui, &format!("Call against you  P{} {}", p.period + 1, clock(p.clock)));
        ui.add(egui::Label::new(name(&call)).wrap());
        if let Some(by) = by.filter(|_| !compact) {
            ui.label(RichText::new(format!("Called by {by}")).color(theme::TEXT_DIM));
        }
        ui.label(
            RichText::new(format!("Assistant: {}", opinion_words(p.opinion))).color(theme::TEXT),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let go = ui.add_enabled(can, egui::Button::new("Challenge").min_size(vec2(120.0, 40.0)));
            if go.clicked() {
                commands.push(Command::Challenge);
            }
            if ui.add(egui::Button::new("Play on").min_size(vec2(100.0, 40.0))).clicked() {
                commands.push(Command::PlayOn);
            }
        });
        let small = if compact {
            "One challenge a game. A lost one costs a timeout. C or Space.".to_string()
        } else {
            format!(
                "One challenge a game; a lost one costs a timeout. Timeouts: {}. Keys: C challenges, Space lets it stand.",
                p.timeouts
            )
        };
        ui.add(egui::Label::new(RichText::new(small).size(12.0).color(theme::TEXT_DIM)).wrap());
    }

    /// After the game: who officiated, the fouls each way, the calls the
    /// film shows were wrong and each bench's challenge. The truth is shown
    /// only once the game is over.
    pub(super) fn officials_box(&self, ui: &mut Ui, bd: &Backdrop, result: &GameResult, teams: &[String; 2], live: bool) {
        let Some(o) = result.officiating.as_ref() else {
            return;
        };
        ui.add_space(6.0);
        bd.card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow(ui, "Officials");
            let refs = self.world.referees_of(o.crew);
            for r in &refs {
                ui.horizontal_wrapped(|ui| {
                    ui.label(name(&r.name));
                    let tags = r.tags();
                    if !tags.is_empty() {
                        ui.label(RichText::new(tags.join(", ")).color(theme::TEXT_DIM));
                    }
                });
            }
            ui.add_space(4.0);
            ui.label(format!(
                "Fouls called: {} {}, {} {}",
                teams[0], o.fouls[0], teams[1], o.fouls[1]
            ));
            if live {
                ui.label(
                    RichText::new("What the film shows is told after the game.").color(theme::TEXT_DIM),
                );
                return;
            }
            let wrong: Vec<&CallRecord> = o.notable.iter().filter(|c| c.verdict().is_wrong()).collect();
            ui.add_space(4.0);
            ui.label(
                RichText::new(if wrong.is_empty() {
                    "The film shows no wrong calls that mattered.".to_string()
                } else {
                    format!("Calls the film shows were wrong: {}", wrong.len())
                })
                .color(theme::TEXT_DIM),
            );
            let players = &self.world.players;
            for c in wrong.iter().take(8) {
                let who = refs.get(c.by as usize).map_or("Official", |r| r.name.as_str());
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "{} P{}  {} called {} on {}. Film: {}. ({})",
                            clock(c.clock),
                            c.period + 1,
                            who,
                            ruling_text(c.called),
                            players[c.who as usize].name,
                            ruling_text(c.truth),
                            c.verdict().label()
                        ))
                        .size(12.5),
                    )
                    .wrap(),
                );
            }
            if wrong.len() > 8 {
                ui.label(RichText::new(format!("and {} more.", wrong.len() - 8)).color(theme::TEXT_DIM));
            }
            ui.add_space(4.0);
            for side in 0..2 {
                let line = match &o.challenge[side] {
                    Some(c) => {
                        let res = if c.review.is_some_and(|r| r.overturned) {
                            "Overturned"
                        } else {
                            "The call stood"
                        };
                        format!(
                            "{} challenged {}. Film: {}. {}.",
                            teams[side],
                            ruling_text(c.called),
                            ruling_text(c.truth),
                            res
                        )
                    }
                    None => format!("{}: no challenge used.", teams[side]),
                };
                ui.add(egui::Label::new(line).wrap());
            }
        });
    }
}

fn opinion_words(o: Opinion) -> &'static str {
    o.words()
}

fn prompt_can_challenge(p: &ReviewPrompt) -> bool {
    p.left > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use basketman_core::court::v;
    use basketman_core::officials::{Review, Ruling};
    use basketman_core::replay::FoulKind;

    fn call(time: f32, by: u8, called: Ruling) -> CallRecord {
        CallRecord {
            time,
            period: 0,
            clock: 500.0,
            by,
            what: What::Foul,
            truth: Ruling::Nothing,
            called,
            who: 3,
            on: 4,
            gains: 1,
            swing: 0.0,
            reviewable: true,
            review: None::<Review>,
        }
    }

    fn event(time: f32, kind: EventKind) -> Event {
        Event {
            time,
            period: 0,
            clock: 500.0,
            offense: 0,
            score: [0, 0],
            kind,
        }
    }

    fn frame(time: f32, officials: [Vec2; 3], whistler: u8) -> Frame {
        Frame {
            time,
            period: 0,
            clock: 0.0,
            shot_clock: 24.0,
            score: [0, 0],
            offense: 0,
            pos: [[Vec2::ZERO; 5]; 2],
            who: [[0; 5]; 2],
            ball: Vec2::ZERO,
            ball_height: 0.0,
            holder: None,
            pose: Default::default(),
            whistle: whistler > 0,
            cut: false,
            swish: false,
            officials,
            whistler,
        }
    }

    #[test]
    fn the_whistle_says_what_was_called_and_nothing_else() {
        let c = call(10.0, 1, Ruling::Foul { kind: FoulKind::ReachIn, by: 0 });
        assert_eq!(whistle_text(&c, "Ruiz"), "Whistle: Reach-in foul, Ruiz (Home)");
        let c = call(10.0, 1, Ruling::Foul { kind: FoulKind::Charging, by: 1 });
        assert_eq!(whistle_text(&c, "Ruiz"), "Whistle: Charging, Ruiz (Away)");
        // Contact that was legal still reads as the foul that was called.
        let mut c = call(10.0, 1, Ruling::Foul { kind: FoulKind::Pushing, by: 1 });
        c.truth = Ruling::Nothing;
        assert!(!whistle_text(&c, "Ruiz").contains("no foul"));
    }

    #[test]
    fn the_whistle_belongs_to_the_official_who_blew_it_and_fades() {
        let foul = Ruling::Foul { kind: FoulKind::Holding, by: 0 };
        let calls = vec![call(5.0, 0, foul), call(20.0, 2, foul)];
        assert_eq!(whistle_call(&calls, 3, 20.5).map(|c| c.time), Some(20.0));
        // Official 1 (index 0) did not blow this one.
        assert!(whistle_call(&calls, 1, 20.5).is_none());
        // Not yet blown, and long gone.
        assert!(whistle_call(&calls, 3, 19.0).is_none());
        assert!(whistle_call(&calls, 3, 25.0).is_none());
        assert!(whistle_call(&calls, 0, 20.5).is_none());
    }

    #[test]
    fn a_challenge_is_under_review_then_decided_then_gone() {
        let ev = vec![
            event(10.0, EventKind::PeriodStart),
            event(
                30.0,
                EventKind::Review { side: 0, overturned: true, what: What::Foul },
            ),
        ];
        assert_eq!(review_stage(&ev, 29.9), None);
        assert_eq!(review_stage(&ev, 30.0).map(|r| r.2), Some(Stage::Under));
        assert_eq!(review_stage(&ev, 31.4).map(|r| r.2), Some(Stage::Under));
        assert_eq!(review_stage(&ev, 31.6).map(|r| r.2), Some(Stage::Overturned));
        assert_eq!(review_stage(&ev, 30.0 + UNDER + SHOWN + 0.1), None);
        let ev = vec![event(
            30.0,
            EventKind::Review { side: 1, overturned: false, what: What::Foul },
        )];
        assert_eq!(review_stage(&ev, 32.0).map(|r| r.2), Some(Stage::Stands));
        assert_eq!(Stage::Stands.text(), "Call stands");
    }

    #[test]
    fn officials_glide_between_frames_but_not_across_a_cut() {
        let a = frame(1.0, [v(2.0, 2.0), v(10.0, 5.0), v(20.0, 9.0)], 0);
        let b = frame(1.2, [v(3.0, 2.0), v(10.0, 5.0), v(2.0, 9.0)], 0);
        let s = spots(&a, Some((&b, 0.5)));
        assert!(s[0].dist(v(2.5, 2.0)) < 1e-5);
        // A jump of eighteen metres is a cut: stay.
        assert!(s[2].dist(v(20.0, 9.0)) < 1e-5);
        let sp = speeds(&a, Some((&b, 0.5)));
        assert!((sp[0].x - 5.0).abs() < 1e-3);
        assert_eq!(sp[2], Vec2::ZERO);
        assert!(present(&a));
        assert!(!present(&frame(0.0, [Vec2::ZERO; 3], 0)));
    }

    #[test]
    fn the_prompt_can_be_challenged_while_the_one_challenge_is_in_hand() {
        let mut p = ReviewPrompt {
            side: 0,
            what: What::Foul,
            called: Ruling::Nothing,
            who: 1,
            on: 2,
            period: 0,
            clock: 300.0,
            by: 0,
            opinion: Opinion::LooksWrong,
            left: 1,
            timeouts: 2,
        };
        assert!(prompt_can_challenge(&p));
        // No timeout left: it can still be used, there is just nothing more to lose.
        p.timeouts = 0;
        assert!(prompt_can_challenge(&p));
        p.left = 0;
        assert!(!prompt_can_challenge(&p));
        assert_eq!(opinion_words(Opinion::LooksWrong), "Looked clean to me.");
    }
}
