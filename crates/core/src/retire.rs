//! Retirement and the hall of fame.
//!
//! Every summer each player who is still active may decide to stop
//! (`retire_chance`). The chance rises with age from 35, with how far his
//! rating has fallen from his best, with a serious injury, with not having
//! a role (not in the first team's top ten, or no club at all), and with
//! every summer without a club; it falls for a starter and for the driven
//! (ambition and determination hang on). Players under thirty only retire
//! when nobody has wanted them for two summers or when an injury ends them.
//!
//! A player who has decided, and is famous enough to matter
//! (`FAREWELL_FAME`) and still has a club, announces it instead: the coming
//! season is his farewell season (`Renown::farewell`). It lifts the gate and
//! the shop wherever he plays (`pull`, `player_merch`), there is a send-off
//! at his last home game, and when it is over he retires with no more dice.
//! Everyone else just retires, and the mail says so only if the user knew
//! him.
//!
//! A retired player stays in `World::players`, because ids are positions
//! and the career, the box scores in the history and the awards all point at
//! them; he leaves every club, list and market. `Player::is_retired` marks
//! him. Retirement is decided from the world and `Economy::seed`, never from
//! `World::rng`.

use serde::{Deserialize, Serialize};

use crate::fame::{FAREWELL_FAME, Hunt, Renown};
use crate::inbox::{Kind, MAX_ROSTER, MIN_ROSTER, OfferState};
use crate::league::{TeamId, World};
use crate::player::{Attr, PlayerId};
use crate::rng::Rng;

/// Career score at which a retired player enters the hall of fame.
pub const HALL_MIN: f32 = 45.0;
/// A club under this many first-team players signs someone.
const FLOOR: usize = 10;

/// A member of the hall of fame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HallEntry {
    pub player: PlayerId,
    /// The season (counting from 0) he retired after.
    pub season: u16,
    pub score: f32,
}

/// How great a career was: his peak fame, awards, titles and seasons at the
/// top.
pub fn career_score(r: &Renown) -> f32 {
    r.peak * 0.5
        + r.mvps as f32 * 14.0
        + r.titles as f32 * 6.0
        + r.finals as f32 * 2.0
        + r.scoring as f32 * 5.0
        + r.elite as f32 * 1.5
}

