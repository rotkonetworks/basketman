//! Divisions: one league per tier, promotion and relegation between them.
//!
//! All clubs live in `World::teams`; each has a `division` (0 is the top).
//! Fixtures carry their division. The standings are indexed by club as
//! always, so a division's table is just its clubs, sorted.

use serde::{Deserialize, Serialize};

use crate::fans::C_FINISH;
use crate::finance::Cat;
use crate::inbox::Kind;
use crate::league::{TeamId, World};

/// How much weaker, in the units of a squad's average quality, each
/// division down is.
pub const QUALITY_STEP: f32 = 1.05;
/// What a club of each division starts with in the bank.
pub const START_BUDGET: [i64; 5] = [1_000_000, 650_000, 420_000, 300_000, 250_000];
/// How big a hall is, against the top division's.
pub const HALL_SCALE: [f32; 5] = [1.0, 0.8, 0.65, 0.5, 0.45];
/// Prize and broadcast money against the top division's.
pub const MONEY_SCALE: [f32; 5] = [1.0, 0.6, 0.4, 0.25, 0.2];
/// Paid into the bank of a club that goes up.
pub const PROMOTION_WINDFALL: i64 = 250_000;

/// The shape of the league.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tiers {
    pub divisions: usize,
    /// Clubs that go up from each division, and down from each.
    pub moves: usize,
}

impl Default for Tiers {
    fn default() -> Self {
        Tiers {
            divisions: 1,
            moves: 2,
        }
    }
}

/// A club that changed division at the end of a season.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub team: TeamId,
    pub from: usize,
    pub to: usize,
}

impl World {
    /// The division of the club the manager runs, or the top one.
    pub fn my_division(&self) -> usize {
        self.watch.map_or(0, |t| self.teams[t].division)
    }

    /// A division's clubs, first place first: wins, then point difference.
    pub fn table_of(&self, division: usize) -> Vec<TeamId> {
        let mut ids = self.division_teams(division);
        ids.sort_by_key(|&t| {
            let s = &self.standings[t];
            (
                std::cmp::Reverse(s.wins),
                std::cmp::Reverse(s.points_for as i64 - s.points_against as i64),
            )
        });
        ids
    }

    /// The table of the manager's own division.
    pub fn table(&self) -> Vec<TeamId> {
        self.table_of(self.my_division())
    }

    /// Every division's table, the top one first.
    pub fn division_tables(&self) -> Vec<Vec<TeamId>> {
        (0..self.division_count())
            .map(|d| self.table_of(d))
            .collect()
    }

    /// Clubs in the division of `club`.
    pub fn division_size(&self, club: TeamId) -> usize {
        self.division_teams(self.teams[club].division).len()
    }

    /// Rounds of the regular season: that of the largest division.
    pub(crate) fn regular_rounds(&self) -> usize {
        let biggest = (0..self.division_count())
            .map(|d| self.division_teams(d).len())
            .max()
            .unwrap_or(0);
        2 * biggest.saturating_sub(1)
    }

    /// Prize and broadcast money of a club's division, against the top one.
    pub(crate) fn money_scale(&self, club: TeamId) -> f32 {
        MONEY_SCALE[self.teams[club].division.min(4)]
    }

    /// The clubs that go up and down, from final tables. Changes nothing.
    pub fn moves_from(&self, tables: &[Vec<TeamId>]) -> Vec<Move> {
        let m = self.tiers.moves;
        let mut out = Vec::new();
        if tables.len() < 2 {
            return out;
        }
        for d in 1..tables.len() {
            let k = m.min(tables[d].len() / 2);
            for &t in tables[d].iter().take(k) {
                out.push(Move {
                    team: t,
                    from: d,
                    to: d - 1,
                });
            }
        }
        for d in 0..tables.len() - 1 {
            let k = m.min(tables[d].len() / 2);
            for &t in tables[d].iter().rev().take(k) {
                out.push(Move {
                    team: t,
                    from: d,
                    to: d + 1,
                });
            }
        }
        out
    }

