//! Where a club's money comes from, besides the board's opening grant.
//!
//! Income: gate receipts at every home game (attendance times the club's
//! ticket price, playoff games dearer), sponsors (see `sponsor.rs`, paid a
//! little each round of the regular season) and league prize money at the
//! season's end. Outgoings are the wages `complete_round` already takes,
//! and what the club spends on its hall.
//!
//! A club whose budget is below zero is not bankrupt: the board freezes
//! spending, which means no bids, hires, renovations or expansions until
//! the books are back in the black. Income keeps flowing.
//!
//! Nothing here draws from `World::rng`.

use serde::{Deserialize, Serialize};

use crate::arena::{Arena, EXPANSION_SEATS, MAX_CAPACITY};
use crate::books::Books;
use crate::inbox::{Kind, money};
use crate::league::{TeamId, World};
use crate::rng::Rng;
use crate::sponsor::{Bonus, Sponsor, SponsorDeal, SponsorOffer, pool};

/// Prize money for finishing top of the table; the rest scale down to
/// nothing for last.
pub const PRIZE_TOP: i64 = 50_000;
/// Extra for playing a semifinal, the final, and winning it.
pub const PRIZE_SEMI: i64 = 12_000;
pub const PRIZE_FINAL: i64 = 20_000;
pub const PRIZE_TITLE: i64 = 40_000;

/// The visiting club gets one part in this many of a home gate.
pub const GATE_SHARE: i64 = 12;

/// What a movement of club money was for.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Cat {
    Gate,
    Sponsorship,
    Prize,
    PlayerWages,
    StaffWages,
    TransferIn,
    TransferOut,
    TradeCash,
    Severance,
    Renovation,
    Expansion,
    /// Running the hall on game days.
    Upkeep,
    Other,
    // Appended after `Other` so the positions in old saves keep their meaning.
    /// The league's broadcast pool.
    Broadcast,
    /// Shirts, scarves and the like.
    Merchandise,
    /// Paid by clubs over the salary cap.
    LuxuryTax,
    /// A share of the tax, for clubs under the cap.
    TaxShare,
    /// On an overdraft.
    Interest,
    /// Running the training ground, medical centre and youth setup.
    FacilityRunning,
    /// Building them better.
    FacilityWorks,
}

impl Cat {
    pub const ALL: [Cat; 20] = [
        Cat::Gate,
        Cat::Sponsorship,
        Cat::Prize,
        Cat::PlayerWages,
        Cat::StaffWages,
        Cat::TransferIn,
        Cat::TransferOut,
        Cat::TradeCash,
        Cat::Severance,
        Cat::Renovation,
        Cat::Expansion,
        Cat::Upkeep,
        Cat::Other,
        Cat::Broadcast,
        Cat::Merchandise,
        Cat::LuxuryTax,
        Cat::TaxShare,
        Cat::Interest,
        Cat::FacilityRunning,
        Cat::FacilityWorks,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Cat::Gate => "Gate receipts",
            Cat::Sponsorship => "Sponsorship",
            Cat::Prize => "Prize money",
            Cat::PlayerWages => "Player wages",
            Cat::StaffWages => "Staff wages",
            Cat::TransferIn => "Transfer fees received",
            Cat::TransferOut => "Transfer fees paid",
            Cat::TradeCash => "Cash in trades",
            Cat::Severance => "Severance",
            Cat::Renovation => "Renovation",
            Cat::Expansion => "Expansion",
            Cat::Upkeep => "Arena running costs",
            Cat::Other => "Other",
            Cat::Broadcast => "Broadcast money",
            Cat::Merchandise => "Merchandise",
            Cat::LuxuryTax => "Luxury tax paid",
            Cat::TaxShare => "Luxury tax share",
            Cat::Interest => "Overdraft interest",
            Cat::FacilityRunning => "Facility running costs",
            Cat::FacilityWorks => "Facility building",
        }
    }

    /// True for the money the club earns by being a club, as opposed to
    /// what moves because of deals and penalties.
    pub fn is_income(self) -> bool {
        matches!(
            self,
            Cat::Gate | Cat::Sponsorship | Cat::Prize | Cat::Broadcast | Cat::Merchandise
        )
    }

    pub(crate) fn index(self) -> usize {
        Cat::ALL
            .iter()
            .position(|&c| c == self)
            .unwrap_or(Cat::ALL.len() - 1)
    }
}

