//! Media and community work.
//!
//! The manager gives each player a weekly level of media work: none, some
//! or a lot (`MediaLevel`). It raises that player's fame a little and the
//! club's fan base (`media_pull`), and it costs: his training gains that
//! week shrink in proportion (`media_factors`) and he is a little more
//! tired (`media_week`). Sponsors also write with appearance requests, an
//! event on Thursday for a named player, for a fee through the ledger
//! (`Appearance`); one the manager accepts costs a little training and
//! freshness too.
//!
//! The hidden trait `Personality::presence` (1..=20) is how well a player
//! comes across. Whoever speaks in public can say the wrong thing (a gaffe:
//! belief in the dressing room, a teammate's mood, an annoyed sponsor, a
//! story) or the right one (a glowing piece: fan base, his own mood). A poor
//! presence, a hot temper and low professionalism make a gaffe likelier; a
//! good presence makes the good story likelier. The club's press officer
//! (`Job::Press`, rated on `Staffer::media`) catches a share of gaffes
//! before they get out (`catch_rate`) and sets how fast media work improves
//! a player's presence (`presence_growth`).
//!
//! Randomness comes from a stream derived from `Economy::seed`, never from
//! `World::rng`.

use serde::{Deserialize, Serialize};

use crate::dressing::Topic;
use crate::finance::Cat;
use crate::inbox::{Kind, OfferState, date, money};
use crate::league::{TeamId, World};
use crate::player::PlayerId;
use crate::rng::Rng;
use crate::staff::Job;

/// How much media work a player is given each week.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaLevel {
    #[default]
    None,
    Some,
    ALot,
}

impl MediaLevel {
    pub const ALL: [MediaLevel; 3] = [MediaLevel::None, MediaLevel::Some, MediaLevel::ALot];

    pub fn label(self) -> &'static str {
        match self {
            MediaLevel::None => "None",
            MediaLevel::Some => "Some",
            MediaLevel::ALot => "A lot",
        }
    }

    /// How much work it is; the effects scale with it.
    pub fn weight(self) -> f32 {
        match self {
            MediaLevel::None => 0.0,
            MediaLevel::Some => 1.0,
            MediaLevel::ALot => 2.5,
        }
    }
}

/// What a fresh player's presence is: two rolls averaged like the other
/// traits, from his id alone so that adding it moved no other random
/// number in the world.
pub(crate) fn presence_roll(id: PlayerId) -> u8 {
    let mut rng = Rng::new(0x9E5E_0C1A_0000_0000 ^ (id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    1 + ((rng.range(0, 20) + rng.range(0, 20)) / 2) as u8
}

/// Presence in words, for a manager who knows the player.
pub fn presence_words(presence: u8) -> &'static str {
    match presence {
        16.. => "Natural in front of a camera",
        12..=15 => "Comes across well",
        7..=11 => "Fine in public, nothing more",
        4..=6 => "Awkward in public",
        _ => "A liability with a microphone",
    }
}

/// A sponsor's request for one of the user's players.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Appearance {
    /// Index into `Economy::sponsors`.
    pub sponsor: usize,
    pub player: PlayerId,
    /// The day of the event.
    pub event_day: u32,
    /// Last day it can be accepted.
    pub expires: u32,
    pub fee: i64,
    pub state: OfferState,
    /// The event has taken place.
    pub held: bool,
}

impl Appearance {
    pub fn open_on(&self, day: u32) -> bool {
        self.state == OfferState::Open && self.expires >= day
    }
}

/// Chance in a week that a player says something he should not, before the
/// press officer: from his presence, temper and professionalism, and from
/// how much he is in front of people.
pub fn gaffe_chance(
    presence: u8,
    temperament: u8,
    professionalism: u8,
    level: MediaLevel,
    fame: f32,
) -> f32 {
    let exposure = 0.35 + 0.30 * level.weight() + fame / 250.0;
    let risk = (21.0 - presence as f32) / 10.0
        * (1.0 + (10.0 - temperament as f32) / 14.0).clamp(0.6, 1.7)
        * (1.0 + (8.0 - professionalism as f32) / 16.0).clamp(0.7, 1.4);
    0.015 * exposure * risk
}

