//! The second team. Every club keeps a reserve squad of youngsters, and its
//! reserves play the other clubs' reserves the same week the first teams
//! meet. It is where the young get minutes, where a first-team player on
//! the fringe stays sharp, and where a man back from injury finds his legs
//! before he is asked to play a real game.

use serde::{Deserialize, Serialize};

use crate::engine::{BoxLine, Lineup, simulate_game};
use crate::inbox::{MAX_ROSTER, MIN_ROSTER};
use crate::league::{Standing, TeamId, World};
use crate::player::{Player, PlayerId};
use crate::rng::Rng;
use crate::tactics::Tactics;

/// Reserve players a club starts with, and keeps at most after a summer.
pub const RESERVE_SQUAD: usize = 8;
pub const RESERVE_MAX: usize = 11;
/// New youngsters each club brings in every summer.
const INTAKE: usize = 2;

/// One reserve-league game, for the results list.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReserveGame {
    pub round: usize,
    pub home: TeamId,
    pub away: TeamId,
    pub score: [u16; 2],
    /// Best performer by game score, and his line.
    pub star: Option<(PlayerId, BoxLine)>,
}

/// A youngster for a reserve squad: raw, cheap, with room to grow.
fn youth(id: PlayerId, quality: f32, rng: &mut Rng) -> Player {
    let slot = rng.range(0, 5) as usize;
    let age = 17 + rng.range(0, 4) as u8;
    let mut p = crate::league::generate_player_at(id, slot, quality, age, rng);
    p.potential = (p.overall() + crate::league::age_gap(age) + 1.0 + rng.f32() * 5.0).min(20.0);
    p.assign_signatures();
    p.contract.salary = 8_000;
    p.contract.years_left = 2;
    p
}

/// The reserve squads a new world starts with, appended to `players`.
/// Drawn from its own stream so the first teams do not depend on them.
pub(crate) fn seed(seed: u64, clubs: usize, players: &mut Vec<Player>) -> Vec<Vec<PlayerId>> {
    let rng = &mut Rng::new(seed ^ 0x2E5E_27E5);
    (0..clubs)
        .map(|_| {
            (0..RESERVE_SQUAD)
                .map(|_| {
                    let id = players.len() as PlayerId;
                    let quality = 6.6 + rng.f32() * 1.6;
                    players.push(youth(id, quality, rng));
                    id
                })
                .collect()
        })
        .collect()
}

impl World {
    /// True if he is in a club's reserve squad.
    pub fn in_reserves(&self, p: PlayerId) -> bool {
        self.teams.iter().any(|t| t.reserves.contains(&p))
    }

    /// Who would turn out for a club's reserves this week: the fit men of
    /// the reserve squad, and first-team players made available who are
    /// not in the first team's rotation.
    pub fn reserve_side(&self, team: TeamId) -> Vec<PlayerId> {
        let t = &self.teams[team];
        let rotation = &t.tactics().rotation;
        let lent = t
            .down
            .iter()
            .copied()
            .filter(|p| t.roster.contains(p) && !rotation.iter().any(|s| s.player == *p));
        let mut side: Vec<PlayerId> = t
            .reserves
            .iter()
            .copied()
            .chain(lent)
            .filter(|&p| self.players[p as usize].fit())
            .collect();
        // Ten dress. First-team men who need the game go first, then the
        // best of the rest.
        side.sort_by(|&a, &b| {
            let key = |p: PlayerId| {
                let lent = t.roster.contains(&p) as u8 as f32 * 100.0;
                lent + self.players[p as usize].overall()
            };
            key(b).total_cmp(&key(a))
        });
        side.truncate(10);
        side
    }

    /// Whether a first-team player turns out for the reserves when he is
    /// not in the first team's rotation.
    pub fn set_down(&mut self, team: TeamId, p: PlayerId, on: bool) {
        let down = &mut self.teams[team].down;
        down.retain(|&q| q != p);
        if on {
            down.push(p);
        }
    }

    /// Bring a reserve into the first-team squad.
    pub fn promote(&mut self, team: TeamId, p: PlayerId) -> Result<(), String> {
        let t = &mut self.teams[team];
        if !t.reserves.contains(&p) {
            return Err("He is not in the reserves.".into());
        }
        if t.roster.len() >= MAX_ROSTER {
            return Err(format!("The first team is full at {MAX_ROSTER}."));
        }
        t.reserves.retain(|&q| q != p);
        t.roster.push(p);
        Ok(())
    }

    /// Move a first-team player to the reserve squad for good.
    pub fn demote(&mut self, team: TeamId, p: PlayerId) -> Result<(), String> {
        let t = &mut self.teams[team];
        if !t.roster.contains(&p) {
            return Err("He is not in the first team.".into());
        }
        if t.roster.len() <= MIN_ROSTER {
            return Err(format!("The first team cannot go below {MIN_ROSTER}."));
        }
        if t.reserves.len() >= RESERVE_MAX {
            return Err("The reserve squad is full.".into());
        }
        t.roster.retain(|&q| q != p);
        t.down.retain(|&q| q != p);
        t.reserves.push(p);
        self.players[p as usize].morale = (self.players[p as usize].morale - 0.12).max(0.0);
        self.refit(team);
        Ok(())
    }

