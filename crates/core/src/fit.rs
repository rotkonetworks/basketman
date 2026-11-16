//! The teaching layer of the tactics screen: every choice a coach can make
//! as a card with a plain sentence, what it needs, how well *this* squad
//! fits it (computed from the attributes the choice leans on), what it is
//! good and bad against, and a drawing of the play.
//!
//! Nothing here touches the simulation or a random source. The numbers are
//! arithmetic over the five on the floor, so a rating moves when the squad
//! does.

use crate::court::{Vec2, v};
use crate::league::{TeamId, World};
use crate::player::{Attr, Player};
use crate::tactics::{DefenseScheme, OffenseSet, Press, Tactics};

/// A three-point shooter, for counting how many a team has.
const SHOOTER: f32 = 11.0;

/// How well a squad suits a choice: 0..=100 and the reason in a sentence
/// that names the men and the numbers.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Fit {
    pub score: u8,
    pub reason: String,
}

impl Fit {
    pub fn verdict(&self) -> &'static str {
        match self.score {
            75.. => "Great fit",
            55..=74 => "Good fit",
            35..=54 => "Fair fit",
            _ => "Poor fit",
        }
    }
}

/// An attribute point scale, 1..=20, as 0..=1 where 4 is poor and 16 is
/// excellent.
fn norm(x: f32) -> f32 {
    ((x - 4.0) / 12.0).clamp(0.0, 1.0)
}

fn pts(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 100.0 + 0.5) as u8
}

fn mean(xs: impl Iterator<Item = f32>) -> f32 {
    let (s, n) = xs.fold((0.0, 0), |(s, n), x| (s + x, n + 1));
    if n == 0 { 0.0 } else { s / n as f32 }
}

fn last_name(p: &Player) -> &str {
    p.name.rsplit(' ').next().unwrap_or(&p.name)
}

/// A man's name among the five: his surname, with an initial (and his
/// position) when somebody else of the five would read the same.
fn nm(five: &[&Player], p: &Player) -> String {
    let last = last_name(p);
    let twins = five.iter().filter(|o| o.id != p.id && last_name(o) == last);
    let mut same_surname = false;
    let mut same_name = false;
    for o in twins {
        same_surname = true;
        same_name |= o.name == p.name;
    }
    let initial = p.name.chars().next().unwrap_or(' ');
    if same_name {
        format!("{initial}. {last} ({})", p.position.label())
    } else if same_surname {
        format!("{initial}. {last}")
    } else {
        last.to_string()
    }
}

/// The five at the places of the board, point guard first, and falling back
/// to the first five of the rotation when the cast cannot be worked out.
pub fn starters<'a>(t: &Tactics, players: &'a [Player]) -> Vec<&'a Player> {
    let cast = t.cast(players);
    let five: Vec<&Player> = cast
        .iter()
        .flatten()
        .map(|&id| &players[id as usize])
        .collect();
    if five.len() == 5 {
        return five;
    }
    t.rotation
        .iter()
        .take(5)
        .map(|s| &players[s.player as usize])
        .collect()
}

fn best<'a>(five: &[&'a Player], f: impl Fn(&Player) -> f32) -> (&'a Player, f32) {
    let mut out = (five[0], f(five[0]));
    for &p in &five[1..] {
        let x = f(p);
        if x > out.1 {
            out = (p, x);
        }
    }
    out
}

fn a(p: &Player, at: Attr) -> f32 {
    p.attrs[at]
}

fn shooters(five: &[&Player]) -> usize {
    five.iter().filter(|p| a(p, Attr::ThreePoint) >= SHOOTER).count()
}

// ---- places on the board -----------------------------------------------------

/// What each place on the board asks of the man who stands there, and the
/// attributes that make him good at it.
pub fn place_needs(place: usize) -> (&'static str, &'static [Attr]) {
    match place {
        0 => (
            "Brings the ball up and starts the offense. He needs to handle pressure and find the open man.",
            &[Attr::Handling, Attr::Passing, Attr::Iq, Attr::Speed],
        ),
        1 => (
            "A second ball handler and a shooter on the perimeter who can also guard quick men.",
            &[
                Attr::ThreePoint,
                Attr::Handling,
                Attr::PerimeterD,
                Attr::Footwork,
                Attr::Speed,
            ],
        ),
        2 => (
            "Spaces the floor, shoots off the pass and runs the break.",
            &[
                Attr::ThreePoint,
                Attr::MidRange,
                Attr::PerimeterD,
                Attr::Speed,
            ],
        ),
        3 => (
            "Between the perimeter and the paint: scores, rebounds and switches onto either.",
            &[
                Attr::MidRange,
                Attr::CloseShot,
                Attr::DefRebound,
                Attr::Strength,
            ],
        ),
        _ => (
            "Protects the rim, rebounds and finishes close to the basket.",
            &[
                Attr::InteriorD,
                Attr::DefRebound,
                Attr::Block,
                Attr::CloseShot,
                Attr::Footwork,
            ],
        ),
    }
}

/// How well a man suits the place he stands at under this offense, 0..=1:
/// the mean of what the place asks, with the offense's own demand added.
pub fn place_fit(offense: OffenseSet, place: usize, p: &Player) -> f32 {
    let (_, attrs) = place_needs(place);
    let mut xs: Vec<f32> = attrs.iter().map(|&x| a(p, x)).collect();
    match (offense, place) {
        (OffenseSet::PickAndRoll, 0) => xs.push(a(p, Attr::Handling)),
        (OffenseSet::PickAndRoll, 4) => xs.push(a(p, Attr::Vertical)),
        (OffenseSet::PostUp, 4) => xs.push(a(p, Attr::PostMoves)),
        (OffenseSet::Isolation, 0) => {
            xs.push((a(p, Attr::CloseShot) + a(p, Attr::MidRange) + a(p, Attr::ThreePoint)) / 3.0)
        }
        (OffenseSet::FiveOut, _) => xs.push(a(p, Attr::ThreePoint)),
        (OffenseSet::Motion, _) => xs.push(a(p, Attr::Passing)),
        _ => {}
    }
    norm(mean(xs.into_iter()))
}

// ---- offense -----------------------------------------------------------------