/// One club's season in money. Every change of a budget is in here, by
/// category, so `start` plus the sum of `totals` is the budget.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    /// The season it is for (set when it is filed in the history).
    pub season: u16,
    /// The budget when the season began.
    pub start: i64,
    /// The budget when the season ended (history only).
    pub closing: i64,
    /// Signed sums by `Cat`, in `Cat::ALL` order. Shorter than `Cat::ALL`
    /// in saves from before a category existed.
    #[serde(default)]
    pub totals: Vec<i64>,
    /// (round, category, amount), entries of one round and category added
    /// together. Cleared when the season is filed.
    pub rounds: Vec<(u16, Cat, i64)>,
    /// Home games played, playoffs included.
    pub games: u16,
    pub attendance: u64,
    pub last_attendance: u32,
    pub last_gate: i64,
    /// History only: for every round of the season, what came in, what went
    /// out, and the balance at its end.
    #[serde(default)]
    pub weekly: Vec<[i64; 3]>,
}

impl Ledger {
    pub fn total(&self, c: Cat) -> i64 {
        self.totals.get(c.index()).copied().unwrap_or(0)
    }

    pub(crate) fn add(&mut self, c: Cat, amount: i64) {
        let i = c.index();
        if self.totals.len() <= i {
            self.totals.resize(Cat::ALL.len(), 0);
        }
        self.totals[i] += amount;
    }

    /// Round by round: money in, money out and the balance after it, for
    /// the rounds this ledger has entries for (`rounds` while the season
    /// runs, `weekly` once it is filed).
    pub fn weeks(&self) -> Vec<[i64; 3]> {
        if self.rounds.is_empty() {
            return self.weekly.clone();
        }
        weekly_from(self.start, &self.rounds)
    }

    /// Everything that came in or went out, as one number.
    pub fn net(&self) -> i64 {
        self.totals.iter().sum()
    }

    /// Money earned by the club itself: gate, sponsors, broadcast,
    /// merchandise and prizes.
    pub fn income(&self) -> i64 {
        Cat::ALL
            .iter()
            .filter(|c| c.is_income())
            .map(|&c| self.total(c))
            .sum()
    }

    /// Everything spent on wages, the hall, facilities, fees and interest:
    /// all that went out, as a positive number.
    pub fn spending(&self) -> i64 {
        -Cat::ALL.iter().map(|&c| self.total(c).min(0)).sum::<i64>()
    }

    /// Average crowd, 0 before the first home game.
    pub fn average_attendance(&self) -> u32 {
        (self.attendance / self.games.max(1) as u64) as u32
    }
}

/// Weekly (in, out, balance) from a season's per-round entries.
pub(crate) fn weekly_from(start: i64, rounds: &[(u16, Cat, i64)]) -> Vec<[i64; 3]> {
    let last = rounds
        .iter()
        .map(|r| r.0)
        .max()
        .map_or(0, |r| r as usize + 1);
    let mut out = vec![[0i64; 3]; last];
    for &(round, _, amount) in rounds {
        let w = &mut out[round as usize];
        if amount >= 0 {
            w[0] += amount;
        } else {
            w[1] -= amount;
        }
    }
    let mut balance = start;
    for w in &mut out {
        balance += w[0] - w[1];
        w[2] = balance;
    }
    out
}

/// Arenas, sponsors and money of the whole league.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Economy {
    /// Source of everything random in here; set at generation.
    pub seed: u64,
    /// Indexed by `TeamId`.
    pub arenas: Vec<Arena>,
    pub sponsors: Vec<Sponsor>,
    pub deals: Vec<SponsorDeal>,
    /// Proposals to the user's club.
    pub offers: Vec<SponsorOffer>,
    /// This season, indexed by `TeamId`.
    pub ledgers: Vec<Ledger>,
    /// Finished seasons: `history[season][club]`.
    pub history: Vec<Vec<Ledger>>,
    /// By fixture index: people and gate, 0 before the game is played.
    pub crowds: Vec<(u32, i64)>,
    /// The user's club was told the board froze spending.
    pub warned: bool,
    /// What each club's board, books and facilities are doing; see
    /// `books.rs`. Indexed by `TeamId`.
    #[serde(default)]
    pub books: Vec<crate::books::Books>,
}

const SPONSOR_POOL: usize = 24;

