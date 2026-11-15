//! A club's view of the world: what its manager may know, as plain data.
//!
//! `World` holds everything, including what the human UI never shows: other
//! players' true attributes, potential, personality numbers, proneness and
//! recovery, other clubs' budgets and tactics. A `ClubView` holds only what
//! the UI shows the manager, built from the same functions the UI uses:
//! `Scouting::estimate` and friends for ranges, `medical` for bodies,
//! `Personality::kind` for character, the same knowledge thresholds.
//!
//! Own players are exact where the UI shows them exact (attributes are whole
//! numbers, as on the squad page); other clubs' players are ranges that
//! narrow with scouting. Nothing here is hidden behind an `Option` for
//! convenience: a field is absent only where the UI hides it.
//!
//! The views are serde data, no I/O. There is one scouting department in a
//! `World`, the managed club's, so `view(club)` is only defined for
//! `World::watch`.

use serde::Serialize;

use crate::dressing::standing as room_words;
use crate::engine::BoxLine;
use crate::finance::Cat;
use crate::health::medical;
use crate::inbox::OfferState;
use crate::instructions::Instructions;
use crate::league::{TeamId, World};
use crate::legal::{Waiting, waiting};
use crate::player::{Attr, Body, Player, PlayerId, Position};
use crate::sponsor::slot_name;
use crate::staff::Job;
use crate::tactics::Tactics;
use crate::training::Schedule;

/// Knowledge at which the UI shows another player's character, condition
/// and mood (`pages.rs`: `CHARACTER_KNOWN_AT`, `board.rs`: `known >= 0.6`).
pub const CHARACTER_KNOWN_AT: f32 = 0.6;
/// Knowledge at which the UI shows his jumping figures.
pub const BODY_KNOWN_AT: f32 = 0.95;

fn r1(x: f32) -> f32 {
    (x * 10.0).round() / 10.0
}

fn range(r: (f32, f32)) -> Range {
    Range {
        lo: (r.0 * 10.0).floor() / 10.0,
        hi: (r.1 * 10.0).ceil() / 10.0,
    }
}

fn pct(x: f32) -> u8 {
    (x * 100.0).round().clamp(0.0, 100.0) as u8
}

/// A figure on the 1..=20 scale known only to within a range. When the
/// player is one of ours, `lo == hi`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Range {
    pub lo: f32,
    pub hi: f32,
}

