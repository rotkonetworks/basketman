//! Fans: how many people follow a club, and what moves them.
//!
//! A club has two numbers about its public and they are different things.
//! `FameBook::fans` is the fan base: how many supporters there are, in
//! people. `Arena::support` (0..=1) is how much the town cares: the share of
//! those who come, the mood of the place. The hall's `fill` uses both: the
//! intensity (`support`) as it always did, and the size of the fan base
//! compared with the hall (`fan_index`: 0.5 is a typical club, so a normal
//! club's gate does not change). Support is what prices, bargains and
//! results shape over a year; the fan base is what stars, titles, media
//! work and years of winning build, and what sponsors and the shop read.
//!
//! The fan base grows with winning, with playoff runs, with names on the
//! roster, with exciting games, low prices and media work, and shrinks with
//! losing, dear tickets, a favourite benched or sold. A growing club grows
//! slower as it nears the ceiling of what its hall and town can hold, and
//! every summer the base drifts a little toward what the club's standing
//! says it should be, so a long run of success does not run away.
//!
//! Nothing here draws from `World::rng`.

use crate::inbox::Kind;
use crate::league::{TeamId, World};
use crate::player::PlayerId;

/// What moves the fan base, in the order `FameBook::moved` keeps them.
pub const CAUSES: [&str; 7] = [
    "Results",
    "Finish and playoffs",
    "Stars on the team",
    "Media work",
    "Ticket prices",
    "Style of play",
    "Signings and sales",
];
pub const C_RESULTS: usize = 0;
pub const C_FINISH: usize = 1;
pub const C_STARS: usize = 2;
pub const C_MEDIA: usize = 3;
pub const C_PRICE: usize = 4;
pub const C_STYLE: usize = 5;
pub const C_MOVES: usize = 6;

/// Supporters per seat of the hall at an index of 1.
const PER_SEAT: f32 = 6.0;

impl World {
    /// Supporters, in people.
    pub fn fan_base(&self, club: TeamId) -> u32 {
        self.fame.fans.get(club).copied().unwrap_or(0.0) as u32
    }

    /// 0..=1: the fan base next to the hall. 0.5 is an ordinary club.
    pub fn fan_index(&self, club: TeamId) -> f32 {
        let cap = self.econ.arenas[club].capacity.max(1) as f32;
        let base = self.fame.fans.get(club).copied().unwrap_or(cap * 3.0);
        (base / (PER_SEAT * cap)).clamp(0.0, 1.0)
    }

    /// This season's change in the fan base so far, as a share of what it
    /// was when the season began.
    pub fn fan_trend(&self, club: TeamId) -> f32 {
        let moved: f32 = self.fame.moved.get(club).map_or(0.0, |m| m.iter().sum());
        let now = self.fame.fans.get(club).copied().unwrap_or(0.0);
        let start = (now - moved).max(1.0);
        moved / start
    }

    /// What the fan base did this season, by cause, in people.
    pub fn fans_moved(&self, club: TeamId) -> [f32; CAUSES.len()] {
        self.fame.moved.get(club).copied().unwrap_or_default()
    }

    pub(crate) fn fans_seed(&mut self) {
        let n = self.teams.len();
        self.fame.fans = (0..n)
            .map(|c| {
                let a = &self.econ.arenas[c];
                a.capacity as f32 * 3.0 * (0.75 + 0.5 * a.support) * (0.9 + 0.2 * a.prestige)
            })
            .collect();
        self.fame.moved = vec![Default::default(); n];
        self.fame.last_moved = vec![Default::default(); n];
        self.fame.past.clear();
    }

    /// Move the fan base by `pct` (0.01 is one percent) for `cause`.
    /// Gains shrink as the base nears what the hall can use; losses are
    /// bigger the more there is to lose.
    pub(crate) fn fans_move(&mut self, club: TeamId, cause: usize, pct: f32) {
        let Some(&base) = self.fame.fans.get(club) else {
            return;
        };
        let idx = self.fan_index(club);
        let scale = if pct > 0.0 {
            (1.15 - idx).clamp(0.2, 1.0)
        } else {
            (0.45 + idx).clamp(0.45, 1.3)
        };
        let new = (base * (1.0 + pct * scale)).max(500.0);
        self.fame.fans[club] = new;
        if let Some(m) = self.fame.moved.get_mut(club) {
            m[cause] += new - base;
        }
    }

