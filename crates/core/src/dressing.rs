//! The dressing room and the press room.
//!
//! A squad has an opinion of its coach: `Team::belief`, 0..=1. It is earned
//! by winning games the squad did not expect to win and lost by losing the
//! ones it did, by playing men where they cannot play, sending out a system
//! nobody has learned, and leaving the best players on the bench. Belief
//! feeds morale every week and morale feeds results, so a coach who keeps
//! getting it wrong loses the room, and one who gets it right has a team
//! that plays above itself. What he says to the press moves both.

use serde::{Deserialize, Serialize};

use crate::engine::GameResult;
use crate::inbox::Kind;
use crate::league::{TeamId, World};
use crate::player::PlayerId;
use crate::rng::Rng;

/// How much a point of difference in the squads' quality changes the odds a
/// squad gives itself, and what the hall is worth to the home side. They are
/// what the game really does (`docs/realism.md`, "League shape"): a squad
/// that expected less than it got would find more belief in every win and
/// the strong would grow stronger.
const EXPECT_PER_POINT: f32 = 0.28;
const EXPECT_HOME: f32 = 0.09;
/// Where belief settles with nothing happening.
const NEUTRAL: f32 = 0.55;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Topic {
    AfterWin,
    AfterLoss,
    BeforeBig,
    /// A wrong call decided the game against us.
    BadCall,
}

/// A question from the press, and what the manager said.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Presser {
    pub round: usize,
    pub topic: Topic,
    /// The player the question turns on: the star of a win, the man who
    /// had a poor night in a defeat.
    pub about: Option<PlayerId>,
    pub options: [String; 3],
    pub answer: Option<u8>,
    /// How it went down, once answered.
    pub outcome: String,
}

/// A promise made before a game, settled by its result.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Pledge {
    pub round: usize,
    /// True: "we will win". False: "nobody expects anything from us".
    pub bold: bool,
}

/// The word for a level of belief.
pub fn standing(belief: f32) -> &'static str {
    match belief {
        b if b >= 0.8 => "They would run through a wall for you",
        b if b >= 0.62 => "The squad believes in you",
        b if b >= 0.45 => "The squad is with you, for now",
        b if b >= 0.3 => "Doubts in the dressing room",
        b if b >= 0.15 => "You are losing the dressing room",
        _ => "The players have stopped listening",
    }
}

impl World {
    /// Mean overall of a club's eight best fit players.
    fn strength(&self, team: TeamId) -> f32 {
        let mut ov: Vec<f32> = self.teams[team]
            .roster
            .iter()
            .map(|&p| &self.players[p as usize])
            .filter(|p| p.fit())
            .map(|p| p.overall())
            .collect();
        ov.sort_by(|a, b| b.total_cmp(a));
        let n = ov.len().clamp(1, 8);
        ov.iter().take(8).sum::<f32>() / n as f32
    }

    /// What a result does to each squad's opinion of its coach. Called
    /// before the game's injuries are known, with the team that played it.
    pub(crate) fn judge_coaches(&mut self, fixture: usize, result: &GameResult) {
        let (home, away) = (self.fixtures[fixture].home, self.fixtures[fixture].away);
        let edge = self.strength(home) - self.strength(away);
        for (side, team) in [home, away].into_iter().enumerate() {
            let sign = if side == 0 { 1.0 } else { -1.0 };
            // What the squad thought its chances were.
            let expected = (0.5 + sign * (edge * EXPECT_PER_POINT + EXPECT_HOME)).clamp(0.1, 0.9);
            let margin = result.score[side] as f32 - result.score[1 - side] as f32;
            let won = if margin > 0.0 { 1.0 } else { 0.0 };
            let mut change = 0.04 * (won - expected) + (margin / 800.0).clamp(-0.02, 0.02);

            // The things players hold against a coach whatever the score.
            let t = &self.teams[team];
            let slot = &t.playbook[t.active];
            if slot.familiarity() < 0.5 {
                change -= 0.015;
            }
            for s in slot.tactics.rotation.iter().take(8) {
                let p = &self.players[s.player as usize];
                if s.position.is_some_and(|pos| p.knows(pos) < 0.6) {
                    change -= 0.008;
                }
            }
            let mut best: Vec<PlayerId> = t
                .roster
                .iter()
                .copied()
                .filter(|&p| self.players[p as usize].fit())
                .collect();
            best.sort_by(|&a, &b| {
                self.players[b as usize]
                    .overall()
                    .total_cmp(&self.players[a as usize].overall())
            });
            for &p in best.iter().take(3) {
                let secs = result.boxes[side]
                    .iter()
                    .find(|(id, _)| *id == p)
                    .map_or(0, |(_, l)| l.secs);
                // A star who sat while the game was lost.
                if secs < 600 && won == 0.0 {
                    change -= 0.02;
                }
            }
            let belief = &mut self.teams[team].belief;
            *belief = (*belief + change).clamp(0.0, 1.0);
        }
    }

