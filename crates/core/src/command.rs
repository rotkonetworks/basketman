//! Everything the player can do, as values.
//!
//! The UI never reaches into the world to change it: it builds a `Command`
//! and hands it to `World::apply`, the one function that validates an action
//! and carries it out. Every command that succeeded is appended to
//! `World::log`, so a career is its seed plus its log (live games coached
//! through `World::kickoff` are logged by `World::full_time`).
//!
//! Commands never name the club: they act for the user's club,
//! `World::watch`, which `Command::TakeCharge` sets. Nothing here
//! reimplements game rules; each command validates what it addresses and
//! calls the method that already does the work.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::assistant;
use crate::books::FacilityKind;
use crate::inbox::{Deal, OfferState, Stop};
use crate::league::{LiveInput, Scope, TeamId, World};
use crate::media::MediaLevel;
use crate::player::PlayerId;
use crate::staff::Job;
use crate::tactics::Tactics;
use crate::training::{Intensity, Schedule, TrainingFocus};

/// One thing the player does.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Command {
    /// Take the job at `TeamId`. Only valid once per world.
    TakeCharge(TeamId),
    /// Let the calendar run until a game, new mail or the draft.
    Advance,
    /// Pass one day. Whatever is scheduled for it happens: other clubs'
    /// games, reserve games, friendlies. Does nothing if the manager has a
    /// game to play first.
    Continue,
    /// Play this round's games without coaching any of them.
    PlayRound,
    /// Play only today's games, the manager's with the assistant (or the
    /// standing instructions) in charge. The rest of the round follows on
    /// its days.
    PlayToday,
    /// A live game of `PlayToday`'s scope; logged by `World::full_time`.
    PlayLiveToday {
        inputs: Vec<(u32, LiveInput)>,
    },
    /// Coach this round's game of the user's club live: the inputs were
    /// applied after that many `step` calls (see `LiveRound`). Logged by
    /// `World::full_time`; replayed by `apply`.
    PlayLive {
        inputs: Vec<(u32, LiveInput)>,
    },
    MarkRead(u32),
    /// Answer the transfer offer at `World::desk.offers[offer]`.
    AnswerOffer {
        offer: usize,
        accept: bool,
    },
    /// Answer press question `question` with option `choice` (0..=2).
    AnswerPress {
        question: usize,
        choice: u8,
    },
    /// Offer `fee` for a player of another club, or sign a free agent.
    Bid {
        player: PlayerId,
        fee: i64,
    },
    ProposeTrade(Deal),
    DraftPick(PlayerId),
    /// The assistant makes the pick, or all of ours.
    DraftAuto {
        all: bool,
    },
    /// Give `job` to the `candidate`th person on the staff market.
    Hire {
        job: Job,
        candidate: usize,
    },
    Dismiss(Job),
    /// Bring a reserve into the first team.
    Promote(PlayerId),
    /// Move a first-team player to the reserves.
    Demote(PlayerId),
    /// Whether a first-team player turns out for the reserves.
    SetDown {
        player: PlayerId,
        on: bool,
    },
    /// Put a player on, or take him off, the shortlist.
    Shortlist {
        player: PlayerId,
        on: bool,
    },
    SetTraining {
        player: PlayerId,
        focus: TrainingFocus,
        intensity: Intensity,
    },
    SetSchedule(Schedule),
    DelegateTraining(bool),
    /// The assistant plans the week and every player's training.
    AskAssistantToPlan,
    /// Replace the system, sliders, rotation and board of playbook entry
    /// `slot`. The squad's familiarity with it is not touched.
    SetTactics {
        slot: usize,
        tactics: Tactics,
    },
    RenameTactic {
        slot: usize,
        name: String,
    },
    /// Use playbook entry `slot` in matches.
    UseTactic(usize),
    /// Copy playbook entry `from` into a new one.
    CopyTactic {
        from: usize,
    },
    DeleteTactic(usize),
    /// Answer the sponsor offer at `World::econ.offers[offer]`.
    AnswerSponsor {
        offer: usize,
        accept: bool,
    },
    /// Refit the club's hall.
    Renovate,
    /// Build more seats, open next season.
    Expand,
    /// Charge this much for a ticket at home games.
    SetTicketPrice(i64),
    /// Move `amount` of the board's budget to the wage budget (`to_wages`)
    /// or to the transfer budget.
    MoveBudget {
        to_wages: bool,
        amount: i64,
    },
    /// Start building a facility up one level.
    UpgradeFacility(FacilityKind),
    /// Give a player of the club a weekly level of media and community
    /// work.
    SetMedia {
        player: PlayerId,
        level: MediaLevel,
    },
    /// Answer the sponsor's appearance request at `FameBook::asks[request]`.
    AnswerAppearance {
        request: usize,
        accept: bool,
    },
    /// Leave standing match instructions for the club; an empty set
    /// leaves the game to the assistant.
    SetInstructions(crate::instructions::Instructions),
    /// Play this round with the club's game coached by its standing
    /// instructions (the assistant's own game if there are none). The game
    /// is logged as what it was, `PlayLive` or `PlayRound`.
    PlayWithInstructions,
    /// Take the job offered at `World::career.offers[offer]`. `bring` are
    /// the jobs whose holders on your staff you ask to come; those who are
    /// willing do. `keep` are the jobs at the new club whose holders stay;
    /// the others are paid off.
    AcceptJob {
        offer: usize,
        bring: Vec<Job>,
        keep: Vec<Job>,
    },
    /// Turn down the job offered at `World::career.offers[offer]`.
    DeclineJob(usize),
    /// Leave the club you manage. You are out of work until somebody calls.
    Resign,
    /// Send the chief scout to watch `team` play on `day` (a day of the
    /// calendar); he files an opponent report after the game. Replaces
    /// what he was to do that day.
    AssignScout {
        team: TeamId,
        day: u32,
    },
    /// Take the chief scout off whatever he was to do on `day`.
    UnassignScout {
        day: u32,
    },
}