fn nth(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

impl Economy {
    /// The halls, sponsors and first deals of a new league.
    pub(crate) fn generate(seed: u64, world: &World) -> Economy {
        let seed = seed ^ 0xEC0_17A4;
        let mut econ = Economy {
            seed,
            sponsors: pool(seed, SPONSOR_POOL),
            ..Economy::default()
        };
        econ.arenas = halls(seed, world);
        econ.books = econ
            .arenas
            .iter()
            .enumerate()
            .map(|(id, a)| Books::generate(seed, id, a.prestige))
            .collect();
        econ.ledgers = world
            .teams
            .iter()
            .map(|t| Ledger {
                start: t.budget,
                ..Ledger::default()
            })
            .collect();
        econ
    }
}

/// The halls of every club, bigger ones going loosely to better squads.
fn halls(seed: u64, world: &World) -> Vec<Arena> {
    let strength: Vec<f32> = world
        .teams
        .iter()
        .map(|t| {
            let mut ov: Vec<f32> = t
                .roster
                .iter()
                .map(|&p| world.players[p as usize].overall())
                .collect();
            ov.sort_by(|a, b| b.total_cmp(a));
            ov.iter().take(8).sum::<f32>()
        })
        .collect();
    let n = world.teams.len();
    world
        .teams
        .iter()
        .enumerate()
        .map(|(id, t)| {
            let weaker = strength.iter().filter(|&&s| s < strength[id]).count();
            let rank = if n > 1 {
                weaker as f32 / (n - 1) as f32
            } else {
                0.5
            };
            let city = t.name.rsplit_once(' ').map_or(t.name.as_str(), |c| c.0);
            Arena::generate(seed, id, city, rank)
        })
        .collect()
}

fn hash_names(world: &World) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for t in &world.teams {
        for b in t.name.bytes() {
            h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
        }
    }
    h
}

impl World {
    /// Make sure the league has halls, sponsors and ledgers: a world
    /// generated before they existed, or loaded from an old save, gets them
    /// here. Every save is repaired this way when it is opened.
    pub fn ensure_economy(&mut self) {
        let n = self.teams.len();
        if self.econ.arenas.len() != n {
            let seed = if self.econ.seed == 0 {
                hash_names(self) ^ 0xEC0_17A4
            } else {
                self.econ.seed
            };
            self.econ.seed = seed;
            self.econ.arenas = halls(seed, self);
            self.econ.deals.clear();
            for club in 0..n {
                let mut rng = Rng::new(seed ^ 0xDEA1 ^ club as u64);
                if self.econ.sponsors.is_empty() {
                    self.econ.sponsors = pool(seed, SPONSOR_POOL);
                }
                if Some(club) != self.watch {
                    self.sign_ai_deals(club, &mut rng);
                }
            }
        }
        if self.econ.sponsors.is_empty() {
            self.econ.sponsors = pool(self.econ.seed, SPONSOR_POOL);
        }
        if self.econ.books.len() != n {
            let seed = self.econ.seed;
            self.econ.books = (0..n)
                .map(|id| Books::generate(seed, id, self.econ.arenas[id].prestige))
                .collect();
            for club in 0..n {
                self.set_budgets(club);
            }
        }
        if self.econ.ledgers.len() != n {
            self.econ.ledgers = self
                .teams
                .iter()
                .map(|t| Ledger {
                    start: t.budget,
                    ..Ledger::default()
                })
                .collect();
        }
    }

    /// The first deals of a fresh league. Called once by `generate`.
    pub(crate) fn sign_first_deals(&mut self) {
        for club in 0..self.teams.len() {
            let mut rng = Rng::new(self.econ.seed ^ 0xDEA1 ^ club as u64);
            self.sign_ai_deals(club, &mut rng);
        }
        for club in 0..self.teams.len() {
            self.set_budgets(club);
        }
        for club in 0..self.teams.len() {
            let price = self.sensible_price(club);
            self.econ.books[club].price = price;
        }
    }

    pub fn ledger(&self, team: TeamId) -> &Ledger {
        &self.econ.ledgers[team]
    }

    /// True if the board has frozen the club's spending: the budget is
    /// below zero by more than the bank's overdraft limit.
    pub fn spending_frozen(&self, team: TeamId) -> bool {
        self.teams[team].budget < -self.overdraft_limit(team)
    }

    /// The one way a club's budget changes: `amount` (positive in, negative
    /// out) for `cat`, booked in this season's ledger for the current round.
    pub fn transact(&mut self, club: TeamId, amount: i64, cat: Cat) {
        self.teams[club].budget += amount;
        let round = self.round as u16;
        if let Some(l) = self.econ.ledgers.get_mut(club) {
            l.add(cat, amount);
            match l.rounds.last_mut() {
                Some(last) if last.0 == round && last.1 == cat => last.2 += amount,
                _ => l.rounds.push((round, cat, amount)),
            }
        }
    }

