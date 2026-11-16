//! The club as a place with a past and with people.
//!
//! Every club has an `Identity` (when it was founded, where it comes from,
//! who it remembers, who it hates) and a `Boardroom` (an owner, sometimes a
//! chairman, and the confidence they have in the manager). The league's
//! history before the game began is laid down once, as one table per year,
//! so every club's honours agree: one champion a year. The seasons played
//! in the game extend that record (`World::club_record`).
//!
//! All of it is drawn from streams of its own, seeded from the economy's
//! seed, so the players and the matches are not disturbed by it.

use serde::{Deserialize, Serialize};

use crate::league::{FIRST_NAMES, LAST_NAMES, TeamId, World};
use crate::player::Position;
use crate::rng::Rng;
use crate::staff::Job;

/// The year season 0 is played in.
pub const START_YEAR: u16 = 2026;
/// How many seasons of history exist before the game begins.
pub const PAST_YEARS: u16 = 40;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClubBook {
    /// Indexed by `TeamId`.
    pub clubs: Vec<ClubState>,
    /// Oldest year first.
    pub past: Vec<PastSeason>,
    /// The managed club's board has decided to dismiss him when the
    /// season turns: why.
    #[serde(default)]
    pub pending: Option<String>,
}

/// One year before the game: the clubs that played, best first.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PastSeason {
    pub year: u16,
    /// Every club of the pyramid that played, best first. The best of the
    /// top division is the champion; the divisions are cut from the list
    /// in `sizes`.
    pub order: Vec<u8>,
    /// How many clubs each division had that year, the top one first.
    /// Empty: one division with everybody.
    #[serde(default)]
    pub sizes: Vec<u8>,
}