impl OffenseSet {
    /// One sentence, for the card.
    pub fn line(self) -> &'static str {
        match self {
            OffenseSet::Motion => "Everyone cuts, screens and passes until somebody is open.",
            OffenseSet::PickAndRoll => {
                "The big man screens for the ball handler, who either drives or hands him the ball."
            }
            OffenseSet::PostUp => "Feed the ball to a big man near the basket and play off him.",
            OffenseSet::Isolation => "Clear one side and let your best scorer beat his man alone.",
            OffenseSet::FiveOut => "All five stand outside the arc; drive, kick it out, shoot threes.",
        }
    }

    /// What kind of players it needs, in a phrase.
    pub fn needs(self) -> &'static str {
        match self {
            OffenseSet::Motion => "Good passers who read the game, all five.",
            OffenseSet::PickAndRoll => "A real ball handler, a big who finishes, and two shooters.",
            OffenseSet::PostUp => "A strong post scorer with size on his man.",
            OffenseSet::Isolation => "One star who can score on anybody.",
            OffenseSet::FiveOut => "Four or five who can hit the three.",
        }
    }

    /// Defenses this offense beats.
    pub fn good_vs(self) -> &'static [DefenseScheme] {
        use DefenseScheme::*;
        match self {
            OffenseSet::Motion => &[Zone23, Zone32],
            OffenseSet::PickAndRoll => &[Zone32],
            OffenseSet::PostUp => &[SwitchEverything, Zone32],
            OffenseSet::Isolation => &[SwitchEverything],
            OffenseSet::FiveOut => &[Zone23, PackLine],
        }
    }

    /// Defenses that take this offense away.
    pub fn bad_vs(self) -> &'static [DefenseScheme] {
        use DefenseScheme::*;
        match self {
            OffenseSet::Motion => &[],
            OffenseSet::PickAndRoll => &[SwitchEverything, PackLine, Zone23],
            OffenseSet::PostUp => &[Zone23, PackLine],
            OffenseSet::Isolation => &[Zone23, PackLine],
            OffenseSet::FiveOut => &[Zone32, SwitchEverything],
        }
    }

    /// How well the five suit it.
    pub fn fit(self, five: &[&Player]) -> Fit {
        if five.len() < 5 {
            return Fit {
                score: 0,
                reason: "Needs five players.".into(),
            };
        }
        match self {
            OffenseSet::Motion => {
                let m = mean(five.iter().map(|p| (a(p, Attr::Passing) + a(p, Attr::Iq)) / 2.0));
                let (weak, w) = best(five, |p| -(a(p, Attr::Passing) + a(p, Attr::Iq)) / 2.0);
                Fit {
                    score: pts(norm(m)),
                    reason: format!(
                        "Passing and IQ average {m:.0} across your five; the weak link is {} at {:.0}.",
                        nm(five, weak),
                        -w
                    ),
                }
            }
            OffenseSet::PickAndRoll => {
                let (h, hv) = best(five, |p| (a(p, Attr::Handling) + a(p, Attr::Passing)) / 2.0);
                let rest: Vec<&Player> = five.iter().copied().filter(|p| p.id != h.id).collect();
                let (big, bv) = best(&rest, |p| {
                    (a(p, Attr::CloseShot) + a(p, Attr::Vertical) + a(p, Attr::Strength)) / 3.0
                });
                let n = rest
                    .iter()
                    .filter(|p| p.id != big.id && a(p, Attr::ThreePoint) >= SHOOTER)
                    .count();
                let score = 0.4 * norm(hv) + 0.35 * norm(bv) + 0.25 * (n.min(2) as f32 / 2.0);
                Fit {
                    score: pts(score),
                    reason: format!(
                        "{} handles it at {hv:.0}, {} finishes at {bv:.0}; needs two shooters, you have {}.",
                        nm(five, h),
                        nm(five, big),
                        n
                    ),
                }
            }
            OffenseSet::PostUp => {
                let (post, pv) = best(five, |p| (a(p, Attr::PostMoves) + a(p, Attr::Strength)) / 2.0);
                let size = ((post.body.height_cm - 185.0) / 20.0).clamp(0.0, 1.0);
                Fit {
                    score: pts(0.7 * norm(pv) + 0.3 * size),
                    reason: format!(
                        "{} is your post man: post moves and strength {pv:.0}, {:.0} cm tall.",
                        nm(five, post),
                        post.body.height_cm
                    ),
                }
            }
            OffenseSet::Isolation => {
                let skill = |p: &Player| {
                    0.5 * (a(p, Attr::CloseShot) + a(p, Attr::MidRange) + a(p, Attr::ThreePoint)) / 3.0
                        + 0.3 * a(p, Attr::Handling)
                        + 0.2 * a(p, Attr::Speed)
                };
                let (star, sv) = best(five, skill);
                let others = mean(five.iter().filter(|p| p.id != star.id).map(|p| skill(p)));
                let gap = sv - others;
                Fit {
                    score: pts(0.6 * norm(sv) + 0.4 * (gap / 4.0).clamp(0.0, 1.0)),
                    reason: if gap >= 1.5 {
                        format!(
                            "{} is a real scorer, {gap:.1} points of skill above the other four.",
                            nm(five, star)
                        )
                    } else {
                        format!(
                            "Nobody stands out: your best scorer, {}, is only {gap:.1} above the rest.",
                            nm(five, star)
                        )
                    },
                }
            }
            OffenseSet::FiveOut => {
                let n = shooters(five);
                let m = mean(five.iter().map(|p| a(p, Attr::ThreePoint)));
                Fit {
                    score: pts(0.7 * (n as f32 / 5.0) + 0.3 * norm(m)),
                    reason: format!(
                        "Needs four shooters; you have {} (three-point 11 or better).",
                        n
                    ),
                }
            }
        }
    }
}

// ---- defense -----------------------------------------------------------------

