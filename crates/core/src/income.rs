//! Income that is not the gate or the boards: the league's broadcast money
//! and what the club sells in its shop.
//!
//! Broadcast: the league sells its games as one package. The pool is a fixed
//! sum per club; half is shared equally and half by how each club finished
//! last season (before there was a last season, by the standing of its hall).
//! It is paid in equal parts through the regular season.
//!
//! Merchandise: what the town spends on the club's colours. It follows the
//! support the club has, how it is winning, and how big its home town is, and
//! it jumps in the playoffs. Each player adds his own pull on top
//! (`player_merch`: his fame, his form and the club's fan base, see
//! `fame.rs` and `fans.rs`), and `merch_week` sums it over the squad.
//!
//! Nothing here draws from `World::rng`.

use crate::league::{TeamId, World};
use crate::player::PlayerId;

/// What the league's broadcast partners pay for each club in the league.
pub const BROADCAST_PER_CLUB: i64 = 90_000;
/// What an average club's shop takes in a round.
const MERCH_UNIT: f32 = 2_000.0;
/// What the shop takes in a round from a player of fame 100 in an ordinary
/// club, before his form and the fan base.
const MERCH_STAR: f32 = 1_000.0;

impl World {
    /// Rounds the money in `Economy` is spread over.
    pub(crate) fn pay_rounds(&self) -> i64 {
        (self.regular_rounds()).max(1) as i64
    }

    /// 0-based finish last season, or by the hall's standing when there was
    /// none.
    pub fn last_finish(&self, club: TeamId) -> usize {
        if let Some(rec) = self.history.last() {
            // Where the club finished, in the division it played in.
            let tables: Vec<&Vec<(TeamId, u16, u16)>> = if rec.divisions.is_empty() {
                vec![&rec.table]
            } else {
                rec.divisions.iter().collect()
            };
            let played = rec.division_of(club);
            let now = self.teams[club].division;
            match played {
                // Just promoted: the bottom of the new division's expectations;
                // just relegated: the top of the new one.
                Some(d) if d > now => return self.division_size(club) - 1,
                Some(d) if d < now => return 0,
                _ => {}
            }
            if let Some(p) = tables
                .iter()
                .find_map(|t| t.iter().position(|r| r.0 == club))
            {
                return p;
            }
        }
        let mine = self.econ.arenas[club].prestige;
        self.division_teams(self.teams[club].division)
            .into_iter()
            .filter(|&t| {
                let o = self.econ.arenas[t].prestige;
                o > mine || (o == mine && t < club)
            })
            .count()
    }

    /// The club's broadcast money for the season.
    pub fn broadcast_season(&self, club: TeamId) -> i64 {
        let n = self.division_size(club).max(1) as i64;
        let pool = (n * BROADCAST_PER_CLUB) as f32 * self.money_scale(club);
        let pool = pool as i64;
        let equal = pool / 2 / n;
        let rank = self.last_finish(club) as i64;
        let perf = pool / 2 * (n - rank) / (n * (n + 1) / 2);
        equal + perf
    }

    /// What one player's name adds to the shop in a round, at the club he
    /// is on: his fame, how he is playing and how big the club's fan base
    /// is. A player who is hurt sells a little, and one in his farewell
    /// season a lot more.
    pub fn player_merch(&self, player: PlayerId) -> i64 {
        let p = &self.players[player as usize];
        let Some(club) = self.team_of(player) else {
            return 0;
        };
        let f = crate::fame::heat(p.renown.fame);
        if f <= 0.0 {
            return 0;
        }
        let g = self.games_played[player as usize];
        let form = if g >= 3 {
            0.75 + 0.5
                * ((self.season_stats[player as usize].rating_per(g) - 4.0) / 4.0).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let fit = if p.fit() { 1.0 } else { 0.3 };
        let size = (self.fan_base(club) as f32 / 15_000.0).clamp(0.5, 1.8);
        let farewell = if p.renown.farewell { 1.8 } else { 1.0 };
        let local = if p.renown.tenure_club == Some(club) {
            1.0 + 0.03 * p.renown.tenure.min(10) as f32
        } else {
            1.0
        };
        (MERCH_STAR * f * form * fit * size * farewell * local) as i64
    }

    /// The shirts the club's players sell, best first: player and what he
    /// brings in a round.
    pub fn top_sellers(&self, club: TeamId, n: usize) -> Vec<(PlayerId, i64)> {
        let mut v: Vec<(PlayerId, i64)> = self.teams[club]
            .roster
            .iter()
            .map(|&p| (p, self.player_merch(p)))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.truncate(n);
        v
    }

    /// What the shop takes in a regular round: the club's colours, plus
    /// whatever its players add.
    pub fn merch_week(&self, club: TeamId) -> i64 {
        let a = &self.econ.arenas[club];
        let standing = self.standing_of(club);
        let town = 0.6 + 0.4 * a.capacity as f32 / 6_000.0;
        let colours = MERCH_UNIT * (0.45 + 1.2 * a.support) * (0.65 + 0.7 * standing) * town;
        let stars: i64 = self.teams[club]
            .roster
            .iter()
            .map(|&p| self.player_merch(p))
            .sum();
        colours as i64 + stars
    }

    /// Add this round's shirt sales to each player's season total.
    fn tally_shirts(&mut self, club: TeamId) {
        for p in self.teams[club].roster.clone() {
            let m = self.player_merch(p);
            self.players[p as usize].renown.shirts += m;
        }
    }

    /// Broadcast and shop money for round `self.round`, for every club.
    /// Playoff nights sell extra at the shop for the clubs still playing.
    pub(crate) fn book_media_income(&mut self, regular: bool, played: &[usize]) {
        let rounds = self.pay_rounds();
        for club in 0..self.teams.len() {
            if regular {
                let b = self.broadcast_season(club) / rounds;
                self.transact(club, b, crate::finance::Cat::Broadcast);
                let m = self.merch_week(club);
                self.transact(club, m, crate::finance::Cat::Merchandise);
                self.tally_shirts(club);
            } else {
                let playing = played.iter().any(|&i| {
                    let f = &self.fixtures[i];
                    f.playoff && (f.home == club || f.away == club)
                });
                if playing {
                    let final_night = self
                        .playoffs
                        .iter()
                        .any(|s| s.is_final && (s.high == club || s.low == club));
                    let mult = if final_night { 3.0 } else { 1.8 };
                    let m = (self.merch_week(club) as f32 * mult) as i64;
                    self.transact(club, m, crate::finance::Cat::Merchandise);
                }
            }
        }
    }
}