    /// The week in the dressing room: belief drifts back toward the middle,
    /// pulls every player's mood with it, and a miserable squad stops
    /// believing. Tells the user when the room changes its mind about him.
    pub(crate) fn dressing_week(&mut self) {
        for team in 0..self.teams.len() {
            let squad = self.teams[team].roster.clone();
            let belief = self.teams[team].belief;
            let mut mood = 0.0;
            for &p in &squad {
                let player = &mut self.players[p as usize];
                player.morale = (player.morale + (belief - 0.5) * 0.035).clamp(0.0, 1.0);
                mood += player.morale;
            }
            mood /= squad.len().max(1) as f32;
            let mut belief = belief + (NEUTRAL - belief) * 0.05;
            if mood < 0.35 {
                belief -= 0.008;
            } else if mood > 0.65 {
                belief += 0.006;
            }
            self.teams[team].belief = belief.clamp(0.0, 1.0);
        }

        let Some(me) = self.watch else { return };
        let belief = self.teams[me].belief;
        let band = match belief {
            b if b >= 0.8 => 4,
            b if b >= 0.45 => 3,
            b if b >= 0.3 => 2,
            b if b >= 0.15 => 1,
            _ => 0,
        };
        let was = self.desk.room;
        self.desk.room = band;
        if band == was || (band == 3 && was > 3) {
            return;
        }
        // The squad's leader speaks for it.
        let captain = self.teams[me]
            .roster
            .iter()
            .copied()
            .max_by_key(|&p| self.players[p as usize].personality.leadership);
        let Some(captain) = captain else { return };
        let name = self.players[captain as usize].name.clone();
        let (subject, body) = match band {
            4 => (
                "The squad is right behind you",
                "The lads asked me to say it: whatever you are doing, keep doing it. Everyone \
                 in that room believes we can beat anybody.",
            ),
            3 => (
                "Things are settling down",
                "For what it is worth, the mood is better. People are listening again.",
            ),
            2 => (
                "Some of the lads have doubts",
                "I will be straight with you. There is talk in the dressing room: about the \
                 results, about who plays and where. It is not a crisis, but they need a \
                 reason to believe. Winning would do it. So would playing people in their \
                 positions, in a system we have trained, with the best men on the floor.",
            ),
            1 => (
                "You are losing the dressing room",
                "I have to tell you that several players no longer trust the way we are being \
                 set up. When you shout from the bench, some of them are not hearing it any \
                 more. It is dragging everyone's mood down, and it will get worse before it \
                 gets better unless results turn.",
            ),
            _ => (
                "The players have stopped listening",
                "It has gone. They are playing for themselves and their next contract. I do \
                 not know what brings this back short of a run of wins nobody sees coming.",
            ),
        };
        self.post(
            self.day,
            Kind::Board,
            &format!("{name}, captain"),
            subject,
            body.into(),
            vec![captain],
        );
    }

