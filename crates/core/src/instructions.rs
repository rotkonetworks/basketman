//! Standing match instructions: if-then rules a manager leaves for a game he
//! is not coaching live.
//!
//! A rule is a list of conditions (all must hold) and a list of actions. The
//! `Coach` steps a `LiveRound` and, about once a second of play, looks at the
//! game as a person on the bench would and fires the rules whose conditions
//! hold. The actions are the same things a live coach can do (`LiveInput`),
//! and every one goes through `LiveRound::input`, so the game is logged as a
//! `PlayLive` and replays exactly without the rules.
//!
//! Everything is integers and enums so a rule set compares and serialises
//! without surprises. Nothing here reads the clock or a random source.
//!
//! "Time" is game time reconstructed from the minutes on the floor, not the
//! replay clock: a quarter is `Rules::period_secs` long and the rules see
//! the same seconds the box score counts.

use serde::{Deserialize, Serialize};

use crate::command::CommandError;
use crate::engine::{Orders, Pause, Rules, Shout};
use crate::league::{LiveInput, LiveRound, Team, TeamId, World};
use crate::player::PlayerId;
use crate::tactics::{DefenseScheme, OffenseSet, Press};

/// Most rules a club may leave.
pub const MAX_RULES: usize = 16;
/// Most conditions in one rule.
pub const MAX_CONDITIONS: usize = 8;
/// Most actions in one rule.
pub const MAX_ACTIONS: usize = 4;
/// The most times one rule may fire in one game, whatever it asks for.
pub const MAX_FIRES: u8 = 10;
/// Steps (tenths of a second) between looks at the game.
const LOOK_EVERY: u32 = 10;
/// Hard stop on a game's length in steps, so a bug can never spin forever.
const STEP_LIMIT: u32 = 2_000_000;

fn once() -> u8 {
    1
}

fn a_minute() -> u16 {
    60
}

/// A club's standing rules. Empty means the assistant coaches the game.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Instructions {
    pub rules: Vec<Rule>,
}

/// One if-then rule.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    /// A label for the manager's own use.
    #[serde(default)]
    pub name: String,
    /// All of these must hold. An empty list always holds.
    #[serde(default)]
    pub when: Vec<Condition>,
    /// Done in order when the rule fires.
    pub then: Vec<Action>,
    /// Most times the rule fires in one game, 1..=`MAX_FIRES`.
    #[serde(default = "once")]
    pub max_fires: u8,
    /// Game seconds that must pass before the same rule fires again.
    #[serde(default = "a_minute")]
    pub cooldown_secs: u16,
}

/// Something about the game that is true or not.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    /// Our score minus theirs is at most this (`-8`: we trail by eight or more).
    MarginAtMost(i16),
    /// Our score minus theirs is at least this (`10`: we lead by ten or more).
    MarginAtLeast(i16),
    /// The period is within `from..=to`, counting from 1; overtime periods
    /// continue the count.
    Period {
        from: u8,
        to: u8,
    },
    /// At most this many game seconds are left in the period.
    PeriodClockLeftAtMost(u16),
    /// At least this many game seconds have been played in all.
    ElapsedAtLeast(u16),
    /// The opponent has scored at least this many points since we last did.
    OpponentRunAtLeast(u16),
    /// A player on the floor has at most this much energy left (percent).
    /// `None` means any of ours on the floor.
    PlayerEnergyBelow {
        player: Option<PlayerId>,
        percent: u8,
    },
    /// A player on the floor has at least this many fouls.
    PlayerFoulsAtLeast {
        player: Option<PlayerId>,
        fouls: u8,
    },
    /// Our team fouls in the current period are at least this many.
    TeamFoulsAtLeast(u8),
    /// The opposing bench is running this defense now.
    OpponentDefenseIs(DefenseScheme),
    OpponentOffenseIs(OffenseSet),
    OpponentPressIs(Press),
}

/// A player named in an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerRef {
    Id(PlayerId),
    /// The player a condition of this rule matched (the tired or fouled one).
    Triggering,
    /// The bench player with the most energy who is not fouled out.
    BestRested,
}

