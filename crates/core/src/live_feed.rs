//! The day around a live game: the other fixtures of the day as they stand
//! on the clock of the user's game, and the league table as it would read
//! if every game ended now.
//!
//! The other games are played at kickoff (see `World::kickoff_scoped`) from
//! the seeds they would have had anyway; this module only keeps their score
//! over the clock and reveals it. Nothing here touches the world.

use serde::Serialize;

use crate::engine::GameResult;
use crate::league::{LiveRound, TeamId, World};
use crate::replay::Recording;

/// Position on the game clock: periods run on, so one number orders any two
/// moments of a game (and of two games on the same clock).
pub fn clock_key(period: u8, clock: f32) -> f32 {
    period as f32 * 10_000.0 - clock
}

/// A game's score over its clock, kept after its recording is dropped.
#[derive(Clone, Debug, Default)]
pub struct Timeline {
    /// `(clock key, score)` at each change of the score.
    steps: Vec<(f32, [u16; 2])>,
    /// Where the game ended.
    end: f32,
    end_period: u8,
}

impl Timeline {
    pub fn of(rec: &Recording) -> Timeline {
        let mut steps = Vec::new();
        let mut last = [0u16; 2];
        for f in &rec.frames {
            if f.score != last {
                steps.push((clock_key(f.period, f.clock), f.score));
                last = f.score;
            }
        }
        let (end, end_period) = rec
            .frames
            .last()
            .map_or((0.0, 0), |f| (clock_key(f.period, f.clock), f.period));
        Timeline {
            steps,
            end,
            end_period,
        }
    }

    /// The score at `key`, and how long ago (in game seconds) it last
    /// changed, with the side that scored.
    fn at(&self, key: f32) -> ([u16; 2], Option<(f32, usize)>) {
        let n = self.steps.partition_point(|s| s.0 <= key);
        if n == 0 {
            return ([0, 0], None);
        }
        let (at, score) = self.steps[n - 1];
        let before = if n >= 2 { self.steps[n - 2].1 } else { [0, 0] };
        let side = usize::from(score[1] > before[1] && score[0] == before[0]);
        ([score[0], score[1]], Some((key - at, side)))
    }
}

/// One game of the day that is not the user's.
#[derive(Clone, Debug)]
pub struct Other {
    pub fixture: usize,
    pub timeline: Timeline,
}

/// How far a game of the day has got.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Status {
    Live,
    Final,
}

/// A game of the day as it stands now.
#[derive(Clone, Debug, Serialize)]
pub struct GameNow {
    pub fixture: usize,
    pub division: usize,
    pub home: TeamId,
    pub away: TeamId,
    pub home_name: String,
    pub away_name: String,
    pub score: [u16; 2],
    pub status: Status,
    pub period: u8,
    /// Seconds left in the period; 0 when final.
    pub clock: f32,
    /// Game seconds since the score last changed, and who scored.
    pub changed_ago: Option<f32>,
    pub changed_side: Option<usize>,
    /// The user's own game.
    pub ours: bool,
}

/// Where the user's game is: the clock on screen and the score on it.
#[derive(Copy, Clone, Debug)]
pub struct Now {
    pub period: u8,
    pub clock: f32,
    pub score: [u16; 2],
    /// The game is over: every other game of the day is final too.
    pub over: bool,
}

/// Why a place in the table matters.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Zone {
    None,
    Playoffs,
    Promotion,
    Relegation,
}

#[derive(Clone, Debug, Serialize)]
pub struct LiveRow {
    pub place: usize,
    /// Place before today's games.
    pub was: usize,
    pub club: TeamId,
    pub name: String,
    pub wins: u16,
    pub losses: u16,
    pub points_for: u32,
    pub points_against: u32,
    /// Playing right now, with the score on the clock (this side first).
    pub playing: Option<[u16; 2]>,
    pub zone: Zone,
    /// The row is the user's club.
    pub ours: bool,
}

/// How many places at either end of a division's table count for something.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Lines {
    /// Top places that are promoted (not in the top division).
    pub up: usize,
    /// Top places that go to the playoffs (top division only).
    pub playoffs: usize,
    /// Bottom places that go down (not in the bottom division).
    pub down: usize,
}

impl World {
    pub fn table_lines(&self, division: usize) -> Lines {
        let n = self.division_teams(division).len();
        let k = self.tiers.moves.min(n / 2);
        Lines {
            up: if division > 0 { k } else { 0 },
            playoffs: if division == 0 && n >= 4 { 4 } else { 0 },
            down: if division + 1 < self.division_count() { k } else { 0 },
        }
    }
}

