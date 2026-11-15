//! The persistent game world: teams, players, schedule, standings, and the
//! weekly loop (play a round, train everyone, pay wages).

use serde::{Deserialize, Serialize};

use crate::assistant::{self, Reason};
use crate::command::{Command, CommandError};
use crate::engine::{
    BoxLine, GameResult, Lineup, LiveMatch, Orders, Pause, Reaction, Rules, Shout, simulate_game,
    simulate_recorded,
};
use crate::inbox::Desk;
use crate::player::{
    Attr, Attributes, Body, Contract, Hand, Personality, Player, PlayerId, Position,
};
use crate::replay::Recording;
use crate::rng::Rng;
use crate::scouting::Scouting;
use crate::season::{CareerLine, Draft, Pick, SeasonRecord, Series, initial_picks};
use crate::staff::{FrontOffice, Job, Staffer};
use crate::tactics::{
    DefenseScheme, MAX_TACTICS, OffenseSet, Press, TacticSlot, Tactics, train_playbook,
};
use crate::training::{Intensity, Schedule, TrainingFocus, train_week};

/// Index into `World::teams`.
pub type TeamId = usize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Team {
    pub id: TeamId,
    pub name: String,
    pub roster: Vec<PlayerId>,
    /// One to `MAX_TACTICS` trained tactics.
    pub playbook: Vec<TacticSlot>,
    /// Index into `playbook` of the tactic used in matches.
    pub active: usize,
    pub budget: i64,
    /// The weekly team training plan.
    pub schedule: Schedule,
    /// When set, the assistant coach redoes the schedule and individual
    /// plans every week from the latest numbers.
    pub delegate_training: bool,
    /// The assistant's reasoning the last time he planned the week.
    pub assistant_notes: Vec<Reason>,
    /// The reserve squad: under contract, outside the first-team roster.
    #[serde(default)]
    pub reserves: Vec<PlayerId>,
    /// First-team players who turn out for the reserves when they are not
    /// in the rotation.
    #[serde(default)]
    pub down: Vec<PlayerId>,
    /// 0..=1, how far the squad trusts its coach; see `dressing.rs`.
    #[serde(default = "trusting")]
    pub belief: f32,
    /// General manager and scouts. For the user's club the manager is the
    /// user; the figure upstairs is his assistant GM.
    pub office: FrontOffice,
    /// Standing rules for games the manager does not coach; see
    /// `instructions.rs`.
    #[serde(default)]
    pub instructions: crate::instructions::Instructions,
}

fn trusting() -> f32 {
    0.6
}

/// Season totals for a team and for its opponents against it.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct TeamStats {
    pub games: u16,
    pub own: BoxLine,
    pub opp: BoxLine,
}

impl Team {
    pub fn tactics(&self) -> &Tactics {
        &self.playbook[self.active].tactics
    }

    /// Copy playbook entry `from` into a new slot. The squad starts with a
    /// fraction of the familiarity, since most of the work carries over only
    /// if the system stays the same.
    pub fn add_tactic(&mut self, from: usize) -> Option<usize> {
        if self.playbook.len() >= MAX_TACTICS {
            return None;
        }
        let source = &self.playbook[from];
        let slot = TacticSlot::new(
            format!("Tactic {}", self.playbook.len() + 1),
            source.tactics.clone(),
            source.familiarity() * 0.5,
        );
        self.playbook.push(slot);
        Some(self.playbook.len() - 1)
    }

    /// How well the squad knows a system: the best any playbook entry
    /// offers, each counted as if it were switched to that system.
    pub fn familiarity_with(
        &self,
        offense: OffenseSet,
        defense: DefenseScheme,
        press: Press,
    ) -> f32 {
        self.playbook
            .iter()
            .map(|slot| slot.familiarity_with(offense, defense, press))
            .fold(0.0, f32::max)
    }

