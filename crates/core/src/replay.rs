//! What a watched game leaves behind: positions over time and a log of
//! everything that happened, including who was at fault.
//!
//! The event log is the single source of truth for commentary. `Event::text`
//! is the built-in phrasing; a language model can be handed the same events
//! and asked to phrase them better, but must not be asked what happened.

use serde::{Deserialize, Serialize};

use crate::court::{LENGTH, ShotKind, Vec2, v};
use crate::engine::Shout;
use crate::player::PlayerId;

/// Frames per second in a `Recording`.
pub const FRAME_RATE: f32 = 10.0;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurnoverKind {
    BadPass,
    Stripped,
    OffensiveFoul,
    ShotClock,
    Pressed,
    /// An attacker stood in the lane for more than three seconds.
    ThreeSeconds,
}

/// How a shot came about.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Transition,
    Drive,
    PullUp,
    CatchAndShoot,
    Cut,
    Roll,
    Post,
    Putback,
    /// Clock running out.
    Forced,
}

impl Source {
    pub const ALL: [Source; 9] = [
        Source::Transition,
        Source::Drive,
        Source::PullUp,
        Source::CatchAndShoot,
        Source::Cut,
        Source::Roll,
        Source::Post,
        Source::Putback,
        Source::Forced,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Source::Transition => "Transition",
            Source::Drive => "Drive",
            Source::PullUp => "Pull-up",
            Source::CatchAndShoot => "Catch and shoot",
            Source::Cut => "Cut",
            Source::Roll => "Roll man",
            Source::Post => "Post up",
            Source::Putback => "Putback",
            Source::Forced => "Late clock",
        }
    }
}

/// A failure the engine can pin on one player.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mistake {
    /// Lost concentration and left a shooter alone.
    BallWatching,
    /// In the right place, too late.
    LateCloseout,
    /// Beaten off the dribble with no help behind.
    BlownBy,
    /// Lost a cutter to the basket.
    LostCutter,
    /// Did not know where the play wanted him.
    OutOfPosition,
}

/// What the whistle was for.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FoulKind {
    /// Body or arm on a shooter.
    #[default]
    Shooting,
    /// A defender who was not set (or was under the basket) in the path of
    /// a driver.
    Blocking,
    /// The driver ran into a defender who was set outside the no-charge area.
    Charging,
    /// Hand or arm on the ball handler.
    ReachIn,
    /// Off the ball: chasing a cutter, fighting a screen.
    Holding,
    /// Shoving a man out of his position, post or rebound.
    Pushing,
    /// A screener who moved or leaned into the defender.
    IllegalScreen,
    /// Pushing off or hooking a defender in the post.
    PostPushOff,
}

