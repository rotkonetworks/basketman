//! Fame: a player's pull on the public, separate from how good he is now.
//!
//! Fame (`Renown::fame`, 0..=100) is earned over a career and fades slowly:
//! every summer it keeps `DECAY` of what it was and adds what the season
//! earned. A season earns fame for high-level play (how far his overall is
//! over what a good starter has, weighted by how much he played), for
//! awards (most valuable player, scoring title), for titles and finals, for
//! big playoff games, for years at one club (a small, steady term that only
//! counts for men who play), and for being young and exciting. Nothing in
//! it follows his rating after the fact: a great whose body has gone keeps
//! most of his name for years, and a good player nobody writes about never
//! builds one. Fame is public.
//!
//! What fame does: it fills halls (`pull`, `star_pull`, the `Draw` terms in
//! `arena.rs`), sells shirts (`income.rs::player_merch`), raises sponsors'
//! fees (`sponsor.rs`), makes a general manager pay more (`staff.rs`) and the
//! player ask for more at renewal (`wage_mult`), and decides who is a
//! favourite of the supporters (`fans.rs`).
//!
//! Nothing here draws from `World::rng`.

use serde::{Deserialize, Serialize};

use crate::league::{TeamId, World};
use crate::media::MediaLevel;
use crate::player::{Attr, Player, PlayerId};
use crate::rng::Rng;

/// What a summer leaves of last year's fame.
pub const DECAY: f32 = 0.90;
/// Fame earned is damped as it approaches this, so nobody reaches 100 by
/// piling up seasons.
const SATURATE: f32 = 120.0;
/// Fame needed to be a name worth a farewell season.
pub const FAREWELL_FAME: f32 = 38.0;
/// Fame at which a player is somebody the supporters care about.
pub const FAVOURITE_FAME: f32 = 30.0;

/// Everything about a player's public life that changes: his fame, the
/// media work he is given, and where he stands in his career's last years.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Renown {
    /// 0..=100.
    pub fame: f32,
    /// The most it ever was.
    pub peak: f32,
    /// Seasons at `tenure_club`, ending with the last one played.
    pub tenure: u8,
    pub tenure_club: Option<TeamId>,
    /// The season (counting from 0) after which he retired.
    pub retired: Option<u16>,
    /// He has said the coming season is his last.
    pub farewell: bool,
    /// The media work the manager gives him each week.
    pub media: MediaLevel,
    /// Progress toward the next point of presence.
    pub xp: f32,
    /// Summers without a club.
    pub idle: u8,
    /// Seasons of high-level play.
    pub elite: u8,
    pub mvps: u8,
    pub titles: u8,
    pub finals: u8,
    pub scoring: u8,
    /// Shirt money he has sold this season.
    pub shirts: i64,
    /// Weeks in a row a favourite has not played.
    pub bench_weeks: u8,
    /// The supporters have written about his place this season.
    pub bench_told: bool,
    /// Events he has done since the last training week.
    pub busy: u8,
}

/// The fame of the whole league that is not a player's own: fans, the hall
/// of fame, appearance requests and running counts.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FameBook {
    /// Fame and fan bases have been set up (they are for a new world, and
    /// for an old save when it is opened).
    pub seeded: bool,
    /// Each club's supporters, in people. Indexed by `TeamId`.
    pub fans: Vec<f32>,
    /// This season's change in each club's supporters, by cause (see
    /// `fans::CAUSES`), in people.
    pub moved: Vec<[f32; crate::fans::CAUSES.len()]>,
    /// Last season's, kept when it closed.
    pub last_moved: Vec<[f32; crate::fans::CAUSES.len()]>,
    /// The supporters at the end of each finished season.
    pub past: Vec<Vec<f32>>,
    /// The league's greats, in the order they were chosen.
    pub hall: Vec<crate::retire::HallEntry>,
    /// Sponsors' requests for one of the user's players.
    pub asks: Vec<crate::media::Appearance>,
    pub counts: Counts,
    /// Names clubs went out and got.
    pub hunts: Vec<Hunt>,
}

/// Running totals, for the measurements and the season's review.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Counts {
    pub gaffes: u32,
    pub caught: u32,
    pub stories: u32,
    pub retired: u32,
    pub farewells: u32,
}

/// A club that signed a name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hunt {
    pub season: u16,
    pub club: TeamId,
    pub player: PlayerId,
    pub fee: i64,
    pub from: Option<TeamId>,
}

/// Fame in words.
pub fn fame_words(f: f32) -> &'static str {
    match f {
        f if f >= 80.0 => "A legend",
        f if f >= 60.0 => "A superstar",
        f if f >= 40.0 => "A star",
        f if f >= 20.0 => "Known across the league",
        f if f >= 8.0 => "A local name",
        _ => "Unknown",
    }
}