impl LiveRound {
    /// Every game of the day as it stands at `now`, the user's first.
    pub fn games_now(&self, world: &World, now: Now) -> Vec<GameNow> {
        let key = clock_key(now.period, now.clock);
        let mut out = Vec::new();
        let f = &world.fixtures[self.fixture];
        out.push(GameNow {
            fixture: self.fixture,
            division: f.division,
            home: f.home,
            away: f.away,
            home_name: world.teams[f.home].name.clone(),
            away_name: world.teams[f.away].name.clone(),
            score: now.score,
            status: if now.over { Status::Final } else { Status::Live },
            period: now.period,
            clock: now.clock,
            changed_ago: None,
            changed_side: None,
            ours: true,
        });
        for o in &self.around {
            let f = &world.fixtures[o.fixture];
            let over = now.over || key >= o.timeline.end;
            let (score, change) = o.timeline.at(key.min(o.timeline.end));
            out.push(GameNow {
                fixture: o.fixture,
                division: f.division,
                home: f.home,
                away: f.away,
                home_name: world.teams[f.home].name.clone(),
                away_name: world.teams[f.away].name.clone(),
                score: if over { self.final_result(o.fixture).map_or(score, |r| r.score) } else { score },
                status: if over { Status::Final } else { Status::Live },
                period: if over { o.timeline.end_period } else { now.period },
                clock: if over { 0.0 } else { now.clock },
                changed_ago: change.map(|c| c.0).filter(|_| !over),
                changed_side: change.map(|c| c.1).filter(|_| !over),
                ours: false,
            });
        }
        out
    }

    /// A division's table as it reads if every game of the day that is on
    /// ended as it stands.
    pub fn table_now(&self, world: &World, now: Now, division: usize) -> Vec<LiveRow> {
        let games = self.games_now(world, now);
        let mut st = world.standings.clone();
        let mut playing = vec![None; world.teams.len()];
        for g in &games {
            if world.fixtures[g.fixture].playoff {
                continue;
            }
            for (side, team) in [g.home, g.away].into_iter().enumerate() {
                let (mine, theirs) = (g.score[side], g.score[1 - side]);
                let s = &mut st[team];
                s.points_for += mine as u32;
                s.points_against += theirs as u32;
                if mine > theirs {
                    s.wins += 1;
                } else if mine < theirs {
                    s.losses += 1;
                }
                if g.status == Status::Live {
                    playing[team] = Some([mine, theirs]);
                }
            }
        }
        let order = |st: &[crate::league::Standing]| {
            let mut ids = world.division_teams(division);
            ids.sort_by_key(|&t| {
                let s = &st[t];
                (
                    std::cmp::Reverse(s.wins),
                    std::cmp::Reverse(s.points_for as i64 - s.points_against as i64),
                )
            });
            ids
        };
        let before = order(&world.standings);
        let after = order(&st);
        let lines = world.table_lines(division);
        let n = after.len();
        after
            .iter()
            .enumerate()
            .map(|(i, &t)| LiveRow {
                place: i + 1,
                was: before.iter().position(|&b| b == t).unwrap_or(0) + 1,
                club: t,
                name: world.teams[t].name.clone(),
                wins: st[t].wins,
                losses: st[t].losses,
                points_for: st[t].points_for,
                points_against: st[t].points_against,
                playing: playing[t],
                zone: if i < lines.up {
                    Zone::Promotion
                } else if i < lines.playoffs {
                    Zone::Playoffs
                } else if i >= n - lines.down {
                    Zone::Relegation
                } else {
                    Zone::None
                },
                ours: world.watch == Some(t),
            })
            .collect()
    }

