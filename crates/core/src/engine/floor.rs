//! Possessions played out on the floor.
//!
//! Positions live in the attack frame of the team with the ball (see
//! `court.rs`) and are mirrored when possession changes, so players really do
//! have to run back. Indices `k` are court indices 0..5 into `Side::on`.

use super::ball::{self, Ball, Flown, Hand, v3};
use super::contact::{self, Bulk, Touch};
use super::defense::{self, View, is_zone};
use super::flight::{self, Path};
use super::offense;
use super::shot::{self, Errors};
use super::{BoxLine, DT, GameResult, Lineup, Pause, Rules, Shout, Side};
use crate::court::{
    KEY_DEPTH, KEY_HALF_WIDTH, HALF_WIDTH, BALL_RADIUS, BOARD_HALF, BOARD_LOW, BOARD_Y, GRAVITY, HOOP, LENGTH, NO_CHARGE_RADIUS, RIM_HEIGHT, ShotKind,
    Vec2, v,
};
use crate::player::{Attr, PlayerId};
use crate::replay::{
    Action, BallArc, Event, EventKind, FRAME_RATE, FoulKind, Frame, Mistake, Pose, Recording, ShotStage,
    Source, Style, TurnoverKind, to_court,
};
use crate::rng::Rng;
use crate::tactics::{Brief, DefenseScheme, OffenseSet, Press};

// ---- the shot's error model -------------------------------------------
/// Scale of the error of an average shooter's open shot in play, against
/// the free throw's `FT_SCALE`: a live shot is taken on the run of a play,
/// with a crowd and a clock.
const GAME: f32 = 1.14;
/// What a live two, a three and a finish at the rim add or take off the
/// error, on top of their own base: the part of in-game difficulty that
/// the base error shape does not carry.
const MID_SCALE: f32 = 0.96;
const THREE_SCALE: f32 = 0.96;
const RIM_SCALE: f32 = 1.22;
/// What each point of rating above 10 takes off the error of a jump shot,
/// of a finish at the rim, and of a free throw.
const SKILL_SLOPE: f32 = 0.035;
const FINISH_SLOPE: f32 = 0.07;
const FT_SCALE: f32 = 0.58;
const FT_SLOPE: f32 = 0.045;
/// What a hand in the face (contest 1) adds to the error: on a three, a
/// two, at the rim and on a dunk; and what each point of the defender's
/// skill above 10 adds to that.
const CONTEST_THREE: f32 = 0.45;
const CONTEST_MID: f32 = 0.35;
const CONTEST_RIM: f32 = 1.1;
const DUNK_CONTEST: f32 = 1.5;
const GUARD_SLOPE: f32 = 0.04;
/// A dunk from the very top of his reach is shaky: what each metre short
/// of a clear 0.15 m adds.
const DUNK_EDGE: f32 = 8.0;
/// Per m/s of the shooter's own speed as the ball leaves a jump shot, and
/// for a shot off the dribble.
const MOVING: f32 = 0.10;
const OFF_DRIBBLE: f32 = 0.07;
/// Per point of composure above 10, for an unfamiliar set, for the home crowd.
const COMPOSURE: f32 = 0.004;
const RHYTHM: f32 = 0.06;
const HOME: f32 = 0.016;
/// A hand on the shooter that was called, and one that was not.
const FOULED_ERR: f32 = 2.5;
const FOULED_JUMPER: f32 = 1.5;
/// A tip-in at the ring: how much harder than a layup (a ball in the air, a
/// hand's width of control), and how close a defender has to be to be in the way.
/// The lane rule, and how soon the attackers clear it of their own accord.
const LANE_SECONDS: f32 = 3.0;
const LANE_WARN: f32 = 2.2;
/// A shot this soon after the ball changed hands live came off the break.
const BREAK_SECONDS: f32 = 8.0;
/// How far from the baseline the man with the ball goes at the first defender,
/// and how far he runs on before he gives up a break he is outnumbered in.
const ATTACK_Y: f32 = 9.0;
const PULL_Y: f32 = 11.5;
/// A teammate this much further up the floor can be thrown the ball.
const LEAD_PASS: f32 = 3.0;
/// The most risk of a deflection a throw ahead is worth.
const LEAD_RISK: f32 = 0.08;
/// Chance per look that a defender who has been beaten on the break and has
/// a clear path to the rim in front of the ball pulls him down.
const STOP_FOUL: f32 = 0.05;
/// Seconds the team that lost the ball takes to turn round: a base, so much
/// more for each point of IQ under 20, and more for the man who lost it.
const BEAT_BASE: f32 = 0.0;
const BEAT_IQ: f32 = 0.006;
const BEAT_LOST: f32 = 0.15;
/// Chance, at neutral tempo, that the receiver of a throw-in pushes the ball.
const PUSH_OFF_INBOUND: f32 = 0.35;
/// Fatigue above which a man does not run the floor.
const RUN_LIMIT: f32 = 0.6;
/// Added to the crowding of a pass into a man at the rim, on top of 2.6.
const CROWD_RIM: f32 = 3.0;
const TIP_SCALE: f32 = 1.0;
const TIP_CROWD: f32 = 1.5;
const TIP_IN_RANGE: f32 = 0.8;
const CONTACT_ERR: f32 = 1.35;
/// A defender who has not jumped still has a hand up: how much of a
/// contest he is, against one in the air.
const HAND_STANDING: f32 = 0.75;
/// How much of a man's standing reach is his arm (shoulder to fingertip).
const ARM_SHARE: f32 = 0.37;
/// How much shorter than his arm the reach is at which his hand meets a
/// ball in flight: a block needs the ball well inside it, not at the fingertip.
const BLOCK_SLACK: f32 = 0.292;
// ---- the loose ball ------------------------------------------------------
/// How high a rebounder jumps, as a share of the most he can.
const JUMP_FRAC: f32 = 0.9;
/// His read of where the ball will be: metres of error for a player of
/// rating 0, and what each point of rating takes off.
const READ_SPOT: f32 = 0.6;
const READ_SPOT_SKILL: f32 = 0.04;
/// Seconds before he reacts to a ball off the iron, by rating.
const READ_DELAY: f32 = 0.26;
const READ_DELAY_SKILL: f32 = 0.012;
/// Seconds of error in the timing of his jump.
const READ_TIMING: f32 = 0.22;
const READ_TIMING_SKILL: f32 = 0.008;
/// Chance a hand on the ball holds it: for an average man, per point of skill
/// above 10, less for a contested ball and for a fast one.
const HOLD_BASE: f32 = 0.9;
const HOLD_SKILL: f32 = 0.012;
const HOLD_CONTEST: f32 = 0.75;
const HOLD_SPEED: f32 = 0.03;
/// Metres within which an opponent makes a ball a contested one.
const CONTEST_ZONE: f32 = 3.0;

const ACCEL: f32 = 7.0;
/// A shooter stopping for a jump shot brakes this many times as hard as he
/// speeds up: a jump stop is a plant of both feet.
const BRAKE: f32 = 2.4;
/// Seconds a shooter bends his knees before he goes up.
const DIP: f32 = 0.14;
/// Seconds that pass between a shot's decision and a layup's take-off.
const STEP_IN: f32 = 0.05;
/// How a player's Vertical turns into how high he goes for each kind of
/// shot, as a share of the most he can jump.
const LIFT_JUMPER: f32 = 0.5;
const LIFT_LAYUP: f32 = 0.85;
/// Metres a ball handler's hand is above his head at the top of a jump
/// shot, measured from the reach of his raised arm.
const HAND_BELOW_REACH: f32 = 0.10;
/// The ring is dunked from above: his reach and leap must clear it by this.
const DUNK_CLEAR: f32 = 0.18;
/// The most traffic in front of him a man will go up through for a dunk.
const DUNK_TRAFFIC: f32 = 1.0;
/// Furthest from the basket a dunk can start.
const DUNK_RANGE: f32 = 1.1;
/// Seconds a free throw shooter takes before he goes up.
const FT_ROUTINE: f32 = 0.7;
/// Longest the players take to line up for a free throw, seconds.
const FT_LINEUP: f32 = 3.0;
/// Metres within which a body counts as at his spot.
const SPOT_NEAR: f32 = 0.5;
/// Seconds from a dead ball to the ball being in play, counting what the ball
/// spends dropping through the net (it was 3.6 when that took no time).
const INBOUND_WAIT: f32 = 2.95;
/// How much lower the bar is for a man who has just taken an offensive
/// board: the ball is already at the basket and he goes straight back up.
const PUTBACK_EDGE: f32 = 0.25;
/// Seconds the floor stands still after a foul away from a shot.
const DEAD_FOUL: f32 = 1.2;
/// Where the free throw shooter stands, in the attack frame.
const FT_LINE: Vec2 = v(0.0, 5.8);
/// How far in front of his body a finisher releases the ball, in metres.
const LUNGE: f32 = 0.30;
/// Seconds after a driver beats his man during which that defender is not
/// counted as a body in the way: the driver is still getting round him.
const BEATEN_GRACE: f32 = 0.62;
/// Fraction of top speed a post player drives into his man; what he gains is
/// decided by the two bodies.
/// Speed (m/s) under which a defender counts as set in the driver's path.
const HELP_SET_SPEED: f32 = 2.0;
/// Fatigue per metre of being pushed, per m/s of closing speed in a collision
/// (as a share of a metre), and the anchor of a body that takes the cost as
/// given.
const CONTACT_COST: f32 = 0.0014;
const CONTACT_CLOSING: f32 = 0.02;
const CONTACT_REF: f32 = 90.0;
/// Bodies further apart than this (m) are never in each other's way.
/// Seconds of grace gained per point of Speed the beaten defender lacks.
const GRACE_SPEED: f32 = 0.016;
/// Seconds a whistle is marked in the replay frames.
const WHISTLE_SHOWN: f32 = 0.6;
/// The loosest and the tightest whistle a crew can bring.
const WHISTLE_LOOSE: f32 = 0.85;
const WHISTLE_TIGHT: f32 = 1.15;
/// Share of illegal contact a crew of average tightness calls.
const CALL_BASE: f32 = 0.85;
/// Closing speed (m/s) at which two bodies meeting is a collision worth a look.
const IMPACT: f32 = 0.9;
/// Seconds a defender has to be still before a driver arrives for his
/// position to count as established.
const SET_FOR: f32 = 0.25;
/// What a shooter's foul chance is multiplied by when the defender is
/// standing and when he is moving into him (per m/s beyond 0.8).
const VERT_SET: f32 = 0.95;
const VERT_MOVING: f32 = 0.35;
/// Energy a defender spends taking a charge, per m/s of impact.
const CHARGE_COST: f32 = 0.01;
/// A defender who is not set and sells the contact anyway: the chance for a
/// man who is all urge, and how often the referee buys it (times the whistle).
const FLOP: f32 = 0.09;
const FOOLED: f32 = 0.4;
/// The most likely a defender is to go for a charge when the urge is full
/// (an urge of 1), and how much each metre of distance from the driver's path
/// counts against him when choosing who goes.
const HUNT_CAP: f32 = 1.0;
const HUNT_NEAR: f32 = 0.04;
/// Metres further out, ahead of and beside the driver's line, a man with a
/// full urge goes to meet him.
const HUNT_FAR: f32 = 1.0;
/// What a full urge adds to the chance the driver does not see him coming.
const HUNT_BLIND: f32 = 0.5;
/// Chance a post move ends in a push-off: a floor, and what each metre of
/// ground gained beyond par adds.
const POST_SHOVE: f32 = 0.006;
const POST_SHOVE_RATE: f32 = 0.05;
/// Chance per screen of an illegal one, and of a defender grabbing while
/// fighting through.
const SCREEN_ILLEGAL: f32 = 0.005;
const SCREEN_HOLD: f32 = 0.03;
/// Chance of a push for each body (as heavy as himself) a rebounder has to go through.
const REB_PUSH: f32 = 0.015;
/// How likely a crew is to call a charge or a blocking foul on a collision:
/// a floor, and what each m/s of impact adds. (Most contact between a driver
/// and a defender is left alone.)
const CHARGE_SEV: f32 = 0.001;
const CHARGE_IMPACT: f32 = 0.001;
const BLOCK_SEV: f32 = 0.03;
const BLOCK_IMPACT: f32 = 0.04;
/// The same for a defender who stepped in to take the charge.
const PLANTED_SEV: f32 = 0.9;
const PLANTED_IMPACT: f32 = 0.4;
/// A specialist knows how to take the hit and gets the call more often:
/// what a full urge adds to the severity of the contact.
const PLANTED_SKILL: f32 = 0.75;
/// How close (beyond touching) a blind driver comes to the man planted in his
/// way before the two meet.
const HUNT_REACH: f32 = 1.4;
const NEAR: f32 = 2.2;
/// How much more a three is worth to a shooter than its points, before the
/// coach's three-point bias.
const THREE_TASTE: f32 = 1.014;
/// Multiplier on the contest of a pull-up jumper after a contained drive.
const PULL_UP_EDGE: f32 = 0.85;
/// A beaten driver stopped inside this many metres of the basket goes up
/// rather than looking for a pass or a reset.
const COMMIT_DIST: f32 = 4.0;
/// Chance a driver sees a help defender running at him (and stops to kick
/// or pull up) rather than running into him: a floor, and what each point of
/// basketball IQ adds. Not on the break, where the defense is late.
const SIGHT_BASE: f32 = 0.3;
const SIGHT_IQ: f32 = 0.03;
const POST_PUSH: f32 = 0.09;
/// Metres a post player gains on an even match, and how much each metre more or
/// less moves the chance of winning the move.
const POST_PAR: f32 = 0.7;
const POST_GROUND: f32 = 0.35;
/// Chance an even post move is won, before skill, size, ground and mass.
const POST_BASE: f32 = 0.44;
/// What each tenth of extra weight (as a share of the other man's) adds to
/// the chance of winning a post move, and what each metre of ground gained
/// beyond par takes off the contest on the shot.
const POST_MASS: f32 = 0.3;
const POST_ROOM: f32 = 0.5;
/// The anchor of an average body, which `heft` leaves as it is.
const POST_REF: f32 = 90.0;
/// How close to the defender's body, beyond touching, a screener counts as set.
const SCREEN_SET: f32 = 0.35;
/// How close (m) the handler gets to the point past the screener's shoulder
/// before he heads for the basket, and the share of top speed he uses.
const SHOULDER_REACH: f32 = 0.45;
const SHOULDER_PACE: f32 = 0.85;
/// Seconds the screener's man shows himself to the handler.
const HEDGE_TIME: f32 = 0.7;
/// Seconds a defender who goes under takes to settle on his cushion, and
/// how far (m) in front of the handler he sits, before the screener's
/// weight is added.
const UNDER_TIME: f32 = 1.0;
const UNDER_GAP: f32 = 1.9;
const UNDER_BULK: f32 = 0.9;
/// Seconds a handler whose man went under takes before he pulls up.
const UNDER_PULL: f32 = 0.5;
/// How readily a defender goes under a screen: the share who do at even
/// odds, what a pack line adds, and what each point of the handler's
/// three-point rating above 10, of the aggression slider (-0.9..1) and of
/// the defender's own speed and strength above 10 takes off.
const UNDER_BASE: f32 = 0.45;
const UNDER_PACK: f32 = 0.35;
const UNDER_THREAT: f32 = 0.05;
const UNDER_AGGRESSION: f32 = 0.35;
const UNDER_SKILL: f32 = 0.01;

/// A finisher releases the ball a forearm in front of his chest, so a body
/// that cannot get closer to the rim than another body lets him still
/// finishes at the rim.
fn shot_kind(p: Vec2) -> ShotKind {
    if p.dist(HOOP) < 3.0 {
        ShotKind::at(p.toward(HOOP, LUNGE))
    } else {
        ShotKind::at(p)
    }
}

/// What a body's anchor is worth in a shoving match in the post: mass counts
/// twice, once for the body and once for the leverage it gives, so the
/// heavier man wins the ground by more than the ratio of weights.
fn heft(anchor: f32) -> f32 {
    anchor * anchor / POST_REF
}

/// Things worked out from attributes, which move slowly (fatigue) or only
/// when the men on the floor change, kept for a few ticks at a time.
#[derive(Copy, Clone)]
struct Cache {
    /// Top speed of each of the ten.
    top: [f32; 10],
    /// A defender's reaction time before the zone's reading is added.
    lag: [f32; 5],
    /// What the defense thinks of each attacker as a shooter, and how much
    /// room each defender gives his man.
    threat: [f32; 5],
    cushion: [f32; 5],
    /// The defense's last aims and the tick they were worked out on.
    aims: defense::Aims,
    aims_tick: u32,
}

/// "Going straight": a detour point that is not a point, so that a result
/// travels in registers and not through memory.
const NO_WAY: Vec2 = Vec2 {
    x: f32::NAN,
    y: f32::NAN,
};

#[derive(Copy, Clone, Default)]
struct Mover {
    pos: Vec2,
    vel: Vec2,
    target: Vec2,
    /// Fraction of top speed he is willing to use.
    pace: f32,
    /// Planting for a shot: he can slow down much faster than he speeds up.
    hard: bool,
    /// Taking the ball out: he may stand behind the line until he is back in.
    out: bool,
}

impl Mover {
    /// One tick toward `aim` (the target, or a point that goes round a body
    /// in the way); `r` is his body radius, which keeps him on the court.
    fn step(&mut self, top: f32, aim: Vec2, r: f32) {
        let to = aim - self.pos;
        let dist = to.len();
        let speed = (top * self.pace).min(dist * 3.5);
        let want = if dist > 0.03 {
            to * (speed / dist)
        } else {
            Vec2::ZERO
        };
        let dv = want - self.vel;
        let limit = ACCEL * DT * if self.hard { BRAKE } else { 1.0 };
        // Compared as squares: the root is only needed when he is held to
        // the most he can change his velocity in a tick.
        let len2 = dv.dot(dv);
        self.vel = if len2 > limit * limit {
            self.vel + dv * (limit / len2.sqrt())
        } else {
            want
        };
        let next = self.pos + self.vel * DT;
        if self.out {
            self.pos = v(
                next.x.clamp(-HALF_WIDTH - 2.0, HALF_WIDTH + 2.0),
                next.y.clamp(-2.0, LENGTH + 2.0),
            );
            if contact::keep_in(self.pos, r) == self.pos {
                self.out = false;
            }
        } else {
            self.pos = contact::keep_in(next, r);
        }
    }
}

/// A jump: when it starts, how long it lasts and how high it goes. The
/// game reads it (for a hand's height) as well as the replay.
#[derive(Copy, Clone)]
struct Jump {
    action: Action,
    start: f32,
    dur: f32,
    peak: f32,
}

impl Jump {
    const NONE: Jump = Jump {
        action: Action::Standing,
        start: 0.0,
        dur: 0.0,
        peak: 0.0,
    };
}

#[derive(Copy, Clone)]
enum Phase {
    /// Bringing the ball up.
    Advance,
    /// Half-court offense, ball handler reading the floor.
    Set,
    /// Waiting for court index `s` to arrive and set a ball screen.
    Screen {
        s: usize,
        until: f32,
    },
    Drive {
        dk: usize,
        beaten: bool,
        until: f32,
    },
    Post {
        dk: usize,
        until: f32,
    },
    /// Going up for a shot, released at `at`.
    Gather {
        at: f32,
        off_dribble: bool,
    },
    /// Ball in the air.
    Air,
    /// Free throws: the players line up, then the shooter takes them one
    /// by one. `at` is when the next one is released (a very large number
    /// while he is not yet in his routine).
    Line {
        shooter: usize,
        left: u16,
        of: u16,
        made: u16,
        at: f32,
        /// Play time at which the lineup is called complete.
        ready: f32,
    },
}

#[derive(Copy, Clone, PartialEq)]
enum Task {
    Spot,
    Cut(Vec2),
    SetScreen,
    Roll,
    Pop(Vec2),
    Crash,
    GetBack,
    /// Running to a place on the break, at the pace he is willing to run.
    Fill(Vec2),
    /// Stays where he is, watching the ball, until this play time, then
    /// gets back.
    Watch(f32),
}

#[derive(Copy, Clone)]
enum FlightKind {
    Pass {
        from: usize,
        to: usize,
    },
    /// A pass a defender (court index `to`) gets a hand on.
    Steal {
        to: usize,
    },
    Shot {
        shooter: usize,
        made: bool,
        kind: ShotKind,
        assist: Option<usize>,
        and_one: bool,
        /// Free throws to shoot once this missed shot has settled (he was
        /// fouled in the act and missed).
        foul_shots: u16,
        /// This is a free throw.
        line: bool,
    },
}

#[derive(Copy, Clone)]
struct Flight {
    from: Vec2,
    to: Vec2,
    t: f32,
    /// Seconds until the pass arrives, or the shot is at the ring or the
    /// iron or in a defender's hand.
    dur: f32,
    kind: FlightKind,
    /// A shot's path, from the hand on.
    path: Option<Path>,
}

#[derive(Copy, Clone, PartialEq)]
enum AfterKind {
    /// Through the net and down; `then` free throws follow (an and-one).
    Drop { shooter: usize, then: u16 },
    /// Off the iron on a dead ball; `shots` free throws follow.
    Fall { shooter: usize, shots: u16 },
    /// Off the iron or the glass and live: the ball is in the air or on the
    /// floor and whoever gets a hand to it first has it.
    Live { free_throw: bool },
    /// Out of bounds with the attackers: the ball is brought back to him.
    Inbound { who: usize },
    /// Back to the free throw shooter.
    Return { shooter: usize },
}

/// What the ball does after a shot has been decided: still in the air, and
/// the play is not over until it has settled.
#[derive(Copy, Clone)]
struct After {
    kind: AfterKind,
    path: Path,
    t: f32,
    /// Seconds into the path when it is over.
    end: f32,
}

/// How a shot will go, settled as he leaves the floor.
#[derive(Copy, Clone)]
struct Verdict {
    kind: ShotKind,
    /// How the contest stood as he left the floor, and the edge a won post
    /// move gave him.
    c: f32,
    edge: f32,
    near: Option<usize>,
    /// A hand got to him, and the crew called it.
    contact: bool,
    fouled: bool,
    assist: Option<usize>,
}

/// How a player reads a ball that has come off the iron: where he thinks it
/// will be when he meets it, when he starts moving and jumping, and when he
/// expects it (path seconds).
#[derive(Copy, Clone)]
struct Chase {
    active: bool,
    spot: Vec2,
    /// Where it really comes down: he sees it better the nearer it gets.
    truth: Vec2,
    go: f32,
    arrive: f32,
    jump_at: f32,
    jumped: bool,
}

impl Chase {
    const NONE: Chase = Chase {
        active: false,
        spot: Vec2::ZERO,
        truth: Vec2::ZERO,
        go: 0.0,
        arrive: 0.0,
        jump_at: 0.0,
        jumped: false,
    };
}

/// A shot being taken, from the moment he decided to the end of his
/// follow-through. Times are play time.
#[derive(Copy, Clone)]
struct ShotPlan {
    shooter: usize,
    style: Style,
    start: f32,
    jump_at: f32,
    release: f32,
    land: f32,
    follow_end: f32,
    /// How high his feet get.
    peak: f32,
    /// Where the ball leaves his hand.
    hand_z: f32,
}

/// How the next possession begins.
#[derive(Copy, Clone)]
enum Start {
    /// Dead ball: made basket, whistle, start of a period.
    Inbound,
    /// Live change: court index of the new ball handler.
    Live(usize),
}

/// Where the game is between two calls to `Game::step`.
#[derive(Copy, Clone)]
enum Stage {
    /// A period is about to start.
    Period,
    /// The side with this index is about to get the ball.
    Possession(usize, Start),
    Play,
    Over,
}

enum End {
    /// The other team takes the ball out.
    Dead,
    /// Rebound or steal by the defender at this court index.
    Live(usize),
    Period,
}

struct Play {
    o: usize,
    t: f32,
    shot_clock: f32,
    live_start: bool,
    /// Seconds until the ball is put in play after a whistle or a basket.
    wait: f32,
    holder: Option<usize>,
    ball: Vec2,
    ball_h: f32,
    flight: Option<Flight>,
    after: Option<After>,
    shot: Option<ShotPlan>,
    verdict: Option<Verdict>,
    /// How each of the ten (attackers first) reads the loose ball.
    chase: [Chase; 10],
    /// The last man to touch the ball was a defender, and how many times it
    /// has been tipped since the shot.
    def_touch: bool,
    tips: u8,
    /// How long each attacker has stood in the lane: three seconds is a turnover.
    lane: [f32; 5],
    /// Attacker who goes to pick up a loose ball, once it has stopped, and
    /// the play time he was sent at.
    fetcher: Option<usize>,
    fetch_t: f32,
    /// A push called on the rebound: who pushed (attackers first) and whom.
    push: Option<(usize, usize)>,
    /// The ball was loose and one man has to hand it to the handler.
    inbound: bool,
    /// Seconds the floor stands still after a whistle (replay time only).
    dead: f32,
    /// Seconds of this play spent with the clocks stopped.
    frozen: f32,
    phase: Phase,
    think: f32,
    spots: [Vec2; 5],
    /// Runs drawn on the board and not made yet this possession.
    runs: [Option<Vec2>; 5],
    task: [Task; 5],
    /// `guard[dk]` = attacker that defender is matched with.
    guard: [usize; 5],
    /// Seconds a defender is frozen: screened, wrong-footed, ball-watching.
    stun: [f32; 5],
    /// When each defender last switched off.
    lapse_at: [f32; 5],
    dribbled: bool,
    /// Moves the ball handler has tried since he caught it.
    moves: u8,
    /// Drives each attacker has already tried this possession.
    tries: [u8; 5],
    /// Last passer and when the ball was caught.
    assist: Option<(usize, f32)>,
    /// Who passed to the current holder (do not pass straight back).
    from: Option<usize>,
    handler: usize,
    star: usize,
    post: usize,
    big: usize,
    drop: Option<usize>,
    on_ball: Option<usize>,
    help_logged: bool,
    boxout: bool,
    crossed: bool,
    /// Multiplier on the contest of the next shot (a won post move).
    edge: f32,
    /// Screener who rolled to the basket on the last ball screen.
    roller: Option<usize>,
    /// How the ball handler came by his next shot.
    source: Source,
    /// Defender beaten on the last drive, for blame.
    blown: Option<(usize, f32)>,
    /// Where the post player started backing his man down.
    post_from: Vec2,
    /// A help defender who ran into the driver's path too late to be set.
    late_help: Option<usize>,
    /// How the current drive met the defense: 0 clean, 1 late help, 2 stopped.
    drive_class: usize,
    /// The driver has seen the help defender now running at him.
    late_seen: bool,
    /// The defender this driver has already run into.
    hit: Option<usize>,
    /// Where the ball handler runs first off a screen: past the far shoulder
    /// of the screener, so the screener's body is between him and his man.
    shoulder: Option<Vec2>,
    /// A defender sent round a screen by a set route: who, where to, until.
    route: Option<(usize, Vec2, f32)>,
    /// The screener's man steps up to show himself to the handler until then.
    hedge_until: f32,
    /// The next drive is contained from the start (he went under).
    contained: bool,
    /// A defender who has stepped into the driver's path to take a charge,
    /// where he stands, and whether the driver has not seen him.
    hunt: Option<(usize, Vec2, bool)>,
    /// The ball changed hands live and the team is running: the break is on
    /// until the man with the ball attacks or pulls out into the set.
    brk: bool,
    /// The possession began live, whatever became of the break.
    live_origin: bool,
    /// The fast break has been called in the event log.
    brk_logged: bool,
    /// The pace each attacker is willing to run at on the break.
    go: [f32; 5],
    /// A throw-in under way.
    inb: Option<Inb>,
}