/// What a rule does: the live coach's choices.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// Ask for a timeout at the next dead ball. Skipped if none is left or
    /// one is already asked for.
    Timeout,
    /// Change who is on the floor at the next dead ball. When it has been
    /// done the assistant takes the substitutions back, unless a rule later
    /// says `AutoSubs(false)`.
    Swap { off: PlayerRef, on: PlayerRef },
    /// Change the named parts of the system. The rest stays as it is. How
    /// well the squad knows the new system is worked out by the game from
    /// the playbook; a rule cannot claim it.
    SetOrders {
        #[serde(default)]
        offense: Option<OffenseSet>,
        #[serde(default)]
        defense: Option<DefenseScheme>,
        #[serde(default)]
        press: Option<Press>,
        #[serde(default)]
        pace: Option<u8>,
        #[serde(default)]
        three_bias: Option<u8>,
        #[serde(default)]
        crash_boards: Option<u8>,
        #[serde(default)]
        aggression: Option<u8>,
    },
    /// Say something to the floor, or to one player.
    Shout {
        shout: Shout,
        #[serde(default)]
        target: Option<PlayerRef>,
    },
    /// Hand the substitutions to the assistant, or take them.
    AutoSubs(bool),
}

impl Instructions {
    /// Check limits and that every named player is on `roster`.
    pub fn validate(&self, roster: &[PlayerId]) -> Result<(), String> {
        if self.rules.len() > MAX_RULES {
            return Err(format!("At most {MAX_RULES} rules."));
        }
        let own = |p: &PlayerId| roster.contains(p);
        for (i, r) in self.rules.iter().enumerate() {
            let n = i + 1;
            if r.then.is_empty() {
                return Err(format!("Rule {n} does nothing."));
            }
            if r.then.len() > MAX_ACTIONS {
                return Err(format!("Rule {n} has more than {MAX_ACTIONS} actions."));
            }
            if r.when.len() > MAX_CONDITIONS {
                return Err(format!(
                    "Rule {n} has more than {MAX_CONDITIONS} conditions."
                ));
            }
            if r.max_fires == 0 || r.max_fires > MAX_FIRES {
                return Err(format!("Rule {n}: max_fires must be 1 to {MAX_FIRES}."));
            }
            for c in &r.when {
                match c {
                    Condition::PlayerEnergyBelow { player, percent } => {
                        if *percent > 100 {
                            return Err(format!("Rule {n}: energy is a percent."));
                        }
                        if player.as_ref().is_some_and(|p| !own(p)) {
                            return Err(format!("Rule {n} names a player who is not yours."));
                        }
                    }
                    Condition::PlayerFoulsAtLeast { player, .. } => {
                        if player.as_ref().is_some_and(|p| !own(p)) {
                            return Err(format!("Rule {n} names a player who is not yours."));
                        }
                    }
                    Condition::Period { from, to } if from > to || *from == 0 => {
                        return Err(format!("Rule {n}: periods count from 1, from <= to."));
                    }
                    _ => {}
                }
            }
            let needs_player = r.when.iter().any(|c| {
                matches!(
                    c,
                    Condition::PlayerEnergyBelow { .. } | Condition::PlayerFoulsAtLeast { .. }
                )
            });
            for a in &r.then {
                let refs: Vec<&PlayerRef> = match a {
                    Action::Swap { off, on } => vec![off, on],
                    Action::Shout {
                        target: Some(t), ..
                    } => vec![t],
                    _ => vec![],
                };
                for p in refs {
                    match p {
                        PlayerRef::Id(id) if !own(id) => {
                            return Err(format!("Rule {n} names a player who is not yours."));
                        }
                        PlayerRef::Triggering if !needs_player => {
                            return Err(format!(
                                "Rule {n} uses Triggering without an energy or fouls condition."
                            ));
                        }
                        _ => {}
                    }
                }
                if let Action::SetOrders {
                    pace,
                    three_bias,
                    crash_boards,
                    aggression,
                    ..
                } = a
                    && [pace, three_bias, crash_boards, aggression]
                        .iter()
                        .any(|v| v.is_some_and(|v| !(1..=20).contains(&v)))
                {
                    return Err(format!("Rule {n}: sliders run from 1 to 20."));
                }
            }
        }
        Ok(())
    }
}

