//! The manager's desk: a calendar, the mail it delivers, and the transfer
//! business conducted through that mail.
//!
//! A week runs Monday to Sunday with the game on Saturday. Mail is written
//! when its cause happens and dated for the day it should arrive, so the
//! week has a rhythm: report after the game, results on Sunday, training on
//! Monday, offers midweek, the preview on Friday.

use serde::{Deserialize, Serialize};

use crate::engine::BoxLine;
use crate::league::{TeamId, World};
use crate::player::{Player, PlayerId};
use crate::tactics::{DefenseScheme, OffenseSet, Press, Role, RotationSlot, Tactics};

/// Day of the week games are played on, Monday being 0.
pub const MATCH_DAY: u32 = 5;
const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
pub const MIN_ROSTER: usize = 9;
pub const MAX_ROSTER: usize = 15;

/// "Wed, week 3".
pub fn date(day: u32) -> String {
    format!("{}, week {}", DAYS[(day % 7) as usize], day / 7 + 1)
}

pub(crate) fn money(v: i64) -> String {
    let digits = v.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if v < 0 { format!("-{out}") } else { out }
}

fn ordinal(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

fn pct(made: u16, of: u16) -> f32 {
    if of == 0 {
        0.0
    } else {
        made as f32 * 100.0 / of as f32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    Board,
    Match,
    League,
    Training,
    Scouting,
    Transfer,
    Media,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Board,
        Kind::Match,
        Kind::League,
        Kind::Training,
        Kind::Scouting,
        Kind::Transfer,
        Kind::Media,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Board => "Board",
            Kind::Match => "Match",
            Kind::League => "League",
            Kind::Training => "Training",
            Kind::Scouting => "Scouting",
            Kind::Transfer => "Transfers",
            Kind::Media => "Press",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OfferState {
    Open,
    Accepted,
    Rejected,
    /// Nobody answered in time, or the player has moved on.
    Lapsed,
}

/// Another club's offer for one of the user's players.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Offer {
    pub player: PlayerId,
    pub club: TeamId,
    pub fee: i64,
    /// Last day it can be accepted.
    pub expires: u32,
    pub state: OfferState,
}

/// The user's offer for somebody else's player, waiting for an answer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bid {
    pub player: PlayerId,
    pub fee: i64,
    pub answer_on: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: u32,
    /// Day it arrives; later than today means still in the post.
    pub day: u32,
    pub kind: Kind,
    pub from: String,
    pub subject: String,
    pub body: String,
    /// Players the message is about, for links.
    pub players: Vec<PlayerId>,
    /// Index into `Desk::offers` when the message wants an answer.
    pub offer: Option<usize>,
    /// Index into `Desk::press` when it is a question from the press.
    #[serde(default)]
    pub press: Option<usize>,
    /// Index into `Economy::offers` when it is a sponsor's proposal.
    #[serde(default)]
    pub sponsor: Option<usize>,
    /// Index into `FameBook::asks` when it is a sponsor's request for a
    /// player's time.
    #[serde(default)]
    pub appearance: Option<usize>,
    pub read: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Desk {
    pub messages: Vec<Message>,
    pub offers: Vec<Offer>,
    pub bids: Vec<Bid>,
    /// League position the board wants, 1 = champions.
    pub expectation: usize,
    #[serde(default)]
    pub press: Vec<crate::dressing::Presser>,
    #[serde(default)]
    pub pledge: Option<crate::dressing::Pledge>,
    /// The band of dressing-room belief the user was last told about.
    #[serde(default = "settled")]
    pub room: u8,
}

fn settled() -> u8 {
    3
}

/// A proposed trade between the user's club and `club`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Deal {
    pub club: TeamId,
    /// Players the user sends, and those he gets.
    pub give: Vec<PlayerId>,
    pub get: Vec<PlayerId>,
    /// Indices into `World::picks`.
    pub give_picks: Vec<usize>,
    pub get_picks: Vec<usize>,
    /// Cash the user adds; negative if the other club pays.
    pub cash: i64,
}

/// Why `World::advance` stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// There is a game to play today.
    MatchDay,
    /// New mail arrived.
    Mail,
    /// The season is over and the draft is being held.
    Draft,
}

fn hint(offense: OffenseSet, defense: DefenseScheme, press: Press) -> String {
    let d = match defense {
        DefenseScheme::Zone23 => {
            "A 2-3 zone gives up the three and the high post: shooters and ball movement beat it, \
             post-ups do not."
        }
        DefenseScheme::Zone32 => {
            "A 3-2 zone takes away the arc and leaves the baseline and the boards: go inside."
        }
        DefenseScheme::PackLine => {
            "They pack the lane and dare you to shoot. Drives will run into help."
        }
        DefenseScheme::SwitchEverything => {
            "They switch every screen, so hunt the mismatch: a guard on their slowest big."
        }
        DefenseScheme::ManToMan => "Straight man to man. Whoever wins his matchup decides it.",
    };
    let o = match offense {
        OffenseSet::Isolation => {
            " Their offense runs through one man; our best defender should know."
        }
        OffenseSet::PostUp => " They feed the post, so their big men against ours is the game.",
        OffenseSet::PickAndRoll => " They live off the ball screen.",
        OffenseSet::FiveOut => " They spread five shooters around the arc.",
        OffenseSet::Motion => "",
    };
    let p = match press {
        Press::None => "",
        _ => " They press: steady ball handlers on the floor, and expect them to tire late.",
    };
    format!("{d}{o}{p}")
}

impl World {
    /// Put a person in charge of `team`: its games are recorded, and mail
    /// starts arriving.
    pub fn take_charge(&mut self, team: TeamId) {
        self.watch = Some(team);
        self.desk.room = 3;
        // The bench is yours now; the man who had it looks for work.
        if let Some(head) =
            self.teams[team].office.staff[crate::staff::Job::HeadCoach as usize].take()
        {
            self.staff_pool.push(head);
        }
        let strength = |t: TeamId| {
            let mut ov: Vec<f32> = self.teams[t]
                .roster
                .iter()
                .map(|&p| self.players[p as usize].overall())
                .collect();
            ov.sort_by(|a, b| b.total_cmp(a));
            ov.iter().take(8).sum::<f32>()
        };
        let n = self.teams.len();
        let better = (0..n).filter(|&t| strength(t) > strength(team)).count();
        let (target, wish) = if better < 3 {
            (
                3,
                "The squad is among the best in the league and we expect a top-three finish.",
            )
        } else if better < n / 2 {
            (n / 2, "We see this squad in the top half of the table.")
        } else {
            (
                n.saturating_sub(2),
                "Nobody expects miracles. Keep us out of the bottom two.",
            )
        };
        self.desk.expectation = target;
        let name = self.teams[team].name.clone();
        let body = format!(
            "Welcome to {name}.\n\n{wish}\n\nYou have {} to work with and a wage bill of {} a week. \
             Your assistant runs training unless you take it over, and the scouts follow \
             whoever you shortlist. The coaches and scouts you inherit are on the Staff \
             page; better ones can be hired.\n\nThe season is {} rounds, one game every Saturday. \
             Press Continue to move through the week.",
            money(self.teams[team].budget),
            money((self.wages(team) + self.teams[team].office.wages()) / 52),
            self.rounds(),
        );
        self.post(
            self.day,
            Kind::Board,
            "The board",
            "Welcome to the club",
            body,
            vec![],
        );
        self.econ_welcome(team);
        self.preview();
    }