impl PastSeason {
    /// The division a club played in that year and its place in it, 1 being
    /// the best. `None` if it did not exist.
    pub fn place_of(&self, club: usize) -> Option<(usize, usize)> {
        let pos = self.order.iter().position(|&x| x as usize == club)?;
        let mut start = 0;
        for (d, &size) in self.sizes.iter().enumerate() {
            if pos < start + size as usize {
                return Some((d, pos - start + 1));
            }
            start += size as usize;
        }
        // No sizes: everybody is in one division.
        Some((self.sizes.len().saturating_sub(1), pos + 1 - start.min(pos)))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClubState {
    pub identity: Identity,
    pub board: Boardroom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Identity {
    pub founded: u16,
    /// A line or two on where the club comes from.
    pub origin: String,
    /// 1..=100 when the game begins. See `World::stature` for today's.
    pub stature: u8,
    pub rival: TeamId,
    pub legends: Vec<Legend>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Legend {
    pub name: String,
    pub position: Position,
    pub from: u16,
    pub to: u16,
    pub known_for: String,
}

/// A person above the manager. Traits are 1..=20.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Person {
    pub name: String,
    pub age: u8,
    pub background: String,
    /// High: gives a manager time. Low: wants results now.
    pub patience: u8,
    /// High: wants trophies. Low: happy with a safe season.
    pub ambition: u8,
    /// High: spends freely. Low: counts every coin.
    pub spending: u8,
    /// High: gets involved in the team. Low: leaves it to the manager.
    pub interference: u8,
    /// High: sticks by his people. Low: quick to replace them.
    pub loyalty: u8,
    /// Season he took over; 0 for the owners the game began with.
    pub since: u16,
}

/// Why the board's confidence moved.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reason {
    pub season: u16,
    pub round: u16,
    pub delta: f32,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Boardroom {
    pub owner: Person,
    /// Small clubs have none: the owner chairs it himself.
    pub chairman: Option<Person>,
    /// 0..=100: the board's confidence in the club's manager.
    pub confidence: f32,
    /// Newest last, at most `MAX_REASONS`.
    pub reasons: Vec<Reason>,
    pub warned: bool,
    /// Games lost in a row.
    pub losing_run: u8,
    /// Games won in a row.
    pub winning_run: u8,
}

pub const MAX_REASONS: usize = 12;

pub(crate) fn roll(rng: &mut Rng) -> u8 {
    1 + ((rng.range(0, 20) + rng.range(0, 20)) / 2) as u8
}

impl Person {
    pub(crate) fn generate(rng: &mut Rng, base_age: u8, since: u16) -> Person {
        let name = format!(
            "{} {}",
            FIRST_NAMES[rng.range(0, FIRST_NAMES.len() as u32) as usize],
            LAST_NAMES[rng.range(0, LAST_NAMES.len() as u32) as usize]
        );
        let (patience, ambition, spending, interference, loyalty) = (
            roll(rng),
            roll(rng),
            roll(rng),
            roll(rng),
            roll(rng),
        );
        let background = {
            let pool: &[&str] = if ambition >= 14 {
                &[
                    "Made his money in shipping and wants a trophy to put beside the boats.",
                    "Built a software firm from a garage. Treats the club as the next start-up.",
                    "Sold his company last year and is looking for something that wins.",
                ]
            } else if spending <= 7 {
                &[
                    "Runs a chain of hardware shops. Counts every coin twice.",
                    "An accountant by training. Reads the balance sheet before the table.",
                    "Took the club over to save it from the bank and has not forgotten.",
                ]
            } else if loyalty >= 14 {
                &[
                    "Played here as a boy and has never left.",
                    "Runs the family bakery that sponsored the club in its first year.",
                    "A local doctor who has held a season ticket for thirty years.",
                ]
            } else {
                &[
                    "Owns a few warehouses by the harbour.",
                    "A property man who came for the tax break and stayed for the games.",
                    "Made a quiet fortune in timber.",
                ]
            };
            pool[rng.range(0, pool.len() as u32) as usize].to_string()
        };
        Person {
            name,
            age: base_age + rng.range(0, 14) as u8,
            background,
            patience,
            ambition,
            spending,
            interference,
            loyalty,
            since,
        }
    }

    /// The man in a few phrases, most telling first.
    pub fn traits(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        match self.patience {
            14.. => out.push("patient: gives a manager time"),
            ..=7 => out.push("impatient: wants results now"),
            _ => {}
        }
        match self.ambition {
            14.. => out.push("ambitious: wants trophies"),
            ..=7 => out.push("modest: a safe season is enough"),
            _ => {}
        }
        match self.spending {
            14.. => out.push("spends freely"),
            ..=7 => out.push("counts every coin"),
            _ => {}
        }
        match self.interference {
            14.. => out.push("likes to be involved in the team"),
            ..=7 => out.push("leaves the team to the manager"),
            _ => {}
        }
        match self.loyalty {
            14.. => out.push("loyal to his people"),
            ..=7 => out.push("quick to replace people"),
            _ => {}
        }
        if out.is_empty() {
            out.push("a steady hand, nothing extreme");
        }
        out
    }

    /// In his own voice: what he expects and what he is like to work for.
    /// `place` is the finish he wants, `n` the size of the league.
    pub fn greeting(&self, club: &str, place: usize, n: usize) -> Vec<String> {
        let mut out = Vec::new();
        out.push(
            match self.interference {
                14.. => "I will be at every game. And at training when I can.",
                ..=7 => "You run the team. I will stay out of your way.",
                _ => "My door is open. Call me when it matters.",
            }
            .to_string(),
        );
        out.push(if place <= 1 {
            format!("{club} must win the title. Nothing less.")
        } else if place <= 3 {
            format!("I want a top-{place} finish and a run in the playoffs.")
        } else if place * 2 <= n + 1 {
            format!("Finish {place} or better. The top half is where this club belongs.")
        } else {
            format!("Finish {place} or better. Stay out of trouble and we are happy.")
        });
        out.push(
            match self.spending {
                14.. => "If there is the right player, tell me. We will find the money.",
                ..=7 => "Money is tight. Every contract goes over my desk.",
                _ => "We have a budget and we keep to it.",
            }
            .to_string(),
        );
        out.push(
            match self.patience {
                14.. => "Building takes time. I will not panic after one bad month.",
                ..=7 => "I have no patience for a long rebuild. Three bad weeks and we talk.",
                _ => "Give me results over a season, not a week.",
            }
            .to_string(),
        );
        out.push(
            match self.loyalty {
                14.. => "The staff have given years to this club. Do not break that lightly.",
                ..=7 => "Change whoever you need to. I will not ask why.",
                _ => "The staff are yours to judge. Judge them fairly.",
            }
            .to_string(),
        );
        out
    }
}

impl Boardroom {
    pub(crate) fn generate(rng: &mut Rng, stature: u8) -> Boardroom {
        let owner = Person::generate(rng, 45, 0);
        let p = match stature {
            55.. => 0.8,
            35..=54 => 0.4,
            _ => 0.1,
        };
        let chairman = rng.chance(p).then(|| Person::generate(rng, 40, 0));
        Boardroom {
            owner,
            chairman,
            confidence: 60.0,
            reasons: Vec::new(),
            warned: false,
            losing_run: 0,
            winning_run: 0,
        }
    }

    /// The person who deals with the manager.
    pub fn face(&self) -> &Person {
        self.chairman.as_ref().unwrap_or(&self.owner)
    }

    pub fn patience(&self) -> u8 {
        self.face().patience
    }

    pub fn ambition(&self) -> u8 {
        self.owner.ambition
    }

    pub fn spending(&self) -> u8 {
        self.owner.spending
    }

    pub fn interference(&self) -> u8 {
        self.face().interference
    }

    pub fn loyalty(&self) -> u8 {
        self.face().loyalty
    }

    pub fn push_reason(&mut self, season: u16, round: u16, delta: f32, text: impl Into<String>) {
        self.reasons.push(Reason {
            season,
            round,
            delta,
            text: text.into(),
        });
        if self.reasons.len() > MAX_REASONS {
            self.reasons.remove(0);
        }
    }
}

// ---- generation ---------------------------------------------------------

const ORIGINS: [&str; 8] = [
    "Started in {y} as the works team of the {c} harbour. The first hoops were nailed to a warehouse wall.",
    "Founded in {y} by teachers and pupils of a {c} school who wanted a winter game.",
    "Born in {y} from a merger of two struggling {c} clubs, which is why old fans still argue about the colours.",
    "Founded in {y} in the back room of a {c} bakery. The baker was the first coach and the first sponsor.",
    "Formed in {y} by railway workers. The club still plays not far from the old yard.",
    "Set up in {y} by a student who came home from abroad having seen the game played.",
    "Started in {y} as a youth project of the {c} council. It outgrew the project quickly.",
    "Founded in {y} by the {c} fire brigade, who needed something to do between shifts.",
];

/// The range of stature a club of this division has, with `slack` more at
/// each end. The bands overlap a little; the top one reaches the top.
pub fn stature_band(division: usize, tiers: usize, slack: i32) -> (i32, i32) {
    let tiers = tiers.max(1) as i32;
    let d = division.min(tiers as usize - 1) as i32;
    let width = 130 / tiers;
    let centre = 100 * (2 * (tiers - d) - 1) / (2 * tiers);
    let lo = (centre - width / 2).max(8) - slack;
    let hi = (centre + width / 2).min(92) + slack;
    (lo.max(1), hi.min(100))
}

/// The city in a club's name: everything before the nickname.
pub fn city_of(name: &str) -> &str {
    name.rsplit_once(' ').map_or(name, |(c, _)| c)
}

impl ClubBook {
    /// `divisions` holds each club's division as the game begins, 0 being the
    /// top: a club of a lower division is a smaller club with a shorter
    /// past.
    pub(crate) fn generate(seed: u64, names: &[String], divisions: &[usize]) -> ClubBook {
        let n = names.len();
        let tiers = divisions.iter().map(|d| d + 1).max().unwrap_or(1);
        let mut rng = Rng::new(seed ^ 0xC1B_0001);
        let base: Vec<i32> = (0..n)
            .map(|c| {
                let raw = 12 + (rng.range(0, 76) + rng.range(0, 76)) as i32 / 2;
                if tiers == 1 {
                    return raw;
                }
                // The size of a club follows the division it plays in; the
                // bands overlap, so a big club can be down and a small one up.
                let (lo, hi) = stature_band(divisions[c], tiers, 0);
                lo + (raw - 12) * (hi - lo) / 76
            })
            .collect();
        // Founded: the big clubs are old, with some noise.
        let window_start = START_YEAR - PAST_YEARS;
        let mut founded: Vec<u16> = (0..n)
            .map(|c| {
                let y = 1890 + (100 - base[c]) * 9 / 10 + rng.range(0, 31) as i32 - 15;
                y.clamp(1890, 2012) as u16
            })
            .collect();
        // At least four clubs have always been there.
        let mut by_age: Vec<usize> = (0..n).collect();
        by_age.sort_by_key(|&c| (founded[c], c));
        for &c in by_age.iter().take(4) {
            founded[c] = founded[c].min(window_start - 1 - (c as u16 % 6));
        }

        // The years before the game: each club has a strength that wanders
        // around what it is, and the best of them wins.
        let mut power: Vec<i32> = base.iter().map(|b| b * 10).collect();
        let mut past = Vec::new();
        for year in window_start..START_YEAR {
            let mut here: Vec<(i32, usize)> = Vec::new();
            for c in 0..n {
                if founded[c] > year {
                    continue;
                }
                power[c] += rng.range(0, 41) as i32 - 20 + (base[c] * 10 - power[c]) / 8;
                let form = power[c] + rng.range(0, 201) as i32 - 100;
                here.push((form, c));
            }
            here.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            // The year's pyramid: the best clubs form the top division, as
            // many as there are today, and so on down.
            let mut sizes = Vec::new();
            let mut left = here.len();
            for d in 0..tiers {
                let size = if d + 1 == tiers {
                    left
                } else {
                    divisions.iter().filter(|&&x| x == d).count().min(left)
                };
                sizes.push(size as u8);
                left -= size;
            }
            past.push(PastSeason {
                year,
                order: here.iter().map(|&(_, c)| c as u8).collect(),
                sizes,
            });
        }

        // Today's stature: what the club is, and how it has done lately.
        let recent = |c: usize| -> Option<i32> {
            let mut sum = 0;
            let mut k = 0;
            for s in past.iter().rev().take(15) {
                if let Some(r) = s.order.iter().position(|&x| x as usize == c) {
                    let m = s.order.len() as i32;
                    sum += if m > 1 { 100 * (m - 1 - r as i32) / (m - 1) } else { 50 };
                    k += 1;
                }
            }
            (k > 0).then(|| sum / k)
        };
        let stature: Vec<u8> = (0..n)
            .map(|c| match recent(c) {
                Some(r) => ((2 * base[c] + r) / 3).clamp(5, 95) as u8,
                None => base[c].clamp(5, 95) as u8,
            })
            .collect();

        // Rivals: clubs of about the same size, paired off.
        let mut pairing: Vec<(i32, usize)> = (0..n)
            .map(|c| (stature[c] as i32 * 4 + rng.range(0, 40) as i32, c))
            .collect();
        pairing.sort();
        let mut rival = vec![0usize; n];
        for pair in pairing.chunks(2) {
            if let [a, b] = pair {
                rival[a.1] = b.1;
                rival[b.1] = a.1;
            }
        }

        let clubs = (0..n)
            .map(|c| {
                let r = &mut Rng::new(seed ^ 0xC1B_0002 ^ ((c as u64) << 32));
                let identity = Self::identity_of(
                    r, c, &names[c], founded[c], stature[c], rival[c], &past,
                );
                let board = Boardroom::generate(r, stature[c]);
                ClubState { identity, board }
            })
            .collect();
        ClubBook {
            clubs,
            past,
            pending: None,
        }
    }

    fn identity_of(
        rng: &mut Rng,
        c: TeamId,
        name: &str,
        founded: u16,
        stature: u8,
        rival: TeamId,
        past: &[PastSeason],
    ) -> Identity {
        let city = city_of(name);
        let origin = ORIGINS[rng.range(0, ORIGINS.len() as u32) as usize]
            .replace("{y}", &founded.to_string())
            .replace("{c}", city);
        let titles: Vec<u16> = past
            .iter()
            .filter(|s| s.order.first().is_some_and(|&x| x as usize == c))
            .map(|s| s.year)
            .collect();
        let finals: Vec<u16> = past
            .iter()
            .filter(|s| s.order.get(1).is_some_and(|&x| x as usize == c))
            .map(|s| s.year)
            .collect();
        let count = 1 + (stature as u32 / 30).min(2) + rng.range(0, 2);
        let mut legends = Vec::new();
        for i in 0..count as usize {
            let name = format!(
                "{} {}",
                FIRST_NAMES[rng.range(0, FIRST_NAMES.len() as u32) as usize],
                LAST_NAMES[rng.range(0, LAST_NAMES.len() as u32) as usize]
            );
            let position = Position::ALL[rng.range(0, 5) as usize];
            let kind = if i % 2 == 0 && !titles.is_empty() {
                0
            } else {
                1 + rng.range(0, 3)
            };
            let (from, to, known_for) = match kind {
                0 => {
                    let y = titles[(rng.range(0, titles.len() as u32)) as usize];
                    (
                        y.saturating_sub(rng.range(2, 6) as u16).max(founded),
                        y + rng.range(1, 5) as u16,
                        format!("Captain of the {y} champions."),
                    )
                }
                1 if !finals.is_empty() => {
                    let y = finals[rng.range(0, finals.len() as u32) as usize];
                    (
                        y.saturating_sub(3).max(founded),
                        y + 2,
                        format!("Lost the {y} final and still says he was fouled."),
                    )
                }
                2 => {
                    let from = START_YEAR - rng.range(12, 35) as u16;
                    let from = from.max(founded);
                    let years = rng.range(9, 16) as u16;
                    (
                        from,
                        (from + years).min(START_YEAR - 2),
                        format!("Played {years} seasons here and never for another club."),
                    )
                }
                _ => {
                    let from = (START_YEAR - rng.range(8, 30) as u16).max(founded);
                    let pts = rng.range(41, 62);
                    (
                        from,
                        (from + 6).min(START_YEAR - 2),
                        format!(
                            "Scored {pts} points in one game in {}. It is still the club record.",
                            from + 3
                        ),
                    )
                }
            };
            legends.push(Legend {
                name,
                position,
                from,
                to: to.max(from),
                known_for,
            });
        }
        Identity {
            founded,
            origin,
            stature,
            rival,
            legends,
        }
    }
}

// ---- the record -----------------------------------------------------------

/// One season of a club's life: before the game from the old tables, after
/// it from the seasons that were played.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct YearLine {
    pub year: u16,
    /// 1 = top of the table; 0 if the club did not exist.
    pub place: u8,
    /// Champion of its own division, which need not be the top one.
    pub champion: bool,
    /// Runner-up of the top division.
    pub runner_up: bool,
    /// Played in this game rather than before it.
    pub played: bool,
    /// The division, 0 being the top.
    #[serde(default)]
    pub division: u8,
    /// 1 if the club went up after the season, -1 if it went down.
    #[serde(default)]
    pub moved: i8,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Honours {
    /// Years the top division was won.
    pub titles: Vec<u16>,
    pub finals: Vec<u16>,
    /// A division below the top won, by year and division.
    pub lower_titles: Vec<(u16, u8)>,
    /// Years after which the club went up.
    pub promotions: Vec<u16>,
    /// Best league place and the first year it was reached, in the highest
    /// division the club has played in.
    pub best: Option<(u8, u16)>,
    pub best_division: u8,
    pub seasons: usize,
}

impl World {
    /// Make sure every club has an identity and a board: a new world, or an
    /// old save.
    pub fn ensure_clubs(&mut self) {
        let n = self.teams.len();
        if self.club.clubs.len() == n {
            return;
        }
        let seed = if self.econ.seed == 0 {
            0xC1B_5EED
        } else {
            self.econ.seed
        };
        let names: Vec<String> = self.teams.iter().map(|t| t.name.clone()).collect();
        let divisions: Vec<usize> = self.teams.iter().map(|t| t.division).collect();
        self.club = ClubBook::generate(seed, &names, &divisions);
    }

    pub fn identity(&self, t: TeamId) -> &Identity {
        &self.club.clubs[t].identity
    }

    pub fn boardroom(&self, t: TeamId) -> &Boardroom {
        &self.club.clubs[t].board
    }

    pub fn year(&self) -> u16 {
        START_YEAR + self.season
    }

    /// How big and how respected a club is, 1..=100. The one number other
    /// systems should ask: what the club was when the game began, moved by
    /// how it has done in the seasons since. Places count in the pyramid:
    /// first in the Third Division is worth less than twelfth in the top
    /// one. The division the club plays in now bounds it.
    pub fn stature(&self, t: TeamId) -> u8 {
        let base = self.club.clubs.get(t).map_or(50, |c| c.identity.stature) as i32;
        let tiers = self.division_count();
        let recent: Vec<i32> = self
            .history
            .iter()
            .rev()
            .take(5)
            .map(|r| self.pyramid_percent(r, t))
            .collect();
        let mut s = base;
        if !recent.is_empty() {
            let k = recent.len() as i32;
            let avg = recent.iter().sum::<i32>() / k;
            // Five seasons move it half way.
            s = (base * (10 - k) + avg * k) / 10;
        }
        let division = self.teams.get(t).map_or(0, |x| x.division);
        let (lo, hi) = stature_band(division, tiers, 10);
        s.clamp(lo, hi).clamp(1, 100) as u8
    }

    /// 0..=100 for a season's finish, the best club of the pyramid being
    /// 100 and the worst 0. A club that did not play is in the middle.
    fn pyramid_percent(&self, r: &crate::season::SeasonRecord, t: TeamId) -> i32 {
        let place = r.place(t);
        let Some(d) = r.division_of(t).filter(|_| place > 0) else {
            return 50;
        };
        let (above, total) = if r.divisions.is_empty() {
            (0, r.table.len())
        } else {
            (
                r.divisions[..d].iter().map(|x| x.len()).sum::<usize>(),
                r.divisions.iter().map(|x| x.len()).sum::<usize>(),
            )
        };
        let (above, total) = (above as i32, total as i32);
        if total > 1 {
            100 * (total - above - place as i32) / (total - 1)
        } else {
            50
        }
    }

    /// Every season of a club's life, oldest first, one line each.
    pub fn club_record(&self, t: TeamId) -> Vec<YearLine> {
        let mut out = Vec::new();
        for s in &self.club.past {
            let found = s.place_of(t);
            out.push(YearLine {
                year: s.year,
                place: found.map_or(0, |(_, p)| p as u8),
                division: found.map_or(0, |(d, _)| d as u8),
                champion: found.is_some_and(|(_, p)| p == 1),
                runner_up: found.is_some_and(|(_, p)| p == 2),
                played: false,
                moved: 0,
            });
        }
        for r in &self.history {
            let division = r.division_of(t);
            let place = r.place(t);
            out.push(YearLine {
                year: START_YEAR + r.season,
                place: place as u8,
                division: division.unwrap_or(0) as u8,
                champion: division.is_some_and(|d| {
                    if d == 0 { r.champion == t } else { r.division_champions.get(d - 1) == Some(&t) }
                }),
                runner_up: division == Some(0) && r.runner_up == Some(t),
                played: true,
                moved: 0,
            });
        }
        // Promotion and relegation: the division of the next year, and for
        // the last, the division the club plays in now.
        for i in 0..out.len() {
            if out[i].place == 0 {
                continue;
            }
            let next = out.get(i + 1).map_or(Some(self.teams[t].division as u8), |n| {
                (n.place > 0).then_some(n.division)
            });
            out[i].moved = match next {
                Some(d) if d < out[i].division => 1,
                Some(d) if d > out[i].division => -1,
                _ => 0,
            };
        }
        out
    }

    pub fn honours(&self, t: TeamId) -> Honours {
        let rec = self.club_record(t);
        // Best finish: the highest division first, then the place.
        let mut best: Option<(u8, u8, u16)> = None;
        for l in rec.iter().filter(|l| l.place > 0) {
            if best.is_none_or(|(d, p, _)| (l.division, l.place) < (d, p)) {
                best = Some((l.division, l.place, l.year));
            }
        }
        Honours {
            titles: rec.iter().filter(|l| l.champion && l.division == 0).map(|l| l.year).collect(),
            finals: rec.iter().filter(|l| l.runner_up && l.division == 0).map(|l| l.year).collect(),
            lower_titles: rec
                .iter()
                .filter(|l| l.champion && l.division > 0)
                .map(|l| (l.year, l.division))
                .collect(),
            promotions: rec.iter().filter(|l| l.moved > 0).map(|l| l.year).collect(),
            best: best.map(|(_, p, y)| (p, y)),
            best_division: best.map_or(0, |(d, _, _)| d),
            seasons: rec.iter().filter(|l| l.place > 0).count(),
        }
    }

    /// What the club's own people say the fans want.
    pub fn fans_expect(&self, t: TeamId) -> &'static str {
        match self.stature(t) {
            75.. => "The fans expect titles. Second place is a disappointment.",
            55..=74 => "The fans expect a place in the playoffs every year.",
            35..=54 => "The fans want a solid mid-table side that beats the rival.",
            _ => "The fans want to stay up and to beat the rival once a year.",
        }
    }
}

// ---- the introduction -----------------------------------------------------

/// A player at a glance.
#[derive(Clone, Debug, Serialize)]
pub struct Glance {
    pub id: u32,
    pub name: String,
    pub position: Position,
    pub age: u8,
    pub overall: f32,
    pub salary: u32,
    pub years_left: u8,
    pub injured: bool,
}

/// How strong a position is, against the league.
#[derive(Clone, Debug, Serialize)]
pub struct PositionLine {
    pub position: Position,
    pub best: f32,
    pub players: usize,
    pub verdict: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct SquadView {
    pub stars: Vec<Glance>,
    pub positions: Vec<PositionLine>,
    pub average_age: f32,
    pub expiring: Vec<String>,
    pub injured: Vec<String>,
    pub strength_rank: usize,
    /// The assistant's view, plainly.
    pub assistant: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ArenaView {
    pub name: String,
    pub capacity: u32,
    pub condition: f32,
    pub special: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StaffLine {
    pub job: &'static str,
    pub name: String,
    pub rating: u8,
    pub skill: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct PersonView {
    pub role: &'static str,
    pub name: String,
    pub age: u8,
    pub background: String,
    pub traits: Vec<&'static str>,
    pub says: Vec<String>,
}

/// Everything the introduction and the club page show.
#[derive(Clone, Debug, Serialize)]
pub struct ClubSummary {
    pub team: TeamId,
    pub name: String,
    pub founded: u16,
    pub origin: String,
    pub stature: u8,
    pub stature_word: &'static str,
    pub rival: String,
    pub rival_id: TeamId,
    pub titles: Vec<u16>,
    pub finals: Vec<u16>,
    pub best: Option<(u8, u16)>,
    pub best_division: u8,
    pub lower_titles: Vec<(u16, u8)>,
    pub promotions: Vec<u16>,
    /// The division the club plays in now, and its name.
    pub division: usize,
    pub division_name: String,
    /// How many clubs are in it.
    pub division_size: usize,
    pub legends: Vec<Legend>,
    pub fans_expect: &'static str,
    pub record: Vec<YearLine>,
    pub people: Vec<PersonView>,
    pub confidence: f32,
    pub confidence_word: &'static str,
    pub warned: bool,
    pub reasons: Vec<Reason>,
    pub expected_place: usize,
    pub gm: StaffLine,
    pub staff: Vec<StaffLine>,
    pub arena: ArenaView,
    pub squad: SquadView,
}

pub fn stature_word(s: u8) -> &'static str {
    match s {
        80.. => "one of the giants of the league",
        60..=79 => "an established name",
        40..=59 => "a club of the middle",
        20..=39 => "a small club",
        _ => "a newcomer with little behind it",
    }
}

pub fn confidence_word(c: f32) -> &'static str {
    match c as i32 {
        80.. => "delighted",
        60..=79 => "pleased",
        40..=59 => "watching",
        25..=39 => "worried",
        _ => "close to losing patience",
    }
}

impl World {
    pub fn club_summary(&self, t: TeamId) -> ClubSummary {
        let id = self.identity(t);
        let board = self.boardroom(t);
        let honours = self.honours(t);
        let n = self.division_size(t);
        let expected = self.expected_finish(t);
        let mut people = Vec::new();
        let mut push = |role: &'static str, p: &Person, speaks: bool| {
            people.push(PersonView {
                role,
                name: p.name.clone(),
                age: p.age,
                background: p.background.clone(),
                traits: p.traits(),
                says: if speaks {
                    p.greeting(&self.teams[t].name, expected, n)
                } else {
                    Vec::new()
                },
            });
        };
        match &board.chairman {
            Some(c) => {
                push("Chairman", c, true);
                push("Owner", &board.owner, false);
            }
            None => push("Owner and chairman", &board.owner, true),
        }
        let office = &self.teams[t].office;
        let gm = &office.gm;
        let gm_line = StaffLine {
            job: "General manager",
            name: gm.name.clone(),
            rating: gm.judgement,
            skill: "Judgement",
        };
        let _ = gm;
        let staff = Job::ALL
            .iter()
            .filter(|&&j| j != Job::HeadCoach)
            .filter_map(|&j| {
                office.staffer(j).map(|s| StaffLine {
                    job: j.label(),
                    name: s.name.clone(),
                    rating: s.rating(j),
                    skill: j.skill(),
                })
            })
            .collect();
        ClubSummary {
            team: t,
            name: self.teams[t].name.clone(),
            founded: id.founded,
            origin: id.origin.clone(),
            stature: self.stature(t),
            stature_word: stature_word(self.stature(t)),
            rival: self.teams[id.rival].name.clone(),
            rival_id: id.rival,
            titles: honours.titles,
            finals: honours.finals,
            best: honours.best,
            best_division: honours.best_division,
            lower_titles: honours.lower_titles,
            promotions: honours.promotions,
            division: self.teams[t].division,
            division_name: self.division_name(self.teams[t].division),
            division_size: n,
            legends: id.legends.clone(),
            fans_expect: self.fans_expect(t),
            record: self.club_record(t),
            people,
            confidence: board.confidence,
            confidence_word: confidence_word(board.confidence),
            warned: board.warned,
            reasons: board.reasons.clone(),
            expected_place: expected,
            gm: gm_line,
            staff,
            arena: self.club_arena(t),
            squad: self.squad_view(t),
        }
    }

    fn club_arena(&self, t: TeamId) -> ArenaView {
        let a = self.arena(t);
        let mut special = Vec::new();
        let mates = self.division_teams(self.teams[t].division);
        let biggest = mates.iter().all(|&o| self.arena(o).capacity <= a.capacity);
        let smallest = mates.iter().all(|&o| self.arena(o).capacity >= a.capacity);
        if biggest {
            special.push("The biggest hall in the division.".to_string());
        }
        if smallest {
            special.push("The smallest hall in the division. Visitors say it is loud.".to_string());
        }
        if a.look.two_tiers {
            special.push("Two tiers of seats. The upper one hangs over the floor.".into());
        }
        if a.look.behind_baskets {
            special.push("Seats behind both baskets, close to the play.".into());
        }
        if a.condition < 0.45 {
            special.push("It is worn. The roof leaks in heavy rain.".into());
        } else if a.condition > 0.8 {
            special.push("In very good order.".into());
        }
        if a.support > 0.7 {
            special.push("The town packs it, and it is loud.".into());
        }
        if special.is_empty() {
            special.push("A plain hall with a good floor.".into());
        }
        ArenaView {
            name: a.name.clone(),
            capacity: a.capacity,
            condition: a.condition,
            special,
        }
    }

    fn squad_view(&self, t: TeamId) -> SquadView {
        let team = &self.teams[t];
        let strength = |c: TeamId| -> f32 {
            let mut ov: Vec<f32> = self.teams[c]
                .roster
                .iter()
                .map(|&p| self.players[p as usize].overall())
                .collect();
            ov.sort_by(|a, b| b.total_cmp(a));
            ov.iter().take(8).sum()
        };
        let mine = strength(t);
        let mates = self.division_teams(team.division);
        let rank = 1 + mates.iter().filter(|&&c| strength(c) > mine).count();
        let glance = |p: u32| {
            let pl = &self.players[p as usize];
            Glance {
                id: p,
                name: pl.name.clone(),
                position: pl.position,
                age: pl.age,
                overall: pl.overall(),
                salary: pl.contract.salary,
                years_left: pl.contract.years_left,
                injured: pl.injury.is_some(),
            }
        };
        let mut all: Vec<Glance> = team.roster.iter().map(|&p| glance(p)).collect();
        all.sort_by(|a, b| b.overall.total_cmp(&a.overall));
        // The league's best at each position, for comparison.
        let positions: Vec<PositionLine> = Position::ALL
            .iter()
            .map(|&pos| {
                let best = all
                    .iter()
                    .filter(|g| g.position == pos)
                    .map(|g| g.overall)
                    .fold(0.0, f32::max);
                let players = all.iter().filter(|g| g.position == pos).count();
                let mut league: Vec<f32> = mates
                    .iter()
                    .map(|&c| {
                        self.teams[c]
                            .roster
                            .iter()
                            .map(|&p| &self.players[p as usize])
                            .filter(|p| p.position == pos)
                            .map(|p| p.overall())
                            .fold(0.0, f32::max)
                    })
                    .collect();
                league.sort_by(|a, b| b.total_cmp(a));
                let better = league.iter().filter(|&&x| x > best).count();
                let verdict = if players == 0 {
                    "nobody there"
                } else if better * 3 < league.len() {
                    "strong"
                } else if better * 3 < league.len() * 2 {
                    "fair"
                } else {
                    "weak"
                };
                PositionLine {
                    position: pos,
                    best,
                    players,
                    verdict,
                }
            })
            .collect();
        let average_age = if all.is_empty() {
            0.0
        } else {
            all.iter().map(|g| g.age as f32).sum::<f32>() / all.len() as f32
        };
        let expiring: Vec<String> = all
            .iter()
            .filter(|g| g.years_left <= 1)
            .map(|g| g.name.clone())
            .collect();
        let injured: Vec<String> = all
            .iter()
            .filter(|g| g.injured)
            .map(|g| g.name.clone())
            .collect();
        let n = mates.len();
        let mut assistant = Vec::new();
        assistant.push(if rank <= 3 {
            format!("On paper we are the number {rank} squad. We should be at the top.")
        } else if rank * 2 <= n + 1 {
            format!("We rank {rank} of {n} on paper. Good enough for the playoffs if we are sharp.")
        } else {
            format!("Honestly, we rank {rank} of {n}. We are the underdogs most weeks.")
        });
        if let Some(w) = positions.iter().find(|p| p.verdict == "weak") {
            assistant.push(format!(
                "Our weak spot is {}. We need help there.",
                w.position.label()
            ));
        }
        if let Some(s) = positions.iter().find(|p| p.verdict == "strong") {
            assistant.push(format!("{} is where we are strongest.", s.position.label()));
        }
        if average_age > 28.5 {
            assistant.push("The squad is old. The legs will go before the heads do.".into());
        } else if average_age < 24.5 {
            assistant.push("The squad is young. Expect mistakes, and growth.".into());
        }
        if !expiring.is_empty() {
            assistant.push(format!(
                "Contracts end soon for {}.",
                expiring.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        if !injured.is_empty() {
            assistant.push(format!("Injured now: {}.", injured.join(", ")));
        }
        SquadView {
            stars: all.iter().take(3).cloned().collect(),
            positions,
            average_age,
            expiring,
            injured,
            strength_rank: rank,
            assistant,
        }
    }
}