/// A throw-in: who takes the ball out, who is to get it, where from, where
/// the receiver comes to, and when the thrower got to his spot.
#[derive(Copy, Clone)]
struct Inb {
    who: usize,
    to: usize,
    spot: Vec2,
    rx: Vec2,
    since: Option<f32>,
}

pub(super) struct Game {
    pub(super) sides: [Side; 2],
    pub(super) rules: Rules,
    pub(super) rng: Rng,
    m: [[Mover; 5]; 2],
    /// Side whose attack frame `m` is expressed in.
    frame: usize,
    play: Play,
    period: u8,
    clock: f32,
    elapsed: f32,
    regulation: f32,
    /// Replay time: game time plus pauses for whistles.
    pub(super) wall: f32,
    ticks: u32,
    pub(super) rec: Option<Recording>,
    stage: Stage,
    /// Side that has the ball first in the opening period.
    first: usize,
    periods: Vec<[u16; 2]>,
    possessions: [u16; 2],
    /// Score when the current period began.
    before: [u16; 2],
    /// Score at the end of the last possession.
    seen: [u16; 2],
    /// A timeout was just taken at this dead ball.
    timeout_taken: bool,
    /// Seconds each player has been nearly still far from his target.
    still: [[f32; 5]; 2],
    /// What each body felt at the last tick, `[side * 5 + k]`.
    touch: [Touch; 10],
    /// 0.85 for a crew that lets them play up to 1.15 for a tight whistle:
    /// the chance that illegal contact is called scales with it.
    pub(super) whistle: f32,
    /// The replay shows the whistle until this time.
    whistle_until: f32,
    /// Seconds each player has been nearly still.
    set_for: [[f32; 5]; 2],
    /// Until when each player is on the floor after contact (replay time).
    down: [[f32; 5]; 2],
    /// Where each player is going round a body in his way, if he is.
    way: [Vec2; 10],
    /// Bodies as of the last refresh, and whether somebody has changed since.
    bulk_base: [Bulk; 10],
    bulk_dirty: bool,
    cache: Cache,
    /// Seconds of animation, which run through every tick including the
    /// ones where the clock is stopped.
    anim: f32,
    /// The next frame follows a cut.
    cut: bool,
    /// The ball has gone through the net this tick.
    swish: bool,
    /// Seconds of the next dead ball the ball has already spent in the net.
    credit: f32,
    /// Where each of the ten stands for a free throw, attackers first.
    ft_spot: [Vec2; 10],
    /// The ball is loose: a path in the current attack frame and how far
    /// along it is.
    loose: Option<(Path, f32)>,
    /// Jumps being shown, `[side][k]`. Only kept while recording.
    jump: [[Jump; 5]; 2],
    /// The rebounders of the shot in the air have been sent up.
    /// Where the ten stood at the start of the tick.
    prev_pos: [[Vec2; 5]; 2],
    /// Whoever lost the ball to a steal, in the old attack frame, until the
    /// next possession has been set up.
    lost_by: Option<usize>,
    /// The dead-ball clock at the last tick.
    dead_was: f32,
}

impl Game {
    pub(super) fn new(
        home: &Lineup,
        away: &Lineup,
        rules: &Rules,
        mut rng: Rng,
        record: bool,
    ) -> Self {
        let regulation = (rules.periods as u32 * rules.period_secs) as f32;
        let sides = [
            Side::new(home, regulation, &mut rng),
            Side::new(away, regulation, &mut rng),
        ];
        let first = rng.range(0, 2) as usize;
        // How tightly this crew calls it tonight, drawn once.
        let whistle = WHISTLE_LOOSE + rng.f32() * (WHISTLE_TIGHT - WHISTLE_LOOSE);
        let mut m = [[Mover::default(); 5]; 2];
        for (t, side) in m.iter_mut().enumerate() {
            for (k, mover) in side.iter_mut().enumerate() {
                let y = if t == 0 { 15.5 } else { 12.5 };
                mover.pos = v(k as f32 * 2.4 - 4.8, y);
                mover.target = mover.pos;
            }
        }
        Game {
            sides,
            rules: rules.clone(),
            rng,
            stage: Stage::Period,
            first,
            periods: Vec::new(),
            possessions: [0; 2],
            before: [0; 2],
            seen: [0; 2],
            timeout_taken: false,
            still: [[0.0; 5]; 2],
            touch: [Touch::CLEAR; 10],
            bulk_base: [Bulk {
                r: 0.27,
                anchor: 90.0,
            }; 10],
            bulk_dirty: true,
            cache: Cache {
                top: [0.0; 10],
                lag: [0.0; 5],
                threat: [0.0; 5],
                cushion: [0.0; 5],
                aims: defense::Aims {
                    at: [Vec2::ZERO; 5],
                    on_ball: None,
                    helper: None,
                },
                aims_tick: 0,
            },
            way: [NO_WAY; 10],
            whistle,
            whistle_until: -1.0,
            set_for: [[0.0; 5]; 2],
            down: [[-1.0; 5]; 2],
            jump: [[Jump::NONE; 5]; 2],
            anim: 0.0,
            cut: false,
            swish: false,
            credit: 0.0,
            loose: None,
            ft_spot: [Vec2::ZERO; 10],
            prev_pos: [[Vec2::ZERO; 5]; 2],
            lost_by: None,
            dead_was: 0.0,
            m,
            frame: 0,
            play: Play::idle(),
            period: 0,
            clock: 0.0,
            elapsed: 0.0,
            regulation,
            wall: 0.0,
            ticks: 0,
            rec: record.then(|| Recording {
                whistle,
                ..Recording::default()
            }),
        }
    }

    pub(super) fn over(&self) -> bool {
        matches!(self.stage, Stage::Over)
    }

    /// Advance by one tick of play, or over one dead ball. Returns the
    /// reason when the game reaches a natural stop.
    pub(super) fn step(&mut self) -> Option<Pause> {
        match self.stage {
            Stage::Period => {
                self.clock = if self.period < self.rules.periods {
                    self.rules.period_secs
                } else {
                    self.rules.overtime_secs
                } as f32;
                // Two timeouts for the first half, three for the second,
                // one for each overtime; none carry over.
                let timeouts = if self.period == 0 {
                    Some(2)
                } else if self.period == self.rules.periods / 2 {
                    Some(3)
                } else if self.period >= self.rules.periods {
                    Some(1)
                } else {
                    None
                };
                for side in &mut self.sides {
                    side.team_fouls = 0;
                    if let Some(n) = timeouts {
                        side.timeouts = n;
                    }
                }
                let o = (self.first + self.period as usize) % 2;
                self.play.o = o;
                self.loose = None;
                self.event(EventKind::PeriodStart);
                if self.period > 0 {
                    self.adapt(0);
                    self.adapt(1);
                }
                self.stage = Stage::Possession(o, Start::Inbound);
                None
            }
            Stage::Possession(o, start) => {
                if matches!(start, Start::Inbound) {
                    if let Some(side) = self.timeout() {
                        return Some(Pause::Timeout(side));
                    }
                    self.substitutions();
                }
                self.timeout_taken = false;
                if self.frame != o {
                    for side in &mut self.m {
                        for m in side {
                            m.pos = m.pos.flipped();
                            m.target = m.target.flipped();
                            m.vel = m.vel * -1.0;
                        }
                    }
                    self.frame = o;
                    if let Some((path, _)) = self.loose.as_mut() {
                        *path = path.flipped();
                    }
                }
                self.setup(o, start);
                self.stage = Stage::Play;
                None
            }
            Stage::Play => {
                let end = self.tick().or_else(|| {
                    // A possession that will not end is ended; not with a shot in the air.
                    let idle = self.play.flight.is_none()
                        && self.play.after.is_none()
                        && !matches!(self.play.phase, Phase::Line { .. });
                    (idle && self.play.t - self.play.frozen > 45.0).then_some(End::Dead)
                })?;
                let o = self.play.o;
                self.possessions[o] += 1;
                let pace = self.sides[o].tactics.pace as f32 * 0.7
                    + self.sides[1 - o].tactics.pace as f32 * 0.3
                    + (self.sides[1 - o].press_load() - 1.0) * 8.0;
                let dur = self.play.t - self.play.frozen;
                for side in &mut self.sides {
                    side.tick(dur, pace);
                }
                for t in 0..2 {
                    let got = self.sides[t].score - self.seen[t];
                    if got > 0 {
                        self.seen[t] = self.sides[t].score;
                        self.sides[t].unanswered = 0;
                        self.sides[1 - t].conceded(got);
                        // Plus/minus for the ten on the floor.
                        for (side, sign) in [(t, 1), (1 - t, -1)] {
                            let s = &mut self.sides[side];
                            for k in 0..s.on.len() {
                                let slot = s.on[k];
                                s.slots[slot].line.pm += sign * got as i16;
                            }
                        }
                    }
                }
                if matches!(end, End::Dead) {
                    if self.loose.is_none() {
                        // A whistle or a turnover: the ball drops where it was.
                        let mut path = Path::new();
                        path.fall(self.play.ball, self.play.ball_h.min(1.2), 0.0, Vec2::ZERO);
                        self.loose = Some((path, 0.0));
                    }
                } else {
                    self.loose = None;
                }
                let next = match end {
                    End::Dead => Some(Start::Inbound),
                    End::Live(k) => Some(Start::Live(k)),
                    End::Period => None,
                };
                if let Some(start) = next
                    && self.clock > 0.0
                {
                    self.stage = Stage::Possession(1 - o, start);
                    return None;
                }

                self.event(EventKind::PeriodEnd);
                self.wall += 4.0;
                self.cut = true;
                let score = [self.sides[0].score, self.sides[1].score];
                self.periods
                    .push([score[0] - self.before[0], score[1] - self.before[1]]);
                self.before = score;
                self.period += 1;
                if self.period >= self.rules.periods && score[0] != score[1] {
                    self.stage = Stage::Over;
                    Some(Pause::Final)
                } else {
                    self.stage = Stage::Period;
                    Some(Pause::PeriodEnd)
                }
            }
            Stage::Over => Some(Pause::Final),
        }
    }

    /// The box score as it stands.
    pub(super) fn result(&self) -> GameResult {
        let boxes = |side: &Side| {
            side.slots
                .iter()
                .map(|s| {
                    let mut line = s.line;
                    line.secs = (s.played + 0.5) as u32;
                    (s.p.id, line)
                })
                .collect::<Vec<_>>()
        };
        let score = [self.sides[0].score, self.sides[1].score];
        let mut periods = self.periods.clone();
        if matches!(self.stage, Stage::Possession(..) | Stage::Play) {
            periods.push([score[0] - self.before[0], score[1] - self.before[1]]);
        }
        GameResult {
            score,
            periods,
            boxes: [boxes(&self.sides[0]), boxes(&self.sides[1])],
            possessions: self.possessions,
        }
    }

    /// A timeout at this dead ball, if either bench wants one: a coached
    /// side when it has asked, the other when a run has to be stopped.
    fn timeout(&mut self) -> Option<usize> {
        if self.timeout_taken {
            return None;
        }
        let t = (0..2).find(|&t| {
            let s = &self.sides[t];
            s.timeouts > 0
                && if s.coached {
                    s.timeout_wanted
                } else {
                    s.unanswered >= 9
                }
        })?;
        self.sides[t].timeouts -= 1;
        self.sides[t].timeout_wanted = false;
        for (u, side) in self.sides.iter_mut().enumerate() {
            side.rest(u == t);
        }
        self.timeout_taken = true;
        self.event(EventKind::Timeout { side: t as u8 });
        self.adapt(t);
        self.wall += 6.0;
        self.cut = true;
        Some(t)
    }

    /// A bench without a person on it looks at the score and at how it is
    /// being beaten, and changes the system. A poor coach often does not
    /// see it; a good one sees it and has his team better prepared for
    /// the change.
    fn adapt(&mut self, t: usize) {
        let s = &self.sides[t];
        if s.coached || s.coach == 0 || s.adjusted >= 3 {
            return;
        }
        let margin = s.score as i32 - self.sides[1 - t].score as i32;
        let late = self.elapsed > self.regulation * 0.7;
        if margin > -7 && !(margin >= 12 && late) {
            return;
        }
        let coach = s.coach.min(20) as f32 / 20.0;
        if !self.rng.chance(0.2 + 0.7 * coach) {
            return;
        }
        let (s, opp) = (&self.sides[t], &self.sides[1 - t]);
        let (mut fga, mut tpa, mut tpm) = (0u32, 0u32, 0u32);
        for slot in &opp.slots {
            fga += slot.line.fga as u32;
            tpa += slot.line.tpa as u32;
            tpm += slot.line.tpm as u32;
        }
        let share = tpa as f32 / fga.max(1) as f32;
        let hot = tpm as f32 / tpa.max(1) as f32 > 0.36;
        let before = super::Orders::of(&s.tactics);
        let mut o = before;
        if margin <= -7 {
            o.defense = if share > 0.42 && hot {
                // Beaten from outside: get out to the shooters.
                match o.defense {
                    DefenseScheme::Zone23 | DefenseScheme::PackLine => DefenseScheme::ManToMan,
                    _ => DefenseScheme::Zone32,
                }
            } else if share < 0.30 {
                // Beaten at the basket: close the lane.
                match o.defense {
                    DefenseScheme::PackLine => DefenseScheme::Zone23,
                    _ => DefenseScheme::PackLine,
                }
            } else if o.defense == DefenseScheme::ManToMan {
                // Nothing obvious: show them a different picture.
                DefenseScheme::Zone23
            } else {
                DefenseScheme::ManToMan
            };
            if late && margin <= -10 {
                o.press = Press::FullCourt;
                o.pace = (o.pace + 4).min(18);
                o.three_bias = (o.three_bias + 3).min(18);
            }
        } else {
            // Ahead late: take the air out of the ball.
            o.pace = o.pace.saturating_sub(4).max(4);
            o.press = Press::None;
        }
        if o == before {
            return;
        }
        let side = &mut self.sides[t];
        side.orders = Some((o, 0.45 + 0.4 * coach));
        side.adjusted += 1;
        self.event(EventKind::Adjust {
            side: t as u8,
            offense: o.offense,
            defense: o.defense,
            press: o.press,
        });
    }

    pub(super) fn shouted(&mut self, side: usize, shout: Shout) {
        self.event(EventKind::Shout {
            side: side as u8,
            shout,
        });
    }

    /// Dead-ball substitutions and changes of system. A player staying on
    /// keeps his place on the floor even if his court index changes.
    fn substitutions(&mut self) {
        for t in 0..2 {
            if let Some((orders, familiarity)) = self.sides[t].orders.take() {
                orders.apply(&mut self.sides[t].tactics);
                self.sides[t].unfamiliar = 1.0 - familiarity.clamp(0.0, 1.0);
            }
            let old = self.sides[t].on.clone();
            self.sides[t].substitute(self.elapsed, self.regulation, self.rules.foul_limit);
            if old.is_empty() {
                continue;
            }
            self.jump[t] = [Jump::NONE; 5];
            self.bulk_dirty = true;
            let was = self.m[t];
            let mut free: Vec<usize> = (0..5)
                .filter(|&k| !self.sides[t].on.contains(&old[k]))
                .collect();
            let off: Vec<PlayerId> = free
                .iter()
                .map(|&k| self.sides[t].slots[old[k]].p.id)
                .collect();
            let mut n = 0;
            for k in 0..5 {
                let slot = self.sides[t].on[k];
                let stayed = old.iter().position(|&s| s == slot);
                if stayed.is_none() {
                    let on = self.sides[t].slots[slot].p.id;
                    self.event(EventKind::Substitution { on, off: off[n] });
                    n += 1;
                }
                let from = stayed.or_else(|| free.pop()).unwrap_or(k);
                self.m[t][k] = was[from];
            }
        }
    }

    // ---- small accessors -------------------------------------------------

    fn off(&self) -> &Side {
        &self.sides[self.play.o]
    }

    fn def(&self) -> &Side {
        &self.sides[1 - self.play.o]
    }

    fn oe(&self, k: usize, a: Attr) -> f32 {
        let s = self.off();
        s.eff(s.on[k], a)
    }

    fn de(&self, k: usize, a: Attr) -> f32 {
        let s = self.def();
        s.eff(s.on[k], a)
    }

    fn op(&self, k: usize) -> Vec2 {
        self.m[self.play.o][k].pos
    }

    fn dp(&self, k: usize) -> Vec2 {
        self.m[1 - self.play.o][k].pos
    }

    fn oid(&self, k: usize) -> PlayerId {
        let s = self.off();
        s.slots[s.on[k]].p.id
    }

    fn did(&self, k: usize) -> PlayerId {
        let s = self.def();
        s.slots[s.on[k]].p.id
    }

    fn oline(&mut self, k: usize) -> &mut BoxLine {
        let s = &mut self.sides[self.play.o];
        let i = s.on[k];
        &mut s.slots[i].line
    }

    fn dline(&mut self, k: usize) -> &mut BoxLine {
        let s = &mut self.sides[1 - self.play.o];
        let i = s.on[k];
        &mut s.slots[i].line
    }

    fn scheme(&self) -> DefenseScheme {
        self.def().tactics.defense
    }

    fn event(&mut self, kind: EventKind) {
        let (score, offense) = (
            [self.sides[0].score, self.sides[1].score],
            self.play.o as u8,
        );
        if let Some(rec) = &mut self.rec {
            rec.events.push(Event {
                time: self.wall,
                period: self.period,
                clock: self.clock.max(0.0),
                offense,
                score,
                kind,
            });
        }
    }

    /// Defender responsible for attacker `k`: his man, or in a zone whoever
    /// is nearest.
    fn defender_of(&self, k: usize) -> usize {
        if !is_zone(self.scheme())
            && let Some(dk) = self.play.guard.iter().position(|&g| g == k)
        {
            return dk;
        }
        self.nearest_defender(self.op(k)).0
    }

    fn nearest_defender(&self, p: Vec2) -> (usize, f32) {
        let mut best = (0, f32::MAX);
        for dk in 0..5 {
            let dist = self.dp(dk).dist(p);
            if dist < best.1 {
                best = (dk, dist);
            }
        }
        best
    }

    // ---- possession ------------------------------------------------------

    fn setup(&mut self, o: usize, start: Start) {
        self.bulk_dirty = true;
        self.cache.aims_tick = 0;
        let side = &self.sides[o];
        let best = |score: &dyn Fn(usize) -> f32, except: Option<usize>| {
            (0..5)
                .filter(|&k| Some(k) != except)
                .max_by(|&a, &b| score(a).total_cmp(&score(b)))
                .unwrap_or(0)
        };
        let eff = |k: usize, a: Attr| side.eff(side.on[k], a);
        let usage = |k: usize| side.slots[side.on[k]].usage;
        let handler = best(&|k| eff(k, Attr::Handling) + eff(k, Attr::Passing), None);
        let star = best(&usage, None);
        let post = best(
            &|k| (eff(k, Attr::PostMoves) + eff(k, Attr::Strength)) * (0.7 + 0.3 * usage(k)),
            None,
        );
        let set = side.tactics.offense;
        let lead = if set == OffenseSet::Isolation {
            star
        } else {
            handler
        };
        let height = |k: usize| side.slots[side.on[k]].p.body.height_cm;
        let big = match set {
            OffenseSet::Motion | OffenseSet::PostUp if post != lead => post,
            _ => best(&height, Some(lead)),
        };
        let post = if post == lead { big } else { post };
        let unfamiliar = side.unfamiliar;
        let drawn = side.tactics.board.as_ref().is_some_and(|b| b.set == set);
        let board = side.tactics.layout();

        let flip = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
        let (mut spots, mut runs) = offense::spots(&board, flip, lead, big);
        // A cut into the lane is a call, not a habit: run every time, the
        // defense would simply be waiting for it.
        for run in &mut runs {
            if run.is_some_and(|to| to.dist(HOOP) <= 4.5) && !self.rng.chance(0.4) {
                *run = None;
            }
        }
        // Against a zone the big man flashes to the high post, the hole in
        // the middle of every zone. A coach who drew his own set meant it.
        if is_zone(self.sides[1 - o].tactics.defense) && set != OffenseSet::PostUp && !drawn {
            spots[big] = v(-0.4 * flip, 5.5);
        }
        // A squad still learning the set stands in roughly the right places.
        for spot in &mut spots {
            spot.x += (self.rng.f32() - 0.5) * 3.0 * unfamiliar;
            spot.y += (self.rng.f32() - 0.5) * 3.0 * unfamiliar;
        }
        let (holder, live_start) = match start {
            Start::Inbound => (handler, false),
            Start::Live(k) => (k, true),
        };
        // After a basket or a whistle the ball is loose, and somebody has to
        // go and get it.
        let loose = match (start, self.loose) {
            (Start::Inbound, Some((path, t))) => Some(path.at(t)),
            _ => None,
        };
        let fetcher = loose.map(|(at, _)| {
            (0..5)
                .min_by(|&a, &b| {
                    self.m[o][a]
                        .pos
                        .dist(at)
                        .total_cmp(&self.m[o][b].pos.dist(at))
                })
                .unwrap_or(0)
        });
        if loose.is_none() {
            self.loose = None;
        }
        let credit = std::mem::take(&mut self.credit);
        let carried = self.play.ball_h;
        self.play = Play {
            o,
            shot_clock: self.rules.shot_clock as f32,
            live_start,
            wait: if live_start {
                0.0
            } else {
                (INBOUND_WAIT - credit).max(1.2)
            },
            holder: if loose.is_some() { None } else { Some(holder) },
            ball: loose.map_or(self.m[o][holder].pos, |l| l.0),
            ball_h: loose.map_or(if live_start { carried.max(1.0) } else { 1.0 }, |l| l.1),
            fetcher,
            inbound: fetcher.is_some_and(|f| f != handler),
            spots,
            runs,
            handler,
            star,
            post,
            big,
            ..Play::idle()
        };
        if live_start {
            self.start_break(holder);
        } else {
            self.start_inbound(loose.map(|l| l.0), credit > 0.0, None);
        }
        self.lost_by = None;
    }

    fn tick(&mut self) -> Option<End> {
        // The clocks stand still while a whistle is being answered and while
        // free throws are shot; the floor does not.
        let stopped = self.play.dead > 0.0 || matches!(self.play.phase, Phase::Line { .. });
        self.play.t += DT;
        self.wall += DT;
        self.anim += DT;
        if self.play.dead > 0.0 {
            self.play.dead -= DT;
        }
        if stopped {
            self.play.frozen += DT;
        } else {
            self.clock -= DT;
            self.elapsed += DT;
            if self.play.wait > 0.0 {
                self.play.wait -= DT;
            } else if self.play.fetcher.is_none() {
                self.play.shot_clock -= DT;
            }
        }
        self.ticks += 1;
        for side in &mut self.m {
            for m in side {
                m.hard = false;
            }
        }
        for t in 0..2 {
            for k in 0..5 {
                self.prev_pos[t][k] = self.m[t][k].pos;
            }
        }

        if self.bulk_dirty || self.ticks.is_multiple_of(20) {
            self.refresh_bulks();
        }
        if matches!(self.play.phase, Phase::Line { .. }) {
            self.steer_line();
        } else {
            self.steer_offense();
            self.steer_defense();
            self.steer_rebound();
        }
        let bulk = self.bulks();
        let at: [Vec2; 10] = std::array::from_fn(|i| self.m[i / 5][i % 5].pos);
        // Going round a body is worked out every other tick and kept in
        // between: a detour does not change in a fifth of a second.
        if stopped {
            // Standing about while the clock is stopped: nobody has to go round anybody.
            self.way = [NO_WAY; 10];
        } else if self.ticks.is_multiple_of(2) {
            let mut near = [0u16; 10];
            for i in 0..10 {
                for j in (i + 1)..10 {
                    let d = at[j] - at[i];
                    let hit = (d.dot(d) < NEAR * NEAR) as u16;
                    near[i] |= hit << j;
                    near[j] |= hit << i;
                }
            }
            for i in 0..10 {
                self.way[i] = if near[i] == 0 {
                    NO_WAY
                } else {
                    self.way_round(i / 5, i % 5, &bulk, &at, near[i])
                };
            }
        }
        for t in 0..2 {
            for k in 0..5 {
                let top = self.cache.top[t * 5 + k];
                let w = self.way[t * 5 + k];
                let aim = if w.x.is_nan() { self.m[t][k].target } else { w };
                self.m[t][k].step(top, aim, bulk[t * 5 + k].r);
            }
        }
        if let Some(h) = self.play.holder {
            self.hold_off_glass(self.play.o, h);
        }
        // A man taking the ball out stands where the court is not: the bodies
        // pushing everybody back inside do not move him.
        let outside: Vec<(usize, usize, Vec2)> = (0..2)
            .flat_map(|t| (0..5).map(move |k| (t, k)))
            .filter(|&(t, k)| self.m[t][k].out)
            .map(|(t, k)| (t, k, self.m[t][k].pos))
            .collect();
        self.collide(&bulk);
        for (t, k, pos) in outside {
            self.m[t][k].pos = pos;
            self.touch[t * 5 + k] = Touch::CLEAR;
            for i in 0..10 {
                if self.touch[i].is_some() && self.touch[i].with as usize == t * 5 + k {
                    self.touch[i] = Touch::CLEAR;
                }
            }
        }
        self.dead_ball_ends();
        if !stopped {
            self.tire_from_contact(&bulk);
        }
        for t in 0..2 {
            for k in 0..5 {
                let m = &self.m[t][k];
                let still = m.vel.dot(m.vel) < 0.64;
                self.set_for[t][k] = if still { self.set_for[t][k] + DT } else { 0.0 };
            }
        }
        self.watch_stuck();
        self.move_ball();
        self.pick_up();
        if let Some(rec) = &mut self.rec {
            for (t, side) in self.sides.iter().enumerate() {
                rec.fatigue[t] += side.on.iter().map(|&i| side.slots[i].fatigue).sum::<f32>() / 5.0;
            }
            rec.fatigue_ticks += 1;
        }
        let end = if self.play.dead > 0.0 {
            None
        } else {
            self.step_phase()
        };
        self.record();
        if end.is_some() {
            return end;
        }

        let shot_up = self.play.after.is_some()
            || matches!(
                self.play.flight,
                Some(Flight {
                    kind: FlightKind::Shot { .. },
                    ..
                })
            );
        if shot_up
            || stopped
            || self.play.dead > 0.0
            || matches!(self.play.phase, Phase::Line { .. })
        {
            return None;
        }
        if self.clock <= 0.0 {
            return Some(End::Period);
        }
        // Three seconds: nobody camps in the lane while his team has the ball
        // in the frontcourt. Going to the basket with the ball or going up
        // for the shot is not camping.
        if !matches!(self.play.phase, Phase::Advance) && self.play.wait <= 0.0 {
            let o = self.play.o;
            let going = matches!(
                self.play.phase,
                Phase::Drive { .. } | Phase::Gather { .. } | Phase::Post { .. }
            );
            for k in 0..5 {
                let p = self.m[o][k].pos;
                let inside = p.x.abs() <= KEY_HALF_WIDTH && p.y <= KEY_DEPTH;
                if !inside || (going && self.play.holder == Some(k)) {
                    self.play.lane[k] = 0.0;
                    continue;
                }
                self.play.lane[k] += DT;
                if self.play.lane[k] > LANE_SECONDS {
                    self.play.lane = [0.0; 5];
                    return self.turnover(k, TurnoverKind::ThreeSeconds, None, 0.0);
                }
            }
        }
        if self.play.shot_clock <= 0.0 {
            let who = self.play.holder.unwrap_or(self.play.handler);
            self.oline(who).tov += 1;
            let who = self.oid(who);
            self.event(EventKind::Turnover {
                who,
                kind: TurnoverKind::ShotClock,
                stolen_by: None,
            });
            return Some(End::Dead);
        }
        None
    }