impl World {
    /// The hall of fame, best career first.
    pub fn hall_of_fame(&self) -> Vec<&HallEntry> {
        let mut h: Vec<&HallEntry> = self.fame.hall.iter().collect();
        h.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.player.cmp(&b.player)));
        h
    }

    /// Players not retired.
    pub fn active_players(&self) -> usize {
        self.players.iter().filter(|p| !p.is_retired()).count()
    }

    /// The chance that player `p` stops after this season, 0..=1, from the
    /// world as it is at the end of the season.
    pub fn retire_chance(&self, p: PlayerId) -> f32 {
        let pl = &self.players[p as usize];
        if pl.is_retired() {
            return 0.0;
        }
        let age = pl.age as i32;
        let team = self.team_of(p);
        let on_roster = team.is_some_and(|t| self.teams[t].roster.contains(&p));
        let weeks_out = pl.injury.as_ref().map_or(0, |i| i.weeks_left);
        // An injury that ends a career at any age.
        let mut score = if weeks_out >= 60 { 0.5 } else { 0.0 };
        // Nobody has signed him: the young get two summers before it counts.
        let waited = pl.renown.idle as i32 - if age < 27 { 1 } else { 0 };
        if team.is_none() && waited >= 1 {
            score += 0.6 + 0.35 * (waited as f32 - 1.0);
            if age >= 29 {
                score += 0.15;
            }
        }
        if age >= 30 {
            score += ((age - 34) as f32 * 0.11).max(0.0);
            let best = self
                .career(p)
                .iter()
                .map(|l| l.overall)
                .fold(pl.overall(), f32::max);
            let fall = best - pl.overall();
            if fall > 1.0 {
                score += ((fall - 1.0) * 0.12).min(0.35);
            }
            if weeks_out >= 30 {
                score += 0.25;
            }
            if pl.weeks_lost() >= 60 {
                score += 0.12;
            }
            if let Some(t) = team {
                let rank = self.teams[t]
                    .roster
                    .iter()
                    .filter(|&&q| self.players[q as usize].overall() > pl.overall())
                    .count();
                if !on_roster || rank >= 9 {
                    score += 0.08;
                }
                if on_roster && rank < 8 {
                    score -= 0.12;
                }
            }
            let drive =
                (pl.personality.determination as f32 + pl.personality.ambition as f32) / 40.0;
            score -= (drive - 0.4).max(0.0) * 0.5;
        }
        if age >= 44 {
            return 1.0;
        }
        score.clamp(0.0, 0.95)
    }

    /// Summer: players retire, or announce that they will. Called with the
    /// season just finished still in `self.season`, before anyone is a year
    /// older.
    pub(crate) fn summer_retirements(&mut self) {
        let mut rng = Rng::new(self.econ.seed ^ 0xBE71_4E00 ^ ((self.season as u64) << 20));
        let me = self.watch;
        let mut touched: Vec<TeamId> = Vec::new();
        let mut known_names: Vec<(PlayerId, String)> = Vec::new();
        for p in 0..self.players.len() as PlayerId {
            if self.players[p as usize].is_retired() {
                continue;
            }
            let team = self.team_of(p);
            // Who has been without a club this long.
            if team.is_none() {
                self.players[p as usize].renown.idle =
                    self.players[p as usize].renown.idle.saturating_add(1);
            } else {
                self.players[p as usize].renown.idle = 0;
            }
            let farewell = self.players[p as usize].renown.farewell;
            let decided = farewell || {
                let chance = self.retire_chance(p);
                chance > 0.0 && rng.chance(chance)
            };
            if !decided {
                continue;
            }
            let fame = self.players[p as usize].renown.fame;
            if !farewell && fame >= FAREWELL_FAME && team.is_some() {
                self.announce_farewell(p, team.unwrap_or(0));
                continue;
            }
            if let Some(t) = team {
                touched.push(t);
                if Some(t) == me || fame >= 25.0 {
                    known_names.push((p, self.players[p as usize].name.clone()));
                }
            }
            self.retire(p);
        }
        touched.sort_unstable();
        touched.dedup();
        for t in touched {
            self.refit(t);
        }
        self.age_the_unattached();
        // The user hears about the names who stopped.
        if me.is_some() && !known_names.is_empty() {
            let lines: Vec<String> = known_names
                .iter()
                .map(|(p, n)| {
                    let pl = &self.players[*p as usize];
                    let hall = self.fame.hall.iter().any(|h| h.player == *p);
                    format!(
                        "{n} ({}), {} seasons on the books{}.",
                        pl.age,
                        self.career(*p).len(),
                        if hall {
                            ", elected to the hall of fame"
                        } else {
                            ""
                        }
                    )
                })
                .collect();
            let body = format!(
                "These players have announced their retirement:\n\n{}\n\nThey are off the \
                 books of their clubs, and their careers stay in the record.",
                lines.join("\n")
            );
            let players = known_names.iter().map(|k| k.0).collect();
            let day = self.day;
            self.post(
                day,
                Kind::League,
                "League office",
                "Retirements",
                body,
                players,
            );
        }
    }

    /// Everyone without a club is a year older, with what that does to the
    /// body.
    fn age_the_unattached(&mut self) {
        let mut taken = vec![false; self.players.len()];
        for t in &self.teams {
            for &p in t.roster.iter().chain(&t.reserves) {
                taken[p as usize] = true;
            }
        }
        for (i, pl) in self.players.iter_mut().enumerate() {
            if taken[i] || pl.is_retired() {
                continue;
            }
            pl.age = pl.age.saturating_add(1);
            let decline = match pl.age {
                ..=30 => 0.0,
                31..=33 => 0.25,
                _ => 0.5,
            };
            for a in Attr::ALL {
                let fall = if a.is_physical() {
                    decline * 1.6
                } else {
                    decline * 0.4
                };
                pl.attrs[a] = (pl.attrs[a] - fall).clamp(1.0, 20.0);
            }
        }
    }

    fn announce_farewell(&mut self, p: PlayerId, club: TeamId) {
        self.players[p as usize].renown.farewell = true;
        self.fame.counts.farewells += 1;
        let name = self.players[p as usize].name.clone();
        let team = self.teams[club].name.clone();
        let fame = self.players[p as usize].renown.fame;
        if self.watch.is_none() {
            return;
        }
        let ours = self.watch == Some(club);
        let body = if ours {
            format!(
                "{name} has told me that the coming season will be his last. He wants to go \
                 out playing, and people will come to see him do it. Expect fuller halls at \
                 home and away, and a send-off at his last home game. If you bench him, the \
                 supporters will want to know why.",
            )
        } else {
            format!(
                "{name} of {team}, {}, has announced that the coming season will be his last. \
                 Halls around the league expect a farewell tour.",
                crate::fame::fame_words(fame).to_lowercase()
            )
        };
        let day = self.day;
        self.post(
            day,
            if ours { Kind::Board } else { Kind::League },
            if ours { "The club" } else { "League office" },
            format!("{name} will retire after the season"),
            body,
            vec![p],
        );
    }

    /// He has stopped: off every list, still in the record.
    pub(crate) fn retire(&mut self, p: PlayerId) {
        let season = self.season;
        let r = &mut self.players[p as usize].renown;
        r.retired = Some(season);
        r.farewell = false;
        r.media = Default::default();
        let score = career_score(r);
        self.fame.counts.retired += 1;
        for t in &mut self.teams {
            t.roster.retain(|&q| q != p);
            t.reserves.retain(|&q| q != p);
            t.down.retain(|&q| q != p);
        }
        if self.scouting.is_shortlisted(p) {
            self.scouting.toggle(p);
        }
        self.desk.bids.retain(|b| b.player != p);
        for o in &mut self.desk.offers {
            if o.player == p && o.state == OfferState::Open {
                o.state = OfferState::Lapsed;
            }
        }
        for a in &mut self.fame.asks {
            if a.player == p && a.state == OfferState::Open {
                a.state = OfferState::Lapsed;
            }
        }
        if score >= HALL_MIN {
            self.fame.hall.push(HallEntry {
                player: p,
                season,
                score,
            });
        }
    }

    /// After the summer's contracts: clubs short of players sign some, and
    /// clubs with an empty hall and money go for a name.
    pub(crate) fn after_retirements(&mut self) {
        for club in 0..self.teams.len() {
            self.fill_squad(club);
        }
        self.name_hunt();
    }

    /// Keep the first team at ten: promote a reserve, or sign the best free
    /// agent.
    fn fill_squad(&mut self, club: TeamId) {
        let mut changed = false;
        while self.teams[club].roster.len() < FLOOR.max(MIN_ROSTER) {
            let best_reserve = self.teams[club].reserves.iter().copied().max_by(|&a, &b| {
                self.players[a as usize]
                    .overall()
                    .total_cmp(&self.players[b as usize].overall())
            });
            let signed = match best_reserve {
                Some(r) => {
                    self.teams[club].reserves.retain(|&q| q != r);
                    r
                }
                None => {
                    let taken: Vec<PlayerId> = self.free_agents().iter().map(|p| p.id).collect();
                    let Some(&f) = taken.iter().max_by(|&&a, &&b| {
                        self.players[a as usize]
                            .overall()
                            .total_cmp(&self.players[b as usize].overall())
                    }) else {
                        break;
                    };
                    self.players[f as usize].contract.years_left = 2;
                    f
                }
            };
            self.teams[club].roster.push(signed);
            changed = true;
            if self.watch == Some(club) {
                let name = self.players[signed as usize].name.clone();
                let day = self.day;
                self.post(
                    day,
                    Kind::Board,
                    "The club",
                    format!("{name} added to the squad"),
                    format!(
                        "Retirements left us short of players, so {name} has been added to the \
                         first team. He is not in the rotation unless a place was free: check \
                         the Tactics screen."
                    ),
                    vec![signed],
                );
            }
        }
        if changed {
            self.refit(club);
        }
    }

    /// A club with an empty hall and money, and nobody who draws a crowd,
    /// signs the biggest name it can use: a free agent, or a veteran from
    /// another club at his price. Only the clubs the user does not run.
    fn name_hunt(&mut self) {
        let mut rng = Rng::new(self.econ.seed ^ 0x4A41_7000 ^ ((self.season as u64) << 20));
        let mut order: Vec<TeamId> = (0..self.teams.len())
            .filter(|&c| Some(c) != self.watch)
            .collect();
        order.sort_by(|&a, &b| {
            self.commercial(b)
                .total_cmp(&self.commercial(a))
                .then(a.cmp(&b))
        });
        for club in order {
            if self.commercial(club) < 0.6 || self.teams[club].budget < 350_000 {
                continue;
            }
            if !rng.chance(0.4) {
                continue;
            }
            let own_best = self.teams[club]
                .roster
                .iter()
                .map(|&p| self.fame_of(p))
                .fold(0.0, f32::max);
            let mut ov: Vec<f32> = self.teams[club]
                .roster
                .iter()
                .map(|&p| self.players[p as usize].overall())
                .collect();
            ov.sort_by(|a, b| b.total_cmp(a));
            let rotation_bar = ov.get(7).copied().unwrap_or(0.0) - 0.3;
            let mut targets: Vec<PlayerId> = (0..self.players.len() as PlayerId)
                .filter(|&p| {
                    let pl = &self.players[p as usize];
                    !pl.is_retired()
                        && pl.age <= 41
                        && !self.teams[club].roster.contains(&p)
                        && !self.teams[club].reserves.contains(&p)
                        && pl.renown.fame >= 35.0f32.max(own_best + 8.0)
                        && pl.overall() >= rotation_bar
                })
                .collect();
            targets.sort_by(|&a, &b| self.fame_of(b).total_cmp(&self.fame_of(a)).then(a.cmp(&b)));
            for p in targets {
                let owner = self.team_of(p);
                let salary = self.players[p as usize].contract.salary as i64;
                let price = match owner {
                    None => 0,
                    Some(o) => {
                        if o == club || self.teams[o].roster.len() <= MIN_ROSTER + 1 {
                            continue;
                        }
                        let value = self.value_to(o, p);
                        let key = self.is_key(p, o);
                        let gm = &self.teams[o].office.gm;
                        (value * if key { 14 } else { 10 } / 10 + value * gm.margin() / 100) / 1000
                            * 1000
                    }
                };
                // Not ruinous: it must leave the club in the black.
                if self.teams[club].budget - price - salary / 2 < 250_000
                    || self.payroll(club) + salary > crate::season::SALARY_CAP
                    || self.fee_check(club, price).is_err()
                    || self.wage_check(club, salary).is_err()
                {
                    continue;
                }
                if self.teams[club].roster.len() >= MAX_ROSTER {
                    let worst = self.teams[club]
                        .roster
                        .iter()
                        .copied()
                        .min_by_key(|&q| self.value_to(club, q));
                    if let Some(w) = worst {
                        self.teams[club].roster.retain(|&q| q != w);
                    }
                }
                match owner {
                    Some(_) => self.transfer(p, club, price),
                    None => {
                        self.teams[club].roster.push(p);
                        self.players[p as usize].contract.years_left = 2;
                        self.fans_signed(p, club);
                        self.refit(club);
                    }
                }
                self.fame.hunts.push(Hunt {
                    season: self.season,
                    club,
                    player: p,
                    fee: price,
                    from: owner,
                });
                break;
            }
        }
    }

    /// The farewell night of the home game `i`: the player who is saying
    /// goodbye, if this is his club's last regular-season home game.
    pub fn farewell_night(&self, i: usize) -> Option<PlayerId> {
        let f = &self.fixtures[i];
        if f.playoff {
            return None;
        }
        let p = self.teams[f.home]
            .roster
            .iter()
            .copied()
            .find(|&p| self.players[p as usize].renown.farewell)?;
        let later = self
            .fixtures
            .iter()
            .skip(i + 1)
            .any(|g| g.home == f.home && !g.playoff && g.result.is_none());
        (!later).then_some(p)
    }

    /// After the user's home game: the send-off.
    pub(crate) fn send_off(&mut self, played: &[usize]) {
        let Some(me) = self.watch else { return };
        for &i in played {
            if self.fixtures[i].home != me {
                continue;
            }
            let Some(p) = self.farewell_night(i) else {
                continue;
            };
            let name = self.players[p as usize].name.clone();
            let crowd = self.attendance_at(i);
            self.fans_move(me, crate::fans::C_MOVES, 0.008);
            let pl = &mut self.players[p as usize];
            pl.morale = (pl.morale + 0.1).clamp(0.0, 1.0);
            let body = format!(
                "{} people came to say goodbye to {name} at his last home game, and they gave \
                 him a standing ovation that went on long after the buzzer. The club presented \
                 him with his shirt in a frame. There was not a dry eye in the hall.",
                crate::inbox::money(crowd as i64)
            );
            let day = self.day;
            self.post(
                day,
                Kind::Match,
                "The club",
                format!("A send-off for {name}"),
                body,
                vec![p],
            );
        }
    }

    /// Every player's age is a year more on the seasons that have been
    /// played; used to check that retirement keeps the world in bounds.
    pub fn count_by_status(&self) -> (usize, usize, usize, usize) {
        let mut on_club = 0;
        for t in &self.teams {
            on_club += t.roster.len() + t.reserves.len();
        }
        let free = self.free_agents().len();
        let retired = self.players.iter().filter(|p| p.is_retired()).count();
        (on_club, free, retired, self.players.len())
    }
}