/// Chance in a week of a good story about a player.
pub fn story_chance(presence: u8, level: MediaLevel, fame: f32) -> f32 {
    if presence < 12 {
        return 0.0;
    }
    let exposure = 0.35 + 0.30 * level.weight() + fame / 250.0;
    0.013 * exposure * (presence as f32 - 9.0) / 5.0
}

impl World {
    /// Share of gaffes at `club` the press officer stops, 0..=1.
    pub fn catch_rate(&self, club: TeamId) -> f32 {
        0.04 * self.teams[club].office.rating(Job::Press) as f32
    }

    /// Presence points a week of media work at `level` is worth at `club`.
    pub fn presence_growth(&self, club: TeamId, level: MediaLevel) -> f32 {
        level.weight() * (0.015 + 0.004 * self.teams[club].office.rating(Job::Press) as f32)
    }

    /// A player's chance of a gaffe this week, before the press officer.
    pub fn gaffe_risk(&self, p: PlayerId) -> f32 {
        let pl = &self.players[p as usize];
        gaffe_chance(
            pl.personality.presence,
            pl.personality.temperament,
            pl.personality.professionalism,
            pl.renown.media,
            pl.renown.fame,
        )
    }

    /// The share of the fan base a week of the club's media work adds.
    pub(crate) fn media_pull(&self, club: TeamId) -> f32 {
        self.teams[club]
            .roster
            .iter()
            .map(|&p| {
                let pl = &self.players[p as usize];
                pl.renown.media.weight() * (0.3 + pl.renown.fame / 100.0) * 0.0003
            })
            .sum()
    }

    /// For every player, what is left of this week's training after his
    /// media work and appearances: 1 for most.
    pub(crate) fn media_factors(&self) -> Vec<f32> {
        self.players
            .iter()
            .map(|p| (1.0 - 0.07 * p.renown.media.weight() - 0.08 * p.renown.busy as f32).max(0.5))
            .collect()
    }

    /// Give player `p` of the user's club a level of media work.
    pub fn set_media(&mut self, p: PlayerId, level: MediaLevel) {
        self.players[p as usize].renown.media = level;
    }

    // ---- the week ---------------------------------------------------------

    /// After a round has been played and trained: fame and fan base, what
    /// people say in public, and the next appearance request.
    pub(crate) fn fame_week(&mut self, played: &[usize]) {
        self.fans_week(played);
        let regular = played.iter().any(|&i| !self.fixtures[i].playoff);
        let mut rng = Rng::new(
            self.econ.seed ^ 0x3ED1_A000 ^ ((self.season as u64) << 24) ^ self.round as u64,
        );
        for club in 0..self.teams.len() {
            self.media_week(club, &mut rng);
        }
        if regular {
            self.appearance_request(&mut rng);
        }
        for p in &mut self.players {
            p.renown.busy = 0;
        }
    }

    fn media_week(&mut self, club: TeamId, rng: &mut Rng) {
        let catch = self.catch_rate(club);
        for p in self.teams[club].roster.clone() {
            let (level, presence, fame, fit) = {
                let pl = &self.players[p as usize];
                (
                    pl.renown.media,
                    pl.personality.presence,
                    pl.renown.fame,
                    pl.fit(),
                )
            };
            let w = level.weight();
            if w > 0.0 && fit {
                let growth = self.presence_growth(club, level);
                let pl = &mut self.players[p as usize];
                let r = &mut pl.renown;
                let gain = 0.15 * w * (0.6 + presence as f32 / 25.0) * (1.0 - fame / 60.0).max(0.0);
                r.fame = (r.fame + gain).min(100.0);
                r.peak = r.peak.max(r.fame);
                pl.condition = (pl.condition - 0.006 * w).max(0.7);
                r.xp += growth;
                if r.xp >= 1.0 && pl.personality.presence < 20 {
                    r.xp -= 1.0;
                    pl.personality.presence += 1;
                }
            }
            if rng.chance(self.gaffe_risk(p)) {
                if rng.chance(catch) {
                    self.fame.counts.caught += 1;
                } else {
                    self.gaffe(club, p, rng);
                }
            } else if rng.chance(story_chance(presence, level, fame)) {
                self.good_story(club, p);
            }
        }
    }