    /// Being shoved and bumped costs energy, more the lighter the body that
    /// takes it, so a man who is leaned on all night tires sooner.
    fn tire_from_contact(&mut self, bulk: &[Bulk; 10]) {
        for i in 0..10 {
            let (t, k) = (i / 5, i % 5);
            let slot = self.sides[t].on[k];
            let touch = &self.touch[i];
            // Nothing to pay without contact; a recording still counts him.
            if !touch.is_some() && self.rec.is_none() {
                continue;
            }
            let load = if touch.is_some() {
                touch.pushed + CONTACT_CLOSING * touch.closing
            } else {
                0.0
            };
            // Leaning on a lighter man costs him more than a heavier one
            // leaning on you, and the heavier the partner the worse.
            let other = if touch.is_some() {
                bulk[touch.with as usize].anchor
            } else {
                CONTACT_REF
            };
            let frail = CONTACT_REF / bulk[i].anchor;
            let cost = CONTACT_COST * load * frail * (other / bulk[i].anchor).clamp(0.5, 2.0);
            if cost > 0.0 {
                let s = &mut self.sides[t].slots[slot];
                s.fatigue = (s.fatigue + cost).min(1.0);
            }
            if let Some(rec) = &mut self.rec {
                let n = self.sides[t].slots.len();
                if rec.leaned[t].len() < n {
                    rec.leaned[t].resize(n, 0.0);
                    rec.leaned_ticks[t].resize(n, 0);
                }
                rec.leaned[t][slot] += cost;
                rec.leaned_ticks[t][slot] += 1;
            }
        }
    }

    /// Size and weight-for-strength of the ten bodies as they play now,
    /// `[side * 5 + k]`.
    fn bulks(&self) -> [Bulk; 10] {
        let mut out = self.bulk_base;
        if !matches!(self.play.phase, Phase::Post { .. }) {
            return out;
        }
        let Some(h) = self.play.holder else {
            return out;
        };
        // Backing down is leverage and footwork as much as mass.
        let o = self.play.o;
        let (side, slot) = (&self.sides[o], self.sides[o].on[h]);
        out[o * 5 + h].anchor =
            heft(out[o * 5 + h].anchor * (0.75 + 0.025 * side.eff(slot, Attr::PostMoves)));
        if let Some(dk) = self.play.on_ball {
            let (side, slot) = (&self.sides[1 - o], self.sides[1 - o].on[dk]);
            out[(1 - o) * 5 + dk].anchor = heft(
                out[(1 - o) * 5 + dk].anchor * (0.75 + 0.025 * side.eff(slot, Attr::InteriorD)),
            );
        }
        out
    }

    /// Re-read sizes and strengths: they only move slowly (tiring) or when
    /// somebody comes on.
    fn refresh_bulks(&mut self) {
        self.bulk_base = std::array::from_fn(|i| {
            let side = &self.sides[i / 5];
            let slot = side.on[i % 5];
            let body = &side.slots[slot].p.body;
            Bulk {
                r: contact::radius(body),
                anchor: contact::anchor(body.weight_kg, side.eff(slot, Attr::Strength)),
            }
        });
        self.bulk_dirty = false;
        self.refresh_cache();
    }

    /// Work out the slow things again: top speeds, reaction times, who the
    /// defense fears and how much room each man gives.
    fn refresh_cache(&mut self) {
        let o = self.play.o;
        for t in 0..2 {
            let side = &self.sides[t];
            for k in 0..5 {
                self.cache.top[t * 5 + k] = 4.6 + side.eff(side.on[k], Attr::Speed) * 0.16;
            }
        }
        let side = self.def();
        let unfamiliar = side.unfamiliar;
        let mut lag = [0.0; 5];
        for (dk, l) in lag.iter_mut().enumerate() {
            let slot = &side.slots[side.on[dk]];
            // Sharp, drilled, fresh players move with the ball; the rest
            // arrive a beat later.
            *l = 0.14
                + (20.0 - side.eff(side.on[dk], Attr::Iq)).max(0.0) * 0.012
                + unfamiliar * 0.30
                + slot.fatigue * 0.15;
        }
        // Defenses key on whoever is doing the shooting tonight.
        let off = self.off();
        let team_fga: u16 = off.slots.iter().map(|s| s.line.fga).sum();
        let mut threat = [0.0; 5];
        for (k, t) in threat.iter_mut().enumerate() {
            let share = off.slots[off.on[k]].line.fga as f32 / team_fga.max(12) as f32;
            *t = ((self.oe(k, Attr::ThreePoint) - 7.0) / 7.0).clamp(0.0, 1.0)
                + ((share - 0.2) * 4.0).clamp(0.0, 0.6);
        }
        // A slower defender gives his man room rather than get beaten.
        let mut cushion = [0.0; 5];
        if !is_zone(side.tactics.defense) {
            for (dk, c) in cushion.iter_mut().enumerate() {
                let gap = self.oe(self.play.guard[dk], Attr::Speed) - self.de(dk, Attr::Speed);
                *c = ((gap - 2.5) * 0.2).clamp(0.0, 1.0);
                // A switching defense stays up on the ball whoever ends up
                // there; the price is being driven past.
                if side.tactics.defense == DefenseScheme::SwitchEverything {
                    *c *= 0.3;
                }
            }
        }
        let _ = o;
        self.cache.lag = lag;
        self.cache.threat = threat;
        self.cache.cushion = cushion;
    }

    /// Bodies cannot share a spot: push them apart, heavier and set ones
    /// giving less ground, and keep what each one felt.
    fn collide(&mut self, bulk: &[Bulk; 10]) {
        let mut pos = [Vec2::ZERO; 10];
        let mut vel = [Vec2::ZERO; 10];
        for i in 0..10 {
            pos[i] = self.m[i / 5][i % 5].pos;
            vel[i] = self.m[i / 5][i % 5].vel;
        }
        contact::resolve_ten(&mut pos, &mut vel, bulk, &mut self.touch);
        for i in 0..10 {
            self.m[i / 5][i % 5].pos = pos[i];
            self.m[i / 5][i % 5].vel = vel[i];
        }
    }

    /// Where player `k` of side `t` heads this tick: his target, or a point
    /// round the nearest body standing in his way, on the shorter side.
    fn way_round(&self, t: usize, k: usize, bulk: &[Bulk; 10], at: &[Vec2; 10], near: u16) -> Vec2 {
        let me = &self.m[t][k];
        let to = me.target - me.pos;
        let len = to.len();
        let o = self.play.o;
        // A post player leans on his man and an unbeaten dribbler is held
        // up: contact is the point there.
        let leaning = t == o
            && self.play.holder == Some(k)
            && match self.play.phase {
                Phase::Post { .. } => true,
                Phase::Drive { beaten, .. } => !beaten,
                _ => false,
            };
        let leaning_late = t == o && self.play.holder == Some(k);
        if len < 0.1 || me.pace <= 0.0 || leaning {
            return NO_WAY;
        }
        let dir = to * (1.0 / len);
        let i = t * 5 + k;
        let mut best: Option<(f32, f32, usize)> = None;
        let mut bits = near;
        while bits != 0 {
            let j = bits.trailing_zeros() as usize;
            bits &= bits - 1;
            // A driver who has not seen the man planted for the charge runs
            // straight at him.
            if leaning_late
                && j / 5 != o
                && self
                    .play
                    .hunt
                    .is_some_and(|(hk, _, blind)| blind && hk == j % 5)
            {
                continue;
            }
            // A helper who has come too late is met, not gone round.
            if leaning_late && j / 5 != o && Some(j % 5) == self.play.late_help {
                continue;
            }
            let other = at[j];
            let rel = other - me.pos;
            let ahead = rel.dot(dir);
            if ahead <= 0.0 || ahead > len.min(1.8) {
                continue;
            }
            let reach = bulk[i].r + bulk[j].r;
            let side = dir.x * rel.y - dir.y * rel.x;
            if side.abs() >= reach + 0.12 || other.dist(me.target) < reach + 0.35 {
                continue;
            }
            if best.is_none_or(|b| ahead < b.0) {
                best = Some((ahead, side, j));
            }
        }
        let Some((_, side, j)) = best else {
            return NO_WAY;
        };
        // He is on my left when `side` is positive, so go right unless that
        // way is crowded; a body dead ahead is passed on the side toward
        // mid-court, then by index.
        let right = Vec2 {
            x: dir.y,
            y: -dir.x,
        };
        let clear = bulk[i].r + bulk[j].r + 0.2;
        let other = at[j];
        let lean = if side > 0.03 {
            1.0
        } else if side < -0.03 {
            -1.0
        } else if me.pos.x.abs() > 0.05 {
            if right.x * me.pos.x < 0.0 { 1.0 } else { -1.0 }
        } else if i.is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        // Count the bodies each detour would run into.
        let crowd = |go: f32| {
            let way = other + right * (go * clear) + dir * clear;
            let mid = me.pos.lerp(way, 0.5);
            let mut n = 0.0;
            for q in 0..10 {
                if q == i || q == j {
                    continue;
                }
                let tight = bulk[i].r + bulk[q].r + 0.1;
                if at[q].dist(way) < tight || at[q].dist(mid) < tight {
                    n += 1.0;
                }
            }
            n
        };
        // Only a body almost dead ahead can be passed on either side.
        let go = if side.abs() < 0.15 && crowd(lean) > crowd(-lean) {
            -lean
        } else {
            lean
        };
        let way = other + right * (go * clear) + dir * clear;
        if way.dist(me.pos) < len { way } else { NO_WAY }
    }

    /// Diagnostic: count players who stand nearly still far from where they
    /// are headed for more than two seconds of play.
    fn watch_stuck(&mut self) {
        if self.rec.is_none() || self.play.wait > 0.0 {
            return;
        }
        for t in 0..2 {
            for k in 0..5 {
                let m = &self.m[t][k];
                let held = (t != self.play.o && self.play.stun[k] > 0.0)
                    || (t == self.play.o && self.play.holder == Some(k));
                let still =
                    m.pace > 0.1 && !held && m.pos.dist(m.target) > 3.0 && m.vel.len() < 0.3;
                let before = self.still[t][k];
                self.still[t][k] = if still { before + DT } else { 0.0 };
                if before <= 2.0
                    && self.still[t][k] > 2.0
                    && let Some(rec) = &mut self.rec
                {
                    rec.stuck += 1;
                }
            }
        }
    }

    fn steer_offense(&mut self) {
        let o = self.play.o;
        for k in 0..5 {
            let pos = self.m[o][k].pos;
            let mut brake = false;
            let (target, pace) = if self.play.holder.is_none()
                && self.play.flight.is_none()
                && self.play.fetcher == Some(k)
            {
                // Going after the loose ball.
                (self.play.ball, 0.9)
            } else if let Some(ib) = self.play.inb.filter(|ib| ib.who == k && self.play.holder == Some(k)) {
                // Out to the spot he takes it from, and stands there.
                self.m[o][k].out = true;
                (ib.spot, 0.9)
            } else if self.play.holder == Some(k) && (self.play.wait > 0.0 || self.play.dead > 0.0)
            {
                (pos, 0.0)
            } else if self.play.holder == Some(k) {
                match self.play.phase {
                    Phase::Advance if self.play.brk => {
                        // Straight at the basket, at the speed he can dribble.
                        let aim = HOOP.toward(pos, 2.4_f32.min(pos.dist(HOOP)));
                        (aim, self.dribble_pace(k).min(self.play.go[k].max(0.6)))
                    }
                    Phase::Advance => {
                        if k == self.play.handler || self.play.live_start {
                            let spot = self.play.spots[k];
                            // Walk it up to the top, then to the spot.
                            let pace = if self.play.live_start { 0.55 } else { 0.30 }
                                + self.off().tactics.pace as f32 * 0.014;
                            (
                                if pos.y > 11.0 {
                                    v(spot.x * 0.5, 9.4)
                                } else {
                                    spot
                                },
                                pace,
                            )
                        } else {
                            (v(pos.x, pos.y - 2.0), 0.3)
                        }
                    }
                    Phase::Drive { .. }
                        if self
                            .play
                            .shoulder
                            .is_some_and(|w| pos.dist(w) > SHOULDER_REACH) =>
                    {
                        // Off the screen, brushing the screener's shoulder.
                        (self.play.shoulder.unwrap_or(pos), SHOULDER_PACE)
                    }
                    Phase::Drive { beaten, .. } => {
                        let side = if pos.x >= 0.0 { 0.6 } else { -0.6 };
                        // A contained dribbler gains very little ground.
                        (HOOP + v(side, 0.5), if beaten { 0.9 } else { 0.15 })
                    }
                    Phase::Post { .. } => (pos.toward(HOOP, 0.8), POST_PUSH),
                    Phase::Gather { .. } => {
                        let style = self.play.shot.map_or(Style::Jumper, |s| s.style);
                        if matches!(style, Style::Layup | Style::Dunk) {
                            // He keeps going to the basket, a step past it.
                            (HOOP + (HOOP - pos).norm() * 0.45, 0.8)
                        } else {
                            // A jump stop: both feet planted, squared up.
                            brake = true;
                            (pos, 0.0)
                        }
                    }
                    _ => (pos, 0.0),
                }
            } else {
                match self.play.task[k] {
                    Task::Spot => {
                        // Two and a bit seconds in the lane: he steps out, and
                        // the place he was drawn at is given up for this play.
                        if self.play.lane[k] > LANE_WARN {
                            let side = if pos.x >= 0.0 { 1.0 } else { -1.0 };
                            self.play.spots[k] = v(side * (KEY_HALF_WIDTH + 0.9), pos.y.clamp(0.8, KEY_DEPTH));
                            self.play.lane[k] = 0.0;
                        }
                        let pace = if matches!(self.play.phase, Phase::Advance) {
                            0.9
                        } else {
                            0.6
                        };
                        (self.play.spots[k], pace)
                    }
                    Task::Cut(to) => {
                        if pos.dist(to) < 1.0 {
                            self.play.task[k] = Task::Spot;
                        }
                        (to, 0.9)
                    }
                    Task::SetScreen => {
                        let dk = self.play.on_ball.unwrap_or(0);
                        let q = self.dp(dk);
                        // Plant against the defender's body, on his side.
                        let r = contact::radius(&self.off().slots[self.off().on[k]].p.body)
                            + contact::radius(&self.def().slots[self.def().on[dk]].p.body);
                        let side = if pos.x >= q.x { r + 0.1 } else { -(r + 0.1) };
                        (q + v(side, 0.1), 0.8)
                    }
                    Task::Roll => {
                        let side = if pos.x >= 0.0 { 1.0 } else { -1.0 };
                        (HOOP + v(side, 0.9), 0.85)
                    }
                    Task::Pop(p) => (p, 0.85),
                    Task::Crash => (HOOP.toward(pos, 1.4), 1.0),
                    Task::GetBack => (v(pos.x * 0.6, 16.0), 0.9),
                    Task::Fill(to) => (to, self.play.go[k]),
                    Task::Watch(until) => {
                        if self.play.t >= until {
                            self.play.task[k] = Task::GetBack;
                        }
                        let airborne = self
                            .play
                            .shot
                            .is_some_and(|s| s.shooter == k && self.play.t < s.land);
                        if airborne {
                            // Still carrying his speed into the jump.
                            let vel = self.m[o][k].vel;
                            (pos + vel, 1.0)
                        } else {
                            brake = true;
                            (pos, 0.0)
                        }
                    }
                }
            };
            let m = &mut self.m[o][k];
            m.target = target;
            m.pace = pace;
            m.hard = brake;
        }
    }

    fn steer_defense(&mut self) {
        let o = self.play.o;
        let d = 1 - o;
        let (mut ball, mut owner) = (self.play.ball, self.play.holder);
        let mut boxout = self.play.boxout;
        match self.play.flight {
            Some(Flight {
                kind: FlightKind::Pass { to, .. },
                ..
            }) => {
                // Move on the flight of the ball.
                ball = self.op(to);
                owner = Some(to);
            }
            Some(Flight {
                kind: FlightKind::Steal { .. },
                ..
            }) => {}
            Some(Flight {
                kind: FlightKind::Shot { .. },
                ..
            }) => boxout = true,
            None => {}
        }
        let side = self.def();
        let advancing = matches!(self.play.phase, Phase::Advance);
        let (pickup, pressing) = match (advancing, side.tactics.press) {
            (false, _) => (LENGTH, false),
            (true, Press::None) => (9.8, false),
            (true, Press::ThreeQuarter) => (20.0, ball.y > 14.0),
            (true, Press::FullCourt) => (27.0, ball.y > 14.0),
        };
        // A press is played man to man until the ball is over half court.
        let scheme = if pressing {
            DefenseScheme::ManToMan
        } else {
            side.tactics.defense
        };
        let beaten = matches!(self.play.phase, Phase::Drive { beaten: true, .. });
        // The aims are worked out twice a second and held between: where a
        // defender wants to stand does not change in a tenth of one.
        let fresh = self.ticks.wrapping_sub(self.cache.aims_tick) >= 2 || self.ticks <= 1;
        let rushed = self.play.brk
            && matches!(self.play.phase, Phase::Advance | Phase::Drive { .. })
            && !matches!(
                self.play.flight,
                Some(Flight {
                    kind: FlightKind::Shot { .. },
                    ..
                })
            );
        if fresh && rushed {
            // The ball is coming at them: nearest man stops it, the rest get
            // goal side of the men nearest the basket.
            let back = defense::get_back(&defense::Rush {
                o: std::array::from_fn(|k| self.op(k)),
                d: std::array::from_fn(|k| self.dp(k)),
                ball,
                handler: owner.unwrap_or(self.play.handler),
            });
            self.play.guard = back.man;
            self.cache.aims = defense::Aims {
                at: back.at,
                on_ball: Some(back.stopper),
                helper: None,
            };
            self.cache.aims_tick = self.ticks;
        } else if fresh {
            let view = View {
                scheme,
                o: std::array::from_fn(|k| self.op(k)),
                d: std::array::from_fn(|k| self.dp(k)),
                ball,
                owner,
                guard: self.play.guard,
                cushion: self.cache.cushion,
                threat: self.cache.threat,
                aggression: side.aggression(),
                beaten,
                drop: self.play.drop,
                hedge: self.play.t < self.play.hedge_until,
                pickup,
                boxout,
                driving: matches!(self.play.phase, Phase::Drive { .. })
                    && self.play.source != Source::Transition,
                posted: matches!(self.play.phase, Phase::Post { .. }),
            };
            self.cache.aims = defense::aims(&view);
            self.cache.aims_tick = self.ticks;
        }
        let aims = self.cache.aims;
        if !matches!(self.play.phase, Phase::Drive { .. } | Phase::Screen { .. }) {
            self.play.on_ball = aims.on_ball;
        }
        if let (Some(helper), false) = (aims.helper, self.play.help_logged)
            && let Some(h) = self.play.holder
        {
            self.play.help_logged = true;
            let (helper, on) = (self.did(helper), self.oid(h));
            self.event(EventKind::Help { helper, on });
        }

        // A zone has to see where the ball went and agree whose it is
        // before anyone moves, which is why quick reversals hurt it.
        let read = if is_zone(scheme) { 0.14 } else { 0.0 };
        for dk in 0..5 {
            if self.play.stun[dk] > 0.0 {
                self.play.stun[dk] -= DT;
                continue;
            }
            let lag = self.cache.lag[dk] + read;
            let m = &mut self.m[d][dk];
            m.target = m.target + (aims.at[dk] - m.target) * (DT / lag).min(1.0);
            let chasing = beaten && aims.on_ball == Some(dk);
            m.pace = if chasing {
                0.82
            } else if m.pos.dist(m.target) > 2.5 {
                1.0
            } else {
                0.88
            };
        }
        // A ball picked off is already on its way to the thief's hand: his
        // team is running before it gets there, with whatever speed they have.
        if let Some(Flight {
            kind: FlightKind::Steal { .. },
            ..
        }) = self.play.flight
        {
            for dk in 0..5 {
                if self.sides[d].slots[self.sides[d].on[dk]].fatigue > RUN_LIMIT {
                    continue;
                }
                let pace = self.run_pace(d, dk);
                let m = &mut self.m[d][dk];
                m.target = v(m.pos.x * 0.7, (m.pos.y + 9.0).min(LENGTH - 2.0));
                m.pace = pace;
            }
        }
        // A man sent round a screen goes where he was told.
        if let Some((rk, spot, until)) = self.play.route {
            if self.play.t >= until {
                self.play.route = None;
            } else if self.play.stun[rk] <= 0.0 {
                let m = &mut self.m[d][rk];
                m.target = spot;
                m.pace = 1.0;
            }
        }
        // Whoever has gone to take a charge runs to his spot and stands
        // there, set, until the drive is over.
        if !matches!(self.play.phase, Phase::Drive { beaten: true, .. }) {
            self.play.hunt = None;
        }
        if let Some((hk, spot, _)) = self.play.hunt
            && self.play.stun[hk] <= 0.0
        {
            let m = &mut self.m[d][hk];
            m.target = spot;
            m.pace = if m.pos.dist(spot) < 0.15 { 0.0 } else { 1.0 };
        }
    }

    /// The ball: on its path in the air, in a hand, or loose on the floor.
    fn move_ball(&mut self) {
        let o = self.play.o;
        if let Some(f) = self.play.flight.as_mut() {
            f.t += DT;
            let (t, from, dur) = (f.t, f.from, f.dur);
            // A pass goes to where the receiver is by the time it gets there.
            let to = match f.kind {
                FlightKind::Pass { to, .. } => self.m[o][to].pos,
                FlightKind::Steal { to } => self.m[1 - o][to].pos,
                FlightKind::Shot { .. } => f.to,
            };
            if let Some(path) = f.path {
                let (p, z) = path.at(t);
                self.play.ball = p;
                self.play.ball_h = z;
            } else {
                let u = (t / dur).min(1.0);
                self.play.ball = from.lerp(to, u);
                self.play.ball_h = 1.3 + 0.5 * 4.0 * u * (1.0 - u);
            }
        } else if let Some(a) = self.play.after.as_mut() {
            a.t += DT;
            let (mut p, z) = a.path.at(a.t);
            if let AfterKind::Inbound { who } = a.kind {
                // Into his hands, wherever he has got to.
                let at = self.m[o][who].pos;
                let u = (a.t / a.end.max(0.05)).min(1.0);
                p = p + (at - a.path.end().0) * (u * u);
            }
            self.play.ball = p;
            self.play.ball_h = z;
        } else if let Some(h) = self.play.holder {
            // A man holding the ball up at the height of the glass cannot
            // take it through: his body stops against it.
            let m = &self.m[o][h];
            self.play.ball = m.pos + m.vel * 0.08;
            // The hands come down after a catch.
            self.play.ball_h = (self.play.ball_h - 4.0 * DT).max(1.0);
        } else if let Some((path, t)) = self.loose.as_mut() {
            *t += DT;
            let (p, z) = path.at(*t);
            self.play.ball = p;
            self.play.ball_h = z;
        }
    }

    /// A loose ball is taken by the man sent for it once he is on it.
    fn pick_up(&mut self) {
        if self.play.holder.is_some() || self.play.flight.is_some() || self.play.after.is_some() {
            return;
        }
        let Some(k) = self.play.fetcher else { return };
        let near = self.op(k).dist(self.play.ball) < 0.55 && self.play.ball_h < 1.6;
        if near || self.play.t - self.play.fetch_t > 6.0 {
            self.play.holder = Some(k);
            self.play.fetcher = None;
            self.loose = None;
            self.play.ball = self.op(k);
            self.play.wait = self.play.wait.max(0.8);
            if let Phase::Line { shooter, .. } = self.play.phase {
                debug_assert_eq!(shooter, k);
            }
        }
    }

    fn step_phase(&mut self) -> Option<End> {
        if let Some(f) = self.play.flight {
            if f.t >= f.dur {
                self.play.flight = None;
                return self.land(f);
            }
            return None;
        }
        if let Some(a) = self.play.after {
            if matches!(a.kind, AfterKind::Live { .. }) {
                return self.live_ball(a);
            }
            if a.t >= a.end {
                self.play.after = None;
                return self.settle(a);
            }
            return None;
        }
        if matches!(self.play.phase, Phase::Line { .. }) {
            return self.line_up();
        }
        let h = self.play.holder?;
        if self.play.wait > 0.0 {
            return None;
        }
        self.play.think -= DT;
        let left = self.play.shot_clock.min(self.clock);
        if left < 1.0 && !matches!(self.play.phase, Phase::Gather { .. }) {
            // Out of time: put something up.
            self.play.source = Source::Forced;
            return self.gather(0.3, true);
        }
        match self.play.phase {
            Phase::Advance => self.advance(h),
            Phase::Set => {
                self.play.brk = false;
                if self.play.think <= 0.0 {
                    self.decide(h)
                } else {
                    None
                }
            }
            Phase::Screen { s, until } => {
                let dk = self.play.on_ball.unwrap_or(0);
                // He has to get his body up against the defender's.
                let bulk = self.bulks();
                let o = self.play.o;
                let near = bulk[o * 5 + s].r + bulk[(1 - o) * 5 + dk].r + SCREEN_SET;
                let arrived = self.op(s).dist(self.dp(dk)) < near;
                if arrived || self.play.t >= until {
                    self.resolve_screen(h, s, arrived)
                } else {
                    None
                }
            }
            Phase::Drive { dk, beaten, until } => self.driving(h, dk, beaten, until),
            Phase::Post { dk, until } => {
                if self.play.t >= until {
                    self.post_move(h, dk)
                } else {
                    None
                }
            }
            Phase::Gather { at, off_dribble } => {
                // The verdict is in as his feet leave the floor.
                let up = self.play.shot.is_none_or(|s| self.play.t >= s.jump_at);
                if up && self.play.verdict.is_none() {
                    self.play.verdict = Some(self.verdict(h, off_dribble));
                }
                if self.play.t >= at {
                    self.shoot(h, off_dribble)
                } else {
                    None
                }
            }
            Phase::Air | Phase::Line { .. } => None,
        }
    }

    // ---- bringing it up --------------------------------------------------

    fn advance(&mut self, h: usize) -> Option<End> {
        if let Some(ib) = self.play.inb {
            return self.throw_in(h, ib);
        }
        let p = self.op(h);
        let press = self.def().tactics.press;

        if p.y > 14.0 && press != Press::None {
            // Pressure in the backcourt.
            let (dk, dist) = self.nearest_defender(p);
            if dist < 2.4 {
                let rate = if press == Press::FullCourt {
                    0.030
                } else {
                    0.016
                };
                let ratio = (self.de(dk, Attr::Steal) + self.de(dk, Attr::Speed))
                    / (self.oe(h, Attr::Handling) + self.oe(h, Attr::Passing)).max(4.0);
                let unsure = 1.0 + 0.4 * self.off().unfamiliar;
                if self.rng.chance(rate * DT * ratio * ratio * unsure) {
                    return self.turnover(h, TurnoverKind::Pressed, Some(dk), 0.75);
                }
            }
        }
        if self.play.brk {
            return self.push(h);
        }
        let run = |game: &mut Self| {
            game.play.source = Source::Transition;
            game.play.phase = Phase::Drive {
                dk: game.defender_of(h),
                beaten: true,
                until: game.play.t + 6.0,
            };
            game.play.dribbled = true;
        };
        if p.y <= 14.0 && !self.play.crossed {
            self.play.crossed = true;
            // Beating a press leaves the pressing team short at the back.
            if press != Press::None && !self.play.live_start {
                let base = if press == Press::FullCourt {
                    0.20
                } else {
                    0.10
                };
                let skill = (self.oe(h, Attr::Handling) + self.oe(h, Attr::Passing)) / 20.0;
                if self.rng.chance(base * (0.4 + 0.6 * skill)) {
                    for dk in 0..5 {
                        if self.dp(dk).y > p.y - 6.0 {
                            self.play.stun[dk] = 0.9;
                        }
                    }
                    let who = self.oid(h);
                    self.event(EventKind::PressBroken { who });
                    run(self);
                    return None;
                }
            }
        }
        // Numbers on a live ball: run. Defenders who went to the offensive
        // glass are the ones missing.
        if self.play.live_start && p.y < 22.0 && p.y > 8.0 {
            let back = (0..5).filter(|&dk| self.dp(dk).y < p.y - 0.5).count();
            let up = (0..5).filter(|&k| self.op(k).y < p.y + 2.0).count();
            if back <= 2 && up > back {
                let who = self.oid(h);
                self.event(EventKind::FastBreak { who });
                self.play.live_start = false;
                run(self);
                return None;
            }
        }

        // Outlet to the point guard.
        let handler = self.play.handler;
        if h != handler && self.play.t > 0.8 && p.dist(self.op(handler)) < 12.0 {
            self.play.live_start = false;
            return self.pass(h, handler, true);
        }
        let target = self.m[self.play.o][h].target;
        if p.y < 10.6 || (p.y < 14.0 && p.dist(target) < 1.2) {
            self.play.phase = Phase::Set;
            self.play.think = 0.3;
        }
        None
    }