    /// Rounds in the regular season.
    fn regular_rounds(&self) -> usize {
        2 * self.teams.len().saturating_sub(1)
    }

    /// Before a round is played: take the gate of every home game and the
    /// round's share of sponsor money, and note what wages the round costs.
    pub(crate) fn econ_before_games(&mut self, played: &[usize]) {
        self.ensure_economy();
        if self.econ.crowds.len() < self.fixtures.len() {
            self.econ.crowds.resize(self.fixtures.len(), (0, 0));
        }
        for &i in played {
            let draw = self.draw_for(i);
            let home = self.fixtures[i].home;
            let price = self.ticket_price(home);
            let att = self.attendance_priced(&draw, home, price);
            self.note_bargain(home, &draw, price);
            let mut gate = att as i64 * price;
            if self.fixtures[i].playoff {
                gate = gate * 3 / 2;
            }
            self.econ.crowds[i] = (att, gate);
            self.transact(home, gate, Cat::Gate);
            // The visitors take a share of the gate, so a club that fills its
            // hall with a name gives some of it to the clubs that bring one,
            // and a full hall does not run away with the league's money.
            let away = self.fixtures[i].away;
            let share = gate / GATE_SHARE;
            self.transact(home, -share, Cat::Gate);
            self.transact(away, share, Cat::Gate);
            let running = self.econ.arenas[home].running_cost();
            self.transact(home, -running, Cat::Upkeep);
            let l = &mut self.econ.ledgers[home];
            l.games += 1;
            l.attendance += att as u64;
            l.last_attendance = att;
            l.last_gate = gate;
        }
        let regular = self.round < self.regular_rounds();
        let rounds = self.regular_rounds().max(1) as i64;
        for club in 0..self.teams.len() {
            if regular {
                let pay: i64 = self
                    .econ
                    .deals
                    .iter()
                    .filter(|d| d.club == club)
                    .map(|d| d.fee / rounds)
                    .sum();
                self.transact(club, pay, Cat::Sponsorship);
            }
        }
        self.book_media_income(regular, played);
        if regular {
            self.book_facility_running();
        }
    }

    /// One line about the crowd at fixture `i` for the match report.
    pub(crate) fn gate_line(&self, i: usize) -> Option<String> {
        let &(att, gate) = self.econ.crowds.get(i)?;
        if att == 0 {
            return None;
        }
        let f = &self.fixtures[i];
        let arena = &self.econ.arenas[f.home];
        let pct = att as u64 * 100 / arena.capacity.max(1) as u64;
        Some(if Some(f.home) == self.watch {
            format!(
                "{} came to {}: {pct}% full, and the gate came to {}.\n\n",
                money(att as i64),
                arena.name,
                money(gate)
            )
        } else {
            format!(
                "{} watched at {}, {pct}% full.\n\n",
                money(att as i64),
                arena.name
            )
        })
    }

    /// After a round: warnings about money, and proposals from sponsors.
    pub(crate) fn econ_after_round(&mut self) {
        self.econ_weekly();
        let Some(me) = self.watch else { return };
        let round = self.round - 1;
        let today = round as u32 * 7 + crate::inbox::MATCH_DAY;
        if !self.spending_frozen(me) {
            self.econ.warned = false;
        }
        let open = self
            .econ
            .offers
            .iter()
            .filter(|o| o.club == me && o.open_on(today))
            .count();
        let mut rng =
            Rng::new(self.econ.seed ^ 0x0FFE ^ ((self.season as u64) << 20) ^ round as u64);
        if round < self.regular_rounds()
            && open < 2
            && !self.free_slots(me).is_empty()
            && rng.chance(0.2)
        {
            self.post_sponsor_offer(today + 4, &mut rng);
        }
    }

    /// The user's club has been handed to a new manager: tell him about
    /// the hall and bring in a first offer.
    pub(crate) fn econ_welcome(&mut self, team: TeamId) {
        self.ensure_economy();
        let a = self.econ.arenas[team].clone();
        let sold = 20 - self.free_slots(team).len();
        let body = format!(
            "We play at {}, which holds {} and is {}. {sold} of our 20 boards have a \
             sponsor, bringing in {} a season. Prize money and gate receipts make up the \
             rest of the income, with the league's broadcast money and the shop; the books are \
             on the Finances page.\n\nA renovation \
             restores the hall, and the seats can be expanded for the next season, but \
             both cost money and the board will not sign them off while the budget is deep \
             in the red. Sponsors write to us now and then. Their offers are in the post, and \
             it is your call.",
            a.name,
            money(a.capacity as i64),
            a.condition_words().to_lowercase(),
            money(self.sponsor_income(team)),
        );
        self.post(4, Kind::Board, "The board", "Our hall", body, vec![]);
        // The manager starts from the hall's own price.
        self.econ.books[team].price = 0;
        let why = self.set_budgets(team);
        self.post_budget_letter(team, &why, 4);
        let mut rng = Rng::new(self.econ.seed ^ 0x0FFE_0001);
        self.post_sponsor_offer(4, &mut rng);
    }