    /// Put a question from the press in the inbox.
    fn ask(
        &mut self,
        day: u32,
        topic: Topic,
        about: Option<PlayerId>,
        subject: String,
        body: String,
    ) {
        let who = about.map_or(String::new(), |p| self.players[p as usize].name.clone());
        let options = match topic {
            Topic::AfterWin => [
                "Credit the whole group".to_string(),
                format!("Single out {who}"),
                "Say you expect more than that".to_string(),
            ],
            Topic::AfterLoss => [
                "Take the blame yourself".to_string(),
                "Criticise the performance".to_string(),
                format!("Say {who} let the team down"),
            ],
            Topic::BadCall => [
                "Say nothing about the officials".to_string(),
                "Say, politely, that the calls were wrong".to_string(),
                "Go for the referee".to_string(),
            ],
            Topic::BeforeBig => [
                "\"We are going there to win\"".to_string(),
                "\"We respect them and fear nobody\"".to_string(),
                "\"Nobody expects anything from us\"".to_string(),
            ],
        };
        let i = self.desk.press.len();
        self.desk.press.push(Presser {
            round: self.round,
            topic,
            about,
            options,
            answer: None,
            outcome: String::new(),
        });
        let players = about.into_iter().collect();
        self.post(day, Kind::Media, "The press", subject, body, players)
            .press = Some(i);
    }

    /// The press after the user's game: who won it, or who lost it.
    pub(crate) fn press_after(&mut self, day: u32, fixture: usize) {
        let Some(me) = self.watch else { return };
        let f = &self.fixtures[fixture];
        let Some(r) = &f.result else { return };
        let side = if f.home == me { 0 } else { 1 };
        let opp = self.teams[[f.away, f.home][side]].name.clone();
        let (us, them) = (r.score[side], r.score[1 - side]);
        let settled = r.boxes[side].iter().filter(|(_, l)| l.secs >= 720);
        if us > them {
            let star = settled
                .max_by(|a, b| a.1.game_score().total_cmp(&b.1.game_score()))
                .map(|x| x.0);
            self.ask(
                day,
                Topic::AfterWin,
                star,
                format!("Press: the {us}-{them} win over {opp}"),
                format!(
                    "\"Coach, a {us}-{them} win over {opp}. What did you make of it?\"\n\n\
                     The players will read what you say."
                ),
            );
        } else {
            let poor = settled
                .min_by(|a, b| a.1.rating().total_cmp(&b.1.rating()))
                .map(|x| x.0);
            self.ask(
                day,
                Topic::AfterLoss,
                poor,
                format!("Press: the {us}-{them} defeat to {opp}"),
                format!(
                    "\"Coach, beaten {them}-{us} by {opp}. Where did it go wrong?\"\n\n\
                     The players will read what you say."
                ),
            );
        }
    }

    /// The press after a game a wrong call decided against the user's club:
    /// by the film, the margin was smaller than the points the wrong calls
    /// gave the other side.
    pub(crate) fn press_bad_call(&mut self, day: u32, fixture: usize) {
        let Some(me) = self.watch else { return };
        let f = &self.fixtures[fixture];
        let Some(r) = &f.result else { return };
        let side = if f.home == me { 0 } else { 1 };
        let Some(o) = &r.officiating else { return };
        let Some(winner) = o.decided_for(r.score) else { return };
        if winner as usize == side {
            return;
        }
        let who = o
            .notable
            .iter()
            .filter(|c| c.verdict().is_wrong() && c.gains as usize != side)
            .map(|c| c.by as usize)
            .next()
            .and_then(|b| self.officials.get(o.crew[b.min(2)]))
            .map(|r| r.name.clone())
            .unwrap_or_else(|| "the officials".into());
        let (us, them) = (r.score[side], r.score[1 - side]);
        self.ask(
            day,
            Topic::BadCall,
            None,
            format!("Press: {us}-{them}, and the officials"),
            format!(
                "\"Coach, {them}-{us}, and the replays show some calls that went against you, \
                 {who}'s among them. What do you say about the officiating?\"\n\nThe players, \
                 the supporters and the board will read it, and so may the league office."
            ),
        );
    }

    /// The press before a game against one of the top three.
    pub(crate) fn press_before(&mut self, opp: TeamId, rank: usize) {
        if self.round < 3 || rank > 3 {
            return;
        }
        let name = self.teams[opp].name.clone();
        self.ask(
            self.day,
            Topic::BeforeBig,
            None,
            format!("Press: {name} on Saturday"),
            format!(
                "\"{name} are {} in the table. What do you say to your players before a game \
                 like this?\"\n\nA bold answer lifts the squad if you back it up, and costs you \
                 if you do not.",
                match rank {
                    1 => "top",
                    2 => "second",
                    _ => "third",
                }
            ),
        );
    }