/// How much of a name there is at `points` of fame (or of pull, times 100),
/// 0..=1.2: nothing below 15, rising fast above it, so that a roster of
/// men a little known adds up to nothing and one real name to a lot.
pub fn heat(points: f32) -> f32 {
    let x = ((points - 15.0) / 70.0).clamp(0.0, 1.1);
    x * x
}

/// What a player asks for in pay over what his play is worth: a name
/// costs more.
pub fn wage_mult(p: &Player) -> f32 {
    1.0 + 0.55 * heat(p.renown.fame)
}

impl Player {
    pub fn fame(&self) -> f32 {
        self.renown.fame
    }

    pub fn is_retired(&self) -> bool {
        self.renown.retired.is_some()
    }
}

impl World {
    pub fn fame_of(&self, p: PlayerId) -> f32 {
        self.players.get(p as usize).map_or(0.0, |p| p.renown.fame)
    }

    /// Target minutes `p` has in `club`'s rotation, if he is in it.
    fn rotation_minutes(&self, club: TeamId, p: PlayerId) -> Option<u8> {
        let t = &self.teams[club];
        t.playbook
            .get(t.active)?
            .tactics
            .rotation
            .iter()
            .find(|s| s.player == p)
            .map(|s| s.minutes)
    }

    /// How much of him the crowd gets, 0..=1: none while he is hurt or a
    /// reserve, a little if he is on the bench, all of it if he plays.
    pub fn plays_factor(&self, club: TeamId, p: PlayerId) -> f32 {
        if !self.teams[club].roster.contains(&p) || !self.players[p as usize].fit() {
            return 0.0;
        }
        match self.rotation_minutes(club, p) {
            None => 0.1,
            Some(m) => 0.1 + 0.9 * (m.min(24) as f32 / 24.0),
        }
    }

    /// His pull at `venue`, 0..=1: fame, bigger at the club where he has
    /// spent his years and bigger still in the season he says goodbye.
    pub fn pull(&self, p: PlayerId, venue: TeamId) -> f32 {
        let r = &self.players[p as usize].renown;
        let mut f = r.fame;
        if r.tenure_club == Some(venue) {
            f += f * 0.04 * r.tenure.min(10) as f32;
        }
        if r.farewell {
            f *= 1.35;
        }
        (f / 100.0).clamp(0.0, 1.0)
    }

    /// What the best names of `club` are worth at the gate of `venue`,
    /// 0..=1: the pull of those who play, combined so that two stars are
    /// better than one but never twice as good. A roster of men a little
    /// known is not a star (`heat`).
    pub fn star_pull(&self, club: TeamId, venue: TeamId) -> f32 {
        let mut left = 1.0f32;
        for &p in &self.teams[club].roster {
            let v = heat(self.pull(p, venue) * 100.0) * self.plays_factor(club, p);
            left *= 1.0 - v.min(0.9);
        }
        1.0 - left
    }

    /// The most famous player at `club`: who carries its name.
    pub fn top_name(&self, club: TeamId) -> Option<PlayerId> {
        self.teams[club]
            .roster
            .iter()
            .copied()
            .max_by(|&a, &b| self.fame_of(a).total_cmp(&self.fame_of(b)))
    }

    /// True if the supporters would mind losing him: a name, or years of
    /// service.
    pub fn is_favourite(&self, p: PlayerId, club: TeamId) -> bool {
        let r = &self.players[p as usize].renown;
        r.tenure_club == Some(club)
            && r.fame + 3.0 * r.tenure.min(8) as f32 >= FAVOURITE_FAME + 10.0
            || r.fame >= FAVOURITE_FAME + 15.0
    }

    /// The league's most famous players still playing, most famous first.
    pub fn biggest_names(&self, n: usize) -> Vec<(PlayerId, f32)> {
        let mut all: Vec<(PlayerId, f32)> = self
            .teams
            .iter()
            .flat_map(|t| t.roster.iter().copied())
            .map(|p| (p, self.fame_of(p)))
            .collect();
        all.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        all.truncate(n);
        all
    }

    /// 0..=1: how much `club` wants a name, from how empty its hall is and
    /// whether it has the money. A full hall in a rich club does not need
    /// one; an empty hall does.
    pub fn commercial(&self, club: TeamId) -> f32 {
        let a = &self.econ.arenas[club];
        let fill = match self.econ.history.last() {
            Some(h) => h[club].average_attendance() as f32 / a.capacity.max(1) as f32,
            None => self.expected_attendance(club) as f32 / a.capacity.max(1) as f32,
        };
        let hunger = ((0.62 - fill) / 0.25).clamp(0.0, 1.0);
        let funds = (self.teams[club].budget as f32 / 400_000.0).clamp(0.0, 1.0);
        (0.2 + 0.6 * hunger + 0.2 * funds).clamp(0.0, 1.0)
    }