    // ---- throwing it in -------------------------------------------------------

    /// A dead ball: one man takes it out where the rules say, a different
    /// man comes to get it. `at` is where the ball died (none at the start of
    /// a period), `made` that it went through the net, and `carrier` the man
    /// who already has it in his hands.
    fn start_inbound(&mut self, at: Option<Vec2>, made: bool, carrier: Option<usize>) {
        let o = self.play.o;
        let spot = match at {
            None => v(HALF_WIDTH + 1.0, 14.0),
            // Behind the baseline of the basket just scored on.
            Some(_) if made => v(0.0, LENGTH + 1.0),
            Some(a) => {
                let side = HALF_WIDTH - a.x.abs();
                let end = a.y.min(LENGTH - a.y);
                if end < side {
                    v(a.x.clamp(-5.0, 5.0), if a.y > 14.0 { LENGTH + 1.0 } else { -1.0 })
                } else {
                    let s = if a.x >= 0.0 { 1.0 } else { -1.0 };
                    v(s * (HALF_WIDTH + 1.0), a.y.clamp(2.0, LENGTH - 2.0))
                }
            }
        };
        let handler = self.play.handler;
        let from = at.unwrap_or(spot);
        let who = carrier.unwrap_or_else(|| {
            (0..5)
                .filter(|&k| k != handler)
                .min_by(|&a, &b| self.op(a).dist(from).total_cmp(&self.op(b).dist(from)))
                .unwrap_or(0)
        });
        // The receiver is somebody else: the ball handler, unless it is he.
        let to = if who != handler {
            handler
        } else {
            (0..5)
                .filter(|&k| k != who)
                .max_by(|&a, &b| {
                    let f = |k: usize| self.oe(k, Attr::Handling) + self.oe(k, Attr::Passing);
                    f(a).total_cmp(&f(b))
                })
                .unwrap_or(0)
        };
        let mut rx = spot.toward(v(0.0, 14.0), 5.5);
        rx.x = rx.x.clamp(-5.5, 5.5);
        if rx.x.abs() < 2.0 {
            rx.x = if spot.x >= 0.0 { 3.0 } else { -3.0 };
        }
        rx.y = rx.y.clamp(2.0, LENGTH - 2.0);
        self.play.inb = Some(Inb {
            who,
            to,
            spot,
            rx,
            since: None,
        });
        self.play.handler = to;
        self.play.phase = Phase::Advance;
        self.play.live_start = false;
        self.play.brk = false;
        self.play.inbound = true;
        self.play.task = [Task::Spot; 5];
        self.play.task[to] = Task::Fill(rx);
        self.play.go[to] = 0.9;
        match (carrier, at) {
            (Some(c), _) => self.play.holder = Some(c),
            (None, None) => {
                // The start of a period: he is already at the spot with it.
                self.m[o][who].pos = spot;
                self.m[o][who].out = true;
                self.m[o][who].vel = Vec2::ZERO;
                self.play.holder = Some(who);
                self.play.fetcher = None;
                self.play.ball = spot;
            }
            (None, Some(_)) => {
                self.play.holder = None;
                self.play.fetcher = Some(who);
                self.play.fetch_t = self.play.t;
            }
        }
    }

    /// The man with the ball at the line looks for somebody open: the man
    /// who came for it if his lane is clear, otherwise another, and he has
    /// to throw within five seconds.
    fn throw_in(&mut self, h: usize, ib: Inb) -> Option<End> {
        if h != ib.who {
            self.play.inb = None;
            return None;
        }
        if self.op(h).dist(ib.spot) > 0.7 {
            return None;
        }
        let since = ib.since.unwrap_or(self.play.t);
        if let Some(i) = self.play.inb.as_mut() {
            i.since = Some(since);
        }
        let waited = self.play.t - since;
        if waited < 0.6 {
            return None;
        }
        let ready = self.op(ib.to).dist(ib.rx) < 3.0 || waited > 1.5;
        if !ready {
            return None;
        }
        let mut best = (ib.to, self.pass_risk(h, ib.to));
        if best.1 > 0.10 {
            for j in 0..5 {
                if j == h || j == ib.to {
                    continue;
                }
                let r = self.pass_risk(h, j);
                if r < best.1 {
                    best = (j, r);
                }
            }
        }
        if best.1 > 0.10 && waited < 4.0 {
            return None;
        }
        self.play.inb = None;
        self.play.handler = best.0;
        self.play.task = [Task::Spot; 5];
        self.play.inbound = true;
        // A team that likes to run pushes it up the floor off the throw-in.
        let tempo = self.off().tactics.pace as f32;
        self.play.live_start = self.rng.chance(PUSH_OFF_INBOUND * (0.4 + 0.06 * tempo));
        self.pass(h, best.0, false)
    }

    /// After a whistle for a foul that is not a shooting foul, the floor
    /// stands still a moment and the ball is taken out at the spot.
    fn dead_ball_ends(&mut self) {
        if self.dead_was > 0.0
            && self.play.dead <= 0.0
            && self.play.inb.is_none()
            && matches!(self.play.phase, Phase::Set)
            && self.play.flight.is_none()
            && self.play.after.is_none()
            && let Some(h) = self.play.holder
        {
            let at = self.op(h);
            self.start_inbound(Some(at), false, Some(h));
        }
        self.dead_was = self.play.dead;
    }

    // ---- the break ---------------------------------------------------------

    /// How fast `k` of the side with the ball dribbles: handling decides how
    /// much speed he can carry the ball at, and tired legs take it away.
    fn dribble_pace(&self, k: usize) -> f32 {
        let f = self.off().slots[self.off().on[k]].fatigue;
        (0.80 + 0.0085 * self.oe(k, Attr::Handling)).min(0.97) * (1.0 - 0.5 * (f - 0.2).max(0.0))
    }

    /// The share of his top speed that man `k` of side `t` is willing to
    /// run at on the floor: the tempo asked for pushes it up, tiredness
    /// takes it away.
    fn run_pace(&self, t: usize, k: usize) -> f32 {
        let side = &self.sides[t];
        let f = side.slots[side.on[k]].fatigue;
        let tempo = side.tactics.pace as f32 - 10.0;
        (1.0 - 0.8 * (f - 0.15).max(0.0) + 0.012 * tempo).clamp(0.5, 1.0)
    }

    /// The ball has changed hands live: who runs, and where to. The nearest
    /// men to the far basket fill the wings and trail; whoever is tired or
    /// has been told to get back stays at half court. The team that lost it
    /// loses a beat, longest the man who gave it away.
    fn start_break(&mut self, h: usize) {
        let o = self.play.o;
        self.play.brk = true;
        self.play.live_origin = true;
        self.play.source = Source::Transition;
        let tempo = self.off().tactics.pace;
        let wanted = 2 + (tempo >= 13) as usize + (tempo >= 17) as usize;
        let mut order: Vec<usize> = (0..5).filter(|&k| k != h).collect();
        order.sort_by(|&a, &b| self.op(a).y.total_cmp(&self.op(b).y));
        let side = if self.op(order[0]).x >= 0.0 { 1.0 } else { -1.0 };
        let lanes = [
            v(side * 4.3, 2.8),
            v(-side * 4.3, 2.8),
            v(side * 1.4, 7.8),
            v(-side * 2.6, 10.0),
        ];
        for (i, &k) in order.iter().enumerate() {
            let pos = self.op(k);
            let tired = self.off().slots[self.off().on[k]].fatigue > RUN_LIMIT;
            let pace = self.run_pace(o, k);
            // Two fill the wings, two trail the ball to the top of the floor;
            // a team told to slow it down keeps one at half court, and a
            // tired man does not sprint.
            let safety = i == order.len() - 1 && tempo <= 7;
            if tired || safety {
                self.play.task[k] = Task::Fill(v(pos.x * 0.6, 14.5));
                self.play.go[k] = pace * 0.85;
            } else {
                self.play.task[k] = Task::Fill(lanes[i]);
                self.play.go[k] = if i < wanted { pace } else { pace * 0.85 };
            }
        }
        self.play.go[h] = self.run_pace(o, h);
        let lost = self.lost_by;
        for dk in 0..5 {
            let iq = self.de(dk, Attr::Iq);
            let mut beat = BEAT_BASE + (20.0 - iq).max(0.0) * BEAT_IQ;
            if Some(dk) == lost {
                beat += BEAT_LOST;
            }
            self.play.stun[dk] = beat;
        }
        self.play.think = 0.1 + (20.0 - self.oe(h, Attr::Iq)).max(0.0) * 0.01;
    }

    /// One look up the floor by the man with the ball on the break: throw it
    /// ahead, take it at the first defender, or give up the run.
    fn push(&mut self, h: usize) -> Option<End> {
        if self.play.think > 0.0 {
            return None;
        }
        self.play.think = 0.2;
        let p = self.op(h);
        let to_rim = p.dist(HOOP);
        let mut back = 0usize;
        let mut first: Option<(usize, f32)> = None;
        for dk in 0..5 {
            let q = self.dp(dk);
            if q.y < p.y + 0.4 {
                back += 1;
                let dist = q.dist(p);
                if first.is_none_or(|f| dist < f.1) {
                    first = Some((dk, dist));
                }
            }
        }
        let up = 1 + (0..5)
            .filter(|&k| k != h && self.op(k).y < p.y + 1.0)
            .count();

        // A defender who has been beaten and is chasing from behind, with
        // nobody in front of the ball, may pull him down to stop two points.
        if to_rim < 10.0 {
            let chaser = (0..5)
                .filter(|&dk| {
                    let q = self.dp(dk);
                    q.dist(p) < 1.6 && q.dist(HOOP) > to_rim + 0.2
                })
                .min_by(|&a, &b| self.dp(a).dist(p).total_cmp(&self.dp(b).dist(p)));
            let clear = (0..5).all(|dk| {
                let q = self.dp(dk);
                q.dist(HOOP) > to_rim || q.dist(p) > 4.5
            });
            if let Some(dk) = chaser
                && clear
            {
                let eager = STOP_FOUL * (1.0 + 0.8 * self.def().aggression());
                if self.rng.chance(eager) && self.called(1.0) {
                    return self.floor_foul(dk, h, FoulKind::ReachIn);
                }
            }
        }

        let edge = up as i32 - back as i32;
        if p.y <= 14.0 && !self.play.crossed {
            // Across half court: more of them up the floor than of us means
            // the break is on.
            self.play.crossed = true;
            if edge >= 1 {
                self.break_called(h);
            }
        }
        // Ahead of the ball and open: throw it.
        let vision = 0.35 + (self.oe(h, Attr::Passing) + self.oe(h, Attr::Iq)) / 40.0 * 0.65;
        if p.y > 3.0
            && self.rng.chance(vision)
            && let Some(j) = self.lead_pass(h, first)
        {
            return self.pass(h, j, false);
        }

        let clear = first.is_none_or(|f| f.1 > 3.4);
        if p.y < ATTACK_Y {
            if edge >= 1 || (edge == 0 && clear) {
                return self.attack(h, first, edge);
            }
            return self.pull_out();
        }
        if p.y < PULL_Y && edge < 0 {
            return self.pull_out();
        }
        None
    }

    fn break_called(&mut self, h: usize) {
        if !self.play.brk_logged {
            self.play.brk_logged = true;
            let who = self.oid(h);
            self.event(EventKind::FastBreak { who });
        }
    }

    /// The best teammate to throw it to: well up the floor and open when the
    /// ball gets there.
    fn lead_pass(&mut self, h: usize, first: Option<(usize, f32)>) -> Option<usize> {
        let o = self.play.o;
        let p = self.op(h);
        // A defender in front of the ball makes the throw more worth it.
        let blocked = first.is_some_and(|(dk, d)| d < 4.5 && self.dp(dk).y < p.y);
        let mut best: Option<(usize, f32)> = None;
        for j in 0..5 {
            if j == h || self.play.from == Some(j) {
                continue;
            }
            let m = &self.m[o][j];
            let at = m.pos + m.vel * 0.5;
            let ahead = p.y - at.y;
            if ahead < LEAD_PASS || p.dist(at) > 20.0 {
                continue;
            }
            let rim = ShotKind::at(at) == ShotKind::Rim;
            let (c, _) = self.contest(at, j, rim);
            let c = (c + 0.2).min(1.0);
            let risk = self.pass_risk(h, j);
            let wanted = if blocked { c < 0.7 } else { ahead >= 5.0 && c < 0.45 };
            if !wanted || risk > LEAD_RISK {
                continue;
            }
            let score = ahead * 0.1 - c - risk * 2.0;
            if best.is_none_or(|b| score > b.1) {
                best = Some((j, score));
            }
        }
        best.map(|b| b.0)
    }

    /// Go at the first defender: through the drive's own rules if he is
    /// close enough to be a contest, past him if he is not there.
    fn attack(&mut self, h: usize, first: Option<(usize, f32)>, edge: i32) -> Option<End> {
        self.break_called(h);
        self.play.source = Source::Transition;
        // With the numbers he goes by on the run; one on one it is a drive
        // like any other, with its reaching, its stripping and its charges.
        if edge <= 0
            && let Some((_, dist)) = first
            && dist < 2.6
        {
            let end = self.drive(h, 1.0 + edge.max(0) as f32);
            if !matches!(self.play.phase, Phase::Drive { beaten: true, .. }) {
                self.play.brk = false;
            }
            return end;
        }
        let dk = first.map_or_else(|| self.defender_of(h), |f| f.0);
        self.play.phase = Phase::Drive {
            dk,
            beaten: true,
            until: self.play.t + 4.0,
        };
        self.play.dribbled = true;
        None
    }

    /// He is outnumbered: the break is off, and the team sets up.
    fn pull_out(&mut self) -> Option<End> {
        self.play.brk = false;
        self.play.live_start = false;
        self.play.phase = Phase::Set;
        self.play.think = 0.25;
        self.play.source = Source::PullUp;
        self.play.task = [Task::Spot; 5];
        None
    }

    // ---- reading the floor -----------------------------------------------

    /// What kind of shot attacker `k` has right now and how contested it is.
    fn look(&self, k: usize) -> (ShotKind, f32, Option<usize>) {
        let p = self.op(k);
        let kind = shot_kind(p);
        let (c, near) = self.contest(p, k, kind == ShotKind::Rim);
        (kind, c, near)
    }

    /// 0 = nobody near, 1 = a hand in the face. At the rim every body in the
    /// way counts, so a crowded paint can go above 1.
    fn contest(&self, p: Vec2, k: usize, rim: bool) -> (f32, Option<usize>) {
        let w = self.contest_each(p, k, rim);
        Self::sum_contest(w, rim)
    }

    fn sum_contest(w: [f32; 5], rim: bool) -> (f32, Option<usize>) {
        let mut total = 0.0f32;
        let mut near: Option<(f32, usize)> = None;
        for (dk, &w) in w.iter().enumerate() {
            if w <= 0.0 {
                continue;
            }
            if near.is_none_or(|n| w > n.0) {
                near = Some((w, dk));
            }
            total = if rim { total + w } else { total.max(w) };
        }
        (total.min(if rim { 1.4 } else { 1.0 }), near.map(|n| n.1))
    }

    /// How much each defender bothers a shot from `p` by attacker `k`.
    fn contest_each(&self, p: Vec2, k: usize, rim: bool) -> [f32; 5] {
        let to_hoop = (HOOP - p).norm();
        let off = self.off();
        let reach = off.slots[off.on[k]].p.body.standing_reach_cm;
        let def = self.def();
        let mut out = [0.0f32; 5];
        for dk in 0..5 {
            let q = self.dp(dk);
            let dist = q.dist(p);
            let mut w = if rim {
                1.0 - (dist - 0.5) / 1.7
            } else {
                1.0 - (dist - 0.6) / 1.7
            }
            .clamp(0.0, 1.0);
            if w <= 0.0 {
                continue;
            }
            // A man planted for a charge is not contesting anything.
            if self.play.hunt.is_some_and(|(hk, _, _)| hk == dk) {
                w *= 0.2;
            }
            // A longer defender bothers the shot more; one trailing the
            // play hardly at all.
            let d_reach = def.slots[def.on[dk]].p.body.standing_reach_cm;
            w *= if rim {
                def.length(def.on[dk])
            } else {
                (1.0 + (d_reach - reach) / 100.0).max(0.5)
            };
            if (q - p).dot(to_hoop) < -0.25 && p.dist(HOOP) > 1.0 {
                w *= 0.45;
            }
            out[dk] = w;
        }
        out
    }

    /// How high above standing height defender or attacker `k` of side `t`
    /// is, `ahead` seconds from now, in the jump he is in.
    fn lift_at(&self, t: usize, k: usize, ahead: f32) -> f32 {
        let j = &self.jump[t][k];
        let u = self.anim + ahead - j.start;
        if j.dur <= 0.0 || u <= 0.0 || u >= j.dur {
            return 0.0;
        }
        let w = u / j.dur;
        j.peak * 4.0 * w * (1.0 - w)
    }

    fn pct(&self, k: usize, kind: ShotKind, c: f32, dk: Option<usize>, off_dribble: bool) -> f32 {
        let base = match kind {
            ShotKind::Rim => {
                let finish = 0.6 * self.oe(k, Attr::CloseShot)
                    + 0.2 * self.oe(k, Attr::Vertical)
                    + 0.2 * self.oe(k, Attr::Strength);
                let protect = dk.map_or(10.0, |d| {
                    0.5 * self.de(d, Attr::InteriorD) + 0.5 * self.de(d, Attr::Block)
                });
                0.83 + (finish - 10.0) * 0.013 - c * (0.31 + (protect - 10.0) * 0.012)
            }
            ShotKind::Mid => {
                let guard = dk.map_or(10.0, |d| self.de(d, Attr::PerimeterD));
                // Floaters and short jumpers go in more often than long twos.
                let range = self.op(k).dist(HOOP);
                // Close in, a post player's touch counts as much as his jumper.
                let touch = if range < 4.0 {
                    self.oe(k, Attr::MidRange).max(self.oe(k, Attr::PostMoves))
                } else {
                    self.oe(k, Attr::MidRange)
                };
                0.52 - (range - 1.9).max(0.0) * 0.024 + (touch - 10.0) * 0.011
                    // A hook over a body is a different shot from an open one.
                    - c * (0.11 + (4.0 - range).max(0.0) * 0.035 + (guard - 10.0) * 0.005)
                    - if off_dribble { 0.02 } else { 0.0 }
            }
            ShotKind::Three => {
                let guard = dk.map_or(10.0, |d| self.de(d, Attr::PerimeterD));
                0.372 + (self.oe(k, Attr::ThreePoint) - 10.0) * 0.011
                    - c * (0.115 + (guard - 10.0) * 0.005)
                    - if off_dribble { 0.035 } else { 0.0 }
            }
        };
        let composure = (self.oe(k, Attr::Composure) - 10.0) * 0.001;
        let home = if self.play.o == 0 { 0.008 } else { 0.0 };
        // Players unsure of the set shoot out of rhythm.
        let rhythm = 0.03 * self.off().unfamiliar;
        (base + composure + home - rhythm).clamp(0.05, 0.92)
    }

    fn foul_chance(&self, kind: ShotKind, c: f32) -> f32 {
        let base = match kind {
            ShotKind::Rim => 0.06 + 0.18 * c.min(1.0),
            ShotKind::Mid => 0.015 + 0.06 * c,
            ShotKind::Three => 0.005 + 0.02 * c,
        };
        base * (1.0 + 0.5 * self.def().aggression())
    }

    /// Expected points of that shot, as the player would judge it.
    fn ev(&self, k: usize, kind: ShotKind, c: f32, dk: Option<usize>, off_dribble: bool) -> f32 {
        let pts = kind.points() as f32;
        let bias = (self.off().tactics.three_bias as f32 - 10.0) * 0.012;
        let taste = if kind == ShotKind::Three {
            THREE_TASTE + bias
        } else {
            1.0 - bias * 0.5
        };
        let pct = self.pct(k, kind, c, dk, off_dribble);
        // Good shooters hold themselves to a higher standard: half of what
        // their touch adds is spent on being choosier, not on volume.
        let touch = match kind {
            ShotKind::Rim => 0.0,
            ShotKind::Mid => self.oe(k, Attr::MidRange) - 10.0,
            ShotKind::Three => self.oe(k, Attr::ThreePoint) - 10.0,
        };
        let choosy = touch.max(0.0) * 0.011 * 0.5;
        ((pct - choosy) * pts + self.foul_chance(kind, c) * pts * 0.4) * taste
    }

    /// How good a shot has to be before `k` takes it with `left` seconds on
    /// the clock. Early on only good looks; late, anything.
    fn threshold(&self, k: usize, left: f32) -> f32 {
        let side = self.off();
        let usage = side.slots[side.on[k]].usage;
        let pace = (side.tactics.pace as f32 - 10.0) * 0.012;
        let told = match side.slots[side.on[k]].brief {
            Brief::Shoot => -0.07,
            Brief::Pass => 0.07,
            _ => 0.0,
        };
        1.18 + 0.40 * (left - 12.0) / 12.0 - pace - (usage - 0.9) * 0.06 + told
    }

    fn pass_time(len: f32) -> f32 {
        0.12 + len / 14.0 + (len - 6.0).max(0.0) * 0.035
    }