    /// Refit a hall: its condition goes up. The caller has checked the
    /// money.
    pub fn renovate(&mut self, team: TeamId) -> Result<String, String> {
        let cost = self.econ.arenas[team].renovation_cost();
        let a = &mut self.econ.arenas[team];
        a.condition = (a.condition + Arena::RENOVATION_GAIN).min(1.0);
        a.renovated = true;
        self.transact(team, -cost, Cat::Renovation);
        Ok(format!(
            "{} has been refitted for {} and is now {}.",
            self.econ.arenas[team].name,
            money(cost),
            self.econ.arenas[team].condition_words().to_lowercase()
        ))
    }

    /// Start building `EXPANSION_SEATS` more seats, open next season.
    pub fn expand(&mut self, team: TeamId) -> Result<String, String> {
        let cost = self.econ.arenas[team].expansion_cost();
        self.transact(team, -cost, Cat::Expansion);
        self.econ.arenas[team].pending_seats += EXPANSION_SEATS;
        Ok(format!(
            "Work on {} more seats has begun for {}. They open when the season turns.",
            money(EXPANSION_SEATS as i64),
            money(cost)
        ))
    }

    /// Why the user's club may not do work on the hall now, if it may not.
    pub fn works_blocked(&self, team: TeamId, expand: bool) -> Option<String> {
        let a = &self.econ.arenas[team];
        if self.spending_frozen(team) {
            return Some("The board has frozen spending until the budget is above zero.".into());
        }
        if expand {
            if a.capacity + a.pending_seats + EXPANSION_SEATS > MAX_CAPACITY {
                return Some("The site cannot take more seats.".into());
            }
            if a.pending_seats > 0 {
                return Some("An expansion is already under way.".into());
            }
            if a.expansion_cost() > self.teams[team].budget {
                return Some("The budget does not stretch that far.".into());
            }
        } else {
            if a.condition >= 0.95 {
                return Some("The hall is in perfect order.".into());
            }
            if a.renovation_cost() > self.teams[team].budget {
                return Some("The budget does not stretch that far.".into());
            }
        }
        None
    }