    /// Other clubs' coaches: whoever is fit and not in the rotation plays
    /// for the reserves, so nobody at the end of the bench goes stale.
    fn manage_reserves(&mut self) {
        for team in 0..self.teams.len() {
            if self.watch == Some(team) {
                continue;
            }
            let t = &self.teams[team];
            let rotation = &t.tactics().rotation;
            let down = t
                .roster
                .iter()
                .copied()
                .filter(|p| !rotation.iter().any(|s| s.player == *p))
                .collect();
            self.teams[team].down = down;
        }
    }

    /// The reserve games that go with a round of first-team fixtures:
    /// the same pairings, played midweek.
    pub(crate) fn reserves_for(&mut self, round: usize) {
        if self.reserve_rounds_done > round {
            return;
        }
        self.reserve_rounds_done = round + 1;
        let played: Vec<usize> = (0..self.fixtures.len())
            .filter(|&i| self.fixtures[i].round == round && !self.fixtures[i].playoff)
            .collect();
        // The seed of the league game, if it was played; a game of its own
        // otherwise, so the reserves never disturb the world's stream.
        let seeds: Vec<u64> = played
            .iter()
            .map(|&i| {
                self.round_seeds
                    .iter()
                    .find(|s| s.0 == i)
                    .map_or(self.econ.seed ^ ((self.season as u64) << 32) ^ (i as u64) << 8, |s| s.1)
            })
            .collect();
        self.round_seeds.clear();
        self.manage_reserves();
        let n = self.players.len();
        self.reserve_stats.resize(n, BoxLine::default());
        self.reserve_games.resize(n, 0);
        if self.reserve_table.len() != self.teams.len() {
            self.reserve_table = vec![Standing::default(); self.teams.len()];
        }
        let regulation = (self.rules.periods as u32 * self.rules.period_secs) as f32;
        // Each game touches only its own two clubs' players, so all of the
        // round's games can be played at once and their results taken in
        // order. Forfeits first, then the games, across threads when the
        // build has them.
        struct Game {
            round: usize,
            home: TeamId,
            away: TeamId,
            seed: u64,
            sides: [Vec<PlayerId>; 2],
        }
        let mut games: Vec<Game> = Vec::new();
        for (&i, &seed) in played.iter().zip(&seeds) {
            let f = &self.fixtures[i];
            if f.playoff {
                continue;
            }
            let (round, home, away) = (f.round, f.home, f.away);
            let sides = [self.reserve_side(home), self.reserve_side(away)];
            // A club that cannot raise five forfeits.
            let short = [sides[0].len() < 5, sides[1].len() < 5];
            if short[0] || short[1] {
                if short[0] != short[1] {
                    let winner = if short[0] { away } else { home };
                    self.reserve_table[winner].wins += 1;
                    self.reserve_table[[home, away][(winner == home) as usize]].losses += 1;
                }
                continue;
            }
            games.push(Game { round, home, away, seed, sides });
        }
        let play = |g: &Game| {
            let squad = |side: &[PlayerId]| -> Vec<&Player> {
                side.iter().map(|&p| &self.players[p as usize]).collect()
            };
            let (a, b) = (squad(&g.sides[0]), squad(&g.sides[1]));
            let (ta, tb) = (Tactics::auto(&a), Tactics::auto(&b));
            let line = |t: &Tactics| -> Vec<&Player> {
                t.rotation
                    .iter()
                    .map(|s| &self.players[s.player as usize])
                    .collect()
            };
            // Thrown together on the day: nobody knows the system well.
            let lineup = |t| Lineup {
                tactics: t,
                players: line(t),
                familiarity: 0.6,
                belief: 0.55,
                coach: 8,
            };
            simulate_game(
                &lineup(&ta),
                &lineup(&tb),
                &self.rules,
                &mut Rng::new(g.seed ^ 0x2E5E_27E5),
            )
        };
        #[cfg(feature = "parallel")]
        let results: Vec<_> = {
            use rayon::prelude::*;
            games.par_iter().map(play).collect()
        };
        #[cfg(not(feature = "parallel"))]
        let results: Vec<_> = games.iter().map(play).collect();
        for (g, result) in games.iter().zip(results) {
            let (round, home, away) = (g.round, g.home, g.away);
            let mut star: Option<(PlayerId, BoxLine)> = None;
            for (side, team) in [home, away].into_iter().enumerate() {
                let s = &mut self.reserve_table[team];
                s.points_for += result.score[side] as u32;
                s.points_against += result.score[1 - side] as u32;
                if result.score[side] > result.score[1 - side] {
                    s.wins += 1;
                } else {
                    s.losses += 1;
                }
                for (pid, line) in &result.boxes[side] {
                    if line.secs == 0 {
                        continue;
                    }
                    self.reserve_stats[*pid as usize].add(line);
                    self.reserve_games[*pid as usize] += 1;
                    if star.is_none_or(|(_, l)| line.game_score() > l.game_score()) {
                        star = Some((*pid, *line));
                    }
                    let minutes = line.secs as f32 / 60.0;
                    // The pace is gentler than a first-team game.
                    self.wear_and_tear(team, *pid, minutes, line.secs as f32 / regulation, 0.7);
                }
            }
            self.reserve_log.push(ReserveGame {
                round,
                home,
                away,
                score: result.score,
                star,
            });
        }
    }