impl FoulKind {
    pub const ALL: [FoulKind; 8] = [
        FoulKind::Shooting,
        FoulKind::Blocking,
        FoulKind::Charging,
        FoulKind::ReachIn,
        FoulKind::Holding,
        FoulKind::Pushing,
        FoulKind::IllegalScreen,
        FoulKind::PostPushOff,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FoulKind::Shooting => "Shooting",
            FoulKind::Blocking => "Blocking",
            FoulKind::Charging => "Charging",
            FoulKind::ReachIn => "Reach-in",
            FoulKind::Holding => "Holding",
            FoulKind::Pushing => "Pushing",
            FoulKind::IllegalScreen => "Illegal screen",
            FoulKind::PostPushOff => "Post push-off",
        }
    }

    /// Fouls committed by the team with the ball.
    pub fn is_offensive(self) -> bool {
        matches!(
            self,
            FoulKind::Charging | FoulKind::IllegalScreen | FoulKind::PostPushOff
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EventKind {
    PeriodStart,
    PeriodEnd,
    FastBreak {
        who: PlayerId,
    },
    PressBroken {
        who: PlayerId,
    },
    Screen {
        screener: PlayerId,
        handler: PlayerId,
    },
    /// The defense switched; `mismatch` is the height gap in cm the new
    /// on-ball defender gives away (negative) or gains.
    Switch {
        onto: PlayerId,
        handler: PlayerId,
        mismatch: i16,
    },
    Beaten {
        who: PlayerId,
        defender: PlayerId,
    },
    Help {
        helper: PlayerId,
        on: PlayerId,
    },
    PostUp {
        who: PlayerId,
        defender: PlayerId,
    },
    Shot {
        who: PlayerId,
        kind: ShotKind,
        source: Source,
        made: bool,
        /// 0 = wide open, 1 = smothered.
        contest: f32,
        defender: Option<PlayerId>,
        assist: Option<PlayerId>,
        blocked_by: Option<PlayerId>,
        fouled: bool,
    },
    Rebound {
        who: PlayerId,
        offensive: bool,
    },
    Turnover {
        who: PlayerId,
        kind: TurnoverKind,
        stolen_by: Option<PlayerId>,
    },
    /// `who` committed the foul on `on`. An offensive foul also shows up as
    /// a turnover.
    Foul {
        who: PlayerId,
        on: PlayerId,
        shots: u8,
        #[serde(default)]
        kind: FoulKind,
    },
    FreeThrows {
        who: PlayerId,
        made: u8,
        of: u8,
    },
    Mistake {
        who: PlayerId,
        kind: Mistake,
        beneficiary: PlayerId,
    },
    /// The bench of `side` changes how the team plays.
    Adjust {
        side: u8,
        offense: crate::tactics::OffenseSet,
        defense: crate::tactics::DefenseScheme,
        press: crate::tactics::Press,
    },
    /// Timeout called by `side` (0 = home).
    Timeout {
        side: u8,
    },
    Substitution {
        on: PlayerId,
        off: PlayerId,
    },
    /// The coach of `side` made himself heard.
    Shout {
        side: u8,
        shout: Shout,
    },
    /// The bench of `side` challenged a call, and the film decided.
    Review {
        side: u8,
        overturned: bool,
        what: crate::officials::What,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    /// Seconds of replay time, same clock as `Frame::time`.
    pub time: f32,
    pub period: u8,
    /// Seconds left in the period.
    pub clock: f32,
    /// Side with the ball, 0 = home.
    pub offense: u8,
    pub score: [u16; 2],
    pub kind: EventKind,
}

impl Event {
    /// True for the handful of events worth a highlight.
    pub fn is_key(&self) -> bool {
        matches!(
            self.kind,
            EventKind::Shot { made: true, .. }
                | EventKind::Shot {
                    blocked_by: Some(_),
                    ..
                }
                | EventKind::Turnover { .. }
                | EventKind::Mistake { .. }
                | EventKind::FastBreak { .. }
                | EventKind::Timeout { .. }
                | EventKind::Adjust { .. }
                | EventKind::PeriodEnd
        )
    }

    /// The player the event is mostly about.
    pub fn actor(&self) -> Option<PlayerId> {
        use EventKind::*;
        match &self.kind {
            FastBreak { who }
            | PressBroken { who }
            | Beaten { who, .. }
            | PostUp { who, .. }
            | Shot { who, .. }
            | Rebound { who, .. }
            | Turnover { who, .. }
            | Foul { who, .. }
            | FreeThrows { who, .. }
            | Mistake { who, .. } => Some(*who),
            Screen { handler, .. } => Some(*handler),
            Switch { onto, .. } => Some(*onto),
            Help { helper, .. } => Some(*helper),
            Substitution { on, .. } => Some(*on),
            PeriodStart | PeriodEnd | Timeout { .. } | Shout { .. } | Adjust { .. } | Review { .. } => None,
        }
    }

    /// Plain-language line for the match feed.
    pub fn text<'a>(&self, name: impl Fn(PlayerId) -> &'a str) -> String {
        use EventKind::*;
        match &self.kind {
            PeriodStart => format!("Period {} is under way.", self.period + 1),
            PeriodEnd => format!(
                "End of period {}: {}-{}.",
                self.period + 1,
                self.score[0],
                self.score[1]
            ),
            FastBreak { who } => format!("{} pushes it on the break.", name(*who)),
            PressBroken { who } => format!(
                "{} splits the press, numbers going the other way.",
                name(*who)
            ),
            Screen { screener, handler } => {
                format!(
                    "{} comes up to screen for {}.",
                    name(*screener),
                    name(*handler)
                )
            }
            Switch {
                onto,
                handler,
                mismatch,
            } => {
                if *mismatch <= -12 {
                    format!(
                        "They switch: {} is left on the bigger {}.",
                        name(*onto),
                        name(*handler)
                    )
                } else if *mismatch >= 12 {
                    format!(
                        "They switch: the big man {} has to guard {} out here.",
                        name(*onto),
                        name(*handler)
                    )
                } else {
                    format!("Switch, {} picks up {}.", name(*onto), name(*handler))
                }
            }
            Beaten { who, defender } => {
                format!("{} gets a step on {}.", name(*who), name(*defender))
            }
            Help { helper, on } => format!("{} steps across to stop {}.", name(*helper), name(*on)),
            PostUp { who, defender } => {
                format!(
                    "{} goes to work on {} in the post.",
                    name(*who),
                    name(*defender)
                )
            }
            Shot {
                who,
                kind,
                made,
                contest,
                defender,
                assist,
                blocked_by,
                fouled,
                ..
            } => {
                let what = match (kind, *contest < 0.25) {
                    (ShotKind::Rim, true) => "an open layup",
                    (ShotKind::Rim, false) => "a shot in traffic",
                    (ShotKind::Mid, true) => "an open jumper",
                    (ShotKind::Mid, false) => "a contested jumper",
                    (ShotKind::Three, true) => "an open three",
                    (ShotKind::Three, false) => "a contested three",
                };
                let mut s = if let Some(b) = blocked_by {
                    format!("{} is blocked by {}!", name(*who), name(*b))
                } else if *made {
                    match assist {
                        Some(a) => format!("{} scores, {} from {}.", name(*who), what, name(*a)),
                        None => format!("{} scores, {}.", name(*who), what),
                    }
                } else {
                    match defender {
                        Some(d) if *contest >= 0.5 => {
                            format!("{} misses {} over {}.", name(*who), what, name(*d))
                        }
                        _ => format!("{} misses {}.", name(*who), what),
                    }
                };
                if *fouled && *made {
                    s.push_str(" And the foul.");
                }
                s
            }
            Rebound { who, offensive } => {
                if *offensive {
                    format!("Offensive rebound, {}.", name(*who))
                } else {
                    format!("{} rebounds.", name(*who))
                }
            }
            Turnover {
                who,
                kind,
                stolen_by,
            } => match (kind, stolen_by) {
                (TurnoverKind::BadPass, Some(s)) => {
                    format!(
                        "{} reads the pass from {} and steals it.",
                        name(*s),
                        name(*who)
                    )
                }
                (TurnoverKind::BadPass, None) => format!("{} throws it away.", name(*who)),
                (TurnoverKind::Stripped, Some(s)) => {
                    format!("{} strips {}.", name(*s), name(*who))
                }
                (TurnoverKind::Stripped, None) => format!("{} loses the handle.", name(*who)),
                (TurnoverKind::OffensiveFoul, _) => {
                    format!("Offensive foul on {}, turnover.", name(*who))
                }
                (TurnoverKind::ShotClock, _) => "Shot clock violation.".to_string(),
                (TurnoverKind::ThreeSeconds, _) => {
                    format!("Three seconds in the lane on {}.", name(*who))
                }
                (TurnoverKind::Pressed, Some(s)) => {
                    format!(
                        "The press gets to {}, {} comes away with it.",
                        name(*who),
                        name(*s)
                    )
                }
                (TurnoverKind::Pressed, None) => {
                    format!("{} cannot get it over half court.", name(*who))
                }
            },
            Foul {
                who,
                on,
                shots,
                kind,
            } => {
                let what = match kind {
                    FoulKind::Shooting => "Shooting foul",
                    FoulKind::Blocking => "Blocking foul",
                    FoulKind::Charging => "Charge",
                    FoulKind::ReachIn => "Reach-in foul",
                    FoulKind::Holding => "Holding foul",
                    FoulKind::Pushing => "Pushing foul",
                    FoulKind::IllegalScreen => "Illegal screen",
                    FoulKind::PostPushOff => "Offensive foul, pushing off in the post",
                };
                if kind.is_offensive() {
                    format!(
                        "{} by {}, {} is the man it was on.",
                        what,
                        name(*who),
                        name(*on)
                    )
                } else if *shots > 0 {
                    format!(
                        "{} by {} on {}, {} shots.",
                        what,
                        name(*who),
                        name(*on),
                        shots
                    )
                } else {
                    format!("{} by {} on {}.", what, name(*who), name(*on))
                }
            }
            Review {
                side,
                overturned,
                what,
            } => {
                let bench = if *side == 0 { "home" } else { "away" };
                let call = match what {
                    crate::officials::What::Foul => "the foul",
                    crate::officials::What::OutOfBounds => "the ball",
                    crate::officials::What::Count => "the count",
                };
                if *overturned {
                    format!("The {bench} bench challenges {call}: overturned.")
                } else {
                    format!("The {bench} bench challenges {call}: the call stands.")
                }
            }
            FreeThrows { who, made, of } => {
                format!("{} makes {} of {} at the line.", name(*who), made, of)
            }
            Mistake {
                who,
                kind,
                beneficiary,
            } => match kind {
                self::Mistake::BallWatching => format!(
                    "{} was watching the ball and left {} alone.",
                    name(*who),
                    name(*beneficiary)
                ),
                self::Mistake::LateCloseout => {
                    format!(
                        "{} was late getting out to {}.",
                        name(*who),
                        name(*beneficiary)
                    )
                }
                self::Mistake::BlownBy => format!(
                    "{} was beaten too easily by {} and nobody was behind him.",
                    name(*who),
                    name(*beneficiary)
                ),
                self::Mistake::LostCutter => {
                    format!("{} lost {} on the cut.", name(*who), name(*beneficiary))
                }
                self::Mistake::OutOfPosition => {
                    format!("{} was in the wrong spot, the play broke down.", name(*who))
                }
            },
            Adjust {
                side,
                offense,
                defense,
                press,
            } => format!(
                "The {} bench changes it: {} on offense, {} on defense, {}.",
                if *side == 0 { "home" } else { "away" },
                offense.label().to_lowercase(),
                defense.label().to_lowercase(),
                press.label().to_lowercase()
            ),
            Timeout { side } => format!(
                "Timeout, {} bench.",
                if *side == 0 { "home" } else { "away" }
            ),
            Substitution { on, off } => format!("{} comes in for {}.", name(*on), name(*off)),
            Shout { shout, .. } => match shout {
                self::Shout::Encourage => "The coach claps his players on.",
                self::Shout::DemandMore => "The coach is on his feet, demanding more.",
                self::Shout::CalmDown => "The coach signals for calm.",
                self::Shout::Focus => "The coach taps his temple: concentrate.",
                self::Shout::Praise => "Applause from the bench.",
            }
            .to_string(),
        }
    }
}

/// What a player is doing in a frame. The viewer picks an animation from it.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// On his feet, not going anywhere.
    #[default]
    Standing,
    /// Running or walking without the ball and without a special task.
    Moving,
    /// Walking the ball up or working it in place.
    Dribbling,
    /// Attacking the basket off the dribble.
    Driving,
    /// Back to the basket, working his man.
    PostingUp,
    /// Planted as a screener, or on his way to the spot.
    Screening,
    /// Running a cut or a roll to the basket.
    Cutting,
    /// Sealing his man off the glass, back to him.
    BoxingOut,
    /// Gathering for a shot, in the air with it, or holding the follow-through;
    /// `Pose::style` and `Pose::stage` say which.
    Shooting,
    /// In the air for a rebound.
    Rebounding,
    /// In the air with a hand up on a shot.
    Contesting,
    /// In the air and getting a hand on the ball.
    Blocking,
    /// Just released a pass.
    Passing,
    /// Sprinting out at a shooter or ball handler.
    ClosingOut,
    /// Low, hands up, in front of his man.
    Defending,
    /// Set in the dribbler's path, taking the contact (a charge).
    Bracing,
    /// Knocked to the floor by contact.
    Down,
}

impl Action {
    /// Actions that leave the floor.
    pub fn is_jump(self) -> bool {
        matches!(
            self,
            Action::Shooting | Action::Rebounding | Action::Contesting | Action::Blocking
        )
    }
}

/// How a shot is taken.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Style {
    /// Set, up and release near the top of the jump, down where he took off.
    #[default]
    Jumper,
    /// Off one foot after a step or two, laid up off the glass or over the rim.
    Layup,
    /// Through the ring from his hand.
    Dunk,
    /// Back to the basket, turning into a one-handed shot.
    Hook,
    /// Leaning away from a hand in his face.
    Fadeaway,
}