    /// The season is decided: pay prize money and bonuses, age the halls,
    /// open new seats, drop expired deals and tell the manager how the year
    /// went. Files the season's ledger in the history and starts a new one.
    /// Called before the table is reset.
    pub(crate) fn econ_close_season(&mut self, table: &[TeamId], champion: TeamId) {
        self.ensure_economy();
        let n = table.len();
        let me = self.watch;
        let semi = |t: TeamId| {
            self.playoffs
                .iter()
                .any(|s| !s.is_final && (s.high == t || s.low == t))
        };
        let finalist = |t: TeamId| {
            self.playoffs
                .iter()
                .any(|s| s.is_final && (s.high == t || s.low == t))
        };
        let titled = |t: TeamId| t == champion && !self.playoffs.is_empty();
        let mut paid: Vec<(i64, i64, f32)> = vec![(0, 0, 0.0); n];
        for (i, &t) in table.iter().enumerate() {
            let place = if n > 1 {
                1.0 - i as f32 / (n - 1) as f32
            } else {
                0.5
            };
            let mut prize = PRIZE_TOP * (n - 1 - i) as i64 / (n.max(2) - 1) as i64;
            let mut run = 0.0;
            if semi(t) {
                prize += PRIZE_SEMI;
                run = 0.04;
            }
            if finalist(t) {
                prize += PRIZE_FINAL;
                run = 0.08;
            }
            if titled(t) {
                prize += PRIZE_TITLE;
                run = 0.15;
            }
            let bonus: i64 = self
                .econ
                .deals
                .iter()
                .filter(|d| d.club == t)
                .map(|d| match d.bonus {
                    Bonus::Playoffs(v) if semi(t) => v,
                    Bonus::Title(v) if titled(t) => v,
                    _ => 0,
                })
                .sum();
            paid[t] = (prize, bonus, 0.85 * place + run);
        }
        for (t, &(prize, bonus, score)) in paid.iter().enumerate() {
            self.transact(t, prize, Cat::Prize);
            self.transact(t, bonus, Cat::Sponsorship);
            let a = &mut self.econ.arenas[t];
            a.prestige = (0.6 * a.prestige + 0.4 * score).clamp(0.05, 1.0);
            a.support = (a.support + (0.3 + 0.55 * score - a.support) * 0.12).clamp(0.1, 0.95);
            a.condition = (a.condition - if a.renovated { 0.045 } else { 0.09 }).max(0.05);
            a.renovated = false;
            a.capacity += a.pending_seats;
            a.pending_seats = 0;
        }
        self.settle_bargains();
        if let Some(me) = me {
            let l = &self.econ.ledgers[me];
            let place = table.iter().position(|&t| t == me).map_or(0, |p| p + 1);
            let mut lines = String::new();
            for c in Cat::ALL {
                if l.total(c) != 0 {
                    lines += &format!("{}: {}\n", c.label(), money(l.total(c)));
                }
            }
            let body = format!(
                "The books for season {}. We finished {}.\n\n\
                 {} home games, {} people a game on average.\n\n{lines}\n\
                 {}\n\nThe budget moved from {} to {}.{}",
                self.season + 1,
                nth(place),
                l.games,
                money(l.average_attendance() as i64),
                self.budget_summary(me),
                money(l.start),
                money(self.teams[me].budget),
                if self.spending_frozen(me) {
                    " The board has frozen spending until the budget is back inside the bank's limit."
                } else {
                    ""
                },
            );
            self.post(
                self.day,
                Kind::Board,
                "The board",
                format!("Season {} finances", self.season + 1),
                body,
                vec![],
            );
        }
        // File the year and open the next set of books.
        let season = self.season;
        let filed: Vec<Ledger> = self
            .econ
            .ledgers
            .iter_mut()
            .zip(&self.teams)
            .map(|(l, t)| {
                let mut done = std::mem::replace(
                    l,
                    Ledger {
                        start: t.budget,
                        ..Ledger::default()
                    },
                );
                done.season = season;
                done.closing = t.budget;
                done.weekly = weekly_from(done.start, &done.rounds);
                done.rounds.clear();
                done
            })
            .collect();
        self.econ.history.push(filed);
        self.econ.deals.retain(|d| d.ends > season);
        for o in &mut self.econ.offers {
            if o.state == crate::inbox::OfferState::Open {
                o.state = crate::inbox::OfferState::Lapsed;
            }
        }
    }