    fn pass_risk(&self, h: usize, j: usize) -> f32 {
        let (a, b) = (self.op(h), self.op(j));
        let len = a.dist(b);
        let aggression = self.def().aggression();
        let mut risk = 0.003 + (len - 6.0).max(0.0) * 0.008;
        for dk in 0..5 {
            let q = self.dp(dk);
            let on_lane = q.project(a, b);
            if on_lane.dist(a) < 0.7 {
                continue;
            }
            let off_lane = q.dist(on_lane);
            if off_lane < 1.5 {
                // Hands around the catch (a crowded paint) are worse than a
                // defender shading the lane.
                let crowd = if on_lane.dist(b) < 1.2 && b.dist(HOOP) < 4.0 {
                    // The nearer the basket the more bodies and hands there are
                    // to go through: the entry to a man at the rim is the hardest.
                    2.6 + CROWD_RIM * (1.0 - b.dist(HOOP) / 2.4).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                risk += (1.5 - off_lane) / 1.5
                    * 0.021
                    * crowd
                    * (self.de(dk, Attr::Steal) / 10.0)
                    * (1.0 + 0.5 * aggression);
            }
        }
        risk *= 1.55 - self.oe(h, Attr::Passing) / 20.0 * 1.1;
        risk *= 1.0 + 0.5 * self.off().unfamiliar;
        if self.nearest_defender(a).1 < 1.0 {
            risk *= 1.15;
        }
        risk
    }

    /// The teammate with the best look, if it beats `bar` expected points.
    fn best_pass(&mut self, h: usize, bar: f32, left: f32) -> Option<usize> {
        // Any teammate with a good enough look is an option, the better
        // the look the likelier; the passer does not always find the best.
        let mut weights = [0.0f32; 5];
        for j in 0..5 {
            if j == h {
                continue;
            }
            let pj = self.op(j);
            if pj.y > 12.0 {
                continue;
            }
            let kind = ShotKind::at(pj);
            let (c_now, near) = self.contest(pj, j, kind == ShotKind::Rim);
            // The defender closes while the ball is in the air.
            let dur = Self::pass_time(pj.dist(self.op(h))) + 0.3;
            let c = if kind == ShotKind::Rim {
                // The paint collapses on a catch at the rim.
                (c_now + 0.45).min(1.4)
            } else {
                (c_now + dur * 0.6).min(1.0)
            };
            let value = self.ev(j, kind, c, near, false) - self.pass_risk(h, j) * 1.6;
            if value > bar && value > self.threshold(j, left - 1.0) {
                weights[j] = value - bar + 0.04;
            }
        }
        (weights.iter().sum::<f32>() > 0.0).then(|| self.rng.weighted(&weights))
    }

    fn outside(&self, k: usize) -> bool {
        self.op(k).dist(HOOP) > 5.5
    }

    fn in_post(&self, k: usize) -> bool {
        let p = self.op(k);
        p.dist(HOOP) < 4.3 && p.y < 5.5
    }

    fn decide(&mut self, h: usize) -> Option<End> {
        let left = self.play.shot_clock.min(self.clock);
        self.play.drop = None;
        self.play.shoulder = None;
        self.play.help_logged = false;
        // Whoever rolled or popped stays where the play left him.
        for k in 0..5 {
            match self.play.task[k] {
                // Three seconds: the roller cannot wait under the basket,
                // he steps out to the dunker spot on the baseline.
                Task::Roll => {
                    let side = if self.op(k).x >= 0.0 { 3.1 } else { -3.1 };
                    self.play.spots[k] = v(side, 0.9);
                    self.play.task[k] = Task::Spot;
                }
                Task::Pop(_) | Task::SetScreen => {
                    self.play.spots[k] = self.m[self.play.o][k].target;
                    self.play.task[k] = Task::Spot;
                }
                _ => {}
            }
        }

        // Runs off the coach's board: one man at a time leaves his spot for
        // the place he was drawn to.
        if let Some(k) = (0..5).find(|&k| {
            k != h
                && self.play.runs[k].is_some()
                && matches!(self.play.task[k], Task::Spot)
                && self.op(k).dist(self.play.spots[k]) < 1.5
        }) && let Some(to) = self.play.runs[k].take()
        {
            // A run to open floor is a new place to stand. A run into the
            // lane is a cut: nobody may camp there, so if the ball does
            // not find him he clears back out to where he came from.
            if to.dist(HOOP) > 4.5 {
                self.play.spots[k] = to;
            }
            self.play.task[k] = Task::Cut(to);
        }

        // He has been in the lane too long to wait: he goes up with it.
        if self.play.lane[h] > LANE_WARN {
            return self.gather(0.3, self.play.dribbled);
        }
        let (kind, c, near) = self.look(h);
        let value = self.ev(h, kind, c * self.play.edge, near, self.play.dribbled);
        // A player who made the opening himself is expected to finish it.
        let created = matches!(
            self.play.source,
            Source::Drive | Source::PullUp | Source::Post
        );
        let bar = self.threshold(h, left)
            - if self.play.source == Source::Putback {
                PUTBACK_EDGE
            } else if created {
                0.12
            } else {
                0.0
            };
        if value > bar {
            return self.gather(
                if self.play.dribbled { 0.35 } else { 0.3 },
                self.play.dribbled,
            );
        }
        // Whatever room a dribble made is gone once he does not shoot.
        self.play.edge = 1.0;
        let vision = 0.4 + (self.oe(h, Attr::Passing) + self.oe(h, Attr::Iq)) / 40.0 * 0.6;
        if self.rng.chance(vision)
            && let Some(j) = self.best_pass(h, (value + 0.08).max(bar), left)
        {
            return self.pass(h, j, false);
        }
        if left < 3.2 {
            self.play.source = Source::Forced;
            return self.gather(0.3, true);
        }
        let think = 1.0
            + (20.0 - self.oe(h, Attr::Iq)).max(0.0) * 0.015
            + self.rng.f32() * 0.3
            + self.off().unfamiliar * 0.5;
        self.play.think = think;

        // Half-learned sets break down: somebody is in the wrong place and
        // the ball sticks.
        let unfamiliar = self.off().unfamiliar;
        if self.rng.chance(0.10 * unfamiliar) {
            let k = self.rng.range(0, 5) as usize;
            if k != h {
                self.play.spots[k] = v(self.rng.f32() * 10.0 - 5.0, 3.0 + self.rng.f32() * 5.0);
                let who = self.oid(k);
                self.event(EventKind::Mistake {
                    who,
                    kind: Mistake::OutOfPosition,
                    beneficiary: who,
                });
                self.play.think += 0.8;
                return None;
            }
        }
        if left < 6.5 {
            // Time to make something happen.
            return if self.in_post(h) && !self.play.dribbled {
                self.post_up(h)
            } else {
                self.drive(h, 0.0)
            };
        }
        // A big man with the ball on the block goes to work, whatever the set.
        if self.in_post(h) && !self.play.dribbled {
            let dk = self.defender_of(h);
            let (off, def) = (self.off(), self.def());
            let taller =
                off.slots[off.on[h]].p.body.height_cm - def.slots[def.on[dk]].p.body.height_cm;
            let skilled = self.oe(h, Attr::PostMoves) >= 11.0;
            let called = h == self.play.post && off.tactics.offense == OffenseSet::PostUp;
            if called || (skilled && (h == self.play.post || taller >= 8.0)) {
                return self.post_up(h);
            }
        }

        // A contained move usually kills the dribble and the ball has to
        // move. The man the play is built around gets to size his defender
        // up a few times.
        let set = self.off().tactics.offense;
        let lead = match set {
            OffenseSet::Isolation => h == self.play.star,
            OffenseSet::PickAndRoll => h == self.play.handler,
            _ => false,
        };
        let brief = self.off().slots[self.off().on[h]].brief;
        let patience = (if lead { 2 } else { 1 }) - (brief == Brief::Pass) as u8;
        if self.play.moves >= patience {
            return self.swing(h);
        }
        if brief == Brief::Attack && self.rng.chance(0.5) {
            return self.drive(h, 0.0);
        }
        let r = self.rng.f32();
        let (handler, star, post) = (self.play.handler, self.play.star, self.play.post);
        match set {
            OffenseSet::Motion => {
                if r < 0.10 && h != post && self.in_post(post) && self.pass_risk(h, post) < 0.09 {
                    self.pass(h, post, false)
                } else if r < 0.42 {
                    self.drive(h, 0.0)
                } else if r < 0.52 && self.op(h).y > 6.0 {
                    self.ball_screen(h)
                } else {
                    self.swing(h)
                }
            }
            OffenseSet::FiveOut => {
                if r < 0.42 {
                    self.drive(h, 0.0)
                } else if r < 0.50 && self.op(h).y > 6.0 {
                    self.ball_screen(h)
                } else {
                    self.swing(h)
                }
            }
            OffenseSet::PickAndRoll => {
                if h == handler {
                    if r < 0.72 {
                        self.ball_screen(h)
                    } else if r < 0.84 {
                        self.drive(h, 0.0)
                    } else {
                        self.swing(h)
                    }
                } else if r < 0.6 && self.outside(handler) && self.pass_risk(h, handler) < 0.08 {
                    self.pass(h, handler, false)
                } else if r < 0.8 {
                    self.drive(h, 0.0)
                } else {
                    self.swing(h)
                }
            }
            OffenseSet::PostUp => {
                if h != post && self.in_post(post) && self.pass_risk(h, post) < 0.10 && r < 0.8 {
                    self.pass(h, post, false)
                } else if r < 0.25 {
                    self.drive(h, 0.0)
                } else {
                    self.swing(h)
                }
            }
            OffenseSet::Isolation => {
                // Two or three goes at his man is all the clock allows;
                // after that the others have to play.
                let spent = self.play.tries[star] >= 3;
                if h == star && !spent {
                    self.drive(h, 0.0)
                } else if h == star {
                    self.swing(h)
                } else if !spent && self.outside(star) && self.pass_risk(h, star) < 0.09 && r < 0.85
                {
                    self.pass(h, star, false)
                } else if r < 0.5 {
                    self.drive(h, 0.0)
                } else {
                    self.swing(h)
                }
            }
        }
    }

    // ---- passing ---------------------------------------------------------

    /// Swing the ball to a neighbour, then cut through and trade places with
    /// the far side, the way a motion offense keeps the floor moving.
    fn swing(&mut self, h: usize) -> Option<End> {
        let p = self.op(h);
        let mut weights = [0.0f32; 5];
        for (j, w) in weights.iter_mut().enumerate() {
            let pj = self.op(j);
            if j == h || pj.dist(HOOP) < 4.5 || pj.y > 12.0 {
                continue;
            }
            *w = (1.0 - self.pass_risk(h, j) * 7.0).max(0.03) / (1.0 + p.dist(pj));
            if self.play.from == Some(j) {
                *w *= 0.4;
            }
        }
        if weights.iter().sum::<f32>() <= 0.0 {
            return self.drive(h, 0.0);
        }
        let j = self.rng.weighted(&weights);

        // Cut to the basket and fill the far spot; the far man lifts.
        let pj = self.op(j);
        let far = (0..5)
            .filter(|&k| k != h && k != j && self.play.spots[k].dist(HOOP) > 4.5)
            .max_by(|&a, &b| {
                self.play.spots[a]
                    .dist(pj)
                    .total_cmp(&self.play.spots[b].dist(pj))
            });
        // Only the motion offenses cut after every pass; in a set built
        // around one man the others hold their spacing.
        let cuts = matches!(
            self.off().tactics.offense,
            OffenseSet::Motion | OffenseSet::FiveOut
        );
        if let Some(far) = far
            && cuts
            && self.play.spots[h].dist(HOOP) > 4.5
        {
            self.play.spots.swap(h, far);
            let side = if p.x >= 0.0 { 0.9 } else { -0.9 };
            self.play.task[h] = Task::Cut(HOOP + v(side, 1.3));
            // The exchange doubles as a screen away from the ball. A
            // switching defense just trades men; everyone else can get hung
            // up on it.
            let scheme = self.scheme();
            if !is_zone(scheme)
                && scheme != DefenseScheme::SwitchEverything
                && self.rng.chance(0.45)
            {
                let dk = self.defender_of(far);
                let chase = self.de(dk, Attr::PerimeterD) + self.de(dk, Attr::Iq);
                self.play.stun[dk] = (0.75 - chase * 0.015).max(0.2);
            }
        }
        self.pass(h, j, false)
    }

    fn pass(&mut self, h: usize, j: usize, safe: bool) -> Option<End> {
        let (a, b) = (self.op(h), self.op(j));
        if let Some(rec) = &mut self.rec {
            rec.passes.push(crate::replay::PassRec {
                time: self.wall,
                side: self.play.o as u8,
                from: h as u8,
                to: j as u8,
                at: crate::replay::to_court(a, self.play.o),
                inbound: self.play.inbound,
            });
        }
        if !safe {
            let risk = self.pass_risk(h, j);
            if self.rng.chance(risk) {
                // Whoever was closest to the lane gets a hand on it.
                let mut thief = (0, f32::MAX);
                for dk in 0..5 {
                    let q = self.dp(dk);
                    let off_lane = q.dist(q.project(a, b));
                    if off_lane < thief.1 {
                        thief = (dk, off_lane);
                    }
                }
                let end = self.turnover(h, TurnoverKind::BadPass, Some(thief.0), 0.62);
                if let Some(End::Live(dk)) = end {
                    // The ball goes to his hand and he takes it from there.
                    self.play.flight = Some(Flight {
                        from: a,
                        to: self.dp(dk),
                        t: 0.0,
                        dur: Self::pass_time(a.dist(self.dp(dk))).min(0.5),
                        kind: FlightKind::Steal { to: dk },
                        path: None,
                    });
                    self.play.holder = None;
                    return None;
                }
                return end;
            }
            self.lapses();
        }
        self.play.flight = Some(Flight {
            from: a,
            to: b,
            t: 0.0,
            // Long passes have to be lobbed over the defense.
            dur: Self::pass_time(a.dist(b)),
            kind: FlightKind::Pass { from: h, to: j },
            path: None,
        });
        self.play.holder = None;
        if !matches!(self.play.phase, Phase::Advance) {
            self.play.phase = Phase::Set;
        }
        None
    }

    /// Every pass and drive is a chance for a defender to switch off.
    fn lapses(&mut self) {
        let side = self.def();
        let mut hit = [0.0f32; 5];
        for (dk, h) in hit.iter_mut().enumerate() {
            let slot = &side.slots[side.on[dk]];
            *h = 0.010
                + (20.0 - side.eff(side.on[dk], Attr::Iq)).max(0.0) * 0.0022
                + side.unfamiliar * 0.07
                + slot.fatigue * 0.03
                - slot.focus * 0.006;
        }
        for (dk, p) in hit.into_iter().enumerate() {
            if self.rng.chance(p.max(0.002)) {
                self.play.stun[dk] = 0.5 + self.rng.f32() * 0.5;
                self.play.lapse_at[dk] = self.play.t;
            }
        }
    }

    fn turnover(
        &mut self,
        h: usize,
        kind: TurnoverKind,
        thief: Option<usize>,
        live: f32,
    ) -> Option<End> {
        self.oline(h).tov += 1;
        let who = self.oid(h);
        let stolen = thief.filter(|_| self.rng.chance(live));
        if let Some(dk) = stolen {
            self.dline(dk).stl += 1;
        }
        let stolen_by = stolen.map(|dk| self.did(dk));
        self.lost_by = stolen.map(|_| h);
        self.event(EventKind::Turnover {
            who,
            kind,
            stolen_by,
        });
        match stolen {
            Some(dk) => Some(End::Live(dk)),
            None => Some(End::Dead),
        }
    }

    // ---- off the dribble -------------------------------------------------

    fn drive(&mut self, h: usize, bonus: f32) -> Option<End> {
        self.play.shoulder = None;
        self.play.hit = None;
        self.play.hunt = None;
        let dk = self.defender_of(h);
        let (p, q) = (self.op(h), self.dp(dk));
        let aggression = self.def().aggression();
        let def = self.def();
        let slot = &def.slots[def.on[dk]];
        let temper = (1.5 - slot.p.personality.temperament as f32 / 20.0)
            * (1.0 + 0.4 * slot.drive.max(0.0) - 0.3 * slot.focus.max(0.0));

        let r = self.rng.f32();
        let strip = 0.015
            * (self.de(dk, Attr::Steal) / 10.0)
            * (1.0 + 0.6 * aggression)
            * (1.6 - self.oe(h, Attr::Handling) / 20.0 * 1.2);
        // Hand-checking and reaching: closer, more of it; the handler who
        // protects the ball and gets himself fouled draws more.
        let closeness = (1.0 + (1.3 - q.dist(p)) * 0.5).clamp(0.4, 1.5);
        let protect = 1.4 - self.oe(h, Attr::Handling) / 20.0 * 0.8;
        let foul = 0.045
            * (1.0 + 0.8 * aggression)
            * temper
            * closeness
            * protect
            * self.def().foul_mult(self.def().on[dk])
            * self.off().draw_mult(self.off().on[h])
            / CALL_BASE;
        let guarded = q.dist(p) < 2.6;
        if guarded {
            if r < strip {
                return self.turnover(h, TurnoverKind::Stripped, Some(dk), 0.7);
            }
            if r < strip + foul && self.called(1.0) {
                return self.floor_foul(dk, h, FoulKind::ReachIn);
            }
        }

        // A defender flying out at the shooter is easy to go past.
        // It has to be a real close-out: a long run at speed, not a denial
        // defender stepping up a metre.
        let closing = self.m[1 - self.play.o][dk].vel.dot((p - q).norm());
        let closeout =
            ((closing - 1.5) / 2.0).clamp(0.0, 1.0) * ((q.dist(p) - 1.0) / 1.5).clamp(0.0, 1.0);
        let attack = 0.45 * self.oe(h, Attr::Speed)
            + 0.35 * self.oe(h, Attr::Handling)
            + 0.2 * self.oe(h, Attr::Strength)
            + bonus
            + closeout * 5.0
            // A defender giving a cushion is hard to get past (but is not
            // contesting the pull-up).
            - (1.0 - closeout) * (q.dist(p) - 1.3).max(0.0) * 2.5;
        let contain = 0.5 * self.de(dk, Attr::PerimeterD)
            + 0.3 * self.de(dk, Attr::Speed)
            + 0.2 * self.de(dk, Attr::Strength);
        // Defenders sitting in the gaps either side of the lane make the
        // first step much harder ("positioning is your help").
        let lane = p.toward(HOOP, p.dist(HOOP).min(4.0));
        let mut gaps = 0.0;
        for other in 0..5 {
            if other == dk {
                continue;
            }
            let g = self.dp(other);
            let on = g.project(p, lane);
            if on.dist(p) > 0.6 {
                gaps += (1.0 - g.dist(on) / 2.2).max(0.0);
            }
        }
        // Going at a set defender a second time, he has seen the move.
        let again = 0.05 * self.play.tries[h].min(3) as f32;
        self.play.tries[h] += 1;
        let p_beat = (0.33 + (attack - contain) * 0.035 + aggression * 0.05 - gaps * 0.07 - again)
            .clamp(0.06, 0.85);
        // A sagging defender is not beaten, he is waiting further back.
        let contained = std::mem::take(&mut self.play.contained);
        let beaten = !contained && (!guarded || self.rng.chance(p_beat));
        if beaten && guarded {
            // Beaten defenders reach.
            if self.rng.chance(foul.min(0.95)) && self.called(1.0) {
                return self.floor_foul(dk, h, FoulKind::ReachIn);
            }
            self.play.stun[dk] = self.play.stun[dk].max(0.4);
            self.play.blown = Some((dk, self.play.t));
            let (who, defender) = (self.oid(h), self.did(dk));
            self.event(EventKind::Beaten { who, defender });
            self.hunt_charge(h, dk);
        }
        let until = self.play.t
            + if beaten {
                5.0
            } else {
                // Sizing him up, a move, a counter: it eats clock.
                1.4 + self.rng.f32() * 0.8
            };
        self.play.late_help = None;
        self.play.drive_class = 0;
        self.play.phase = Phase::Drive { dk, beaten, until };
        if self.play.source != Source::Transition || !beaten {
            self.play.source = if beaten {
                Source::Drive
            } else {
                Source::PullUp
            };
        }
        self.play.on_ball = Some(dk);
        self.play.dribbled = true;
        self.play.moves += 1;
        if self
            .play
            .assist
            .is_some_and(|(_, at)| self.play.t - at > 1.2)
        {
            self.play.assist = None;
        }
        self.lapses();
        None
    }

    fn driving(&mut self, h: usize, dk: usize, beaten: bool, until: f32) -> Option<End> {
        if let Some(end) = self.drive_contact(h) {
            return end;
        }
        let p = self.op(h);
        if p.dist(HOOP) < 1.5 {
            return self.gather(0.12, false);
        }
        if !beaten {
            if self.play.t >= until {
                self.play.phase = Phase::Set;
                self.play.think = 0.15;
                // A contained dribbler steps back or stops short for room, so
                // his jumper is less crowded than the body on him suggests.
                self.play.edge = PULL_UP_EDGE;
            }
            return None;
        }
        // Is somebody standing in the way? (In the open floor he goes round.)
        let dir = (HOOP - p).norm();
        for other in 0..5 {
            if p.y > 9.5 {
                break;
            }
            // A slower defender takes longer to get back in front of him.
            let grace = BEATEN_GRACE + (10.0 - self.de(dk, Attr::Speed)) * GRACE_SPEED;
            // In a 2-3 the man a drive beats is still part of the wall in the
            // lane, so nobody gets a free run past him.
            let past = self.scheme() != DefenseScheme::Zone23
                && self
                    .play
                    .blown
                    .is_some_and(|(b, at)| b == dk && self.play.t - at < grace);
            if other == dk && (self.play.stun[dk] > 0.0 || past) {
                continue;
            }
            // He does not see the man planted in his way.
            if self
                .play
                .hunt
                .is_some_and(|(hk, _, blind)| blind && hk == other)
            {
                continue;
            }
            let rel = self.dp(other) - p;
            let ahead = rel.dot(dir);
            let aside = (rel - dir * ahead).len();
            if ahead > 0.0 && ahead < 1.5 && aside < 0.9 {
                // A body that is set in the lane stops the drive. One
                // still running at the driver has come too late to stand
                // in front of him: they meet at speed instead.
                let speed = self.m[1 - self.play.o][other].vel.len();
                if speed < HELP_SET_SPEED {
                    if let Some(rec) = &mut self.rec {
                        rec.help_stops += 1;
                    }
                    self.play.drive_class = 2;
                    return self.drive_end(h);
                }
                if self.play.late_help != Some(other) {
                    if let Some(rec) = &mut self.rec {
                        rec.late_help += 1;
                    }
                    let sight = (SIGHT_BASE + self.oe(h, Attr::Iq) * SIGHT_IQ).min(0.97);
                    self.play.late_seen =
                        self.play.source != Source::Transition && self.rng.chance(sight);
                }
                self.play.late_help = Some(other);
                if self.play.late_seen {
                    self.play.drive_class = 2;
                    return self.drive_end(h);
                }
                self.play.drive_class = self.play.drive_class.max(1);
            }
        }
        if self.play.t >= until {
            return self.drive_end(h);
        }
        None
    }

    /// The drive has run into a defender: finish, pull up, or find the man
    /// the help came from.
    fn drive_end(&mut self, h: usize) -> Option<End> {
        let p = self.op(h);
        let left = self.play.shot_clock.min(self.clock);
        let crowd = (0..5).filter(|&dk| self.dp(dk).dist(p) < 1.7).count() as f32;
        let loose = (0.014 + 0.03 * (crowd - 1.0).max(0.0))
            * (1.6 - self.oe(h, Attr::Handling) / 20.0 * 1.2)
            * (1.0 + 0.5 * self.def().aggression());
        if self.rng.chance(loose) {
            let (dk, _) = self.nearest_defender(p);
            return self.turnover(h, TurnoverKind::Stripped, Some(dk), 0.7);
        }

        let (kind, c, near) = self.look(h);
        let value = self.ev(h, kind, c, near, true);
        let vision = 0.35 + (self.oe(h, Attr::Passing) + self.oe(h, Attr::Iq)) / 40.0 * 0.65;
        if self.rng.chance(vision)
            && let Some(j) = self.best_pass(h, value + 0.2, left)
        {
            return self.pass(h, j, false);
        }
        // Deep in the paint he is committed: layup or floater.
        if p.dist(HOOP) < COMMIT_DIST || value > self.threshold(h, left) - 0.2 {
            return self.gather(0.18, true);
        }
        self.play.phase = if p.y > 10.6 {
            Phase::Advance
        } else {
            Phase::Set
        };
        self.play.think = 0.3;
        None
    }

    fn ball_screen(&mut self, h: usize) -> Option<End> {
        let s = if self.play.big != h {
            self.play.big
        } else {
            (0..5).rev().find(|&k| k != h).unwrap_or(0)
        };
        self.play.on_ball = Some(self.defender_of(h));
        self.play.task[s] = Task::SetScreen;
        self.play.phase = Phase::Screen {
            s,
            until: self.play.t + 2.4,
        };
        None
    }

    /// How much body the screener brings (1 for an average big) and how
    /// long the screen holds the ball handler's defender, from where the
    /// bodies are.
    fn screen_contact(&self, h: usize, s: usize, arrived: bool) -> (f32, f32) {
        let bulk = self.bulks();
        let o = self.play.o;
        let dk = self.defender_of(h);
        let (si, di, hi) = (o * 5 + s, (1 - o) * 5 + dk, o * 5 + h);
        let p = self.op(h);
        let input = contact::Screen {
            handler: p,
            screener: self.op(s),
            defender: self.dp(dk),
            toward: p.toward(HOOP, 2.5),
            r_handler: bulk[hi].r,
            r_screener: bulk[si].r,
            r_defender: bulk[di].r,
            anchor_screener: bulk[si].anchor,
            fight: 0.35 * self.de(dk, Attr::Strength)
                + 0.25 * self.de(dk, Attr::Speed)
                + 0.2 * self.de(dk, Attr::Iq)
                + 0.2 * self.de(dk, Attr::PerimeterD),
        };
        let weight = (bulk[si].anchor / 90.0) * (bulk[si].r / 0.27);
        let delay = if arrived {
            contact::screen_delay(&input)
        } else {
            0.1
        };
        (weight, delay)
    }

    fn resolve_screen(&mut self, h: usize, s: usize, arrived: bool) -> Option<End> {
        let (screener, handler) = (self.oid(s), self.oid(h));
        self.event(EventKind::Screen { screener, handler });
        let (bulk, delay) = self.screen_contact(h, s, arrived);
        // A screener who is still moving, or leans into the man, has not
        // given him room to stop. Smart, strong screeners set legal ones.
        if arrived && !is_zone(self.scheme()) && self.scheme() != DefenseScheme::SwitchEverything {
            let dk = self.defender_of(h);
            let s_speed = self.m[self.play.o][s].vel.len();
            let d_speed = self.m[1 - self.play.o][dk].vel.len();
            let raw = SCREEN_ILLEGAL
                * (1.0 + 0.8 * (s_speed - 0.3).max(0.0))
                * (1.0 + 0.25 * (d_speed - 1.0).max(0.0))
                * self.off().foul_mult(self.off().on[s])
                / CALL_BASE;
            if self.rng.chance(raw.min(0.9)) && self.called(1.0) {
                return self.offensive_foul(s, dk, FoulKind::IllegalScreen);
            }
            // Fighting through it, a man grabs.
            if delay > 0.0
                && self.rng.chance(
                    (SCREEN_HOLD * self.def().foul_mult(self.def().on[dk]) / CALL_BASE).min(0.9),
                )
                && self.called(1.0)
            {
                return self.floor_foul(dk, s, FoulKind::Holding);
            }
        }
        let quality = if arrived {
            1.5 + (self.oe(s, Attr::Strength) - 10.0) * 0.1 + (bulk - 1.0)
        } else {
            0.5
        };
        let scheme = self.scheme();
        let mut went_under = false;
        let bonus = match scheme {
            DefenseScheme::SwitchEverything => {
                let (a, b) = (self.defender_of(h), self.defender_of(s));
                self.play.guard.swap(a, b);
                self.bulk_dirty = true;
                let onto = self.defender_of(h);
                let (off, def) = (self.off(), self.def());
                let mismatch = def.slots[def.on[onto]].p.body.height_cm
                    - off.slots[off.on[h]].p.body.height_cm;
                let onto = self.did(onto);
                self.event(EventKind::Switch {
                    onto,
                    handler,
                    mismatch: mismatch as i16,
                });
                0.0
            }
            DefenseScheme::ManToMan | DefenseScheme::PackLine => {
                let dk = self.defender_of(h);
                let big = self.defender_of(s);
                let under = self.goes_under(h, dk);
                if let Some(rec) = &mut self.rec {
                    rec.screens.push((self.wall, under));
                }
                let o = self.play.o;
                let bulks = self.bulks();
                let (rs, rh) = (bulks[o * 5 + s].r, bulks[o * 5 + h].r);
                let ps = self.op(s);
                // The handler's way off the screen: past the shoulder of the
                // screener that is away from the man who is guarding him.
                let away = (ps - self.dp(dk)).norm();
                let shoulder = ps + away * (rs + rh + 0.1);
                self.play.drop = Some(big);
                if under {
                    // Behind the screener and out in front of the handler's
                    // line to the basket: the shot is his, the lane is not.
                    // A wide, heavy screener makes the way round longer.
                    let gap = UNDER_GAP + UNDER_BULK * (bulk - 1.0);
                    let spot = shoulder.toward(HOOP, gap.min(shoulder.dist(HOOP)));
                    self.play.route = Some((dk, spot, self.play.t + UNDER_TIME));
                    self.play.contained = true;
                    went_under = true;
                    // The screener's man sinks to the paint.
                    self.play.hedge_until = 0.0;
                    // The lane is closed: a drive here is a contained one.
                    -2.0
                } else {
                    // Over the top: held on the screener's body for as long
                    // as it takes to get through, so the handler has a step,
                    // and the big shows himself to slow him down.
                    self.play.stun[dk] += delay;
                    if delay > 0.0 {
                        // Held up on the screener's body: he stops where he is.
                        let m = &mut self.m[1 - self.play.o][dk];
                        m.target = m.pos;
                    }
                    if scheme == DefenseScheme::ManToMan {
                        self.play.hedge_until = self.play.t + HEDGE_TIME;
                    }
                    // Every tenth of a second he loses is a step for the handler.
                    quality * 1.3 + 1.5 + (delay - 0.3) * 12.0
                }
            }
            // A zone does not chase over screens; the next man is there.
            DefenseScheme::Zone23 | DefenseScheme::Zone32 => quality * 0.4,
        };
        let ps = self.op(s);
        let side = if ps.x >= 0.0 { 1.0 } else { -1.0 };
        self.play.task[s] = match scheme {
            // Nothing to roll into against a switch: the defender is already
            // on his hip. Take the smaller man down to the block instead.
            DefenseScheme::SwitchEverything => {
                let dk = self.defender_of(s);
                let (off, def) = (self.off(), self.def());
                let taller =
                    off.slots[off.on[s]].p.body.height_cm - def.slots[def.on[dk]].p.body.height_cm;
                if taller >= 6.0 {
                    self.play.spots[s] = v(side * 2.7, 2.9);
                    Task::Cut(v(side * 2.7, 2.9))
                } else {
                    Task::Spot
                }
            }
            // The paint is full against a zone: short roll to the high post.
            DefenseScheme::Zone23 | DefenseScheme::Zone32 => Task::Pop(v(side * 0.6, 5.2)),
            // A big who can shoot pops; the rest dive to the rim.
            _ if self.oe(s, Attr::ThreePoint) >= 12.5 && self.rng.chance(0.65) => {
                Task::Pop(v(side * 3.3, 8.2))
            }
            _ => Task::Roll,
        };
        self.play.roller = (self.play.task[s] == Task::Roll).then_some(s);
        let end = self.drive(h, bonus);
        // With the shot there for the taking he does not wait long.
        if went_under && let Phase::Drive { dk, beaten, .. } = self.play.phase {
            self.play.phase = Phase::Drive {
                dk,
                beaten,
                until: self.play.t + UNDER_PULL,
            };
        }
        // Off the screener's shoulder, whichever way the defender went.
        if matches!(scheme, DefenseScheme::ManToMan | DefenseScheme::PackLine) {
            let o = self.play.o;
            let bulks = self.bulks();
            let (rs, rh) = (bulks[o * 5 + s].r, bulks[o * 5 + h].r);
            let dk = self.defender_of(h);
            let away = (self.op(s) - self.dp(dk)).norm();
            self.play.shoulder = Some(self.op(s) + away * (rs + rh + 0.1));
        }
        end
    }

    /// Does the man guarding the ball handler go under the screen or fight
    /// over it? Fought over for shooters and by aggressive, quick, strong
    /// defenders; a pack line sags and goes under.
    fn goes_under(&mut self, h: usize, dk: usize) -> bool {
        let pack = self.scheme() == DefenseScheme::PackLine;
        let threat = (self.oe(h, Attr::ThreePoint) - 10.0) * UNDER_THREAT;
        let skill = (self.de(dk, Attr::Speed) + self.de(dk, Attr::Strength) - 20.0) * UNDER_SKILL;
        let p = UNDER_BASE + if pack { UNDER_PACK } else { 0.0 }
            - threat
            - self.def().aggression() * UNDER_AGGRESSION
            - skill;
        self.rng.chance(p.clamp(0.05, 0.95))
    }

    // ---- with his back to the basket -------------------------------------

    fn post_up(&mut self, h: usize) -> Option<End> {
        let dk = self.defender_of(h);
        let (who, defender) = (self.oid(h), self.did(dk));
        self.event(EventKind::PostUp { who, defender });
        self.play.post_from = self.op(h);
        self.play.phase = Phase::Post {
            dk,
            until: self.play.t + 1.3 + self.rng.f32() * 0.8,
        };
        self.play.on_ball = Some(dk);
        self.play.source = Source::Post;
        self.play.dribbled = true;
        self.lapses();
        None
    }

    /// How much heavier (as a fraction, -0.5..=1) the man with the ball is
    /// than the one guarding him, as the shoving match sees it.
    fn mass_edge(&self, h: usize, dk: usize) -> f32 {
        let o = self.play.o;
        (self.bulk_base[o * 5 + h].anchor / self.bulk_base[(1 - o) * 5 + dk].anchor - 1.0)
            .clamp(-0.5, 1.0)
    }

    fn post_move(&mut self, h: usize, dk: usize) -> Option<End> {
        let p = self.op(h);
        let left = self.play.shot_clock.min(self.clock);
        // Hands from the perimeter digging at the ball.
        let mut strip = 0.012 * self.de(dk, Attr::Steal) / 10.0;
        let mut digger = dk;
        for other in 0..5 {
            if other != dk && self.dp(other).dist(p) < 2.2 {
                strip += 0.022 * self.de(other, Attr::Steal) / 10.0;
                digger = other;
            }
        }
        let r = self.rng.f32();
        if r < strip {
            return self.turnover(h, TurnoverKind::Stripped, Some(digger), 0.7);
        }
        // Ground taken on the way: a man who has pushed his defender a
        // metre closer to the basket has the better of the move.
        let gained = (self.play.post_from.dist(HOOP) - p.dist(HOOP)).clamp(-0.5, 1.8);
        let mult = self.def().foul_mult(self.def().on[dk]);
        if r < strip + 0.07 * (1.0 + 0.6 * self.def().aggression()) * mult / CALL_BASE
            && self.called(1.0)
        {
            return self.floor_foul(dk, h, FoulKind::Pushing);
        }
        // Moving a man who had his position is a foul, the more so the
        // further he was moved.
        let shove = POST_SHOVE + POST_SHOVE_RATE * (gained - POST_PAR).clamp(0.0, 1.1);
        if self.rng.chance(shove.min(0.9) / CALL_BASE) && self.called(1.0) {
            return self.offensive_foul(h, dk, FoulKind::PostPushOff);
        }
        // Travels and three seconds.
        if self
            .rng
            .chance(0.045 * 0.7 - self.oe(h, Attr::PostMoves) * 0.0015 * 0.7)
        {
            return self.turnover(h, TurnoverKind::Stripped, None, 0.0);
        }
        let (off, def) = (self.off(), self.def());
        let size = (off.slots[off.on[h]].p.body.height_cm - def.slots[def.on[dk]].p.body.height_cm)
            .clamp(-25.0, 25.0);
        let attack = 0.55 * self.oe(h, Attr::PostMoves)
            + 0.25 * self.oe(h, Attr::Strength)
            + 0.2 * (10.0 + size * 0.3);
        let hold = 0.6 * self.de(dk, Attr::InteriorD) + 0.4 * self.de(dk, Attr::Strength);
        let won = self.rng.chance(
            (POST_BASE
                + (attack - hold) * 0.05
                + (gained - POST_PAR) * POST_GROUND
                + POST_MASS * self.mass_edge(h, dk))
            .clamp(0.08, 0.88),
        );
        if let Some(rec) = &mut self.rec {
            rec.post[0] += 1.0;
            rec.post[1] += gained;
            rec.post[2] += won as u8 as f32;
        }
        if won {
            self.play.stun[dk] = 0.5;
            self.play.blown = Some((dk, self.play.t));
        }
        // Ground taken is room to shoot: more of it, the less the shot is crowded.
        self.play.edge = (if won { 0.6 } else { 1.0 }
            - POST_ROOM * (gained - POST_PAR).clamp(-0.6, 1.2))
        .clamp(0.3, 1.2);
        let (kind, c, near) = self.look(h);
        let value = self.ev(h, kind, c * self.play.edge, near, false);
        let vision = 0.3 + (self.oe(h, Attr::Passing) + self.oe(h, Attr::Iq)) / 40.0 * 0.6;
        if self.rng.chance(vision)
            && let Some(j) = self.best_pass(h, value + 0.05, left + 6.0)
        {
            self.play.edge = 1.0;
            return self.pass(h, j, false);
        }
        if won || value > self.threshold(h, left) - 0.1 {
            return self.gather(0.25, false);
        }
        self.play.phase = Phase::Set;
        self.play.think = 0.3;
        None
    }

    // ---- shooting --------------------------------------------------------

    /// Send player `k` of side `t` up for `dur` seconds starting `after`
    /// seconds from now, to a fraction `frac` of the best his Vertical
    /// allows. Only for the replay.
    fn leap(&mut self, t: usize, k: usize, action: Action, frac: f32, after: f32, dur: f32) {
        let side = &self.sides[t];
        let vertical = side.slots[side.on[k]].p.attrs[Attr::Vertical];
        self.jump[t][k] = Jump {
            action,
            start: self.anim + after,
            dur,
            peak: Pose::max_lift(vertical) * frac.min(1.0),
        };
    }

    fn gather(&mut self, delay: f32, off_dribble: bool) -> Option<End> {
        if let Some(h) = self.play.holder {
            self.plan_shot(h, delay, false);
        }
        let at = self
            .play
            .shot
            .map_or(self.play.t + delay, |plan| plan.release);
        self.play.phase = Phase::Gather { at, off_dribble };
        None
    }

    /// The timeline of a shot: stopping, the dip, the jump, the release at
    /// the top of it, the landing and the follow-through. `delay` is the
    /// least time to release; `free_throw` is a standing shot at the line.
    fn plan_shot(&mut self, h: usize, delay: f32, free_throw: bool) {
        let o = self.play.o;
        let p = self.op(h);
        let d = p.dist(HOOP);
        let kind = ShotKind::at(p);
        let (reach, max_lift, iq) = {
            let side = &self.sides[o];
            let slot = side.on[h];
            let pl = &side.slots[slot].p;
            (
                pl.body.standing_reach_cm / 100.0,
                Pose::max_lift(pl.attrs[Attr::Vertical]),
                side.eff(slot, Attr::Iq),
            )
        };
        let mut style = if free_throw {
            Style::Jumper
        } else if d < 2.2 {
            Style::Layup
        } else if self.play.source == Source::Post {
            Style::Hook
        } else {
            Style::Jumper
        };
        if style == Style::Layup && reach + max_lift >= RIM_HEIGHT + DUNK_CLEAR {
            let (_, c, _) = self.look(h);
            if c < DUNK_TRAFFIC {
                style = Style::Dunk;
            }
        }
        let frac = match style {
            _ if free_throw => 0.2,
            Style::Dunk => 1.0,
            Style::Layup => LIFT_LAYUP,
            Style::Hook | Style::Fadeaway => 0.4,
            Style::Jumper => {
                if kind == ShotKind::Three {
                    LIFT_JUMPER - 0.05
                } else {
                    LIFT_JUMPER + 0.05
                }
            }
        };
        let peak = max_lift * frac;
        let rise = (2.0 * peak / GRAVITY).sqrt();
        let t0 = self.play.t;
        let speed = self.m[o][h].vel.len();
        let lead = match style {
            Style::Layup | Style::Dunk => STEP_IN,
            _ => {
                (speed / (ACCEL * BRAKE)).min(0.4) + DIP + if free_throw { FT_ROUTINE } else { 0.0 }
            }
        };
        let release = (t0 + lead + rise).max(t0 + delay);
        let jump_at = release - rise;
        let land = jump_at + 2.0 * rise;
        // The alert ones turn and go sooner than the ones who stay to look.
        let watch = (0.9 - 0.035 * iq).clamp(0.25, 0.9);
        let hand_z = match style {
            Style::Layup | Style::Dunk => reach + peak,
            _ => reach - HAND_BELOW_REACH + peak,
        };
        self.play.shot = Some(ShotPlan {
            shooter: h,
            style,
            start: t0,
            jump_at,
            release,
            land,
            follow_end: land + watch,
            peak,
            hand_z,
        });
        if !free_throw {
            // Hands go up with the shot, each timed to top out near the
            // release: how well, and how high, is the defender's blocking.
            let p = self.op(h);
            let w = self.contest_each(p, h, ShotKind::at(p) == ShotKind::Rim);
            for (dk, &wk) in w.iter().enumerate() {
                if wk <= 0.3 {
                    continue;
                }
                let (vertical, block) = {
                    let side = self.def();
                    (
                        side.slots[side.on[dk]].p.attrs[Attr::Vertical],
                        side.eff(side.on[dk], Attr::Block),
                    )
                };
                let frac = (0.5 + 0.025 * block).clamp(0.5, 1.0);
                let jitter = (0.16 - 0.006 * block).clamp(0.04, 0.18);
                let late = shot::gauss(&mut self.rng) * jitter;
                let air = 2.0 * (2.0 * Pose::max_lift(vertical) * frac / GRAVITY).sqrt();
                let after = (release - t0 - air / 2.0 + late).max(0.0);
                self.leap(1 - o, dk, Action::Contesting, frac, after, air);
            }
        }
    }

    /// What is settled as he leaves the floor: the contest on the shot, and
    /// whether a hand fouls him. Whether it goes in is not: that is up to
    /// the ball.
    fn verdict(&mut self, h: usize, off_dribble: bool) -> Verdict {
        let _ = off_dribble;
        let p = self.op(h);
        let (kind, c, near) = self.look(h);
        let edge = self.play.edge;
        let c = c * edge;
        self.play.edge = 1.0;
        // Contact on the shooter: more of it from a defender moving into
        // him than from one who goes straight up, from a man who cannot
        // keep his hands off, and for a shooter who goes looking for it.
        // Whether it is called is up to the crew; a shooter who was hit
        // and not given the call still feels it.
        let contact = near.is_some_and(|dk| {
            let raw = self.foul_chance(kind, c)
                * self.def().foul_mult(self.def().on[dk])
                * self.off().draw_mult(self.off().on[h])
                * self.verticality(dk, p)
                / CALL_BASE;
            self.rng.chance(raw.min(0.95))
        });
        let fouled = contact && self.called(1.0);
        let assist = self
            .play
            .assist
            .filter(|&(_, at)| self.play.t - at < 3.0)
            .map(|(k, _)| k);
        Verdict {
            kind,
            c,
            edge,
            near,
            contact,
            fouled,
            assist,
        }
    }

    /// The error in a launch: how far the ball is put off line and off the
    /// speed, one standard deviation at the ring. Skill shrinks it; distance,
    /// a man on the move, a hand in his face (how close, how long, and
    /// whether he is up), a poor look, an unfamiliar set and being hit all
    /// grow it.
    #[allow(clippy::too_many_arguments)]
    fn shot_errors(
        &self,
        h: usize,
        style: Style,
        kind: ShotKind,
        d: f32,
        c: f32,
        near: Option<usize>,
        off_dribble: bool,
        verdict: &Verdict,
        reach_edge: f32,
    ) -> Errors {
        let rim = matches!(style, Style::Layup | Style::Dunk) || kind == ShotKind::Rim;
        let (lat, depth) = shot::base(style, rim, d);
        let comp = self.oe(h, Attr::Composure) - 10.0;
        let mut m = if style == Style::Dunk {
            // The hand carries it through: only a body in the way, or the
            // top of his reach, spoils it.
            1.0 + DUNK_CONTEST * c + DUNK_EDGE * (0.15 - reach_edge).max(0.0)
        } else if rim {
            let finish = 0.6 * self.oe(h, Attr::CloseShot)
                + 0.2 * self.oe(h, Attr::Vertical)
                + 0.2 * self.oe(h, Attr::Strength);
            let protect = near.map_or(10.0, |dk| {
                0.5 * self.de(dk, Attr::InteriorD) + 0.5 * self.de(dk, Attr::Block)
            });
            (1.0 - FINISH_SLOPE * (finish - 10.0)).clamp(0.4, 2.0)
                * (1.0 + CONTEST_RIM * c * (1.0 + GUARD_SLOPE * (protect - 10.0)).max(0.3))
        } else {
            let touch = if kind == ShotKind::Three {
                self.oe(h, Attr::ThreePoint)
            } else if d < 4.0 {
                self.oe(h, Attr::MidRange).max(self.oe(h, Attr::PostMoves))
            } else {
                self.oe(h, Attr::MidRange)
            };
            let guard = near.map_or(10.0, |dk| self.de(dk, Attr::PerimeterD));
            let k = if kind == ShotKind::Three {
                CONTEST_THREE
            } else {
                CONTEST_MID + (4.0 - d).max(0.0) * 0.06
            };
            // Off balance: the speed he is going at as the ball leaves him.
            let speed = self.m[self.play.o][h].vel.len();
            (1.0 - SKILL_SLOPE * (touch - 10.0)).clamp(0.4, 2.0)
                * (1.0 + k * c * (1.0 + GUARD_SLOPE * (guard - 10.0)).max(0.3))
                * (1.0 + MOVING * speed)
                * if off_dribble { 1.0 + OFF_DRIBBLE } else { 1.0 }
        };
        m *= match (style, rim, kind) {
            (Style::Dunk, _, _) => 1.0,
            (_, true, _) => RIM_SCALE,
            (_, _, ShotKind::Three) => THREE_SCALE,
            _ => MID_SCALE,
        };
        m *= 1.0 - COMPOSURE * comp;
        m *= 1.0 + RHYTHM * self.off().unfamiliar;
        if self.play.o == 0 {
            m *= 1.0 - HOME;
        }
        // A hand on the arm or the body: whether or not it was called, he felt it.
        if verdict.fouled {
            m *= if rim { FOULED_ERR } else { FOULED_JUMPER };
        } else if verdict.contact {
            m *= CONTACT_ERR;
        }
        Errors::at_rim(d, lat * m * GAME, depth * m * GAME)
    }

    /// The arms of the defenders who could reach this ball as it leaves a
    /// shooter's hand: where he stands, how long his arm, and the jump he
    /// is in.
    fn block_hands(&self, shooter: Vec2) -> ([Hand; 5], usize) {
        let d = 1 - self.play.o;
        let mut out = [Hand {
            who: 0,
            pos: Vec2::ZERO,
            vel: Vec2::ZERO,
            shoulder: 0.0,
            arm: 0.0,
            up_at: 0.0,
            air: 0.0,
            peak: 0.0,
        }; 5];
        let mut n = 0;
        for dk in 0..5 {
            let m = &self.m[d][dk];
            if m.pos.dist(shooter) > 2.4 {
                continue;
            }
            // A man planted for a charge has his arms folded.
            if self.play.hunt.is_some_and(|(hk, _, _)| hk == dk) {
                continue;
            }
            let reach = self.sides[d].slots[self.sides[d].on[dk]].p.body.standing_reach_cm / 100.0;
            let arm = reach * ARM_SHARE;
            let j = &self.jump[d][dk];
            out[n] = Hand {
                who: dk,
                pos: m.pos,
                vel: m.vel,
                shoulder: reach - arm,
                arm: arm - BLOCK_SLACK,
                up_at: j.start - self.anim,
                air: j.dur,
                peak: j.peak,
            };
            n += 1;
        }
        (out, n)
    }

    fn shoot(&mut self, h: usize, off_dribble: bool) -> Option<End> {
        let p = self.op(h);
        let verdict = match self.play.verdict.take() {
            Some(v) => v,
            None => self.verdict(h, off_dribble),
        };
        let Verdict {
            kind,
            c: c_takeoff,
            near: near_takeoff,
            fouled,
            assist,
            ..
        } = verdict;
        // The flight: from the hand above his head.
        let mut plan = self
            .play
            .shot
            .filter(|s| s.shooter == h)
            .unwrap_or(ShotPlan {
                shooter: h,
                style: Style::Jumper,
                start: self.play.t,
                jump_at: self.play.t,
                release: self.play.t,
                land: self.play.t + 0.5,
                follow_end: self.play.t + 1.0,
                peak: 0.3,
                hand_z: 2.6,
            });
        let to_rim = p.dist(HOOP);
        if plan.style == Style::Dunk && to_rim > DUNK_RANGE {
            plan.style = Style::Layup;
        }
        self.play.shot = Some(plan);
        let style = plan.style;
        let toward = (HOOP - p).norm();
        let from = if style == Style::Dunk {
            p.toward(HOOP, to_rim.min(0.5))
        } else {
            p + toward * 0.22
        };
        let z0 = if style == Style::Dunk {
            plan.hand_z.max(RIM_HEIGHT + 0.2)
        } else {
            plan.hand_z
        };

        // How the contest stands as the ball leaves his hand: the men near
        // him as they are now, and how high their hands are.
        let rim_kind = kind == ShotKind::Rim;
        let mut w = self.contest_each(p, h, rim_kind);
        for (dk, w) in w.iter_mut().enumerate() {
            if *w > 0.0 {
                let up = self.lift_at(1 - self.play.o, dk, 0.0);
                let max = {
                    let d = self.def();
                    Pose::max_lift(d.slots[d.on[dk]].p.attrs[Attr::Vertical])
                };
                *w *= HAND_STANDING + (1.0 - HAND_STANDING) * (up / (0.5 * max)).min(1.0);
            }
        }
        let (c_now, near_now) = Self::sum_contest(w, rim_kind);
        let c = c_now * verdict.edge;
        let near = near_now.or(near_takeoff);
        let _ = c_takeoff;
        let reach_edge = {
            let pl = &self.off().slots[self.off().on[h]].p;
            pl.body.standing_reach_cm / 100.0 + plan.peak - (RIM_HEIGHT + DUNK_CLEAR)
        };
        let errors = self.shot_errors(
            h,
            style,
            kind,
            to_rim,
            c,
            near,
            off_dribble,
            &verdict,
            reach_edge,
        );
        let start = ball::clear_start(v3(from.x, from.y, z0));
        let (ideal, _) = shot::ideal(start, style);
        let launch = shot::perturb(ideal, errors, &mut self.rng);
        let (hands, nh) = if style == Style::Dunk {
            ([Hand::NONE; 5], 0)
        } else {
            self.block_hands(p)
        };
        let flown = ball::fly(Ball { p: start, v: launch }, &hands[..nh]);
        let made = flown.made;
        let blocker = flown.blocked.map(|(dk, _)| dk);

        if made || !fouled {
            let line = self.oline(h);
            line.fga += 1;
            if kind == ShotKind::Three {
                line.tpa += 1;
            }
        }
        let who = self.oid(h);
        // A shot in the first seconds of a possession that began on a live ball
        // is a transition shot, whether it came off a run or an early set.
        let source = if self.play.live_origin
            && self.play.t < BREAK_SECONDS
            && !matches!(self.play.source, Source::Putback | Source::Forced)
        {
            Source::Transition
        } else {
            self.play.source
        };
        let event = EventKind::Shot {
            who,
            kind,
            source,
            made,
            contest: c_takeoff,
            defender: near_takeoff.map(|dk| self.did(dk)),
            assist: assist.filter(|_| made).map(|k| self.oid(k)),
            blocked_by: blocker.map(|dk| self.did(dk)),
            fouled,
        };
        if self.play.source == Source::Drive
            && let Some(rec) = &mut self.rec
        {
            let d = &mut rec.drive[self.play.drive_class];
            d[0] += 1.0;
            d[1] += (kind == ShotKind::Rim) as u8 as f32;
            d[2] += c_takeoff;
            d[3] += if made { kind.points() as f32 } else { 0.0 };
        }
        self.event(event);
        if made {
            self.blame(h, kind, c_takeoff);
        }
        let mut foul_shots = 0;
        if fouled && let Some(dk) = near_takeoff {
            let shots = if made { 1 } else { kind.points() };
            self.defensive_foul(dk, h, shots as u8, FoulKind::Shooting);
            if !made {
                foul_shots = shots;
            }
        }
        if let Some(dk) = blocker {
            self.dline(dk).blk += 1;
            let d = 1 - self.play.o;
            let tb = flown.blocked.map_or(0.0, |b| b.1);
            let j = self.jump[d][dk];
            if j.dur > 0.0 && self.anim + tb >= j.start && self.anim + tb <= j.start + j.dur {
                self.jump[d][dk].action = Action::Blocking;
            } else {
                // He met it standing, arm up: shown as a reach, not a leap.
                self.leap(d, dk, Action::Blocking, 0.0, (tb - 0.2).max(0.0), 0.5);
            }
        }
        self.play.def_touch = blocker.is_some();
        self.play.tips = 0;

        let path = flown.path;
        self.log_path(&path, 0.0, 0.0);
        let (b, bz) = path.at(0.0);
        self.play.ball = b;
        self.play.ball_h = bz;
        let dur = if made {
            flown.ring_at
        } else {
            Self::live_time(&flown)
        };
        self.play.flight = Some(Flight {
            from: p,
            to: HOOP,
            t: 0.0,
            dur,
            kind: FlightKind::Shot {
                shooter: h,
                made,
                kind,
                assist: assist.filter(|_| made),
                and_one: fouled,
                foul_shots,
                line: false,
            },
            path: Some(path),
        });
        self.play.holder = None;
        self.play.phase = Phase::Air;
        self.play.boxout = foul_shots == 0;

        // Who goes to the glass, who gets back and who stays to watch it.
        let crash = match self.off().tactics.crash_boards {
            0..=5 => 1,
            6..=12 => 2,
            13..=17 => 3,
            _ => 4,
        };
        let mut order: Vec<usize> = (0..5)
            .filter(|&k| k != h || kind == ShotKind::Rim)
            .collect();
        order.sort_by(|&a, &b| self.op(a).dist(HOOP).total_cmp(&self.op(b).dist(HOOP)));
        let release = self.play.t;
        self.play.task = [Task::GetBack; 5];
        if foul_shots > 0 {
            self.play.task = [Task::Spot; 5];
        } else {
            // The man furthest from the basket, the safety, is already going.
            let safety = (0..5)
                .filter(|&k| k != h)
                .max_by(|&a, &b| self.op(a).y.total_cmp(&self.op(b).y));
            for k in 0..5 {
                if Some(k) == safety {
                    continue;
                }
                let iq = self.oe(k, Attr::Iq);
                self.play.task[k] = Task::Watch(release + (0.8 - 0.03 * iq).clamp(0.15, 0.8));
            }
            for &k in order.iter().take(crash) {
                self.play.task[k] = Task::Crash;
            }
        }
        // The shooter holds his follow-through and watches.
        self.play.task[h] = Task::Watch(plan.follow_end);
        None
    }

    /// Seconds into the flight at which the ball can be played: when it
    /// first touches iron, glass or a hand, or, if it touches nothing, when
    /// it is on its way down.
    fn live_time(f: &Flown) -> f32 {
        if let Some((t, _, _)) = f.touch {
            return t;
        }
        let total = f.path.total();
        let mut t = 0.1;
        while t < total {
            let (_, z, _, vz) = f.path.state(t);
            if vz < 0.0 && z < 4.2 {
                return t;
            }
            t += 0.02;
        }
        total
    }

    /// Pin a made basket on a defender when one was clearly at fault.
    fn blame(&mut self, h: usize, kind: ShotKind, c: f32) {
        let t = self.play.t;
        let dk = self.defender_of(h);
        let beneficiary = self.oid(h);
        let mistake = if c < 0.35 && t - self.play.lapse_at[dk] < 3.0 {
            Some((
                dk,
                if kind == ShotKind::Rim {
                    Mistake::LostCutter
                } else {
                    Mistake::BallWatching
                },
            ))
        } else if let Some((beaten, at)) = self.play.blown
            && kind == ShotKind::Rim
            && c < 0.3
            && t - at < 3.0
        {
            Some((beaten, Mistake::BlownBy))
        } else if kind == ShotKind::Three
            && c < 0.15
            && !is_zone(self.scheme())
            && self.dp(dk).dist(self.op(h)) < 4.5
        {
            Some((dk, Mistake::LateCloseout))
        } else {
            None
        };
        if let Some((dk, kind)) = mistake {
            let who = self.did(dk);
            self.event(EventKind::Mistake {
                who,
                kind,
                beneficiary,
            });
        }
    }

    fn land(&mut self, f: Flight) -> Option<End> {
        match f.kind {
            FlightKind::Pass { from, to } => {
                self.play.holder = Some(to);
                let near = self.op(to).dist(HOOP) < 2.6;
                self.play.source = match self.play.task[to] {
                    Task::Roll => Source::Roll,
                    _ if near && self.play.roller == Some(to) => Source::Roll,
                    Task::Cut(_) => Source::Cut,
                    _ if near => Source::Cut,
                    _ => Source::CatchAndShoot,
                };
                self.play.dribbled = false;
                self.play.moves = 0;
                // Throwing the ball in is not an assist.
                self.play.assist = if std::mem::take(&mut self.play.inbound) {
                    None
                } else {
                    Some((from, self.play.t))
                };
                self.play.from = Some(from);
                self.play.think =
                    0.3 + self.rng.f32() * 0.25 + self.off().unfamiliar * self.rng.f32() * 0.5;
                if matches!(self.play.phase, Phase::Advance) && self.op(to).y < 10.6 {
                    self.play.phase = Phase::Set;
                }
                if self.play.brk {
                    // Caught on the run: he looks up and goes on with it.
                    self.play.phase = Phase::Advance;
                    self.play.source = Source::Transition;
                    self.play.think = 0.05;
                }
                None
            }
            FlightKind::Steal { to } => Some(End::Live(to)),
            FlightKind::Shot {
                shooter,
                made,
                kind,
                assist,
                and_one,
                foul_shots,
                line,
            } => {
                let path = f.path.unwrap_or_else(Path::new);
                if line {
                    return self.land_free_throw(shooter, made, f.t, path);
                }
                if !made {
                    if foul_shots > 0 {
                        // A dead ball: it falls away and the shooter goes to the line.
                        self.after_path(
                            AfterKind::Fall {
                                shooter,
                                shots: foul_shots,
                            },
                            path,
                            f.t,
                        );
                        return None;
                    }
                    return self.go_live(path, f.t, false);
                }
                let points = kind.points();
                let line = self.oline(shooter);
                line.fgm += 1;
                line.pts += points;
                if kind == ShotKind::Three {
                    line.tpm += 1;
                }
                self.sides[self.play.o].score += points;
                if let Some(a) = assist {
                    self.oline(a).ast += 1;
                }
                self.swish = true;
                self.after_path(
                    AfterKind::Drop {
                        shooter,
                        then: and_one as u16,
                    },
                    path,
                    f.t,
                );
                None
            }
        }
    }

    /// Start the ball on the rest of its path.
    fn after_path(&mut self, kind: AfterKind, path: Path, t: f32) {
        let end = match kind {
            AfterKind::Drop { .. } | AfterKind::Fall { .. } => {
                path.first_floor().unwrap_or(path.total()) + 0.1
            }
            _ => path.total(),
        };
        self.play.after = Some(After { kind, path, t, end });
    }

    /// The ball has done what the result said; what comes next.
    fn settle(&mut self, a: After) -> Option<End> {
        match a.kind {
            AfterKind::Drop { shooter, then } => {
                if let Phase::Line { left, .. } = self.play.phase
                    && left > 0
                {
                    return self.send_back(shooter, a);
                }
                self.loose = Some((a.path, a.t));
                if then > 0 {
                    return self.free_throws(shooter, then, Some(a.path.at(a.t).0));
                }
                self.credit = 0.9;
                Some(End::Dead)
            }
            AfterKind::Fall { shooter, shots } => {
                self.loose = Some((a.path, a.t));
                if shots > 0 {
                    return self.free_throws(shooter, shots, Some(a.path.at(a.t).0));
                }
                self.send_back(shooter, a)
            }
            AfterKind::Live { .. } => None,
            AfterKind::Inbound { who } => {
                // Back from out of bounds to the attackers.
                self.play.holder = Some(who);
                self.play.ball = self.op(who);
                self.play.ball_h = a.path.end().1;
                self.play.phase = Phase::Set;
                self.play.think = 0.5;
                self.play.shot_clock = self.play.shot_clock.max(14.0);
                self.play.task = [Task::Spot; 5];
                self.play.dribbled = false;
                self.play.moves = 0;
                self.play.assist = None;
                self.play.from = None;
                self.play.boxout = false;
                None
            }
            AfterKind::Return { shooter } => {
                self.play.holder = Some(shooter);
                self.play.ball = self.op(shooter);
                self.play.ball_h = a.path.end().1;
                if let Phase::Line {
                    shooter,
                    left,
                    of,
                    made,
                    ..
                } = self.play.phase
                {
                    self.play.phase = Phase::Line {
                        shooter,
                        left,
                        of,
                        made,
                        at: f32::MAX,
                        ready: self.play.t + 0.3,
                    };
                }
                None
            }
        }
    }

    /// The ball goes back to the free throw shooter.
    fn send_back(&mut self, shooter: usize, a: After) -> Option<End> {
        let (p, z) = a.path.at(a.t);
        let mut path = Path::new();
        path.push(flight::Arc::reach(p, z, self.op(shooter), 1.1, 0.9));
        self.after_path(AfterKind::Return { shooter }, path, 0.0);
        None
    }

    /// Record the arcs of `path` from path time `from_t` on, with path time
    /// `from_t` falling `ahead` seconds after the current frame time.
    fn log_path(&mut self, path: &Path, from_t: f32, ahead: f32) {
        let f = self.frame;
        let wall = self.wall + ahead;
        let Some(rec) = &mut self.rec else { return };
        let mut start = 0.0f32;
        for a in path.arcs() {
            let end = start + a.dur;
            if end > from_t {
                let s = (from_t - start).max(0.0);
                let u = s / a.dur;
                let p0 = a.p0.lerp(a.p1, u);
                let vel = (a.p1 - a.p0) * (1.0 / a.dur);
                let c0 = to_court(p0, f);
                let cv = to_court(p0 + vel, f) - c0;
                rec.ball_arcs.push(BallArc {
                    t0: wall + (start + s - from_t),
                    dur: a.dur - s,
                    p0: c0,
                    vel: cv,
                    z0: a.z0 + a.vz * s - 0.5 * a.g * s * s,
                    vz: a.vz - a.g * s,
                    g: a.g,
                });
            }
            start = end;
        }
    }

    /// The ball was taken out of the air `back` seconds from the current
    /// frame time (zero or negative): drop what was recorded after that.
    fn cut_log(&mut self, back: f32) {
        let at = self.wall + back;
        let Some(rec) = &mut self.rec else { return };
        while let Some(a) = rec.ball_arcs.last() {
            if a.t0 >= at - 1e-4 {
                rec.ball_arcs.pop();
            } else {
                break;
            }
        }
        if let Some(a) = rec.ball_arcs.last_mut() {
            a.dur = a.dur.min(at - a.t0);
        }
    }

    /// A man holding the ball up at the height of the glass cannot take it
    /// through: his body stops against it.
    fn hold_off_glass(&mut self, t: usize, k: usize) {
        if self.play.ball_h > BOARD_LOW - 0.15 {
            let m = &mut self.m[t][k];
            if m.pos.y < BOARD_Y + BALL_RADIUS && m.pos.x.abs() < BOARD_HALF + BALL_RADIUS {
                m.pos.y = BOARD_Y + BALL_RADIUS;
                m.vel.y = m.vel.y.max(0.0);
            }
        }
    }

    /// Court index (side, k) of attackers-first index `i`.
    fn side_of(&self, i: usize) -> (usize, usize) {
        if i < 5 {
            (self.play.o, i)
        } else {
            (1 - self.play.o, i - 5)
        }
    }

    /// The ball is off the iron, the glass or a hand and live. Everybody
    /// reads it, and goes to where he thinks it will be.
    fn go_live(&mut self, path: Path, t0: f32, free_throw: bool) -> Option<End> {
        self.play.boxout = false;
        self.play.after = Some(After {
            kind: AfterKind::Live { free_throw },
            path,
            t: t0,
            end: path.total(),
        });
        self.plan_chase(&path, t0);
        None
    }

    /// Each man's read of the loose ball. How well he reads it is his
    /// rebounding: how early he sees where it is going, how close his guess
    /// lands and how well he times the jump. Whether he can be there is a
    /// matter of where he stands, how fast he is and who is in the way.
    fn plan_chase(&mut self, path: &Path, t0: f32) {
        for i in 0..10 {
            let (t, k) = self.side_of(i);
            let side = &self.sides[t];
            let slot = side.on[k];
            let attr = if t == self.play.o {
                Attr::OffRebound
            } else {
                Attr::DefRebound
            };
            let skill = side.eff(slot, attr);
            let pl = &side.slots[slot].p;
            let reach = pl.body.standing_reach_cm / 100.0;
            let peak = Pose::max_lift(pl.attrs[Attr::Vertical]) * JUMP_FRAC;
            let air = 2.0 * (2.0 * peak / GRAVITY).sqrt();
            let want = (reach + 0.9 * peak - 0.05).clamp(2.0, 3.6);
            let (tc, spot) = path
                .comes_down_to(t0, want)
                .or_else(|| path.comes_down_to(t0, 1.2))
                .unwrap_or((path.total(), path.end().0));
            let sigma = (READ_SPOT - READ_SPOT_SKILL * skill).clamp(0.12, 1.2);
            let truth = spot;
            let spot = Vec2 {
                x: spot.x + shot::gauss(&mut self.rng) * sigma,
                y: spot.y + shot::gauss(&mut self.rng) * sigma,
            };
            let react = (READ_DELAY - READ_DELAY_SKILL * skill).clamp(0.05, 0.5);
            let jitter = (READ_TIMING - READ_TIMING_SKILL * skill).clamp(0.03, 0.3);
            let jump_at = tc + shot::gauss(&mut self.rng) * jitter - 0.5 * air;
            let me = self.m[t][k].pos;
            let top = self.cache.top[t * 5 + k];
            let secs = (tc - t0 + 0.3 - react).max(0.0);
            self.play.chase[i] = Chase {
                active: me.dist(spot) <= top * secs + 1.5,
                spot,
                truth,
                go: t0 + react,
                arrive: tc,
                jump_at,
                jumped: false,
            };
        }
    }

    /// Whoever is under a rebound goes for it, and goes up to meet it.
    fn steer_rebound(&mut self) {
        let Some(After {
            kind: AfterKind::Live { .. },
            t: now,
            path,
            ..
        }) = self.play.after
        else {
            return;
        };
        let ball = self.play.ball;
        for i in 0..10 {
            let c = self.play.chase[i];
            let (t, k) = self.side_of(i);
            let pos = self.m[t][k].pos;
            // A man who sees the ball coming into his jump reach goes up
            // for it, planned or not.
            if !c.jumped {
                let pl = &self.sides[t].slots[self.sides[t].on[k]].p;
                let reach = pl.body.standing_reach_cm / 100.0;
                let frac = JUMP_FRAC;
                let peak = Pose::max_lift(pl.attrs[Attr::Vertical]) * frac;
                let (bp, bz, _, bvz) = path.state(now + 0.25);
                if bp.dist(pos) < 1.3 && bz > reach - 0.3 && bz < reach + peak && bvz < 1.0 && self.lift_at(t, k, 0.0) == 0.0 {
                    self.play.chase[i].jumped = true;
                    let air = 2.0 * (2.0 * peak / GRAVITY).sqrt();
                    self.leap(t, k, Action::Rebounding, frac, 0.0, air);
                    continue;
                }
            }
            // A ball that has come down and is still moving: whoever is
            // near goes to it where it is.
            let late = now > c.arrive + 0.2;
            if late && pos.dist(ball) < 6.0 {
                let m = &mut self.m[t][k];
                m.target = ball;
                m.pace = 1.0;
                continue;
            }
            if !c.active || now < c.go {
                continue;
            }
            // He sees it better as it comes: his guess closes on the truth.
            let w = ((now - c.go) / (c.arrive - c.go).max(0.1)).clamp(0.0, 1.0);
            let aim = c.spot.lerp(c.truth, w * w * 0.85);
            let dist = pos.dist(aim);
            let m = &mut self.m[t][k];
            m.target = aim;
            m.pace = if dist < 0.15 { 0.0 } else { 1.0 };
            let can = self.cache.top[t * 5 + k] * (c.arrive - now).max(0.0) + 1.2;
            if !c.jumped && now >= c.jump_at && dist < can {
                self.play.chase[i].jumped = true;
                let vertical = self.sides[t].slots[self.sides[t].on[k]].p.attrs[Attr::Vertical];
                let peak = Pose::max_lift(vertical) * JUMP_FRAC;
                let air = 2.0 * (2.0 * peak / GRAVITY).sqrt();
                self.leap(t, k, Action::Rebounding, JUMP_FRAC, 0.0, air);
            }
        }
    }

    /// Whose hand, if any, is on the ball at `ball` (over the floor, height)
    /// with the men where they are `ahead` seconds from now: for each of
    /// the ten, how far inside his reach it is.
    fn reach_of(&self, i: usize, ball: (Vec2, f32), pos: Vec2, ahead: f32) -> Option<f32> {
        let (t, k) = self.side_of(i);
        // The glass is in the way of a man behind it.
        if (pos.y < BOARD_Y) != (ball.0.y < BOARD_Y)
            && ball.1 > BOARD_LOW - 0.3
            && ball.0.x.abs() < 1.2
            && pos.x.abs() < 1.2
        {
            return None;
        }
        let reach = self.sides[t].slots[self.sides[t].on[k]].p.body.standing_reach_cm / 100.0;
        let arm = reach * ARM_SHARE;
        let lift = self.lift_at(t, k, ahead);
        let dh = pos.dist(ball.0);
        let dz = ball.1 - (reach - arm + lift);
        let gap = arm + 0.02 - (dh * dh + dz * dz).sqrt();
        if gap >= 0.0 {
            return Some(gap);
        }
        // Down at his feet he can bend for it.
        (ball.1 < 0.9 && dh < 0.65).then_some(0.0)
    }

    /// One tick of a live ball: look for the first hand on it, in fine
    /// steps through the tick.
    fn live_ball(&mut self, a: After) -> Option<End> {
        let AfterKind::Live { free_throw } = a.kind else {
            return None;
        };
        let steps = 10;
        let t1 = a.t;
        let t0 = (t1 - DT).max(0.0);
        for s in 1..=steps {
            let f = s as f32 / steps as f32;
            let ts = t0 + (t1 - t0) * f;
            let (bp, bz, bv, bvz) = a.path.state(ts);
            // On the rim the ball can still go in: nobody has a hand on it.
            let near_ring = bp.dist(HOOP) < 0.45 && bz > RIM_HEIGHT - 0.3 && bz < RIM_HEIGHT + 0.6;
            let mut cands: [(usize, f32); 10] = [(0, 0.0); 10];
            let mut n = 0;
            if !near_ring {
                for i in 0..10 {
                    let (t, k) = self.side_of(i);
                    let pos = self.prev_pos[t][k].lerp(self.m[t][k].pos, f);
                    if let Some(gap) = self.reach_of(i, (bp, bz), pos, ts - t1) {
                        cands[n] = (i, gap);
                        n += 1;
                    }
                }
            }
            if n > 0 {
                return self.touch_ball(a, ts, (bp, bz, bv, bvz), &cands[..n], free_throw);
            }
            // Out of play.
            if bp.x.abs() > 7.5 + BALL_RADIUS || bp.y < -BALL_RADIUS || bp.y > LENGTH + BALL_RADIUS
            {
                return self.out_of_bounds(a, ts);
            }
        }
        if t1 >= a.end + 8.0 {
            // Nobody has gone for it: the nearest man.
            let (bp, _, _, _) = a.path.state(a.end);
            let mut best = (0, f32::MAX);
            for i in 0..10 {
                let (t, k) = self.side_of(i);
                let d = self.m[t][k].pos.dist(bp);
                if d < best.1 {
                    best = (i, d);
                }
            }
            return self.control(a, a.end, best.0, free_throw);
        }
        None
    }

    /// Hands are on the ball: who has it, or whether it is knocked about
    /// and stays live.
    fn touch_ball(
        &mut self,
        a: After,
        ts: f32,
        state: (Vec2, f32, Vec2, f32),
        cands: &[(usize, f32)],
        free_throw: bool,
    ) -> Option<End> {
        let (bp, bz, bv, bvz) = state;
        let speed = (bv.dot(bv) + bvz * bvz).sqrt();
        // Whose it is when several have a hand on it: the one with the
        // better reach into it, the better rebounder and the stronger.
        let mut weights = [0.0f32; 10];
        for (j, &(i, gap)) in cands.iter().enumerate() {
            let (t, k) = self.side_of(i);
            let side = &self.sides[t];
            let attr = if t == self.play.o {
                Attr::OffRebound
            } else {
                Attr::DefRebound
            };
            let skill = side.eff(side.on[k], attr);
            weights[j] = skill * skill * (0.3 + gap) * side.length(side.on[k]);
        }
        let pick = self.rng.weighted(&weights[..cands.len()]);
        let who = cands[pick].0;
        let (t, k) = self.side_of(who);
        // Another hand close enough to it makes it a fight.
        let mut contested = cands.len() > 1;
        for i in 0..10 {
            if (i < 5) == (who < 5) {
                continue;
            }
            let (ot, ok) = self.side_of(i);
            if self.m[ot][ok].pos.dist(self.m[t][k].pos) < CONTEST_ZONE {
                contested = true;
            }
        }
        let side = &self.sides[t];
        let attr = if t == self.play.o {
            Attr::OffRebound
        } else {
            Attr::DefRebound
        };
        let skill = side.eff(side.on[k], attr);
        let hands = side.eff(side.on[k], Attr::Handling);
        let p_hold = (HOLD_BASE + HOLD_SKILL * (0.7 * skill + 0.3 * hands - 10.0)
            - if contested { HOLD_CONTEST } else { 0.0 }
            - (speed - 4.0).max(0.0) * HOLD_SPEED)
            .clamp(0.2, 0.99);
        if self.play.tips >= 3 || self.rng.chance(p_hold) {
            return self.control(a, ts, who, free_throw);
        }
        // Tipped: it goes up and away off his fingers and is live again.
        self.play.tips += 1;
        self.play.def_touch = who >= 5;
        // An attacker's hand on a ball at the ring that he cannot hold is a
        // tip-in: he puts it back up towards the basket, and it is a shot like
        // any other, only a rushed one.
        if who < 5
            && !free_throw
            && bp.dist(HOOP) < TIP_IN_RANGE
            && bz > RIM_HEIGHT - 0.3
            && bz < RIM_HEIGHT + 0.9
            && self.lift_at(self.play.o, who, 0.0) > 0.0
        {
            return self.tip_in(a, ts, (bp, bz), who);
        }
        // A direction from a unit square, no trigonometry; a ball knocked
        // about off the glass goes away from the basket (a tip is not a
        // shot: it is not scored if it happens to drop).
        let (dx, dy) = (self.rng.f32() * 2.0 - 1.0, self.rng.f32() * 2.0 - 1.0);
        let mut dir = if dx == 0.0 && dy == 0.0 { v(1.0, 0.0) } else { v(dx, dy).norm() };
        let out = (bp - HOOP).norm();
        if dir.dot(out) < 0.0 {
            dir = dir * -1.0;
        }
        let kick = 1.5 + self.rng.f32() * 1.5;
        let vel = v3(
            bv.x * 0.25 + dir.x * kick,
            bv.y * 0.25 + dir.y * kick,
            (bvz * 0.2).max(0.0) + 1.5 + self.rng.f32() * 2.0,
        );
        self.cut_log(ts - a.t);
        let flown = ball::fly(Ball { p: v3(bp.x, bp.y, bz), v: vel }, &[]);
        // The rest of this tick on the new path.
        let skip = a.t - ts;
        self.log_path(&flown.path, 0.0, -skip);
        self.play.after = Some(After {
            kind: a.kind,
            path: flown.path,
            t: skip,
            end: flown.path.total(),
        });
        self.plan_chase(&flown.path, skip);
        None
    }

    /// A tip at the ring by attacker `who`: a launch from where his hand
    /// met the ball, aimed at the ring, with the error of a hurried finish
    /// (his close touch and rebounding, grown by the hands that are on him).
    /// Whether it goes in is up to the ball.
    fn tip_in(&mut self, a: After, ts: f32, at: (Vec2, f32), who: usize) -> Option<End> {
        let (bp, bz) = at;
        let o = self.play.o;
        let finish = 0.4 * self.oe(who, Attr::OffRebound)
            + 0.4 * self.oe(who, Attr::CloseShot)
            + 0.2 * self.oe(who, Attr::Vertical);
        // Hands and bodies around the ball.
        let mut c = 0.0f32;
        for dk in 0..5 {
            let d = self.m[1 - o][dk].pos.dist(bp);
            if d < TIP_CROWD {
                c += 0.5 * (1.0 - d / TIP_CROWD);
            }
        }
        let c = c.min(1.2);
        let m = (1.0 - FINISH_SLOPE * (finish - 10.0)).clamp(0.4, 2.0)
            * TIP_SCALE
            * (1.0 + CONTEST_RIM * c);
        // A hand on a man going up for it: the same whistle as for any shot.
        let near = (0..5)
            .map(|dk| (dk, self.m[1 - o][dk].pos.dist(bp)))
            .filter(|&(_, d)| d < TIP_CROWD)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(dk, _)| dk);
        let contact = near.is_some_and(|dk| {
            let raw = self.foul_chance(ShotKind::Rim, c.min(1.0))
                * self.def().foul_mult(self.def().on[dk])
                * self.off().draw_mult(self.off().on[who])
                * self.verticality(dk, bp)
                / CALL_BASE;
            self.rng.chance(raw.min(0.95))
        });
        let fouled = contact && self.called(1.0);
        let m = if fouled {
            m * FOULED_ERR
        } else if contact {
            m * CONTACT_ERR
        } else {
            m
        };
        let start = ball::clear_start(v3(bp.x, bp.y, bz));
        let (ideal, _) = shot::ideal(start, Style::Layup);
        let (lat, depth) = shot::base(Style::Layup, true, bp.dist(HOOP));
        let e = shot::Errors::at_rim(bp.dist(HOOP), lat * m, depth * m);
        let launch = shot::perturb(ideal, e, &mut self.rng);
        let flown = ball::fly(Ball { p: start, v: launch }, &[]);
        let made = flown.made;
        if made || !fouled {
            self.oline(who).fga += 1;
        }
        let id = self.oid(who);
        if made {
            // A tip-in is the rebound and the basket at once: the book
            // gives him the board.
            self.oline(who).orb += 1;
            self.event(EventKind::Rebound {
                who: id,
                offensive: true,
            });
        }
        self.event(EventKind::Shot {
            who: id,
            kind: ShotKind::Rim,
            source: Source::Putback,
            made,
            contest: c.min(1.0),
            defender: near.map(|dk| self.did(dk)),
            assist: None,
            blocked_by: None,
            fouled,
        });
        if fouled && let Some(dk) = near {
            self.defensive_foul(dk, who, if made { 1 } else { 2 }, FoulKind::Shooting);
        }
        self.cut_log(ts - a.t);
        let skip = a.t - ts;
        self.log_path(&flown.path, 0.0, -skip);
        if made {
            let line = self.oline(who);
            line.fgm += 1;
            line.pts += 2;
            self.sides[o].score += 2;
            self.swish = true;
            if fouled {
                self.play.task = [Task::Spot; 5];
            }
            self.after_path(
                AfterKind::Drop {
                    shooter: who,
                    then: fouled as u16,
                },
                flown.path,
                skip,
            );
            return None;
        }
        if fouled {
            // Hit going up: a dead ball, and two shots.
            self.play.task = [Task::Spot; 5];
            self.after_path(AfterKind::Fall { shooter: who, shots: 2 }, flown.path, skip);
            return None;
        }
        self.play.after = Some(After {
            kind: a.kind,
            path: flown.path,
            t: skip,
            end: flown.path.total(),
        });
        self.plan_chase(&flown.path, skip);
        None
    }

