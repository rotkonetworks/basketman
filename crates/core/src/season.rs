//! What happens between seasons, the way the NBA does it: draft picks are
//! assets clubs own and trade, the worst team picks first from a class of
//! young prospects, contracts run down, and then it all starts again.

use serde::{Deserialize, Serialize};

use crate::engine::BoxLine;
use crate::inbox::{Kind, MAX_ROSTER, money};
use crate::league::{Fixture, Standing, TeamId, TeamStats, World};
use crate::player::Attr;
use crate::player::{Player, PlayerId};

/// Drafts a club's picks exist for ahead of time: this season's and the
/// two after it.
const YEARS_AHEAD: u16 = 3;
/// Payroll above which a trade has to send out about as much salary as it
/// brings in.
pub const SALARY_CAP: i64 = 950_000;

/// The right to one selection in one draft.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pick {
    /// Season at whose end the draft is held, counting from 0.
    pub year: u16,
    /// 1 or 2.
    pub round: u8,
    /// The club whose finishing position decides where it falls.
    pub original: TeamId,
    pub owner: TeamId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DraftSlot {
    pub round: u8,
    pub original: TeamId,
    pub owner: TeamId,
    pub taken: Option<PlayerId>,
}

/// A draft in progress.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Draft {
    /// In picking order.
    pub slots: Vec<DraftSlot>,
    /// Prospects not yet taken.
    pub pool: Vec<PlayerId>,
}

impl Draft {
    /// Index of the selection being made.
    pub fn at(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.taken.is_none())
    }

    pub fn on_clock(&self) -> Option<TeamId> {
        self.at().map(|i| self.slots[i].owner)
    }
}

/// How a finished season ended.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeasonRecord {
    pub season: u16,
    /// Final table: club, wins, losses.
    pub table: Vec<(TeamId, u16, u16)>,
    /// Season totals for and against, indexed by `TeamId`.
    pub stats: Vec<TeamStats>,
    /// Each club's best scorer per game: player and average.
    pub scorers: Vec<Option<(PlayerId, f32)>>,
    /// Playoff winners; the top of the table if no playoffs were held.
    pub champion: TeamId,
    pub runner_up: Option<TeamId>,
    pub mvp: Option<PlayerId>,
    /// Every division's final table, the top one first; `table` is the
    /// first of them.
    #[serde(default)]
    pub divisions: Vec<Vec<(TeamId, u16, u16)>>,
    /// Who won each division below the top, by division number.
    #[serde(default)]
    pub division_champions: Vec<TeamId>,
}

/// A best-of-three playoff series. The higher seed hosts games one and
/// three.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Series {
    pub high: TeamId,
    pub low: TeamId,
    /// Wins of `high`, then of `low`.
    pub wins: [u8; 2],
    pub is_final: bool,
}

impl Series {
    pub fn winner(&self) -> Option<TeamId> {
        match self.wins {
            [2, _] => Some(self.high),
            [_, 2] => Some(self.low),
            _ => None,
        }
    }

    pub fn loser(&self) -> Option<TeamId> {
        self.winner()
            .map(|w| if w == self.high { self.low } else { self.high })
    }

    fn has(&self, a: TeamId, b: TeamId) -> bool {
        (self.high == a && self.low == b) || (self.high == b && self.low == a)
    }
}

/// One candidate in the race for most valuable player.
#[derive(Clone, Debug)]
pub struct MvpCase {
    pub player: PlayerId,
    pub team: TeamId,
    pub games: u16,
    /// Plus/minus per game.
    pub plus_minus: f32,
    pub rating: f32,
    pub points: f32,
    /// Share of games his team won.
    pub winning: f32,
    pub score: f32,
}

impl SeasonRecord {
    /// 1-based finishing position, in the club's own division.
    pub fn place(&self, team: TeamId) -> usize {
        if let Some(t) = self.divisions.iter().find(|t| t.iter().any(|r| r.0 == team)) {
            return t.iter().position(|r| r.0 == team).map_or(0, |p| p + 1);
        }
        self.table
            .iter()
            .position(|t| t.0 == team)
            .map_or(0, |p| p + 1)
    }