    /// The summer in the reserves: a year older, the new intake arrives,
    /// and those who never made it are let go.
    pub(crate) fn reserve_summer(&mut self) {
        self.reserve_table = vec![Standing::default(); self.teams.len()];
        self.reserve_log.clear();
        self.reserve_rounds_done = 0;
        for line in &mut self.reserve_stats {
            *line = BoxLine::default();
        }
        self.reserve_games.fill(0);
        for team in 0..self.teams.len() {
            self.teams[team].down.clear();
            for p in self.teams[team].reserves.clone() {
                let player = &mut self.players[p as usize];
                player.age += 1;
                player.condition = 1.0;
                player.morale += (0.5 - player.morale) * 0.6;
                player.complained = false;
                if let Some(inj) = &mut player.injury {
                    inj.weeks_left = inj.weeks_left.saturating_sub(14);
                    if inj.weeks_left == 0 {
                        player.injury = None;
                    }
                }
            }
            for _ in 0..INTAKE {
                let id = self.players.len() as PlayerId;
                let quality = 6.6 + self.rng.f32() * 1.8 + self.facilities(team).intake_quality();
                let mut p = youth(id, quality, &mut self.rng);
                p.potential =
                    (p.potential + self.facilities(team).intake_potential()).clamp(1.0, 20.0);
                self.players.push(p);
                self.season_stats.push(BoxLine::default());
                self.games_played.push(0);
                self.teams[team].reserves.push(id);
            }
            // Too many: the oldest with the least left to give go first.
            let mut keep = self.teams[team].reserves.clone();
            keep.sort_by(|&a, &b| {
                let worth = |p: PlayerId| {
                    let p = &self.players[p as usize];
                    p.potential + p.overall() - (p.age.saturating_sub(21)) as f32 * 1.5
                };
                worth(b).total_cmp(&worth(a))
            });
            keep.truncate(RESERVE_MAX - 1);
            self.teams[team].reserves.retain(|p| keep.contains(p));
        }
        self.scouting.grow(self.players.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A man back from a long injury is short of sharpness, sitting on the
    /// bench does not bring it back, and reserve games do.
    #[test]
    fn the_reserves_bring_a_player_back() {
        let run = |seed: u64, down: bool| {
            let mut w = World::generate(seed, 8);
            w.take_charge(0);
            let who = w.teams[0].tactics().rotation[0].player;
            w.players[who as usize].sharp = 0.55;
            for slot in &mut w.teams[0].playbook {
                slot.tactics.rotation.retain(|s| s.player != who);
            }
            w.set_down(0, who, down);
            for _ in 0..5 {
                w.play_round();
            }
            (
                w.players[who as usize].sharp,
                w.reserve_games.get(who as usize).copied().unwrap_or(0),
            )
        };
        // How many reserve games he gets in five weeks is down to the fixtures
        // of one club, and to whether he is fit and still out of the rotation,
        // so look at several worlds (eight: the claims are per world).
        let (mut idle_sum, mut played_sum, mut games_sum) = (0.0, 0.0, 0);
        for seed in 4..12 {
            let (idle, none) = run(seed, false);
            let (played, games) = run(seed, true);
            assert_eq!(none, 0);
            games_sum += games;
            idle_sum += idle;
            played_sum += played;
        }
        assert!(games_sum >= 16, "{games_sum} reserve games in eight worlds");
        assert!(
            played_sum / 8.0 > 0.8 && played_sum - idle_sum > 1.2,
            "idle {idle_sum}, reserves {played_sum} (sums over eight worlds)"
        );
    }

    #[test]
    fn a_reserve_league_is_played() {
        let mut w = World::generate(4, 8);
        w.take_charge(0);
        while !w.season_over() {
            w.play_round();
        }
        let games: u32 = w
            .reserve_table
            .iter()
            .map(|s| (s.wins + s.losses) as u32)
            .sum();
        assert_eq!(
            games,
            2 * 56,
            "every regular-season fixture has a reserve game"
        );
        assert!(w.teams.iter().all(|t| t.reserves.len() >= 5));
        while w.draft.is_none() {
            w.advance();
            if let Some(id) = w.needs_answer() {
                let offer = w.desk.messages[id as usize].offer.unwrap();
                w.answer_offer(offer, false);
            }
        }
        w.draft_auto(true);
        assert_eq!(w.season, 1);
        assert!(
            w.teams
                .iter()
                .all(|t| (5..RESERVE_MAX).contains(&t.reserves.len()))
        );
        assert!(w.reserve_log.is_empty());
    }
}