/// Attribute name to range, in the order of the profile page.
#[derive(Clone, Debug)]
pub struct Attrs(pub Vec<(&'static str, Range)>);

impl Serialize for Attrs {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }
}

/// Per-game season figures.
#[derive(Clone, Debug, Serialize)]
pub struct StatLine {
    pub games: u16,
    pub min: f32,
    pub pts: f32,
    pub reb: f32,
    pub ast: f32,
    pub stl: f32,
    pub blk: f32,
    pub tov: f32,
    /// Shooting percentages, 0 when no attempts.
    pub fg_pct: f32,
    pub three_pct: f32,
    pub ft_pct: f32,
    /// The match rating the UI shows, 1..=10.
    pub rating: f32,
}

fn percent(made: u16, att: u16) -> f32 {
    if att == 0 {
        0.0
    } else {
        r1(100.0 * made as f32 / att as f32)
    }
}

fn stat_line(l: &BoxLine, games: u16) -> Option<StatLine> {
    if games == 0 {
        return None;
    }
    let g = games as f32;
    Some(StatLine {
        games,
        min: r1(l.secs as f32 / 60.0 / g),
        pts: r1(l.pts as f32 / g),
        reb: r1(l.reb() as f32 / g),
        ast: r1(l.ast as f32 / g),
        stl: r1(l.stl as f32 / g),
        blk: r1(l.blk as f32 / g),
        tov: r1(l.tov as f32 / g),
        fg_pct: percent(l.fgm, l.fga),
        three_pct: percent(l.tpm, l.tpa),
        ft_pct: percent(l.ftm, l.fta),
        rating: r1(l.rating_per(games)),
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct ContractView {
    pub salary: i64,
    pub years_left: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct InjuryView {
    pub what: String,
    pub weeks_left: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct InjuryLine {
    pub season: u16,
    pub age: u8,
    pub what: String,
    pub weeks: u8,
    /// "full recovery", "lost a step" or "never the same athlete".
    pub after: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScoutLine {
    pub round: usize,
    pub opponent: TeamId,
    pub home: bool,
    pub minutes: u32,
    pub pts: u16,
    pub reb: u16,
    pub ast: u16,
    pub rating: f32,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RotationView {
    pub minutes: u8,
    pub role: &'static str,
}

/// One signature move the manager knows about.
#[derive(Clone, Debug, Serialize)]
pub struct SignatureView {
    /// Stable id of the move, for agents: the variant name.
    pub id: String,
    pub name: &'static str,
    pub group: &'static str,
    pub description: &'static str,
    /// What a coach can do about it.
    pub answer: &'static str,
    /// False when his body can no longer do it. Only known for own players
    /// and fully scouted ones; otherwise absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_still_do: Option<bool>,
}

/// One player, as the manager sees him. Fields marked "own" are absent for
/// other clubs' players.
#[derive(Clone, Debug, Serialize)]
pub struct PlayerView {
    pub id: PlayerId,
    pub name: String,
    pub own: bool,
    /// Where he plays: a club, or none for a free agent or draft prospect.
    pub club: Option<TeamId>,
    pub age: u8,
    pub position: Position,
    /// Familiarity with each position he knows at all, percent.
    pub positions: Vec<(Position, u8)>,
    /// Combine measurements: public.
    pub body: Body,
    pub attrs: Attrs,
    /// His signature moves as far as the manager knows them: all of an own
    /// player's, only the revealed ones of anyone else's. Innate and cannot
    /// be taught. An empty list says nothing about unrevealed ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signatures: Vec<SignatureView>,
    pub overall: Range,
    /// Own: the staff's range around his ceiling. Others: the scouts' range.
    pub potential: Range,
    /// How well scouted he is, percent; own players are 100.
    pub known_pct: u8,
    /// The one-line character the UI shows, when it shows one.
    pub character: Option<&'static str>,
    pub character_note: Option<&'static str>,
    /// Percent; shown for own players and well-scouted others.
    pub condition_pct: Option<u8>,
    pub sharp_pct: Option<u8>,
    /// "Delighted" .. "Unhappy".
    pub mood: Option<&'static str>,
    /// What the medical staff say; for others it depends on scouting.
    pub medical: String,
    pub injury: Option<InjuryView>,
    pub injury_record: Vec<InjuryLine>,
    pub contract: ContractView,
    pub season: Option<StatLine>,
    /// His max vertical and touch, shown for own and fully scouted players.
    pub max_vertical_cm: Option<f32>,
    pub max_touch_cm: Option<f32>,
    /// Fame, 0..=100: public, everybody knows who is famous.
    pub fame: f32,
    pub fame_words: &'static str,
    /// He has said this season is his last.
    pub farewell: bool,
    /// The season (counting from 1) he retired after; he is off every list
    /// but his page and the record stay.
    pub retired_after_season: Option<u16>,
    // Own only:
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_work: Option<&'static str>,
    /// How he comes across, in words: what the profile page says of a man
    /// the manager knows. Never a number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manner_in_public: Option<&'static str>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shirt_sales_per_round: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shirt_sales_this_season: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training: Option<(String, String)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<RotationView>,
    /// "first_team" or "reserves".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub squad: Option<&'static str>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plays_for_reserves: Option<bool>,
    // Others only:
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortlisted: Option<bool>,
    /// What the market would pay, as the bid page shows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_value: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bid_pending: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scout_reports: Vec<ScoutLine>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TableRow {
    pub place: usize,
    pub club: TeamId,
    pub name: String,
    pub wins: u16,
    pub losses: u16,
    pub points_for: u32,
    pub points_against: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct FixtureView {
    pub index: usize,
    pub round: usize,
    pub home: TeamId,
    pub away: TeamId,
    pub home_name: String,
    pub away_name: String,
    pub playoff: bool,
    pub played: bool,
    pub score: Option<[u16; 2]>,
    pub periods: Vec<[u16; 2]>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BoxRow {
    pub player: PlayerId,
    pub name: String,
    pub minutes: u32,
    pub pts: u16,
    pub fg: String,
    pub three: String,
    pub ft: String,
    pub reb: u16,
    pub ast: u16,
    pub stl: u16,
    pub blk: u16,
    pub tov: u16,
    pub pf: u16,
    pub plus_minus: i16,
}

#[derive(Clone, Debug, Serialize)]
pub struct BoxScoreView {
    pub fixture: FixtureView,
    /// Home then away.
    pub sides: [Vec<BoxRow>; 2],
}

#[derive(Clone, Debug, Serialize)]
pub struct StaffLine {
    pub job: &'static str,
    pub name: String,
    pub age: u8,
    pub rating: u8,
    /// The head coach's style.
    pub runs: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GmView {
    pub name: String,
    pub age: u8,
    pub traits: Vec<&'static str>,
    pub judgement: u8,
    pub negotiation: u8,
    pub patience: u8,
    pub activity: u8,
    pub scouting_dept: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct DealView {
    pub sponsor: String,
    pub slots: Vec<String>,
    pub fee: i64,
    pub ends_season: u16,
}

/// A hall and its boards, as any club's page shows them.
#[derive(Clone, Debug, Serialize)]
pub struct ArenaView {
    pub name: String,
    pub capacity: u32,
    pub condition: &'static str,
    pub condition_pct: u8,
    /// The price in force at home games.
    pub ticket_price: i64,
    /// The hall's own price, before the club moved it.
    pub reference_price: i64,
    /// Supporters, in people.
    pub fan_base: u32,
    pub pending_seats: u32,
    pub average_crowd: Option<u32>,
    pub last_crowd: Option<u32>,
    pub expected_crowd: Option<u32>,
    pub gate_this_season: i64,
    pub sponsors_this_season: i64,
    pub boards_sold: usize,
    pub sponsor_income: i64,
    pub deals: Vec<DealView>,
}

/// What anyone can see of a club.
#[derive(Clone, Debug, Serialize)]
pub struct ClubPublic {
    pub club: TeamId,
    pub name: String,
    pub wins: u16,
    pub losses: u16,
    pub place: usize,
    pub scores: f32,
    pub allows: f32,
    pub offense: &'static str,
    pub defense: &'static str,
    pub press: &'static str,
    pub gm: GmView,
    pub staff: Vec<StaffLine>,
    pub arena: ArenaView,
    /// "Sound", "Tight", "Overdrawn" or "Over its limit": what the
    /// published accounts show without the figure.
    pub money: &'static str,
    pub picks_held: Vec<String>,
    /// First-team squad, best known overall first.
    pub squad: Vec<PlayerView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Calendar {
    pub season: u16,
    pub day: u32,
    /// "Friday 9 October 2026".
    pub date: String,
    /// "Fri 9 Oct".
    pub date_short: String,
    pub weekday: &'static str,
    /// "preseason", "season", "playoffs" or "offseason".
    pub phase: &'static str,
    pub round: usize,
    pub rounds: usize,
    pub season_over: bool,
    pub match_today: bool,
    /// Day the next game is on.
    pub match_day: Option<u32>,
    /// What `Advance` will stop at: "match_day", "mail" or "draft".
    pub next_stop: &'static str,
    pub next_stop_day: Option<u32>,
    pub draft_on: bool,
    /// The day the draft is (or will be) held, once the season is over.
    pub draft_day: Option<u32>,
    pub champion: Option<TeamId>,
    /// The manager has a friendly today; it is played when the day passes.
    pub friendly_today: bool,
    /// Days of training camp before the first league week.
    pub preseason_days: u32,
    /// Everything on today's calendar across the league.
    pub today: Vec<crate::calendar::CalEvent>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NextGame {
    pub fixture: usize,
    pub round: usize,
    pub opponent: TeamId,
    pub opponent_name: String,
    pub home: bool,
    pub day: u32,
    pub date: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub club: TeamId,
    pub name: String,
    pub calendar: Calendar,
    pub wins: u16,
    pub losses: u16,
    pub place: usize,
    pub point_difference: i64,
    pub budget: i64,
    pub wages_per_week: i64,
    pub board_expects_place: usize,
    pub dressing_room_pct: u8,
    pub dressing_room: &'static str,
    pub match_tactic: String,
    pub tactic_familiarity_pct: u8,
    pub next_game: Option<NextGame>,
    pub unread: usize,
    pub waiting: Waiting,
    pub squad_size: usize,
    pub injured: Vec<(PlayerId, String, u8)>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OfferView {
    pub index: usize,
    pub player: PlayerId,
    pub player_name: String,
    pub club: TeamId,
    pub club_name: String,
    pub fee: i64,
    pub expires_day: u32,
    pub state: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct PressView {
    pub index: usize,
    pub topic: &'static str,
    pub about: Option<PlayerId>,
    pub options: [String; 3],
    pub open: bool,
    pub answer: Option<u8>,
    pub outcome: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SponsorOfferView {
    pub index: usize,
    pub sponsor: String,
    pub slots: Vec<String>,
    pub years: u8,
    pub fee: i64,
    pub bonus: String,
    pub expires_day: u32,
    pub state: &'static str,
    pub open: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct MessageView {
    pub id: u32,
    pub day: u32,
    pub date: String,
    pub kind: &'static str,
    pub from: String,
    pub subject: String,
    pub body: String,
    pub players: Vec<PlayerId>,
    pub read: bool,
    /// Set when the message carries something that needs an answer.
    pub offer: Option<OfferView>,
    pub press: Option<PressView>,
    pub sponsor: Option<SponsorOfferView>,
    /// A sponsor's request for one of your players' time.
    pub appearance: Option<crate::view_more::AppearanceView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BidView {
    pub player: PlayerId,
    pub player_name: String,
    pub fee: i64,
    pub answer_on_day: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct InboxView {
    /// Arrived mail, newest first.
    pub messages: Vec<MessageView>,
    pub unread: usize,
    pub pending_bids: Vec<BidView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TacticView {
    pub slot: usize,
    pub name: String,
    pub active: bool,
    pub familiarity_pct: u8,
    pub tactics: Tactics,
}

#[derive(Clone, Debug, Serialize)]
pub struct TrainingView {
    pub schedule: Schedule,
    pub delegated_to_assistant: bool,
    pub assistant_notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StaffRow {
    pub job: &'static str,
    pub vacant: bool,
    pub name: Option<String>,
    pub age: Option<u8>,
    /// Tactics, teaching, fitness, judgement.
    pub ratings: Option<[u8; 4]>,
    /// Handling the public (the press officer's rating).
    pub media: Option<u8>,
    pub wage: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StaffCandidate {
    pub index: usize,
    pub name: String,
    pub age: u8,
    pub ratings: [u8; 4],
    pub media: u8,
    pub wage: i64,
    pub runs: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Books {
    pub budget: i64,
    pub spending_frozen: bool,
    pub wages_per_week: i64,
    pub payroll: i64,
    pub salary_cap: i64,
    /// This season so far, by category.
    pub lines: Vec<(&'static str, i64)>,
    pub net: i64,
    pub last_season: Option<(i64, i64, i64)>,
    pub renovation_cost: i64,
    pub renovation_blocked: Option<String>,
    pub expansion_cost: i64,
    pub expansion_blocked: Option<String>,
}

/// Everything about the managed club.
#[derive(Clone, Debug, Serialize)]
pub struct OwnClub {
    pub summary: Summary,
    pub squad: Vec<PlayerView>,
    pub reserves: Vec<PlayerView>,
    pub playbook: Vec<TacticView>,
    pub training: TrainingView,
    pub staff: Vec<StaffRow>,
    pub gm: GmView,
    pub books: Books,
    pub arena: ArenaView,
    pub open_sponsor_offers: Vec<SponsorOfferView>,
    /// Sponsors' requests for a player's time that can still be answered.
    pub open_appearance_requests: Vec<crate::view_more::AppearanceView>,
    /// Ticket price and preview, budgets, debt, tax, facilities, projection,
    /// the shop and the fan base.
    pub finance: crate::view_more::FinanceView,
    pub shortlist: Vec<PlayerId>,
    pub picks: Vec<(usize, String)>,
    pub instructions: Instructions,
}

#[derive(Clone, Debug, Serialize)]
pub struct SeriesView {
    pub high: TeamId,
    pub low: TeamId,
    pub wins: [u8; 2],
    pub is_final: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoryRow {
    pub season: u16,
    pub champion: TeamId,
    pub runner_up: Option<TeamId>,
    pub mvp: Option<String>,
    pub our_place: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Leader {
    pub player: PlayerId,
    pub name: String,
    pub club: Option<TeamId>,
    pub games: u16,
    pub pts: f32,
    pub reb: f32,
    pub ast: f32,
    pub rating: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct DraftSlotView {
    pub pick: usize,
    pub round: u8,
    pub owner: TeamId,
    pub original: TeamId,
    pub taken: Option<(PlayerId, String)>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DraftView {
    pub on_clock: Option<TeamId>,
    pub ours_on_clock: bool,
    pub slots: Vec<DraftSlotView>,
    /// Prospects left, as scouted.
    pub prospects: Vec<PlayerView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Market {
    pub free_agents: Vec<PlayerView>,
    pub shortlist: Vec<PlayerView>,
    pub staff_pool: Vec<StaffCandidate>,
    pub budget: i64,
    pub roster_size: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct LeagueView {
    pub table: Vec<TableRow>,
    pub fixtures: Vec<FixtureView>,
    pub playoffs: Vec<SeriesView>,
    pub history: Vec<HistoryRow>,
    pub leaders: Vec<Leader>,
    pub clubs: Vec<ClubPublic>,
    /// The biggest names still playing, and the hall of fame.
    pub names: crate::view_more::NamesView,
    /// Box scores of our games and of the latest round.
    pub box_scores: Vec<BoxScoreView>,
}

/// Everything the managed club's manager may know.
#[derive(Clone, Debug, Serialize)]
pub struct ClubView {
    pub own: OwnClub,
    pub inbox: InboxView,
    pub league: LeagueView,
    pub market: Market,
    pub draft: Option<DraftView>,
}

fn kind_label(k: crate::inbox::Kind) -> &'static str {
    k.label()
}

fn state_label(s: OfferState) -> &'static str {
    match s {
        OfferState::Open => "open",
        OfferState::Accepted => "accepted",
        OfferState::Rejected => "rejected",
        OfferState::Lapsed => "lapsed",
    }
}

impl World {
    fn is_own(&self, club: TeamId, p: PlayerId) -> bool {
        let t = &self.teams[club];
        t.roster.contains(&p) || t.reserves.contains(&p)
    }

    /// One player as the manager `club` sees him.
    pub fn view_player(&self, club: TeamId, pid: PlayerId) -> Option<PlayerView> {
        if self.watch != Some(club) || pid as usize >= self.players.len() {
            return None;
        }
        let p: &Player = &self.players[pid as usize];
        let own = self.is_own(club, pid);
        let known = if own {
            1.0
        } else {
            self.scouting.knowledge(pid)
        };
        let attrs = Attrs(
            Attr::ALL
                .iter()
                .map(|&a| {
                    let r = if own {
                        let v = p.attrs[a].round();
                        Range { lo: v, hi: v }
                    } else {
                        range(self.scouting.estimate(p, a))
                    };
                    (a.name(), r)
                })
                .collect(),
        );
        let overall = if own {
            let v = p.overall().round();
            Range { lo: v, hi: v }
        } else {
            range(self.scouting.estimate_overall(p))
        };
        let potential = if own {
            range((
                (p.potential - 1.0).max(p.overall()),
                (p.potential + 1.0).min(20.0),
            ))
        } else {
            range(self.scouting.estimate_potential(p))
        };
        let detailed = own || known >= CHARACTER_KNOWN_AT;
        let kind = p.personality.kind();
        let positions = Position::ALL
            .into_iter()
            .filter(|&pos| p.knows(pos) >= 0.05)
            .map(|pos| (pos, pct(p.knows(pos))))
            .collect();
        let tactic = self.teams[club].tactics();
        let squad = if self.teams[club].roster.contains(&pid) {
            Some("first_team")
        } else if self.teams[club].reserves.contains(&pid) {
            Some("reserves")
        } else {
            None
        };
        let games = self.games_played[pid as usize];
        let sharp_known = own;
        let reports = if own {
            Vec::new()
        } else {
            self.scouting
                .reports_for(pid)
                .take(6)
                .map(|r| ScoutLine {
                    round: r.round,
                    opponent: r.opponent,
                    home: r.home,
                    minutes: r.line.secs / 60,
                    pts: r.line.pts,
                    reb: r.line.reb(),
                    ast: r.line.ast,
                    rating: r1(r.rating),
                    notes: r.observations.iter().map(|o| o.to_string()).collect(),
                })
                .collect()
        };
        Some(PlayerView {
            id: pid,
            name: p.name.clone(),
            own,
            club: self.team_of(pid),
            age: p.age,
            position: p.position,
            positions,
            body: p.body.clone(),
            attrs,
            signatures: self
                .known_signatures(pid)
                .into_iter()
                .map(|s| SignatureView {
                    id: format!("{s:?}"),
                    name: s.name(),
                    group: s.group().label(),
                    description: s.description(),
                    answer: s.answer(),
                    can_still_do: (own || known >= BODY_KNOWN_AT).then(|| s.live(p)),
                })
                .collect(),
            overall,
            potential,
            known_pct: pct(known),
            character: detailed.then(|| kind.label()),
            character_note: detailed.then(|| kind.description()),
            condition_pct: detailed.then(|| pct(p.condition)),
            sharp_pct: sharp_known.then(|| pct(p.sharp)),
            mood: detailed.then(|| p.mood()),
            medical: medical(p, known),
            injury: p.injury.as_ref().map(|i| InjuryView {
                what: i.what.label().to_string(),
                weeks_left: i.weeks_left,
            }),
            injury_record: p
                .injuries
                .iter()
                .rev()
                .map(|r| InjuryLine {
                    season: r.season + 1,
                    age: r.age,
                    what: r.what.label().to_string(),
                    weeks: r.weeks,
                    after: match r.lost {
                        l if l >= 0.8 => "never the same athlete",
                        l if l > 0.0 => "lost a step",
                        _ => "full recovery",
                    },
                })
                .collect(),
            contract: ContractView {
                salary: p.contract.salary as i64,
                years_left: p.contract.years_left,
            },
            season: stat_line(&self.season_stats[pid as usize], games),
            max_vertical_cm: (own || known >= BODY_KNOWN_AT).then(|| r1(p.max_vertical_cm())),
            max_touch_cm: (own || known >= BODY_KNOWN_AT).then(|| r1(p.max_touch_cm())),
            fame: r1(p.renown.fame),
            fame_words: crate::fame::fame_words(p.renown.fame),
            farewell: p.renown.farewell,
            retired_after_season: p.renown.retired.map(|s| s + 1),
            media_work: own.then(|| p.renown.media.label()),
            manner_in_public: own.then(|| crate::media::presence_words(p.personality.presence)),
            shirt_sales_per_round: own.then(|| self.player_merch(pid)),
            shirt_sales_this_season: own.then_some(p.renown.shirts),
            training: own.then(|| {
                (
                    p.training.label().to_string(),
                    p.intensity.label().to_string(),
                )
            }),
            rotation: own
                .then(|| {
                    tactic
                        .rotation
                        .iter()
                        .find(|s| s.player == pid)
                        .map(|s| RotationView {
                            minutes: s.minutes,
                            role: s.role.label(),
                        })
                })
                .flatten(),
            squad: if own { squad } else { None },
            plays_for_reserves: own.then(|| self.teams[club].down.contains(&pid)),
            shortlisted: (!own).then(|| self.scouting.is_shortlisted(pid)),
            market_value: (!own).then(|| self.value(pid)),
            bid_pending: (!own)
                .then(|| self.bid_pending(pid).map(|b| b.fee))
                .flatten(),
            scout_reports: reports,
        })
    }

    fn views(&self, club: TeamId, ids: &[PlayerId]) -> Vec<PlayerView> {
        ids.iter()
            .filter_map(|&p| self.view_player(club, p))
            .collect()
    }

    pub fn view_calendar(&self) -> Calendar {
        let pending_mail = self
            .desk
            .messages
            .iter()
            .filter(|m| m.day > self.day)
            .map(|m| m.day)
            .min();
        let match_day = (!self.season_over()).then(|| self.match_day());
        let draft_day = (self.season_over()).then(|| {
            if self.draft_day == 0 {
                self.default_draft_day()
            } else {
                self.draft_day
            }
        });
        let (next_stop, next_stop_day) = if self.match_today() {
            ("match_day", Some(self.day))
        } else if self.draft.is_some() {
            ("draft", Some(self.day))
        } else if let Some(d) = draft_day {
            match pending_mail {
                Some(m) if m < d => ("mail", Some(m)),
                _ => ("draft", Some(d.max(self.day))),
            }
        } else {
            match (pending_mail, match_day) {
                (Some(m), Some(g)) if m <= g => ("mail", Some(m)),
                (_, Some(g)) => ("match_day", Some(g)),
                (Some(m), None) => ("mail", Some(m)),
                (None, None) => ("draft", Some(self.day)),
            }
        };
        let phase = if self.season_over() {
            "offseason"
        } else if self.in_preseason() {
            "preseason"
        } else if self.fixtures.iter().any(|f| f.playoff) {
            "playoffs"
        } else {
            "season"
        };
        Calendar {
            season: self.season + 1,
            day: self.day,
            date: self.day_long(self.day),
            date_short: self.day_label(self.day),
            weekday: self.weekday(self.day),
            phase,
            round: self.round.min(self.rounds()),
            rounds: self.rounds(),
            season_over: self.season_over(),
            match_today: self.match_today(),
            match_day,
            next_stop,
            next_stop_day,
            draft_on: self.draft.is_some(),
            draft_day,
            champion: self.champion,
            friendly_today: self.friendly_today().is_some(),
            preseason_days: self.preseason_days,
            today: self.events_on(self.day),
        }
    }

    pub fn view_table(&self) -> Vec<TableRow> {
        self.table()
            .into_iter()
            .enumerate()
            .map(|(i, t)| {
                let s = &self.standings[t];
                TableRow {
                    place: i + 1,
                    club: t,
                    name: self.teams[t].name.clone(),
                    wins: s.wins,
                    losses: s.losses,
                    points_for: s.points_for,
                    points_against: s.points_against,
                }
            })
            .collect()
    }

    fn fixture_view(&self, i: usize) -> FixtureView {
        let f = &self.fixtures[i];
        FixtureView {
            index: i,
            round: f.round,
            home: f.home,
            away: f.away,
            home_name: self.teams[f.home].name.clone(),
            away_name: self.teams[f.away].name.clone(),
            playoff: f.playoff,
            played: f.result.is_some(),
            score: f.result.as_ref().map(|r| r.score),
            periods: f
                .result
                .as_ref()
                .map(|r| r.periods.clone())
                .unwrap_or_default(),
        }
    }

    pub fn view_fixtures(&self) -> Vec<FixtureView> {
        (0..self.fixtures.len())
            .map(|i| self.fixture_view(i))
            .collect()
    }

    /// The box score of a played game: public to everyone.
    pub fn view_box_score(&self, fixture: usize) -> Option<BoxScoreView> {
        let f = self.fixtures.get(fixture)?;
        let r = f.result.as_ref()?;
        let side = |s: usize| {
            r.boxes[s]
                .iter()
                .map(|(pid, l)| BoxRow {
                    player: *pid,
                    name: self.players[*pid as usize].name.clone(),
                    minutes: l.secs / 60,
                    pts: l.pts,
                    fg: format!("{}-{}", l.fgm, l.fga),
                    three: format!("{}-{}", l.tpm, l.tpa),
                    ft: format!("{}-{}", l.ftm, l.fta),
                    reb: l.reb(),
                    ast: l.ast,
                    stl: l.stl,
                    blk: l.blk,
                    tov: l.tov,
                    pf: l.pf,
                    plus_minus: l.pm,
                })
                .collect::<Vec<_>>()
        };
        Some(BoxScoreView {
            fixture: self.fixture_view(fixture),
            sides: [side(0), side(1)],
        })
    }

    fn arena_view(&self, t: TeamId) -> ArenaView {
        let a = self.arena(t);
        let ledger = self.ledger(t);
        let deals: Vec<DealView> = self
            .econ
            .deals
            .iter()
            .filter(|d| d.club == t)
            .map(|d| DealView {
                sponsor: self.econ.sponsors[d.sponsor].name.clone(),
                slots: d.slots.iter().map(|&i| slot_name(i as usize)).collect(),
                fee: d.fee,
                ends_season: d.ends + 1,
            })
            .collect();
        ArenaView {
            name: a.name.clone(),
            capacity: a.capacity,
            condition: a.condition_words(),
            condition_pct: pct(a.condition),
            ticket_price: self.ticket_price(t),
            reference_price: self.fair_price(t),
            fan_base: self.fan_base(t),
            pending_seats: a.pending_seats,
            average_crowd: (ledger.games > 0).then(|| ledger.average_attendance()),
            last_crowd: (ledger.games > 0).then_some(ledger.last_attendance),
            expected_crowd: (ledger.games == 0).then(|| self.expected_attendance(t)),
            gate_this_season: ledger.total(Cat::Gate),
            sponsors_this_season: ledger.total(Cat::Sponsorship),
            boards_sold: crate::sponsor::SLOTS - self.free_slots(t).len(),
            sponsor_income: self.sponsor_income(t),
            deals,
        }
    }

    fn gm_view(&self, t: TeamId) -> GmView {
        let o = &self.teams[t].office;
        GmView {
            name: o.gm.name.clone(),
            age: o.gm.age,
            traits: o.gm.traits(),
            judgement: o.gm.judgement,
            negotiation: o.gm.negotiation,
            patience: o.gm.patience,
            activity: o.gm.activity,
            scouting_dept: o.scouting(),
        }
    }

    fn staff_lines(&self, t: TeamId) -> Vec<StaffLine> {
        let office = &self.teams[t].office;
        Job::ALL
            .into_iter()
            .filter_map(|job| {
                let s = office.staffer(job)?;
                Some(StaffLine {
                    job: job.label(),
                    name: s.name.clone(),
                    age: s.age,
                    rating: s.rating(job),
                    runs: (job == Job::HeadCoach)
                        .then(|| format!("{}, {}", s.offense.label(), s.defense.label())),
                })
            })
            .collect()
    }

    /// What anyone can see of club `t`, whichever club is looking.
    pub fn view_club(&self, club: TeamId, t: TeamId) -> Option<ClubPublic> {
        if self.watch != Some(club) || t >= self.teams.len() {
            return None;
        }
        let team = &self.teams[t];
        let s = &self.standings[t];
        let games = (s.wins + s.losses).max(1) as f32;
        let tactics = team.tactics();
        let mut roster = team.roster.clone();
        // The club page sorts by what the viewer knows: true overall for
        // his own, the scouts' middle for the rest.
        let key = |p: PlayerId| {
            let pl = &self.players[p as usize];
            if t == club {
                pl.overall()
            } else {
                let r = self.scouting.estimate_overall(pl);
                (r.0 + r.1) / 2.0
            }
        };
        roster.sort_by(|&a, &b| key(b).total_cmp(&key(a)).then(a.cmp(&b)));
        Some(ClubPublic {
            club: t,
            name: team.name.clone(),
            wins: s.wins,
            losses: s.losses,
            place: self
                .table()
                .iter()
                .position(|&x| x == t)
                .map_or(0, |p| p + 1),
            scores: r1(s.points_for as f32 / games),
            allows: r1(s.points_against as f32 / games),
            offense: tactics.offense.label(),
            defense: tactics.defense.label(),
            press: tactics.press.label(),
            gm: self.gm_view(t),
            staff: self.staff_lines(t),
            arena: self.arena_view(t),
            money: self.money_state(t).label(),
            picks_held: (0..self.picks.len())
                .filter(|&i| self.picks[i].owner == t)
                .map(|i| self.pick_label(i))
                .collect(),
            squad: self.views(club, &roster),
        })
    }

    fn offer_view(&self, i: usize) -> OfferView {
        let o = &self.desk.offers[i];
        OfferView {
            index: i,
            player: o.player,
            player_name: self.players[o.player as usize].name.clone(),
            club: o.club,
            club_name: self.teams[o.club].name.clone(),
            fee: o.fee,
            expires_day: o.expires,
            state: state_label(o.state),
        }
    }

    fn sponsor_offer_view(&self, i: usize) -> SponsorOfferView {
        let o = &self.econ.offers[i];
        SponsorOfferView {
            index: i,
            sponsor: self.econ.sponsors[o.sponsor].name.clone(),
            slots: o.slots.iter().map(|&s| slot_name(s as usize)).collect(),
            years: o.years,
            fee: o.fee,
            bonus: o.bonus.describe(),
            expires_day: o.expires,
            state: state_label(o.state),
            open: o.club == self.watch.unwrap_or(usize::MAX) && o.open_on(self.day),
        }
    }

    pub fn view_inbox(&self, club: TeamId) -> Option<InboxView> {
        if self.watch != Some(club) {
            return None;
        }
        let messages = self
            .inbox()
            .into_iter()
            .map(|m| MessageView {
                id: m.id,
                day: m.day,
                date: self.day_label(m.day),
                kind: kind_label(m.kind),
                from: m.from.clone(),
                subject: m.subject.clone(),
                body: m.body.clone(),
                players: m.players.clone(),
                read: m.read,
                offer: m
                    .offer
                    .filter(|&i| i < self.desk.offers.len())
                    .map(|i| self.offer_view(i)),
                press: m.press.filter(|&i| i < self.desk.press.len()).map(|i| {
                    let p = &self.desk.press[i];
                    PressView {
                        index: i,
                        topic: match p.topic {
                            crate::dressing::Topic::AfterWin => "after_win",
                            crate::dressing::Topic::AfterLoss => "after_loss",
                            crate::dressing::Topic::BeforeBig => "before_big_game",
                        },
                        about: p.about,
                        options: p.options.clone(),
                        open: self.press_open(i),
                        answer: p.answer,
                        outcome: p.outcome.clone(),
                    }
                }),
                sponsor: m
                    .sponsor
                    .filter(|&i| i < self.econ.offers.len())
                    .map(|i| self.sponsor_offer_view(i)),
                appearance: m.appearance.and_then(|i| self.view_appearance(i)),
            })
            .collect();
        Some(InboxView {
            messages,
            unread: self.unread(),
            pending_bids: self
                .desk
                .bids
                .iter()
                .map(|b| BidView {
                    player: b.player,
                    player_name: self.players[b.player as usize].name.clone(),
                    fee: b.fee,
                    answer_on_day: b.answer_on,
                })
                .collect(),
        })
    }

    pub fn view_summary(&self, club: TeamId) -> Option<Summary> {
        if self.watch != Some(club) {
            return None;
        }
        let team = &self.teams[club];
        let s = &self.standings[club];
        let tactic = &team.playbook[team.active];
        let next = self
            .fixtures
            .iter()
            .enumerate()
            .find(|(_, f)| {
                f.round == self.round
                    && f.result.is_none()
                    && (f.home == club || f.away == club)
            });
        let wages: i64 = team
            .roster
            .iter()
            .map(|&p| self.players[p as usize].contract.salary as i64)
            .sum();
        Some(Summary {
            club,
            name: team.name.clone(),
            calendar: self.view_calendar(),
            wins: s.wins,
            losses: s.losses,
            place: self
                .table()
                .iter()
                .position(|&t| t == club)
                .map_or(0, |p| p + 1),
            point_difference: s.points_for as i64 - s.points_against as i64,
            budget: team.budget,
            wages_per_week: wages / 52,
            board_expects_place: self.desk.expectation,
            dressing_room_pct: pct(team.belief),
            dressing_room: room_words(team.belief),
            match_tactic: tactic.name.clone(),
            tactic_familiarity_pct: pct(tactic.familiarity()),
            next_game: next.map(|(i, f)| {
                let opp = if f.home == club { f.away } else { f.home };
                NextGame {
                    fixture: i,
                    round: f.round,
                    opponent: opp,
                    opponent_name: self.teams[opp].name.clone(),
                    home: f.home == club,
                    day: f.day,
                    date: self.day_label(f.day),
                }
            }),
            unread: self.unread(),
            waiting: waiting(self),
            squad_size: team.roster.len(),
            injured: team
                .roster
                .iter()
                .chain(&team.reserves)
                .filter_map(|&p| {
                    let pl = &self.players[p as usize];
                    pl.injury
                        .as_ref()
                        .map(|i| (p, i.what.label().to_string(), i.weeks_left))
                })
                .collect(),
        })
    }

    pub fn view_squad(&self, club: TeamId) -> Option<(Vec<PlayerView>, Vec<PlayerView>)> {
        if self.watch != Some(club) {
            return None;
        }
        let t = &self.teams[club];
        Some((self.views(club, &t.roster), self.views(club, &t.reserves)))
    }

    pub fn view_market(&self, club: TeamId) -> Option<Market> {
        if self.watch != Some(club) {
            return None;
        }
        let in_draft = |p: PlayerId| self.draft.as_ref().is_some_and(|d| d.pool.contains(&p));
        let free: Vec<PlayerId> = (0..self.players.len() as PlayerId)
            .filter(|&p| {
                self.team_of(p).is_none() && !in_draft(p) && !self.players[p as usize].is_retired()
            })
            .collect();
        Some(Market {
            free_agents: self.views(club, &free),
            shortlist: self.views(club, &self.scouting.shortlist),
            staff_pool: self
                .staff_pool
                .iter()
                .enumerate()
                .map(|(i, s)| StaffCandidate {
                    index: i,
                    name: s.name.clone(),
                    age: s.age,
                    ratings: [s.tactics, s.teaching, s.fitness, s.judgement],
                    media: s.media,
                    wage: s.wage,
                    runs: format!("{}, {}", s.offense.label(), s.defense.label()),
                })
                .collect(),
            budget: self.teams[club].budget,
            roster_size: self.teams[club].roster.len(),
        })
    }

    pub fn view_draft(&self, club: TeamId) -> Option<DraftView> {
        if self.watch != Some(club) {
            return None;
        }
        let d = self.draft.as_ref()?;
        Some(DraftView {
            on_clock: d.on_clock(),
            ours_on_clock: d.on_clock() == Some(club),
            slots: d
                .slots
                .iter()
                .enumerate()
                .map(|(i, s)| DraftSlotView {
                    pick: i + 1,
                    round: s.round,
                    owner: s.owner,
                    original: s.original,
                    taken: s.taken.map(|p| (p, self.players[p as usize].name.clone())),
                })
                .collect(),
            prospects: self.views(club, &d.pool),
        })
    }

    pub fn view_leaders(&self, n: usize) -> Vec<Leader> {
        let mut ids: Vec<PlayerId> = (0..self.players.len() as PlayerId)
            .filter(|&p| self.games_played[p as usize] >= 3)
            .collect();
        let ppg = |p: PlayerId| {
            self.season_stats[p as usize].pts as f32 / self.games_played[p as usize] as f32
        };
        ids.sort_by(|&a, &b| ppg(b).total_cmp(&ppg(a)).then(a.cmp(&b)));
        ids.into_iter()
            .take(n)
            .map(|p| {
                let l = &self.season_stats[p as usize];
                let g = self.games_played[p as usize];
                Leader {
                    player: p,
                    name: self.players[p as usize].name.clone(),
                    club: self.team_of(p),
                    games: g,
                    pts: r1(l.pts as f32 / g as f32),
                    reb: r1(l.reb() as f32 / g as f32),
                    ast: r1(l.ast as f32 / g as f32),
                    rating: r1(l.rating_per(g)),
                }
            })
            .collect()
    }

    pub fn view_league(&self, club: TeamId) -> Option<LeagueView> {
        if self.watch != Some(club) {
            return None;
        }
        let last_round = self.round.checked_sub(1);
        let box_scores = (0..self.fixtures.len())
            .filter(|&i| {
                let f = &self.fixtures[i];
                f.result.is_some()
                    && (f.home == club || f.away == club || Some(f.round) == last_round)
            })
            .filter_map(|i| self.view_box_score(i))
            .collect();
        Some(LeagueView {
            table: self.view_table(),
            fixtures: self.view_fixtures(),
            playoffs: self
                .playoffs
                .iter()
                .map(|s| SeriesView {
                    high: s.high,
                    low: s.low,
                    wins: s.wins,
                    is_final: s.is_final,
                })
                .collect(),
            history: self
                .history
                .iter()
                .map(|h| HistoryRow {
                    season: h.season + 1,
                    champion: h.champion,
                    runner_up: h.runner_up,
                    mvp: h.mvp.map(|p| self.players[p as usize].name.clone()),
                    our_place: h.place(club),
                })
                .collect(),
            leaders: self.view_leaders(15),
            clubs: (0..self.teams.len())
                .filter_map(|t| self.view_club(club, t))
                .collect(),
            names: self.view_names(10),
            box_scores,
        })
    }

    fn own_club(&self, club: TeamId) -> Option<OwnClub> {
        let summary = self.view_summary(club)?;
        let (squad, reserves) = self.view_squad(club)?;
        let team = &self.teams[club];
        let ledger = self.ledger(club);
        let a = self.arena(club);
        let staff = Job::ALL
            .into_iter()
            .map(|job| match team.office.staffer(job) {
                Some(s) => StaffRow {
                    job: job.label(),
                    vacant: false,
                    name: Some(s.name.clone()),
                    age: Some(s.age),
                    ratings: Some([s.tactics, s.teaching, s.fitness, s.judgement]),
                    media: Some(s.media),
                    wage: Some(s.wage),
                },
                None => StaffRow {
                    job: job.label(),
                    vacant: true,
                    name: None,
                    age: None,
                    ratings: None,
                    media: None,
                    wage: None,
                },
            })
            .collect();
        let wages: i64 = team
            .roster
            .iter()
            .map(|&p| self.players[p as usize].contract.salary as i64)
            .sum();
        Some(OwnClub {
            gm: self.gm_view(club),
            squad,
            reserves,
            playbook: team
                .playbook
                .iter()
                .enumerate()
                .map(|(i, s)| TacticView {
                    slot: i,
                    name: s.name.clone(),
                    active: i == team.active,
                    familiarity_pct: pct(s.familiarity()),
                    tactics: s.tactics.clone(),
                })
                .collect(),
            training: TrainingView {
                schedule: team.schedule.clone(),
                delegated_to_assistant: team.delegate_training,
                assistant_notes: team.assistant_notes.iter().map(|r| r.to_string()).collect(),
            },
            staff,
            books: Books {
                budget: team.budget,
                spending_frozen: self.spending_frozen(club),
                wages_per_week: wages / 52,
                payroll: self.payroll(club),
                salary_cap: crate::season::SALARY_CAP,
                lines: Cat::ALL
                    .into_iter()
                    .filter(|&c| ledger.total(c) != 0)
                    .map(|c| (c.label(), ledger.total(c)))
                    .collect(),
                net: ledger.net(),
                last_season: self
                    .econ
                    .history
                    .last()
                    .map(|h| (h[club].income(), h[club].net(), h[club].closing)),
                renovation_cost: a.renovation_cost(),
                renovation_blocked: self.works_blocked(club, false),
                expansion_cost: a.expansion_cost(),
                expansion_blocked: self.works_blocked(club, true),
            },
            arena: self.arena_view(club),
            open_sponsor_offers: (0..self.econ.offers.len())
                .map(|i| self.sponsor_offer_view(i))
                .filter(|o| o.open)
                .collect(),
            open_appearance_requests: self.view_appearances(club)?,
            finance: self.view_finance(club)?,
            shortlist: self.scouting.shortlist.clone(),
            picks: (0..self.picks.len())
                .filter(|&i| self.picks[i].owner == club)
                .map(|i| (i, self.pick_label(i)))
                .collect(),
            instructions: team.instructions.clone(),
            summary,
        })
    }

    /// Everything the manager of `club` may know. `None` unless `club` is
    /// the managed club: the world has one scouting department and one
    /// desk, and they are his.
    pub fn view(&self, club: TeamId) -> Option<ClubView> {
        Some(ClubView {
            own: self.own_club(club)?,
            inbox: self.view_inbox(club)?,
            league: self.view_league(club)?,
            market: self.view_market(club)?,
            draft: self.view_draft(club),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::command::tests::tidy;
    use crate::inbox::Stop;

    /// Collect every number in a JSON tree, with the key path it was under.
    fn numbers(v: &serde_json::Value, path: &str, out: &mut Vec<(String, f64)>) {
        match v {
            serde_json::Value::Number(n) => out.push((path.to_string(), n.as_f64().unwrap())),
            serde_json::Value::Array(a) => {
                for x in a {
                    numbers(x, path, out);
                }
            }
            serde_json::Value::Object(m) => {
                for (k, x) in m {
                    numbers(x, &format!("{path}.{k}"), out);
                }
            }
            _ => {}
        }
    }

    fn keys(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Array(a) => a.iter().for_each(|x| keys(x, out)),
            serde_json::Value::Object(m) => {
                for (k, x) in m {
                    out.push(k.clone());
                    keys(x, out);
                }
            }
            _ => {}
        }
    }

    trait Erased {
        fn json(&self) -> serde_json::Value;
    }
    impl<T: Serialize> Erased for T {
        fn json(&self) -> serde_json::Value {
            serde_json::to_value(self).unwrap()
        }
    }

    fn played_world() -> World {
        let mut w = World::generate(21, 8);
        w.apply(Command::TakeCharge(0)).unwrap();
        let mut n = 0;
        let mut rounds = 0;
        while rounds < 5 {
            tidy(&mut w, &mut n);
            if w.apply(Command::Advance).unwrap().stop == Some(Stop::MatchDay) {
                w.apply(Command::PlayRound).unwrap();
                rounds += 1;
            }
        }
        w
    }

    #[test]
    fn the_view_has_a_fog_over_other_clubs() {
        let w = played_world();
        let view = w.view(0).expect("a view of the managed club");
        let json = serde_json::to_value(&view).unwrap();
        let text = serde_json::to_string(&json).unwrap();
        // The view round-trips as data.
        assert!(text.len() > 1000);

        // Nobody else gets a view.
        assert!(w.view(1).is_none());

        // The hidden keys appear nowhere, for anyone.
        let mut all = Vec::new();
        keys(&json, &mut all);
        for hidden in [
            "proneness",
            "recovery",
            "professionalism",
            "ambition",
            "determination",
            "temperament",
            "leadership",
            "consistency",
            "loyalty",
            "personality",
            "presence",
            "xp",
            "morale",
            "belief",
            "familiar",
        ] {
            assert!(
                !all.iter().any(|k| k == hidden),
                "key {hidden} is in the view"
            );
        }

        // Other clubs: no budget, no tactic details, no reserves.
        let clubs = json["league"]["clubs"].as_array().unwrap();
        assert_eq!(clubs.len(), 8);
        for c in clubs.iter().filter(|c| c["club"] != 0) {
            let mut ks = Vec::new();
            keys(c, &mut ks);
            for private in [
                "budget",
                "rotation",
                "pace",
                "three_bias",
                "crash_boards",
                "aggression",
                "schedule",
                "reserves",
                "playbook",
                "board",
            ] {
                assert!(!ks.iter().any(|k| k == private), "club key {private} shown");
            }
        }

        // Every other player: ranges that contain the truth, not the truth.
        let mut checked = 0;
        for t in 1..8 {
            for &pid in &w.teams[t].roster {
                let p = &w.players[pid as usize];
                let pv = view.league.clubs[t]
                    .squad
                    .iter()
                    .find(|q| q.id == pid)
                    .unwrap();
                assert!(!pv.own);
                let known = w.scouting.knowledge(pid);
                for a in Attr::ALL {
                    let r = pv.attrs.0.iter().find(|(n, _)| *n == a.name()).unwrap().1;
                    let truth = p.attrs[a];
                    assert!(
                        r.lo <= truth + 1e-3 && truth <= r.hi + 1e-3,
                        "{} {a:?}",
                        p.name
                    );
                    if known < 0.95 {
                        assert!(r.hi - r.lo > 0.1, "range collapsed for an unscouted man");
                    }
                }
                assert!(pv.overall.lo <= p.overall() + 1e-3 && p.overall() <= pv.overall.hi + 1e-3);
                assert!(pv.potential.lo <= p.potential + 1e-3);
                assert!(p.potential <= pv.potential.hi + 1e-3);
                // No number in his subtree is a true figure (within rounding
                // of the display), except ranges' ends which differ by design.
                let sub = serde_json::to_value(pv).unwrap();
                let mut nums = Vec::new();
                numbers(&sub, "", &mut nums);
                let truths: Vec<f64> = Attr::ALL
                    .iter()
                    .map(|&a| p.attrs[a] as f64)
                    .chain([p.potential as f64, p.overall() as f64, p.condition as f64])
                    .collect();
                for (path, n) in &nums {
                    let figure = [".attrs", ".overall", ".potential"]
                        .iter()
                        .any(|p| path.starts_with(p));
                    // The scale's ends are clamps, not information.
                    if !figure || *n == 1.0 || *n == 20.0 {
                        continue;
                    }
                    for t in &truths {
                        // Display ranges are rounded outwards to 0.1, so an
                        // exact float match would be a leak.
                        assert!(
                            (n - t).abs() > 1e-6 || *n == 0.0,
                            "{} {path}={n} equals a true figure",
                            p.name
                        );
                    }
                }
                // Character, condition and mood only once well scouted.
                assert_eq!(pv.character.is_some(), known >= CHARACTER_KNOWN_AT);
                assert_eq!(pv.condition_pct.is_some(), known >= CHARACTER_KNOWN_AT);
                assert_eq!(pv.mood.is_some(), known >= CHARACTER_KNOWN_AT);
                assert!(pv.sharp_pct.is_none());
                if known < 0.5 {
                    assert!(pv.medical.starts_with("Not seen enough"));
                }
                checked += 1;
            }
        }
        assert!(checked > 50);
        // Fame is public; media work and manner are the manager's own.
        for club in view.league.clubs.iter().take(8).skip(1) {
            for pv in &club.squad {
                let p = &w.players[pv.id as usize];
                assert_eq!(pv.fame, r1(p.renown.fame));
                assert!(pv.media_work.is_none() && pv.manner_in_public.is_none());
                assert!(pv.shirt_sales_per_round.is_none());
            }
        }
        for pv in &view.own.squad {
            assert!(pv.media_work.is_some() && pv.manner_in_public.is_some());
        }

        // Own players: exact, with labels where the numbers are hidden.
        for pv in &view.own.squad {
            assert!(pv.own);
            let p = &w.players[pv.id as usize];
            let close = pv.attrs.0[0].1;
            assert_eq!(close.lo, close.hi);
            assert_eq!(pv.character, Some(p.personality.kind().label()));
            assert_eq!(pv.medical, medical(p, 1.0));
            // Potential is a range, never the number itself.
            assert!(pv.potential.hi - pv.potential.lo >= 0.0);
        }
    }

    #[test]
    fn scouting_narrows_what_the_view_shows() {
        let mut w = played_world();
        let target = *w.teams[3]
            .roster
            .iter()
            .max_by_key(|&&p| w.games_played[p as usize])
            .unwrap();
        let width = |w: &World| {
            let v = w.view_player(0, target).unwrap();
            v.attrs.0.iter().map(|a| a.1.hi - a.1.lo).sum::<f32>()
        };
        let before = width(&w);
        w.apply(Command::Shortlist {
            player: target,
            on: true,
        })
        .unwrap();
        let mut n = 0;
        let mut rounds = 0;
        while rounds < 8 {
            tidy(&mut w, &mut n);
            if w.apply(Command::Advance).unwrap().stop == Some(Stop::MatchDay) {
                w.apply(Command::PlayRound).unwrap();
                rounds += 1;
            }
        }
        assert!(width(&w) < before, "scouting did not narrow the ranges");
        let v = w.view_player(0, target).unwrap();
        assert!(!v.scout_reports.is_empty());
        assert_eq!(v.shortlisted, Some(true));
    }

    #[test]
    fn the_narrow_queries_agree_with_the_whole() {
        let w = played_world();
        let view = w.view(0).unwrap();
        let j = |v: &dyn Erased| v.json();
        assert_eq!(j(&w.view_table()), j(&view.league.table));
        assert_eq!(j(&w.view_inbox(0).unwrap()), j(&view.inbox));
        let (squad, reserves) = w.view_squad(0).unwrap();
        assert_eq!(j(&squad), j(&view.own.squad));
        assert_eq!(j(&reserves), j(&view.own.reserves));
        assert_eq!(j(&w.view_summary(0).unwrap()), j(&view.own.summary));
        let b = w.view_box_score(0).unwrap();
        assert!(b.sides[0].len() + b.sides[1].len() > 10);
        // The calendar says what Advance will do.
        let stop = view.own.summary.calendar.next_stop;
        assert!(["match_day", "mail", "draft"].contains(&stop));
    }

    #[test]
    fn money_fans_and_retirement_are_in_the_view() {
        let mut w = played_world();
        let view = w.view(0).unwrap();
        let f = &view.own.finance;
        assert_eq!(f.price.in_force, w.ticket_price(0));
        assert!(f.price.min < f.price.max);
        assert!(
            f.price
                .preview
                .as_ref()
                .is_some_and(|p| p.points.len() == 3)
        );
        assert_eq!(f.budgets.wage_budget, w.books(0).wage_budget);
        assert_eq!(f.facilities.len(), 3);
        assert!(f.fans.base > 1000 && !f.fans.moved.is_empty());
        assert!(f.tax.cap > 0 && f.debt.overdraft_limit > 0);
        assert!(!view.league.names.biggest.is_empty());
        // Nothing of other clubs' money beyond a word.
        let other = serde_json::to_value(&view.league.clubs[1]).unwrap();
        assert!(other["money"].is_string());
        // A retired player is off the market but his page still opens.
        let gone = w.teams[3].roster[0];
        w.retire(gone);
        let market = w.view_market(0).unwrap();
        assert!(market.free_agents.iter().all(|p| p.id != gone));
        let pv = w.view_player(0, gone).unwrap();
        assert!(pv.retired_after_season.is_some() && pv.club.is_none());
    }
}