    /// The ball goes out of bounds.
    fn out_of_bounds(&mut self, a: After, ts: f32) -> Option<End> {
        self.cut_log(ts - a.t);
        self.play.after = None;
        let (bp, bz, _, _) = a.path.state(ts);
        if self.play.def_touch {
            // Off a defender: it is the attackers' ball still, and they take
            // it in from the spot where it went out.
            let at = v(bp.x.clamp(-HALF_WIDTH, HALF_WIDTH), bp.y.clamp(0.0, LENGTH));
            let mut path = Path::new();
            path.fall(
                v(bp.x.clamp(-7.8, 7.8), bp.y.clamp(-0.3, LENGTH + 0.3)),
                bz.max(BALL_RADIUS),
                0.0,
                Vec2::ZERO,
            );
            self.loose = Some((path, 0.0));
            self.play.holder = None;
            self.play.flight = None;
            self.play.shot = None;
            self.play.verdict = None;
            self.play.shot_clock = self.play.shot_clock.max(14.0);
            self.start_inbound(Some(at), false, None);
            return None;
        }
        let mut path = Path::new();
        path.fall(
            Vec2 {
                x: bp.x.clamp(-7.8, 7.8),
                y: bp.y.clamp(-0.3, LENGTH + 0.3),
            },
            bz.max(BALL_RADIUS),
            0.0,
            Vec2::ZERO,
        );
        self.loose = Some((path, 0.0));
        self.credit = 0.0;
        Some(End::Dead)
    }