    fn nudge_support(&mut self, club: TeamId, d: f32) {
        let a = &mut self.econ.arenas[club];
        a.support = (a.support + d).clamp(0.1, 0.95);
    }

    /// After a round: what the games, the names, the prices and the media
    /// work did to every club's fan base.
    pub(crate) fn fans_week(&mut self, played: &[usize]) {
        if self.fame.fans.len() != self.teams.len() {
            return;
        }
        // The league's usual points in a game, to tell a shootout from a
        // slog.
        let (mut total, mut games) = (0.0f32, 0.0f32);
        for f in self.fixtures.iter().filter(|f| !f.playoff) {
            if let Some(r) = &f.result {
                total += (r.score[0] + r.score[1]) as f32;
                games += 1.0;
            }
        }
        let usual = if games > 0.0 { total / games } else { 160.0 };
        for &i in played {
            let f = self.fixtures[i].clone();
            let Some(r) = &f.result else { continue };
            for (side, club) in [f.home, f.away].into_iter().enumerate() {
                let won = r.score[side] > r.score[1 - side];
                let pct = match (f.playoff, won) {
                    (false, true) => 0.003,
                    (false, false) => -0.003,
                    (true, true) => 0.008,
                    (true, false) => 0.0,
                };
                self.fans_move(club, C_RESULTS, pct);
                let pts = (r.score[0] + r.score[1]) as f32;
                if !f.playoff {
                    if pts > usual * 1.08 {
                        self.fans_move(club, C_STYLE, 0.0012);
                    } else if pts < usual * 0.92 {
                        self.fans_move(club, C_STYLE, -0.0008);
                    }
                }
            }
            if !f.playoff {
                let d = self.draw_for(i);
                let fair = self.fair_price(f.home) as f32;
                let b = crate::pricing::bargain(&d, fair, self.ticket_price(f.home));
                self.fans_move(f.home, C_PRICE, 0.0015 * b);
            }
        }
        let regular = played.iter().any(|&i| !self.fixtures[i].playoff);
        if regular {
            for club in 0..self.teams.len() {
                let star = self.star_pull(club, club);
                self.fans_move(club, C_STARS, 0.0018 * star);
                let work = self.media_pull(club);
                self.fans_move(club, C_MEDIA, work);
                // Interest drifts away if nothing keeps it.
                self.fans_move(club, C_RESULTS, -0.0003);
            }
            self.fans_bench();
        }
    }

    /// A favourite who is not playing: the supporters notice after three
    /// weeks and say so after that.
    fn fans_bench(&mut self) {
        let me = self.watch;
        for club in 0..self.teams.len() {
            for p in self.teams[club].roster.clone() {
                let pl = &self.players[p as usize];
                if !self.is_favourite(p, club) || !pl.fit() {
                    self.players[p as usize].renown.bench_weeks = 0;
                    continue;
                }
                let sat = self.plays_factor(club, p) < 0.5;
                let r = &mut self.players[p as usize].renown;
                if !sat {
                    r.bench_weeks = 0;
                    continue;
                }
                r.bench_weeks = r.bench_weeks.saturating_add(1);
                if r.bench_weeks < 3 {
                    continue;
                }
                let told = r.bench_told;
                r.bench_told = true;
                let heat = self.pull(p, club);
                self.fans_move(club, C_MOVES, -0.004 * (0.5 + heat));
                self.nudge_support(club, -0.004 * (0.5 + heat));
                if Some(club) == me && !told {
                    let name = self.players[p as usize].name.clone();
                    let body = format!(
                        "We are the supporters' club and we write as one voice. {name} has not \
                         been playing and the stands are asking why. People bought tickets to \
                         see him. We are not telling you how to pick a team, but the gate is \
                         noticing, and so are we.",
                    );
                    let day = self.day;
                    self.post(
                        day,
                        Kind::Media,
                        "The supporters' club",
                        format!("Where is {name}?"),
                        body,
                        vec![p],
                    );
                }
            }
        }
    }