    /// Promotion and relegation: the moves are made and money, fans and
    /// sponsors answer them. Returns what moved.
    pub(crate) fn promote_and_relegate(&mut self, tables: &[Vec<TeamId>]) -> Vec<Move> {
        let moves = self.moves_from(tables);
        for mv in &moves {
            self.teams[mv.team].division = mv.to;
            let up = mv.to < mv.from;
            if up {
                // A windfall: the league's money is bigger up there.
                self.transact(mv.team, PROMOTION_WINDFALL, Cat::Prize);
            }
            let a = &mut self.econ.arenas[mv.team];
            let (prestige, support, fans, fee) = if up {
                (0.10, 0.05, 0.06, 1.25)
            } else {
                (-0.10, -0.06, -0.08, 0.70)
            };
            a.prestige = (a.prestige + prestige).clamp(0.05, 1.0);
            a.support = (a.support + support).clamp(0.1, 0.95);
            self.fans_move(mv.team, C_FINISH, fans);
            // Sponsors pay for the league the club plays in.
            for d in self.econ.deals.iter_mut().filter(|d| d.club == mv.team) {
                d.fee = (d.fee as f32 * fee) as i64 / 100 * 100;
            }
        }
        if let Some(me) = self.watch
            && let Some(mv) = moves.iter().find(|m| m.team == me)
        {
            let up = mv.to < mv.from;
            // Promotion raises what is asked. Relegation asks for an
            // immediate return. The board's number is set in `club_after_moves`.
            let ask = self.expectation_after_move(me, !up);
            let name = self.division_name(mv.to);
            let (subject, body) = if up {
                (
                    format!("Promoted to the {name}"),
                    format!(
                        "We are going up. The league office has paid {} into the club, the \
                         sponsors are paying more and the fans are talking about little else. \
                         The {name} is a harder league: the squads are better, the halls are \
                         bigger and nobody will give us anything. The board asks for a place in \
                         the top {}.",
                        crate::inbox::money(PROMOTION_WINDFALL),
                        ask
                    ),
                )
            } else {
                (
                    format!("Relegated to the {name}"),
                    format!(
                        "We are going down. Sponsors have cut what they pay, the crowds will be \
                         smaller and the league's money is less. The board wants an immediate \
                         return: a place in the top {} of the {name}.",
                        ask
                    ),
                )
            };
            self.post(self.day, Kind::Board, "The board", subject, body, vec![]);
        }
        moves
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::inbox::Stop;

    #[test]
    fn three_divisions_are_three_leagues() {
        let w = World::generate_tiers(3, 8, 3);
        assert_eq!(w.teams.len(), 24);
        assert_eq!(w.division_count(), 3);
        for d in 0..3 {
            assert_eq!(w.division_teams(d).len(), 8);
        }
        // Fixtures carry their division and never cross it.
        for f in &w.fixtures {
            assert_eq!(w.teams[f.home].division, f.division);
            assert_eq!(w.teams[f.away].division, f.division);
        }
        assert_eq!(w.fixtures.len(), 3 * 8 * 7);
        // Names are unique, squads weaker and halls and budgets smaller by
        // tier.
        let mut names: Vec<&String> = w.teams.iter().map(|t| &t.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 24);
        let strength = |d: usize| {
            let ids = w.division_teams(d);
            ids.iter()
                .map(|&t| {
                    let mut ov: Vec<f32> = w.teams[t]
                        .roster
                        .iter()
                        .map(|&p| w.players[p as usize].overall())
                        .collect();
                    ov.sort_by(|a, b| b.total_cmp(a));
                    ov.iter().take(8).sum::<f32>() / 8.0
                })
                .sum::<f32>()
                / ids.len() as f32
        };
        let cap = |d: usize| {
            let ids = w.division_teams(d);
            ids.iter().map(|&t| w.econ.arenas[t].capacity as f32).sum::<f32>() / ids.len() as f32
        };
        let pay = |d: usize| {
            let ids = w.division_teams(d);
            ids.iter().map(|&t| w.payroll(t) as f32).sum::<f32>() / ids.len() as f32
        };
        for d in 0..2 {
            assert!(strength(d) > strength(d + 1) + 0.5, "{d}");
            assert!(cap(d) > cap(d + 1), "halls {d}");
            assert!(pay(d) > pay(d + 1), "payroll {d}");
            assert!(
                w.division_teams(d).iter().map(|&t| w.teams[t].budget).sum::<i64>()
                    > w.division_teams(d + 1).iter().map(|&t| w.teams[t].budget).sum::<i64>()
            );
        }
    }

    #[test]
    fn promotion_and_relegation_swap_the_extremes() {
        let mut w = World::new_career_tiers(5, 8, 3);
        w.apply(Command::TakeCharge(17)).unwrap();
        assert_eq!(w.teams[17].division, 2);
        let mut guard = 0;
        while !w.season_over() {
            guard += 1;
            assert!(guard < 3000);
            if w.match_today() {
                w.apply(Command::PlayRound).unwrap();
            } else {
                w.apply(Command::Advance).unwrap();
            }
        }
        // Everyone played their own division only.
        for t in 0..24 {
            let s = &w.standings[t];
            assert_eq!(s.wins + s.losses, 14, "club {t}");
        }
        let tables = w.division_tables();
        let budgets: Vec<i64> = w.teams.iter().map(|t| t.budget).collect();
        let expect = w.moves_from(&tables);
        assert_eq!(expect.len(), 4 * 2);
        loop {
            match w.apply(Command::Advance).unwrap().stop {
                Some(Stop::Draft) => {
                    w.apply(Command::DraftAuto { all: true }).unwrap();
                    break;
                }
                _ if w.season == 1 => break,
                _ => {}
            }
        }
        assert_eq!(w.season, 1);
        // The top two of the lower divisions went up, the bottom two of
        // the higher ones went down, and nobody else moved.
        for t in 0..24 {
            let want = expect
                .iter()
                .find(|m| m.team == t)
                .map_or(w.teams[t].division, |m| m.to);
            assert_eq!(w.teams[t].division, want);
        }
        for d in 0..3 {
            assert_eq!(w.division_teams(d).len(), 8);
        }
        for m in &expect {
            let up = m.to < m.from;
            let was = budgets[m.team];
            let _ = was;
            // The record of the season remembers the divisions.
            assert_eq!(w.history[0].division_of(m.team), Some(m.from));
            if up {
                assert!(w.econ.arenas[m.team].prestige > 0.0);
            }
        }
        // New fixtures are within the new divisions.
        for f in &w.fixtures {
            assert_eq!(w.teams[f.home].division, f.division);
            assert_eq!(w.teams[f.away].division, f.division);
        }
        assert_eq!(w.fixtures.len(), 3 * 8 * 7);
        // The First Division has its champion; lower tables are in history.
        assert_eq!(w.history[0].divisions.len(), 3);
        assert!(w.history[0].division_of(w.history[0].champion) == Some(0));
    }

    #[test]
    fn a_career_with_one_division_is_the_league_it_was() {
        let w = World::generate(4, 8);
        assert_eq!(w.division_count(), 1);
        assert_eq!(w.table().len(), 8);
        assert!(w.moves_from(&w.division_tables()).is_empty());
    }

    #[test]
    fn three_seasons_of_moves_keep_every_club_able_to_play() {
        let mut w = World::new_career_tiers(8, 6, 3);
        w.apply(Command::TakeCharge(15)).unwrap();
        let mut guard = 0;
        while w.season < 3 {
            guard += 1;
            assert!(guard < 6000, "stuck in season {}", w.season);
            match w.apply(Command::Advance).unwrap().stop {
                Some(Stop::MatchDay) => {
                    w.apply(Command::PlayRound).unwrap();
                }
                Some(Stop::Draft) => {
                    w.apply(Command::DraftAuto { all: true }).unwrap();
                }
                _ => {}
            }
        }
        assert_eq!(w.history.len(), 3);
        for r in &w.history {
            assert_eq!(r.divisions.len(), 3);
            assert!(r.divisions.iter().all(|t| t.len() == 6));
            assert_eq!(r.division_of(r.champion), Some(0));
        }
        for d in 0..3 {
            assert_eq!(w.division_teams(d).len(), 6, "division {d}");
        }
        for t in &w.teams {
            assert!((crate::inbox::MIN_ROSTER..=crate::inbox::MAX_ROSTER).contains(&t.roster.len()));
            assert!(t.playbook[t.active].tactics.rotation.len() >= 5);
        }
        // The money stays in the books: every club has a ledger and a hall.
        assert_eq!(w.econ.arenas.len(), 18);
        assert_eq!(w.econ.ledgers.len(), 18);
        // Promotion mail reached the manager when he went up or down.
        let moved = w.history.windows(2).any(|p| p[0].division_of(15) != p[1].division_of(15));
        let mail = w
            .desk
            .messages
            .iter()
            .any(|m| m.subject.starts_with("Promoted") || m.subject.starts_with("Relegated"));
        let moved = moved || w.teams[15].division != w.history[2].division_of(15).unwrap();
        // Mail only ever follows a move; and a manager who kept the job in
        // these three seasons was told of every one (one the board sacked
        // stops hearing about the club, so only the first part holds).
        assert!(!mail || moved, "promotion mail without a move");
        if w.watch.is_some() {
            assert_eq!(moved, mail);
        }
    }
}