impl DefenseScheme {
    pub fn line(self) -> &'static str {
        match self {
            DefenseScheme::ManToMan => "Everyone guards one man and stays between him and the basket.",
            DefenseScheme::SwitchEverything => {
                "When a screen comes, the two defenders swap men instead of fighting through."
            }
            DefenseScheme::Zone23 => {
                "Two guards on top, three along the baseline; each guards an area."
            }
            DefenseScheme::Zone32 => "Three across the top, two underneath; each guards an area.",
            DefenseScheme::PackLine => "Man to man, but everyone off the ball sags into the lane.",
        }
    }

    pub fn needs(self) -> &'static str {
        match self {
            DefenseScheme::ManToMan => "Five solid one-on-one defenders.",
            DefenseScheme::SwitchEverything => "Five of similar size who move their feet.",
            DefenseScheme::Zone23 => "Two bigs who protect the rim and rebound; smart heads.",
            DefenseScheme::Zone32 => "Quick, long guards and wings across the top.",
            DefenseScheme::PackLine => "Strong, smart defenders who guard the lane.",
        }
    }

    /// Offenses this defense handles well: the ones that name it as a
    /// weakness.
    pub fn good_vs(self) -> Vec<OffenseSet> {
        OffenseSet::ALL
            .into_iter()
            .filter(|o| o.bad_vs().contains(&self))
            .collect()
    }

    /// Offenses that beat it.
    pub fn bad_vs(self) -> Vec<OffenseSet> {
        OffenseSet::ALL
            .into_iter()
            .filter(|o| o.good_vs().contains(&self))
            .collect()
    }

    pub fn fit(self, five: &[&Player]) -> Fit {
        if five.len() < 5 {
            return Fit {
                score: 0,
                reason: "Needs five players.".into(),
            };
        }
        let mut by_height: Vec<&Player> = five.to_vec();
        by_height.sort_by(|x, y| y.body.height_cm.total_cmp(&x.body.height_cm));
        match self {
            DefenseScheme::ManToMan => {
                let m = mean(five.iter().map(|p| (a(p, Attr::PerimeterD) + a(p, Attr::InteriorD)) / 2.0));
                let (weak, w) = best(five, |p| -(a(p, Attr::PerimeterD).max(a(p, Attr::InteriorD))));
                Fit {
                    score: pts(norm(m)),
                    reason: format!(
                        "Defense averages {m:.0} across the five; the weakest, {}, is {:.0} at his best.",
                        nm(five, weak),
                        -w
                    ),
                }
            }
            DefenseScheme::SwitchEverything => {
                let spread = by_height[0].body.height_cm - by_height[4].body.height_cm;
                let feet = mean(five.iter().map(|p| (a(p, Attr::PerimeterD) + a(p, Attr::Footwork)) / 2.0));
                let size = (1.0 - (spread - 12.0) / 28.0).clamp(0.0, 1.0);
                Fit {
                    score: pts(0.5 * norm(feet) + 0.5 * size),
                    reason: format!(
                        "Your five are {spread:.0} cm apart, tallest to shortest; a switch is safe under about 15 cm. Feet {feet:.0}."
                    ),
                }
            }
            DefenseScheme::Zone23 => {
                let bigs = &by_height[..2];
                let rim = mean(bigs.iter().map(|p| (a(p, Attr::InteriorD) + a(p, Attr::DefRebound)) / 2.0));
                let iq = mean(five.iter().map(|p| a(p, Attr::Iq)));
                Fit {
                    score: pts(0.7 * norm(rim) + 0.3 * norm(iq)),
                    reason: format!(
                        "{} and {} guard the rim at {rim:.0}; team IQ {iq:.0}. It gives up threes.",
                        nm(five, bigs[0]),
                        nm(five, bigs[1])
                    ),
                }
            }
            DefenseScheme::Zone32 => {
                let front = &by_height[2..];
                let f = mean(front.iter().map(|p| (a(p, Attr::PerimeterD) + a(p, Attr::Speed)) / 2.0));
                let reb = mean(by_height[..2].iter().map(|p| a(p, Attr::DefRebound)));
                Fit {
                    score: pts(0.7 * norm(f) + 0.3 * norm(reb)),
                    reason: format!(
                        "Your top three defend the arc at {f:.0}; the two behind them rebound at {reb:.0}."
                    ),
                }
            }
            DefenseScheme::PackLine => {
                let i = mean(five.iter().map(|p| a(p, Attr::InteriorD)));
                let s = mean(five.iter().map(|p| a(p, Attr::Strength)));
                let q = mean(five.iter().map(|p| a(p, Attr::Iq)));
                Fit {
                    score: pts(0.4 * norm(i) + 0.3 * norm(s) + 0.3 * norm(q)),
                    reason: format!(
                        "Lane defense {i:.0}, strength {s:.0}, IQ {q:.0}: it takes all three to sag and still recover."
                    ),
                }
            }
        }
    }
}

// ---- press and the sliders ---------------------------------------------------

impl Press {
    pub fn line(self) -> &'static str {
        match self {
            Press::None => "Fall back and defend in the half court.",
            Press::ThreeQuarter => "Pick the ball up before half court to force mistakes.",
            Press::FullCourt => "Pressure the ball from the baseline, all game.",
        }
    }

    pub fn needs(self) -> &'static str {
        match self {
            Press::None => "Nothing.",
            _ => "Fit players with stamina and quick hands.",
        }
    }

    pub fn fit(self, rotation: &[&Player]) -> Fit {
        if self == Press::None || rotation.is_empty() {
            return Fit {
                score: 60,
                reason: "Asks nothing of the squad, and gains nothing.".into(),
            };
        }
        let top: Vec<&Player> = rotation.iter().take(8).copied().collect();
        let stamina = mean(top.iter().map(|p| a(p, Attr::Stamina)));
        let hands = mean(top.iter().map(|p| (a(p, Attr::Steal) + a(p, Attr::Speed)) / 2.0));
        let weight = if self == Press::FullCourt { 1.25 } else { 1.0 };
        let s = (0.55 * norm(stamina) + 0.45 * norm(hands)) / weight;
        Fit {
            score: pts(s),
            reason: format!(
                "Stamina averages {stamina:.0} and steal and speed {hands:.0} over your top eight; pressing tires them faster."
            ),
        }
    }
}

/// The four sliders, as three settings each.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Knob {
    Pace,
    Threes,
    Boards,
    Gamble,
}

impl Knob {
    pub const ALL: [Knob; 4] = [Knob::Pace, Knob::Threes, Knob::Boards, Knob::Gamble];