    /// He said the wrong thing.
    fn gaffe(&mut self, club: TeamId, p: PlayerId, rng: &mut Rng) {
        self.fame.counts.gaffes += 1;
        let (name, temper, prof) = {
            let pl = &self.players[p as usize];
            (
                pl.name.clone(),
                pl.personality.temperament,
                pl.personality.professionalism,
            )
        };
        let what = if temper <= 6 {
            "lost his temper with a reporter and said things about the referees"
        } else if prof <= 6 {
            "told a radio audience that training was a joke"
        } else {
            "put his foot in it in an interview about his teammates"
        };
        let b = &mut self.teams[club].belief;
        *b = (*b - 0.012).clamp(0.0, 1.0);
        let pl = &mut self.players[p as usize];
        pl.morale = (pl.morale - 0.03).clamp(0.0, 1.0);
        pl.renown.fame = (pl.renown.fame - 0.6).max(0.0);
        // A teammate takes it badly.
        let mates: Vec<PlayerId> = self.teams[club]
            .roster
            .iter()
            .copied()
            .filter(|&q| q != p)
            .collect();
        let mate = if mates.is_empty() {
            None
        } else {
            Some(mates[rng.range(0, mates.len() as u32) as usize])
        };
        if let Some(m) = mate {
            let mp = &mut self.players[m as usize];
            mp.morale = (mp.morale - 0.05).clamp(0.0, 1.0);
        }
        self.fans_move(club, crate::fans::C_MEDIA, -0.002);
        // A sponsor does not like it.
        let mut docked = 0;
        if let Some(d) = self.econ.deals.iter().find(|d| d.club == club) {
            docked = (d.fee / 20).max(0);
        }
        if docked > 0 {
            self.transact(club, -docked, Cat::Sponsorship);
        }
        if self.watch == Some(club) {
            let mut body = format!(
                "{name} {what}. It is in the papers and on the radio.\n\nThe dressing room is \
                 not pleased",
            );
            if let Some(m) = mate {
                body += &format!(", {} least of all", self.players[m as usize].name);
            }
            body += ".";
            if docked > 0 {
                body += &format!(
                    " One of our sponsors has docked {} from what it owes us.",
                    money(docked)
                );
            }
            body += &format!(
                "\n\n{}",
                if self.teams[club].office.staffer(Job::Press).is_none() {
                    "A press officer would catch some of these before they get out."
                } else {
                    "Our press officer could not stop this one."
                }
            );
            self.post(
                self.day,
                Kind::Media,
                "The press",
                format!("{name} in the papers, for the wrong reasons"),
                body,
                vec![p],
            );
        }
    }

    /// A glowing piece about him.
    fn good_story(&mut self, club: TeamId, p: PlayerId) {
        self.fame.counts.stories += 1;
        let name = self.players[p as usize].name.clone();
        let pl = &mut self.players[p as usize];
        pl.morale = (pl.morale + 0.05).clamp(0.0, 1.0);
        pl.renown.fame = (pl.renown.fame + 0.7).min(100.0);
        pl.renown.peak = pl.renown.peak.max(pl.renown.fame);
        self.fans_move(club, crate::fans::C_MEDIA, 0.0035);
        if self.watch == Some(club) {
            let body = format!(
                "A long and warm piece about {name} ran in the weekend paper: how he comes \
                 across, what he does for the town, why people like having him around. The \
                 fans are talking about it and he is delighted.",
            );
            self.post(
                self.day,
                Kind::Media,
                "The press",
                format!("A good week for {name} in the papers"),
                body,
                vec![p],
            );
        }
    }

    // ---- appearances -----------------------------------------------------