    /// The division the club played in that season, 0 being the top.
    pub fn division_of(&self, team: TeamId) -> Option<usize> {
        if self.divisions.is_empty() {
            return self.table.iter().any(|t| t.0 == team).then_some(0);
        }
        self.divisions
            .iter()
            .position(|t| t.iter().any(|r| r.0 == team))
    }
}

/// One season of one player's career, kept after the box scores are gone.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CareerLine {
    pub season: u16,
    /// Club he finished the season with.
    pub team: Option<TeamId>,
    pub age: u8,
    /// Overall rating at the end of the season.
    pub overall: f32,
    pub games: u16,
    pub totals: BoxLine,
}

pub(crate) fn initial_picks(teams: usize) -> Vec<Pick> {
    (0..YEARS_AHEAD)
        .flat_map(|year| picks_for(year, teams))
        .collect()
}

fn picks_for(year: u16, teams: usize) -> Vec<Pick> {
    (1..=2)
        .flat_map(|round| {
            (0..teams).map(move |t| Pick {
                year,
                round,
                original: t,
                owner: t,
            })
        })
        .collect()
}

impl World {
    /// "2nd round 2027 (via Riga Seagulls)".
    pub fn pick_label(&self, i: usize) -> String {
        let p = &self.picks[i];
        let round = if p.round == 1 { "1st" } else { "2nd" };
        let when = match p.year - self.season {
            0 => "this year".to_string(),
            1 => "next year".to_string(),
            n => format!("in {n} years"),
        };
        if p.original == p.owner {
            format!("{round} round, {when}")
        } else {
            format!(
                "{round} round, {when} (from {})",
                self.teams[p.original].name
            )
        }
    }

    /// What a pick is worth in a trade. A first from a bad team is gold;
    /// the further off the draft, the less anybody will pay today.
    pub fn pick_value(&self, i: usize) -> i64 {
        let p = &self.picks[i];
        let n = self.division_size(p.original).max(2) as i64;
        // Where the original club stands now is the best guess at where
        // the pick will fall; before any games, the middle.
        let place = if self.round > 0 {
            self.table_of(self.teams[p.original].division)
                .iter()
                .position(|&t| t == p.original)
                .unwrap_or(0) as i64
        } else {
            n / 2
        };
        let base = match p.round {
            1 => 40_000 + 110_000 * place / (n - 1),
            _ => 8_000 + 14_000 * place / (n - 1),
        };
        let mut value = base;
        for _ in self.season..p.year {
            value = value * 8 / 10;
        }
        value / 1000 * 1000
    }

    pub fn payroll(&self, team: TeamId) -> i64 {
        self.teams[team]
            .roster
            .iter()
            .map(|&p| self.players[p as usize].contract.salary as i64)
            .sum()
    }

    // ---- the draft -------------------------------------------------------

