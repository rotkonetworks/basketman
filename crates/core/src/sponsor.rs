//! Boards, sponsors and what they pay.
//!
//! Every arena has the same twenty named slots (see `slot_name`): courtside
//! panels along both sidelines, boards behind each baseline, the centre
//! circle, the padding on the basket supports and the front of the scorer's
//! table. A sponsor takes some of a club's slots for a number of seasons
//! for a fee a season, paid a little each round of the regular season.
//! Slots nobody holds show the league's own board.

use serde::{Deserialize, Serialize};

use crate::inbox::{OfferState, money};
use crate::league::{TeamId, World};
use crate::rng::Rng;

pub const SLOTS: usize = 20;

/// Where on the floor a slot is.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SlotKind {
    /// Panel `n` (0..6) of the sideline on the scorer's table side.
    TableSide(u8),
    /// Panel `n` (0..6) of the opposite sideline.
    FarSide(u8),
    /// Board `n` (0..2) behind the baseline at the low end.
    BaselineA(u8),
    /// Board `n` (0..2) behind the baseline at the high end.
    BaselineB(u8),
    /// The middle of the floor.
    Centre,
    /// Padding on the basket support at the low (0) or high (1) end.
    Padding(u8),
    /// The front of the scorer's table.
    Table,
}

/// The kind of slot number `i`.
pub fn slot_kind(i: usize) -> SlotKind {
    match i {
        0..=5 => SlotKind::TableSide(i as u8),
        6..=11 => SlotKind::FarSide(i as u8 - 6),
        12..=13 => SlotKind::BaselineA(i as u8 - 12),
        14..=15 => SlotKind::BaselineB(i as u8 - 14),
        16 => SlotKind::Centre,
        17 | 18 => SlotKind::Padding(i as u8 - 17),
        _ => SlotKind::Table,
    }
}

pub fn slot_name(i: usize) -> String {
    match slot_kind(i) {
        SlotKind::TableSide(n) => format!("Courtside, table side, panel {}", n + 1),
        SlotKind::FarSide(n) => format!("Courtside, far side, panel {}", n + 1),
        SlotKind::BaselineA(n) => format!("Baseline board, west end, {}", n + 1),
        SlotKind::BaselineB(n) => format!("Baseline board, east end, {}", n + 1),
        SlotKind::Centre => "Centre circle".to_string(),
        SlotKind::Padding(0) => "Basket padding, west".to_string(),
        SlotKind::Padding(_) => "Basket padding, east".to_string(),
        SlotKind::Table => "Scorer's table".to_string(),
    }
}

/// How much a slot is worth relative to the others: how often the camera
/// sees it.
pub fn slot_weight(i: usize) -> f32 {
    match slot_kind(i) {
        SlotKind::Centre => 5.0,
        SlotKind::Table => 3.0,
        SlotKind::BaselineA(_) | SlotKind::BaselineB(_) => 3.0,
        SlotKind::Padding(_) => 2.5,
        SlotKind::TableSide(_) | SlotKind::FarSide(_) => 1.5,
    }
}

/// Fee per season for a slot for a typical club, before the club's own
/// standing and the sponsor's size count.
const UNIT: f32 = 4_500.0;

/// A company that buys boards.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sponsor {
    pub name: String,
    /// 1 small, 2 regional, 3 national.
    pub tier: u8,
    /// Board background.
    pub bg: [u8; 3],
    /// Lettering.
    pub fg: [u8; 3],
}

impl Sponsor {
    fn tier_factor(&self) -> f32 {
        match self.tier {
            1 => 0.8,
            2 => 1.0,
            _ => 1.3,
        }
    }
}

/// Extra money a sponsor pays when the club does well.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bonus {
    None,
    /// For making the playoffs.
    Playoffs(i64),
    /// For winning the title.
    Title(i64),
}