    /// Maybe a sponsor asks for one of the user's players.
    fn appearance_request(&mut self, rng: &mut Rng) {
        let Some(me) = self.watch else { return };
        if self.round + 2 >= 2 * self.teams.len().saturating_sub(1) {
            return;
        }
        let today = (self.round as u32 - 1) * 7 + crate::inbox::MATCH_DAY;
        if self.fame.asks.iter().any(|a| a.open_on(today + 1)) {
            return;
        }
        let first = self.fame.asks.is_empty();
        if !first && !rng.chance(0.28) {
            return;
        }
        // The one a sponsor would like: known, and good in front of people.
        let who = self.teams[me]
            .roster
            .iter()
            .copied()
            .filter(|&p| self.players[p as usize].fit())
            .filter(|&p| self.fame_of(p) >= 1.0)
            .max_by(|&a, &b| {
                let s = |p: PlayerId| {
                    let pl = &self.players[p as usize];
                    (pl.renown.fame + 2.0) * (0.5 + pl.personality.presence as f32 / 20.0)
                };
                s(a).total_cmp(&s(b))
            });
        let Some(player) = who else { return };
        let deals: Vec<usize> = self
            .econ
            .deals
            .iter()
            .filter(|d| d.club == me)
            .map(|d| d.sponsor)
            .collect();
        let sponsor = if deals.is_empty() {
            rng.range(0, self.econ.sponsors.len() as u32) as usize
        } else {
            deals[rng.range(0, deals.len() as u32) as usize]
        };
        let tier = self.econ.sponsors[sponsor].tier as f32;
        let fee = ((1_200.0 + 90.0 * self.fame_of(player)) * (0.7 + 0.2 * tier)) as i64 / 100 * 100;
        let event_day = today + 5;
        let idx = self.fame.asks.len();
        let name = self.players[player as usize].name.clone();
        let company = self.econ.sponsors[sponsor].name.clone();
        let kind = self.players[player as usize].personality.kind();
        let body = format!(
            "{company} would like {name} at one of their events on {}. A few hours of \
             handshakes, photographs and a short speech, for {} from them.\n\nIt takes his \
             day: some of the week's training goes, and he will be tired for Saturday. \
             People who know him describe him as: {}. If we do not answer by {} the \
             request lapses.",
            date(event_day),
            money(fee),
            kind.label().to_lowercase(),
            date(today + 4),
        );
        self.fame.asks.push(Appearance {
            sponsor,
            player,
            event_day,
            expires: today + 4,
            fee,
            state: OfferState::Open,
            held: false,
        });
        let subject = format!("{company} wants {name} on Thursday");
        let m = self.post(
            today + 1,
            Kind::Board,
            &company,
            subject,
            body,
            vec![player],
        );
        m.appearance = Some(idx);
    }

    /// True while request `i` can still be answered.
    pub fn appearance_open(&self, i: usize) -> bool {
        self.fame.asks.get(i).is_some_and(|a| {
            a.open_on(self.day)
                && self
                    .watch
                    .is_some_and(|me| self.teams[me].roster.contains(&a.player))
        })
    }

    /// Accept or turn down request `i`.
    pub fn answer_appearance(&mut self, i: usize, accept: bool) {
        let Some(a) = self.fame.asks.get_mut(i) else {
            return;
        };
        a.state = if accept {
            OfferState::Accepted
        } else {
            OfferState::Rejected
        };
    }

