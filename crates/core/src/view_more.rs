//! More of the manager's view: the money (price, budgets, debt, tax,
//! facilities, projection, shop), the fan base, appearance requests, and the
//! league's names and hall of fame. Same rule as `view.rs`: what the human
//! sees on the Finances, Arena, Fans and League screens, no more. All of it
//! is about the managed club or about public fame; nothing here shows a
//! hidden trait.

use serde::Serialize;

use crate::books::FacilityKind;
use crate::facility::{MAX_LEVEL, build_cost, effect_words};
use crate::fame::fame_words;
use crate::league::{TeamId, World};
use crate::player::PlayerId;

#[derive(Clone, Debug, Serialize)]
pub struct PricePointView {
    pub price: i64,
    pub attendance: u32,
    pub gate: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct PricePreviewView {
    pub fixture: usize,
    pub opponent: TeamId,
    pub opponent_name: String,
    pub playoff: bool,
    /// What the crowd would happily pay for this game.
    pub crowd_would_pay: i64,
    /// One step lower, the price in force, one step higher.
    pub points: Vec<PricePointView>,
    /// The price that would take most at the gate for this game.
    pub best_price: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct PriceView {
    pub in_force: i64,
    /// The hall's own price, which `SetTicketPrice` moves away from.
    pub reference: i64,
    pub min: i64,
    pub max: i64,
    /// None when there is no home game left.
    pub preview: Option<PricePreviewView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BudgetView {
    pub wage_budget: i64,
    pub wage_committed: i64,
    pub wage_left: i64,
    pub wage_board_set: i64,
    pub transfer_budget: i64,
    pub transfer_left: i64,
    pub transfer_board_set: i64,
    /// The most `MoveBudget` can move each way now.
    pub move_to_wages_max: i64,
    pub move_to_transfers_max: i64,
    pub board_expects_place: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct DebtView {
    /// "Sound", "Tight", "Overdrawn" or "Over its limit".
    pub state: &'static str,
    pub in_overdraft: bool,
    pub overdraft_limit: i64,
    pub spending_frozen: bool,
    pub interest_next_round: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaxPosition {
    pub payroll: i64,
    pub cap: i64,
    pub excess: i64,
    pub annual_tax: i64,
    pub per_checkpoint: i64,
    pub checkpoints_left: i64,
    pub still_to_pay: i64,
    pub marginal_percent: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct FacilityView {
    pub kind: &'static str,
    /// For `UpgradeFacility`: "Training", "Medical" or "Youth".
    pub command_name: &'static str,
    pub level: u8,
    pub max_level: u8,
    pub effect: String,
    pub upgrade_cost: Option<i64>,
    /// Why it cannot be built up now; None when it can.
    pub blocked: Option<String>,
    /// Being built: the level it will be and the round it opens.
    pub building: Option<(u8, usize)>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectionView {
    pub rounds_left: i64,
    pub home_games_left: usize,
    pub rows: Vec<(&'static str, i64)>,
    pub budget_now: i64,
    pub budget_at_season_end: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SellerView {
    pub player: PlayerId,
    pub name: String,
    pub fame: f32,
    pub per_round: i64,
    pub this_season: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct FansView {
    /// Supporters, in people.
    pub base: u32,
    /// How big that is next to the hall, 0..=100; 50 is an ordinary club.
    pub size_vs_hall_pct: u8,
    /// This season's change so far, in percent.
    pub trend_pct: f32,
    /// What moved it this season, in people.
    pub moved: Vec<(&'static str, i64)>,
    /// And last season.
    pub moved_last_season: Vec<(&'static str, i64)>,
    /// The supporters at the end of each finished season.
    pub past: Vec<u32>,
    /// 0..=100, how much the town cares (the arena's support).
    pub support_pct: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct FinanceView {
    pub price: PriceView,
    pub budgets: BudgetView,
    pub debt: DebtView,
    pub tax: TaxPosition,
    pub facilities: Vec<FacilityView>,
    pub projection: ProjectionView,
    /// What the shop takes in a round.
    pub merchandise_per_round: i64,
    pub top_sellers: Vec<SellerView>,
    pub fans: FansView,
}

#[derive(Clone, Debug, Serialize)]
pub struct AppearanceView {
    pub index: usize,
    pub sponsor: String,
    pub player: PlayerId,
    pub player_name: String,
    pub event_day: u32,
    pub event_date: String,
    pub expires_day: u32,
    pub fee: i64,
    pub state: &'static str,
    pub open: bool,
    pub held: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct NameView {
    pub player: PlayerId,
    pub name: String,
    pub club: Option<TeamId>,
    pub age: u8,
    pub fame: f32,
    pub fame_words: &'static str,
    /// He has said this season is his last.
    pub farewell: bool,
    /// His signature moves the manager knows of; none if unrevealed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signatures: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HallView {
    pub player: PlayerId,
    pub name: String,
    /// The season he retired after, counting from 1.
    pub retired_after_season: u16,
    pub score: f32,
    pub mvps: u8,
    pub titles: u8,
    pub peak_fame: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct NamesView {
    /// The biggest names still playing.
    pub biggest: Vec<NameView>,
    pub hall_of_fame: Vec<HallView>,
}

fn state(s: crate::inbox::OfferState) -> &'static str {
    use crate::inbox::OfferState::*;
    match s {
        Open => "open",
        Accepted => "accepted",
        Rejected => "rejected",
        Lapsed => "lapsed",
    }
}

impl World {
    pub fn view_appearance(&self, i: usize) -> Option<AppearanceView> {
        let a = self.fame.asks.get(i)?;
        Some(AppearanceView {
            index: i,
            sponsor: self.econ.sponsors[a.sponsor].name.clone(),
            player: a.player,
            player_name: self.players[a.player as usize].name.clone(),
            event_day: a.event_day,
            event_date: crate::inbox::date(a.event_day),
            expires_day: a.expires,
            fee: a.fee,
            state: state(a.state),
            open: self.appearance_open(i),
            held: a.held,
        })
    }

    /// The open appearance requests.
    pub fn view_appearances(&self, club: TeamId) -> Option<Vec<AppearanceView>> {
        if self.watch != Some(club) {
            return None;
        }
        Some(
            (0..self.fame.asks.len())
                .filter(|&i| self.appearance_open(i))
                .filter_map(|i| self.view_appearance(i))
                .collect(),
        )
    }

    pub fn view_names(&self, n: usize) -> NamesView {
        NamesView {
            biggest: self
                .biggest_names(n)
                .into_iter()
                .map(|(p, f)| {
                    let pl = &self.players[p as usize];
                    NameView {
                        player: p,
                        name: pl.name.clone(),
                        club: self.team_of(p),
                        age: pl.age,
                        fame: (f * 10.0).round() / 10.0,
                        fame_words: fame_words(f),
                        farewell: pl.renown.farewell,
                        signatures: self
                            .known_signatures(p)
                            .into_iter()
                            .map(|s| s.name())
                            .collect(),
                    }
                })
                .collect(),
            hall_of_fame: self
                .hall_of_fame()
                .into_iter()
                .map(|h| {
                    let r = &self.players[h.player as usize].renown;
                    HallView {
                        player: h.player,
                        name: self.players[h.player as usize].name.clone(),
                        retired_after_season: h.season + 1,
                        score: (h.score * 10.0).round() / 10.0,
                        mvps: r.mvps,
                        titles: r.titles,
                        peak_fame: (r.peak * 10.0).round() / 10.0,
                    }
                })
                .collect(),
        }
    }

    fn fans_view(&self, club: TeamId) -> FansView {
        let moved = |m: [f32; crate::fans::CAUSES.len()]| {
            crate::fans::CAUSES
                .iter()
                .zip(m)
                .map(|(&c, v)| (c, v.round() as i64))
                .collect::<Vec<_>>()
        };
        FansView {
            base: self.fan_base(club),
            size_vs_hall_pct: (self.fan_index(club) * 100.0).round() as u8,
            trend_pct: (self.fan_trend(club) * 1000.0).round() / 10.0,
            moved: moved(self.fans_moved(club)),
            moved_last_season: moved(self.fame.last_moved.get(club).copied().unwrap_or_default()),
            past: self
                .fame
                .past
                .iter()
                .map(|s| s.get(club).copied().unwrap_or(0.0) as u32)
                .collect(),
            support_pct: (self.arena(club).support * 100.0).round() as u8,
        }
    }

    /// The Finances screen as data, for the managed club.
    pub fn view_finance(&self, club: TeamId) -> Option<FinanceView> {
        if self.watch != Some(club) {
            return None;
        }
        let b = self.books(club);
        let (min, max) = self.price_range(club);
        let preview = self.price_preview(club).map(|p| PricePreviewView {
            fixture: p.fixture,
            opponent: p.opponent,
            opponent_name: self.teams[p.opponent].name.clone(),
            playoff: p.playoff,
            crowd_would_pay: p.willing.round() as i64,
            points: p
                .points
                .iter()
                .map(|x| PricePointView {
                    price: x.price,
                    attendance: x.attendance,
                    gate: x.gate,
                })
                .collect(),
            best_price: p.best,
        });
        let tax = self.tax_view(club);
        let proj = self.projection(club);
        let facilities = FacilityKind::ALL
            .into_iter()
            .map(|k| {
                let level = b.facilities.level(k);
                let blocked = self.works_refused(club, k);
                FacilityView {
                    kind: k.label(),
                    command_name: match k {
                        FacilityKind::Training => "Training",
                        FacilityKind::Medical => "Medical",
                        FacilityKind::Youth => "Youth",
                    },
                    level,
                    max_level: MAX_LEVEL,
                    effect: effect_words(k, level),
                    upgrade_cost: (level < MAX_LEVEL).then(|| build_cost(level + 1)),
                    blocked,
                    building: b
                        .works
                        .filter(|w| w.kind == k)
                        .map(|w| (w.to, w.round as usize)),
                }
            })
            .collect();
        Some(FinanceView {
            price: PriceView {
                in_force: self.ticket_price(club),
                reference: self.fair_price(club),
                min,
                max,
                preview,
            },
            budgets: BudgetView {
                wage_budget: b.wage_budget,
                wage_committed: self.wage_committed(club),
                wage_left: self.wage_left(club),
                wage_board_set: b.wage_board,
                transfer_budget: b.transfer_budget,
                transfer_left: self.transfer_left(club),
                transfer_board_set: b.transfer_board,
                move_to_wages_max: self.budget_move_limit(club, true),
                move_to_transfers_max: self.budget_move_limit(club, false),
                board_expects_place: self.desk.expectation,
            },
            debt: DebtView {
                state: self.money_state(club).label(),
                in_overdraft: self.in_overdraft(club),
                overdraft_limit: self.overdraft_limit(club),
                spending_frozen: self.spending_frozen(club),
                interest_next_round: self.interest_due(club),
            },
            tax: TaxPosition {
                payroll: tax.payroll,
                cap: tax.cap,
                excess: tax.excess,
                annual_tax: tax.annual,
                per_checkpoint: tax.per_checkpoint,
                checkpoints_left: tax.checkpoints_left,
                still_to_pay: tax.to_pay(),
                marginal_percent: tax.marginal,
            },
            facilities,
            projection: ProjectionView {
                rounds_left: proj.rounds_left,
                home_games_left: proj.home_games_left,
                rows: proj.rows(),
                budget_now: proj.now,
                budget_at_season_end: proj.closing,
            },
            merchandise_per_round: self.merch_week(club),
            top_sellers: self
                .top_sellers(club, 5)
                .into_iter()
                .map(|(p, per)| {
                    let r = &self.players[p as usize].renown;
                    SellerView {
                        player: p,
                        name: self.players[p as usize].name.clone(),
                        fame: (r.fame * 10.0).round() / 10.0,
                        per_round: per,
                        this_season: r.shirts,
                    }
                })
                .collect(),
            fans: self.fans_view(club),
        })
    }
}
