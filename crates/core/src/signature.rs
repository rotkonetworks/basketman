//! Signature moves: things a player is born able to do, that nobody can
//! teach him. See `docs/signature-moves.md` for the design and the
//! catalogue this table follows.
//!
//! Everything about a move lives in one row of `TABLE`: its plain name, what
//! it does, what answers it, its group and what a body has to be like to make
//! it plausible. The profile page, scout reports, the assistant's mail and
//! the agent interface all read from here.
//!
//! The match engine does not use signatures yet. Until it does they change
//! no result. When a game shows one, `Scouting::observe_signature` marks it
//! known.
//!
//! Assignment (`roll`) uses its own random stream, seeded from the player's
//! id and name, so adding or changing it never disturbs the rest of
//! generation, and the same player always gets the same answer.

use serde::{Deserialize, Serialize};

use crate::player::{Attr, Attributes, Body, Player, PlayerId};
use crate::rng::Rng;

/// A player has at most this many.
pub const MAX_SIGNATURES: usize = 3;
/// Scouting knowledge at which another club's player's signatures are
/// known. The same bar as character (`view::CHARACTER_KNOWN_AT`).
pub const KNOWN_AT: f32 = 0.6;
/// How far below the level that makes a move plausible a physical attribute
/// may fall before his body can no longer do it.
const WEAR_ROOM: f32 = 3.0;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Group {
    Shots,
    OffTheDribble,
    Passing,
    AtTheRim,
    DrawingFouls,
    Defense,
    ScreensAndBoards,
    Moments,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Shots => "Shots",
            Group::OffTheDribble => "Off the dribble",
            Group::Passing => "Passing",
            Group::AtTheRim => "At the rim",
            Group::DrawingFouls => "Drawing fouls",
            Group::Defense => "Defense",
            Group::ScreensAndBoards => "Screens and boards",
            Group::Moments => "Moments",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Signature {
    // Shots
    SweepingHook,
    OneLeggedFadeaway,
    OffBalanceLeaner,
    TurnaroundFadeaway,
    GlassSpecialist,
    StepBackThree,
    LogoRange,
    QuickRelease,
    ShooterOnTheMove,
    Teardrop,
    FingerRoll,
    MidRangePullUp,
    UnderhandFreeThrows,
    // Off the dribble
    KillerCrossover,
    ChangeOfPace,
    LongStrideGather,
    SideStep,
    SpinMove,
    WrongFootFinish,
    PowerSpin,
    UpAndUnder,
    PostFootwork,
    JabStep,
    // Passing
    NoLookPass,
    OneHandedSling,
    FullCourtOutlet,
    TouchPass,
    LobThrower,
    PassFake,
    BigManAsHub,
    ShortRollPlaymaker,
    // At the rim
    PosterDunk,
    LobFinisher,
    PutbackInstinct,
    FloaterOrLob,
    // Drawing fouls
    RipThrough,
    PumpFake,
    SellsContact,
    // Defense
    ChaseDownBlock,
    StraightUp,
    ShotBlockingPresence,
    TakesTheCharge,
    OnBallPest,
    BigHands,
    StripOnTheGather,
    // Screens and boards
    BrickWallScreens,
    SealForTheDriver,
    ReadsTheBounce,
    BoxesOutForOthers,
    // Moments
    BigShotTaker,
}

/// What a body and skill set must have for a move to be plausible. Physical
/// attributes in `attrs` are also what wears out: see `Signature::live`.
#[derive(Copy, Clone, Debug)]
pub struct Needs {
    /// Minimum attribute levels.
    pub attrs: &'static [(Attr, f32)],
    /// Height in cm, at least and at most.
    pub height_min: f32,
    pub height_max: f32,
    pub weight_min: f32,
    /// Wingspan minus height, in cm, at least.
    pub ape_min: f32,
    pub hand_min: f32,
}

const ANY: Needs = Needs {
    attrs: &[],
    height_min: 0.0,
    height_max: 999.0,
    weight_min: 0.0,
    ape_min: -99.0,
    hand_min: 0.0,
};

pub struct Info {
    pub sig: Signature,
    pub name: &'static str,
    pub does: &'static str,
    pub answer: &'static str,
    pub group: Group,
    pub needs: Needs,
}

use Attr::*;
use Signature as S;

macro_rules! row {
    ($sig:expr, $group:expr, $name:expr, $does:expr, $answer:expr, $needs:expr) => {
        Info {
            sig: $sig,
            name: $name,
            does: $does,
            answer: $answer,
            group: $group,
            needs: $needs,
        }
    };
}

/// One row per move, in the order of `Signature::ALL`.
static TABLE: [Info; Signature::COUNT] = [
    row!(
        S::SweepingHook,
        Group::Shots,
        "Sweeping hook",
        "Releases a hook high above the defender's reach, so only a taller and longer defender can contest it.",
        "Deny the catch, front the post, or send a second defender before he turns.",
        Needs {
            attrs: &[(PostMoves, 12.0), (CloseShot, 11.0)],
            height_min: 200.0,
            ape_min: 7.0,
            ..ANY
        }
    ),
    row!(
        S::OneLeggedFadeaway,
        Group::Shots,
        "One-legged fadeaway",
        "A fadeaway off one foot that makes its own space, so the contest counts for far less and it cannot be blocked from the front.",
        "Put a smaller, quicker defender on him who crowds the catch, so he never reaches his spot.",
        Needs {
            attrs: &[(MidRange, 13.0), (CloseShot, 10.0), (Vertical, 9.0)],
            height_min: 198.0,
            ..ANY
        }
    ),
    row!(
        S::OffBalanceLeaner,
        Group::Shots,
        "Off-balance leaner",
        "Shoots a one-legged shot off the wrong foot, going away from the basket, and gets a clean look without beating his man.",
        "Stay down, do not jump at the fake, and send help from the baseline side.",
        Needs {
            attrs: &[(CloseShot, 12.0), (Iq, 12.0)],
            weight_min: 95.0,
            ..ANY
        }
    ),
    row!(
        S::TurnaroundFadeaway,
        Group::Shots,
        "Turnaround fadeaway",
        "From the post or the elbow he turns either way into a fadeaway, so the defender cannot sit on one shoulder.",
        "Use length: a defender with the reach to bother it without leaving the floor.",
        Needs {
            attrs: &[(PostMoves, 12.0), (MidRange, 12.0)],
            height_min: 193.0,
            ..ANY
        }
    ),
    row!(
        S::GlassSpecialist,
        Group::Shots,
        "Glass specialist",
        "Bank shots from the wings and the block go in more often than the same shot straight on.",
        "Force him to the middle of the floor.",
        Needs {
            attrs: &[(CloseShot, 12.0), (PostMoves, 11.0), (Iq, 11.0)],
            height_min: 198.0,
            ..ANY
        }
    ),
    row!(
        S::StepBackThree,
        Group::Shots,
        "Step-back three",
        "Steps back behind the line off the dribble and opens enough space to shoot the three as if lightly contested.",
        "Pick him up higher, crowd his gather hand, or switch a long defender onto him.",
        Needs {
            attrs: &[
                (ThreePoint, 13.0),
                (Handling, 13.0),
                (Footwork, 12.0),
                (Speed, 9.0)
            ],
            height_max: 205.0,
            ..ANY
        }
    ),
    row!(
        S::LogoRange,
        Group::Shots,
        "Logo range",
        "Makes real shots from two metres behind the line, so the defense has to guard him from half court and the floor opens for everyone else.",
        "Pick up at half court and trap the ball screen, accepting the four-on-three behind it.",
        Needs {
            attrs: &[(ThreePoint, 16.0), (FreeThrow, 13.0), (Composure, 12.0)],
            ..ANY
        }
    ),
    row!(
        S::QuickRelease,
        Group::Shots,
        "Quick release",
        "Catch-and-shoot threes leave his hands before a closing defender arrives, so the close-out counts for less.",
        "Top-lock him and chase him over every screen so he never catches the ball facing the basket.",
        Needs {
            attrs: &[(ThreePoint, 14.0), (Composure, 11.0)],
            ..ANY
        }
    ),
    row!(
        S::ShooterOnTheMove,
        Group::Shots,
        "Shooter on the move",
        "Shoots without penalty straight off a sprint around a screen, and wears his defender out doing it.",
        "Use a fit, disciplined chaser, or switch the screens.",
        Needs {
            attrs: &[(ThreePoint, 13.0), (Stamina, 12.0), (Speed, 12.0)],
            height_max: 206.0,
            ..ANY
        }
    ),
    row!(
        S::Teardrop,
        Group::Shots,
        "Teardrop",
        "Releases a floater early over a waiting big man, so the rim protector's contest counts for little.",
        "Bring the big man up to the level of the screen so there is no room to float it.",
        Needs {
            attrs: &[(CloseShot, 12.0), (Handling, 10.0), (Speed, 11.0)],
            height_max: 196.0,
            ..ANY
        }
    ),
    row!(
        S::FingerRoll,
        Group::Shots,
        "Finger roll",
        "A high, soft finish at full stretch that goes over shot blockers on the drive.",
        "Meet him early and body him before the gather.",
        Needs {
            attrs: &[(CloseShot, 13.0), (Handling, 10.0), (Vertical, 11.0)],
            height_min: 196.0,
            ape_min: 8.0,
            ..ANY
        }
    ),
    row!(
        S::MidRangePullUp,
        Group::Shots,
        "Mid-range pull-up",
        "His pull-up jumpers from the elbows are as good as his open ones, which punishes a big man who drops back.",
        "Do not drop. Switch or hedge on the screen.",
        Needs {
            attrs: &[(MidRange, 14.0), (Handling, 12.0), (Iq, 11.0)],
            ..ANY
        }
    ),
    row!(
        S::UnderhandFreeThrows,
        Group::Shots,
        "Underhand free throws",
        "Shoots his free throws underhand and makes a very high share of them whatever his shooting says, though young players may refuse to do it.",
        "None. Do not foul him.",
        Needs {
            attrs: &[(FreeThrow, 11.0)],
            ..ANY
        }
    ),
    row!(
        S::KillerCrossover,
        Group::OffTheDribble,
        "Killer crossover",
        "His first crossover throws the defender's balance twice as far as usual, and ankles are the usual price.",
        "Give a step and keep him in front, ideally with a defender who has first-class footwork.",
        Needs {
            attrs: &[(Handling, 14.0), (Footwork, 12.0), (Speed, 12.0)],
            height_max: 198.0,
            ..ANY
        }
    ),
    row!(
        S::ChangeOfPace,
        Group::OffTheDribble,
        "Change of pace",
        "Hesitation moves freeze the defender for a moment before the drive.",
        "Stay off his hip and send him to his weaker hand.",
        Needs {
            attrs: &[(Handling, 14.0), (Speed, 11.0), (Iq, 11.0)],
            ..ANY
        }
    ),
    row!(
        S::LongStrideGather,
        Group::OffTheDribble,
        "Long-stride gather",
        "Picks the ball up near the free-throw line and still reaches the rim, so the last help defender has less time to arrive.",
        "Build a wall: three defenders in the lane before he starts.",
        Needs {
            attrs: &[(Speed, 12.0), (Strength, 11.0), (Handling, 9.0)],
            height_min: 200.0,
            ape_min: 7.0,
            ..ANY
        }
    ),
    row!(
        S::SideStep,
        Group::OffTheDribble,
        "Side step",
        "A two-step change of direction at the rim that takes him around a set defender, and charges against him are rarer.",
        "Stay vertical and make him finish over length.",
        Needs {
            attrs: &[
                (Handling, 11.0),
                (CloseShot, 12.0),
                (Footwork, 12.0),
                (Iq, 11.0)
            ],
            ..ANY
        }
    ),
    row!(
        S::SpinMove,
        Group::OffTheDribble,
        "Spin move",
        "A reverse pivot off the dribble that beats a defender who has cut off the drive.",
        "Wait for the spin with a second defender on that side, and strip the ball as he turns his back.",
        Needs {
            attrs: &[(Handling, 12.0), (Footwork, 12.0), (Speed, 11.0)],
            ..ANY
        }
    ),
    row!(
        S::WrongFootFinish,
        Group::OffTheDribble,
        "Wrong-foot finish",
        "Finishes layups off either foot and either hand with no gather step, so blocks against him are rare.",
        "Make body contact before the shot.",
        Needs {
            attrs: &[
                (CloseShot, 13.0),
                (Handling, 12.0),
                (Iq, 12.0),
                (Footwork, 11.0)
            ],
            ..ANY
        }
    ),
    row!(
        S::PowerSpin,
        Group::OffTheDribble,
        "Power spin",
        "A drop step and spin with his weight behind it: the post defender is moved, not beaten.",
        "Use a very heavy defender, or foul him if he cannot shoot free throws.",
        Needs {
            attrs: &[(PostMoves, 12.0), (Strength, 14.0), (Footwork, 9.0)],
            weight_min: 100.0,
            ..ANY
        }
    ),
    row!(
        S::UpAndUnder,
        Group::OffTheDribble,
        "Up and under",
        "A shot fake in the post that gets the defender in the air, then a step through, which draws fouls.",
        "Use a disciplined defender who stays down.",
        Needs {
            attrs: &[(PostMoves, 12.0), (Iq, 11.0), (Footwork, 11.0)],
            height_min: 198.0,
            ..ANY
        }
    ),
    row!(
        S::PostFootwork,
        Group::OffTheDribble,
        "Post footwork",
        "Chains fakes and pivots in the post: every fake the defender bites on makes the next one stronger.",
        "Do not play him one on one. Double on the catch.",
        Needs {
            attrs: &[(PostMoves, 14.0), (Footwork, 14.0)],
            height_min: 198.0,
            ..ANY
        }
    ),
    row!(
        S::JabStep,
        Group::OffTheDribble,
        "Jab step",
        "From a standing start, a jab that moves the defender's balance before any dribble.",
        "Crowd him so there is no room to jab.",
        Needs {
            attrs: &[(Handling, 11.0), (MidRange, 12.0), (Footwork, 12.0)],
            ..ANY
        }
    ),
    row!(
        S::NoLookPass,
        Group::Passing,
        "No-look pass",
        "His passes are read late: fewer are intercepted, and the receiver's defender reacts late.",
        "Play the passing lanes by position, not by his eyes.",
        Needs {
            attrs: &[(Passing, 15.0), (Iq, 13.0), (Handling, 11.0)],
            ..ANY
        }
    ),
    row!(
        S::OneHandedSling,
        Group::Passing,
        "One-handed sling",
        "Throws full-speed one-handed passes off the dribble or the rebound with no gather, which opens passes others cannot throw in time.",
        "Deny the cutters and make him a scorer.",
        Needs {
            attrs: &[(Passing, 14.0), (Handling, 11.0), (Iq, 12.0)],
            hand_min: 20.8,
            ..ANY
        }
    ),
    row!(
        S::FullCourtOutlet,
        Group::Passing,
        "Full-court outlet",
        "After a defensive rebound he throws a long pass that starts a fast break before the defense is back.",
        "Send two players back on every shot, and do not crash the boards.",
        Needs {
            attrs: &[(Passing, 11.0), (DefRebound, 13.0)],
            height_min: 196.0,
            ..ANY
        }
    ),
    row!(
        S::TouchPass,
        Group::Passing,
        "Touch pass",
        "Moves the ball on without catching it, so the defense has no time to rotate to the next man.",
        "Stay home on shooters and do not help off the next pass.",
        Needs {
            attrs: &[(Passing, 13.0), (Iq, 13.0), (Handling, 10.0)],
            ..ANY
        }
    ),
    row!(
        S::LobThrower,
        Group::Passing,
        "Lob thrower",
        "His lobs to a rolling big man arrive where only the big man can catch them.",
        "Drop the big man early and tag the roller from the weak side.",
        Needs {
            attrs: &[(Passing, 14.0), (Iq, 12.0), (Handling, 12.0)],
            height_max: 200.0,
            ..ANY
        }
    ),
    row!(
        S::PassFake,
        Group::Passing,
        "Pass fake",
        "A ball fake that moves a help defender out of the lane before the real pass or drive.",
        "Use help defenders with discipline.",
        Needs {
            attrs: &[(Passing, 13.0), (Handling, 12.0), (Iq, 12.0)],
            ..ANY
        }
    ),
    row!(
        S::BigManAsHub,
        Group::Passing,
        "Big man as hub",
        "Runs handoffs and passes from the high post: cutters around him get better looks, and a handoff works as a screen.",
        "Sag off him if he cannot shoot, and deny the handoff.",
        Needs {
            attrs: &[(Passing, 14.0), (Iq, 13.0), (PostMoves, 9.0)],
            height_min: 203.0,
            ..ANY
        }
    ),
    row!(
        S::ShortRollPlaymaker,
        Group::Passing,
        "Short-roll playmaker",
        "Catches the ball in the middle after a trap on the ball handler and makes the right four-on-three read.",
        "Do not trap the ball handler.",
        Needs {
            attrs: &[
                (Passing, 12.0),
                (Iq, 14.0),
                (DefRebound, 10.0),
                (Handling, 10.0)
            ],
            height_min: 198.0,
            ..ANY
        }
    ),
    row!(
        S::PosterDunk,
        Group::AtTheRim,
        "Poster dunk",
        "Dunks through contact: a defender in the way is dunked on, which hits his focus and lifts the home crowd.",
        "Take the charge early, or foul before he leaves the floor.",
        Needs {
            attrs: &[(Vertical, 14.0), (Strength, 11.0)],
            height_min: 193.0,
            ..ANY
        }
    ),
    row!(
        S::LobFinisher,
        Group::AtTheRim,
        "Lob finisher",
        "Catches and finishes anything thrown near the rim, a constant threat rolling to the basket.",
        "Put a body on him before he rolls.",
        Needs {
            attrs: &[(Vertical, 14.0), (CloseShot, 11.0)],
            height_min: 198.0,
            ..ANY
        }
    ),
    row!(
        S::PutbackInstinct,
        Group::AtTheRim,
        "Putback instinct",
        "Rebounds his own misses and his team's misses near the rim far more often than his rating says.",
        "Box him out with two players.",
        Needs {
            attrs: &[(OffRebound, 14.0), (Iq, 11.0), (Vertical, 10.0)],
            height_min: 195.0,
            ..ANY
        }
    ),
    row!(
        S::FloaterOrLob,
        Group::AtTheRim,
        "Floater or lob",
        "In the lane with a big man waiting, the floater and the lob look the same until the release, so the big man has to guess.",
        "Stay between the two and make someone else make the play.",
        Needs {
            attrs: &[
                (Passing, 12.0),
                (CloseShot, 12.0),
                (Handling, 12.0),
                (Iq, 12.0)
            ],
            height_max: 192.0,
            ..ANY
        }
    ),
    row!(
        S::RipThrough,
        Group::DrawingFouls,
        "Rip through",
        "Swings the ball through a reaching defender's arms to draw a foul.",
        "Keep your hands back and do not reach.",
        Needs {
            attrs: &[(Handling, 12.0), (FreeThrow, 12.0), (Iq, 12.0)],
            height_min: 195.0,
            ..ANY
        }
    ),
    row!(
        S::PumpFake,
        Group::DrawingFouls,
        "Pump fake",
        "Gets defenders in the air and jumps into them, drawing shooting fouls on jumpers.",
        "Use defenders who stay down.",
        Needs {
            attrs: &[
                (FreeThrow, 13.0),
                (Iq, 12.0),
                (Handling, 10.0),
                (Vertical, 11.0)
            ],
            ..ANY
        }
    ),
    row!(
        S::SellsContact,
        Group::DrawingFouls,
        "Sells contact",
        "Wins calls that were not there. With a tight whistle it pays; with a crew that lets them play, he stays on the floor and play goes on.",
        "A referee who is not fooled.",
        Needs {
            attrs: &[(Iq, 13.0), (Composure, 10.0)],
            height_min: 200.0,
            ..ANY
        }
    ),
    row!(
        S::ChaseDownBlock,
        Group::Defense,
        "Chase-down block",
        "Blocks fast-break layups from behind, so a lost ball is not an automatic two points.",
        "Finish with a dunk, or pull up.",
        Needs {
            attrs: &[(Block, 13.0), (Speed, 13.0), (Vertical, 13.0)],
            height_min: 196.0,
            ..ANY
        }
    ),
    row!(
        S::StraightUp,
        Group::Defense,
        "Straight up",
        "Jumps vertically and takes contact without fouling: shots at the rim are harder and he stays out of foul trouble.",
        "Pull him away from the basket with a shooting big man.",
        Needs {
            attrs: &[
                (Block, 12.0),
                (InteriorD, 13.0),
                (Iq, 11.0),
                (Strength, 11.0)
            ],
            height_min: 205.0,
            weight_min: 100.0,
            ..ANY
        }
    ),
    row!(
        S::ShotBlockingPresence,
        Group::Defense,
        "Shot-blocking presence",
        "Attackers he has blocked hesitate, so there are fewer drives at him for a while.",
        "Take mid-range shots and floaters.",
        Needs {
            attrs: &[(Block, 15.0), (InteriorD, 12.0), (Vertical, 11.0)],
            height_min: 203.0,
            ape_min: 8.0,
            ..ANY
        }
    ),
    row!(
        S::TakesTheCharge,
        Group::Defense,
        "Takes the charge",
        "Gets set in the driver's path early and often, and draws several times the usual number of charges.",
        "Take pull-up jumpers, or use a side step.",
        Needs {
            attrs: &[(Iq, 13.0), (Footwork, 12.0), (WorkRate, 12.0)],
            ..ANY
        }
    ),
    row!(
        S::OnBallPest,
        Group::Defense,
        "On-ball pest",
        "Pressures the ball handler the full length of the floor without fouling, which costs him seconds and energy every possession.",
        "Let someone else bring the ball up.",
        Needs {
            attrs: &[
                (PerimeterD, 14.0),
                (Stamina, 14.0),
                (Speed, 12.0),
                (Footwork, 13.0)
            ],
            height_max: 196.0,
            ..ANY
        }
    ),
    row!(
        S::BigHands,
        Group::Defense,
        "Big hands",
        "Deflects passes and strips drivers at arm's length, so he steals without gambling out of position.",
        "Keep the ball away from his side of the floor.",
        Needs {
            attrs: &[(Steal, 13.0), (PerimeterD, 12.0)],
            hand_min: 20.5,
            ..ANY
        }
    ),
    row!(
        S::StripOnTheGather,
        Group::Defense,
        "Strip on the gather",
        "Takes the ball from big men as they bring it down to gather.",
        "Use big men who keep the ball high.",
        Needs {
            attrs: &[(Steal, 13.0), (Iq, 12.0), (Handling, 10.0)],
            ..ANY
        }
    ),
    row!(
        S::BrickWallScreens,
        Group::ScreensAndBoards,
        "Brick-wall screens",
        "Defenders who hit his screens are held up longer, and it is legal.",
        "Go under the screen, or switch.",
        Needs {
            attrs: &[(Strength, 14.0)],
            height_min: 203.0,
            weight_min: 105.0,
            ..ANY
        }
    ),
    row!(
        S::SealForTheDriver,
        Group::ScreensAndBoards,
        "Seal for the driver",
        "After a ball screen he turns and screens the help defender, which opens the lane.",
        "Help from the other side.",
        Needs {
            attrs: &[(Strength, 12.0), (Iq, 12.0)],
            height_min: 203.0,
            ..ANY
        }
    ),
    row!(
        S::ReadsTheBounce,
        Group::ScreensAndBoards,
        "Reads the bounce",
        "Gets to where a miss will land before anyone else, and rebounds far above his size.",
        "Use two players to box him out, which frees a teammate.",
        Needs {
            attrs: &[(DefRebound, 13.0), (OffRebound, 13.0), (Iq, 13.0)],
            height_min: 193.0,
            ..ANY
        }
    ),
    row!(
        S::BoxesOutForOthers,
        Group::ScreensAndBoards,
        "Boxes out for others",
        "Takes two rebounders out of the play so a teammate gets the ball, while his own numbers stay low.",
        "Nothing direct. Crash from the perimeter.",
        Needs {
            attrs: &[
                (Strength, 13.0),
                (DefRebound, 10.0),
                (Iq, 11.0),
                (WorkRate, 12.0)
            ],
            weight_min: 100.0,
            ..ANY
        }
    ),
    row!(
        S::BigShotTaker,
        Group::Moments,
        "Big-shot taker",
        "Does not drop off in the last two minutes of a close game, when most players' composure is tested.",
        "Deny him the ball late.",
        Needs {
            attrs: &[(Composure, 15.0), (ThreePoint, 12.0)],
            ..ANY
        }
    ),
];

impl Signature {
    pub const COUNT: usize = 50;
    pub const ALL: [Signature; Signature::COUNT] = [
        S::SweepingHook,
        S::OneLeggedFadeaway,
        S::OffBalanceLeaner,
        S::TurnaroundFadeaway,
        S::GlassSpecialist,
        S::StepBackThree,
        S::LogoRange,
        S::QuickRelease,
        S::ShooterOnTheMove,
        S::Teardrop,
        S::FingerRoll,
        S::MidRangePullUp,
        S::UnderhandFreeThrows,
        S::KillerCrossover,
        S::ChangeOfPace,
        S::LongStrideGather,
        S::SideStep,
        S::SpinMove,
        S::WrongFootFinish,
        S::PowerSpin,
        S::UpAndUnder,
        S::PostFootwork,
        S::JabStep,
        S::NoLookPass,
        S::OneHandedSling,
        S::FullCourtOutlet,
        S::TouchPass,
        S::LobThrower,
        S::PassFake,
        S::BigManAsHub,
        S::ShortRollPlaymaker,
        S::PosterDunk,
        S::LobFinisher,
        S::PutbackInstinct,
        S::FloaterOrLob,
        S::RipThrough,
        S::PumpFake,
        S::SellsContact,
        S::ChaseDownBlock,
        S::StraightUp,
        S::ShotBlockingPresence,
        S::TakesTheCharge,
        S::OnBallPest,
        S::BigHands,
        S::StripOnTheGather,
        S::BrickWallScreens,
        S::SealForTheDriver,
        S::ReadsTheBounce,
        S::BoxesOutForOthers,
        S::BigShotTaker,
    ];

    pub fn info(self) -> &'static Info {
        &TABLE[self as usize]
    }

    /// Plain name.
    pub fn name(self) -> &'static str {
        self.info().name
    }

    /// One sentence on what it does.
    pub fn description(self) -> &'static str {
        self.info().does
    }

    /// What a coach can do about it.
    pub fn answer(self) -> &'static str {
        self.info().answer
    }

    pub fn group(self) -> Group {
        self.info().group
    }

    /// The prerequisites as a list of plain phrases, for designers and tests.
    pub fn needs_text(self) -> String {
        let n = &self.info().needs;
        let mut parts: Vec<String> = n
            .attrs
            .iter()
            .map(|(a, v)| format!("{} {}", a.label(), v))
            .collect();
        if n.height_min > 0.0 {
            parts.push(format!("height >= {} cm", n.height_min));
        }
        if n.height_max < 999.0 {
            parts.push(format!("height <= {} cm", n.height_max));
        }
        if n.weight_min > 0.0 {
            parts.push(format!("weight >= {} kg", n.weight_min));
        }
        if n.ape_min > -99.0 {
            parts.push(format!("wingspan - height >= {} cm", n.ape_min));
        }
        if n.hand_min > 0.0 {
            parts.push(format!("hand >= {} cm", n.hand_min));
        }
        parts.join(", ")
    }

    fn body_fits(self, b: &Body) -> bool {
        let n = &self.info().needs;
        b.height_cm >= n.height_min
            && b.height_cm <= n.height_max
            && b.weight_kg >= n.weight_min
            && b.ape_index_cm() >= n.ape_min
            && b.hand_length_cm >= n.hand_min
    }

    fn attrs_meet(self, a: &Attributes, slack: impl Fn(Attr) -> f32) -> bool {
        self.info()
            .needs
            .attrs
            .iter()
            .all(|&(attr, min)| a[attr] >= min - slack(attr))
    }

    /// His attributes if he reaches his potential: every attribute lifted by
    /// the gap between his potential and his level today.
    fn at_potential(p: &Player) -> Attributes {
        let gap = (p.potential - p.overall()).max(0.0);
        let mut a = p.attrs.clone();
        for v in a.0.iter_mut() {
            *v = (*v + gap).min(20.0);
        }
        a
    }

    /// Could this body and skill set plausibly have the move, at the level
    /// he will reach? This is the bar for giving it to him at birth.
    pub fn plausible(self, p: &Player) -> bool {
        self.body_fits(&p.body) && self.attrs_meet(&Self::at_potential(p), |_| 0.0)
    }

    /// How well he suits it: 1 at the bar, more when he is well above it.
    fn fit(self, p: &Player) -> f32 {
        let a = Self::at_potential(p);
        let n = self.info().needs.attrs;
        let over: f32 = n.iter().map(|&(x, min)| (a[x] - min).max(0.0)).sum();
        1.0 + over / n.len().max(1) as f32
    }

    /// Can his body still do it today? Each physical attribute the move
    /// needs may have worn a little below the level that made it plausible.
    /// A move that needs only skill is always live.
    pub fn live(self, p: &Player) -> bool {
        self.body_fits(&p.body)
            && self.attrs_meet(&p.attrs, |a| if a.is_physical() { WEAR_ROOM } else { 99.0 })
    }

    /// True if the move leans on physical attributes at all, so that age or
    /// injury can take it away.
    pub fn is_body_dependent(self) -> bool {
        self.info().needs.attrs.iter().any(|(a, _)| a.is_physical())
    }
}