    /// Open the draft: order the picks, bring in the class.
    pub fn begin_draft(&mut self) {
        if self.draft.is_some() {
            return;
        }
        let n = self.teams.len();
        // The lowest division picks first, the worst record first within a
        // division, in both rounds.
        let order: Vec<TeamId> = (0..self.division_count())
            .rev()
            .flat_map(|d| self.table_of(d).into_iter().rev())
            .collect();
        let mut slots = Vec::new();
        for round in 1..=2u8 {
            for &original in &order {
                let owner = self
                    .picks
                    .iter()
                    .find(|p| p.year == self.season && p.round == round && p.original == original)
                    .map_or(original, |p| p.owner);
                slots.push(DraftSlot {
                    round,
                    original,
                    owner,
                    taken: None,
                });
            }
        }
        let mut pool = Vec::new();
        // The class is as deep as the league is big.
        let step = if self.division_count() > 1 {
            0.14 * 22.0 / (2 * n + 6) as f32
        } else {
            0.14
        };
        for i in 0..2 * n + 6 {
            let id = self.players.len() as PlayerId;
            // The class thins out: a few who could start tomorrow, then
            // projects.
            let quality = 9.8 - i as f32 * step;
            let slot = self.rng.range(0, 5) as usize;
            let age = 19 + self.rng.range(0, 3) as u8;
            let mut p = crate::league::generate_player_at(id, slot, quality, age, &mut self.rng);
            p.potential = (p.overall() + crate::league::age_gap(age) + 1.0 + self.rng.f32() * 3.5).min(20.0);
            p.assign_signatures();
            p.contract.salary = 15_000;
            p.contract.years_left = 2;
            self.players.push(p);
            self.season_stats.push(BoxLine::default());
            self.games_played.push(0);
            pool.push(id);
        }
        self.scouting.grow(self.players.len());
        self.draft = Some(Draft { slots, pool });

        if let Some(me) = self.watch {
            let draft = self.draft.as_ref().expect("just set");
            let mine: Vec<String> = draft
                .slots
                .iter()
                .enumerate()
                .filter(|(_, s)| s.owner == me)
                .map(|(i, s)| format!("pick {} (round {})", i + 1, s.round))
                .collect();
            let body = format!(
                "The season is over and the draft is open: {} prospects, two rounds, the \
                 {}.\n\n{}\n\nOur scouts only know the class roughly, \
                 so the figures are ranges. Players you shortlisted during the year are no \
                 better known: none of these played in our league.\n\nWhen the draft is done \
                 the new season starts.",
                draft.pool.len(),
                if self.division_count() > 1 {
                    "every club picks, the lowest division first and the worst record first within a division"
                } else {
                    "the worst record picks first"
                },
                if mine.is_empty() {
                    "We traded our picks away and will only be watching.".to_string()
                } else {
                    format!("We have {}.", mine.join(" and "))
                }
            );
            let players = draft.pool.iter().copied().take(5).collect();
            self.post(
                self.day,
                Kind::League,
                "League office",
                "The draft",
                body,
                players,
            );
        }
        self.draft_ai();
    }

    /// The prospect a club's front office likes best.
    fn best_available(&self) -> Option<PlayerId> {
        let club = self.draft.as_ref()?.on_clock()?;
        let like = |p: &PlayerId| (self.draft_grade(club, *p) * 100.0) as i32;
        self.draft.as_ref()?.pool.iter().copied().max_by_key(like)
    }

    fn select(&mut self, player: PlayerId) {
        let Some(draft) = &mut self.draft else { return };
        let Some(i) = draft.at() else { return };
        draft.pool.retain(|&p| p != player);
        draft.slots[i].taken = Some(player);
        let (owner, round) = (draft.slots[i].owner, draft.slots[i].round);
        // The rookie scale: the higher the pick, the better the deal.
        let p = &mut self.players[player as usize];
        p.contract.salary = if round == 1 {
            (62_000 - i as u32 * 2_500).max(30_000)
        } else {
            20_000
        };
        p.contract.years_left = if round == 1 { 3 } else { 2 };
        self.teams[owner].roster.push(player);
    }

    /// Other clubs pick until it is the user's turn or the draft is over.
    fn draft_ai(&mut self) {
        loop {
            let Some(draft) = &self.draft else { return };
            match draft.on_clock() {
                None => return self.finish_draft(),
                Some(team) if Some(team) == self.watch => return,
                Some(_) => match self.best_available() {
                    Some(p) => self.select(p),
                    None => return self.finish_draft(),
                },
            }
        }
    }

    /// The user's selection.
    pub fn draft_take(&mut self, player: PlayerId) {
        let ours = self
            .draft
            .as_ref()
            .is_some_and(|d| d.on_clock() == self.watch && d.pool.contains(&player));
        if ours {
            self.select(player);
            self.draft_ai();
        }
    }