    /// True while a press question is open: it lapses once the next round
    /// has been played.
    pub fn press_open(&self, i: usize) -> bool {
        self.desk
            .press
            .get(i)
            .is_some_and(|p| p.answer.is_none() && self.round <= p.round + 1)
    }

    /// Answer press question `i` with option `choice`.
    pub fn answer_press(&mut self, i: usize, choice: u8) {
        let Some(me) = self.watch else { return };
        if !self.press_open(i) || choice > 2 {
            return;
        }
        let (topic, about, round) = {
            let p = &self.desk.press[i];
            (p.topic, p.about, p.round)
        };
        let belief = self.teams[me].belief;
        let squad = self.teams[me].roster.clone();
        let mut fined: Option<i64> = None;
        let mut shift = 0.0;
        let mut total = 0.0;
        let mut each = |w: &mut World, f: &dyn Fn(&crate::player::Player, bool) -> f32| {
            for &p in &squad {
                let player = &mut w.players[p as usize];
                let d = f(player, Some(p) == about);
                player.morale = (player.morale + d).clamp(0.0, 1.0);
                total += d;
            }
        };
        match (topic, choice) {
            (Topic::AfterWin, 0) => {
                each(self, &|_, _| 0.02);
                shift = 0.005;
            }
            (Topic::AfterWin, 1) => each(self, &|p, him| {
                if him {
                    0.09
                } else if p.personality.ambition >= 14 {
                    -0.02
                } else {
                    0.0
                }
            }),
            (Topic::AfterWin, _) => {
                each(self, &|p, _| {
                    if p.personality.determination >= 12 {
                        0.02
                    } else {
                        -0.015
                    }
                });
                // A coach they believe in can ask for more.
                shift = if belief >= 0.5 { 0.012 } else { -0.012 };
            }
            (Topic::AfterLoss, 0) => {
                each(self, &|_, _| 0.025);
                // Once it sounds like strength, often it sounds like excuses.
                shift = if belief >= 0.4 { 0.006 } else { -0.01 };
            }
            (Topic::AfterLoss, 1) => {
                each(self, &|p, _| {
                    let tough = p.personality.determination + p.personality.professionalism;
                    if tough >= 24 { 0.02 } else { -0.03 }
                });
                shift = if belief >= 0.55 { 0.012 } else { -0.03 };
            }
            (Topic::AfterLoss, _) => {
                each(self, &|p, him| {
                    if !him {
                        -0.012
                    } else if p.personality.temperament >= 13 && p.personality.determination >= 12 {
                        0.03
                    } else {
                        -0.14
                    }
                });
                shift = if belief >= 0.7 { 0.0 } else { -0.025 };
            }
            (Topic::BadCall, 0) => {}
            (Topic::BadCall, 1) => {
                each(self, &|_, _| 0.012);
                self.fans_move(me, crate::fans::C_MOVES, 0.002);
            }
            (Topic::BadCall, _) => {
                // The players like a coach who fights for them; the board
                // and the league office do not like it as much.
                each(self, &|_, _| 0.03);
                shift = if belief >= 0.35 { 0.012 } else { 0.0 };
                self.fans_move(me, crate::fans::C_MOVES, 0.004);
                let (season, round_now) = (self.season, self.round as u16);
                let b = &mut self.club.clubs[me].board;
                b.confidence = (b.confidence - 1.5).clamp(0.0, 100.0);
                b.push_reason(season, round_now, -1.5, "Unhappy that you attacked the referee in public.");
                // The league office may fine him: one time in three.
                let mut rng = Rng::new(self.econ.seed ^ 0x0FF1CE ^ ((self.season as u64) << 24) ^ i as u64);
                if rng.chance(0.34) {
                    let fine = 4_000 * (3 - self.teams[me].division.min(2)) as i64;
                    self.transact(me, -fine, crate::finance::Cat::Other);
                    fined = Some(fine);
                }
            }
            (Topic::BeforeBig, 0) => {
                each(self, &|_, _| 0.015);
                self.desk.pledge = Some(Pledge { round, bold: true });
            }
            (Topic::BeforeBig, 1) => each(self, &|_, _| 0.008),
            (Topic::BeforeBig, _) => {
                each(self, &|p, _| {
                    if p.personality.ambition >= 15 {
                        -0.02
                    } else {
                        0.015
                    }
                });
                self.desk.pledge = Some(Pledge { round, bold: false });
            }
        }
        let b = &mut self.teams[me].belief;
        *b = (*b + shift).clamp(0.0, 1.0);
        let net = total / squad.len().max(1) as f32 + shift;
        let mut outcome = match net {
            n if n >= 0.015 => "It went down well in the dressing room.",
            n if n > 0.0 => "The squad took it quietly; no harm done.",
            n if n > -0.015 => "A few of the players did not like reading that.",
            _ => "That went down badly in the dressing room.",
        }
        .to_string();
        if let Some(p) = about
            && choice > 0
            && topic != Topic::BeforeBig
        {
            let m = self.players[p as usize].morale;
            outcome += &format!(
                " {} is {} now.",
                self.players[p as usize].name,
                self.players[p as usize].mood().to_lowercase()
            );
            let _ = m;
        }
        if let Some(fine) = fined {
            outcome += &format!(" The league office fined the club {}.", crate::inbox::money(fine));
        }
        if let Some(extra) = self.press_fame(topic, choice, about, i) {
            outcome += &extra;
        }
        if self.desk.pledge.is_some_and(|p| p.round == round) && topic == Topic::BeforeBig {
            outcome += " They will hold you to it on Saturday.";
        }
        let p = &mut self.desk.press[i];
        p.answer = Some(choice);
        p.outcome = outcome;
    }