    pub fn label(self) -> &'static str {
        match self {
            Knob::Pace => "Tempo",
            Knob::Threes => "Shot selection",
            Knob::Boards => "Rebounding",
            Knob::Gamble => "Defensive risk",
        }
    }

    pub fn get(self, t: &Tactics) -> u8 {
        match self {
            Knob::Pace => t.pace,
            Knob::Threes => t.three_bias,
            Knob::Boards => t.crash_boards,
            Knob::Gamble => t.aggression,
        }
    }

    pub fn set(self, t: &mut Tactics, value: u8) {
        let value = value.clamp(1, 20);
        match self {
            Knob::Pace => t.pace = value,
            Knob::Threes => t.three_bias = value,
            Knob::Boards => t.crash_boards = value,
            Knob::Gamble => t.aggression = value,
        }
    }

    /// 0 low, 1 middle, 2 high.
    pub fn level(value: u8) -> usize {
        match value {
            ..=7 => 0,
            8..=12 => 1,
            _ => 2,
        }
    }

    /// The slider value each of the three cards sets.
    pub const SETTINGS: [u8; 3] = [5, 10, 16];

    pub fn title(self, level: usize) -> &'static str {
        match (self, level) {
            (Knob::Pace, 0) => "Walk it up",
            (Knob::Pace, 1) => "Normal tempo",
            (Knob::Pace, _) => "Push the pace",
            (Knob::Threes, 0) => "Work it inside",
            (Knob::Threes, 1) => "Take what comes",
            (Knob::Threes, _) => "Let it fly",
            (Knob::Boards, 0) => "Get back",
            (Knob::Boards, 1) => "Balanced",
            (Knob::Boards, _) => "Crash the glass",
            (Knob::Gamble, 0) => "Stay in front",
            (Knob::Gamble, 1) => "Normal",
            (Knob::Gamble, _) => "Gamble for steals",
        }
    }

    pub fn line(self, level: usize) -> &'static str {
        match (self, level) {
            (Knob::Pace, 0) => "Work the shot clock for a better shot; fewer possessions.",
            (Knob::Pace, 1) => "Shoot when the shot is there.",
            (Knob::Pace, _) => "Shoot early, run after every miss; more possessions, more tired legs.",
            (Knob::Threes, 0) => "More of your shots come near the basket, fewer threes.",
            (Knob::Threes, 1) => "Shooters shoot when they are open.",
            (Knob::Threes, _) => "Shooters take the three at once, even a so-so one.",
            (Knob::Boards, 0) => "Send everybody back; give up second chances to stop the break.",
            (Knob::Boards, 1) => "Some go for the miss, some get back.",
            (Knob::Boards, _) => "More second chances, slower getting back on defense.",
            (Knob::Gamble, 0) => "Stay in front of your man; fewer steals, fewer fouls.",
            (Knob::Gamble, 1) => "Play it straight.",
            (Knob::Gamble, _) => "Reach and jump the passing lanes; more steals, more fouls.",
        }
    }

    pub fn needs(self, level: usize) -> &'static str {
        match (self, level) {
            (Knob::Pace, 0) => "Patient players who make good decisions.",
            (Knob::Pace, 1) => "Nothing in particular.",
            (Knob::Pace, _) => "Fit players with speed and stamina.",
            (Knob::Threes, 0) => "Scorers near the basket.",
            (Knob::Threes, 1) => "Nothing in particular.",
            (Knob::Threes, _) => "Shooters who hit the three.",
            (Knob::Boards, 0) => "Nothing; quick hands in transition help.",
            (Knob::Boards, 1) => "Nothing in particular.",
            (Knob::Boards, _) => "Strong rebounders up front.",
            (Knob::Gamble, 0) => "Disciplined feet.",
            (Knob::Gamble, 1) => "Nothing in particular.",
            (Knob::Gamble, _) => "Quick hands and a cool head so the fouls stay few.",
        }
    }

    pub fn fit(self, level: usize, five: &[&Player]) -> Fit {
        if five.len() < 5 || level == 1 {
            return Fit {
                score: 60,
                reason: "The balanced setting suits anyone.".into(),
            };
        }
        let m = |f: &dyn Fn(&Player) -> f32| mean(five.iter().map(|p| f(p)));
        let (x, what) = match (self, level) {
            (Knob::Pace, 0) => (
                m(&|p| (a(p, Attr::Iq) + a(p, Attr::Composure)) / 2.0),
                "IQ and composure",
            ),
            (Knob::Pace, _) => (
                m(&|p| (a(p, Attr::Speed) + a(p, Attr::Stamina)) / 2.0),
                "speed and stamina",
            ),
            (Knob::Threes, 0) => (
                m(&|p| (a(p, Attr::CloseShot) + a(p, Attr::MidRange)) / 2.0),
                "close and mid-range shooting",
            ),
            (Knob::Threes, _) => (m(&|p| a(p, Attr::ThreePoint)), "three-point shooting"),
            (Knob::Boards, 0) => (m(&|p| a(p, Attr::Speed)), "speed"),
            (Knob::Boards, _) => (
                m(&|p| (a(p, Attr::OffRebound) + a(p, Attr::Strength)) / 2.0),
                "offensive rebounding and strength",
            ),
            (Knob::Gamble, 0) => (m(&|p| a(p, Attr::PerimeterD)), "perimeter defense"),
            (Knob::Gamble, _) => (
                m(&|p| (a(p, Attr::Steal) + a(p, Attr::Composure)) / 2.0),
                "steals and composure",
            ),
        };
        Fit {
            score: pts(norm(x)),
            reason: format!("Your five average {x:.0} in {what}."),
        }
    }
}

// ---- cards -------------------------------------------------------------------

/// One thing the coach can choose.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    Offense(OffenseSet),
    Defense(DefenseScheme),
    Press(Press),
    /// A slider and which of its three settings (0..=2).
    Knob(Knob, usize),
}

/// A row of cards on the screen.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Group {
    Offense,
    Defense,
    Press,
    Knob(Knob),
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Offense => "Offense",
            Group::Defense => "Defense",
            Group::Press => "Press",
            Group::Knob(k) => k.label(),
        }
    }

    pub fn choices(self) -> Vec<Choice> {
        match self {
            Group::Offense => OffenseSet::ALL.into_iter().map(Choice::Offense).collect(),
            Group::Defense => DefenseScheme::ALL.into_iter().map(Choice::Defense).collect(),
            Group::Press => Press::ALL.into_iter().map(Choice::Press).collect(),
            Group::Knob(k) => (0..3).map(|l| Choice::Knob(k, l)).collect(),
        }
    }
}