    pub fn remove_tactic(&mut self, i: usize) {
        if self.playbook.len() > 1 && i < self.playbook.len() {
            self.playbook.remove(i);
            if self.active >= i && self.active > 0 {
                self.active -= 1;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Standing {
    pub wins: u16,
    pub losses: u16,
    pub points_for: u32,
    pub points_against: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fixture {
    pub round: usize,
    pub home: TeamId,
    pub away: TeamId,
    pub result: Option<GameResult>,
    /// A playoff game: it decides a series, not the table.
    #[serde(default)]
    pub playoff: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct World {
    pub rng: Rng,
    pub rules: Rules,
    pub players: Vec<Player>,
    pub teams: Vec<Team>,
    pub fixtures: Vec<Fixture>,
    pub standings: Vec<Standing>,
    /// Season totals, indexed by `PlayerId`.
    pub season_stats: Vec<BoxLine>,
    pub games_played: Vec<u16>,
    /// Indexed by `TeamId`.
    pub team_stats: Vec<TeamStats>,
    /// Next round to play.
    pub round: usize,
    /// The user's scouting department.
    pub scouting: Scouting,
    /// Club whose games are recorded for the match viewer.
    pub watch: Option<TeamId>,
    /// Fixture index and recording of the watched club's latest game.
    pub replay: Option<(usize, Recording)>,
    /// Recordings of the watched club's earlier games.
    #[serde(default)]
    pub tapes: Vec<(usize, Recording)>,
    /// Seasons completed.
    #[serde(default)]
    pub season: u16,
    /// Every draft pick that exists, whoever holds it.
    #[serde(default)]
    pub picks: Vec<Pick>,
    /// The draft, while it is being held.
    #[serde(default)]
    pub draft: Option<Draft>,
    /// The most recent finished draft, for the record.
    #[serde(default)]
    pub last_draft: Option<Draft>,
    #[serde(default)]
    pub history: Vec<SeasonRecord>,
    /// This season's playoff series, semifinals first.
    #[serde(default)]
    pub playoffs: Vec<Series>,
    /// Set once the final is decided.
    #[serde(default)]
    pub champion: Option<TeamId>,
    /// Finished seasons of every player, indexed by `PlayerId`.
    #[serde(default)]
    pub careers: Vec<Vec<CareerLine>>,
    /// The reserve league: table, season totals by `PlayerId`, results.
    #[serde(default)]
    pub reserve_table: Vec<Standing>,
    #[serde(default)]
    pub reserve_stats: Vec<BoxLine>,
    #[serde(default)]
    pub reserve_games: Vec<u16>,
    #[serde(default)]
    pub reserve_log: Vec<crate::reserves::ReserveGame>,
    /// Coaches and scouts without a club.
    #[serde(default)]
    pub staff_pool: Vec<Staffer>,
    /// Days since the season began; see `inbox.rs` for the calendar.
    #[serde(default)]
    pub day: u32,
    /// The manager's mail and transfer business.
    #[serde(default)]
    pub desk: Desk,
    /// Every player action `World::apply` has carried out, in order; see
    /// `command.rs`. A live game is in it, once finished, as `PlayLive`.
    #[serde(default)]
    pub log: Vec<Command>,
    /// Arenas, sponsors and club money; see `finance.rs`.
    #[serde(default)]
    pub econ: crate::finance::Economy,
    /// Fame, fans, media work and the hall of fame; see `fame.rs`.
    #[serde(default)]
    pub fame: crate::fame::FameBook,
}

const CITIES: [&str; 20] = [
    "Helsinki", "Tampere", "Turku", "Oulu", "Lahti", "Kuopio", "Tallinn", "Riga", "Kaunas",
    "Vilnius", "Gdansk", "Malmo", "Bergen", "Aarhus", "Rostock", "Tartu", "Espoo", "Vantaa",
    "Pori", "Joensuu",
];
const NICKNAMES: [&str; 10] = [
    "Wolves", "Seagulls", "Pilots", "Kings", "Bears", "Storm", "Foxes", "Giants", "Comets",
    "Rangers",
];
pub(crate) const FIRST_NAMES: [&str; 24] = [
    "Mikko", "Antti", "Jonas", "Lukas", "Mateusz", "Erik", "Lars", "Tomas", "Marko", "Petteri",
    "Sasu", "Edon", "Karl", "Rihards", "Dario", "Nikola", "Jamal", "Tyrese", "Marcus", "Andre",
    "Olivier", "Pablo", "Sergio", "Elias",
];
pub(crate) const LAST_NAMES: [&str; 24] = [
    "Koponen",
    "Salin",
    "Murphy",
    "Jantunen",
    "Valtonen",
    "Kazlauskas",
    "Berzins",
    "Tamm",
    "Nowak",
    "Larsen",
    "Eriksson",
    "Hansen",
    "Petrovic",
    "Jovanovic",
    "Williams",
    "Carter",
    "Brooks",
    "Dubois",
    "Garcia",
    "Navarro",
    "Maxhuni",
    "Little",
    "Nkamhoua",
    "Madsen",
];

impl World {
    /// A fresh league with `n_teams` clubs of twelve players and a double
    /// round-robin schedule. `n_teams` must be even and at most 20.
    pub fn generate(seed: u64, n_teams: usize) -> World {
        assert!(n_teams >= 2 && n_teams.is_multiple_of(2) && n_teams <= CITIES.len());
        let mut rng = Rng::new(seed);
        let mut players: Vec<Player> = Vec::new();
        let mut teams = Vec::new();

        for (id, city) in CITIES.iter().enumerate().take(n_teams) {
            // Clubs differ, but not by so much that the better squad cannot
            // be outcoached.
            let quality = 9.1 + rng.f32() * 1.8;
            let mut roster = Vec::new();
            for slot in 0..12 {
                let pid = players.len() as PlayerId;
                players.push(generate_player(pid, slot, quality, &mut rng));
                roster.push(pid);
            }
            let refs: Vec<&Player> = roster.iter().map(|&p| &players[p as usize]).collect();
            let office = FrontOffice::generate(seed, id, &FIRST_NAMES, &LAST_NAMES);
            // The head coach runs what he believes in, and a better one
            // has it better drilled.
            let mut tactics = Tactics::auto(&refs);
            let mut known = 0.8;
            if let Some(head) = office.staffer(Job::HeadCoach) {
                (tactics.offense, tactics.defense) = (head.offense, head.defense);
                known = 0.65 + head.tactics as f32 * 0.015;
            }
            teams.push(Team {
                id,
                name: format!(
                    "{} {}",
                    city,
                    NICKNAMES[rng.range(0, NICKNAMES.len() as u32) as usize]
                ),
                playbook: vec![TacticSlot::new("Tactic 1", tactics, known)],
                active: 0,
                roster,
                budget: 1_000_000,
                schedule: Schedule::default(),
                delegate_training: true,
                assistant_notes: Vec::new(),
                reserves: Vec::new(),
                down: Vec::new(),
                belief: 0.6,
                office,
                instructions: Default::default(),
            });
        }

        for (team, squad) in
            teams
                .iter_mut()
                .zip(crate::reserves::seed(seed, n_teams, &mut players))
        {
            team.reserves = squad;
        }

        let mut world = World {
            rng,
            rules: Rules::fiba(),
            reserve_table: vec![Standing::default(); n_teams],
            reserve_stats: Vec::new(),
            reserve_games: Vec::new(),
            reserve_log: Vec::new(),
            season_stats: vec![BoxLine::default(); players.len()],
            games_played: vec![0; players.len()],
            scouting: Scouting::new(players.len()),
            watch: None,
            replay: None,
            season: 0,
            picks: initial_picks(n_teams),
            draft: None,
            last_draft: None,
            history: Vec::new(),
            playoffs: Vec::new(),
            champion: None,
            careers: Vec::new(),
            tapes: Vec::new(),
            staff_pool: crate::staff::market(seed, &FIRST_NAMES, &LAST_NAMES),
            day: 0,
            desk: Desk::default(),
            log: Vec::new(),
            team_stats: vec![TeamStats::default(); n_teams],
            players,
            standings: vec![Standing::default(); n_teams],
            fixtures: schedule(n_teams),
            teams,
            round: 0,
            econ: Default::default(),
            fame: Default::default(),
        };
        world.econ = crate::finance::Economy::generate(seed, &world);
        world.ensure_fame();
        world.sign_first_deals();
        world
    }

    /// Every team's totals added together, for league averages. `games`
    /// counts team games, so per-game figures compare directly.
    pub fn league_totals(&self) -> TeamStats {
        let mut total = TeamStats::default();
        for s in &self.team_stats {
            total.games += s.games;
            total.own.add(&s.own);
            total.opp.add(&s.opp);
        }
        total
    }

    /// The club a player is registered with.
    pub fn team_of(&self, p: PlayerId) -> Option<TeamId> {
        self.teams
            .iter()
            .position(|t| t.roster.contains(&p) || t.reserves.contains(&p))
    }

    pub fn rounds(&self) -> usize {
        self.fixtures.last().map_or(0, |f| f.round + 1)
    }

    pub fn season_over(&self) -> bool {
        self.round >= self.rounds()
    }

    /// Team ids sorted by wins, then point difference.
    pub fn table(&self) -> Vec<TeamId> {
        let mut ids: Vec<TeamId> = (0..self.teams.len()).collect();
        ids.sort_by_key(|&t| {
            let s = &self.standings[t];
            (
                std::cmp::Reverse(s.wins),
                std::cmp::Reverse(s.points_for as i64 - s.points_against as i64),
            )
        });
        ids
    }

    /// Play every fixture of the current round, then run a week of training
    /// and wages. Returns the indices of the fixtures played.
    pub fn play_round(&mut self) -> Vec<usize> {
        let (played, seeds) = self.draw_round();
        self.complete_round(played, seeds, None)
    }

    /// Start this round with `team`'s game played live: the caller steps
    /// `LiveRound::game` to the end and hands it back to `full_time`. `None`
    /// if the team has no game left. The game is seeded exactly as
    /// `play_round` would seed it.
    pub fn kickoff(&mut self, team: TeamId) -> Option<LiveRound> {
        let fixture = (0..self.fixtures.len()).find(|&i| {
            let f = &self.fixtures[i];
            f.round == self.round && (f.home == team || f.away == team)
        })?;
        let (played, seeds) = self.draw_round();
        let n = played.iter().position(|&i| i == fixture)?;
        let f = &self.fixtures[fixture];
        let side = if f.home == team { 0 } else { 1 };
        let game = LiveMatch::new(
            &lineup(&self.teams[f.home], &self.players),
            &lineup(&self.teams[f.away], &self.players),
            &self.rules,
            Rng::new(seeds[n]),
            Some(side),
        );
        Some(LiveRound {
            fixture,
            side,
            game,
            steps: 0,
            inputs: Vec::new(),
            played,
            seeds,
        })
    }

    /// Book a finished live game and play the rest of its round. What the
    /// coach did is appended to `log` as `Command::PlayLive`.
    pub fn full_time(&mut self, live: LiveRound) -> Vec<usize> {
        self.log.push(Command::PlayLive {
            inputs: live.inputs.clone(),
        });
        self.book(live)
    }

    /// Replay `Command::PlayLive`: the user's game with `inputs` applied
    /// at the step counts they were recorded at, then booked like
    /// `full_time` but without logging it.
    pub(crate) fn replay_live(
        &mut self,
        inputs: &[(u32, LiveInput)],
    ) -> Result<Vec<usize>, CommandError> {
        let me = self.watch.ok_or(CommandError::NoClub)?;
        if self.season_over() {
            return Err(CommandError::Refused("The season is over.".into()));
        }
        let mut live = self
            .kickoff(me)
            .ok_or_else(|| CommandError::Refused("There is no game for you this round.".into()))?;
        for (at, input) in inputs {
            while live.steps < *at && !live.game.is_over() {
                live.step();
            }
            live.input(input.clone());
        }
        while !live.game.is_over() {
            live.step();
        }
        Ok(self.book(live))
    }

    fn book(&mut self, live: LiveRound) -> Vec<usize> {
        let (result, recording) = live.game.finish();
        self.complete_round(
            live.played,
            live.seeds,
            Some((live.fixture, result, recording)),
        )
    }

    /// True if fixture `i` was recorded and can be watched again.
    pub fn has_tape(&self, i: usize) -> bool {
        self.replay.as_ref().is_some_and(|r| r.0 == i) || self.tapes.iter().any(|t| t.0 == i)
    }

    /// Make fixture `i` the game in `replay`, if it was recorded.
    pub fn load_tape(&mut self, i: usize) {
        if let Some(n) = self.tapes.iter().position(|t| t.0 == i) {
            let tape = self.tapes.remove(n);
            if let Some(old) = self.replay.replace(tape) {
                self.tapes.push(old);
            }
        }
    }

    /// This round's fixtures and a seed for each. Every game gets its own
    /// seed up front, so the games are independent and the result is the
    /// same however many threads simulate them.
    fn draw_round(&mut self) -> (Vec<usize>, Vec<u64>) {
        let played: Vec<usize> = (0..self.fixtures.len())
            .filter(|&i| self.fixtures[i].round == self.round)
            .collect();
        let seeds = played.iter().map(|_| self.rng.next_u64()).collect();
        (played, seeds)
    }

    fn complete_round(
        &mut self,
        played: Vec<usize>,
        seeds: Vec<u64>,
        live: Option<(usize, GameResult, Recording)>,
    ) -> Vec<usize> {
        let skip = live.as_ref().map(|l| l.0);
        self.econ_before_games(&played);
        let simulate = |(&i, &seed): (&usize, &u64)| {
            if Some(i) == skip {
                return None;
            }
            let f = &self.fixtures[i];
            let home = lineup(&self.teams[f.home], &self.players);
            let away = lineup(&self.teams[f.away], &self.players);
            let rng = &mut Rng::new(seed);
            // Recording does not change the result, only keeps it.
            Some(
                if self.watch == Some(f.home) || self.watch == Some(f.away) {
                    let (result, recording) = simulate_recorded(&home, &away, &self.rules, rng);
                    (result, Some(recording))
                } else {
                    (simulate_game(&home, &away, &self.rules, rng), None)
                },
            )
        };
        #[cfg(feature = "parallel")]
        let results: Vec<Option<(GameResult, Option<Recording>)>> = {
            use rayon::prelude::*;
            played.par_iter().zip(&seeds).map(simulate).collect()
        };
        #[cfg(not(feature = "parallel"))]
        let results: Vec<Option<(GameResult, Option<Recording>)>> =
            played.iter().zip(&seeds).map(simulate).collect();
        let mut live = live.map(|(_, result, recording)| (result, Some(recording)));

        for (&i, result) in played.iter().zip(results) {
            let Some((result, recording)) = result.or_else(|| live.take()) else {
                continue;
            };
            if let Some(recording) = recording
                && let Some(old) = self.replay.replace((i, recording))
            {
                self.tapes.push(old);
            }
            let (home, away) = (self.fixtures[i].home, self.fixtures[i].away);
            // Playoff games are kept, but the table and the season's
            // averages are the regular season's.
            let counted = !self.fixtures[i].playoff;

            for (side, team) in [home, away].into_iter().enumerate().filter(|_| counted) {
                let s = &mut self.standings[team];
                s.points_for += result.score[side] as u32;
                s.points_against += result.score[1 - side] as u32;
                if result.score[side] > result.score[1 - side] {
                    s.wins += 1;
                } else {
                    s.losses += 1;
                }
                self.team_stats[team].games += 1;
                for (pid, line) in &result.boxes[side] {
                    self.team_stats[team].own.add(line);
                    self.team_stats[[away, home][side]].opp.add(line);
                    self.season_stats[*pid as usize].add(line);
                    if line.secs > 0 {
                        self.games_played[*pid as usize] += 1;
                        if self.scouting.is_shortlisted(*pid) {
                            let opponent = [away, home][side];
                            self.scouting.watch(
                                *pid,
                                self.round,
                                i,
                                opponent,
                                side == 0,
                                line,
                                self.rules.foul_limit,
                                self.watch.map_or(10, |t| self.teams[t].office.scouting()),
                            );
                            self.scouting
                                .reveal_signatures(*pid, &self.players[*pid as usize].signatures);
                        }
                    }
                }
            }
            self.judge_coaches(i, &result);
            self.after_game(i, &result);
            self.fixtures[i].result = Some(result);
        }
        self.reserve_round(&played, &seeds);
        self.health_week();
        self.dressing_week();

        let before: Vec<(PlayerId, f32)> = self.watch.map_or(Vec::new(), |t| {
            self.teams[t]
                .roster
                .iter()
                .map(|&p| (p, self.players[p as usize].overall()))
                .collect()
        });
        for team in 0..self.teams.len() {
            if self.teams[team].delegate_training {
                assistant::run(self, team);
            }
        }
        // Media work and events take some of the week's training.
        let media = self.media_factors();
        let mut paid = Vec::new();
        for team in &mut self.teams {
            let gym = self
                .econ
                .books
                .get(team.id)
                .map_or(1.0, |b| b.facilities.training_mult());
            let (teaching, wear) = (team.office.teaching() * gym, team.office.wear());
            for &p in team.roster.iter().chain(&team.reserves) {
                let player = &mut self.players[p as usize];
                // The injured do their rehab; the unhappy go through the
                // motions.
                if !player.fit() {
                    continue;
                }
                let keen = (0.9 + 0.2 * player.morale) * media[p as usize];
                train_week(player, &team.schedule, &mut self.rng, teaching * keen, wear);
            }
            train_playbook(
                &mut team.playbook,
                team.active,
                team.schedule.familiarity_gain() * team.office.drilling(),
            );
            let wages: i64 = team
                .roster
                .iter()
                .chain(&team.reserves)
                .map(|&p| self.players[p as usize].contract.salary as i64)
                .sum();
            let staff = team.office.wages() / 52;
            paid.push((team.id, (wages + team.office.wages()) / 52 - staff, staff));
        }
        for (team, players, staff) in paid {
            self.transact(team, -players, crate::finance::Cat::PlayerWages);
            self.transact(team, -staff, crate::finance::Cat::StaffWages);
        }
        self.round += 1;
        self.playoffs_step(&played);
        self.after_round(&played, &before);
        self.econ_after_round();
        self.fame_week(&played);
        self.send_off(&played);
        played
    }
}

/// A round whose other games wait while one is played live.
pub struct LiveRound {
    /// Index into `World::fixtures`.
    pub fixture: usize,
    /// The side the user coaches, 0 = home.
    pub side: usize,
    /// Read it as you like; change it only through `step` and `input`,
    /// which is what `World::full_time` logs.
    pub game: LiveMatch,
    /// Calls to `step` so far.
    steps: u32,
    /// What the coach did, by the number of steps taken before it.
    inputs: Vec<(u32, LiveInput)>,
    played: Vec<usize>,
    seeds: Vec<u64>,
}

/// Something the coach does during a live game.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LiveInput {
    /// Ask for a timeout at the next dead ball; again to wave it off.
    Timeout,
    AutoSubs(bool),
    /// Bring `on` in for `off`; the assistant stops making changes.
    Swap {
        off: PlayerId,
        on: PlayerId,
    },
    CancelSwap(usize),
    Shout {
        shout: Shout,
        target: Option<PlayerId>,
    },
    /// New orders from the next dead ball; `familiarity` is how well the
    /// squad knows the system, 0..=1.
    Orders {
        orders: Orders,
        familiarity: f32,
    },
}

impl LiveRound {
    /// Advance the game, see `LiveMatch::step`.
    pub fn step(&mut self) -> Option<Pause> {
        self.steps += 1;
        self.game.step()
    }

    /// Do `input` for the user's side and record it. A shout returns how
    /// each player took it; everything else returns nothing.
    pub fn input(&mut self, input: LiveInput) -> Vec<(PlayerId, Reaction)> {
        let side = self.side;
        let mut said = Vec::new();
        match input.clone() {
            LiveInput::Timeout => self.game.call_timeout(side),
            LiveInput::AutoSubs(on) => self.game.set_auto_subs(side, on),
            LiveInput::Swap { off, on } => {
                self.game.set_auto_subs(side, false);
                self.game.swap(side, off, on);
            }
            LiveInput::CancelSwap(n) => self.game.cancel_swap(side, n),
            LiveInput::Shout { shout, target } => said = self.game.shout(side, shout, target),
            LiveInput::Orders {
                orders,
                familiarity,
            } => self.game.set_orders(side, orders, familiarity),
        }
        self.inputs.push((self.steps, input));
        said
    }
}

/// Resolve a team's rotation into player references. Slots pointing at
/// players no longer on the roster are skipped by the caller's tactics UI;
/// here they would panic, which is what we want during development.
pub fn lineup<'a>(team: &'a Team, players: &'a [Player]) -> Lineup<'a> {
    let slot = &team.playbook[team.active];
    Lineup {
        tactics: &slot.tactics,
        players: slot
            .tactics
            .rotation
            .iter()
            .map(|s| &players[s.player as usize])
            .collect(),
        familiarity: slot.familiarity(),
        belief: team.belief,
        coach: team.office.rating(Job::HeadCoach),
    }
}

/// Double round-robin by the circle method; second half mirrors home/away.
pub(crate) fn schedule(n: usize) -> Vec<Fixture> {
    let mut ids: Vec<TeamId> = (0..n).collect();
    let mut fixtures = Vec::new();
    for round in 0..n - 1 {
        for i in 0..n / 2 {
            let (a, b) = (ids[i], ids[n - 1 - i]);
            let (home, away) = if round % 2 == 0 { (a, b) } else { (b, a) };
            fixtures.push(Fixture {
                round,
                home,
                away,
                result: None,
                playoff: false,
            });
        }
        ids[1..].rotate_right(1);
    }
    let mirrored: Vec<Fixture> = fixtures
        .iter()
        .map(|f| Fixture {
            round: f.round + n - 1,
            home: f.away,
            away: f.home,
            result: None,
            playoff: false,
        })
        .collect();
    fixtures.extend(mirrored);
    fixtures
}

/// How strongly an attribute belongs to guards (+) or bigs (-).
fn perimeter_lean(a: Attr) -> f32 {
    match a {
        Attr::Handling | Attr::Passing | Attr::ThreePoint => 3.0,
        Attr::Speed | Attr::PerimeterD => 2.5,
        Attr::Steal => 2.0,
        Attr::FreeThrow => 1.5,
        Attr::MidRange => 1.0,
        Attr::CloseShot => -1.5,
        Attr::Strength => -2.5,
        Attr::PostMoves | Attr::OffRebound | Attr::DefRebound | Attr::InteriorD => -3.0,
        Attr::Block => -3.5,
        _ => 0.0,
    }
}

/// Measurements that hang together: arms a little longer than height, reach
/// following height and arm length, mass following frame and strength.
fn generate_body(base_height: f32, guardness: f32, strength: f32, rng: &mut Rng) -> Body {
    let height_cm = base_height + rng.f32() * 10.0 - 5.0;
    // Two rolls averaged: most players near +7 cm, a few freakishly long.
    let ape = 1.0 + (rng.f32() + rng.f32()) * 6.5;
    let wingspan_cm = height_cm + ape;
    let standing_reach_cm = height_cm * 1.30 + ape * 0.5 + rng.f32() * 5.0 - 2.5;
    let bmi = 23.3 - guardness * 0.9 + (strength - 10.0) * 0.18 + rng.f32() * 1.6 - 0.8;
    let metres = height_cm / 100.0;
    Body {
        height_cm,
        weight_kg: bmi * metres * metres,
        wingspan_cm,
        standing_reach_cm,
        shoulder_width_cm: height_cm * 0.243 + (strength - 10.0) * 0.25 + rng.f32() * 2.0 - 1.0,
        hand_length_cm: height_cm * 0.105 + ape * 0.06 + rng.f32() * 1.2 - 0.6,
        hand: if rng.chance(0.1) {
            Hand::Left
        } else {
            Hand::Right
        },
    }
}

pub(crate) fn generate_player(
    id: PlayerId,
    slot: usize,
    team_quality: f32,
    rng: &mut Rng,
) -> Player {
    use Position::*;
    const SLOTS: [Position; 12] = [PG, SG, SF, PF, C, PG, SG, SF, PF, C, SG, PF];
    let position = SLOTS[slot];
    let (guardness, base_height) = match position {
        PG => (1.0, 188),
        SG => (0.7, 195),
        SF => (0.0, 201),
        PF => (-0.7, 206),
        C => (-1.0, 211),
    };

    let bench = if slot >= 5 { 1.5 } else { 0.0 };
    let quality = team_quality + (rng.f32() - 0.5) * 5.0 - bench;
    let mut attrs = Attributes([0.0; Attr::COUNT]);
    // Footwork is rolled below from the body, not from this stream, so the
    // other twenty attributes come out exactly as they did before it existed.
    for a in Attr::ALL.into_iter().filter(|&a| a != Attr::Footwork) {
        attrs[a] =
            (quality + guardness * perimeter_lean(a) + (rng.f32() - 0.5) * 4.0).clamp(1.0, 20.0);
    }

    let age = 18 + rng.range(0, 17) as u8;
    // Two rolls averaged, so most traits are middling and extremes are rare.
    let mut trait_roll = || 1 + ((rng.range(0, 20) + rng.range(0, 20)) / 2) as u8;
    let personality = Personality {
        professionalism: trait_roll(),
        ambition: trait_roll(),
        determination: trait_roll(),
        temperament: trait_roll(),
        leadership: trait_roll(),
        consistency: trait_roll(),
        loyalty: trait_roll(),
        presence: crate::media::presence_roll(id),
    };
    let body = generate_body(base_height as f32, guardness, attrs[Attr::Strength], rng);
    attrs[Attr::Footwork] = crate::player::roll_footwork(id, &attrs, &body);
    let mut player = Player {
        id,
        name: format!(
            "{} {}",
            FIRST_NAMES[rng.range(0, FIRST_NAMES.len() as u32) as usize],
            LAST_NAMES[rng.range(0, LAST_NAMES.len() as u32) as usize]
        ),
        age,
        body,
        position,
        attrs,
        personality,
        potential: 0.0,
        training: TrainingFocus::Balanced,
        intensity: Intensity::Normal,
        condition: 1.0,
        contract: Contract {
            salary: 0,
            years_left: 1 + rng.range(0, 4) as u8,
        },
        familiar: crate::health::familiarity(position, rng),
        morale: 0.5,
        sharp: 1.0,
        complained: false,
        proneness: 1 + ((rng.range(0, 20) + rng.range(0, 20)) / 2) as u8,
        recovery: 1 + ((rng.range(0, 20) + rng.range(0, 20)) / 2) as u8,
        injury: None,
        injuries: Vec::new(),
        renown: Default::default(),
        signatures: Vec::new(),
        signatures_rolled: false,
    };
    let overall = player.overall();
    let upside = match age {
        ..=23 => rng.f32() * 4.0,
        24..=27 => rng.f32() * 1.5,
        _ => 0.0,
    };
    player.potential = (overall + upside).min(20.0);
    player.contract.salary = (overall * overall * overall * 60.0) as u32;
    player.assign_signatures();
    player
}

#[cfg(test)]
mod tests {
    use super::*;

    fn season(seed: u64) -> World {
        let mut w = World::generate(seed, 8);
        while !w.season_over() {
            w.play_round();
        }
        w
    }

    #[test]
    fn same_seed_same_season() {
        let (a, b) = (season(7), season(7));
        for (fa, fb) in a.fixtures.iter().zip(&b.fixtures) {
            assert_eq!(
                fa.result.as_ref().unwrap().score,
                fb.result.as_ref().unwrap().score
            );
        }
    }

    /// A live game nobody touches is the game the round would have had,
    /// apart from the idle bench never calling a timeout.
    #[test]
    fn live_round_books_like_a_simulated_one() {
        let mut w = World::generate(3, 8);
        let mut live = w.kickoff(0).expect("team 0 plays in round 0");
        let mut stops = 0;
        while !live.game.is_over() {
            if live.step().is_some() {
                stops += 1;
            }
        }
        assert!(stops >= 4, "four periods end");
        let fixture = live.fixture;
        let score = live.game.result().score;
        let played = w.full_time(live);
        assert_eq!(played.len(), 4);
        assert_eq!(w.round, 1);
        assert_eq!(w.fixtures[fixture].result.as_ref().unwrap().score, score);
        assert_eq!(w.replay.as_ref().unwrap().0, fixture);
        let games: u16 = w.standings.iter().map(|s| s.wins + s.losses).sum();
        assert_eq!(games, 8);
    }

    #[test]
    fn the_bench_can_run_a_live_game() {
        use crate::engine::{Orders, Pause, Shout};
        use crate::replay::EventKind;

        let mut w = World::generate(3, 8);
        let mut live = w.kickoff(0).unwrap();
        let side = live.side;
        live.input(LiveInput::AutoSubs(false));
        let (mut timeouts, mut steps, mut sent) = (0, 0u32, 0);
        while !live.game.is_over() {
            steps += 1;
            if steps == 500 {
                // Bench player for the first man on the floor.
                let players = live.game.players(side);
                let off = players.iter().find(|p| p.on_court).unwrap().id;
                let on = players.iter().find(|p| !p.on_court).unwrap().id;
                live.input(LiveInput::Swap { off, on });
                assert_eq!(live.game.queued(side), vec![(off, on)]);
                sent = on;
                live.input(LiveInput::Timeout);
                let said = live.input(LiveInput::Shout {
                    shout: Shout::DemandMore,
                    target: None,
                });
                assert_eq!(said.len(), 5);
                let mut orders = live.game.orders(side);
                orders.pace = 18;
                live.input(LiveInput::Orders {
                    orders,
                    familiarity: 0.5,
                });
            }
            if steps.is_multiple_of(3000) {
                live.input(LiveInput::Shout {
                    shout: Shout::Focus,
                    target: None,
                });
            }
            if live.step() == Some(Pause::Timeout(side)) {
                timeouts += 1;
                assert_eq!(live.game.orders(side).pace, 18);
            }
        }
        assert_eq!(timeouts, 1);
        assert_eq!(live.game.orders(side), {
            let mut o = Orders::of(w.teams[0].tactics());
            o.pace = 18;
            o
        });
        let events = &live.game.recording().events;
        let count = |f: &dyn Fn(&EventKind) -> bool| events.iter().filter(|e| f(&e.kind)).count();
        assert_eq!(
            count(&|k| matches!(k, EventKind::Shout { .. })),
            1 + steps as usize / 3000
        );
        assert!(count(&|k| matches!(k, EventKind::Substitution { .. })) >= 1);
        // The man sent on played; with the assistant off nobody else came
        // in unless a foul-out forced it.
        let players = live.game.players(side);
        assert!(players.iter().find(|p| p.id == sent).unwrap().line.secs > 600);
        let out = players.iter().filter(|p| p.fouled_out).count();
        let used = players.iter().filter(|p| p.line.secs > 0).count();
        assert!(
            (6..=6 + out).contains(&used),
            "{used} used, {out} fouled out"
        );
        w.full_time(live);
    }

    #[test]
    fn schedule_is_complete_and_balanced() {
        let w = season(1);
        let regular = w.fixtures.iter().filter(|f| !f.playoff).count();
        assert_eq!(regular, 8 * 7);
        for s in &w.standings {
            assert_eq!(s.wins + s.losses, 14, "playoff games stay out of the table");
        }
        // Two semifinals and a final, each best of three.
        let playoff = w.fixtures.len() - regular;
        assert!((6..=9).contains(&playoff), "{playoff} playoff games");
        assert_eq!(w.playoffs.len(), 3);
        let last = w.playoffs.last().unwrap();
        assert!(last.is_final && w.champion == last.winner() && w.champion.is_some());
        assert!(w.table()[..4].contains(&w.champion.unwrap()));
        // Plus/minus over a game sums to the margin times five.
        let r = w.fixtures[0].result.as_ref().unwrap();
        let sum: i32 = r.boxes[0].iter().map(|b| b.1.pm as i32).sum();
        assert_eq!(sum, 5 * (r.score[0] as i32 - r.score[1] as i32));
        assert!(!w.mvp_race().is_empty());
    }

    #[test]
    fn box_scores_add_up() {
        let w = season(3);
        for f in &w.fixtures {
            let r = f.result.as_ref().unwrap();
            for side in 0..2 {
                let pts: u16 = r.boxes[side].iter().map(|(_, l)| l.pts).sum();
                assert_eq!(pts, r.score[side]);
                assert_eq!(
                    r.periods.iter().map(|p| p[side]).sum::<u16>(),
                    r.score[side]
                );
            }
            assert_ne!(r.score[0], r.score[1]);
        }
    }

    #[test]
    fn unfamiliar_tactic_hurts() {
        let w = World::generate(5, 2);
        let wins = |familiarity: f32| {
            let mut home = lineup(&w.teams[0], &w.players);
            home.familiarity = familiarity;
            let away = lineup(&w.teams[1], &w.players);
            (0..400)
                .filter(|&seed| {
                    let r = simulate_game(&home, &away, &w.rules, &mut Rng::new(seed));
                    r.score[0] > r.score[1]
                })
                .count()
        };
        let (fluent, raw) = (wins(1.0), wins(0.0));
        assert!(
            fluent > raw + 40,
            "fluent {fluent} vs untrained {raw} wins of 400"
        );
    }

    /// What the coach draws and tells his players reaches the floor.
    #[test]
    fn the_board_and_the_briefs_are_played() {
        use crate::tactics::{Board, Brief};
        let w = World::generate(5, 2);
        let play = |change: &dyn Fn(&mut Tactics)| {
            let mut t = w.teams[0].tactics().clone();
            change(&mut t);
            let mut home = lineup(&w.teams[0], &w.players);
            home.tactics = &t;
            let away = lineup(&w.teams[1], &w.players);
            let who = t.rotation[0].player;
            let mut line = BoxLine::default();
            let mut all = BoxLine::default();
            for seed in 0..60 {
                let r = simulate_game(&home, &away, &w.rules, &mut Rng::new(seed));
                for (p, l) in &r.boxes[0] {
                    all.add(l);
                    if *p == who {
                        line.add(l);
                    }
                }
            }
            (line, all)
        };
        let shoot = play(&|t| t.rotation[0].brief = Brief::Shoot).0;
        let pass = play(&|t| t.rotation[0].brief = Brief::Pass).0;
        assert!(
            shoot.fga as f32 > pass.fga as f32 * 1.15,
            "told to shoot {} attempts, told to pass {}",
            shoot.fga,
            pass.fga
        );
        // Everybody drawn in close: far fewer threes go up.
        let stock = play(&|_| {}).1;
        let wide = play(&|t| {
            let mut b = Board::stock(t.offense);
            b.spots = [
                crate::court::v(0.0, 6.4),
                crate::court::v(3.6, 4.6),
                crate::court::v(-3.6, 4.6),
                crate::court::v(3.0, 1.6),
                crate::court::v(-3.0, 1.6),
            ];
            t.draw(b);
            assert!(t.board.is_some());
        })
        .1;
        assert!(
            (wide.tpa as f32) < stock.tpa as f32 * 0.8,
            "drawn in tight {} threes, stock {}",
            wide.tpa,
            stock.tpa
        );
        // A run to the rim is a cut, and cuts get layups.
        let cuts = play(&|t| {
            let mut b = Board::stock(t.offense);
            b.runs[2] = Some(crate::court::v(1.2, 2.2));
            b.runs[3] = Some(crate::court::v(-1.2, 2.2));
            t.draw(b);
        })
        .1;
        assert_ne!(
            (cuts.fga, cuts.tpa, cuts.pts),
            (stock.fga, stock.tpa, stock.pts),
            "drawn runs change nothing"
        );
        eprintln!(
            "stock {} pts {} threes; wide {} pts {} threes; cuts {} pts {} threes",
            stock.pts, stock.tpa, wide.pts, wide.tpa, cuts.pts, cuts.tpa
        );
    }

    /// A guard used at another position is worse there at first and
    /// learns it by being played there.
    #[test]
    fn a_position_is_learned_by_playing_it() {
        let mut w = World::generate(5, 4);
        let slot = w.teams[0].playbook[0].tactics.rotation[0].clone();
        let p = &w.players[slot.player as usize];
        let other = Position::ALL
            .into_iter()
            .find(|&x| x != p.position && p.knows(x) < 0.9)
            .unwrap();
        let before = p.knows(other);
        w.teams[0].playbook[0].tactics.rotation[0].position = Some(other);
        for _ in 0..6 {
            w.play_round();
        }
        let after = w.players[slot.player as usize].knows(other);
        assert!(after > before + 0.08, "{before} -> {after}");
    }

    #[test]
    fn recording_does_not_change_the_game() {
        let w = World::generate(9, 2);
        let home = lineup(&w.teams[0], &w.players);
        let away = lineup(&w.teams[1], &w.players);
        let plain = simulate_game(&home, &away, &w.rules, &mut Rng::new(4));
        let (seen, rec) = simulate_recorded(&home, &away, &w.rules, &mut Rng::new(4));
        assert_eq!(plain.score, seen.score);
        assert_eq!(plain.periods, seen.periods);
        assert_eq!(plain.possessions, seen.possessions);

        // The recording covers the game, on the court, in time order.
        assert!(rec.duration() > 2400.0);
        assert!(rec.frames.windows(2).all(|f| f[0].time <= f[1].time));
        for f in &rec.frames {
            for p in f.pos.iter().flatten() {
                assert!((0.0..=28.0).contains(&p.x) && (0.0..=15.0).contains(&p.y));
            }
        }
        // Every point on the scoreboard is in the event log.
        let logged: u16 = rec
            .events
            .iter()
            .map(|e| match &e.kind {
                crate::replay::EventKind::Shot {
                    made: true, kind, ..
                } => kind.points(),
                crate::replay::EventKind::FreeThrows { made, .. } => *made as u16,
                _ => 0,
            })
            .sum();
        assert_eq!(logged, seen.score[0] + seen.score[1]);
    }

    /// The man guarding a ball handler goes over a screen or under it, and
    /// the handler's day is different: under, he gets the pull-up and not the
    /// lane; over, he is contested at the shot and gets the lane.
    #[test]
    fn going_under_gives_the_pull_up_and_going_over_gives_the_lane() {
        use crate::replay::EventKind;
        use crate::tactics::{DefenseScheme, OffenseSet};
        let w = World::generate(9, 2);
        let mut offense = w.teams[0].tactics().clone();
        offense.offense = OffenseSet::PickAndRoll;
        let mut home = lineup(&w.teams[0], &w.players);
        home.tactics = &offense;
        let mut defense = w.teams[1].tactics().clone();
        defense.defense = DefenseScheme::ManToMan;
        let mut away = lineup(&w.teams[1], &w.players);
        away.tactics = &defense;
        // Screens, handler's shots within five seconds, their summed contest,
        // and handlers who got past their man, for over and for under.
        let mut by = [[0.0f32; 4]; 2];
        for seed in 0..30 {
            let (_, rec) = simulate_recorded(&home, &away, &w.rules, &mut Rng::new(seed));
            let mut screened: Option<(u32, f32, usize)> = None;
            for e in rec.events.iter().filter(|e| e.offense == 0) {
                match &e.kind {
                    EventKind::Screen { handler, .. } => {
                        screened = rec
                            .screens
                            .iter()
                            .find(|x| x.0 == e.time)
                            .map(|x| (*handler, e.time, x.1 as usize));
                        if let Some((_, _, c)) = screened {
                            by[c][0] += 1.0;
                        }
                    }
                    EventKind::Shot { who, contest, .. } => {
                        if let Some((h, at, c)) = screened.take()
                            && h == *who
                            && e.time - at < 5.0
                        {
                            by[c][1] += 1.0;
                            by[c][2] += *contest;
                        }
                    }
                    EventKind::Beaten { who, .. } => {
                        if let Some((h, at, c)) = screened
                            && h == *who
                            && e.time - at < 1.0
                        {
                            by[c][3] += 1.0;
                        }
                    }
                    _ => {}
                }
            }
        }
        let [over, under] = by;
        assert!(over[0] > 200.0 && under[0] > 200.0, "{over:?} {under:?}");
        // Open pull-ups under, contested ones over.
        assert!(
            under[2] / under[1] < over[2] / over[1] - 0.2,
            "contest under {} over {}",
            under[2] / under[1],
            over[2] / over[1]
        );
        // More of them shoot when it is open, and the lane is only there over it.
        assert!(under[1] / under[0] > over[1] / over[0]);
        assert!(over[3] / over[0] > under[3] / under[0] + 0.1);
    }

    /// Taking charges is a few players' specialty: over a league most men
    /// have no urge to, a handful have all of it.
    #[test]
    fn a_few_players_are_known_for_taking_charges() {
        use crate::engine::contact::charge_urge;
        use crate::player::Attr;
        let (mut urges, mut all) = (0, 0);
        let mut high = 0;
        for seed in 0..6 {
            let w = World::generate(100 + seed, 12);
            for t in &w.teams {
                for s in t.tactics().rotation.iter().take(10) {
                    let p = &w.players[s.player as usize];
                    let a = |x: Attr| p.attrs[x];
                    let skill = (0.3 * a(Attr::Iq)
                        + 0.2 * a(Attr::Composure)
                        + 0.2 * p.personality.determination as f32
                        + 0.15 * a(Attr::WorkRate)
                        + 0.15 * a(Attr::PerimeterD))
                        / 20.0;
                    let u = charge_urge(skill, &p.personality);
                    all += 1;
                    urges += (u > 0.5) as u32;
                    high += (u > 0.95) as u32;
                }
            }
        }
        // Under a third have a real urge, and some have all of it.
        assert!(urges * 3 < all, "{urges} of {all}");
        assert!(high * 100 > all * 3, "{high} of {all}");
    }

    /// What the frames say players are doing hangs together with the
    /// events and with the bodies.
    #[test]
    fn what_players_are_doing_is_coherent() {
        use crate::engine::contact::radius;
        use crate::replay::{Action, EventKind, Pose};
        let w = World::generate(9, 2);
        let home = lineup(&w.teams[0], &w.players);
        let away = lineup(&w.teams[1], &w.players);
        let mut airborne = 0;
        let mut shots_seen = 0;
        for seed in 0..3 {
            let (res, rec) = simulate_recorded(&home, &away, &w.rules, &mut Rng::new(seed));
            let player = |f: &crate::replay::Frame, s: usize, k: usize| {
                &w.players[res.boxes[s][f.who[s][k] as usize].0 as usize]
            };
            for f in &rec.frames {
                for s in 0..2 {
                    for k in 0..5 {
                        let (pose, p) = (f.pose[s][k], player(f, s, k));
                        if pose.lift_cm > 0 {
                            airborne += 1;
                            assert!(pose.action.is_jump(), "{:?}", pose.action);
                            assert!(
                                pose.lift()
                                    <= Pose::max_lift(p.attrs[crate::player::Attr::Vertical])
                                        + 1e-3
                            );
                        }
                        if pose.action == Action::Screening {
                            assert_eq!(pose.lift_cm, 0);
                        }
                        if let Some((s2, k2)) = pose.touching() {
                            let q = player(f, s2 as usize, k2 as usize);
                            let reach = radius(&p.body) + radius(&q.body);
                            let d = f.pos[s][k].dist(f.pos[s2 as usize][k2 as usize]);
                            assert!(d <= reach + 0.05, "{d} > {reach}");
                        }
                    }
                }
            }
            // Every shot has its shooter up for it around the time it is taken.
            for e in &rec.events {
                let EventKind::Shot { who, .. } = &e.kind else {
                    continue;
                };
                shots_seen += 1;
                let tagged = rec
                    .frames
                    .iter()
                    .filter(|f| (f.time - e.time).abs() <= 0.7)
                    .any(|f| {
                        let s = e.offense as usize;
                        (0..5).any(|k| {
                            player(f, s, k).id == *who && f.pose[s][k].action == Action::Shooting
                        })
                    });
                assert!(tagged, "no shooting pose near the shot at {}", e.time);
            }
        }
        assert!(
            airborne > 100 && shots_seen > 100,
            "{airborne} {shots_seen}"
        );
    }

    /// Fouls carry what they were for, offensive ones are turnovers, the
    /// crew's whistle is drawn once, and the frames show it.
    #[test]
    fn fouls_have_kinds_and_the_replay_shows_the_whistle() {
        use crate::replay::{Action, EventKind, FoulKind, TurnoverKind};
        let w = World::generate(9, 2);
        let home = lineup(&w.teams[0], &w.players);
        let away = lineup(&w.teams[1], &w.players);
        let mut kinds = [0u32; 8];
        let (mut down, mut braced, mut whistled) = (0, 0, 0);
        let mut whistles = Vec::new();
        for seed in 0..12 {
            let (_, rec) = simulate_recorded(&home, &away, &w.rules, &mut Rng::new(seed));
            assert!((0.85..=1.15).contains(&rec.whistle), "{}", rec.whistle);
            whistles.push(rec.whistle);
            for (i, e) in rec.events.iter().enumerate() {
                let EventKind::Foul { kind, shots, .. } = &e.kind else {
                    continue;
                };
                kinds[*kind as usize] += 1;
                if kind.is_offensive() {
                    // No free throws, and the same moment holds the turnover.
                    assert_eq!(*shots, 0);
                    assert!(rec.events[i..].iter().take(3).any(|n| matches!(
                        n.kind,
                        EventKind::Turnover {
                            kind: TurnoverKind::OffensiveFoul,
                            ..
                        }
                    )));
                }
            }
            for f in &rec.frames {
                whistled += f.whistle as u32;
                for p in f.pose.iter().flatten() {
                    down += (p.action == Action::Down) as u32;
                    braced += (p.action == Action::Bracing) as u32;
                }
            }
        }
        // The crews differ from game to game.
        assert!(whistles.iter().any(|x| (x - whistles[0]).abs() > 0.02));
        assert!(kinds[FoulKind::Shooting as usize] > 0 && kinds[FoulKind::ReachIn as usize] > 0);
        assert!(kinds[FoulKind::Charging as usize] + kinds[FoulKind::Blocking as usize] > 0);
        assert!(
            whistled > 0 && down > 0 && braced > 0,
            "{whistled} {down} {braced}"
        );
    }

    /// The tactics are not a table of bonuses: a zone packs the paint
    /// because of where its defenders stand, and pays for it outside.
    #[test]
    fn a_zone_trades_shots_at_the_rim_for_threes() {
        use crate::court::ShotKind;
        use crate::tactics::DefenseScheme;
        let w = World::generate(9, 2);
        let mix = |defense: DefenseScheme| {
            let home = lineup(&w.teams[0], &w.players);
            let mut tactics = w.teams[1].tactics().clone();
            tactics.defense = defense;
            let mut away = lineup(&w.teams[1], &w.players);
            away.tactics = &tactics;
            let (mut rim, mut three, mut all) = (0.0f32, 0.0f32, 0.0f32);
            for seed in 0..40 {
                let (_, rec) = simulate_recorded(&home, &away, &w.rules, &mut Rng::new(seed));
                for e in rec.events.iter().filter(|e| e.offense == 0) {
                    if let crate::replay::EventKind::Shot { kind, .. } = &e.kind {
                        all += 1.0;
                        rim += (*kind == ShotKind::Rim) as u8 as f32;
                        three += (*kind == ShotKind::Three) as u8 as f32;
                    }
                }
            }
            (rim / all, three / all)
        };
        let (man, zone) = (mix(DefenseScheme::ManToMan), mix(DefenseScheme::Zone23));
        assert!(zone.0 < man.0, "rim share: zone {} man {}", zone.0, man.0);
        assert!(zone.1 > man.1, "three share: zone {} man {}", zone.1, man.1);
    }

    #[test]
    fn changing_system_costs_familiarity() {
        let mut w = World::generate(5, 2);
        let slot = &mut w.teams[0].playbook[0];
        let before = slot.familiarity();
        let was = slot.tactics.defense;
        slot.tactics.defense = if was == crate::tactics::DefenseScheme::Zone23 {
            crate::tactics::DefenseScheme::ManToMan
        } else {
            crate::tactics::DefenseScheme::Zone23
        };
        assert!(slot.changed() && slot.familiarity() < before);
        slot.tactics.defense = was;
        assert_eq!(slot.familiarity(), before);
        for _ in 0..10 {
            train_playbook(&mut w.teams[0].playbook, 0, 0.15);
        }
        assert_eq!(w.teams[0].playbook[0].familiarity(), 1.0);
    }

    #[test]
    fn scouting_reports_follow_shortlisted_players() {
        let mut w = World::generate(9, 8);
        let target = w.teams[3].playbook[0].tactics.rotation[0].player;
        let p = w.players[target as usize].clone();
        assert!(w.scouting.toggle(target));
        let width = |w: &World| {
            let (lo, hi) = w.scouting.estimate(&p, Attr::ThreePoint);
            assert!(lo <= p.attrs[Attr::ThreePoint] && p.attrs[Attr::ThreePoint] <= hi);
            hi - lo
        };
        let before = width(&w);
        for _ in 0..5 {
            w.play_round();
        }
        // One report for every game he played; he can miss some hurt.
        let games = w.games_played[target as usize] as usize;
        assert!(games >= 3);
        assert_eq!(w.scouting.reports_for(target).count(), games);
        assert!(
            w.scouting
                .reports
                .iter()
                .all(|r| w.team_of(r.player) == Some(3) && r.opponent != 3)
        );
        assert!(width(&w) < before);
        // Unwatched players stay at public knowledge.
        let other = w.teams[4].roster[0];
        assert_eq!(w.scouting.knowledge(other), 0.25);
    }

    #[test]
    fn bodies_are_plausible() {
        let w = World::generate(4, 12);
        for p in &w.players {
            let b = &p.body;
            assert!((175.0..=220.0).contains(&b.height_cm), "{b:?}");
            assert!(
                b.wingspan_cm > b.height_cm && b.ape_index_cm() < 15.0,
                "{b:?}"
            );
            assert!(
                (b.standing_reach_cm / b.height_cm - 1.33).abs() < 0.04,
                "{b:?}"
            );
            assert!((65.0..=135.0).contains(&b.weight_kg), "{b:?}");
        }
    }

    #[test]
    fn assistant_reacts_to_the_numbers() {
        // Whether a club that looks after the ball badly is told so depends on
        // what its first six games happened to be: look at several worlds.
        let (mut told, mut worlds) = (0, 0);
        for seed in 2..10 {
            let mut w = World::generate(seed, 8);
            let t = w.teams[0].tactics().clone();
            w.teams[0].playbook[0] = TacticSlot::new("Tactic 1", t, 0.8);
            let early = assistant::plan_schedule(&w, 0);
            // No stats yet, but the new tactic is only 80% learned.
            assert_eq!(early.reasons[0], Reason::TooEarly);
            assert!(
                early
                    .reasons
                    .iter()
                    .any(|r| matches!(r, Reason::LearningTactic { .. }))
            );
            assert_eq!(early.schedule.count(crate::training::Session::Tactics), 3);

            // A team that cannot look after the ball should be told to work on it.
            for &p in &w.teams[0].roster.clone() {
                w.players[p as usize].attrs[Attr::Handling] = 2.0;
                w.players[p as usize].attrs[Attr::Passing] = 2.0;
            }
            for _ in 0..6 {
                w.play_round();
            }
            let advice = assistant::plan_schedule(&w, 0);
            worlds += 1;
            if advice
                .reasons
                .iter()
                .any(|r| matches!(r, Reason::Turnovers { .. }))
            {
                told += 1;
                assert!(advice.schedule.count(crate::training::Session::Playmaking) >= 1);
                // Delegated teams got the plan applied during the week.
                assert!(!w.teams[0].assistant_notes.is_empty());
            }
        }
        assert!(told * 4 >= worlds * 3, "told in {told} of {worlds} worlds");
    }

    #[test]
    fn professionals_develop_faster() {
        let growth = |professionalism: u8| {
            let mut w = World::generate(6, 2);
            let mut rng = Rng::new(1);
            let before: f32 = w.players.iter().map(|p| p.overall()).sum();
            for p in &mut w.players {
                p.personality.professionalism = professionalism;
                for _ in 0..30 {
                    train_week(p, &Schedule::default(), &mut rng, 1.0, 1.0);
                }
            }
            w.players.iter().map(|p| p.overall()).sum::<f32>() - before
        };
        assert!(growth(20) > growth(1) + 0.5);
    }

    #[test]
    fn stronger_team_wins_more() {
        let mut w = World::generate(11, 2);
        for &p in &w.teams[0].roster.clone() {
            for a in Attr::ALL {
                w.players[p as usize].attrs[a] = (w.players[p as usize].attrs[a] + 3.0).min(20.0);
            }
        }
        let mut wins = 0;
        for seed in 0..200 {
            let r = simulate_game(
                &lineup(&w.teams[0], &w.players),
                &lineup(&w.teams[1], &w.players),
                &w.rules,
                &mut Rng::new(seed),
            );
            wins += (r.score[0] > r.score[1]) as u32;
        }
        assert!(wins > 150, "boosted team won only {wins}/200");
    }
}