    /// A new season: the other clubs spend on their halls and sell boards,
    /// and the user's club gets proposals.
    pub(crate) fn econ_open_season(&mut self) {
        self.econ.crowds.clear();
        self.econ_summer();
        let me = self.watch;
        for club in 0..self.teams.len() {
            if Some(club) == me {
                continue;
            }
            let last = self.econ.history.last().map(|h| h[club].clone());
            let a = &self.econ.arenas[club];
            let budget = self.teams[club].budget;
            let full = last.as_ref().map_or(0.0, |l| {
                l.average_attendance() as f32 / a.capacity.max(1) as f32
            });
            if a.condition < 0.5 && budget > a.renovation_cost() * 3 {
                let _ = self.renovate(club);
            } else if a.support > 0.6
                && full > 0.85
                && a.capacity + EXPANSION_SEATS <= MAX_CAPACITY
                && budget > a.expansion_cost() * 3
            {
                let _ = self.expand(club);
            }
            let mut rng =
                Rng::new(self.econ.seed ^ 0xA1 ^ ((self.season as u64) << 24) ^ club as u64);
            self.sign_ai_deals(club, &mut rng);
        }
        if let Some(me) = me {
            let mut rng = Rng::new(self.econ.seed ^ 0xB2 ^ ((self.season as u64) << 24));
            for k in 0..2 {
                if !self.free_slots(me).is_empty() {
                    self.post_sponsor_offer(k * 3, &mut rng);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::{Draw, attendance, fill};
    use crate::command::Command;
    use crate::command::tests::tidy;
    use crate::inbox::{OfferState, Stop};

    /// Play without a manager until `seasons` have finished.
    fn run(w: &mut World, seasons: u16) {
        let target = w.season + seasons;
        while w.season < target {
            if w.match_today() {
                w.play_round();
            } else {
                w.advance();
            }
        }
    }

    /// Play as the manager of club 0, answering mail, through `seasons`.
    fn run_managed(w: &mut World, seasons: u16) {
        let target = w.season + seasons;
        let mut n = 0;
        for _ in 0..200_000 {
            if w.season >= target {
                return;
            }
            tidy(w, &mut n);
            match w.apply(Command::Advance).unwrap().stop.unwrap() {
                Stop::MatchDay => {
                    w.apply(Command::PlayRound).unwrap();
                }
                Stop::Mail => {}
                Stop::Draft => {
                    let _ = w.apply(Command::DraftAuto { all: true });
                }
            }
        }
        panic!("the seasons never ended");
    }

    fn base() -> Draw {
        Draw {
            capacity: 6000,
            support: 0.5,
            form: 0.5,
            position: 0.5,
            belief: 0.6,
            opponent: 0.5,
            condition: 0.7,
            playoff: false,
            luck: 0.0,
            fans: 0.5,
            star: 0.0,
            visitor: 0.0,
            night: 0.0,
        }
    }

    #[test]
    fn finances_are_deterministic() {
        let mut a = World::generate(11, 10);
        let mut b = World::generate(11, 10);
        run(&mut a, 2);
        run(&mut b, 2);
        assert!(format!("{:?}", a.econ) == format!("{:?}", b.econ));
        let ba: Vec<i64> = a.teams.iter().map(|t| t.budget).collect();
        let bb: Vec<i64> = b.teams.iter().map(|t| t.budget).collect();
        assert_eq!(ba, bb);
    }

    #[test]
    fn halls_differ_and_are_in_range() {
        let w = World::generate(4, 12);
        let caps: Vec<u32> = (0..12).map(|t| w.arena(t).capacity).collect();
        assert!(
            caps.iter().all(|&c| (2_500..=9_000).contains(&c)),
            "{caps:?}"
        );
        assert!(caps.iter().max() > caps.iter().min());
        let names: std::collections::HashSet<_> = (0..12).map(|t| &w.arena(t).name).collect();
        assert_eq!(names.len(), 12);
        for t in 0..12 {
            let a = w.arena(t);
            assert!((0.0..=1.0).contains(&a.condition));
            assert!(a.ticket_price() > 0);
        }
    }

    #[test]
    fn crowds_follow_results_and_stay_in_the_hall() {
        let mut good = base();
        good.form = 1.0;
        good.position = 1.0;
        let mut bad = base();
        bad.form = 0.0;
        bad.position = 0.0;
        assert!(attendance(&good) > attendance(&base()));
        assert!(attendance(&base()) > attendance(&bad));
        let mut final_night = good;
        final_night.playoff = true;
        assert!(attendance(&final_night) >= attendance(&good));
        let mut rival = base();
        rival.opponent = 1.0;
        assert!(attendance(&rival) > attendance(&base()));
        let mut worn = base();
        worn.condition = 0.0;
        assert!(attendance(&worn) < attendance(&base()));
        for d in [good, bad, final_night, rival, worn] {
            assert!(attendance(&d) <= d.capacity);
            assert!((0.0..=1.0).contains(&fill(&d)));
        }

        let mut w = World::generate(5, 10);
        run(&mut w, 1);
        // Every game that was played has a crowd no bigger than the hall.
        let mut seen = 0;
        for (i, f) in w.fixtures.iter().enumerate() {
            let _ = f;
            let (att, _) = w.econ.crowds.get(i).copied().unwrap_or((0, 0));
            let _ = att;
            seen += 1;
        }
        assert!(seen > 0);
        let h = &w.econ.history[0];
        for (t, l) in h.iter().enumerate() {
            assert!(l.games > 0);
            assert!(l.average_attendance() <= w.arena(t).capacity);
        }
    }

    #[test]
    fn crowd_effect_is_a_pure_value_in_range() {
        let mut w = World::generate(5, 8);
        w.play_round();
        for i in 0..w.fixtures.len().min(12) {
            let c = w.crowd_effect(i);
            assert!((0.0..=1.0).contains(&c.full) && (0.0..=1.0).contains(&c.loud));
            assert_eq!(c, w.crowd_effect(i));
        }
    }

    #[test]
    fn every_budget_change_is_in_the_ledger() {
        let mut w = World::generate(9, 8);
        w.apply(Command::TakeCharge(0)).unwrap();
        let _ = w.apply(Command::Renovate);
        let mut checked = 0;
        for _ in 0..3 {
            run_managed(&mut w, 1);
            checked += 1;
            // The books of the season now running.
            for (t, l) in w.econ.ledgers.iter().enumerate() {
                assert_eq!(l.start + l.net(), w.teams[t].budget, "club {t}");
                let by_round: i64 = l.rounds.iter().map(|r| r.2).sum();
                assert_eq!(by_round, l.net(), "club {t} per-round record");
            }
        }
        assert_eq!(w.econ.history.len(), checked);
        for (s, year) in w.econ.history.iter().enumerate() {
            for (t, l) in year.iter().enumerate() {
                assert_eq!(l.season as usize, s);
                assert_eq!(l.start + l.net(), l.closing, "season {s} club {t}");
                assert!(l.rounds.is_empty());
                // The next season opens with what this one closed on.
                let next_start = w
                    .econ
                    .history
                    .get(s + 1)
                    .map_or(w.econ.ledgers[t].start, |n| n[t].start);
                assert_eq!(l.closing, next_start, "season {s} club {t} carries over");
            }
        }
        let me = &w.econ.history[0][0];
        assert!(me.total(Cat::Gate) > 0 && me.total(Cat::PlayerWages) < 0);
        assert!(me.total(Cat::Renovation) < 0 || w.arena(0).condition >= 0.95);
    }

    #[test]
    fn a_season_of_money_balances_roughly() {
        // Many clubs over several seeds: a middling club about breaks even,
        // the best-supported make a clear surplus, nobody is broke in two
        // years.
        let mut nets: Vec<(f32, i64)> = Vec::new();
        let mut lowest = i64::MAX;
        for seed in 1..=4u64 {
            let mut w = World::generate(seed, 12);
            run(&mut w, 2);
            for t in 0..12 {
                lowest = lowest.min(w.teams[t].budget);
                let l = &w.econ.history[0][t];
                nets.push((w.arena(t).support, l.net()));
            }
        }
        assert!(
            lowest > 0,
            "a club ran out of money inside two seasons: {lowest}"
        );
        nets.sort_by_key(|n| n.1);
        let n = nets.len();
        let median = nets[n / 2].1;
        assert!(median.abs() < 150_000, "the median club nets {median}");
        let best = nets[n - 1].1;
        let worst = nets[0].1;
        assert!(best > 150_000, "best surplus {best}");
        assert!(worst < -100_000 && worst > -600_000, "worst loss {worst}");
    }

    #[test]
    fn sponsor_offers_are_answered_once() {
        let mut w = World::generate(3, 8);
        w.apply(Command::TakeCharge(0)).unwrap();
        assert!(!w.econ.offers.is_empty());
        let before = w.econ.deals.len();
        w.apply(Command::AnswerSponsor {
            offer: 0,
            accept: true,
        })
        .unwrap();
        assert_eq!(w.econ.offers[0].state, OfferState::Accepted);
        assert_eq!(w.econ.deals.len(), before + 1);
        assert_eq!(
            w.apply(Command::AnswerSponsor {
                offer: 0,
                accept: true
            }),
            Err(crate::command::CommandError::NothingToDo)
        );
        assert_eq!(w.econ.deals.len(), before + 1);
        assert_eq!(
            w.apply(Command::AnswerSponsor {
                offer: 99,
                accept: true
            }),
            Err(crate::command::CommandError::OutOfRange("offer"))
        );
        // The boards are held by the new sponsor.
        let slot = w.econ.offers[0].slots[0] as usize;
        let name = &w.econ.sponsors[w.econ.offers[0].sponsor].name;
        assert_eq!(&w.slot_sponsor(0, slot).unwrap().name, name);
    }

    #[test]
    fn works_cost_money_and_a_negative_budget_freezes_spending() {
        let mut w = World::generate(6, 8);
        w.apply(Command::TakeCharge(0)).unwrap();
        w.econ.arenas[0].condition = 0.3;
        let before = w.teams[0].budget;
        let cost = w.arena(0).renovation_cost();
        w.apply(Command::Renovate).unwrap();
        assert_eq!(w.teams[0].budget, before - cost);
        assert!((w.arena(0).condition - 0.7).abs() < 1e-4);
        let seats = w.arena(0).capacity;
        w.apply(Command::Expand).unwrap();
        assert_eq!(w.arena(0).capacity, seats, "new seats wait for next season");
        assert!(w.apply(Command::Expand).is_err());
        run_managed(&mut w, 1);
        assert_eq!(w.arena(0).capacity, seats + 1000);
        w.teams[0].budget = -5;
        assert!(w.apply(Command::Renovate).is_err());
        assert!(w.apply(Command::Expand).is_err());
        assert!(
            w.apply(Command::Bid {
                player: w.teams[1].roster[0],
                fee: 1
            })
            .is_err()
        );
    }
}