    /// What a name adds to what a player is worth to `club`, as a share of
    /// his value on the court.
    pub fn commercial_value(&self, club: TeamId, p: PlayerId) -> f32 {
        let f = self.fame_of(p) / 100.0;
        self.commercial(club) * (0.4 * f + 2.0 * f * f)
    }

    /// Set up fame and fan bases for a world that has none: a new one, or a
    /// save from before they existed.
    pub fn ensure_fame(&mut self) {
        if !self.fame.seeded {
            self.seed_fame();
        }
        let n = self.teams.len();
        if self.fame.fans.len() != n {
            self.fans_seed();
        }
        for p in &self.players {
            debug_assert!(p.renown.fame >= 0.0);
        }
    }

    /// The league's reference for a good starter: the overall that a
    /// quarter of the first-team players are above.
    fn starter_bar(&self) -> f32 {
        let mut ov: Vec<f32> = self
            .teams
            .iter()
            .flat_map(|t| t.roster.iter())
            .map(|&p| self.players[p as usize].overall())
            .collect();
        if ov.is_empty() {
            return 10.0;
        }
        ov.sort_by(|a, b| a.total_cmp(b));
        ov[ov.len() * 3 / 4]
    }

    /// The fame a new world starts with. Each player has had `age - 20`
    /// seasons so far, played at a level a little above where he is now if
    /// he is older (his body has gone since), with a career that went better
    /// or worse than his ability says. A few veterans well past their best
    /// are put at the top: names who still sell tickets.
    fn seed_fame(&mut self) {
        let bar = self.starter_bar();
        let mut rng = Rng::new(self.econ.seed ^ 0xFA3E_5EED);
        let mut old: Vec<(f32, PlayerId)> = Vec::new();
        for t in 0..self.teams.len() {
            for &p in &self.teams[t].roster.clone() {
                let pl = &mut self.players[p as usize];
                let age = pl.age as i32;
                let rel = pl.overall() + 0.3 * (age - 30).max(0) as f32 - bar;
                let luck = 0.4 + 1.4 * rng.f32();
                let mut fame = 0.0f32;
                for y in 20..age {
                    let late = (1.0 - 0.12 * (y - 31).max(0) as f32).max(0.0);
                    let g =
                        (rel + 0.8).max(0.0) * 3.8 * 1.15 * luck * late * pl.signature_fame_mult();
                    fame = fame * DECAY + g * (1.0 - fame / SATURATE);
                }
                // The odd career has a title and an award in it.
                if age >= 28 && rel > 0.0 && rng.chance(0.18) {
                    fame += 12.0 + 8.0 * rng.f32();
                }
                if age <= 22 {
                    fame += (pl.potential - 13.0).max(0.0) * 0.5;
                }
                pl.renown.fame = fame.clamp(0.0, 95.0);
                pl.renown.peak = pl.renown.fame;
                pl.renown.tenure = rng.range(0, 7) as u8;
                pl.renown.tenure_club = Some(t);
                if age >= 32 {
                    old.push((rel + rng.f32(), p));
                }
            }
        }
        // Two names from another era: a thirty-something star whose legs
        // have gone but whose name is as big as ever. Only in a world that
        // has not begun: a career being loaded keeps its players as they
        // are, and its contracts as they were signed.
        let fresh = self.history.is_empty() && self.season == 0 && self.round == 0;
        old.sort_by(|a, b| b.0.total_cmp(&a.0));
        for &(_, p) in old.iter().take(if fresh { 2 } else { 0 }) {
            let pl = &mut self.players[p as usize];
            pl.age = 36 + rng.range(0, 4) as u8;
            for a in Attr::ALL {
                let drop = if a.is_physical() { 1.3 } else { 0.3 };
                pl.attrs[a] = (pl.attrs[a] - drop).clamp(1.0, 20.0);
            }
            pl.renown.fame = 66.0 + 14.0 * rng.f32();
            pl.renown.peak = pl.renown.fame + 10.0;
            pl.renown.tenure = 3 + rng.range(0, 8) as u8;
            pl.renown.titles = 1 + rng.range(0, 3) as u8;
            pl.renown.mvps = rng.range(0, 3) as u8;
            pl.renown.elite = 6 + rng.range(0, 6) as u8;
        }
        if fresh {
            for t in 0..self.teams.len() {
                for &p in &self.teams[t].roster.clone() {
                    let pl = &mut self.players[p as usize];
                    pl.contract.salary = (pl.contract.salary as f32 * wage_mult(pl)) as u32;
                }
            }
        }
        self.fame.seeded = true;
    }