fn seed(id: PlayerId, name: &str) -> u64 {
    // FNV-1a over the name, mixed with the id.
    let mut h: u64 = 0xCBF2_9CE4_8422_2325 ^ ((id as u64) << 20);
    for b in name.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01B3);
    }
    h ^ 0x516E_A7E5
}

/// Chance that a player of this potential has at least one signature.
fn chance_any(potential: f32) -> f32 {
    (0.12 + (potential - 9.0) * 0.03).clamp(0.04, 0.5)
}

/// What a new player is born with: usually nothing, rarely one move, very
/// rarely two or three. Only moves he could plausibly do, weighted by how
/// well he suits them. Deterministic in the player's id and name.
pub fn roll(p: &Player) -> Vec<Signature> {
    let mut rng = Rng::new(seed(p.id, &p.name));
    if !rng.chance(chance_any(p.potential)) {
        return Vec::new();
    }
    let mut want = 1;
    if rng.chance(0.18) {
        want += 1;
        if rng.chance(0.04) {
            want += 1;
        }
    }
    let mut picked: Vec<Signature> = Vec::new();
    while picked.len() < want.min(MAX_SIGNATURES) {
        let pool: Vec<Signature> = Signature::ALL
            .into_iter()
            .filter(|s| !picked.contains(s) && s.plausible(p))
            .collect();
        if pool.is_empty() {
            break;
        }
        let weights: Vec<f32> = pool.iter().map(|s| s.fit(p)).collect();
        picked.push(pool[rng.weighted(&weights)]);
    }
    picked
}