impl Choice {
    pub fn title(self) -> &'static str {
        match self {
            Choice::Offense(o) => o.label(),
            Choice::Defense(d) => d.label(),
            Choice::Press(p) => p.label(),
            Choice::Knob(k, l) => k.title(l),
        }
    }

    pub fn line(self) -> &'static str {
        match self {
            Choice::Offense(o) => o.line(),
            Choice::Defense(d) => d.line(),
            Choice::Press(p) => p.line(),
            Choice::Knob(k, l) => k.line(l),
        }
    }

    pub fn needs(self) -> &'static str {
        match self {
            Choice::Offense(o) => o.needs(),
            Choice::Defense(d) => d.needs(),
            Choice::Press(p) => p.needs(),
            Choice::Knob(k, l) => k.needs(l),
        }
    }

    /// What the longer description says, for hover.
    pub fn describe(self) -> &'static str {
        match self {
            Choice::Offense(o) => o.describe(),
            Choice::Defense(d) => d.describe(),
            Choice::Press(p) => p.describe(),
            Choice::Knob(k, l) => k.line(l),
        }
    }

    pub fn is_on(self, t: &Tactics) -> bool {
        match self {
            Choice::Offense(o) => t.offense == o,
            Choice::Defense(d) => t.defense == d,
            Choice::Press(p) => t.press == p,
            Choice::Knob(k, l) => Knob::level(k.get(t)) == l,
        }
    }

    pub fn apply(self, t: &mut Tactics) {
        match self {
            Choice::Offense(o) => t.offense = o,
            Choice::Defense(d) => t.defense = d,
            Choice::Press(p) => t.press = p,
            Choice::Knob(k, l) => {
                // Already in that band: leave the exact value alone.
                if Knob::level(k.get(t)) != l {
                    k.set(t, Knob::SETTINGS[l]);
                }
            }
        }
    }

    pub fn fit(self, t: &Tactics, players: &[Player]) -> Fit {
        let five = starters(t, players);
        match self {
            Choice::Offense(o) => o.fit(&five),
            Choice::Defense(d) => d.fit(&five),
            Choice::Press(p) => {
                let rot: Vec<&Player> = t
                    .rotation
                    .iter()
                    .map(|s| &players[s.player as usize])
                    .collect();
                p.fit(&rot)
            }
            Choice::Knob(k, l) => k.fit(l, &five),
        }
    }

    /// "Good against: ... Poor against: ..." for the cards that have a
    /// matchup, as plain text.
    pub fn against(self) -> Option<(String, String)> {
        let join = |xs: Vec<&str>| {
            if xs.is_empty() {
                "nothing in particular".to_string()
            } else {
                xs.join(", ")
            }
        };
        match self {
            Choice::Offense(o) => Some((
                join(o.good_vs().iter().map(|d| d.label()).collect()),
                join(o.bad_vs().iter().map(|d| d.label()).collect()),
            )),
            Choice::Defense(d) => Some((
                join(d.good_vs().iter().map(|o| o.label()).collect()),
                join(d.bad_vs().iter().map(|o| o.label()).collect()),
            )),
            Choice::Press(Press::None) | Choice::Knob(..) => None,
            Choice::Press(_) => Some((
                "teams with poor ball handlers, and when you are chasing a game".into(),
                "quick guards who split it, and when your players are tired".into(),
            )),
        }
    }

    /// Terms on the card a newcomer may not know; see `term`.
    pub fn terms(self) -> &'static [&'static str] {
        match self {
            Choice::Offense(OffenseSet::Motion) => &["screen", "cut", "spacing"],
            Choice::Offense(OffenseSet::PickAndRoll) => &["pick and roll", "screen", "switch"],
            Choice::Offense(OffenseSet::PostUp) => &["post up", "mismatch"],
            Choice::Offense(OffenseSet::Isolation) => &["isolation", "primary option"],
            Choice::Offense(OffenseSet::FiveOut) => &["spacing", "kick out"],
            Choice::Defense(DefenseScheme::ManToMan) => &["man to man"],
            Choice::Defense(DefenseScheme::SwitchEverything) => &["switch", "mismatch", "screen"],
            Choice::Defense(DefenseScheme::Zone23 | DefenseScheme::Zone32) => &["zone"],
            Choice::Defense(DefenseScheme::PackLine) => &["pack line", "man to man"],
            Choice::Press(Press::None) => &[],
            Choice::Press(_) => &["press"],
            Choice::Knob(Knob::Pace, _) => &["pace", "shot clock"],
            Choice::Knob(Knob::Threes, _) => &["spacing"],
            Choice::Knob(Knob::Boards, _) => &["crash the glass", "fast break"],
            Choice::Knob(Knob::Gamble, _) => &["steal"],
        }
    }
}

/// A basketball word in one line.
pub fn term(word: &str) -> Option<&'static str> {
    let w = word.to_lowercase();
    TERMS.iter().find(|(k, _)| *k == w).map(|(_, d)| *d)
}

pub const TERMS: &[(&str, &str)] = &[
    ("screen", "A player stands still in a defender's way so a teammate can get free. Also called a pick."),
    ("pick and roll", "The big man sets a screen for the ball handler, then rolls toward the basket for a pass. The defense has to choose: stop the guard or the big."),
    ("cut", "A run to the basket without the ball, hoping for a pass."),
    ("spacing", "Standing far enough apart that one defender cannot guard two of you, and the lane stays open to drive."),
    ("switch", "Two defenders swap the men they guard when a screen comes, instead of fighting through it."),
    ("mismatch", "A small defender on a big attacker or the other way round: the attacker has an edge."),
    ("post up", "Playing with your back to the basket, close to it, using strength and footwork."),
    ("isolation", "Everyone else clears out and one player goes one on one."),
    ("primary option", "The player the offense runs through; he gets the most shots."),
    ("kick out", "A pass from a driver or post player back out to a shooter on the perimeter."),
    ("man to man", "Each defender is responsible for one attacker, wherever he goes."),
    ("zone", "Each defender guards an area of the floor, not a man, and passes attackers on as they cross."),
    ("pack line", "Man to man, but defenders off the ball stand inside an imaginary line near the lane to stop drives."),
    ("press", "Defending the ball in the backcourt to cause turnovers, at the price of tired legs and easy layups if it is broken."),
    ("pace", "How many possessions a game has. A fast pace shoots early and runs; a slow one works the shot clock."),
    ("shot clock", "A team must shoot within 24 seconds of getting the ball."),
    ("crash the glass", "Sending players to rebound a missed shot instead of getting back on defense."),
    ("fast break", "A quick attack after a turnover or rebound before the defense is set."),
    ("steal", "Taking the ball from the other team, by reaching or reading a pass."),
    ("familiarity", "How well the squad knows the system. Changing the offense, defense or press sets it back."),
    ("fit", "How well the five on the floor suit a choice, worked out from the attributes it uses."),
];