    /// Let the assistant make this pick, or all of ours.
    pub fn draft_auto(&mut self, all: bool) {
        while self
            .draft
            .as_ref()
            .is_some_and(|d| d.on_clock().is_some() && d.on_clock() == self.watch)
        {
            let Some(p) = self.best_available() else {
                break;
            };
            self.select(p);
            self.draft_ai();
            if !all {
                break;
            }
        }
    }

    fn finish_draft(&mut self) {
        let Some(draft) = self.draft.take() else {
            return;
        };
        let me = self.watch;
        let mut lines = Vec::new();
        let mut ours = Vec::new();
        for (i, s) in draft.slots.iter().enumerate() {
            let Some(p) = s.taken else { continue };
            let player = &self.players[p as usize];
            if i < 5 || Some(s.owner) == me {
                lines.push(format!(
                    "{:>2}. {}  {} ({}, {})",
                    i + 1,
                    self.teams[s.owner].name,
                    player.name,
                    player.position.label(),
                    player.age
                ));
            }
            if Some(s.owner) == me {
                ours.push(p);
            }
        }
        let year = self.season;
        self.picks.retain(|p| p.year != year);
        self.picks
            .extend(picks_for(year + YEARS_AHEAD, self.teams.len()));
        self.last_draft = Some(draft);
        self.new_season();
        if me.is_some() {
            let body = format!(
                "The draft is done.\n\n{}\n\n{}",
                lines.join("\n"),
                if ours.is_empty() {
                    "We did not pick.".to_string()
                } else {
                    "Our rookies have signed and are in the squad, outside the rotation \
                     until you put them in."
                        .to_string()
                }
            );
            self.post(
                self.day,
                Kind::League,
                "League office",
                "Draft results",
                body,
                ours,
            );
        }
    }

    // ---- the turn of the year --------------------------------------------