    /// The season is over: add what it earned to every player's fame and
    /// keep the honours. Called before the season's figures are cleared.
    pub(crate) fn fame_close_season(
        &mut self,
        table: &[TeamId],
        champion: TeamId,
        runner_up: Option<TeamId>,
        mvp: Option<PlayerId>,
    ) {
        let bar = self.starter_bar();
        let had_playoffs = !self.playoffs.is_empty();
        // Best game score of each man in the playoffs.
        let mut big = vec![0.0f32; self.players.len()];
        for f in self.fixtures.iter().filter(|f| f.playoff) {
            let Some(r) = &f.result else { continue };
            for (p, l) in r.boxes.iter().flatten() {
                big[*p as usize] += (l.game_score() - 18.0).max(0.0) * 0.15;
            }
        }
        // Scoring title: most points a game among the regulars.
        let mut scorer: Option<(PlayerId, f32)> = None;
        for t in 0..self.teams.len() {
            let team_games = (self.standings[t].wins + self.standings[t].losses).max(1);
            for &p in &self.teams[t].roster {
                let g = self.games_played[p as usize];
                if g * 10 < team_games * 6 {
                    continue;
                }
                let ppg = self.season_stats[p as usize].pts as f32 / g as f32;
                if scorer.is_none_or(|s| ppg > s.1) {
                    scorer = Some((p, ppg));
                }
            }
        }
        let _ = table;
        for t in 0..self.teams.len() {
            let team_games = (self.standings[t].wins + self.standings[t].losses).max(1) as f32;
            let squad: Vec<PlayerId> = self.teams[t]
                .roster
                .iter()
                .chain(&self.teams[t].reserves)
                .copied()
                .collect();
            for p in squad {
                let games = self.games_played[p as usize];
                let line = self.season_stats[p as usize];
                let pl = &mut self.players[p as usize];
                let r = &mut pl.renown;
                if r.tenure_club == Some(t) {
                    r.tenure = r.tenure.saturating_add(1);
                } else {
                    r.tenure_club = Some(t);
                    r.tenure = 1;
                }
                let share = (games as f32 / team_games).min(1.0);
                let mins = if games > 0 {
                    line.secs as f32 / 60.0 / games as f32
                } else {
                    0.0
                };
                let usage = share * (mins / 28.0).min(1.2);
                // A signature move sells: 10% more fame for each.
                let sells = pl.signature_fame_mult();
                let rel = pl.overall() - bar;
                let mut gain = (rel + 0.8).max(0.0) * 3.8 * usage;
                if games > 0 {
                    gain += (line.rating_per(games) - 6.0).max(0.0) * 1.2 * usage;
                }
                let r = &mut pl.renown;
                if usage >= 0.5 && rel > 0.0 {
                    r.elite = r.elite.saturating_add(1);
                }
                if mvp == Some(p) {
                    gain += 10.0;
                    r.mvps = r.mvps.saturating_add(1);
                }
                if scorer.map(|s| s.0) == Some(p) {
                    gain += 5.0;
                    r.scoring = r.scoring.saturating_add(1);
                }
                if share >= 0.5 && had_playoffs {
                    if t == champion {
                        gain += 4.0;
                        r.titles = r.titles.saturating_add(1);
                        r.finals = r.finals.saturating_add(1);
                    } else if runner_up == Some(t) {
                        gain += 1.5;
                        r.finals = r.finals.saturating_add(1);
                    }
                }
                gain += big[p as usize].min(4.0);
                if pl.age <= 22 && usage > 0.0 {
                    gain += (pl.potential - 13.0).max(0.0) * 0.6;
                }
                let r = &mut pl.renown;
                if usage >= 0.3 {
                    gain += 0.12 * r.tenure.min(10) as f32;
                }
                r.fame =
                    (r.fame * DECAY + gain * sells * (1.0 - r.fame / SATURATE)).clamp(0.0, 99.0);
                r.peak = r.peak.max(r.fame);
                r.shirts = 0;
                r.bench_told = false;
                r.bench_weeks = 0;
            }
        }
        // People without a club only fade.
        for p in 0..self.players.len() {
            if self.team_of(p as PlayerId).is_none() {
                let r = &mut self.players[p].renown;
                r.fame *= DECAY;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_league_has_a_few_big_names() {
        let w = World::generate(5, 12);
        let names = w.biggest_names(10);
        assert!(names[0].1 >= 60.0, "top name {}", names[0].1);
        // A long tail: most of the league is unknown.
        let all: Vec<f32> = w.players.iter().map(|p| p.renown.fame).collect();
        let unknown = all.iter().filter(|&&f| f < 8.0).count();
        assert!(unknown * 2 > all.len(), "{unknown} of {}", all.len());
        // Two veterans past their best are among the names.
        let old = names
            .iter()
            .filter(|n| w.players[n.0 as usize].age >= 36)
            .count();
        assert!(old >= 2, "{old} old names in {names:?}");
    }
}