/// How far a shot has got for the man taking it.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShotStage {
    /// Stopping, squaring up, dipping or taking the last steps.
    #[default]
    Gather,
    /// Off the floor, ball going up.
    Rise,
    /// The ball has just left his hand, arm extended.
    Release,
    /// Coming down.
    Fall,
    /// Down again, wrist held, watching the ball.
    Follow,
}

/// What one player is doing in a frame.
///
/// `lift_cm` is how far his feet are off the floor; it follows a simple arc
/// over the jump and never goes above `Pose::max_lift` for his Vertical.
/// `facing` is a unit vector in court coordinates scaled by 127, or zero when
/// he faces the way he is travelling. `contact` is `side * 5 + k` of the body
/// he is touching, or `Pose::NO_CONTACT`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pose {
    pub action: Action,
    /// How far through the action, 0 to 100 percent.
    pub progress: u8,
    pub lift_cm: u16,
    pub facing: [i8; 2],
    pub contact: u8,
    /// How a shot is being taken (only for `Action::Shooting`).
    #[serde(default)]
    pub style: Style,
    /// How far into the shot he is (only for `Action::Shooting`).
    #[serde(default)]
    pub stage: ShotStage,
}

impl Default for Pose {
    fn default() -> Pose {
        Pose {
            action: Action::Standing,
            progress: 0,
            lift_cm: 0,
            facing: [0, 0],
            contact: Pose::NO_CONTACT,
            style: Style::Jumper,
            stage: ShotStage::Gather,
        }
    }
}