    fn who(&self, p: PlayerId) -> String {
        self.players[p as usize].name.clone()
    }

    fn wages(&self, team: TeamId) -> i64 {
        self.teams[team]
            .roster
            .iter()
            .chain(&self.teams[team].reserves)
            .map(|&p| self.players[p as usize].contract.salary as i64)
            .sum()
    }

    /// Mail that has arrived, newest first.
    pub fn inbox(&self) -> Vec<&Message> {
        let mut out: Vec<&Message> = self
            .desk
            .messages
            .iter()
            .filter(|m| m.day <= self.day)
            .collect();
        out.sort_by_key(|m| std::cmp::Reverse((m.day, m.id)));
        out
    }

    pub fn unread(&self) -> usize {
        self.inbox().iter().filter(|m| !m.read).count()
    }

    /// Id of an arrived message with an offer still waiting for an answer.
    pub fn needs_answer(&self) -> Option<u32> {
        self.inbox()
            .iter()
            .find(|m| {
                m.offer
                    .is_some_and(|i| self.desk.offers[i].state == OfferState::Open)
            })
            .map(|m| m.id)
    }

    pub fn mark_read(&mut self, id: u32) {
        if let Some(m) = self.desk.messages.iter_mut().find(|m| m.id == id) {
            m.read = true;
        }
    }

    pub(crate) fn post(
        &mut self,
        day: u32,
        kind: Kind,
        from: &str,
        subject: impl Into<String>,
        body: String,
        players: Vec<PlayerId>,
    ) -> &mut Message {
        let id = self.desk.messages.len() as u32;
        self.desk.messages.push(Message {
            id,
            day,
            kind,
            from: from.to_string(),
            subject: subject.into(),
            body,
            players,
            offer: None,
            press: None,
            sponsor: None,
            appearance: None,
            read: false,
        });
        self.desk.messages.last_mut().expect("just pushed")
    }

    // ---- transfers -------------------------------------------------------

    /// What a player is worth on the market.
    pub fn value(&self, p: PlayerId) -> i64 {
        let p = &self.players[p as usize];
        let level = (p.overall() - 6.0).max(1.0);
        let age = match p.age {
            0..=23 => 1.3,
            24..=27 => 1.1,
            28..=30 => 0.9,
            31..=33 => 0.6,
            _ => 0.35,
        };
        let upside = 1.0 + (p.potential - p.overall()).max(0.0) * 0.15;
        // The market reads the medical record: a man out for months is
        // worth less today, and one who keeps breaking down less for good.
        let out = p.injury.as_ref().map_or(0, |i| i.weeks_left) as f32;
        let record = p.weeks_lost().min(60) as f32;
        let body = (1.0 - out.min(40.0) * 0.012) * (1.0 - record * 0.004);
        ((level * level * 4000.0 * age * upside * body) as i64 / 1000).max(5) * 1000
    }

    /// True if he is one of the three best players at his club.
    pub(crate) fn is_key(&self, p: PlayerId, club: TeamId) -> bool {
        let mine = self.players[p as usize].overall();
        let better = self.teams[club]
            .roster
            .iter()
            .filter(|&&q| self.players[q as usize].overall() > mine)
            .count();
        better < 3
    }

    pub fn bid_pending(&self, p: PlayerId) -> Option<&Bid> {
        self.desk.bids.iter().find(|b| b.player == p)
    }