    /// Close the books on the season and set up the next one.
    fn new_season(&mut self) {
        let me = self.watch;
        let tables = self.division_tables();
        let table = tables[0].clone();
        let scorers = (0..self.teams.len())
            .map(|t| {
                self.teams[t]
                    .roster
                    .iter()
                    .filter(|&&p| self.games_played[p as usize] > 0)
                    .map(|&p| {
                        let per = self.season_stats[p as usize].pts as f32
                            / self.games_played[p as usize] as f32;
                        (p, per)
                    })
                    .max_by(|a, b| a.1.total_cmp(&b.1))
            })
            .collect();
        let champion_id = self.champion.unwrap_or(table[0]);
        let runner_up = self
            .playoffs
            .iter()
            .find(|s| s.is_final)
            .and_then(|s| s.loser());
        let mvp = self.mvp_race().first().map(|c| c.player);
        // A club over its wage budget keeps renewals at current pay; the
        // books are looked at before they are closed.
        let capped = self.renewals_capped();
        self.econ_close_season(&tables, champion_id);
        let grown = self.develop();
        let rows = |t: &Vec<TeamId>| -> Vec<(TeamId, u16, u16)> {
            t.iter()
                .map(|&c| (c, self.standings[c].wins, self.standings[c].losses))
                .collect()
        };
        self.history.push(SeasonRecord {
            champion: champion_id,
            runner_up,
            mvp,
            season: self.season,
            table: rows(&table),
            stats: self.team_stats.clone(),
            scorers,
            divisions: tables.iter().map(rows).collect(),
            division_champions: tables.iter().skip(1).map(|t| t[0]).collect(),
        });
        // The season's box scores go; what each player did in it stays.
        if self.careers.len() < self.players.len() {
            self.careers.resize(self.players.len(), Vec::new());
        }
        for p in 0..self.players.len() {
            let games = self.games_played[p];
            let team = self.team_of(p as PlayerId);
            if games == 0 && team.is_none() {
                continue;
            }
            self.careers[p].push(CareerLine {
                season: self.season,
                team,
                age: self.players[p].age,
                overall: self.players[p].overall(),
                games,
                totals: self.season_stats[p],
            });
        }
        self.fame_close_season(&table, champion_id, runner_up, mvp);
        self.fans_close_season(&tables, champion_id);
        // The boards judge the season in the divisions the clubs played in.
        self.club_close_season(&tables, champion_id);
        // Promotion and relegation: the clubs change division before the
        // new schedule is drawn.
        let moved = self.promote_and_relegate(&tables);
        self.club_after_moves(&moved);
        self.summer_retirements();
        let champion = self.teams[champion_id].name.clone();
        let mvp_line = mvp.map(|p| {
            let club = self.team_of(p).map_or("", |t| self.teams[t].name.as_str());
            format!(
                "{} ({club}) is the most valuable player.",
                self.players[p as usize].name
            )
        });
        let place = me
            .and_then(|m| tables.iter().find_map(|t| t.iter().position(|&x| x == m)))
            .map_or(0, |p| p + 1);

        // A year older, a year less on every contract.
        let mut renewed = Vec::new();
        let mut released = Vec::new();
        for t in 0..self.teams.len() {
            let mut roster = self.teams[t].roster.clone();
            for &p in &roster {
                let player = &mut self.players[p as usize];
                player.age += 1;
                player.condition = 1.0;
                player.sharp = player.sharp.max(0.85);
                // The summer heals most things and lets grievances cool.
                player.morale += (0.5 - player.morale) * 0.6;
                player.complained = false;
                if let Some(inj) = &mut player.injury {
                    inj.weeks_left = inj.weeks_left.saturating_sub(14);
                    if inj.weeks_left == 0 {
                        player.injury = None;
                    }
                }
                player.contract.years_left = player.contract.years_left.saturating_sub(1);
                if player.contract.years_left == 0 {
                    // Everybody is offered two more years at what he is
                    // now worth. Free agency proper is still to come.
                    let ov = player.overall();
                    let asked = (ov * ov * ov * 60.0 * crate::fame::wage_mult(player)) as u32;
                    player.contract.salary = if capped[t] {
                        asked.min(player.contract.salary)
                    } else {
                        asked
                    };
                    player.contract.years_left = 2;
                    if Some(t) == me {
                        renewed.push(format!(
                            "{} ({} a year)",
                            player.name,
                            money(player.contract.salary as i64)
                        ));
                    }
                }
            }
            // Fifteen is the limit: the least valuable go.
            roster.sort_by_key(|&p| std::cmp::Reverse(self.value_to(t, p)));
            while roster.len() > MAX_ROSTER {
                // A man who has announced his last season is not let go to
                // make room: the least valuable of the others goes.
                let Some(k) = roster
                    .iter()
                    .rposition(|&p| !self.players[p as usize].renown.farewell)
                else {
                    break;
                };
                let p = roster.remove(k);
                if Some(t) == me {
                    released.push(self.players[p as usize].name.clone());
                }
                self.teams[t].roster.retain(|&q| q != p);
            }
            self.refit(t);
        }

        self.after_retirements();
        let n = self.teams.len();
        self.officials_new_season();
        self.season += 1;
        self.round = 0;
        self.day = 0;
        self.draft_day = 0;
        self.fixtures = crate::league::schedule_tiers(&self.teams);
        self.standings = vec![Standing::default(); n];
        self.team_stats = vec![TeamStats::default(); n];
        for line in &mut self.season_stats {
            *line = BoxLine::default();
        }
        self.games_played.fill(0);
        self.playoffs.clear();
        self.champion = None;
        self.replay = None;
        self.tapes.clear();
        self.scouting.reports.clear();
        self.desk.bids.clear();
        // Old mail stays readable; it just is not dated in the new year.
        for m in &mut self.desk.messages {
            m.day = 0;
        }

        // Every season opens with a training camp.
        self.begin_preseason();
        self.reserve_summer();
        let staff_news = self.staff_summer(&crate::league::FIRST_NAMES, &crate::league::LAST_NAMES);
        self.econ_open_season();
        self.career_fill_vacancies();
        self.club_open_season();

        let Some(me) = me else { return };
        let suffix = match place % 10 {
            1 if place != 11 => "st",
            2 if place != 12 => "nd",
            3 if place != 13 => "rd",
            _ => "th",
        };
        let was = moved.iter().find(|m| m.team == me).map(|m| m.from);
        let my_div = was.unwrap_or(self.teams[me].division);
        let mut body = if my_div == 0 {
            format!("{champion} are champions. We finished {place}{suffix}.\n\n")
        } else {
            format!(
                "{champion} are champions of the First Division. We played in the {}, \
                 finished {place}{suffix} and {} won it.\n\n",
                self.division_name(my_div),
                self.teams[tables[my_div][0]].name
            )
        };
        if let Some(line) = mvp_line {
            body += &format!("{line}\n\n");
        }
        let ours: Vec<String> = grown
            .iter()
            .filter(|g| self.teams[me].roster.contains(&g.0))
            .map(|g| format!("{} ({:+.1})", self.players[g.0 as usize].name, g.1))
            .collect();
        if !ours.is_empty() {
            body += &format!(
                "The year changed these players most: {}. Minutes, form and winning all \
                 feed development; age takes it away.\n\n",
                ours.join(", ")
            );
        }
        if !renewed.is_empty() {
            body += &format!(
                "Contracts renewed for two years: {}.\n\n",
                renewed.join(", ")
            );
        }
        if !released.is_empty() {
            body += &format!(
                "Released to get down to {MAX_ROSTER} players: {}.\n\n",
                released.join(", ")
            );
        }
        if !staff_news.is_empty() {
            body += &format!("{}\n\n", staff_news.join(" "));
        }
        body += "New coaches and scouts are looking for work; see the Staff page.\n\n";
        body += &format!(
            "Season {} starts now. The payroll is {} against a cap of {}, and the budget \
             stands at {}.",
            self.season + 1,
            money(self.payroll(me)),
            money(SALARY_CAP),
            money(self.teams[me].budget)
        );
        let subject = format!("Season {} begins", self.season + 1);
        self.post(0, Kind::Board, "The board", subject, body, vec![]);
        self.preview();
    }