// ---- plays on the board ------------------------------------------------------

/// One thing the board draws for an offense. `who` is a place on the board,
/// 0 to 4, and paths are in the attack frame, metres.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    /// A run without the ball, from where he starts through these points.
    Cut { who: usize, path: Vec<Vec2> },
    /// A run with the ball.
    Dribble { who: usize, path: Vec<Vec2> },
    /// `who` stands at `at` as a screen for `for_who`.
    Screen {
        who: usize,
        at: Vec2,
        for_who: usize,
    },
    /// A pass, solid for the main one and dashed for the option.
    Pass {
        from: usize,
        to: usize,
        option: bool,
    },
}

/// The signature movement of a set, over its stock positions. A teaching
/// drawing of what the set is for; the scrimmage shows what really happens.
pub fn play(set: OffenseSet) -> Vec<Mark> {
    use Mark::*;
    match set {
        OffenseSet::Motion => vec![
            Pass { from: 0, to: 1, option: false },
            Cut { who: 0, path: vec![v(1.6, 5.2), v(5.6, 1.6)] },
            Screen { who: 4, at: v(-1.7, 6.3), for_who: 2 },
            Cut { who: 2, path: vec![v(-0.9, 5.9), v(0.5, 3.3)] },
            Pass { from: 1, to: 2, option: true },
        ],
        OffenseSet::PickAndRoll => vec![
            Screen { who: 4, at: v(0.8, 8.2), for_who: 0 },
            Dribble { who: 0, path: vec![v(2.9, 6.9), v(1.5, 4.0)] },
            Cut { who: 4, path: vec![v(1.1, 5.6), v(0.3, 3.2)] },
            Pass { from: 0, to: 4, option: false },
            Pass { from: 0, to: 1, option: true },
        ],
        OffenseSet::PostUp => vec![
            Pass { from: 0, to: 4, option: false },
            Cut { who: 3, path: vec![v(-4.2, 3.6), v(-2.6, 2.3)] },
            Cut { who: 1, path: vec![v(2.8, 7.4)] },
            Pass { from: 4, to: 2, option: true },
        ],
        OffenseSet::Isolation => vec![
            Pass { from: 1, to: 0, option: false },
            Dribble { who: 0, path: vec![v(-2.6, 5.0), v(-0.9, 2.5)] },
            Cut { who: 4, path: vec![v(4.9, 2.0)] },
        ],
        OffenseSet::FiveOut => vec![
            Dribble { who: 0, path: vec![v(1.6, 6.5), v(0.5, 3.6)] },
            Pass { from: 0, to: 3, option: true },
            Cut { who: 1, path: vec![v(3.0, 2.9)] },
            Pass { from: 0, to: 2, option: true },
        ],
    }
}

// ---- who the other side is ---------------------------------------------------

/// How the next opponent tends to play, from what is known about them.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Opponent {
    pub team: TeamId,
    pub name: String,
    pub offense: OffenseSet,
    pub defense: DefenseScheme,
    pub press: Press,
    pub pace: u8,
    /// Plain-language lines about how they play.
    pub tendencies: Vec<String>,
    /// Our offenses that suit their defense, and our defenses that suit
    /// their offense.
    pub we_attack_with: Vec<OffenseSet>,
    pub we_defend_with: Vec<DefenseScheme>,
}

/// The club we play next, if there is a game to play.
pub fn next_opponent(world: &World, team: TeamId) -> Option<TeamId> {
    world
        .fixtures
        .iter()
        .find(|f| f.result.is_none() && (f.home == team || f.away == team))
        .map(|f| if f.home == team { f.away } else { f.home })
}

/// A club to test against when there is no game: the next one in the list.
pub fn sample_opponent(world: &World, team: TeamId) -> TeamId {
    (team + 1) % world.teams.len()
}

pub fn scout(world: &World, opp: TeamId) -> Opponent {
    let them = &world.teams[opp];
    let t = them.tactics();
    let mut tendencies = vec![
        format!("They run {}: {}", t.offense.label().to_lowercase(), t.offense.line().to_lowercase()),
        format!("They defend in {}: {}", t.defense.label().to_lowercase(), t.defense.line().to_lowercase()),
    ];
    tendencies.push(
        match t.pace {
            ..=7 => "They walk it up and use the shot clock.",
            8..=12 => "They play at an ordinary pace.",
            _ => "They push the pace and shoot early.",
        }
        .to_string(),
    );
    if t.press != Press::None {
        tendencies.push(format!("They press ({}).", t.press.label().to_lowercase()));
    }
    if t.three_bias >= 13 {
        tendencies.push("They love the three-pointer.".into());
    } else if t.three_bias <= 7 {
        tendencies.push("They keep their shots inside.".into());
    }
    let s = &world.team_stats[opp];
    if s.games >= 2 && s.own.fga > 0 {
        let share = 100.0 * s.own.tpa as f32 / s.own.fga as f32;
        tendencies.push(format!(
            "{share:.0}% of their shots this season are threes, {:.0} points a game.",
            s.own.pts as f32 / s.games as f32
        ));
    }
    let five = starters(t, &world.players);
    if five.len() == 5 {
        let (star, _) = best(&five, |p| {
            (a(p, Attr::CloseShot) + a(p, Attr::MidRange) + a(p, Attr::ThreePoint)) / 3.0
        });
        tendencies.push(format!("Their scorer to watch is {}.", star.name));
    }
    Opponent {
        team: opp,
        name: them.name.clone(),
        offense: t.offense,
        defense: t.defense,
        press: t.press,
        pace: t.pace,
        tendencies,
        we_attack_with: OffenseSet::ALL
            .into_iter()
            .filter(|o| o.good_vs().contains(&t.defense))
            .collect(),
        we_defend_with: DefenseScheme::ALL
            .into_iter()
            .filter(|d| t.offense.bad_vs().contains(d))
            .collect(),
    }
}