impl Pose {
    pub const NO_CONTACT: u8 = 255;

    /// Feet off the floor, in metres.
    pub fn lift(&self) -> f32 {
        self.lift_cm as f32 / 100.0
    }

    /// Unit direction he faces, if it differs from his travel.
    pub fn face(&self) -> Option<Vec2> {
        (self.facing != [0, 0])
            .then(|| v(self.facing[0] as f32 / 127.0, self.facing[1] as f32 / 127.0))
    }

    /// `(side, k)` of the player he is in contact with.
    pub fn touching(&self) -> Option<(u8, u8)> {
        (self.contact != Pose::NO_CONTACT).then_some((self.contact / 5, self.contact % 5))
    }

    /// The highest a player with this Vertical (1..=20) can get his feet off
    /// the floor, in metres.
    pub fn max_lift(vertical: f32) -> f32 {
        0.30 + 0.025 * vertical.clamp(1.0, 20.0)
    }
}

/// One snapshot of the floor. Positions are in full-court coordinates:
/// x along the court (0..=28), y across it (0..=15). Home attacks x = 28.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub time: f32,
    pub period: u8,
    pub clock: f32,
    pub shot_clock: f32,
    pub score: [u16; 2],
    pub offense: u8,
    /// `[side][k]`, parallel to `who`.
    pub pos: [[Vec2; 5]; 2],
    /// Index into that side's box score (`GameResult::boxes[side]`).
    pub who: [[u8; 5]; 2],
    pub ball: Vec2,
    /// Height of the ball above the floor in metres.
    pub ball_height: f32,
    /// `(side, k)` of the player holding the ball.
    pub holder: Option<(u8, u8)>,
    /// What each player is doing, parallel to `pos`.
    #[serde(default)]
    pub pose: [[Pose; 5]; 2],
    /// A whistle has just blown (the frames for about half a second after a
    /// foul is called).
    #[serde(default)]
    pub whistle: bool,
    /// Play was cut here: a timeout, the break between periods. Nothing
    /// before this frame leads to it by movement, so it must not be glided
    /// into. Every other frame is one tick after the one before it.
    #[serde(default)]
    pub cut: bool,
    /// The ball is going down through the net in this frame.
    #[serde(default)]
    pub swish: bool,
    /// Where the three officials stand, full-court coordinates.
    #[serde(default)]
    pub officials: [Vec2; 3],
    /// The official (1-3, in the crew's order) with his arm up, or 0.
    #[serde(default)]
    pub whistler: u8,
}