    // ---- playoffs --------------------------------------------------------

    /// After a round: count playoff results, and put the next playoff games
    /// on the schedule. The top four play best-of-three semifinals (1 v 4,
    /// 2 v 3) and a best-of-three final.
    pub(crate) fn playoffs_step(&mut self, played: &[usize]) {
        if self.division_teams(0).len() < 4 || self.champion.is_some() {
            return;
        }
        for &i in played {
            let f = &self.fixtures[i];
            let Some(r) = f.result.as_ref().filter(|_| f.playoff) else {
                continue;
            };
            let winner = if r.score[0] > r.score[1] {
                f.home
            } else {
                f.away
            };
            if let Some(s) = self
                .playoffs
                .iter_mut()
                .find(|s| s.winner().is_none() && s.has(f.home, f.away))
            {
                s.wins[(winner == s.low) as usize] += 1;
            }
        }
        let table = self.table_of(0);
        let seed = |t: TeamId| table.iter().position(|&x| x == t).unwrap_or(usize::MAX);
        let sunday = self.round_base(self.round.saturating_sub(1)) + crate::inbox::MATCH_DAY + 1;
        if self.playoffs.is_empty() {
            if !self.season_over() {
                return;
            }
            for (high, low) in [(table[0], table[3]), (table[1], table[2])] {
                self.playoffs.push(Series {
                    high,
                    low,
                    wins: [0; 2],
                    is_final: false,
                });
            }
            if let Some(me) = self.watch {
                let name = |t: TeamId| self.teams[t].name.clone();
                let body = format!(
                    "The regular season is over and the top four play for the title, every \
                     series best of three, the higher seed at home in games one and three.\n\n\
                     Semifinals\n1. {} v 4. {}\n2. {} v 3. {}\n\n{}",
                    name(table[0]),
                    name(table[3]),
                    name(table[1]),
                    name(table[2]),
                    if seed(me) < 4 {
                        "We are in."
                    } else {
                        "We are not among them. Our season is over; the draft follows the final."
                    }
                );
                self.post(
                    sunday,
                    Kind::League,
                    "League office",
                    "The playoffs",
                    body,
                    vec![],
                );
            }
        }
        let semis: Vec<Option<TeamId>> = self
            .playoffs
            .iter()
            .filter(|s| !s.is_final)
            .map(|s| s.winner())
            .collect();
        if !self.playoffs.iter().any(|s| s.is_final)
            && let [Some(a), Some(b)] = semis[..]
        {
            let (high, low) = if seed(a) < seed(b) { (a, b) } else { (b, a) };
            self.playoffs.push(Series {
                high,
                low,
                wins: [0; 2],
                is_final: true,
            });
        }
        if let Some(winner) = self
            .playoffs
            .iter()
            .find(|s| s.is_final)
            .and_then(|s| s.winner())
        {
            self.champion = Some(winner);
            if self.watch.is_some() {
                let body = format!(
                    "{} have won the final and are champions.",
                    self.teams[winner].name
                );
                let subject = format!("{} are champions", self.teams[winner].name);
                self.post(sunday, Kind::League, "League office", subject, body, vec![]);
            }
            return;
        }
        let round = self.round;
        let base = self.round_base(round);
        for (k, s) in self
            .playoffs
            .iter()
            .filter(|s| s.winner().is_none())
            .enumerate()
        {
            // Game two is at the lower seed's.
            let (home, away) = if s.wins[0] + s.wins[1] == 1 {
                (s.low, s.high)
            } else {
                (s.high, s.low)
            };
            self.fixtures.push(Fixture {
                round,
                home,
                away,
                result: None,
                playoff: true,
                day: base + 5 + (k % 2) as u32,
                division: 0,
            });
        }
    }