    /// Each day: the events that are due take place.
    pub(crate) fn fame_daily(&mut self) {
        let Some(me) = self.watch else { return };
        let day = self.day;
        for i in 0..self.fame.asks.len() {
            let a = self.fame.asks[i].clone();
            if a.state == OfferState::Open && a.expires < day {
                self.fame.asks[i].state = OfferState::Lapsed;
                continue;
            }
            if a.state != OfferState::Accepted || a.held || a.event_day > day {
                continue;
            }
            self.fame.asks[i].held = true;
            let here =
                self.teams[me].roster.contains(&a.player) && self.players[a.player as usize].fit();
            let name = self.players[a.player as usize].name.clone();
            let company = self.econ.sponsors[a.sponsor].name.clone();
            if !here {
                self.post(
                    day,
                    Kind::Board,
                    &company,
                    format!("{name} did not make it"),
                    format!(
                        "{name} could not come to the {company} event and they are disappointed."
                    ),
                    vec![a.player],
                )
                .read = true;
                continue;
            }
            self.transact(me, a.fee, Cat::Sponsorship);
            let (presence, temper, prof) = {
                let p = &self.players[a.player as usize];
                (
                    p.personality.presence,
                    p.personality.temperament,
                    p.personality.professionalism,
                )
            };
            let pl = &mut self.players[a.player as usize];
            let fame = pl.renown.fame;
            pl.renown.fame = (fame + 0.6 * (1.0 - fame / 60.0).max(0.0)).min(100.0);
            pl.renown.busy = pl.renown.busy.saturating_add(1);
            pl.condition = (pl.condition - 0.02).max(0.7);
            self.fans_move(me, crate::fans::C_MEDIA, 0.0015);
            let mut rng =
                Rng::new(self.econ.seed ^ 0xA77E ^ ((self.season as u64) << 24) ^ i as u64);
            let risk = gaffe_chance(presence, temper, prof, MediaLevel::ALot, fame) * 3.0;
            let note = if rng.chance(risk) && !rng.chance(self.catch_rate(me)) {
                self.fame.counts.gaffes += 1;
                let docked = (a.fee / 4).max(0);
                self.transact(me, -docked, Cat::Sponsorship);
                self.players[a.player as usize].morale -= 0.03;
                format!(
                    "It did not go well: {name} said something he should not have and {company} \
                     took {} off the fee.",
                    money(docked)
                )
            } else {
                format!(
                    "{name} was a credit to the club. The fee of {} is in the budget.",
                    money(a.fee)
                )
            };
            self.post(
                day,
                Kind::Board,
                &company,
                format!("{name} at the {company} event"),
                note,
                vec![a.player],
            )
            .read = true;
        }
    }

    // ---- what is said to the press ---------------------------------------

    /// A famous player named in a press answer: a good word lifts his name,
    /// and a public criticism of a name costs more than one of a nobody.
    /// Returns a line for the outcome, if any.
    pub(crate) fn press_fame(
        &mut self,
        topic: Topic,
        choice: u8,
        about: Option<PlayerId>,
        question: usize,
    ) -> Option<String> {
        let p = about?;
        let me = self.watch?;
        let fame = self.fame_of(p);
        let name = self.players[p as usize].name.clone();
        match (topic, choice) {
            (Topic::AfterWin, 1) => {
                let r = &mut self.players[p as usize].renown;
                r.fame = (r.fame + 0.8 * (1.0 - r.fame / 70.0).max(0.0)).min(100.0);
                r.peak = r.peak.max(r.fame);
                None
            }
            (Topic::AfterLoss, 2) => {
                let heat = fame / 100.0;
                let r = &mut self.players[p as usize].renown;
                r.fame = (r.fame - 0.5).max(0.0);
                if fame < 20.0 {
                    return None;
                }
                // The bigger the name, the bigger the reaction.
                self.players[p as usize].morale =
                    (self.players[p as usize].morale - 0.08 * heat).clamp(0.0, 1.0);
                self.fans_move(me, crate::fans::C_MOVES, -0.006 * heat);
                let mut rng = Rng::new(
                    self.econ.seed ^ 0x9AE5 ^ ((self.season as u64) << 24) ^ question as u64,
                );
                let mut line = format!(" The supporters like {name} and read it.");
                if rng.chance(0.1 + 0.6 * heat) {
                    let b = &mut self.teams[me].belief;
                    *b = (*b - 0.025).clamp(0.0, 1.0);
                    for q in self.teams[me].roster.clone() {
                        if q != p {
                            let m = &mut self.players[q as usize].morale;
                            *m = (*m - 0.01).clamp(0.0, 1.0);
                        }
                    }
                    line +=
                        &format!(" {name} answered back in the papers and the room took his side.");
                }
                Some(line)
            }
            _ => None,
        }
    }
}
