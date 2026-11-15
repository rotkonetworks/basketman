use serde::{Deserialize, Serialize};

use crate::court::{HALF_WIDTH, Vec2, v};
use crate::player::{Attr, Player, PlayerId, Position};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OffenseSet {
    Motion,
    PickAndRoll,
    PostUp,
    Isolation,
    FiveOut,
}

/// How an offensive set bends the possession model. Multipliers around 1.0.
#[derive(Copy, Clone, Debug)]
pub struct OffenseProfile {
    pub rim: f32,
    pub mid: f32,
    pub three: f32,
    pub turnover: f32,
    /// Base probability a shot comes off a pass.
    pub assist: f32,
    pub guard_usage: f32,
    pub big_usage: f32,
    /// Extra usage for the `Role::PrimaryOption` player.
    pub star_usage: f32,
}

impl OffenseSet {
    pub const ALL: [OffenseSet; 5] = [
        OffenseSet::Motion,
        OffenseSet::PickAndRoll,
        OffenseSet::PostUp,
        OffenseSet::Isolation,
        OffenseSet::FiveOut,
    ];

    pub fn label(self) -> &'static str {
        match self {
            OffenseSet::Motion => "Motion",
            OffenseSet::PickAndRoll => "Pick and roll",
            OffenseSet::PostUp => "Post up",
            OffenseSet::Isolation => "Isolation",
            OffenseSet::FiveOut => "Five out",
        }
    }

    /// What it is, in a coach's words: how the team plays, who gets the
    /// shots, what it needs.
    pub fn describe(self) -> &'static str {
        match self {
            OffenseSet::Motion => {
                "Everybody moves, passes and cuts; the ball finds the open man. A balanced \
                 spread of shots and nobody carries the load. Needs passing and basketball \
                 IQ across the five. The safe choice when no one player stands out."
            }
            OffenseSet::PickAndRoll => {
                "Your ball handler works off a screen from the big man, again and again. \
                 More drives and rolls to the rim, more of the ball for your guard and your \
                 best scorer, a few more turnovers. Needs a real ball handler and a big who \
                 finishes. A defense that switches every screen takes most of it away."
            }
            OffenseSet::PostUp => {
                "Throw it inside to your strongest post player and play off him. A lot at \
                 the rim, few threes, the big men take the shots. Needs a post scorer with \
                 size on his man. A 2-3 zone or a pack line crowds exactly where you want to go."
            }
            OffenseSet::Isolation => {
                "Clear a side of the floor for your primary option and let him beat his man. \
                 Few assists, few turnovers, a lot of mid-range shots. Lives and dies with \
                 one player: wonderful with a star, hopeless without one."
            }
            OffenseSet::FiveOut => {
                "All five outside the three-point line. The most threes and the fewest \
                 mid-range shots, and an empty lane to drive and cut into. Needs shooters at \
                 every position. The answer to a zone packed around the basket."
            }
        }
    }

    pub fn profile(self) -> OffenseProfile {
        let p = |rim, mid, three, turnover, assist, guard_usage, big_usage, star_usage| {
            OffenseProfile {
                rim,
                mid,
                three,
                turnover,
                assist,
                guard_usage,
                big_usage,
                star_usage,
            }
        };
        match self {
            OffenseSet::Motion => p(1.0, 1.0, 1.0, 1.0, 0.62, 1.0, 1.0, 1.0),
            OffenseSet::PickAndRoll => p(1.15, 0.9, 1.0, 1.05, 0.58, 1.2, 1.0, 1.15),
            OffenseSet::PostUp => p(1.3, 1.0, 0.75, 1.0, 0.50, 0.85, 1.35, 1.1),
            OffenseSet::Isolation => p(1.05, 1.25, 0.9, 0.9, 0.38, 1.1, 0.9, 1.5),
            OffenseSet::FiveOut => p(0.9, 0.6, 1.45, 1.0, 0.66, 1.0, 1.0, 0.9),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DefenseScheme {
    ManToMan,
    SwitchEverything,
    Zone23,
    Zone32,
    PackLine,
}

/// How a defensive scheme bends the opponent's possession. `*_freq`, turnover,
/// rebound and foul are multipliers; `*_pct` are additive to make probability.
#[derive(Copy, Clone, Debug)]
pub struct DefenseProfile {
    pub opp_rim_freq: f32,
    pub opp_three_freq: f32,
    pub opp_rim_pct: f32,
    pub opp_three_pct: f32,
    pub force_turnover: f32,
    pub def_rebound: f32,
    pub foul: f32,
}

impl DefenseScheme {
    pub const ALL: [DefenseScheme; 5] = [
        DefenseScheme::ManToMan,
        DefenseScheme::SwitchEverything,
        DefenseScheme::Zone23,
        DefenseScheme::Zone32,
        DefenseScheme::PackLine,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DefenseScheme::ManToMan => "Man to man",
            DefenseScheme::SwitchEverything => "Switch everything",
            DefenseScheme::Zone23 => "2-3 zone",
            DefenseScheme::Zone32 => "3-2 zone",
            DefenseScheme::PackLine => "Pack line",
        }
    }

    /// What it takes away and what it gives up.
    pub fn describe(self) -> &'static str {
        match self {
            DefenseScheme::ManToMan => {
                "Every player guards his own man. No built-in weakness and no built-in help: \
                 it is exactly as good as your five defenders."
            }
            DefenseScheme::SwitchEverything => {
                "Swap men on every screen. Takes away the pick and roll and the open three \
                 off a screen, but leaves small men on big ones: a little softer at the rim \
                 and on the defensive boards. Best with five players of similar size."
            }
            DefenseScheme::Zone23 => {
                "Two up top, three along the baseline. Protects the basket and saves fouls; \
                 gives up open threes and defensive rebounds. Use it on teams that live in \
                 the paint or cannot shoot. Shooters will punish it."
            }
            DefenseScheme::Zone32 => {
                "Three across the perimeter, two behind. Takes away threes and forces a few \
                 more turnovers; soft at the rim and on the boards. Use it on teams that \
                 live on the three."
            }
            DefenseScheme::PackLine => {
                "Man to man, but everyone off the ball sags toward the lane. Fewer drives, \
                 fewer fouls, strong on the defensive boards; forces fewer turnovers and \
                 allows more threes. Use it on teams that drive and post up."
            }
        }
    }

    pub fn profile(self) -> DefenseProfile {
        let p = |opp_rim_freq,
                 opp_three_freq,
                 opp_rim_pct,
                 opp_three_pct,
                 force_turnover,
                 def_rebound,
                 foul| {
            DefenseProfile {
                opp_rim_freq,
                opp_three_freq,
                opp_rim_pct,
                opp_three_pct,
                force_turnover,
                def_rebound,
                foul,
            }
        };
        match self {
            DefenseScheme::ManToMan => p(1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0),
            DefenseScheme::SwitchEverything => p(1.0, 1.0, 0.015, -0.015, 1.0, 0.97, 1.0),
            DefenseScheme::Zone23 => p(0.8, 1.25, -0.03, 0.01, 0.95, 0.93, 0.85),
            DefenseScheme::Zone32 => p(1.1, 0.85, 0.01, -0.02, 1.05, 0.92, 0.9),
            DefenseScheme::PackLine => p(0.82, 1.15, -0.02, 0.0, 0.85, 1.05, 0.9),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Press {
    None,
    ThreeQuarter,
    FullCourt,
}

#[derive(Copy, Clone, Debug)]
pub struct PressProfile {
    pub turnover: f32,
    /// Easy baskets conceded when the press is broken (additive).
    pub opp_rim_pct: f32,
    /// Fatigue multiplier for the pressing team.
    pub fatigue: f32,
}

impl Press {
    pub const ALL: [Press; 3] = [Press::None, Press::ThreeQuarter, Press::FullCourt];

    pub fn label(self) -> &'static str {
        match self {
            Press::None => "No press",
            Press::ThreeQuarter => "Three-quarter court",
            Press::FullCourt => "Full court",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Press::None => "Fall back and defend in the half court. Costs nothing.",
            Press::ThreeQuarter => {
                "Pick them up before half court. About a tenth more turnovers forced; your \
                 players tire a tenth faster and a broken press gives up the odd layup."
            }
            Press::FullCourt => {
                "Pressure from the baseline. About a fifth more turnovers forced; your \
                 players tire a quarter faster and a broken press is a layup. For chasing a \
                 game, or for a deep, fit squad against poor ball handlers."
            }
        }
    }

    pub fn profile(self) -> PressProfile {
        match self {
            Press::None => PressProfile {
                turnover: 1.0,
                opp_rim_pct: 0.0,
                fatigue: 1.0,
            },
            Press::ThreeQuarter => PressProfile {
                turnover: 1.1,
                opp_rim_pct: 0.01,
                fatigue: 1.1,
            },
            Press::FullCourt => PressProfile {
                turnover: 1.22,
                opp_rim_pct: 0.03,
                fatigue: 1.25,
            },
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    PrimaryOption,
    Secondary,
    RolePlayer,
    Limited,
}

impl Role {
    pub const ALL: [Role; 4] = [
        Role::PrimaryOption,
        Role::Secondary,
        Role::RolePlayer,
        Role::Limited,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Role::PrimaryOption => "Primary option",
            Role::Secondary => "Secondary",
            Role::RolePlayer => "Role player",
            Role::Limited => "Limited",
        }
    }

    pub fn usage(self) -> f32 {
        match self {
            Role::PrimaryOption => 1.6,
            Role::Secondary => 1.2,
            Role::RolePlayer => 0.9,
            Role::Limited => 0.5,
        }
    }
}

/// What one player is told to do with the ball.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Brief {
    #[default]
    Free,
    /// Take the shot when it is there, even an ordinary one.
    Shoot,
    /// Keep the ball moving; shoot only the good ones.
    Pass,
    /// Put it on the floor and go at the basket.
    Attack,
}

impl Brief {
    pub const ALL: [Brief; 4] = [Brief::Free, Brief::Shoot, Brief::Pass, Brief::Attack];

    pub fn label(self) -> &'static str {
        match self {
            Brief::Free => "Play his game",
            Brief::Shoot => "Look for his shot",
            Brief::Pass => "Move the ball",
            Brief::Attack => "Attack the rim",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RotationSlot {
    pub player: PlayerId,
    /// Target minutes per game. The engine normalises the column, so it does
    /// not have to add up exactly.
    pub minutes: u8,
    pub role: Role,
    #[serde(default)]
    pub brief: Brief,
    /// Where he is used, if not at his own position. He plays it as well
    /// as he knows it, and learns it by being there.
    #[serde(default)]
    pub position: Option<Position>,
}

/// The five places on the coach's board, in the order the engine fills
/// them: the man who brings the ball up, three others in rotation order,
/// and the big man.
pub const BOARD_ROLES: [&str; 5] = ["Ball handler", "Guard", "Wing", "Forward", "Big"];

/// A half-court set drawn by hand: where each of the five stands when the
/// ball comes over, and the run each makes once it is there. Attack frame,
/// metres, basket at `court::HOOP`; the engine mirrors it at random so it
/// is run to both sides.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Board {
    /// The offense it was drawn for. Under any other it is ignored.
    pub set: OffenseSet,
    pub spots: [Vec2; 5],
    pub runs: [Option<Vec2>; 5],
}

impl Board {
    /// The stock positions of a set, nobody running anywhere.
    pub fn stock(set: OffenseSet) -> Board {
        Board {
            set,
            spots: crate::engine::template(set),
            runs: [None; 5],
        }
    }

    /// Keep a point on the half court a set is run in.
    pub fn clamp(p: Vec2) -> Vec2 {
        v(
            p.x.clamp(-HALF_WIDTH + 0.5, HALF_WIDTH - 0.5),
            p.y.clamp(0.6, 11.5),
        )
    }
}

/// Everything the coach decides. Sliders are 1..=20 with 10 as neutral.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tactics {
    pub offense: OffenseSet,
    pub defense: DefenseScheme,
    pub press: Press,
    pub pace: u8,
    pub three_bias: u8,
    /// Offensive rebounding effort; costs transition defense.
    pub crash_boards: u8,
    /// Gambling for steals; costs fouls.
    pub aggression: u8,
    pub rotation: Vec<RotationSlot>,
    /// Hand-drawn positions and runs for the offense, if the coach made any.
    #[serde(default)]
    pub board: Option<Board>,
}

/// How many tactics a team can keep in its playbook.
pub const MAX_TACTICS: usize = 3;

/// Extra weekly gain for the tactic actually used in matches.
const MATCH_BONUS: f32 = 0.03;

/// One entry in a team's playbook. Familiarity belongs to the *system*
/// (offense, defense, press); sliders and the rotation can change freely.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TacticSlot {
    pub name: String,
    pub tactics: Tactics,
    /// 0..=1, how well the squad knows the system it last drilled.
    familiarity: f32,
    drilled: (OffenseSet, DefenseScheme, Press),
}

impl TacticSlot {
    pub fn new(name: impl Into<String>, tactics: Tactics, familiarity: f32) -> Self {
        let drilled = (tactics.offense, tactics.defense, tactics.press);
        TacticSlot {
            name: name.into(),
            tactics,
            familiarity: familiarity.clamp(0.0, 1.0),
            drilled,
        }
    }

    /// Familiarity with the system as currently configured: switching the
    /// offense or defense since the last training week each costs 40% of what
    /// was built up, the press 20%.
    pub fn familiarity(&self) -> f32 {
        let t = &self.tactics;
        self.familiarity_with(t.offense, t.defense, t.press)
    }

    /// Familiarity the squad would have if this entry ran the given system.
    pub fn familiarity_with(
        &self,
        offense: OffenseSet,
        defense: DefenseScheme,
        press: Press,
    ) -> f32 {
        let mut keep = 1.0;
        if offense != self.drilled.0 {
            keep -= 0.4;
        }
        if defense != self.drilled.1 {
            keep -= 0.4;
        }
        if press != self.drilled.2 {
            keep -= 0.2;
        }
        self.familiarity * keep
    }

    /// True if the system was changed since the squad last trained it.
    pub fn changed(&self) -> bool {
        (
            self.tactics.offense,
            self.tactics.defense,
            self.tactics.press,
        ) != self.drilled
    }

    fn train_week(&mut self, gain: f32) {
        self.familiarity = (self.familiarity() + gain).min(1.0);
        self.drilled = (
            self.tactics.offense,
            self.tactics.defense,
            self.tactics.press,
        );
    }
}

/// A week of tactical training: drill time is split evenly across the
/// playbook, so three tactics each progress a third as fast as one.
pub fn train_playbook(playbook: &mut [TacticSlot], active: usize, drill: f32) {
    let share = drill / playbook.len().max(1) as f32;
    for (i, slot) in playbook.iter_mut().enumerate() {
        slot.train_week(share + if i == active { MATCH_BONUS } else { 0.0 });
    }
}

impl Tactics {
    /// Sensible default: best player per position starts, next five best come
    /// off the bench, roles by overall rank.
    pub fn auto(roster: &[&Player]) -> Tactics {
        let mut rest: Vec<&Player> = roster.to_vec();
        rest.sort_by(|a, b| b.overall().total_cmp(&a.overall()));

        let mut picked: Vec<&Player> = Vec::new();
        for pos in Position::ALL {
            let i = rest.iter().position(|p| p.position == pos).unwrap_or(0);
            if i < rest.len() {
                picked.push(rest.remove(i));
            }
        }
        let starters = picked.len();
        picked.extend(rest.into_iter().take(5));

        let mut rank: Vec<PlayerId> = picked.iter().map(|p| p.id).collect();
        rank.sort_by(|a, b| {
            let ov = |id: &PlayerId| {
                picked
                    .iter()
                    .find(|p| p.id == *id)
                    .map_or(0.0, |p| p.overall())
            };
            ov(b).total_cmp(&ov(a))
        });

        let rotation = picked
            .iter()
            .enumerate()
            .map(|(i, p)| RotationSlot {
                player: p.id,
                minutes: if i < starters { 30 } else { 10 },
                role: match rank.iter().position(|id| *id == p.id).unwrap_or(9) {
                    0 => Role::PrimaryOption,
                    1 | 2 => Role::Secondary,
                    3..=7 => Role::RolePlayer,
                    _ => Role::Limited,
                },
                brief: Brief::Free,
                position: None,
            })
            .collect();

        Tactics {
            offense: OffenseSet::Motion,
            defense: DefenseScheme::ManToMan,
            press: Press::None,
            pace: 10,
            three_bias: 10,
            crash_boards: 10,
            aggression: 10,
            rotation,
            board: None,
        }
    }

    /// The board in force: the hand-drawn one if it belongs to the current
    /// offense, the stock set otherwise.
    pub fn layout(&self) -> Board {
        match &self.board {
            Some(b) if b.set == self.offense => b.clone(),
            _ => Board::stock(self.offense),
        }
    }

    /// Store an edited board, or forget it if it is the stock set again.
    pub fn draw(&mut self, board: Board) {
        self.board = (board != Board::stock(board.set)).then_some(board);
    }

    /// Which starter is expected at each place on the board. The engine
    /// decides by who is on the floor and how tired, so this is the picture
    /// at tip-off.
    pub fn cast(&self, players: &[Player]) -> [Option<PlayerId>; 5] {
        // The engine lines the five up by position, point guard first.
        let mut five: Vec<(&Player, Role, Position)> = self
            .rotation
            .iter()
            .take(5)
            .map(|s| {
                let p = &players[s.player as usize];
                (p, s.role, s.position.unwrap_or(p.position))
            })
            .collect();
        five.sort_by_key(|(p, _, pos)| (*pos, p.body.height_cm as u32));
        let roles: Vec<Role> = five.iter().map(|x| x.1).collect();
        let five: Vec<&Player> = five.into_iter().map(|x| x.0).collect();
        let mut out = [None; 5];
        if five.len() < 5 {
            return out;
        }
        let best = |score: &dyn Fn(usize) -> f32, except: Option<usize>| {
            (0..5)
                .filter(|&k| Some(k) != except)
                .max_by(|&a, &b| score(a).total_cmp(&score(b)))
                .unwrap_or(0)
        };
        let usage = |k: usize| {
            let a = &five[k].attrs;
            roles[k].usage()
                * (0.4 + (a[Attr::CloseShot] + a[Attr::MidRange] + a[Attr::ThreePoint]) / 60.0)
        };
        let handler = best(
            &|k| five[k].attrs[Attr::Handling] + five[k].attrs[Attr::Passing],
            None,
        );
        let lead = if self.offense == OffenseSet::Isolation {
            best(&usage, None)
        } else {
            handler
        };
        let post = best(
            &|k| {
                (five[k].attrs[Attr::PostMoves] + five[k].attrs[Attr::Strength])
                    * (0.7 + 0.3 * usage(k))
            },
            None,
        );
        let big = match self.offense {
            OffenseSet::Motion | OffenseSet::PostUp if post != lead => post,
            _ => best(&|k| five[k].body.height_cm, Some(lead)),
        };
        for (k, place) in crate::engine::places(lead, big).into_iter().enumerate() {
            out[place] = Some(five[k].id);
        }
        out
    }
}