/// What the assistant proposes for a playbook slot, and why.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Proposal {
    pub slot: usize,
    pub tactics: Tactics,
    /// The club it was worked out against, if a game is coming.
    pub against: Option<String>,
    /// Three or four sentences, from the numbers.
    pub why: Vec<String>,
    /// What changes from the slot as it is, as short lines.
    pub changes: Vec<String>,
    /// The test possessions it leaned on, 0 when it went on fit alone.
    pub tested: usize,
}

/// Work out a full setup for the squad, and for the next opponent when one
/// is known. With `deep` the assistant also plays short scrimmages of every
/// offense and defense against that opponent; without, it goes on fit and
/// the matchup tables alone. The rotation is left as it is.
pub fn propose(world: &World, team: TeamId, slot: usize, deep: bool) -> Proposal {
    use crate::scrimmage::{Focus, Setup, compare};
    let me = &world.teams[team];
    let slot = slot.min(me.playbook.len() - 1);
    let now = &me.playbook[slot].tactics;
    let five = starters(now, &world.players);
    let opp = next_opponent(world, team).map(|o| scout(world, o));
    let mut t = now.clone();
    let mut why = Vec::new();
    let mut tested = 0;

    // Offense and defense: fit, the matchup, what the squad already knows,
    // and (when asked) what short scrimmages produced.
    let ppp = |focus: Focus| -> Option<Vec<f32>> {
        if !deep || opp.is_none() {
            return None;
        }
        let rows = compare(world, &Setup::for_slot(world, team, slot, focus), slot, 3);
        Some(rows.iter().map(|r| r.ppp()).collect())
    };
    let off_ppp = ppp(Focus::Attack);
    let def_ppp = ppp(Focus::Defend);
    if off_ppp.is_some() {
        tested = 3 * Setup::for_slot(world, team, slot, Focus::Attack).possessions;
    }
    let mean_of = |x: &Option<Vec<f32>>| x.as_ref().map_or(0.0, |v| v.iter().sum::<f32>() / v.len() as f32);
    let (mo, md) = (mean_of(&off_ppp), mean_of(&def_ppp));

    let mut score_o = Vec::new();
    for (i, o) in OffenseSet::ALL.into_iter().enumerate() {
        let f = o.fit(&five).score as f32;
        let fam = me.playbook[slot].familiarity_with(o, now.defense, now.press);
        let mut x = f + 20.0 * fam;
        if let Some(op) = &opp {
            if o.good_vs().contains(&op.defense) {
                x += 10.0;
            }
            if o.bad_vs().contains(&op.defense) {
                x -= 10.0;
            }
        }
        if let Some(p) = &off_ppp {
            x += 40.0 * (p[i] - mo);
        }
        score_o.push((x, o));
    }
    let mut score_d = Vec::new();
    for (i, d) in DefenseScheme::ALL.into_iter().enumerate() {
        let f = d.fit(&five).score as f32;
        let fam = me.playbook[slot].familiarity_with(t.offense, d, now.press);
        let mut x = f + 20.0 * fam;
        if let Some(op) = &opp {
            if op.offense.bad_vs().contains(&d) {
                x += 10.0;
            }
            if op.offense.good_vs().contains(&d) {
                x -= 10.0;
            }
        }
        if let Some(p) = &def_ppp {
            x -= 40.0 * (p[i] - md);
        }
        score_d.push((x, d));
    }
    let off = score_o.iter().fold(score_o[0], |b, &c| if c.0 > b.0 { c } else { b }).1;
    t.offense = off;
    let defn = score_d.iter().fold(score_d[0], |b, &c| if c.0 > b.0 { c } else { b }).1;
    t.defense = defn;

    // Press: only the lighter one, and only for a squad that can take it.
    t.press = if Press::ThreeQuarter.fit(&starters_all(now, &world.players)).score >= 70 {
        Press::ThreeQuarter
    } else {
        Press::None
    };

    // The sliders: each takes the setting the five suit best, if clearly.
    for k in Knob::ALL {
        let mut level = 1;
        let mut top = 68;
        for l in [0, 2] {
            let s = k.fit(l, &five).score;
            if s > top {
                top = s;
                level = l;
            }
        }
        if t.offense == OffenseSet::FiveOut && k == Knob::Threes {
            level = level.max(1);
        }
        if t.offense == OffenseSet::PostUp && k == Knob::Threes {
            level = level.min(1);
        }
        Choice::Knob(k, level).apply(&mut t);
        if level == 1 {
            // Back to the neutral value, so the proposal is the same however
            // the sliders were left.
            k.set(&mut t, 10);
        }
    }

    // The words.
    let fo = off.fit(&five);
    let fd = defn.fit(&five);
    let mut line = format!("Run the {}: {}", off.label().to_lowercase(), fo.reason);
    if let (Some(p), true) = (&off_ppp, deep) {
        let i = OffenseSet::ALL.iter().position(|&o| o == off).unwrap_or(0);
        let cur = OffenseSet::ALL.iter().position(|&o| o == now.offense).unwrap_or(0);
        if off == now.offense {
            line.push_str(&format!(" In test possessions against {} it makes {:.2} points a possession.", opp.as_ref().map_or("", |o| o.name.as_str()), p[i]));
        } else {
            line.push_str(&format!(" In test possessions against {} it made {:.2} a possession, {} made {:.2}.", opp.as_ref().map_or("", |o| o.name.as_str()), p[i], now.offense.label().to_lowercase(), p[cur]));
        }
    }
    why.push(line);
    let mut line = format!("Defend in {}: {}", defn.label().to_lowercase(), fd.reason);
    if let Some(op) = &opp {
        if op.offense.bad_vs().contains(&defn) {
            line.push_str(&format!(" {} run {}, which it handles well.", op.name, op.offense.label().to_lowercase()));
        } else if op.offense.good_vs().contains(&defn) {
            line.push_str(&format!(" {} run {}, which it does not stop well, but nothing else suited the five better.", op.name, op.offense.label().to_lowercase()));
        }
    }
    why.push(line);
    if t.offense != now.offense || t.defense != now.defense {
        let fam_now = me.playbook[slot].familiarity();
        let fam_new = me.playbook[slot].familiarity_with(t.offense, t.defense, t.press);
        why.push(format!(
            "The squad knows it {:.0}% against {:.0}% now; drills bring it back.",
            fam_new * 100.0,
            fam_now * 100.0
        ));
    } else {
        why.push("That is what you already run, so nothing is lost to relearning.".to_string());
    }
    let styles: Vec<String> = Knob::ALL
        .into_iter()
        .filter(|&k| Knob::level(k.get(&t)) != 1)
        .map(|k| format!("{}: {}", k.label().to_lowercase(), k.title(Knob::level(k.get(&t))).to_lowercase()))
        .collect();
    if styles.is_empty() && t.press == Press::None {
        why.push("The five suit an ordinary style, so the sliders stay in the middle.".to_string());
    } else {
        let mut bits = styles;
        if t.press != Press::None {
            bits.push(format!("press: {}", t.press.label().to_lowercase()));
        }
        why.push(format!("Style: {}, because your five are better at it than the middle setting.", bits.join(", ")));
    }

    let mut changes = Vec::new();
    let mut diff = |what: &str, a: String, b: String| {
        if a != b {
            changes.push(format!("{what}: {a} to {b}"));
        }
    };
    diff("Offense", now.offense.label().into(), t.offense.label().into());
    diff("Defense", now.defense.label().into(), t.defense.label().into());
    diff("Press", now.press.label().into(), t.press.label().into());
    for k in Knob::ALL {
        diff(
            k.label(),
            k.title(Knob::level(k.get(now))).into(),
            k.title(Knob::level(k.get(&t))).into(),
        );
    }
    Proposal {
        slot,
        tactics: t,
        against: opp.map(|o| o.name),
        why,
        changes,
        tested,
    }
}