    /// The season is decided: the table, the playoffs and the pull of a
    /// club's standing change its fan base for good, and the summer files
    /// the year. Called after the season's money has been settled.
    pub(crate) fn fans_close_season(&mut self, table: &[TeamId], champion: TeamId) {
        if self.fame.fans.len() != self.teams.len() {
            return;
        }
        let n = table.len().max(2);
        for (i, &t) in table.iter().enumerate() {
            let place = 1.0 - i as f32 / (n - 1) as f32;
            let mut pct = (place - 0.5) * 0.05;
            let semi = self
                .playoffs
                .iter()
                .any(|s| !s.is_final && (s.high == t || s.low == t));
            let finalist = self
                .playoffs
                .iter()
                .any(|s| s.is_final && (s.high == t || s.low == t));
            if semi {
                pct += 0.015;
            }
            if finalist {
                pct += 0.025;
            }
            if t == champion && !self.playoffs.is_empty() {
                pct += 0.05;
            }
            self.fans_move(t, C_FINISH, pct);
            // Drift toward what the club's standing and hall say it should
            // have.
            let a = &self.econ.arenas[t];
            let target = a.capacity as f32 * PER_SEAT * (0.30 + 0.5 * a.prestige);
            let base = self.fame.fans[t];
            let step = (target - base) * 0.15;
            self.fame.fans[t] = (base + step).max(500.0);
            self.fame.moved[t][C_FINISH] += step;
        }
        let snapshot = self.fame.fans.clone();
        self.fame.past.push(snapshot);
        self.fame.last_moved = std::mem::replace(
            &mut self.fame.moved,
            vec![Default::default(); self.teams.len()],
        );
    }

    /// A player the supporters know is leaving `club`.
    pub(crate) fn fans_sold(&mut self, p: PlayerId, from: TeamId) {
        if self.fame.fans.len() != self.teams.len() || !self.is_favourite(p, from) {
            return;
        }
        let heat = self.pull(p, from);
        self.fans_move(from, C_MOVES, -0.07 * heat);
        self.nudge_support(from, -0.03 * heat);
        if self.watch == Some(from) {
            let name = self.players[p as usize].name.clone();
            let years = self.players[p as usize].renown.tenure;
            let body = format!(
                "The supporters' club has heard that {name} is going. {}He was one of ours. \
                 Whatever the reasons were, a lot of people bought a ticket or a shirt because \
                 of him, and they will not come back for the next man's name alone. We ask you \
                 to remember who we play for.",
                if years >= 3 {
                    format!("{years} years here. ")
                } else {
                    String::new()
                }
            );
            let day = self.day;
            self.post(
                day,
                Kind::Media,
                "The supporters' club",
                format!("{name} leaves: a letter from the stands"),
                body,
                vec![p],
            );
        }
    }

    /// A player the public knows has joined `club`.
    pub(crate) fn fans_signed(&mut self, p: PlayerId, to: TeamId) {
        if self.fame.fans.len() != self.teams.len() {
            return;
        }
        let heat = self.fame_of(p) / 100.0;
        if heat < 0.15 {
            return;
        }
        self.fans_move(to, C_MOVES, 0.05 * heat);
        self.nudge_support(to, 0.012 * heat);
        if self.watch == Some(to) && heat >= 0.3 {
            let name = self.players[p as usize].name.clone();
            let body = format!(
                "The phones have not stopped since the news about {name}. Season tickets are \
                 moving and the shop wants to know how many shirts to print. A name like his \
                 fills the hall, as long as he is on the floor.",
            );
            let day = self.day;
            self.post(
                day,
                Kind::Media,
                "The supporters' club",
                format!("Fans are talking about {name}"),
                body,
                vec![p],
            );
        }
    }
}