    /// A hand has the ball and keeps it.
    fn control(&mut self, a: After, ts: f32, who: usize, free_throw: bool) -> Option<End> {
        self.cut_log(ts - a.t);
        self.play.after = None;
        let (bp, bz, _, _) = a.path.state(ts);
        let (t, k) = self.side_of(who);
        // He takes it where it is: the hand brings it to his body.
        self.play.ball_h = bz;
        self.play.ball = self.m[t][k].pos;
        let _ = bp;
        // Going through a man who had the inside is a push, more likely the
        // more of him there is.
        let mut who = who;
        let landing = bp;
        let (held, blocker) = self.held_by(who, landing);
        if held > 0.0 && !free_throw {
            let (side_k, other_k) = self.side_of(who);
            let mult = self.sides[side_k].foul_mult(self.sides[side_k].on[other_k]);
            let raw = REB_PUSH * held.min(2.0) * mult / CALL_BASE;
            if self.rng.chance(raw.min(0.9)) && self.called(1.0) {
                if who >= 5 {
                    // A defender over the back: the offense keeps it, and it
                    // is the man he went through who has it.
                    let victim = blocker.min(4);
                    self.play.push = Some((who, victim));
                    who = victim;
                } else {
                    self.play.push = Some((who, blocker.saturating_sub(5)));
                }
            }
        }
        // Whoever ends up with it has it at a height he can hold it at.
        let (wt, wk) = self.side_of(who);
        let reach = self.sides[wt].slots[self.sides[wt].on[wk]].p.body.standing_reach_cm / 100.0;
        let lift = self.lift_at(wt, wk, 0.0);
        self.play.ball_h = self.play.ball_h.min(reach + lift);
        self.finish_rebound(who, landing, free_throw)
    }

    /// How much body is between man `i` and the ball at `landing`, on the
    /// inside of him, and the heaviest of it (attackers-first index).
    fn held_by(&self, i: usize, landing: Vec2) -> (f32, usize) {
        let bulk = self.bulks();
        let o = self.play.o;
        let pos = |i: usize| {
            let (t, k) = self.side_of(i);
            self.m[t][k].pos
        };
        let anchor = |i: usize| {
            let (t, k) = self.side_of(i);
            bulk[t * 5 + k].anchor
        };
        let radius = |i: usize| {
            let (t, k) = self.side_of(i);
            bulk[t * 5 + k].r
        };
        let _ = o;
        let mine = pos(i).dist(landing);
        let mut held = 0.0;
        let mut holder = (0usize, 0.0f32);
        for j in 0..10 {
            if (i < 5) == (j < 5) || pos(j).dist(landing) > mine - 0.1 {
                continue;
            }
            let lane = pos(j).project(pos(i), landing);
            if pos(j).dist(lane) < radius(i) + radius(j) + 0.25 {
                let (t, k) = self.side_of(j);
                let still = if self.m[t][k].vel.len() < 1.0 { 1.3 } else { 1.0 };
                held += anchor(j) * still / anchor(i);
                if anchor(j) > holder.1 {
                    holder = (j, anchor(j));
                }
            }
        }
        (held, holder.0)
    }

    /// The ball is in the winner's hands.
    fn finish_rebound(&mut self, who: usize, at: Vec2, free_throw: bool) -> Option<End> {
        let _ = (free_throw, at);
        self.play.ball = if who < 5 {
            self.m[self.play.o][who].pos
        } else {
            self.m[1 - self.play.o][who - 5].pos
        };
        if let Some((pusher, other)) = self.play.push.take() {
            if pusher >= 5 {
                // A defender over the back: the offense keeps it.
                self.play.holder = Some(who);
                self.play.phase = Phase::Set;
                self.play.think = 0.5;
                self.play.shot_clock = self.play.shot_clock.max(14.0);
                self.play.task = [Task::Spot; 5];
                self.play.dribbled = false;
                self.play.assist = None;
                self.play.boxout = false;
                return self.floor_foul(pusher - 5, who, FoulKind::Pushing);
            }
            self.play.holder = Some(who);
            return self.offensive_foul(pusher, other, FoulKind::Pushing);
        }
        if who >= 5 {
            let dk = who - 5;
            self.dline(dk).drb += 1;
            let who = self.did(dk);
            self.event(EventKind::Rebound {
                who,
                offensive: false,
            });
            return Some(End::Live(dk));
        }
        self.oline(who).orb += 1;
        let id = self.oid(who);
        self.event(EventKind::Rebound {
            who: id,
            offensive: true,
        });
        if self.clock <= 0.0 {
            return Some(End::Period);
        }
        self.play.holder = Some(who);
        self.play.source = Source::Putback;
        self.play.phase = Phase::Set;
        // He has to come down with it before he can go back up.
        let air = {
            let side = &self.sides[self.play.o];
            let v = side.slots[side.on[who]].p.attrs[Attr::Vertical];
            (2.0 * Pose::max_lift(v) * 0.9 / GRAVITY).sqrt()
        };
        self.play.think = air.max(0.25);
        self.play.shot_clock = 14.0;
        self.play.task = [Task::Spot; 5];
        self.play.dribbled = false;
        self.play.moves = 0;
        self.play.assist = None;
        self.play.from = None;
        None
    }

    // ---- whistles --------------------------------------------------------