/// One pass: when, which side, who threw it and who it was meant for (court
/// slots), where it left the thrower's hands, and whether it was a throw-in.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PassRec {
    pub time: f32,
    pub side: u8,
    pub from: u8,
    pub to: u8,
    pub at: Vec2,
    pub inbound: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Recording {
    /// Fatigue of the five on the floor, summed over ticks, and the ticks.
    /// A diagnostic.
    #[serde(default)]
    pub fatigue: [f32; 2],
    #[serde(default)]
    pub fatigue_ticks: u32,
    pub frames: Vec<Frame>,
    pub events: Vec<Event>,
    /// How tightly the crew called the game, 0.85 (let them play) to 1.15
    /// (tight). Zero in recordings made before it existed.
    #[serde(default)]
    pub whistle: f32,
    /// Times a player stood nearly still more than 3 m from where he was
    /// headed for over two seconds of play. A diagnostic, not game state.
    #[serde(default)]
    pub stuck: u32,
    /// Drives stopped by a defender who was set in the lane, and drives
    /// that met a helper still running at the driver. Diagnostics.
    #[serde(default)]
    pub help_stops: u32,
    #[serde(default)]
    pub late_help: u32,
    /// Fatigue each player took from contact, `[side][index into the box
    /// score]`, and the on-court ticks it was taken over. Diagnostics.
    /// Shots that came off a drive, by how the drive met the defense (clean,
    /// met a late helper, stopped by a set body): count, at the rim,
    /// summed contest, points. Diagnostics.
    #[serde(default)]
    pub drive: [[f32; 4]; 3],
    /// Charges called by how they came about: a defender who went to take
    /// it, one who happened to be set, one who sold contact the referee
    /// bought; attempts by a defender who went to take it; contact sold and
    /// not bought. Diagnostics.
    #[serde(default)]
    pub charges: [u32; 5],
    /// Ball screens against a man defense: when, and whether the handler's
    /// man went under (otherwise over). Diagnostics.
    #[serde(default)]
    pub screens: Vec<(f32, bool)>,
    /// Post moves: how many, metres of ground gained (summed), moves won.
    /// Diagnostics.
    #[serde(default)]
    pub post: [f32; 3],
    #[serde(default)]
    pub leaned: [Vec<f32>; 2],
    /// The ball's real path after a shot, a block or a tip: stretches
    /// between contacts, each exactly a parabola in height over a straight
    /// line on the floor. Sorted by start time. The viewer draws these
    /// instead of gliding between frames.
    #[serde(default)]
    pub ball_arcs: Vec<BallArc>,
    #[serde(default)]
    pub leaned_ticks: [Vec<u32>; 2],
    /// Every pass thrown, in court coordinates. A diagnostic and a check:
    /// nobody passes to himself, and a throw-in starts out of bounds.
    #[serde(default)]
    pub passes: Vec<PassRec>,
    /// Every whistle and every foul that should have been one, with what
    /// really happened; see `officials.rs`. Review results are filled in.
    #[serde(default)]
    pub calls: Vec<crate::officials::CallRecord>,
}