/// The whole rotation, for things that ask about depth.
fn starters_all<'a>(t: &Tactics, players: &'a [Player]) -> Vec<&'a Player> {
    t.rotation.iter().map(|s| &players[s.player as usize]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::league::World;

    fn world() -> World {
        let mut w = World::generate(3, 8);
        w.take_charge(0);
        w
    }

    #[test]
    fn every_choice_has_a_fit_and_a_reason() {
        let w = world();
        let t = w.teams[0].tactics().clone();
        for g in [
            Group::Offense,
            Group::Defense,
            Group::Press,
            Group::Knob(Knob::Pace),
            Group::Knob(Knob::Threes),
            Group::Knob(Knob::Boards),
            Group::Knob(Knob::Gamble),
        ] {
            for c in g.choices() {
                let f = c.fit(&t, &w.players);
                assert!(f.score <= 100 && !f.reason.is_empty(), "{c:?}");
                assert!(!c.line().is_empty() && !c.needs().is_empty());
                for term_ in c.terms() {
                    assert!(term(term_).is_some(), "missing term {term_}");
                }
            }
        }
    }

    #[test]
    fn the_fit_follows_the_squad() {
        let w = world();
        let t = w.teams[0].tactics().clone();
        let mut players = w.players.clone();
        let before = OffenseSet::FiveOut.fit(&starters(&t, &players));
        // Everyone becomes a shooter.
        for s in t.rotation.iter().take(5) {
            players[s.player as usize].attrs[Attr::ThreePoint] = 18.0;
        }
        let after = OffenseSet::FiveOut.fit(&starters(&t, &players));
        assert!(after.score > before.score, "{before:?} -> {after:?}");
        assert!(after.reason.contains("you have 5"), "{}", after.reason);
        // And everyone a bad one.
        for s in t.rotation.iter().take(5) {
            players[s.player as usize].attrs[Attr::ThreePoint] = 3.0;
        }
        let worst = OffenseSet::FiveOut.fit(&starters(&t, &players));
        assert!(worst.score < before.score && worst.score < 20, "{worst:?}");
        // A taller line-up changes the switching score the other way round.
        let tall = DefenseScheme::SwitchEverything.fit(&starters(&t, &players));
        for s in t.rotation.iter().take(5) {
            players[s.player as usize].body.height_cm = 190.0;
        }
        let even = DefenseScheme::SwitchEverything.fit(&starters(&t, &players));
        assert!(even.score >= tall.score);
    }

    #[test]
    fn the_matchup_tables_agree() {
        for d in DefenseScheme::ALL {
            for o in d.good_vs() {
                assert!(o.bad_vs().contains(&d));
            }
            for o in d.bad_vs() {
                assert!(o.good_vs().contains(&d));
            }
        }
    }

    #[test]
    fn the_proposal_is_legal_and_applies() {
        use crate::command::Command;
        let mut w = world();
        for deep in [false, true] {
            let p = propose(&w, 0, 0, deep);
            assert!(p.why.len() >= 3 && p.why.len() <= 5, "{:?}", p.why);
            assert_eq!(p.tactics.rotation, w.teams[0].tactics().rotation);
            assert_eq!(p.tested > 0, deep);
            match w.apply(Command::SetTactics { slot: 0, tactics: p.tactics.clone() }) {
                Ok(_) | Err(crate::command::CommandError::NothingToDo) => {}
                Err(e) => panic!("{e:?}"),
            }
            assert_eq!(w.teams[0].playbook[0].tactics, p.tactics);
            // On fit alone, asking again of the result gives the same answer;
            // test possessions depend on what is in force, so they may not.
            if !deep {
                assert_eq!(propose(&w, 0, 0, deep).tactics, p.tactics);
            }
        }
    }

    #[test]
    fn choices_apply() {
        let w = world();
        let mut t = w.teams[0].tactics().clone();
        Choice::Offense(OffenseSet::FiveOut).apply(&mut t);
        Choice::Knob(Knob::Pace, 2).apply(&mut t);
        assert_eq!(t.offense, OffenseSet::FiveOut);
        assert!(Choice::Offense(OffenseSet::FiveOut).is_on(&t));
        assert!(Choice::Knob(Knob::Pace, 2).is_on(&t));
        assert!(!Choice::Knob(Knob::Pace, 0).is_on(&t));
    }
}