    /// The result the game will be booked with, for a game that is over.
    pub fn final_result(&self, fixture: usize) -> Option<&GameResult> {
        self.pre.iter().find(|p| p.0 == fixture).map(|p| &p.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::league::{LiveInput, Scope};

    fn career() -> World {
        let mut w = World::new_career_tiers(5, 8, 3);
        w.apply(Command::TakeCharge(1)).unwrap();
        // Through the camp to a match day.
        for _ in 0..200 {
            if w.match_today() {
                break;
            }
            w.apply(Command::Continue).unwrap();
        }
        assert!(w.match_today());
        w
    }

    /// A world with its recordings left out: a live game keeps the user's,
    /// the instant day keeps the same, but what is compared is the game.
    fn state(w: &World) -> String {
        let mut w = w.clone();
        w.log.clear();
        w.replay = None;
        w.tapes.clear();
        serde_json::to_string(&w).unwrap()
    }

    fn step_live(w: &mut World, scope: Scope, inputs: &[(u32, LiveInput)]) -> Vec<usize> {
        let me = w.watch.unwrap();
        let mut live = w.kickoff_scoped(me, scope).unwrap();
        let mut n = 0;
        let mut inputs = inputs.iter().peekable();
        while !live.game.is_over() {
            while inputs.peek().is_some_and(|i| i.0 <= n) {
                live.input(inputs.next().unwrap().1.clone());
            }
            live.step();
            n += 1;
        }
        w.full_time(live)
    }

    #[test]
    fn a_day_played_live_ends_in_the_world_an_instant_day_does() {
        let mut instant = career();
        let mut live = instant.clone();
        let mut late = instant.clone();
        instant.apply(Command::PlayToday).unwrap();
        let mine = live.user_fixture();
        let played = step_live(&mut live, Scope::Day, &[]);
        assert!(played.len() > 1, "other games were played with ours");
        // Against a day nobody watched, every game but the user's is the
        // same to the bit, and so is the table of the clubs not in it.
        for &i in played.iter().filter(|&&i| Some(i) != mine) {
            assert_eq!(
                serde_json::to_string(&instant.fixtures[i].result).unwrap(),
                serde_json::to_string(&live.fixtures[i].result).unwrap(),
                "fixture {i}"
            );
        }
        assert_eq!(
            serde_json::to_string(&instant.rng).unwrap(),
            serde_json::to_string(&live.rng).unwrap()
        );
        // Against the day as it was played before the other games were
        // played ahead (everything simulated when the game ends), the whole
        // world is the same, the user's game included.
        let me = late.watch.unwrap();
        let mut round = late.kickoff_scoped(me, Scope::Day).unwrap();
        round.pre.clear();
        round.around.clear();
        while !round.game.is_over() {
            round.step();
        }
        late.full_time(round);
        assert_eq!(state(&late), state(&live));
    }

    #[test]
    fn a_coached_day_is_still_the_same_for_every_other_game() {
        let mut a = career();
        let mut b = a.clone();
        // The same inputs on the same clock, once with the other games
        // played ahead of time at kickoff (as always now) and once replayed
        // from the log.
        let inputs = [(40, LiveInput::Timeout), (900, LiveInput::AutoSubs(false))];
        let played = step_live(&mut a, Scope::Day, &inputs);
        let Some(Command::PlayLiveToday { inputs }) = a.log.last().cloned() else {
            panic!("the live game was not logged");
        };
        assert_eq!(inputs.len(), 2);
        b.apply(Command::PlayLiveToday { inputs }).unwrap();
        for &i in &played {
            assert_eq!(
                serde_json::to_string(&a.fixtures[i].result).unwrap(),
                serde_json::to_string(&b.fixtures[i].result).unwrap()
            );
        }
        assert_eq!(state(&a), state(&b));
        assert_eq!(a.log.len(), b.log.len());
    }

    #[test]
    fn the_others_are_shown_over_the_clock_and_end_on_their_results() {
        let mut w = career();
        let me = w.watch.unwrap();
        let mut live = w.kickoff_today(me).unwrap();
        assert!(!live.around.is_empty());
        let mut seen_live = false;
        let mut last_sum = 0;
        while !live.game.is_over() {
            live.step();
            let Some(f) = live.game.recording().frames.last().cloned() else {
                continue;
            };
            let now = Now {
                period: f.period,
                clock: f.clock,
                score: f.score,
                over: false,
            };
            let games = live.games_now(&w, now);
            let sum: u32 = games.iter().skip(1).map(|g| (g.score[0] + g.score[1]) as u32).sum();
            // Scores only go up while the clock runs.
            assert!(sum >= last_sum || f.period > 3);
            last_sum = sum;
            seen_live |= games.iter().skip(1).any(|g| g.status == Status::Live);
            let table = live.table_now(&w, now, w.my_division());
            assert_eq!(table.len(), w.division_teams(w.my_division()).len());
        }
        assert!(seen_live);
        let ours = live.game.result().score;
        let over = Now {
            period: 9,
            clock: 0.0,
            score: ours,
            over: true,
        };
        let finals = live.games_now(&w, over);
        let table = live.table_now(&w, over, w.my_division());
        let others: Vec<(usize, [u16; 2])> = finals.iter().skip(1).map(|g| (g.fixture, g.score)).collect();
        w.full_time(live);
        for (i, score) in others {
            assert_eq!(w.fixtures[i].result.as_ref().unwrap().score, score);
        }
        // The table at the end is the table booked.
        let booked = w.table();
        assert_eq!(table.iter().map(|r| r.club).collect::<Vec<_>>(), booked);
        assert!(table.iter().all(|r| r.playing.is_none()));
    }
}