    /// Book a personal foul on defender `dk` against attacker `h`.
    fn defensive_foul(&mut self, dk: usize, h: usize, shots: u8, kind: FoulKind) {
        let (who, on) = (self.did(dk), self.oid(h));
        self.dline(dk).pf += 1;
        let d = 1 - self.play.o;
        self.sides[d].team_fouls += 1;
        self.event(EventKind::Foul {
            who,
            on,
            shots,
            kind,
        });
        self.whistle_until = self.anim + WHISTLE_SHOWN;
        let side = &self.sides[d];
        if side.slots[side.on[dk]].line.pf >= self.rules.foul_limit {
            let (elapsed, regulation, limit) =
                (self.elapsed, self.regulation, self.rules.foul_limit);
            self.sides[d].replace(dk, elapsed, regulation, limit);
            // The man who comes on has not been jumping.
            self.jump[d][dk] = Jump::NONE;
            // The man who comes on may be bigger than the one who left.
            self.refresh_bulks();
            let bulk = self.bulks();
            self.collide(&bulk);
            let side = &self.sides[d];
            let on = side.slots[side.on[dk]].p.id;
            if on != who {
                self.event(EventKind::Substitution { on, off: who });
            }
        }
    }

    /// Does the crew call illegal contact of this size? A tight whistle
    /// calls more of it, a loose one lets them play.
    fn called(&mut self, severity: f32) -> bool {
        let p = (CALL_BASE * self.whistle * severity).min(0.97);
        self.rng.chance(p)
    }

    /// Put a player on the floor for `secs` after contact.
    fn fall(&mut self, t: usize, k: usize, secs: f32) {
        self.down[t][k] = self.anim + secs;
        if t != self.play.o {
            self.play.stun[k] = self.play.stun[k].max(secs);
        }
    }

    /// Defender `dk` against attacker `h`: a shooter's chance of being fouled
    /// goes down if the defender goes straight up from a spot and up if he
    /// moves into him.
    fn verticality(&self, dk: usize, shooter: Vec2) -> f32 {
        let m = &self.m[1 - self.play.o][dk];
        let closing = m.vel.dot((shooter - m.pos).norm());
        if closing < 0.8 {
            VERT_SET
        } else {
            (1.0 + VERT_MOVING * (closing - 0.8)).min(1.8)
        }
    }

    /// A beaten man's teammate may step into the driver's path and plant
    /// himself to take the charge. It takes a head, nerve and will, and it
    /// costs him the chance to block or contest.
    fn hunt_charge(&mut self, h: usize, beaten: usize) {
        let (p, dir) = (self.op(h), (HOOP - self.op(h)).norm());
        // Whoever with the urge can get to the lane: the man most drawn to
        // it, the nearer the better.
        let def = self.def();
        let mut best: Option<(f32, usize)> = None;
        for dk in 0..5 {
            if dk == beaten {
                continue;
            }
            let rel = self.dp(dk) - p;
            let ahead = rel.dot(dir);
            let aside = (rel - dir * ahead).len();
            let inside = self.dp(dk).dist(HOOP) < 4.5;
            let urge = def.hunt_urge(def.on[dk], inside);
            // The specialist reads the drive from further off.
            if ahead > 1.0
                && ahead < 5.0 + HUNT_FAR * urge
                && aside < 3.0 + HUNT_FAR * urge
                && best.is_none_or(|b| urge - HUNT_NEAR * aside > b.0)
            {
                best = Some((urge - HUNT_NEAR * aside, dk));
            }
        }
        let Some((_, hk)) = best else {
            return;
        };
        let slot = def.on[hk];
        let inside = self.dp(hk).dist(HOOP) < 4.5;
        let urge = def.hunt_urge(slot, inside);
        let want = (urge * HUNT_CAP * (1.0 + 0.5 * def.aggression())).clamp(0.0, HUNT_CAP);
        if !self.rng.chance(want) {
            return;
        }
        // Planted outside the no-charge area, in the line to the basket.
        let away = (p.dist(HOOP) - 1.4).clamp(NO_CHARGE_RADIUS + 0.3, 3.4);
        let spot = HOOP.toward(p, away);
        // He has to be there, and still, before the driver is.
        let top = 4.6 + self.def().eff(slot, Attr::Speed) * 0.16;
        let (mine, his) = (self.dp(hk).dist(spot), p.dist(spot));
        if mine / (0.9 * top) + SET_FOR + 0.15 > his / 3.5 + 0.2 {
            return;
        }
        // A driver with his head up sees it coming and goes round.
        // A man who has done it a hundred times comes in where the driver
        // is not looking.
        let blind = self.rng.chance(
            ((0.8 - self.oe(h, Attr::Iq) * 0.02).clamp(0.2, 0.8) + HUNT_BLIND * urge).min(0.97),
        );
        if let Some(rec) = &mut self.rec {
            rec.charges[3] += 1;
        }
        self.play.hunt = Some((hk, spot, blind));
    }

    /// The driver has run into somebody. Who is at fault is read off the
    /// bodies: a defender who was set in the path outside the no-charge area
    /// and took it on the chest has drawn a charge; one who was moving, got
    /// there late, or stood under the basket has blocked.
    fn drive_contact(&mut self, h: usize) -> Option<Option<End>> {
        let (o, d) = (self.play.o, 1 - self.play.o);
        let me = self.touch[o * 5 + h];
        let mut found = (me.is_some() && me.with as usize / 5 == d && me.closing >= IMPACT)
            .then_some((me.with as usize % 5, me.closing));
        // A driver who has not seen the man planted in his path arrives at
        // him at his own speed.
        if found.is_none()
            && let Some((hk, _, true)) = self.play.hunt
        {
            let bulk = self.bulks();
            let (p, q) = (self.op(h), self.dp(hk));
            let reach = bulk[o * 5 + h].r + bulk[d * 5 + hk].r + HUNT_REACH;
            let speed = self.m[o][h].vel.dot((q - p).norm());
            if p.dist(q) < reach && speed >= IMPACT {
                found = Some((hk, speed));
            }
        }
        let (dk, impact) = found?;
        if self.play.hit == Some(dk) {
            return None;
        }
        self.play.hit = Some(dk);
        let theirs = self.touch[d * 5 + dk];
        let q = self.dp(dk);
        let set = theirs.set && self.set_for[d][dk] >= SET_FOR;
        let outside = q.dist(HOOP) >= NO_CHARGE_RADIUS + 0.1;
        let mult = self.def().foul_mult(self.def().on[dk]);
        // Taking it costs him.
        let slot = self.sides[d].on[dk];
        let s = &mut self.sides[d].slots[slot];
        s.fatigue = (s.fatigue + CHARGE_COST * impact).min(1.0);
        if set && outside {
            // A man who went there on purpose gets the call far more often than
            // one who happened to be standing in the way.
            let planted = self.play.hunt.is_some_and(|(hk, _, _)| hk == dk);
            let sev = if planted {
                let urge = self.def().hunt_urge(slot, q.dist(HOOP) < 4.5);
                (PLANTED_SEV + PLANTED_IMPACT * impact) * (1.0 + PLANTED_SKILL * urge)
            } else {
                CHARGE_SEV + CHARGE_IMPACT * impact
            };
            if self.called(sev) {
                if let Some(rec) = &mut self.rec {
                    rec.charges[if planted { 0 } else { 1 }] += 1;
                }
                self.fall(d, dk, 1.5);
                return Some(self.offensive_foul(h, dk, FoulKind::Charging));
            }
            return None;
        }
        // Not set: he may sell it.
        let urge = self.def().flop_urge(slot);
        if self.rng.chance(FLOP * urge * urge) {
            if self.rng.chance(FOOLED * self.whistle) {
                if let Some(rec) = &mut self.rec {
                    rec.charges[2] += 1;
                }
                self.fall(d, dk, 1.5);
                return Some(self.offensive_foul(h, dk, FoulKind::Charging));
            }
            // The referee is not fooled and the play goes on past a man
            // lying on the floor.
            if let Some(rec) = &mut self.rec {
                rec.charges[4] += 1;
            }
            self.fall(d, dk, 1.5);
            return None;
        }
        if self.called((BLOCK_SEV + BLOCK_IMPACT * impact) * mult) {
            return Some(self.floor_foul(dk, h, FoulKind::Blocking));
        }
        None
    }

    /// A foul by attacker `k` on defender `dk`: a turnover, and a personal.
    fn offensive_foul(&mut self, k: usize, dk: usize, kind: FoulKind) -> Option<End> {
        let (who, on) = (self.oid(k), self.did(dk));
        self.oline(k).pf += 1;
        self.event(EventKind::Foul {
            who,
            on,
            shots: 0,
            kind,
        });
        self.whistle_until = self.anim + WHISTLE_SHOWN;
        self.turnover(k, TurnoverKind::OffensiveFoul, None, 0.0)
    }

    /// A foul away from a shot: free throws only in the bonus, otherwise the
    /// offense takes it out and starts again.
    fn floor_foul(&mut self, dk: usize, h: usize, kind: FoulKind) -> Option<End> {
        let d = 1 - self.play.o;
        let bonus = self.sides[d].team_fouls >= self.rules.bonus_after;
        self.defensive_foul(dk, h, if bonus { 2 } else { 0 }, kind);
        if bonus {
            // The ball is where it was, and the man who was fouled goes to it.
            let at = self.play.ball;
            return self.free_throws(h, 2, Some(at));
        }
        // The floor stands still for a moment while the whistle is answered.
        self.play.dead = DEAD_FOUL;
        self.play.shot_clock = self.play.shot_clock.max(14.0);
        self.play.phase = Phase::Set;
        self.play.think = 0.8;
        self.play.task = [Task::Spot; 5];
        self.play.dribbled = false;
        self.play.assist = None;
        self.play.drop = None;
        None
    }

    /// Free throws: the players line up along the lane, then the shooter
    /// takes them one by one. `ball` is where it is, if the shooter has not
    /// got it.
    fn free_throws(&mut self, h: usize, shots: u16, ball: Option<Vec2>) -> Option<End> {
        self.assign_ft_spots(h);
        self.play.flight = None;
        self.play.after = None;
        self.play.dead = 0.0;
        self.play.boxout = false;
        self.play.shot = None;
        self.play.task = [Task::Spot; 5];
        match ball {
            Some(at) => {
                self.play.holder = None;
                self.play.fetcher = Some(h);
                self.play.fetch_t = self.play.t;
                self.play.ball = at;
            }
            None => {
                self.play.holder = Some(h);
                self.play.fetcher = None;
            }
        }
        self.play.phase = Phase::Line {
            shooter: h,
            left: shots,
            of: shots,
            made: 0,
            at: f32::MAX,
            ready: self.play.t + FT_LINEUP,
        };
        None
    }

    /// Where the ten stand: the shooter on the line, the defense on the
    /// spaces nearest the basket, his team behind them.
    fn assign_ft_spots(&mut self, shooter: usize) {
        const OFF: [Vec2; 4] = [v(-2.0, 3.6), v(2.0, 3.6), v(-2.0, 4.7), v(3.6, 7.0)];
        const DEF: [Vec2; 5] = [
            v(-2.0, 2.5),
            v(2.0, 2.5),
            v(2.0, 4.7),
            v(-3.6, 7.0),
            v(0.0, 9.0),
        ];
        let o = self.play.o;
        self.ft_spot[shooter] = FT_LINE;
        let mut taken = [false; 4];
        for k in 0..5 {
            if k == shooter {
                continue;
            }
            let at = self.m[o][k].pos;
            let best = (0..4)
                .filter(|&i| !taken[i])
                .min_by(|&a, &b| at.dist(OFF[a]).total_cmp(&at.dist(OFF[b])))
                .unwrap_or(0);
            taken[best] = true;
            self.ft_spot[k] = OFF[best];
        }
        let mut taken = [false; 5];
        for k in 0..5 {
            let at = self.m[1 - o][k].pos;
            let best = (0..5)
                .filter(|&i| !taken[i])
                .min_by(|&a, &b| at.dist(DEF[a]).total_cmp(&at.dist(DEF[b])))
                .unwrap_or(0);
            taken[best] = true;
            self.ft_spot[5 + k] = DEF[best];
        }
    }

    fn steer_line(&mut self) {
        let Phase::Line { shooter, .. } = self.play.phase else {
            return;
        };
        let o = self.play.o;
        let loose = self.play.holder.is_none()
            && self.play.flight.is_none()
            && self.play.after.is_none()
            && self.play.fetcher == Some(shooter);
        for i in 0..10 {
            let (t, k) = if i < 5 { (o, i) } else { (1 - o, i - 5) };
            let mut target = self.ft_spot[i];
            let mut pace = 0.7;
            let mut brake = false;
            if t == o && k == shooter {
                if loose {
                    target = self.play.ball;
                    pace = 0.9;
                } else if self.play.holder != Some(k) || self.play.shot.is_some() {
                    target = self.m[o][k].pos;
                    pace = 0.0;
                    brake = true;
                }
            }
            let m = &mut self.m[t][k];
            m.target = target;
            m.pace = pace;
            m.hard = brake;
        }
    }

    fn line_up(&mut self) -> Option<End> {
        let Phase::Line {
            shooter,
            left,
            of,
            made,
            at,
            ready,
        } = self.play.phase
        else {
            return None;
        };
        let h = self.play.holder?;
        if at == f32::MAX {
            let there = self.op(h).dist(FT_LINE) < SPOT_NEAR;
            if (self.play.t >= ready && there) || self.play.t >= ready + 3.0 {
                self.play.shot = None;
                self.plan_shot(h, 0.0, true);
                let release = self.play.shot.map_or(self.play.t + 1.0, |s| s.release);
                self.play.phase = Phase::Line {
                    shooter,
                    left,
                    of,
                    made,
                    at: release,
                    ready,
                };
            }
            return None;
        }
        if self.play.t >= at {
            return self.ft_release(h, left, of, made);
        }
        None
    }

    /// The free throw leaves his hand: a standing, unguarded launch, with
    /// an error that is a matter of his free throw rating.
    fn ft_release(&mut self, h: usize, left: u16, of: u16, made: u16) -> Option<End> {
        let o = self.play.o;
        let slot = self.sides[o].on[h];
        let rating = self.sides[o].eff(slot, Attr::FreeThrow);
        let p = self.op(h);
        let plan = self.play.shot.unwrap_or(ShotPlan {
            shooter: h,
            style: Style::Jumper,
            start: self.play.t,
            jump_at: self.play.t,
            release: self.play.t,
            land: self.play.t + 0.5,
            follow_end: self.play.t + 1.0,
            peak: 0.1,
            hand_z: 2.4,
        });
        let from = p + (HOOP - p).norm() * 0.2;
        let d = from.dist(HOOP);
        let (lat, depth) = shot::base(Style::Jumper, false, d);
        let m = (FT_SCALE - FT_SLOPE * (rating - 10.0)).clamp(0.12, 1.4);
        let errors = Errors::at_rim(d, lat * m, depth * m);
        let start = ball::clear_start(v3(from.x, from.y, plan.hand_z));
        let (ideal, _) = shot::ideal(start, Style::Jumper);
        let launch = shot::perturb(ideal, errors, &mut self.rng);
        let flown = ball::fly(Ball { p: start, v: launch }, &[]);
        let hit = flown.made;
        let path = flown.path;
        self.log_path(&path, 0.0, 0.0);
        self.play.def_touch = false;
        self.play.tips = 0;
        let (b, bz) = path.at(0.0);
        self.play.ball = b;
        self.play.ball_h = bz;
        self.play.flight = Some(Flight {
            from: p,
            to: HOOP,
            t: 0.0,
            dur: if hit {
                flown.ring_at
            } else {
                Self::live_time(&flown)
            },
            kind: FlightKind::Shot {
                shooter: h,
                made: hit,
                kind: ShotKind::Mid,
                assist: None,
                and_one: false,
                foul_shots: 0,
                line: true,
            },
            path: Some(path),
        });
        self.play.holder = None;
        self.play.phase = Phase::Line {
            shooter: h,
            left: left.saturating_sub(1),
            of,
            made,
            at: f32::MAX,
            ready: 0.0,
        };
        None
    }

    fn land_free_throw(&mut self, shooter: usize, hit: bool, t: f32, path: Path) -> Option<End> {
        let Phase::Line { left, of, made, .. } = self.play.phase else {
            return None;
        };
        let o = self.play.o;
        let slot = self.sides[o].on[shooter];
        self.sides[o].book_ft(slot, hit);
        let made = made + hit as u16;
        if hit {
            self.swish = true;
        }
        if left == 0 {
            let who = self.oid(shooter);
            self.event(EventKind::FreeThrows {
                who,
                made: made as u8,
                of: of as u8,
            });
            // The line-up is over: the clocks run again.
            self.play.phase = Phase::Air;
            if hit {
                self.after_path(AfterKind::Drop { shooter, then: 0 }, path, t);
                return None;
            }
            return self.go_live(path, t, true);
        }
        self.play.phase = Phase::Line {
            shooter,
            left,
            of,
            made,
            at: f32::MAX,
            ready: 0.0,
        };
        if hit {
            self.after_path(AfterKind::Drop { shooter, then: 0 }, path, t);
        } else {
            self.after_path(AfterKind::Fall { shooter, shots: 0 }, path, t);
        }
        None
    }

    // ---- recording -------------------------------------------------------

    /// A direction in the attack frame as a unit vector in court
    /// coordinates, scaled to bytes.
    fn face_court(&self, d: Vec2) -> [i8; 2] {
        let d = d.norm();
        let c = if self.frame == 0 {
            v(-d.y, d.x)
        } else {
            v(d.y, -d.x)
        };
        [(c.x * 127.0).round() as i8, (c.y * 127.0).round() as i8]
    }

    /// The shot a man is taking: which part of it he is in and how far
    /// through that part, and how high his feet are.
    fn pose_shot(&self, pose: &mut Pose, plan: &ShotPlan, _at: Vec2) {
        let t = self.play.t;
        let pct = |a: f32, b: f32| (((t - a) / (b - a).max(0.01)).clamp(0.0, 1.0) * 100.0) as u8;
        pose.action = Action::Shooting;
        pose.style = plan.style;
        let post = plan.release + 0.15;
        let (stage, progress) = if t < plan.jump_at {
            (ShotStage::Gather, pct(plan.start, plan.jump_at))
        } else if t < plan.release {
            (ShotStage::Rise, pct(plan.jump_at, plan.release))
        } else if t < post {
            (ShotStage::Release, pct(plan.release, post))
        } else if t < plan.land {
            (ShotStage::Fall, pct(post, plan.land))
        } else {
            (ShotStage::Follow, pct(plan.land, plan.follow_end))
        };
        pose.stage = stage;
        pose.progress = progress;
        if t >= plan.jump_at && t < plan.land {
            let u = (t - plan.jump_at) / (plan.land - plan.jump_at).max(0.01);
            pose.lift_cm = (plan.peak * 4.0 * u * (1.0 - u) * 100.0) as u16;
        }
    }

    /// What player `k` of side `t` is doing, read off the state of play.
    fn pose(&self, t: usize, k: usize) -> Pose {
        let me = &self.m[t][k];
        let speed = me.vel.len();
        let o = self.play.o;
        let attack = t == o;
        let now = self.anim;
        let jump = &self.jump[t][k];
        let mut pose = Pose::default();
        let mut toward: Option<Vec2> = None;
        let touch = &self.touch[t * 5 + k];
        if touch.is_some() {
            pose.contact = touch.with;
        }
        let hunting = !attack
            && self
                .play
                .hunt
                .is_some_and(|(hk, spot, _)| hk == k && me.pos.dist(spot) < 0.4);
        let shot_up = self.play.flight.is_some() || self.play.after.is_some();
        // Going up for a rebound takes him out of his follow-through.
        let rebounding = jump.dur > 0.0
            && jump.action == Action::Rebounding
            && now >= jump.start
            && now < jump.start + jump.dur;
        let plan = self.play.shot.filter(|s| {
            attack
                && !rebounding
                && s.shooter == k
                && self.play.t >= s.start
                && self.play.t < s.follow_end
        });
        if let Some(plan) = plan
            && now >= self.down[t][k]
        {
            self.pose_shot(&mut pose, &plan, me.pos);
            toward = Some(match plan.style {
                Style::Hook if self.play.t < plan.jump_at => me.pos + (me.pos - HOOP),
                _ => HOOP,
            });
            // The free throw routine: a few bounces before he goes up.
            if matches!(self.play.phase, Phase::Line { .. })
                && self.play.t < plan.jump_at - DIP
                && self.play.holder == Some(k)
            {
                pose.action = Action::Dribbling;
                pose.stage = ShotStage::Gather;
                pose.progress = 0;
            }
        } else if now < self.down[t][k] {
            pose.action = Action::Down;
            pose.progress = ((1.0 - (self.down[t][k] - now) / 1.5).clamp(0.0, 1.0) * 100.0) as u8;
            toward = Some(self.play.ball);
        } else if hunting {
            pose.action = Action::Bracing;
            toward = self.play.holder.map(|h| self.op(h));
        } else if jump.dur > 0.0 && now >= jump.start && now < jump.start + jump.dur {
            let u = (now - jump.start) / jump.dur;
            pose.action = jump.action;
            pose.progress = (u * 100.0) as u8;
            pose.lift_cm = (jump.peak * 4.0 * u * (1.0 - u) * 100.0) as u16;
            toward = Some(match jump.action {
                Action::Shooting => HOOP,
                _ => self.play.ball,
            });
        } else if attack {
            let holder = self.play.holder == Some(k);
            if let Some(Flight {
                t: ft,
                kind: FlightKind::Pass { from, .. },
                ..
            }) = self.play.flight
                && from == k
                && ft < 0.25
            {
                pose.action = Action::Passing;
                pose.progress = (ft / 0.25 * 100.0) as u8;
                toward = Some(self.play.flight.map_or(HOOP, |f| f.to));
            } else if holder && self.play.wait <= 0.0 {
                match self.play.phase {
                    Phase::Gather { at, .. } => {
                        pose.action = Action::Shooting;
                        pose.progress =
                            ((1.0 - (at - self.play.t) / 0.4).clamp(0.0, 1.0) * 100.0) as u8;
                        toward = Some(HOOP);
                    }
                    Phase::Drive { beaten, until, .. } => {
                        pose.action = if beaten {
                            Action::Driving
                        } else {
                            Action::Dribbling
                        };
                        let left = (until - self.play.t).clamp(0.0, 5.0);
                        pose.progress = ((1.0 - left / 5.0) * 100.0) as u8;
                    }
                    Phase::Post { until, .. } => {
                        pose.action = Action::PostingUp;
                        pose.progress =
                            ((1.0 - (until - self.play.t) / 2.1).clamp(0.0, 1.0) * 100.0) as u8;
                        toward = Some(me.pos + (me.pos - HOOP));
                    }
                    _ => {
                        if speed > 0.3 || self.play.dribbled {
                            pose.action = Action::Dribbling;
                        }
                        if speed <= 0.3 {
                            toward = Some(HOOP);
                        }
                    }
                }
            } else if matches!(self.play.task[k], Task::SetScreen) {
                pose.action = Action::Screening;
                let dk = self.play.on_ball.unwrap_or(0);
                toward = Some(self.dp(dk));
                if let Phase::Screen { until, .. } = self.play.phase {
                    pose.progress =
                        ((1.0 - (until - self.play.t) / 2.4).clamp(0.0, 1.0) * 100.0) as u8;
                }
            } else if matches!(self.play.task[k], Task::Cut(_) | Task::Roll) && speed > 1.0 {
                pose.action = Action::Cutting;
            } else if speed > 1.0 {
                pose.action = Action::Moving;
                // Going back or to the glass with his eyes on the ball.
                if shot_up {
                    toward = Some(self.play.ball);
                }
            } else {
                toward = Some(self.play.ball);
            }
        } else if self.play.boxout && self.play.holder.is_none() {
            pose.action = Action::BoxingOut;
            toward = Some(HOOP);
        } else if matches!(self.play.phase, Phase::Line { .. }) {
            // Lined up along the lane, watching the shooter.
            toward = Some(self.play.ball);
        } else if self.play.wait > 0.0 {
            // Dead ball: standing.
        } else if let Some(h) = self.play.holder
            && speed > 2.2
            && me.vel.dot(self.op(h) - me.pos) > 0.0
            && me.pos.dist(self.op(h)) > 1.2
            && me.pos.dist(self.op(h)) < 6.0
        {
            pose.action = Action::ClosingOut;
            toward = Some(self.op(h));
        } else if speed > 3.0 {
            pose.action = Action::Moving;
        } else if !matches!(self.play.phase, Phase::Advance) || self.play.live_start {
            pose.action = Action::Defending;
            toward = Some(if is_zone(self.scheme()) {
                self.play.ball
            } else {
                self.op(self.play.guard[k])
            });
        }
        if let Some(to) = toward {
            let d = to - me.pos;
            // Only worth recording when it is not simply the way he runs.
            if d.len() > 0.05 && (speed < 0.5 || me.vel.dot(d.norm()) < speed * 0.8) {
                pose.facing = self.face_court(d);
            }
        }
        pose
    }

    fn record(&mut self) {
        // A ball in a hand is no longer on its recorded flight.
        if self.play.holder.is_some() && self.rec.is_some() {
            self.cut_log(0.0);
        }
        let every = (1.0 / (FRAME_RATE * DT) + 0.5) as u32;
        if self.rec.is_none() || !self.ticks.is_multiple_of(every.max(1)) {
            return;
        }
        let cut = std::mem::take(&mut self.cut);
        let swish = std::mem::take(&mut self.swish);
        let f = self.frame;
        let mut pos = [[Vec2::ZERO; 5]; 2];
        let mut who = [[0u8; 5]; 2];
        for t in 0..2 {
            for k in 0..5 {
                pos[t][k] = to_court(self.m[t][k].pos, f);
                // The defending side is stored in the attackers' frame.
                who[t][k] = self.sides[t].on[k] as u8;
            }
        }
        let frame = Frame {
            time: self.wall,
            period: self.period,
            clock: self.clock.max(0.0),
            shot_clock: self.play.shot_clock.max(0.0),
            score: [self.sides[0].score, self.sides[1].score],
            offense: self.play.o as u8,
            pos,
            who,
            ball: to_court(self.play.ball, f),
            ball_height: self.play.ball_h,
            holder: self.play.holder.map(|k| (self.play.o as u8, k as u8)),
            pose: std::array::from_fn(|t| std::array::from_fn(|k| self.pose(t, k))),
            whistle: self.anim < self.whistle_until,
            cut,
            swish,
        };
        if let Some(rec) = &mut self.rec {
            rec.frames.push(frame);
        }
    }
}

impl Play {
    fn idle() -> Play {
        Play {
            o: 0,
            t: 0.0,
            shot_clock: 24.0,
            live_start: false,
            wait: 0.0,
            holder: None,
            ball: Vec2::ZERO,
            ball_h: 1.0,
            flight: None,
            after: None,
            shot: None,
            verdict: None,
            chase: [Chase::NONE; 10],
            def_touch: false,
            tips: 0,
            lane: [0.0; 5],
            fetcher: None,
            fetch_t: 0.0,
            push: None,
            inbound: false,
            dead: 0.0,
            frozen: 0.0,
            phase: Phase::Advance,
            think: 0.0,
            spots: [Vec2::ZERO; 5],
            runs: [None; 5],
            task: [Task::Spot; 5],
            guard: [0, 1, 2, 3, 4],
            stun: [0.0; 5],
            lapse_at: [-100.0; 5],
            dribbled: false,
            moves: 0,
            tries: [0; 5],
            assist: None,
            from: None,
            handler: 0,
            star: 0,
            post: 0,
            big: 4,
            drop: None,
            on_ball: None,
            help_logged: false,
            boxout: false,
            crossed: false,
            edge: 1.0,
            roller: None,
            source: Source::PullUp,
            blown: None,
            post_from: Vec2::ZERO,
            late_help: None,
            drive_class: 0,
            late_seen: false,
            hit: None,
            shoulder: None,
            route: None,
            hedge_until: 0.0,
            contained: false,
            hunt: None,
            brk: false,
            live_origin: false,
            brk_logged: false,
            go: [0.9; 5],
            inb: None,
        }
    }
}