/// One stretch of the ball's flight, in full-court coordinates.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct BallArc {
    pub t0: f32,
    pub dur: f32,
    pub p0: Vec2,
    /// Metres per second over the floor.
    pub vel: Vec2,
    pub z0: f32,
    pub vz: f32,
    /// 9.81 in the air, 0 for a ball rolling.
    pub g: f32,
}

impl BallArc {
    pub fn at(&self, t: f32) -> (Vec2, f32) {
        let t = t.clamp(0.0, self.dur);
        (self.p0 + self.vel * t, self.z0 + self.vz * t - 0.5 * self.g * t * t)
    }
}

impl Recording {
    /// The ball's recorded path at `time`, if it was in free flight then.
    pub fn ball_at(&self, time: f32) -> Option<(Vec2, f32)> {
        let i = self.ball_arcs.partition_point(|a| a.t0 <= time);
        let a = self.ball_arcs[..i].iter().rev().take(4).find(|a| time <= a.t0 + a.dur)?;
        Some(a.at(time - a.t0))
    }

    pub fn duration(&self) -> f32 {
        self.frames.last().map_or(0.0, |f| f.time)
    }

    /// Index of the last frame at or before `time`.
    pub fn frame_at(&self, time: f32) -> usize {
        self.frames
            .partition_point(|f| f.time <= time)
            .saturating_sub(1)
    }
}

/// Attack-frame point of the side `offense` to full-court coordinates.
pub fn to_court(p: Vec2, offense: usize) -> Vec2 {
    if offense == 0 {
        v(LENGTH - p.y, 7.5 + p.x)
    } else {
        v(p.y, 7.5 - p.x)
    }
}