    // ---- awards and development ------------------------------------------

    /// The candidates for most valuable player, best case first. Plus/minus
    /// leads the argument, the box score backs it up, and it has to have
    /// come with winning: only players from the top half of the table are
    /// considered, and a deep playoff run counts.
    pub fn mvp_race(&self) -> Vec<MvpCase> {
        let table = self.table_of(0);
        let mut cases = Vec::new();
        for (place, &t) in table.iter().enumerate() {
            let s = &self.standings[t];
            let team_games = s.wins + s.losses;
            if team_games == 0 || place * 2 >= table.len() {
                continue;
            }
            let winning = s.wins as f32 / team_games as f32;
            let finalist = self
                .playoffs
                .iter()
                .any(|x| x.is_final && (x.high == t || x.low == t));
            let run = if self.champion == Some(t) {
                2.0
            } else if finalist {
                1.0
            } else {
                0.0
            };
            for &p in &self.teams[t].roster {
                let games = self.games_played[p as usize];
                // He has to have been there for most of it.
                if games * 10 < team_games * 6 {
                    continue;
                }
                let l = &self.season_stats[p as usize];
                let g = games as f32;
                let (plus_minus, rating, points) =
                    (l.pm as f32 / g, l.rating_per(games), l.pts as f32 / g);
                cases.push(MvpCase {
                    player: p,
                    team: t,
                    games,
                    plus_minus,
                    rating,
                    points,
                    winning,
                    score: plus_minus * 0.5
                        + (rating - 5.0) * 1.5
                        + points * 0.1
                        + winning * 5.0
                        + run,
                });
            }
        }
        cases.sort_by(|a, b| b.score.total_cmp(&a.score));
        cases
    }