impl Bonus {
    pub fn describe(self) -> String {
        match self {
            Bonus::None => "no bonus".to_string(),
            Bonus::Playoffs(v) => format!("a bonus of {} if we reach the playoffs", money(v)),
            Bonus::Title(v) => format!("a bonus of {} if we win the title", money(v)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SponsorDeal {
    pub club: TeamId,
    /// Index into `Economy::sponsors`.
    pub sponsor: usize,
    pub slots: Vec<u8>,
    /// Last season it runs, counting from 0.
    pub ends: u16,
    /// Per season.
    pub fee: i64,
    pub bonus: Bonus,
}

/// A sponsor's proposal to the user's club.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SponsorOffer {
    pub club: TeamId,
    pub sponsor: usize,
    pub slots: Vec<u8>,
    pub years: u8,
    pub fee: i64,
    pub bonus: Bonus,
    /// Last day it can be accepted.
    pub expires: u32,
    pub state: OfferState,
}

impl SponsorOffer {
    pub fn open_on(&self, day: u32) -> bool {
        self.state == OfferState::Open && self.expires >= day
    }
}

const PREFIX: [&str; 28] = [
    "Valko", "Norvik", "Brenta", "Lumio", "Kestra", "Torvan", "Aldane", "Mirova", "Sundra",
    "Helkon", "Orvela", "Pyrrel", "Zandor", "Nuvio", "Quillo", "Fenora", "Dravik", "Elmara",
    "Tavion", "Ostrel", "Varkko", "Ilmora", "Beskin", "Corvane", "Rylko", "Jantor", "Wexley",
    "Umbra",
];
const SUFFIX: [&str; 20] = [
    "Dairy",
    "Freight",
    "Energy",
    "Telecom",
    "Bank",
    "Foods",
    "Motors",
    "Brewery",
    "Insurance",
    "Logistics",
    "Mobile",
    "Steel",
    "Pharma",
    "Air",
    "Homes",
    "Games",
    "Fibre",
    "Timber",
    "Outdoor",
    "Coffee",
];
/// Brand colours: background and lettering.
const BRANDS: [([u8; 3], [u8; 3]); 14] = [
    ([226, 40, 40], [255, 255, 255]),
    ([16, 40, 96], [255, 214, 64]),
    ([250, 214, 40], [24, 24, 28]),
    ([20, 120, 70], [255, 255, 255]),
    ([255, 255, 255], [200, 30, 60]),
    ([30, 30, 36], [250, 160, 30]),
    ([0, 150, 190], [255, 255, 255]),
    ([120, 40, 150], [255, 255, 255]),
    ([245, 120, 30], [255, 255, 255]),
    ([230, 230, 220], [20, 70, 140]),
    ([60, 180, 120], [20, 30, 40]),
    ([200, 40, 120], [255, 255, 255]),
    ([30, 60, 140], [255, 255, 255]),
    ([250, 90, 70], [30, 30, 36]),
];

/// The fictional companies of the league: `count` of them, from a seed.
pub(crate) fn pool(seed: u64, count: usize) -> Vec<Sponsor> {
    let mut rng = Rng::new(seed ^ 0x5B0_9505);
    let mut out: Vec<Sponsor> = Vec::new();
    while out.len() < count {
        let name = format!(
            "{} {}",
            PREFIX[rng.range(0, PREFIX.len() as u32) as usize],
            SUFFIX[rng.range(0, SUFFIX.len() as u32) as usize]
        );
        let (bg, fg) = BRANDS[rng.range(0, BRANDS.len() as u32) as usize];
        let tier = [1, 1, 2, 2, 2, 3][rng.range(0, 6) as usize];
        if out.iter().any(|s| s.name == name) {
            continue;
        }
        out.push(Sponsor { name, tier, bg, fg });
    }
    out
}

impl World {
    /// The deal holding slot `slot` of `club`'s arena, if any.
    pub fn slot_deal(&self, club: TeamId, slot: usize) -> Option<&SponsorDeal> {
        self.econ
            .deals
            .iter()
            .find(|d| d.club == club && d.slots.contains(&(slot as u8)))
    }

    /// The sponsor on slot `slot` of `club`'s arena, if sold.
    pub fn slot_sponsor(&self, club: TeamId, slot: usize) -> Option<&Sponsor> {
        self.slot_deal(club, slot)
            .map(|d| &self.econ.sponsors[d.sponsor])
    }

    pub fn free_slots(&self, club: TeamId) -> Vec<u8> {
        (0..SLOTS as u8)
            .filter(|&s| self.slot_deal(club, s as usize).is_none())
            .collect()
    }

    /// What `sponsor` pays a season for `slots` of `club`'s arena. It
    /// follows how many people see the boards, how well the club did last
    /// year and how big the company is.
    pub fn slot_fee(&self, club: TeamId, slots: &[u8], sponsor: usize) -> i64 {
        let arena = &self.econ.arenas[club];
        let weight: f32 = slots.iter().map(|&s| slot_weight(s as usize)).sum();
        let crowd = 0.6 + 0.4 * self.expected_attendance(club) as f32 / 4_000.0;
        let standing = 0.8 + 0.5 * arena.prestige;
        let tier = self.econ.sponsors[sponsor].tier_factor();
        let fee = weight * UNIT * crowd * standing * tier * self.commercial_pull(club);
        (fee as i64 + 50) / 100 * 100
    }

    /// What the fan base and the names on the roster add to what sponsors
    /// pay: up to a fifth more for a big following, up to a seventh more for
    /// stars. 1.0 for an ordinary club.
    pub fn commercial_pull(&self, club: TeamId) -> f32 {
        (0.9 + 0.2 * self.fan_index(club)) * (1.0 + 0.15 * self.star_pull(club, club))
    }

    /// Bonus terms a sponsor might put on a deal, from a die roll.
    fn bonus_for(&self, club: TeamId, fee: i64, roll: f32) -> Bonus {
        let prestige = self.econ.arenas[club].prestige;
        if roll < 0.4 {
            Bonus::None
        } else if roll < 0.75 {
            Bonus::Playoffs(fee / 5 / 100 * 100)
        } else if prestige > 0.5 {
            Bonus::Title(fee / 2 / 100 * 100)
        } else {
            Bonus::Playoffs(fee / 4 / 100 * 100)
        }
    }

    /// A proposal for `club` from a random sponsor, if a slot is free.
    pub(crate) fn make_proposal(
        &self,
        club: TeamId,
        rng: &mut Rng,
    ) -> Option<(usize, Vec<u8>, u8, i64, Bonus)> {
        let mut free = self.free_slots(club);
        if free.is_empty() {
            return None;
        }
        let mut sponsor = rng.range(0, self.econ.sponsors.len() as u32) as usize;
        // A company that already has boards here is not asked again, unless
        // everybody has.
        for _ in 0..24 {
            if !self
                .econ
                .deals
                .iter()
                .any(|d| d.club == club && d.sponsor == sponsor)
            {
                break;
            }
            sponsor = rng.range(0, self.econ.sponsors.len() as u32) as usize;
        }
        let want = (1 + rng.range(0, 4) as usize).min(free.len());
        let mut slots = Vec::new();
        for _ in 0..want {
            let k = rng.range(0, free.len() as u32) as usize;
            slots.push(free.remove(k));
        }
        slots.sort_unstable();
        let years = 1 + rng.range(0, 3) as u8;
        let fee = self.slot_fee(club, &slots, sponsor);
        let fee = (fee as f32 * (0.92 + 0.16 * rng.f32())) as i64 / 100 * 100;
        let bonus = self.bonus_for(club, fee, rng.f32());
        Some((sponsor, slots, years, fee, bonus))
    }

    pub(crate) fn sign_deal(
        &mut self,
        club: TeamId,
        sponsor: usize,
        slots: Vec<u8>,
        years: u8,
        fee: i64,
        bonus: Bonus,
    ) {
        self.econ.deals.push(SponsorDeal {
            club,
            sponsor,
            slots,
            ends: self.season + years.max(1) as u16 - 1,
            fee,
            bonus,
        });
    }

    /// Clubs the user does not run sign sponsors until a share of their
    /// boards is sold.
    pub(crate) fn sign_ai_deals(&mut self, club: TeamId, rng: &mut Rng) {
        let target = (0.5 + 0.4 * self.econ.arenas[club].prestige).clamp(0.4, 0.9);
        for _ in 0..12 {
            let sold = SLOTS - self.free_slots(club).len();
            if sold as f32 >= target * SLOTS as f32 {
                break;
            }
            if let Some((sponsor, slots, years, fee, bonus)) = self.make_proposal(club, rng) {
                self.sign_deal(club, sponsor, slots, years, fee, bonus);
            }
        }
    }

    /// Yearly sponsor money, as contracted, for `club`.
    pub fn sponsor_income(&self, club: TeamId) -> i64 {
        self.econ
            .deals
            .iter()
            .filter(|d| d.club == club)
            .map(|d| d.fee)
            .sum()
    }

    /// Put a proposal for the user's club in the post.
    pub(crate) fn post_sponsor_offer(&mut self, day: u32, rng: &mut Rng) {
        let Some(me) = self.watch else { return };
        let Some((sponsor, slots, years, fee, bonus)) = self.make_proposal(me, rng) else {
            return;
        };
        let idx = self.econ.offers.len();
        let name = self.econ.sponsors[sponsor].name.clone();
        let list: Vec<String> = slots.iter().map(|&s| slot_name(s as usize)).collect();
        let body = format!(
            "{name} would like to put its name on the following boards in {}: {}.\n\n\
             They offer {} a season for {} season{}, with {}. If the club does not \
             answer in a week the offer lapses. Money arrives a little each round of the \
             regular season.",
            self.econ.arenas[me].name,
            list.join("; "),
            money(fee),
            years,
            if years == 1 { "" } else { "s" },
            bonus.describe(),
        );
        self.econ.offers.push(SponsorOffer {
            club: me,
            sponsor,
            slots,
            years,
            fee,
            bonus,
            expires: day + 7,
            state: OfferState::Open,
        });
        let subject = format!("Sponsor offer from {name}");
        self.post(day, crate::inbox::Kind::Board, &name, subject, body, vec![])
            .sponsor = Some(idx);
    }

    /// Accept or turn down sponsor offer `offer`. An accepted offer whose
    /// boards were sold meanwhile lapses.
    pub fn answer_sponsor(&mut self, offer: usize, accept: bool) {
        let Some(me) = self.watch else { return };
        let Some(o) = self.econ.offers.get(offer).cloned() else {
            return;
        };
        if !o.open_on(self.day) {
            if o.state == OfferState::Open {
                self.econ.offers[offer].state = OfferState::Lapsed;
            }
            return;
        }
        if !accept {
            self.econ.offers[offer].state = OfferState::Rejected;
            return;
        }
        if o.slots
            .iter()
            .any(|&s| self.slot_deal(me, s as usize).is_some())
        {
            self.econ.offers[offer].state = OfferState::Lapsed;
            return;
        }
        self.econ.offers[offer].state = OfferState::Accepted;
        self.sign_deal(me, o.sponsor, o.slots.clone(), o.years, o.fee, o.bonus);
        let name = self.econ.sponsors[o.sponsor].name.clone();
        let body = format!(
            "{name} is on the boards. {} a season until the end of season {}.",
            money(o.fee),
            self.season + o.years.max(1) as u16,
        );
        self.post(
            self.day,
            crate::inbox::Kind::Board,
            &name,
            format!("{name} signs"),
            body,
            vec![],
        )
        .read = true;
    }
}