    /// Offer `fee` for another club's player. The answer comes by mail in
    /// two days.
    pub fn bid(&mut self, player: PlayerId, fee: i64) -> Result<(), &'static str> {
        let Some(me) = self.watch else {
            return Err("Nobody is in charge.");
        };
        if self.teams[me].roster.contains(&player) {
            return Err("He already plays for you.");
        }
        if self.players[player as usize].is_retired() {
            return Err("He has retired.");
        }
        if self.bid_pending(player).is_some() {
            return Err("You already have an offer in for him.");
        }
        if fee > self.teams[me].budget {
            return Err("The budget does not stretch that far.");
        }
        if self.teams[me].roster.len() >= MAX_ROSTER {
            return Err("The squad is full.");
        }
        if self
            .draft
            .as_ref()
            .is_some_and(|d| d.pool.contains(&player))
        {
            return Err("He is in the draft; pick him there.");
        }
        if self.team_of(player).is_none() {
            // A free agent signs on the spot for his current terms.
            self.teams[me].roster.push(player);
            if self.scouting.is_shortlisted(player) {
                self.scouting.toggle(player);
            }
            self.players[player as usize].contract.years_left = 2;
            self.fans_signed(player, me);
            return Ok(());
        }
        self.desk.bids.push(Bid {
            player,
            fee,
            answer_on: self.day + 2,
        });
        Ok(())
    }

    /// Answer the offer behind a message.
    pub fn answer_offer(&mut self, offer: usize, accept: bool) {
        let Some(me) = self.watch else { return };
        let Some(o) = self.desk.offers.get(offer).cloned() else {
            return;
        };
        if o.state != OfferState::Open {
            return;
        }
        let name = self.players[o.player as usize].name.clone();
        let club = self.teams[o.club].name.clone();
        if !accept {
            self.desk.offers[offer].state = OfferState::Rejected;
            return;
        }
        if !self.teams[me].roster.contains(&o.player) || self.teams[me].roster.len() <= MIN_ROSTER {
            self.desk.offers[offer].state = OfferState::Lapsed;
            return;
        }
        self.desk.offers[offer].state = OfferState::Accepted;
        self.transfer(o.player, o.club, o.fee);
        let body = format!(
            "{name} has joined {club} for {}. The money is in the budget, which now stands at {}.",
            money(o.fee),
            money(self.teams[me].budget),
        );
        self.post(
            self.day,
            Kind::Transfer,
            &club,
            format!("{name} sold for {}", money(o.fee)),
            body,
            vec![o.player],
        )
        .read = true;
    }

    /// Move a player to `to` for `fee` and repair both clubs' rotations.
    pub(crate) fn transfer(&mut self, player: PlayerId, to: TeamId, fee: i64) {
        let Some(from) = self.team_of(player) else {
            return;
        };
        self.fans_sold(player, from);
        self.fans_signed(player, to);
        self.teams[from].roster.retain(|&p| p != player);
        self.teams[from].reserves.retain(|&p| p != player);
        self.teams[from].down.retain(|&p| p != player);
        self.teams[to].roster.push(player);
        self.transact(from, fee, crate::finance::Cat::TransferIn);
        self.transact(to, -fee, crate::finance::Cat::TransferOut);
        self.book_fee(from, to, fee);
        if self.watch == Some(to) && self.scouting.is_shortlisted(player) {
            self.scouting.toggle(player);
        }
        self.refit(from);
        self.refit(to);
    }

    /// What a group of players is worth to a club receiving them. Three
    /// useful players are not one star: each further piece counts for less.
    fn package(&self, club: TeamId, players: &[PlayerId], premium: bool) -> i64 {
        let mut values: Vec<i64> = players
            .iter()
            .map(|&p| {
                let v = self.value_to(club, p);
                // Nobody gives up one of his three best at face value.
                if premium && self.is_key(p, club) {
                    v * 14 / 10
                } else {
                    v
                }
            })
            .collect();
        values.sort_by_key(|&v| std::cmp::Reverse(v));
        values
            .iter()
            .zip([100, 80, 60, 50, 40, 40, 40, 40])
            .map(|(v, weight)| v * weight / 100)
            .sum()
    }

    /// How `club` would answer a trade: the user's `give` and `cash`
    /// (negative if the club pays) for the club's `get`. `Ok` carries how
    /// they see it; `Err` says why not.
    pub fn trade_verdict(&self, deal: &Deal) -> Result<String, String> {
        let (club, give, get, cash) = (deal.club, &deal.give[..], &deal.get[..], deal.cash);
        let Some(me) = self.watch else {
            return Err("Nobody is in charge.".to_string());
        };
        let (mine, theirs) = (&self.teams[me], &self.teams[club]);
        if club == me {
            return Err("Pick another club.".to_string());
        }
        if club >= self.teams.len() {
            return Err("Pick a club.".to_string());
        }
        if give.is_empty()
            && get.is_empty()
            && deal.give_picks.is_empty()
            && deal.get_picks.is_empty()
        {
            return Err("Put players or picks into the deal.".to_string());
        }
        let held = |picks: &[usize], by: TeamId| {
            picks
                .iter()
                .all(|&i| self.picks.get(i).is_some_and(|p| p.owner == by))
        };
        if !held(&deal.give_picks, me) || !held(&deal.get_picks, club) {
            return Err("One of those picks has changed hands.".to_string());
        }
        if self.draft.is_some() {
            return Err("No trades while the draft is being held.".to_string());
        }
        if give.iter().any(|p| !mine.roster.contains(p))
            || get.iter().any(|p| !theirs.roster.contains(p))
        {
            return Err("Somebody in the deal has moved on.".to_string());
        }
        let size = |now: usize, out: usize, back: usize| now - out + back;
        let (my_size, their_size) = (
            size(mine.roster.len(), give.len(), get.len()),
            size(theirs.roster.len(), get.len(), give.len()),
        );
        if my_size < MIN_ROSTER {
            return Err(format!(
                "That leaves us with {my_size} players; we need {MIN_ROSTER}."
            ));
        }
        if my_size > MAX_ROSTER {
            return Err(format!(
                "That gives us {my_size} players; the limit is {MAX_ROSTER}."
            ));
        }
        if !(MIN_ROSTER..=MAX_ROSTER).contains(&their_size) {
            return Err(format!(
                "{} would end up with {their_size} players, which the league does not allow.",
                theirs.name
            ));
        }
        if cash > mine.budget {
            return Err("We do not have that much money.".to_string());
        }
        if -cash > theirs.budget {
            return Err(format!("{} do not have that much money.", theirs.name));
        }
        if cash > self.transfer_left(me) {
            return Err(format!(
                "That much cash is over our transfer budget: {} left.",
                money(self.transfer_left(me).max(0))
            ));
        }
        if -cash > self.transfer_left(club) {
            return Err(format!(
                "{} cannot spend that much on transfers this season.",
                theirs.name
            ));
        }
        // They will not strip a position bare.
        for pos in crate::player::Position::ALL {
            let at = |p: &PlayerId| self.players[*p as usize].position == pos;
            let left = theirs
                .roster
                .iter()
                .filter(|p| at(p) && !get.contains(p))
                .count()
                + give.iter().filter(|p| at(p)).count();
            if left == 0 {
                return Err(format!(
                    "{} would be left without a {}.",
                    theirs.name,
                    pos.label()
                ));
            }
        }
        // Over the cap, a club has to send out about what it takes in.
        let salary = |players: &[PlayerId]| -> i64 {
            players
                .iter()
                .map(|&p| self.players[p as usize].contract.salary as i64)
                .sum()
        };
        let (out, back) = (salary(give), salary(get));
        // The wage budgets: nobody takes on more than the board lets them.
        self.wage_check(me, back - out)
            .map_err(|e| format!("The board says no. {e}"))?;
        if self.wage_check(club, out - back).is_err() {
            return Err(format!(
                "{} cannot fit the extra wages into their budget.",
                theirs.name
            ));
        }
        for (team, sends, takes) in [(me, out, back), (club, back, out)] {
            let after = self.payroll(team) - sends + takes;
            if after > crate::season::SALARY_CAP && takes > sends * 5 / 4 + 20_000 {
                return Err(format!(
                    "Salary cap: {} would have a payroll of {} against a cap of {}. Over the \
                     cap a club can take back 125% of the salary it sends out plus 20,000; \
                     this takes {} for {}.",
                    self.teams[team].name,
                    money(after),
                    money(crate::season::SALARY_CAP),
                    money(takes),
                    money(sends)
                ));
            }
        }
        // All of it as their general manager sees it.
        let picks = |list: &[usize]| {
            list.iter()
                .map(|&i| self.pick_value_to(club, i))
                .sum::<i64>()
        };
        let coming = self.package(club, give, false) + picks(&deal.give_picks) + cash;
        let going = self.package(club, get, true)
            + picks(&deal.get_picks)
            + self.tax_felt(club, out - back);
        // They want to come out a little ahead for their trouble.
        let wanted = going + going * theirs.office.gm.margin() / 100;
        if coming >= wanted {
            Ok(format!(
                "{} would do this. They value what they get at {} and what they give at {}.",
                theirs.name,
                money(coming),
                money(going)
            ))
        } else {
            Err(format!(
                "{} say no: they value what they get at {} and what they give at {}. About {} \
                 more in players or cash would do it.",
                theirs.name,
                money(coming),
                money(going),
                money((wanted - coming + 999) / 1000 * 1000)
            ))
        }
    }

    /// Carry out a trade `trade_verdict` approves of.
    pub fn trade(&mut self, deal: &Deal) -> Result<(), String> {
        self.trade_verdict(deal)?;
        let (club, give, get, cash) = (deal.club, &deal.give[..], &deal.get[..], deal.cash);
        let Some(me) = self.watch else {
            return Ok(());
        };
        for (players, from, to) in [(give, me, club), (get, club, me)] {
            for &p in players {
                self.fans_sold(p, from);
                self.fans_signed(p, to);
                self.teams[from].roster.retain(|&q| q != p);
                self.teams[to].roster.push(p);
                if to == me && self.scouting.is_shortlisted(p) {
                    self.scouting.toggle(p);
                }
            }
        }
        for (list, to) in [(&deal.give_picks, club), (&deal.get_picks, me)] {
            for &i in list {
                self.picks[i].owner = to;
            }
        }
        let pick_names = |list: &[usize]| {
            list.iter()
                .map(|&i| format!("the {}", self.pick_label(i).replace(", ", " pick ")))
                .collect::<Vec<_>>()
        };
        let (picks_out, picks_in) = (pick_names(&deal.give_picks), pick_names(&deal.get_picks));
        self.transact(me, -cash, crate::finance::Cat::TradeCash);
        self.transact(club, cash, crate::finance::Cat::TradeCash);
        self.book_trade_cash(me, club, cash);
        self.desk.bids.retain(|b| !get.contains(&b.player));
        self.refit(me);
        self.refit(club);

        let names = |players: &[PlayerId], picks: &[String]| {
            let all: Vec<String> = players
                .iter()
                .map(|&p| self.who(p))
                .chain(picks.iter().cloned())
                .collect();
            if all.is_empty() {
                "nothing".to_string()
            } else {
                all.join(", ")
            }
        };
        let club_name = self.teams[club].name.clone();
        let mut body = format!(
            "The trade with {club_name} is done.\n\nWe get: {}.\nThey get: {}.",
            names(get, &picks_in),
            names(give, &picks_out)
        );
        if cash != 0 {
            body += &format!(
                "\n{} {} in cash.",
                if cash > 0 { "We pay" } else { "They pay" },
                money(cash.abs())
            );
        }
        if !get.is_empty() {
            body += "\n\nThe new players are not in the rotation unless a place opened up: \
                     check the Tactics screen.";
        }
        let players = get.iter().chain(give).copied().collect();
        self.post(
            self.day,
            Kind::Transfer,
            &club_name,
            format!("Trade with {club_name} completed"),
            body,
            players,
        );
        Ok(())
    }

    /// Make a club's rotations match its roster again. The user's own
    /// choices are kept as far as they still make sense; other clubs let
    /// their coach redo the rotation.
    pub(crate) fn refit(&mut self, team: TeamId) {
        let roster = self.teams[team].roster.clone();
        if self.watch != Some(team) {
            let mut refs: Vec<&Player> = roster
                .iter()
                .map(|&p| &self.players[p as usize])
                .filter(|p| p.fit())
                .collect();
            if refs.len() < 5 {
                refs = roster.iter().map(|&p| &self.players[p as usize]).collect();
            }
            let rotation = Tactics::auto(&refs).rotation;
            for slot in &mut self.teams[team].playbook {
                slot.tactics.rotation = rotation.clone();
            }
            return;
        }
        let mut spare = roster.clone();
        spare.sort_by(|&a, &b| {
            self.players[b as usize]
                .overall()
                .total_cmp(&self.players[a as usize].overall())
        });
        for slot in &mut self.teams[team].playbook {
            let rotation = &mut slot.tactics.rotation;
            rotation.retain(|s| roster.contains(&s.player));
            let want = roster.len().min(8);
            for &p in &spare {
                if rotation.len() >= want {
                    break;
                }
                if !rotation.iter().any(|s| s.player == p) && self.players[p as usize].fit() {
                    rotation.push(RotationSlot {
                        player: p,
                        minutes: 10,
                        role: Role::RolePlayer,
                        brief: Default::default(),
                        position: None,
                    });
                }
            }
        }
    }

    /// Things that happen because a day has passed.
    pub(crate) fn daily(&mut self) {
        let day = self.day;
        for o in &mut self.desk.offers {
            if o.state == OfferState::Open && o.expires < day {
                o.state = OfferState::Lapsed;
            }
        }
        let due: Vec<Bid> = self
            .desk
            .bids
            .iter()
            .filter(|b| b.answer_on <= day)
            .cloned()
            .collect();
        self.desk.bids.retain(|b| b.answer_on > day);
        for bid in due {
            self.answer_bid(bid);
        }
        self.fame_daily();
    }

    fn answer_bid(&mut self, bid: Bid) {
        let (Some(me), Some(owner)) = (self.watch, self.team_of(bid.player)) else {
            return;
        };
        if owner == me {
            return;
        }
        let name = self.players[bid.player as usize].name.clone();
        let club = self.teams[owner].name.clone();
        let value = self.value_to(owner, bid.player);
        let key = self.is_key(bid.player, owner);
        let mut ask = value * if key { 14 } else { 10 } / 10
            + value * self.teams[owner].office.gm.margin() / 100;
        let listed = self.is_listed(bid.player);
        if listed {
            ask = ask.min(self.listed_price(bid.player));
        }
        let (subject, body) = if self.teams[owner].roster.len() <= MIN_ROSTER {
            (
                format!("{name} is not for sale"),
                format!("{club} are too short of players to let {name} go at any price."),
            )
        } else if bid.fee < ask {
            (
                format!("Offer for {name} turned down"),
                format!(
                    "{club} have turned down {} for {name}.{} Our scouts think {} would get \
                     their attention.",
                    money(bid.fee),
                    if key {
                        " He is one of their best players and they do not want to lose him."
                    } else {
                        ""
                    },
                    money((ask + 4999) / 5000 * 5000),
                ),
            )
        } else if bid.fee > self.teams[me].budget
            || self.teams[me].roster.len() >= MAX_ROSTER
            || self.fee_check(me, bid.fee).is_err()
            || self
                .wage_check(me, self.players[bid.player as usize].contract.salary as i64)
                .is_err()
        {
            (
                format!("Deal for {name} is off"),
                format!(
                    "{club} accepted {} for {name}, but we no longer have the money, the \
                     budget or the squad place.",
                    money(bid.fee)
                ),
            )
        } else {
            self.transfer(bid.player, me, bid.fee);
            let p = &self.players[bid.player as usize];
            (
                format!("{name} signs"),
                format!(
                    "{club} accepted {} and {name} is ours. He is on {} a year for {} more \
                     years. He is not in the rotation yet: add him on the Tactics screen.",
                    money(bid.fee),
                    money(p.contract.salary as i64),
                    p.contract.years_left,
                ),
            )
        };
        self.post(
            self.day,
            Kind::Transfer,
            &club,
            subject,
            body,
            vec![bid.player],
        );
    }

    // ---- the weekly mail -------------------------------------------------

    /// Friday's note on Saturday's opponent.
    pub fn preview(&mut self) {
        let Some(me) = self.watch else { return };
        let round = self.round;
        let Some(f) = self
            .fixtures
            .iter()
            .find(|f| f.round == round && (f.home == me || f.away == me))
        else {
            return;
        };
        let (opp, home) = if f.home == me {
            (f.away, true)
        } else {
            (f.home, false)
        };
        let team = &self.teams[opp];
        let t = team.tactics();
        let s = &self.standings[opp];
        let rank = self.table().iter().position(|&x| x == opp).unwrap_or(0) + 1;
        let per_game = |p: PlayerId| {
            self.season_stats[p as usize].pts as f32 / self.games_played[p as usize].max(1) as f32
        };
        let danger = team.roster.iter().copied().max_by(|&a, &b| {
            if round == 0 {
                self.players[a as usize]
                    .overall()
                    .total_cmp(&self.players[b as usize].overall())
            } else {
                per_game(a).total_cmp(&per_game(b))
            }
        });
        let mut body = format!(
            "Saturday we are {} {}.\n\n",
            if home { "at home to" } else { "away at" },
            team.name
        );
        if round > 0 {
            body += &format!(
                "They are {} with a {}-{} record, scoring {:.1} and allowing {:.1} a game.\n\n",
                ordinal(rank),
                s.wins,
                s.losses,
                s.points_for as f32 / (s.wins + s.losses).max(1) as f32,
                s.points_against as f32 / (s.wins + s.losses).max(1) as f32,
            );
        }
        body += &format!(
            "They run {} on offense and defend {}{}.\n\n{}\n\n",
            t.offense.label().to_lowercase(),
            t.defense.label().to_lowercase(),
            match t.press {
                Press::None => String::new(),
                p => format!(", with a {}", p.label().to_lowercase()),
            },
            hint(t.offense, t.defense, t.press),
        );
        let mut players = Vec::new();
        if let Some(d) = danger {
            let p = &self.players[d as usize];
            body += &if round == 0 {
                format!(
                    "Their best player on paper is {} ({}).",
                    p.name,
                    p.position.label()
                )
            } else {
                format!(
                    "The man to stop is {} ({}), {:.1} points a game.",
                    p.name,
                    p.position.label(),
                    per_game(d)
                )
            };
            players.push(d);
        }
        // Signature moves our scouts know about, with the answer to each.
        let mut seen = Vec::new();
        for &pid in &team.roster {
            let p = &self.players[pid as usize];
            for s in self.known_signatures(pid) {
                seen.push(format!(
                    "{}, {}. {} The answer: {}",
                    p.name,
                    s.name().to_lowercase(),
                    s.description(),
                    s.answer()
                ));
                if !players.contains(&pid) && players.len() < 5 {
                    players.push(pid);
                }
            }
        }
        if !seen.is_empty() {
            body += &format!("\n\nSignature moves we know about:\n{}", seen.join("\n"));
        }
        let mine = &self.teams[me].playbook[self.teams[me].active];
        body += &format!(
            "\n\nWe go in with {}, which the squad knows {:.0}%.",
            mine.name,
            mine.familiarity() * 100.0
        );
        let subject = format!(
            "Next up: {} ({})",
            team.name,
            if home { "home" } else { "away" }
        );
        let day = (self.match_day() - 1).max(self.day);
        self.post(day, Kind::Match, "Assistant coach", subject, body, players);
        self.press_before(opp, rank);
    }

    /// What a block of games puts in the post at once: the manager's match
    /// report and the press's questions about it.
    pub(crate) fn after_games(&mut self, played: &[usize]) {
        let Some(me) = self.watch else { return };
        let best = |lines: &[(PlayerId, BoxLine)]| {
            lines
                .iter()
                .max_by(|a, b| a.1.game_score().total_cmp(&b.1.game_score()))
                .copied()
        };
        let line_text =
            |l: &BoxLine| format!("{} points, {} rebounds, {} assists", l.pts, l.reb(), l.ast);
        // The match report.
        let mine = played.iter().copied().find(|&i| {
            let f = &self.fixtures[i];
            f.home == me || f.away == me
        });
        if let Some(i) = mine
            && let Some(r) = self.fixtures[i].result.clone()
        {
            let f = &self.fixtures[i];
            let gameday = f.day;
            let side = if f.home == me { 0 } else { 1 };
            let opp = self.teams[[f.away, f.home][side]].name.clone();
            let (us, them) = (r.score[side], r.score[1 - side]);
            let mut total = BoxLine::default();
            for (_, l) in &r.boxes[side] {
                total.add(l);
            }
            let periods: Vec<String> = r
                .periods
                .iter()
                .map(|p| format!("{}-{}", p[side], p[1 - side]))
                .collect();
            let mut body = format!(
                "{} {} - {} {}\nBy period, us first: {}\n\n",
                self.teams[f.home].name,
                r.score[0],
                r.score[1],
                self.teams[f.away].name,
                periods.join(", "),
            );
            body += &format!(
                "We shot {:.0}% from the field and {:.0}% from three ({} of {}), went to the \
                 line {} times and turned it over {}.\n\n",
                pct(total.fgm, total.fga),
                pct(total.tpm, total.tpa),
                total.tpm,
                total.tpa,
                total.fta,
                total.tov,
            );
            let mut players = Vec::new();
            if let Some((p, l)) = best(&r.boxes[side]) {
                body += &format!(
                    "Our best was {}, rated {:.1}: {}.\n",
                    self.who(p),
                    l.rating(),
                    line_text(&l)
                );
                players.push(p);
            }
            if let Some((p, l)) = best(&r.boxes[1 - side]) {
                body += &format!(
                    "For them {} did the damage: {}.\n",
                    self.who(p),
                    line_text(&l)
                );
                players.push(p);
            }
            if let Some(line) = self.gate_line(i) {
                body += &line;
            }
            body += "\nThe full replay and box score are on the Match screen.";
            let subject = format!(
                "{} {us}-{them} {} {opp}",
                if us > them { "Won" } else { "Lost" },
                if side == 0 { "at home to" } else { "away at" },
            );
            self.post(
                gameday,
                Kind::Match,
                "Assistant coach",
                subject,
                body,
                players,
            );
        }

        if let Some(i) = mine
            && let Some(r) = &self.fixtures[i].result
        {
            let side = if self.fixtures[i].home == me { 0 } else { 1 };
            let won = r.score[side] > r.score[1 - side];
            let (round, day) = (self.fixtures[i].round, self.fixtures[i].day);
            self.settle_pledge(round, won);
            self.press_after(day, i);
        }
    }

    /// Everything a played round puts in the post. `played` are the round's
    /// fixtures, `before` the user's players' overall before the week's
    /// training.
    pub(crate) fn after_round(&mut self, played: &[usize], before: &[(PlayerId, f32)]) {
        let round = self.round - 1;
        let today = self.round_base(round) + MATCH_DAY;
        let Some(me) = self.watch else { return };
        let best = |lines: &[(PlayerId, BoxLine)]| {
            lines
                .iter()
                .max_by(|a, b| a.1.game_score().total_cmp(&b.1.game_score()))
                .copied()
        };
        let line_text =
            |l: &BoxLine| format!("{} points, {} rebounds, {} assists", l.pts, l.reb(), l.ast);
        // Sunday: the round around the league.
        let table = self.table();
        let place = table.iter().position(|&t| t == me).unwrap_or(0) + 1;
        let mut body = String::new();
        let mut star: Option<(PlayerId, BoxLine, TeamId)> = None;
        for &i in played {
            let f = &self.fixtures[i];
            let Some(r) = &f.result else { continue };
            body += &format!(
                "{} {} - {} {}\n",
                self.teams[f.home].name, r.score[0], r.score[1], self.teams[f.away].name
            );
            for (side, club) in [f.home, f.away].into_iter().enumerate() {
                if let Some((p, l)) = best(&r.boxes[side])
                    && star.is_none_or(|s| l.game_score() > s.1.game_score())
                {
                    star = Some((p, l, club));
                }
            }
        }
        let mut players = Vec::new();
        if let Some((p, l, club)) = star {
            body += &format!(
                "\nPlayer of the round: {} ({}), {}.\n",
                self.who(p),
                self.teams[club].name,
                line_text(&l)
            );
            players.push(p);
        }
        body += "\nTable\n";
        for (n, &t) in table.iter().enumerate() {
            if n < 5 || t == me {
                let s = &self.standings[t];
                body += &format!(
                    "{:>2}. {}  {}-{}{}\n",
                    n + 1,
                    self.teams[t].name,
                    s.wins,
                    s.losses,
                    if t == me { "  (us)" } else { "" }
                );
            }
        }
        let subject = format!("Round {} results: we are {}", round + 1, ordinal(place));
        self.post(
            today + 1,
            Kind::League,
            "League office",
            subject,
            body,
            players,
        );

        // Monday: how the week's training went.
        let mut moved: Vec<(PlayerId, f32)> = before
            .iter()
            .map(|&(p, was)| (p, self.players[p as usize].overall() - was))
            .collect();
        moved.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut body = String::new();
        let mut players = Vec::new();
        let up: Vec<String> = moved
            .iter()
            .filter(|m| m.1 > 0.015)
            .take(3)
            .map(|m| {
                players.push(m.0);
                format!("{} ({:+.2})", self.who(m.0), m.1)
            })
            .collect();
        let down: Vec<String> = moved
            .iter()
            .rev()
            .filter(|m| m.1 < -0.015)
            .take(3)
            .map(|m| {
                players.push(m.0);
                format!("{} ({:+.2})", self.who(m.0), m.1)
            })
            .collect();
        body += &if up.is_empty() {
            "Nobody made a visible step this week.\n".to_string()
        } else {
            format!("Coming on: {}.\n", up.join(", "))
        };
        if !down.is_empty() {
            body += &format!("Slipping: {}.\n", down.join(", "));
        }
        let tired: Vec<String> = self.teams[me]
            .roster
            .iter()
            .filter(|&&p| self.players[p as usize].condition < 0.85)
            .map(|&p| {
                format!(
                    "{} ({:.0}%)",
                    self.who(p),
                    self.players[p as usize].condition * 100.0
                )
            })
            .collect();
        if !tired.is_empty() {
            body += &format!(
                "\nShort of fitness: {}. Lighter individual training would bring them back.\n",
                tired.join(", ")
            );
        }
        let team = &self.teams[me];
        let slot = &team.playbook[team.active];
        body += &format!(
            "\nThe squad now knows {} {:.0}%.\n",
            slot.name,
            slot.familiarity() * 100.0
        );
        if team.delegate_training {
            body += "\nMy plan for the coming week:\n";
            for note in &team.assistant_notes {
                body += &format!("- {note}\n");
            }
        } else {
            body += "\nYou are setting the training week yourself.\n";
        }
        let subject = format!("Training report, week {}", round + 1);
        self.post(
            today + 2,
            Kind::Training,
            "Assistant coach",
            subject,
            body,
            players,
        );

        // Monday as well: what the scouts saw.
        let seen: Vec<(PlayerId, String)> = self
            .scouting
            .reports
            .iter()
            .filter(|r| r.round == round)
            .map(|r| {
                let club = self.team_of(r.player).map_or("", |t| &self.teams[t].name);
                let text = format!(
                    "{} ({club}) {} {}: rated {:.1}, {} in {} minutes.",
                    self.who(r.player),
                    if r.home { "at home to" } else { "away at" },
                    self.teams[r.opponent].name,
                    r.rating,
                    line_text(&r.line),
                    r.line.secs / 60,
                );
                (r.player, text)
            })
            .collect();
        if !seen.is_empty() {
            let body = format!(
                "{}\n\nThe full reports are on each player's page.",
                seen.iter()
                    .map(|s| s.1.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            let subject = format!(
                "Scouting: {} shortlisted {} watched",
                seen.len(),
                if seen.len() == 1 { "player" } else { "players" }
            );
            let players = seen.iter().map(|s| s.0).collect();
            self.post(
                today + 2,
                Kind::Scouting,
                "Chief scout",
                subject,
                body,
                players,
            );
        }

        // Midweek: now and then somebody wants one of ours.
        let others = self.teams.len() as u32 - 1;
        let club = (me + 1 + self.rng.range(0, others) as usize) % self.teams.len();
        // A busy general manager calls far more often than a quiet one,
        // and a hard bargainer opens low.
        let gm = self.teams[club].office.gm.clone();
        let keen = 0.12 + gm.activity as f32 * 0.03;
        if self.rng.chance(keen) && self.teams[me].roster.len() > MIN_ROSTER {
            let roster = &self.teams[me].roster;
            let player = roster[self.rng.range(0, roster.len() as u32) as usize];
            let value = self.value(player);
            let theirs = self.value_to(club, player) as f32;
            let opening = 1.0 - (gm.negotiation as f32 - 10.0) * 0.02 + self.rng.f32() * 0.25;
            let fee = (theirs * opening) as i64 / 1000 * 1000;
            let wage = self.players[player as usize].contract.salary as i64;
            if fee <= self.teams[club].budget
                && self.teams[club].roster.len() < MAX_ROSTER
                && self.fee_check(club, fee).is_ok()
                && self.wage_check(club, wage).is_ok()
                && self.tax_felt(club, wage) * 4 < fee
            {
                let expires = today + 4 + 5;
                self.desk.offers.push(Offer {
                    player,
                    club,
                    fee,
                    expires,
                    state: OfferState::Open,
                });
                let p = &self.players[player as usize];
                let body = format!(
                    "{} have offered {} for {}.\n\nHe is {}, a {} rated {:.0} overall, on {} a \
                     year for {} more years. We would put his value at about {}.\n\nThe offer \
                     stands until {}.",
                    self.teams[club].name,
                    money(fee),
                    p.name,
                    p.age,
                    p.position.label(),
                    p.overall(),
                    money(p.contract.salary as i64),
                    p.contract.years_left,
                    money(value),
                    date(expires),
                );
                let subject = format!("Offer for {}: {}", p.name, money(fee));
                let from = self.teams[club].name.clone();
                let offer = self.desk.offers.len() - 1;
                self.post(
                    today + 4,
                    Kind::Transfer,
                    &from,
                    subject,
                    body,
                    vec![player],
                )
                .offer = Some(offer);
            }
        }

        // The board looks in at half way and at the end.
        let (rounds, target) = (self.rounds(), self.desk.expectation);
        if target > 0 && (self.round * 2 == rounds || self.season_over()) {
            let s = &self.standings[me];
            let verdict = if place <= target {
                "That is what we asked for, and the board is pleased."
            } else if place <= target + 2 {
                "That is a little short of what we asked for."
            } else {
                "That is well short of what we asked for, and patience is not endless."
            };
            let (subject, opening) = if self.season_over() {
                (
                    format!("The season is over: {} place", ordinal(place)),
                    format!(
                        "We finished {} with a {}-{} record.",
                        ordinal(place),
                        s.wins,
                        s.losses
                    ),
                )
            } else {
                (
                    "Half-season review".to_string(),
                    format!(
                        "At the half-way mark we are {} with a {}-{} record.",
                        ordinal(place),
                        s.wins,
                        s.losses
                    ),
                )
            };
            let body = format!(
                "{opening} The target was {} or better.\n\n{verdict}\n\nThe budget stands at {}.",
                ordinal(target),
                money(self.teams[me].budget)
            );
            self.post(today + 1, Kind::Board, "The board", subject, body, vec![]);
        }

        self.preview();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A season managed by pressing Continue: mail arrives through the week,
    /// offers can be answered, a bid goes through.
    #[test]
    fn the_week_brings_mail_and_business() {
        let mut w = World::generate(4, 8);
        // Names are priced in elsewhere; this is about the business.
        for p in w.players.iter_mut() {
            p.renown.fame = 0.0;
        }
        w.take_charge(0);
        // The board's wage room is not what this test is about.
        for b in &mut w.econ.books {
            b.wage_budget += 200_000;
        }
        assert_eq!(w.unread(), 1, "the welcome");
        assert_eq!(w.advance(), Stop::Mail, "Friday's preview");
        assert_eq!(w.day, 4);
        assert_eq!(w.advance(), Stop::MatchDay);
        assert_eq!(w.advance(), Stop::MatchDay, "the game has to be played");

        // Bid well over the odds for somebody else's worst player.
        let target = *w.teams[1]
            .roster
            .iter()
            .min_by(|&&a, &&b| {
                w.players[a as usize]
                    .overall()
                    .total_cmp(&w.players[b as usize].overall())
            })
            .unwrap();
        let fee = w.value(target) * 2;
        w.bid(target, fee).unwrap();
        assert!(w.bid(target, fee).is_err(), "one offer at a time");

        let (mut sold, mut kinds) = (0, std::collections::HashSet::new());
        while !w.season_over() {
            w.play_round();
            while w.advance() == Stop::Mail {
                if let Some(id) = w.needs_answer() {
                    let offer = w.desk.messages[id as usize].offer.unwrap();
                    let before = w.teams[0].roster.len();
                    w.answer_offer(offer, sold < 2);
                    if w.teams[0].roster.len() < before {
                        sold += 1;
                    }
                }
            }
            for m in w.inbox() {
                kinds.insert(m.kind);
            }
        }

        let signed = format!("{} signs", w.players[target as usize].name);
        assert!(
            w.inbox().iter().any(|m| m.subject == signed),
            "the bid went through"
        );
        assert!(sold >= 1, "somebody made an offer in fourteen weeks");
        assert_eq!(
            kinds.len(),
            Kind::ALL.len() - 1,
            "no shortlist, no scouting mail"
        );
        assert!(w.needs_answer().is_none());
        // Every club can still field a team after the dealing.
        for t in &w.teams {
            for slot in &t.playbook {
                assert!(slot.tactics.rotation.len() >= 5);
                assert!(
                    slot.tactics
                        .rotation
                        .iter()
                        .all(|s| t.roster.contains(&s.player))
                );
            }
        }
        // Gate, sponsors, broadcast, shop, prizes and running costs are the
        // league's new money (tax only moves between clubs); the rest only moves around.
        let earned: i64 = w
            .econ
            .ledgers
            .iter()
            .map(|l| {
                use crate::finance::Cat;
                l.total(Cat::Gate)
                    + l.total(Cat::Sponsorship)
                    + l.total(Cat::Prize)
                    + l.total(Cat::Upkeep)
                    + l.total(Cat::Broadcast)
                    + l.total(Cat::Merchandise)
                    + l.total(Cat::FacilityRunning)
                    + l.total(Cat::FacilityWorks)
                    + l.total(Cat::Interest)
                    + l.total(Cat::LuxuryTax)
                    + l.total(Cat::TaxShare)
            })
            .sum();
        let total: i64 = w.teams.iter().map(|t| t.budget).sum::<i64>() - earned;
        let wages: i64 = (0..8)
            .map(|t| (w.wages(t) + w.teams[t].office.wages()) / 52 * w.round as i64)
            .sum();
        assert!(
            (total + wages - 8_000_000).abs() < 400_000,
            "fees only move money around: {total} + {wages}"
        );

        // The year ends with the draft; let the assistant handle it.
        w.draft_auto(true);
        assert!(w.draft.is_none());

        // A trade: our worst for their worst is fine, our worst for their
        // best is not, and adding enough cash changes their mind.
        let by_overall = |w: &World, t: usize, best: bool| {
            let ov = |p: &&PlayerId| (w.players[**p as usize].overall() * 100.0) as i32;
            let r = &w.teams[t].roster;
            *if best {
                r.iter().max_by_key(ov)
            } else {
                r.iter().min_by_key(ov)
            }
            .unwrap()
        };
        let (ours, star) = (by_overall(&w, 0, false), by_overall(&w, 2, true));
        let mut deal = Deal {
            club: 2,
            give: vec![ours],
            get: vec![star],
            ..Deal::default()
        };
        assert!(w.trade_verdict(&deal).is_err());
        assert!(w.trade(&deal).is_err());
        let sweetener = w.value(star) * 2;
        w.teams[0].budget = w.teams[0].budget.max(sweetener);
        let sizes = (w.teams[0].roster.len(), w.teams[2].roster.len());
        deal.cash = sweetener;
        // Their first-round pick comes along if we pay for it too.
        let pick = w
            .picks
            .iter()
            .position(|p| p.owner == 2 && p.round == 1)
            .unwrap();
        deal.get_picks = vec![pick];
        deal.cash += w.pick_value(pick) * 2;
        w.teams[0].budget = w.teams[0].budget.max(deal.cash);
        // The board's room is not what this is about.
        for b in &mut w.econ.books {
            b.wage_budget += 500_000;
            b.transfer_budget += 2_000_000;
        }
        w.trade(&deal).unwrap();
        assert_eq!(w.picks[pick].owner, 0);
        assert!(w.teams[0].roster.contains(&star) && w.teams[2].roster.contains(&ours));
        assert_eq!(sizes, (w.teams[0].roster.len(), w.teams[2].roster.len()));
        assert!(w.trade_verdict(&Deal::default()).is_err());
    }
}

#[cfg(test)]
mod seasons {
    use super::*;

    /// Two seasons end to end: the draft is held, the user picks, rosters
    /// stay legal and the next season is playable.
    #[test]
    fn the_draft_leads_into_the_next_season() {
        let mut w = World::generate(9, 8);
        w.take_charge(3);
        let players = w.players.len();
        for season in 0..2 {
            loop {
                match w.advance() {
                    Stop::MatchDay => {
                        w.play_round();
                    }
                    Stop::Mail => {}
                    Stop::Draft => break,
                }
            }
            assert_eq!(w.season, season);
            let draft = w.draft.as_ref().expect("the draft is open");
            assert_eq!(draft.on_clock(), Some(3), "it stops at our pick");
            let before = w.teams[3].roster.len();
            if season == 0 {
                let pick = draft.pool[0];
                w.draft_take(pick);
                assert!(w.teams[3].roster.contains(&pick));
                assert!(w.draft.is_some(), "our second-round pick is still to come");
            }
            w.draft_auto(true);
            assert!(w.draft.is_none());
            assert_eq!(w.season, season + 1);
            assert!(w.teams[3].roster.len() >= before.min(MAX_ROSTER - 1));
            assert_eq!(w.round, 0);
            assert_eq!(w.picks.len(), 8 * 2 * 3);
            assert!(w.picks.iter().all(|p| p.year >= w.season));
            for t in &w.teams {
                assert!((MIN_ROSTER..=MAX_ROSTER).contains(&t.roster.len()));
                for slot in &t.playbook {
                    assert!(slot.tactics.rotation.len() >= 5);
                    assert!(
                        slot.tactics
                            .rotation
                            .iter()
                            .all(|s| t.roster.contains(&s.player))
                    );
                }
            }
        }
        // Two draft classes and two reserve intakes of two a club.
        assert_eq!(w.players.len(), players + 2 * (16 + 6) + 2 * 8 * 2);
        assert_eq!(w.history.len(), 2);
        // Careers were kept: a veteran of both seasons has two lines that
        // add up to games he played, and the club totals are on record.
        let veteran = w.teams[0]
            .roster
            .iter()
            .copied()
            .find(|&p| w.career(p).len() == 2);
        let lines = w.career(veteran.expect("somebody stayed two years"));
        assert_eq!((lines[0].season, lines[1].season), (0, 1));
        assert_eq!(lines[1].age, lines[0].age + 1);
        assert!(
            lines
                .iter()
                .all(|l| l.games <= 14 && l.totals.secs > 0 || l.games == 0)
        );
        let record = &w.history[1];
        assert_eq!(record.stats[record.champion].games, 14);
        assert!(
            record.place(record.champion) <= 4,
            "champions came through the playoffs"
        );
        assert!(record.runner_up.is_some_and(|t| t != record.champion));
        let mvp = record.mvp.expect("somebody was most valuable");
        assert!(w.career(mvp).iter().any(|c| c.season == 1 && c.games >= 9));
        assert!(record.scorers.iter().all(|s| s.is_some()));
        assert!(w.unread() > 0);
        assert!(!w.free_agents().is_empty());
        // And the third season starts like any other.
        assert_eq!(w.advance(), Stop::Mail);
    }
}