    /// The off-season's change in every rostered player, from what the
    /// year was like for him. Returns the user-visible movers as (player,
    /// change in overall).
    fn develop(&mut self) -> Vec<(PlayerId, f32)> {
        let finalists: Vec<TeamId> = self
            .playoffs
            .iter()
            .filter(|s| s.is_final)
            .flat_map(|s| [s.high, s.low])
            .collect();
        let mut out = Vec::new();
        for t in 0..self.teams.len() {
            let s = &self.standings[t];
            let winning = s.wins as f32 / (s.wins + s.losses).max(1) as f32;
            let run = if self.champion == Some(t) {
                0.3
            } else if finalists.contains(&t) {
                0.15
            } else {
                0.0
            };
            let squad: Vec<PlayerId> = self.teams[t]
                .roster
                .iter()
                .chain(&self.teams[t].reserves)
                .copied()
                .collect();
            for p in squad {
                let games = self.games_played[p as usize];
                let line = self.season_stats[p as usize];
                let minutes = line.secs as f32 / 60.0 / games.max(1) as f32;
                let player = &mut self.players[p as usize];
                let before = player.overall();
                let room = (player.potential - before).max(0.0);
                // Young players grow by playing, more when it went well
                // and when the team won; a year on the bench is a year lost.
                let youth = match player.age {
                    ..=21 => 1.0,
                    22..=24 => 0.7,
                    25..=27 => 0.3,
                    _ => 0.0,
                };
                let played = (games as f32 / 14.0).min(1.0) * (minutes / 24.0).min(1.2);
                // Reserve games count too, for rather less.
                let second = self.reserve_games.get(p as usize).copied().unwrap_or(0);
                let second_minutes = self
                    .reserve_stats
                    .get(p as usize)
                    .map_or(0.0, |l| l.secs as f32 / 60.0 / second.max(1) as f32);
                let played = played
                    .max(0.6 * (second as f32 / 14.0).min(1.0) * (second_minutes / 24.0).min(1.2));
                let form = if games > 0 {
                    (line.rating_per(games) - 5.5) * 0.12
                } else {
                    0.0
                };
                let success = (winning - 0.5) * 0.5 + run;
                let mut gain = youth * (0.35 * played + form + success * 0.5).max(-0.1);
                gain = gain.min(room + 0.3);
                // Winning and playing well can lift a young player's
                // ceiling a little; losing years lower it.
                if player.age <= 24 {
                    player.potential = (player.potential
                        + (success * 0.4 + form * 0.5).clamp(-0.3, 0.5))
                    .clamp(1.0, 20.0);
                }
                // From 31 the body goes first.
                let decline = match player.age {
                    ..=30 => 0.0,
                    31..=33 => 0.25,
                    _ => 0.5,
                };
                for a in Attr::ALL {
                    let fall = if a.is_physical() {
                        decline * 1.6
                    } else {
                        decline * 0.4
                    };
                    player.attrs[a] = (player.attrs[a] + gain - fall).clamp(1.0, 20.0);
                }
                let change = player.overall() - before;
                if change.abs() >= 0.15 {
                    out.push((p, change));
                }
            }
        }
        out.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
        out
    }

    /// A player's finished seasons, oldest first.
    pub fn career(&self, p: PlayerId) -> &[CareerLine] {
        self.careers.get(p as usize).map_or(&[], |c| c.as_slice())
    }

    /// Players nobody has under contract.
    pub fn free_agents(&self) -> Vec<&Player> {
        let mut taken = vec![false; self.players.len()];
        for t in &self.teams {
            for &p in t.roster.iter().chain(&t.reserves) {
                taken[p as usize] = true;
            }
        }
        let drafting = self.draft.as_ref();
        self.players
            .iter()
            .filter(|p| {
                !p.is_retired()
                    && !taken[p.id as usize]
                    && drafting.is_none_or(|d| !d.pool.contains(&p.id))
            })
            .collect()
    }
}