impl Player {
    /// Give a newly made player his innate signatures. Call once, after his
    /// age and potential are final.
    pub fn assign_signatures(&mut self) {
        self.signatures = roll(self);
        self.signatures_rolled = true;
    }

    /// His signatures he can still perform today.
    pub fn live_signatures(&self) -> Vec<Signature> {
        self.signatures
            .iter()
            .copied()
            .filter(|s| s.live(self))
            .collect()
    }

    /// Modest extra fame for a player with a move of his own: 10% per
    /// signature, `fame.rs`.
    pub fn signature_fame_mult(&self) -> f32 {
        1.0 + 0.10 * self.signatures.len().min(MAX_SIGNATURES) as f32
    }
}

use crate::league::World;

impl World {
    /// Is he in the managed club's first team or reserves?
    pub fn is_my_player(&self, p: PlayerId) -> bool {
        self.watch.is_some_and(|me| {
            let t = &self.teams[me];
            t.roster.contains(&p) || t.reserves.contains(&p)
        })
    }

    /// The signatures of `p` the managed club may know about: all of them
    /// for its own players, for anyone else those scouting has revealed
    /// (`Scouting::knows_signature`). Never an unrevealed one.
    pub fn known_signatures(&self, p: PlayerId) -> Vec<Signature> {
        let Some(pl) = self.players.get(p as usize) else {
            return Vec::new();
        };
        if self.is_my_player(p) {
            return pl.signatures.clone();
        }
        pl.signatures
            .iter()
            .copied()
            .filter(|&s| self.scouting.knows_signature(p, s))
            .collect()
    }

    /// Saves from before Footwork and signatures: pad the missing attribute
    /// from his body and speed, and give signatures to anyone who never had
    /// them rolled, from his id and name as a new player would get them.
    /// Runs once on load; later loads find everything in place.
    pub(crate) fn repair_new_fields(&mut self) {
        for p in &mut self.players {
            if p.attrs[Attr::Footwork] < 0.5 {
                p.attrs[Attr::Footwork] = crate::player::roll_footwork(p.id, &p.attrs, &p.body);
            }
            if p.off_hand < 0.5 {
                p.off_hand = crate::player::roll_off_hand(p.id, &p.attrs);
            }
            if !p.signatures_rolled {
                p.assign_signatures();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_matches_the_list() {
        for (i, s) in Signature::ALL.iter().enumerate() {
            assert_eq!(TABLE[i].sig, *s, "row {i}");
            assert_eq!(*s as usize, i);
            let info = s.info();
            assert!(
                !info.name.is_empty() && info.does.ends_with('.') && info.answer.ends_with('.')
            );
            assert!(!info.does.contains('\u{2014}') && !info.answer.contains('\u{2014}'));
        }
    }
}