/// Plays a `LiveRound` for one side by its standing rules.
pub struct Coach {
    rules: Vec<Rule>,
    fires: Vec<u8>,
    last_fire: Vec<Option<u32>>,
    rule_set: Rules,
    steps: u32,
    /// Last scores seen, ours then theirs.
    seen: [u16; 2],
    /// Points the opponent has scored since we last did.
    run: u16,
    /// A swap went in: give the substitutions back once it has been made.
    restore_auto: bool,
    /// Game seconds played so far, as of the last look.
    elapsed: u32,
    /// Times the rules fired, for the caller to report.
    pub fired: Vec<(u32, usize)>,
    /// Changes of system and words that fired in live play: tactics wait
    /// for a stoppage, so they are done at the next one.
    pending: Vec<LiveInput>,
}

/// Per-look facts about the game.
struct Look {
    margin: i32,
    elapsed: u32,
    period: u8,
    clock_left: u32,
}

impl Coach {
    pub fn new(instructions: &Instructions, rules: &Rules) -> Coach {
        let n = instructions.rules.len();
        Coach {
            rules: instructions.rules.clone(),
            fires: vec![0; n],
            last_fire: vec![None; n],
            rule_set: rules.clone(),
            steps: 0,
            seen: [0, 0],
            run: 0,
            restore_auto: false,
            elapsed: 0,
            fired: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// Points the opponent has scored in a row, as the coach sees it.
    pub fn opponent_run(&self) -> u16 {
        self.run
    }

    /// Step the game once and act on the rules. Returns what `LiveRound::step`
    /// returns.
    pub fn step(&mut self, team: &Team, live: &mut LiveRound) -> Option<Pause> {
        let pause = live.step();
        self.steps += 1;
        if let Some(Pause::Timeout(s)) = pause
            && s == live.side
        {
            self.run = 0;
        }
        if pause.is_some() {
            for input in std::mem::take(&mut self.pending) {
                live.input(input);
            }
        }
        if pause.is_some() || self.steps.is_multiple_of(LOOK_EVERY) {
            self.look(team, live);
        }
        pause
    }

    /// Play the game to the end.
    pub fn play_out(&mut self, team: &Team, live: &mut LiveRound) {
        let mut n = 0;
        while !live.game.is_over() && n < STEP_LIMIT {
            self.step(team, live);
            n += 1;
        }
    }

    fn look(&mut self, team: &Team, live: &mut LiveRound) {
        let side = live.side;
        let score = live.game.result().score;
        let now = [score[side], score[1 - side]];
        let (us, them) = (now[0] != self.seen[0], now[1] != self.seen[1]);
        if us {
            self.run = 0;
        } else if them {
            self.run += now[1] - self.seen[1];
        }
        self.seen = now;
        let mine = live.game.players(side);
        let secs: u32 = mine.iter().map(|p| p.line.secs).sum();
        self.elapsed = secs / 5;
        let rs = &self.rule_set;
        let regulation = rs.periods as u32 * rs.period_secs;
        let (period, clock_left) = if self.elapsed < regulation {
            let p = self.elapsed / rs.period_secs.max(1);
            (p + 1, rs.period_secs - self.elapsed % rs.period_secs.max(1))
        } else {
            let over = self.elapsed - regulation;
            let o = over / rs.overtime_secs.max(1);
            (
                rs.periods as u32 + o + 1,
                rs.overtime_secs - over % rs.overtime_secs.max(1),
            )
        };
        let look = Look {
            margin: now[0] as i32 - now[1] as i32,
            elapsed: self.elapsed,
            period: period.min(255) as u8,
            clock_left,
        };

        if self.restore_auto && live.game.queued(side).is_empty() {
            self.restore_auto = false;
            live.input(LiveInput::AutoSubs(true));
        }
        for i in 0..self.rules.len() {
            if self.fires[i] >= self.rules[i].max_fires.min(MAX_FIRES) {
                continue;
            }
            if let Some(at) = self.last_fire[i]
                && look.elapsed < at + self.rules[i].cooldown_secs as u32
            {
                continue;
            }
            let Some(trigger) = self.holds(i, &look, live, &mine) else {
                continue;
            };
            self.fires[i] += 1;
            self.last_fire[i] = Some(look.elapsed);
            self.fired.push((look.elapsed, i));
            let actions = self.rules[i].then.clone();
            for a in &actions {
                self.act(a, trigger, team, live);
            }
        }
    }

    /// `Some(triggering player)` when every condition of rule `i` holds.
    fn holds(
        &self,
        i: usize,
        look: &Look,
        live: &LiveRound,
        mine: &[crate::engine::LivePlayer],
    ) -> Option<Option<PlayerId>> {
        let mut trigger = None;
        for c in &self.rules[i].when {
            let ok = match c {
                Condition::MarginAtMost(m) => look.margin <= *m as i32,
                Condition::MarginAtLeast(m) => look.margin >= *m as i32,
                Condition::Period { from, to } => (*from..=*to).contains(&look.period),
                Condition::PeriodClockLeftAtMost(s) => look.clock_left <= *s as u32,
                Condition::ElapsedAtLeast(s) => look.elapsed >= *s as u32,
                Condition::OpponentRunAtLeast(n) => self.run >= *n,
                Condition::PlayerEnergyBelow { player, percent } => {
                    let hit = mine.iter().find(|p| {
                        p.on_court
                            && player.is_none_or(|id| id == p.id)
                            && p.energy * 100.0 < *percent as f32
                    });
                    trigger = trigger.or(hit.map(|p| p.id));
                    hit.is_some()
                }
                Condition::PlayerFoulsAtLeast { player, fouls } => {
                    let hit = mine.iter().find(|p| {
                        p.on_court
                            && player.is_none_or(|id| id == p.id)
                            && p.line.pf >= *fouls as u16
                    });
                    trigger = trigger.or(hit.map(|p| p.id));
                    hit.is_some()
                }
                Condition::TeamFoulsAtLeast(n) => live.game.team_fouls(live.side) >= *n as u16,
                Condition::OpponentDefenseIs(d) => live.game.orders(1 - live.side).defense == *d,
                Condition::OpponentOffenseIs(o) => live.game.orders(1 - live.side).offense == *o,
                Condition::OpponentPressIs(p) => live.game.orders(1 - live.side).press == *p,
            };
            if !ok {
                return None;
            }
        }
        Some(trigger)
    }

    fn resolve(
        &self,
        r: PlayerRef,
        trigger: Option<PlayerId>,
        live: &LiveRound,
        on_court: bool,
    ) -> Option<PlayerId> {
        match r {
            PlayerRef::Id(id) => Some(id),
            PlayerRef::Triggering => trigger,
            PlayerRef::BestRested => {
                let mut bench: Vec<_> = live
                    .game
                    .players(live.side)
                    .into_iter()
                    .filter(|p| p.on_court == on_court && !p.fouled_out)
                    .collect();
                bench.sort_by(|a, b| b.energy.total_cmp(&a.energy).then(a.id.cmp(&b.id)));
                bench.first().map(|p| p.id)
            }
        }
    }

    /// Tactics and talk are for stoppages: in live play they wait for the next.
    fn say_or_wait(&mut self, live: &mut LiveRound, input: LiveInput) {
        if live.stoppage().is_some() {
            live.input(input);
        } else {
            self.pending.push(input);
        }
    }

    fn act(&mut self, a: &Action, trigger: Option<PlayerId>, team: &Team, live: &mut LiveRound) {
        let side = live.side;
        match a {
            Action::Timeout => {
                if live.game.timeouts_left(side) > 0 && !live.game.timeout_wanted(side) {
                    live.input(LiveInput::Timeout);
                }
            }
            Action::Swap { off, on } => {
                let (Some(off), Some(on)) = (
                    self.resolve(*off, trigger, live, true),
                    self.resolve(*on, trigger, live, false),
                ) else {
                    return;
                };
                let players = live.game.players(side);
                let ok = players.iter().any(|p| p.id == off && p.on_court)
                    && players
                        .iter()
                        .any(|p| p.id == on && !p.on_court && !p.fouled_out);
                if ok {
                    live.input(LiveInput::Swap { off, on });
                    self.restore_auto = true;
                }
            }
            Action::SetOrders {
                offense,
                defense,
                press,
                pace,
                three_bias,
                crash_boards,
                aggression,
            } => {
                let base = live.game.orders(side);
                let o = Orders {
                    offense: offense.unwrap_or(base.offense),
                    defense: defense.unwrap_or(base.defense),
                    press: press.unwrap_or(base.press),
                    pace: pace.unwrap_or(base.pace).clamp(1, 20),
                    three_bias: three_bias.unwrap_or(base.three_bias).clamp(1, 20),
                    crash_boards: crash_boards.unwrap_or(base.crash_boards).clamp(1, 20),
                    aggression: aggression.unwrap_or(base.aggression).clamp(1, 20),
                };
                if o != base {
                    let familiarity = team.familiarity_with(o.offense, o.defense, o.press);
                    self.say_or_wait(
                        live,
                        LiveInput::Orders {
                            orders: o,
                            familiarity,
                        },
                    );
                }
            }
            Action::Shout { shout, target } => {
                let target = match target {
                    Some(t) => match self.resolve(*t, trigger, live, true) {
                        Some(id) => Some(id),
                        None => return,
                    },
                    None => None,
                };
                self.say_or_wait(
                    live,
                    LiveInput::Shout {
                        shout: *shout,
                        target,
                    },
                );
            }
            Action::AutoSubs(on) => {
                self.restore_auto = false;
                live.input(LiveInput::AutoSubs(*on));
            }
        }
    }
}

impl World {
    /// Play this round with the user's club coached by its standing
    /// instructions. With none, the assistant coaches the game exactly as
    /// `play_round` would, and the log gets `PlayRound`; otherwise the game
    /// is played live, stepped by a `Coach`, and logged as `PlayLive`.
    pub(crate) fn play_with_instructions(&mut self) -> Result<Vec<usize>, CommandError> {
        self.play_with_instructions_scoped(crate::league::Scope::Round)
    }

    /// As `play_with_instructions`, for the whole round or for today only.
    pub(crate) fn play_with_instructions_scoped(
        &mut self,
        scope: crate::league::Scope,
    ) -> Result<Vec<usize>, CommandError> {
        use crate::league::Scope;
        let me: TeamId = self.watch.ok_or(CommandError::NoClub)?;
        if self.season_over() {
            return Err(CommandError::Refused("The season is over.".into()));
        }
        let instructions = self.teams[me].instructions.clone();
        if scope == Scope::Day && self.user_fixture().is_none() {
            return Err(CommandError::Refused(
                "There is no game for you this round.".into(),
            ));
        }
        let live = if instructions.rules.is_empty() {
            None
        } else {
            self.kickoff_scoped(me, scope)
        };
        let Some(mut live) = live else {
            let (played, command) = match scope {
                Scope::Round => (self.play_round(), crate::command::Command::PlayRound),
                Scope::Day => (self.play_today(), crate::command::Command::PlayToday),
            };
            self.log.push(command);
            return Ok(played);
        };
        let mut coach = Coach::new(&instructions, &self.rules);
        coach.play_out(&self.teams[me], &mut live);
        Ok(self.full_time(live))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::command::tests::tidy;
    use crate::inbox::Stop;

    fn timeout_rule() -> Instructions {
        Instructions {
            rules: vec![Rule {
                name: "stop the run".into(),
                when: vec![Condition::OpponentRunAtLeast(8)],
                then: vec![Action::Timeout],
                max_fires: 3,
                cooldown_secs: 30,
            }],
        }
    }

    /// Advance to the next match day.
    fn to_match_day(w: &mut World, n: &mut usize) {
        loop {
            tidy(w, n);
            if w.apply(Command::Advance).unwrap().stop == Some(Stop::MatchDay) {
                return;
            }
        }
    }

    #[test]
    fn a_run_rule_calls_timeouts_when_the_opponent_runs() {
        let mut w = World::generate(11, 8);
        w.apply(Command::TakeCharge(0)).unwrap();
        w.apply(Command::SetInstructions(timeout_rule())).unwrap();
        let (mut n, mut seen_runs, mut timeouts) = (0, 0, 0);
        for _ in 0..6 {
            to_match_day(&mut w, &mut n);
            // The same game, stepped by hand, to look at what the coach does.
            let mut probe = w.clone();
            let me = 0;
            let mut live = probe.kickoff(me).unwrap();
            let mut coach = Coach::new(&w.teams[me].instructions, &w.rules);
            let mut run_when_called = Vec::new();
            while !live.game.is_over() {
                let before = live.game.timeout_wanted(live.side);
                let run = coach.opponent_run();
                if let Some(Pause::Timeout(s)) = coach.step(&w.teams[me], &mut live) {
                    if s == live.side {
                        timeouts += 1;
                    }
                } else if !before && live.game.timeout_wanted(live.side) {
                    run_when_called.push(run.max(coach.opponent_run()));
                }
            }
            seen_runs += run_when_called.len();
            assert!(
                run_when_called.iter().all(|&r| r >= 8),
                "{run_when_called:?}"
            );
            assert!(coach.fired.len() <= 3);
            w.apply(Command::PlayWithInstructions).unwrap();
        }
        assert!(seen_runs > 0, "no run of eight in six games");
        assert!(
            timeouts > 0,
            "the rule asked for timeouts but none were taken"
        );
    }

    #[test]
    fn no_rules_is_the_assistant_game_and_replay_matches() {
        // Without rules, PlayWithInstructions is PlayRound.
        let mut a = World::generate(5, 8);
        let mut b = World::generate(5, 8);
        for w in [&mut a, &mut b] {
            w.apply(Command::TakeCharge(1)).unwrap();
        }
        let (mut n, mut m) = (0, 0);
        for _ in 0..3 {
            to_match_day(&mut a, &mut n);
            to_match_day(&mut b, &mut m);
            a.apply(Command::PlayWithInstructions).unwrap();
            b.apply(Command::PlayRound).unwrap();
        }
        assert!(format!("{a:?}") == format!("{b:?}"), "worlds differ");

        // With rules, the log replays to the same world.
        let mut first = World::generate(9, 8);
        first.apply(Command::TakeCharge(2)).unwrap();
        let mut rules = timeout_rule();
        rules.rules.push(Rule {
            name: "tired".into(),
            when: vec![Condition::PlayerEnergyBelow {
                player: None,
                percent: 55,
            }],
            then: vec![
                Action::Swap {
                    off: PlayerRef::Triggering,
                    on: PlayerRef::BestRested,
                },
                Action::SetOrders {
                    offense: None,
                    defense: Some(DefenseScheme::Zone23),
                    press: None,
                    pace: Some(9),
                    three_bias: None,
                    crash_boards: None,
                    aggression: None,
                },
                Action::Shout {
                    shout: Shout::Encourage,
                    target: None,
                },
            ],
            max_fires: 4,
            cooldown_secs: 120,
        });
        first.apply(Command::SetInstructions(rules)).unwrap();
        let mut n = 0;
        for _ in 0..5 {
            to_match_day(&mut first, &mut n);
            first.apply(Command::PlayWithInstructions).unwrap();
        }
        assert!(
            first
                .log
                .iter()
                .any(|c| matches!(c, Command::PlayLive { inputs } if !inputs.is_empty()))
        );
        let mut second = World::generate(9, 8);
        for cmd in first.log.clone() {
            second.apply(cmd).unwrap();
        }
        assert_eq!(second.log, first.log);
        assert!(
            format!("{first:?}") == format!("{second:?}"),
            "replay differs"
        );
    }

    #[test]
    fn limits_and_ownership_are_enforced() {
        let mut w = World::generate(3, 8);
        w.apply(Command::TakeCharge(0)).unwrap();
        let foreign = w.teams[1].roster[0];
        let swap = |p| Instructions {
            rules: vec![Rule {
                name: String::new(),
                when: vec![],
                then: vec![Action::Swap {
                    off: PlayerRef::Id(p),
                    on: PlayerRef::BestRested,
                }],
                max_fires: 1,
                cooldown_secs: 0,
            }],
        };
        assert!(w.apply(Command::SetInstructions(swap(foreign))).is_err());
        let own = w.teams[0].roster[0];
        assert!(w.apply(Command::SetInstructions(swap(own))).is_ok());
        let mut many = timeout_rule();
        many.rules = vec![many.rules[0].clone(); MAX_RULES + 1];
        assert!(w.apply(Command::SetInstructions(many)).is_err());
        let mut bad = timeout_rule();
        bad.rules[0].max_fires = 11;
        assert!(w.apply(Command::SetInstructions(bad)).is_err());
        let mut idle = timeout_rule();
        idle.rules[0].then.clear();
        assert!(w.apply(Command::SetInstructions(idle)).is_err());
        let mut sliders = timeout_rule();
        sliders.rules[0].then = vec![Action::SetOrders {
            offense: None,
            defense: None,
            press: None,
            pace: Some(0),
            three_bias: None,
            crash_boards: None,
            aggression: None,
        }];
        assert!(w.apply(Command::SetInstructions(sliders)).is_err());
    }
}