    /// Settle what was said before the game against its result.
    pub(crate) fn settle_pledge(&mut self, round: usize, won: bool) {
        let Some(me) = self.watch else { return };
        let Some(pledge) = self.desk.pledge.filter(|p| p.round == round) else {
            return;
        };
        self.desk.pledge = None;
        self.board_pledge(pledge.bold, won);
        let (belief, mood) = match (pledge.bold, won) {
            (true, true) => (0.05, 0.04),
            (true, false) => (-0.045, -0.02),
            (false, true) => (0.02, 0.03),
            (false, false) => (0.0, 0.0),
        };
        let b = &mut self.teams[me].belief;
        *b = (*b + belief).clamp(0.0, 1.0);
        for p in self.teams[me].roster.clone() {
            let player = &mut self.players[p as usize];
            player.morale = (player.morale + mood).clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Belief follows results against expectation, and a coach who has
    /// lost the room has a worse team for it.
    #[test]
    fn a_losing_coach_loses_the_room() {
        let mut w = World::generate(8, 8);
        // Find the club that does worst and the one that does best.
        while !w.season_over() && w.round < 14 {
            w.play_round();
        }
        let table = w.table();
        let (top, bottom) = (table[0], table[7]);
        assert!(
            w.teams[top].belief > w.teams[bottom].belief + 0.1,
            "top {} bottom {}",
            w.teams[top].belief,
            w.teams[bottom].belief
        );
        let mood = |t: TeamId| {
            let r = &w.teams[t].roster;
            r.iter().map(|&p| w.players[p as usize].morale).sum::<f32>() / r.len() as f32
        };
        assert!(mood(top) > mood(bottom));
        eprintln!(
            "belief top {:.2} bottom {:.2}; morale top {:.2} bottom {:.2}",
            w.teams[top].belief,
            w.teams[bottom].belief,
            mood(top),
            mood(bottom)
        );
    }

    #[test]
    fn the_press_asks_and_the_answer_counts() {
        let mut w = World::generate(8, 8);
        w.take_charge(0);
        w.play_round();
        let i = w.desk.press.len() - 1;
        assert!(w.press_open(i));
        let lost = w.desk.press[i].topic == Topic::AfterLoss;
        let about = w.desk.press[i].about.unwrap();
        let before = w.players[about as usize].morale;
        // Single him out: praise after a win, blame after a defeat.
        w.answer_press(i, if lost { 2 } else { 1 });
        let after = w.players[about as usize].morale;
        assert!(after != before && (after > before) != lost || lost);
        assert!(!w.press_open(i) && !w.desk.press[i].outcome.is_empty());
        w.answer_press(i, 0);
        assert_eq!(w.players[about as usize].morale, after, "one answer only");
    }
}