/// Why a command was not carried out.
#[derive(Clone, Debug, PartialEq)]
pub enum CommandError {
    /// Nobody is in charge of a club yet.
    NoClub,
    NoSuchPlayer(PlayerId),
    /// The player is not on the user's club.
    NotYourPlayer(PlayerId),
    NoSuchTeam(TeamId),
    /// An index addressed something that is not there.
    OutOfRange(&'static str),
    /// Valid, but it would change nothing.
    NothingToDo,
    /// The game said no; this is what it told the player.
    Refused(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::NoClub => f.write_str("Nobody is in charge."),
            CommandError::NoSuchPlayer(p) => write!(f, "There is no player {p}."),
            CommandError::NotYourPlayer(p) => write!(f, "Player {p} is not yours."),
            CommandError::NoSuchTeam(t) => write!(f, "There is no club {t}."),
            CommandError::OutOfRange(what) => write!(f, "No such {what}."),
            CommandError::NothingToDo => f.write_str("Nothing to do."),
            CommandError::Refused(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for CommandError {}

/// What a command hands back to the UI.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// Something to tell the user, if the game had something to say.
    pub note: Option<String>,
    /// `Advance`, `Continue`: why the calendar stopped, if it did.
    pub stop: Option<Stop>,
    /// `Advance`, `Continue`: the day the calendar stands at.
    pub day: Option<u32>,
    /// `PlayRound`: indices of the fixtures played.
    pub played: Vec<usize>,
    /// `CopyTactic`: the playbook index of the new entry.
    pub slot: Option<usize>,
}

fn refused(why: impl Into<String>) -> CommandError {
    CommandError::Refused(why.into())
}

/// Two commands that replace the same thing, the second making the first
/// pointless to keep.
fn replaces(last: &Command, next: &Command) -> bool {
    match (last, next) {
        (Command::SetTactics { slot: a, .. }, Command::SetTactics { slot: b, .. })
        | (Command::RenameTactic { slot: a, .. }, Command::RenameTactic { slot: b, .. }) => a == b,
        (Command::SetTraining { player: a, .. }, Command::SetTraining { player: b, .. }) => a == b,
        (Command::SetSchedule(_), Command::SetSchedule(_)) => true,
        (Command::SetTicketPrice(_), Command::SetTicketPrice(_)) => true,
        (Command::SetMedia { player: a, .. }, Command::SetMedia { player: b, .. }) => a == b,
        _ => false,
    }
}

impl World {
    /// Carry out a player action. A command that is refused changes nothing
    /// and is not logged; one that succeeds is appended to `self.log`.
    ///
    /// The whole-value commands (`SetTactics`, `RenameTactic`, `SetTraining`,
    /// `SetSchedule`) replace what they address, so a run of them on the same
    /// target with nothing in between is stored as its last one: dragging a
    /// slider does not fill the log.
    ///
    /// A live game is logged by `full_time` as `PlayLive`, not by this.
    pub fn apply(&mut self, cmd: Command) -> Result<Outcome, CommandError> {
        let outcome = self.execute(&cmd)?;
        if matches!(cmd, Command::PlayWithInstructions | Command::PlayToday) {
            // It logged the game it played, not itself.
            return Ok(outcome);
        }
        match self.log.last_mut() {
            Some(last) if replaces(last, &cmd) => *last = cmd,
            _ => self.log.push(cmd),
        }
        Ok(outcome)
    }

    fn club(&self) -> Result<TeamId, CommandError> {
        self.watch.ok_or(CommandError::NoClub)
    }

    fn player_in_range(&self, p: PlayerId) -> Result<(), CommandError> {
        if (p as usize) < self.players.len() {
            Ok(())
        } else {
            Err(CommandError::NoSuchPlayer(p))
        }
    }

    fn slot_in_range(&self, me: TeamId, slot: usize) -> Result<(), CommandError> {
        if slot < self.teams[me].playbook.len() {
            Ok(())
        } else {
            Err(CommandError::OutOfRange("tactic"))
        }
    }

    fn own_player(&self, me: TeamId, p: PlayerId) -> Result<(), CommandError> {
        self.player_in_range(p)?;
        if self.teams[me].roster.contains(&p) {
            Ok(())
        } else {
            Err(CommandError::NotYourPlayer(p))
        }
    }

    fn execute(&mut self, cmd: &Command) -> Result<Outcome, CommandError> {
        let mut out = Outcome::default();
        match cmd {
            Command::TakeCharge(team) => {
                if self.watch.is_some() {
                    return Err(refused("A club is already in your charge."));
                }
                if !self.career.spells.is_empty() {
                    return Err(refused("You can only take a job you are offered."));
                }
                if *team >= self.teams.len() {
                    return Err(CommandError::NoSuchTeam(*team));
                }
                self.take_charge(*team);
            }
            Command::Advance => {
                out.stop = Some(self.advance());
                out.day = Some(self.day);
            }
            Command::Continue => {
                out.stop = self.continue_day();
                out.day = Some(self.day);
            }
            Command::PlayToday => out.played = self.play_with_instructions_scoped(Scope::Day)?,
            Command::PlayLiveToday { inputs } => {
                out.played = self.replay_live(inputs, Scope::Day)?
            }
            Command::AcceptJob { offer, bring, keep } => {
                if *offer >= self.career.offers.len() {
                    return Err(CommandError::OutOfRange("job offer"));
                }
                out.note = Some(self.accept_job(*offer, bring, keep).map_err(refused)?);
            }
            Command::DeclineJob(offer) => {
                if *offer >= self.career.offers.len() {
                    return Err(CommandError::OutOfRange("job offer"));
                }
                out.note = Some(self.decline_job(*offer).map_err(refused)?);
            }
            Command::AssignScout { team, day } => {
                self.club()?;
                out.note = Some(self.assign_scout(*team, *day).map_err(refused)?);
            }
            Command::UnassignScout { day } => {
                self.club()?;
                self.unassign_scout(*day).map_err(refused)?;
            }
            Command::Resign => {
                self.club()?;
                out.note = Some(self.resign().map_err(refused)?);
            }
            Command::PlayRound => {
                if self.season_over() {
                    return Err(refused("The season is over."));
                }
                out.played = self.play_round();
            }
            Command::PlayLive { inputs } => out.played = self.replay_live(inputs, Scope::Round)?,
            Command::MarkRead(id) => {
                match self.desk.messages.iter().find(|m| m.id == *id) {
                    None => return Err(CommandError::OutOfRange("message")),
                    Some(m) if m.read => return Err(CommandError::NothingToDo),
                    Some(_) => {}
                }
                self.mark_read(*id);
            }
            Command::AnswerOffer { offer, accept } => {
                self.club()?;
                match self.desk.offers.get(*offer) {
                    None => return Err(CommandError::OutOfRange("offer")),
                    Some(o) if o.state != OfferState::Open => {
                        return Err(CommandError::NothingToDo);
                    }
                    Some(_) => {}
                }
                self.answer_offer(*offer, *accept);
            }
            Command::AnswerPress { question, choice } => {
                self.club()?;
                if *question >= self.desk.press.len() {
                    return Err(CommandError::OutOfRange("press question"));
                }
                if *choice > 2 {
                    return Err(CommandError::OutOfRange("answer"));
                }
                if !self.press_open(*question) {
                    return Err(CommandError::NothingToDo);
                }
                self.answer_press(*question, *choice);
            }
            Command::Bid { player, fee } => {
                let me = self.club()?;
                if self.spending_frozen(me) {
                    return Err(refused(
                        "The board has frozen spending until the budget is above zero.",
                    ));
                }
                self.player_in_range(*player)?;
                if *fee < 0 {
                    return Err(refused("An offer cannot be negative."));
                }
                // The board's budgets: a fee over the transfer budget is
                // refused, and so is a salary that takes the wage bill over
                // its budget.
                if self.team_of(*player).is_some() && !self.teams[me].roster.contains(player) {
                    self.fee_check(me, *fee).map_err(refused)?;
                }
                if !self.teams[me].roster.contains(player) {
                    let salary = self.players[*player as usize].contract.salary as i64;
                    self.wage_check(me, salary).map_err(refused)?;
                }
                self.bid(*player, *fee).map_err(refused)?;
            }
            Command::ProposeTrade(deal) => {
                let me = self.club()?;
                if deal.club >= self.teams.len() || deal.club == me {
                    return Err(CommandError::NoSuchTeam(deal.club));
                }
                for &p in deal.give.iter().chain(&deal.get) {
                    self.player_in_range(p)?;
                }
                for &p in &deal.give {
                    self.own_player(me, p)?;
                }
                if deal
                    .give_picks
                    .iter()
                    .chain(&deal.get_picks)
                    .any(|&i| i >= self.picks.len())
                {
                    return Err(CommandError::OutOfRange("pick"));
                }
                self.trade(deal).map_err(refused)?;
            }
            Command::DraftPick(player) => {
                self.club()?;
                self.player_in_range(*player)?;
                let ours = self
                    .draft
                    .as_ref()
                    .is_some_and(|d| d.on_clock() == self.watch && d.pool.contains(player));
                if !ours {
                    return Err(refused("It is not your pick, or he is not available."));
                }
                self.draft_take(*player);
            }
            Command::DraftAuto { all } => {
                self.club()?;
                let ours = self
                    .draft
                    .as_ref()
                    .is_some_and(|d| d.on_clock().is_some() && d.on_clock() == self.watch);
                if !ours {
                    return Err(CommandError::NothingToDo);
                }
                self.draft_auto(*all);
            }
            Command::Hire { job, candidate } => {
                let me = self.club()?;
                if *job == Job::HeadCoach {
                    return Err(refused("The head coach is you."));
                }
                if self.spending_frozen(me) {
                    return Err(refused(
                        "The board has frozen spending until the budget is above zero.",
                    ));
                }
                out.note = Some(self.hire(me, *job, *candidate).map_err(refused)?);
            }
            Command::Dismiss(job) => {
                let me = self.club()?;
                if *job == Job::HeadCoach {
                    return Err(refused("The head coach is you."));
                }
                if self.teams[me].office.staffer(*job).is_none() {
                    return Err(CommandError::NothingToDo);
                }
                self.dismiss(me, *job);
            }
            Command::Promote(player) => {
                let me = self.club()?;
                self.player_in_range(*player)?;
                self.promote(me, *player).map_err(refused)?;
            }
            Command::Demote(player) => {
                let me = self.club()?;
                self.player_in_range(*player)?;
                self.demote(me, *player).map_err(refused)?;
            }
            Command::SetDown { player, on } => {
                let me = self.club()?;
                self.own_player(me, *player)?;
                self.set_down(me, *player, *on);
            }
            Command::Shortlist { player, on } => {
                self.player_in_range(*player)?;
                if self.scouting.is_shortlisted(*player) == *on {
                    return Err(CommandError::NothingToDo);
                }
                if !self.scouting.toggle(*player) {
                    return Err(refused("The shortlist is full."));
                }
            }
            Command::SetTraining {
                player,
                focus,
                intensity,
            } => {
                let me = self.club()?;
                self.own_player(me, *player)?;
                let p = &mut self.players[*player as usize];
                if (p.training, p.intensity) == (*focus, *intensity) {
                    return Err(CommandError::NothingToDo);
                }
                (p.training, p.intensity) = (*focus, *intensity);
            }
            Command::SetSchedule(schedule) => {
                let me = self.club()?;
                if self.teams[me].schedule == *schedule {
                    return Err(CommandError::NothingToDo);
                }
                self.teams[me].schedule = schedule.clone();
            }
            Command::DelegateTraining(on) => {
                let me = self.club()?;
                if self.teams[me].delegate_training == *on {
                    return Err(CommandError::NothingToDo);
                }
                self.teams[me].delegate_training = *on;
            }
            Command::AskAssistantToPlan => {
                let me = self.club()?;
                assistant::run(self, me);
            }
            Command::SetTactics { slot, tactics } => {
                let me = self.club()?;
                self.slot_in_range(me, *slot)?;
                if tactics.rotation.is_empty() {
                    return Err(refused("The rotation cannot be empty."));
                }
                for (i, s) in tactics.rotation.iter().enumerate() {
                    self.own_player(me, s.player)?;
                    if tactics.rotation[..i].iter().any(|o| o.player == s.player) {
                        return Err(refused("A player is in the rotation twice."));
                    }
                }
                if [
                    tactics.pace,
                    tactics.three_bias,
                    tactics.crash_boards,
                    tactics.aggression,
                ]
                .iter()
                .any(|v| !(1..=20).contains(v))
                {
                    return Err(CommandError::OutOfRange("slider setting"));
                }
                if self.teams[me].playbook[*slot].tactics == *tactics {
                    return Err(CommandError::NothingToDo);
                }
                // Only the tactics: the slot's familiarity is its own.
                self.teams[me].playbook[*slot].tactics = tactics.clone();
            }
            Command::RenameTactic { slot, name } => {
                let me = self.club()?;
                self.slot_in_range(me, *slot)?;
                let s = &mut self.teams[me].playbook[*slot];
                if s.name == *name {
                    return Err(CommandError::NothingToDo);
                }
                s.name.clone_from(name);
            }
            Command::UseTactic(slot) => {
                let me = self.club()?;
                self.slot_in_range(me, *slot)?;
                if self.teams[me].active == *slot {
                    return Err(CommandError::NothingToDo);
                }
                self.teams[me].active = *slot;
            }
            Command::CopyTactic { from } => {
                let me = self.club()?;
                self.slot_in_range(me, *from)?;
                out.slot = self.teams[me].add_tactic(*from);
                if out.slot.is_none() {
                    return Err(refused("The playbook is full."));
                }
            }
            Command::DeleteTactic(slot) => {
                let me = self.club()?;
                self.slot_in_range(me, *slot)?;
                if self.teams[me].playbook.len() <= 1 {
                    return Err(refused("The playbook needs at least one tactic."));
                }
                self.teams[me].remove_tactic(*slot);
            }
            Command::AnswerSponsor { offer, accept } => {
                let me = self.club()?;
                match self.econ.offers.get(*offer) {
                    Some(o) if o.club != me => return Err(CommandError::OutOfRange("offer")),
                    None => return Err(CommandError::OutOfRange("offer")),
                    Some(o) if !o.open_on(self.day) => return Err(CommandError::NothingToDo),
                    Some(_) => {}
                }
                self.answer_sponsor(*offer, *accept);
            }
            Command::Renovate => {
                let me = self.club()?;
                if let Some(why) = self.works_blocked(me, false) {
                    return Err(refused(why));
                }
                out.note = Some(self.renovate(me).map_err(refused)?);
            }
            Command::Expand => {
                let me = self.club()?;
                if let Some(why) = self.works_blocked(me, true) {
                    return Err(refused(why));
                }
                out.note = Some(self.expand(me).map_err(refused)?);
            }
            Command::SetTicketPrice(price) => {
                let me = self.club()?;
                let (low, high) = self.price_range(me);
                if !(low..=high).contains(price) {
                    return Err(CommandError::OutOfRange("ticket price"));
                }
                if self.ticket_price(me) == *price && self.econ.books[me].price == *price {
                    return Err(CommandError::NothingToDo);
                }
                self.econ.books[me].price = *price;
            }
            Command::MoveBudget { to_wages, amount } => {
                let me = self.club()?;
                if *amount == 0 {
                    return Err(CommandError::NothingToDo);
                }
                out.note = Some(self.move_budget(me, *to_wages, *amount).map_err(refused)?);
            }
            Command::UpgradeFacility(kind) => {
                let me = self.club()?;
                out.note = Some(self.upgrade_facility(me, *kind).map_err(refused)?);
            }
            Command::SetMedia { player, level } => {
                let me = self.club()?;
                self.own_player(me, *player)?;
                if self.players[*player as usize].renown.media == *level {
                    return Err(CommandError::NothingToDo);
                }
                self.set_media(*player, *level);
            }
            Command::AnswerAppearance { request, accept } => {
                self.club()?;
                match self.fame.asks.get(*request) {
                    None => return Err(CommandError::OutOfRange("appearance request")),
                    Some(a) if !a.open_on(self.day) => return Err(CommandError::NothingToDo),
                    Some(_) => {}
                }
                if !self.appearance_open(*request) {
                    return Err(CommandError::NothingToDo);
                }
                self.answer_appearance(*request, *accept);
            }
            Command::SetInstructions(instructions) => {
                let me = self.club()?;
                instructions
                    .validate(&self.teams[me].roster)
                    .map_err(refused)?;
                if self.teams[me].instructions == *instructions {
                    return Err(CommandError::NothingToDo);
                }
                self.teams[me].instructions = instructions.clone();
            }
            Command::PlayWithInstructions => out.played = self.play_with_instructions()?,
        }
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::engine::Shout;
    use crate::league::LiveRound;
    use crate::tactics::{Press, Role, RotationSlot};
    use crate::training::Preset;

    /// Answer whatever is waiting on the desk.
    pub(crate) fn tidy(w: &mut World, n: &mut usize) {
        while let Some(id) = w.needs_answer() {
            let offer = w
                .desk
                .messages
                .iter()
                .find(|m| m.id == id)
                .and_then(|m| m.offer)
                .unwrap();
            *n += 1;
            let accept = n.is_multiple_of(2);
            w.apply(Command::AnswerOffer { offer, accept }).unwrap();
        }
        for i in 0..w.desk.press.len() {
            if w.press_open(i) {
                let choice = (i % 3) as u8;
                w.apply(Command::AnswerPress {
                    question: i,
                    choice,
                })
                .unwrap();
            }
        }
        for i in 0..w.fame.asks.len() {
            if w.appearance_open(i) {
                w.apply(Command::AnswerAppearance {
                    request: i,
                    accept: i % 2 == 0,
                })
                .unwrap();
            }
        }
        let unread: Vec<u32> = w.inbox().iter().filter(|m| !m.read).map(|m| m.id).collect();
        for id in unread {
            w.apply(Command::MarkRead(id)).unwrap();
        }
    }

    /// Coach this round's game by hand, the way the match screen does.
    fn coach_live(w: &mut World) -> Vec<usize> {
        let mut live = w.kickoff(w.watch.unwrap()).unwrap();
        let side = live.side;
        let who = |live: &LiveRound| live.game.players(side);
        live.input(LiveInput::AutoSubs(false));
        while live.game.time() < 200.0 {
            live.step();
        }
        live.input(LiveInput::Timeout);
        let said = live.input(LiveInput::Shout {
            shout: Shout::DemandMore,
            target: None,
        });
        assert_eq!(said.len(), 5);
        // Nothing is queued twice, and a swap that is waved off is logged
        // all the same.
        let players = who(&live);
        let off = players.iter().find(|p| p.on_court).unwrap().id;
        let on = players.iter().find(|p| !p.on_court).unwrap().id;
        live.input(LiveInput::Swap { off, on });
        live.input(LiveInput::CancelSwap(7));
        for _ in 0..300 {
            live.step();
        }
        let target = who(&live).iter().find(|p| p.on_court).map(|p| p.id);
        live.input(LiveInput::Shout {
            shout: Shout::Praise,
            target,
        });
        while live.game.time() < 1100.0 {
            live.step();
        }
        let mut orders = live.game.orders(side);
        orders.pace = 17;
        orders.aggression = 6;
        live.input(LiveInput::Orders {
            orders,
            familiarity: 0.35,
        });
        let players = who(&live);
        let off = players.iter().filter(|p| p.on_court).nth(1).unwrap().id;
        let on = players.iter().filter(|p| !p.on_court).nth(1).unwrap().id;
        live.input(LiveInput::Swap { off, on });
        while live.game.time() < 1800.0 {
            live.step();
        }
        live.input(LiveInput::AutoSubs(true));
        while !live.game.is_over() {
            live.step();
        }
        w.full_time(live)
    }

    fn play_career(w: &mut World) {
        w.apply(Command::TakeCharge(0)).unwrap();
        let me = w.watch.unwrap();

        // The hall and the boards.
        w.apply(Command::AnswerSponsor {
            offer: 0,
            accept: true,
        })
        .unwrap();
        w.apply(Command::Renovate).unwrap();

        // The books: the price (dragged, so one command), a budget move and
        // a facility.
        let now = w.ticket_price(me);
        for p in [now + 1, now + 2, now + 3] {
            w.apply(Command::SetTicketPrice(p)).unwrap();
        }
        w.apply(Command::MoveBudget {
            to_wages: true,
            amount: 5_000,
        })
        .unwrap();
        w.apply(Command::UpgradeFacility(FacilityKind::Medical))
            .unwrap();

        // Tactics: drag a slider (several commands), then more changes.
        let mut t = w.teams[me].playbook[0].tactics.clone();
        for pace in [12, 13, 15] {
            t.pace = pace;
            let tactics = t.clone();
            w.apply(Command::SetTactics { slot: 0, tactics }).unwrap();
        }
        t.aggression = 14;
        t.press = Press::ThreeQuarter;
        w.apply(Command::SetTactics {
            slot: 0,
            tactics: t,
        })
        .unwrap();
        w.apply(Command::RenameTactic {
            slot: 0,
            name: "Main".into(),
        })
        .unwrap();
        let copy = w.apply(Command::CopyTactic { from: 0 }).unwrap().slot;
        assert_eq!(copy, Some(1));
        w.apply(Command::UseTactic(1)).unwrap();

        // Training.
        let roster = w.teams[me].roster.clone();
        for (i, &p) in roster.iter().take(4).enumerate() {
            w.apply(Command::SetTraining {
                player: p,
                focus: TrainingFocus::ALL[i + 1],
                intensity: Intensity::Heavy,
            })
            .unwrap();
        }
        // Media work: dragging the level is one command.
        for level in [MediaLevel::Some, MediaLevel::ALot] {
            w.apply(Command::SetMedia {
                player: roster[0],
                level,
            })
            .unwrap();
        }
        w.apply(Command::SetMedia {
            player: roster[1],
            level: MediaLevel::Some,
        })
        .unwrap();
        w.apply(Command::SetSchedule(Schedule::preset(Preset::ALL[1])))
            .unwrap();
        w.apply(Command::DelegateTraining(false)).unwrap();
        w.apply(Command::AskAssistantToPlan).unwrap();

        // The market.
        let other = w.teams[1].roster[0];
        w.apply(Command::Shortlist {
            player: other,
            on: true,
        })
        .unwrap();
        let _ = w.apply(Command::Hire {
            job: Job::Scout,
            candidate: 0,
        });
        let target = w.teams[2].roster[1];
        let _ = w.apply(Command::Bid {
            player: target,
            fee: 100_000,
        });

        let mut n = 0;
        let mut rounds = 0;
        for _ in 0..2000 {
            if rounds >= 12 {
                break;
            }
            tidy(w, &mut n);
            match w.apply(Command::Advance).unwrap().stop.unwrap() {
                Stop::MatchDay => {
                    let played = if rounds == 3 || rounds == 8 {
                        coach_live(w)
                    } else {
                        w.apply(Command::PlayRound).unwrap().played
                    };
                    assert!(!played.is_empty());
                    rounds += 1;
                }
                Stop::Mail => {}
                Stop::Draft => {
                    let _ = w.apply(Command::DraftAuto { all: true });
                }
            }
        }
        tidy(w, &mut n);
    }

    #[test]
    fn replaying_the_log_rebuilds_the_world() {
        let mut first = World::generate(7, 8);
        play_career(&mut first);
        assert!(first.round >= 12);
        assert!(first.log.len() > 20);
        assert!(first.log.contains(&Command::AskAssistantToPlan));
        assert!(first.log.contains(&Command::Renovate));
        assert!(
            first
                .log
                .contains(&Command::UpgradeFacility(FacilityKind::Medical))
        );
        assert!(
            first
                .log
                .iter()
                .any(|c| matches!(c, Command::MoveBudget { to_wages: true, .. }))
        );
        // Dragging the price left one command, not three.
        let prices: Vec<_> = first
            .log
            .iter()
            .filter(|c| matches!(c, Command::SetTicketPrice(_)))
            .collect();
        assert_eq!(prices.len(), 1);
        assert!(
            first
                .log
                .iter()
                .any(|c| matches!(c, Command::AnswerSponsor { accept: true, .. }))
        );
        // The media level dragged on one player left one command, and an
        // appearance request was answered.
        let media = first
            .log
            .iter()
            .filter(|c| matches!(c, Command::SetMedia { player, .. } if *player == first.teams[0].roster[0]))
            .count();
        assert!(media <= 1, "{media} commands for one player");
        assert!(
            first
                .log
                .iter()
                .any(|c| matches!(c, Command::SetMedia { .. }))
        );
        assert!(
            first
                .log
                .iter()
                .any(|c| matches!(c, Command::AnswerAppearance { accept: true, .. }))
        );
        // Dragging the pace slider left one command, not three.
        let sets = first
            .log
            .iter()
            .filter(|c| matches!(c, Command::SetTactics { slot: 0, .. }))
            .count();
        assert_eq!(sets, 1);
        // Two games were coached live, and are in the log with what was done.
        let live: Vec<_> = first
            .log
            .iter()
            .filter_map(|c| match c {
                Command::PlayLive { inputs } => Some(inputs),
                _ => None,
            })
            .collect();
        assert_eq!(live.len(), 2);
        assert_eq!(live[0].len(), 9);
        assert!(live[0].windows(2).all(|w| w[0].0 <= w[1].0));
        assert!(live[0].iter().map(|i| i.0).max() > Some(10_000));

        let mut second = World::generate(7, 8);
        for cmd in first.log.clone() {
            second.apply(cmd).unwrap();
        }
        assert_eq!(second.log, first.log);
        assert_eq!(second.table(), first.table());
        assert_eq!(second.round, first.round);
        assert_eq!(second.day, first.day);
        assert_eq!(second.desk.messages.len(), first.desk.messages.len());
        for (a, b) in first.teams.iter().zip(&second.teams) {
            assert_eq!(a.budget, b.budget);
            assert_eq!(a.roster, b.roster);
        }
        // Every player, in full, and the rest of the world besides.
        assert!(
            format!("{:?}", first.players) == format!("{:?}", second.players),
            "players differ after replay"
        );
        assert!(
            format!("{first:?}") == format!("{second:?}"),
            "worlds differ after replay"
        );
    }

    #[test]
    fn refused_commands_change_nothing_and_are_not_logged() {
        let mut w = World::generate(3, 8);
        // Nobody is in charge yet.
        assert_eq!(w.apply(Command::UseTactic(0)), Err(CommandError::NoClub));
        assert_eq!(
            w.apply(Command::TakeCharge(99)),
            Err(CommandError::NoSuchTeam(99))
        );
        assert!(w.log.is_empty());
        w.apply(Command::TakeCharge(0)).unwrap();
        assert_eq!(
            w.apply(Command::TakeCharge(1)),
            Err(refused("A club is already in your charge."))
        );
        assert_eq!(w.teams[0].playbook.len(), 1);
        let logged = w.log.len();
        let before = format!("{w:?}");

        let foreign = w.teams[1].roster[0];
        let mine = w.teams[0].roster[0];
        let tactics = w.teams[0].playbook[0].tactics.clone();
        let mut wrong_rotation = tactics.clone();
        wrong_rotation.rotation.push(RotationSlot {
            player: foreign,
            minutes: 5,
            role: Role::Limited,
            brief: Default::default(),
            position: None,
        });
        let mut wild = tactics.clone();
        wild.pace = 0;
        let mut twice = tactics.clone();
        twice.rotation.push(twice.rotation[0].clone());
        let mut empty = tactics.clone();
        empty.rotation.clear();

        let cases = [
            (
                Command::SetTraining {
                    player: foreign,
                    focus: TrainingFocus::Shooting,
                    intensity: Intensity::Heavy,
                },
                CommandError::NotYourPlayer(foreign),
            ),
            (
                Command::SetTraining {
                    player: u32::MAX,
                    focus: TrainingFocus::Shooting,
                    intensity: Intensity::Heavy,
                },
                CommandError::NoSuchPlayer(u32::MAX),
            ),
            (
                Command::SetDown {
                    player: foreign,
                    on: true,
                },
                CommandError::NotYourPlayer(foreign),
            ),
            (
                Command::SetMedia {
                    player: foreign,
                    level: MediaLevel::ALot,
                },
                CommandError::NotYourPlayer(foreign),
            ),
            (
                Command::SetMedia {
                    player: mine,
                    level: MediaLevel::None,
                },
                CommandError::NothingToDo,
            ),
            (
                Command::AnswerAppearance {
                    request: 3,
                    accept: true,
                },
                CommandError::OutOfRange("appearance request"),
            ),
            (
                Command::SetTactics {
                    slot: 9,
                    tactics: tactics.clone(),
                },
                CommandError::OutOfRange("tactic"),
            ),
            (
                Command::SetTactics {
                    slot: 0,
                    tactics: wrong_rotation,
                },
                CommandError::NotYourPlayer(foreign),
            ),
            (
                Command::SetTactics {
                    slot: 0,
                    tactics: wild,
                },
                CommandError::OutOfRange("slider setting"),
            ),
            (
                Command::SetTactics {
                    slot: 0,
                    tactics: twice,
                },
                refused("A player is in the rotation twice."),
            ),
            (
                Command::SetTactics {
                    slot: 0,
                    tactics: empty,
                },
                refused("The rotation cannot be empty."),
            ),
            // Unchanged values are not commands.
            (
                Command::SetTactics { slot: 0, tactics },
                CommandError::NothingToDo,
            ),
            (
                Command::RenameTactic {
                    slot: 7,
                    name: "x".into(),
                },
                CommandError::OutOfRange("tactic"),
            ),
            (Command::UseTactic(5), CommandError::OutOfRange("tactic")),
            (Command::UseTactic(0), CommandError::NothingToDo),
            (
                Command::DeleteTactic(0),
                refused("The playbook needs at least one tactic."),
            ),
            (
                Command::CopyTactic { from: 4 },
                CommandError::OutOfRange("tactic"),
            ),
            (
                Command::Shortlist {
                    player: foreign,
                    on: false,
                },
                CommandError::NothingToDo,
            ),
            (
                Command::Promote(mine),
                refused("He is not in the reserves."),
            ),
            (
                Command::MarkRead(u32::MAX),
                CommandError::OutOfRange("message"),
            ),
            (
                Command::AnswerOffer {
                    offer: 50,
                    accept: true,
                },
                CommandError::OutOfRange("offer"),
            ),
            (
                Command::AnswerPress {
                    question: 0,
                    choice: 9,
                },
                CommandError::OutOfRange("press question"),
            ),
            (
                Command::Dismiss(Job::HeadCoach),
                refused("The head coach is you."),
            ),
            (
                Command::ProposeTrade(Deal {
                    club: 0,
                    ..Deal::default()
                }),
                CommandError::NoSuchTeam(0),
            ),
            (
                Command::ProposeTrade(Deal {
                    club: 1,
                    give: vec![foreign],
                    ..Deal::default()
                }),
                CommandError::NotYourPlayer(foreign),
            ),
            (
                Command::ProposeTrade(Deal {
                    club: 1,
                    give_picks: vec![usize::MAX],
                    ..Deal::default()
                }),
                CommandError::OutOfRange("pick"),
            ),
            (
                Command::Bid {
                    player: mine,
                    fee: 1000,
                },
                refused("He already plays for you."),
            ),
            (
                Command::DraftPick(foreign),
                refused("It is not your pick, or he is not available."),
            ),
            (Command::DraftAuto { all: true }, CommandError::NothingToDo),
            (
                Command::SetTicketPrice(0),
                CommandError::OutOfRange("ticket price"),
            ),
            (
                Command::SetTicketPrice(10_000),
                CommandError::OutOfRange("ticket price"),
            ),
            (
                Command::MoveBudget {
                    to_wages: true,
                    amount: 0,
                },
                CommandError::NothingToDo,
            ),
            (
                Command::MoveBudget {
                    to_wages: false,
                    amount: -5,
                },
                refused("Pick an amount above zero."),
            ),
        ];
        for (cmd, want) in cases {
            let shown = format!("{cmd:?}");
            assert_eq!(w.apply(cmd), Err(want), "{shown}");
        }
        assert_eq!(w.log.len(), logged);
        assert!(
            format!("{w:?}") == before,
            "a refused command changed the world"
        );
    }
}
