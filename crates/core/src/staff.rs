//! The people upstairs. Every club has a general manager and a scouting
//! department, and what they are like decides how the club does business:
//! what it thinks players and picks are worth, how hard it bargains, whom
//! it drafts, how often it picks up the phone.

use serde::{Deserialize, Serialize};

use crate::inbox::money;
use crate::league::{TeamId, World};
use crate::player::PlayerId;
use crate::rng::Rng;
use crate::scouting::skew;
use crate::tactics::{DefenseScheme, OffenseSet};

/// Ratings are 1..=20 like a player's.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manager {
    pub name: String,
    pub age: u8,
    /// How close his idea of a player's worth is to the truth. A poor judge
    /// overrates some and underrates others, his own players included.
    pub judgement: u8,
    /// How much of a deal he insists on winning.
    pub negotiation: u8,
    /// High: builds for later, prizes young players and picks. Low: wants
    /// to win now and pays for proven veterans.
    pub patience: u8,
    /// How often he goes looking for a deal.
    pub activity: u8,
}

/// The jobs on a club's staff. One person each.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Job {
    /// Runs the team. The user's club has none: that is the user.
    HeadCoach,
    Assistant,
    Skills,
    Fitness,
    Scout,
    /// Handles the public: media training and the damage when a player
    /// says the wrong thing.
    Press,
}

impl Job {
    pub const ALL: [Job; 6] = [
        Job::HeadCoach,
        Job::Assistant,
        Job::Skills,
        Job::Fitness,
        Job::Scout,
        Job::Press,
    ];
    /// The jobs a manager hires for.
    pub const HIRED: [Job; 5] = [
        Job::Assistant,
        Job::Skills,
        Job::Fitness,
        Job::Scout,
        Job::Press,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Job::HeadCoach => "Head coach",
            Job::Assistant => "Assistant coach",
            Job::Skills => "Skills coach",
            Job::Fitness => "Fitness coach",
            Job::Scout => "Chief scout",
            Job::Press => "Press officer",
        }
    }

    /// The rating the job runs on.
    pub fn skill(self) -> &'static str {
        match self {
            Job::HeadCoach | Job::Assistant => "Tactics",
            Job::Skills => "Teaching",
            Job::Fitness => "Fitness",
            Job::Scout => "Judgement",
            Job::Press => "Media",
        }
    }
}

fn middling() -> u8 {
    8
}

/// A coach or scout. Ratings are 1..=20; which one counts depends on the
/// job he is given.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Staffer {
    pub name: String,
    pub age: u8,
    /// Drilling a system into a squad.
    pub tactics: u8,
    /// Improving players in training.
    pub teaching: u8,
    /// Keeping players fresh and slowing the decline of old legs.
    pub fitness: u8,
    /// Reading players he watches.
    pub judgement: u8,
    /// Handling the public: media training and heading off gaffes.
    #[serde(default = "middling")]
    pub media: u8,
    /// What he likes to run, when the team is his.
    pub offense: OffenseSet,
    pub defense: DefenseScheme,
    /// Per year.
    pub wage: i64,
}

impl Staffer {
    pub fn rating(&self, job: Job) -> u8 {
        match job {
            Job::HeadCoach | Job::Assistant => self.tactics,
            Job::Skills => self.teaching,
            Job::Fitness => self.fitness,
            Job::Scout => self.judgement,
            Job::Press => self.media,
        }
    }

    /// `strong` is the job he is made for: that rating gets the better of
    /// the dice, the others what is left.
    fn generate(rng: &mut Rng, strong: Job, first: &[&str], last: &[&str]) -> Staffer {
        let mut roll = |good: bool| {
            let two = (rng.range(0, 20) + rng.range(0, 20)) / 2;
            (1 + if good { two + 3 } else { two * 3 / 4 }).min(20) as u8
        };
        let tactics = roll(matches!(strong, Job::HeadCoach | Job::Assistant));
        let teaching = roll(strong == Job::Skills);
        let fitness = roll(strong == Job::Fitness);
        let judgement = roll(strong == Job::Scout);
        let offense = [
            OffenseSet::Motion,
            OffenseSet::PickAndRoll,
            OffenseSet::PostUp,
            OffenseSet::Isolation,
            OffenseSet::FiveOut,
        ][rng.weighted(&[0.35, 0.25, 0.12, 0.10, 0.18])];
        let defense = [
            DefenseScheme::ManToMan,
            DefenseScheme::SwitchEverything,
            DefenseScheme::Zone23,
            DefenseScheme::Zone32,
            DefenseScheme::PackLine,
        ][rng.weighted(&[0.50, 0.15, 0.12, 0.08, 0.15])];
        let mut s = Staffer {
            name: format!(
                "{} {}",
                first[rng.range(0, first.len() as u32) as usize],
                last[rng.range(0, last.len() as u32) as usize]
            ),
            age: 32 + rng.range(0, 32) as u8,
            tactics,
            teaching,
            fitness,
            judgement,
            media: 0,
            offense,
            defense,
            wage: 0,
        };
        // Drawn from the staffer himself so that the other ratings come out
        // as they did before this one existed.
        let mut own = Rng::new(
            s.name.bytes().fold(0x9E37_79B9u64, |h, b| {
                h.wrapping_mul(31).wrapping_add(b as u64)
            }) ^ ((s.age as u64) << 40)
                ^ ((s.tactics as u64) << 8)
                ^ s.judgement as u64,
        );
        let two = (own.range(0, 20) + own.range(0, 20)) / 2;
        s.media = (1 + if strong == Job::Press {
            two + 3
        } else {
            two * 3 / 4
        })
        .min(20) as u8;
        s.wage = s.asking();
        s
    }

    /// What he expects to be paid: his best rating sets it.
    fn asking(&self) -> i64 {
        let best = self
            .tactics
            .max(self.teaching)
            .max(self.fitness)
            .max(self.judgement)
            .max(self.media) as i64;
        (6000 + best * best * 140) / 1000 * 1000
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrontOffice {
    pub gm: Manager,
    /// Indexed by `Job as usize`; a job can be vacant.
    pub staff: [Option<Staffer>; 5],
    /// The press officer. Apart from `staff` so that saves from before
    /// the job existed still load.
    #[serde(default)]
    pub press: Option<Staffer>,
}

/// Rating a vacant job is done at: somebody covers, badly.
const VACANT: u8 = 4;

impl FrontOffice {
    pub fn staffer(&self, job: Job) -> Option<&Staffer> {
        match job {
            Job::Press => self.press.as_ref(),
            _ => self.staff[job as usize].as_ref(),
        }
    }

    /// The place `job` is kept in, to put someone in or take him out.
    pub fn slot_mut(&mut self, job: Job) -> &mut Option<Staffer> {
        match job {
            Job::Press => &mut self.press,
            _ => &mut self.staff[job as usize],
        }
    }

    pub fn rating(&self, job: Job) -> u8 {
        self.staffer(job).map_or(VACANT, |s| s.rating(job))
    }

    /// Quality of the scouting department: how well the club reads a draft
    /// class, and for the user how fast shortlisted players become known.
    pub fn scouting(&self) -> u8 {
        self.rating(Job::Scout)
    }

    /// Multiplier on what players gain from a week's training.
    pub fn teaching(&self) -> f32 {
        0.75 + self.rating(Job::Skills) as f32 * 0.025
    }

    /// Multiplier on what training takes out of the legs, and on how fast
    /// veterans lose theirs. Below one is good.
    pub fn wear(&self) -> f32 {
        1.3 - self.rating(Job::Fitness) as f32 * 0.03
    }

    /// Multiplier on how fast the squad learns a tactic. The head coach
    /// and his assistant both drill it; a user's club has only the latter.
    pub fn drilling(&self) -> f32 {
        let lead = match self.staffer(Job::HeadCoach) {
            Some(head) => (head.tactics as f32 * 2.0 + self.rating(Job::Assistant) as f32) / 3.0,
            None => self.rating(Job::Assistant) as f32,
        };
        0.7 + lead * 0.03
    }

    /// Yearly wage bill of the staff.
    pub fn wages(&self) -> i64 {
        self.staff
            .iter()
            .flatten()
            .chain(&self.press)
            .map(|s| s.wage)
            .sum()
    }
}

impl Manager {
    /// -1..=1 from `patience`.
    pub(crate) fn lean(&self) -> f32 {
        (self.patience.min(20) as f32 - 10.0) / 10.0
    }

    /// Percent a deal has to favour him before he says yes. Can be below
    /// zero: a soft touch gives a little away.
    pub fn margin(&self) -> i64 {
        5 + (self.negotiation as i64 - 10) * 3 / 2
    }

    /// The man in a few phrases, most telling first.
    pub fn traits(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        match self.judgement {
            16.. => out.push("a sharp judge of talent"),
            ..=7 => out.push("often misjudges players"),
            _ => {}
        }
        match self.negotiation {
            15.. => out.push("drives a hard bargain"),
            ..=7 => out.push("easy to deal with"),
            _ => {}
        }
        match self.patience {
            14.. => out.push("building for the future, loves youth and picks"),
            ..=7 => out.push("wants to win now, pays for veterans"),
            _ => {}
        }
        match self.activity {
            15.. => out.push("always working the phones"),
            ..=6 => out.push("rarely makes a move"),
            _ => {}
        }
        if out.is_empty() {
            out.push("a steady hand, nothing extreme");
        }
        out
    }
}

impl FrontOffice {
    /// Drawn from its own stream so a world's players and schedule do not
    /// depend on who sits upstairs.
    pub(crate) fn generate(seed: u64, team: TeamId, first: &[&str], last: &[&str]) -> Self {
        let rng = &mut Rng::new(seed ^ 0x0F1C_E000 ^ ((team as u64) << 32));
        // Two rolls averaged: most are middling, extremes are rare.
        let mut roll = || 1 + ((rng.range(0, 20) + rng.range(0, 20)) / 2) as u8;
        let (judgement, negotiation, patience, activity, scouting) =
            (roll(), roll(), roll(), roll(), roll());
        let gm = Manager {
            name: format!(
                "{} {}",
                first[rng.range(0, first.len() as u32) as usize],
                last[rng.range(0, last.len() as u32) as usize]
            ),
            age: 38 + rng.range(0, 28) as u8,
            judgement,
            negotiation,
            patience,
            activity,
        };
        let staff = [
            Job::HeadCoach,
            Job::Assistant,
            Job::Skills,
            Job::Fitness,
            Job::Scout,
        ]
        .map(|job| {
            let mut s = Staffer::generate(rng, job, first, last);
            if job == Job::Scout {
                s.judgement = scouting;
                s.wage = s.asking();
            }
            Some(s)
        });
        // Half the clubs start with a press officer.
        let press = if rng.chance(0.5) {
            Some(Staffer::generate(rng, Job::Press, first, last))
        } else {
            None
        };
        FrontOffice { gm, staff, press }
    }
}

/// How many out-of-work coaches and scouts are on the market.
const POOL: usize = 14;

/// The people looking for a job when the world begins.
pub(crate) fn market(seed: u64, first: &[&str], last: &[&str]) -> Vec<Staffer> {
    let rng = &mut Rng::new(seed ^ 0x57AF_F000);
    (0..POOL)
        .map(|i| Staffer::generate(rng, Job::HIRED[i % Job::HIRED.len()], first, last))
        .collect()
}

impl World {
    /// Give `job` at `club` to the `i`th person on the market. Whoever had
    /// it is paid off with a quarter of a year's wage and joins the market.
    pub fn hire(&mut self, club: TeamId, job: Job, i: usize) -> Result<String, String> {
        let Some(new) = self.staff_pool.get(i) else {
            return Err("He has taken another job.".into());
        };
        let team = &self.teams[club];
        let old = team.office.staffer(job);
        let severance = old.map_or(0, |s| s.wage / 4);
        if team.budget < severance + new.wage / 4 {
            return Err(format!(
                "The board will not fund it: {} to pay off, {} a year in wages.",
                money(severance),
                money(new.wage)
            ));
        }
        let new = self.staff_pool.remove(i);
        let note = match old {
            Some(old) => format!(
                "{} is the new {}. {} leaves with {} in severance.",
                new.name,
                job.label().to_lowercase(),
                old.name,
                money(severance)
            ),
            None => format!("{} is the new {}.", new.name, job.label().to_lowercase()),
        };
        self.transact(club, -severance, crate::finance::Cat::Severance);
        let team = &mut self.teams[club];
        if let Some(old) = team.office.slot_mut(job).replace(new) {
            self.staff_pool.push(old);
        }
        Ok(note)
    }

    /// Let `job` at `club` go without a replacement.
    pub fn dismiss(&mut self, club: TeamId, job: Job) {
        if let Some(old) = self.teams[club].office.slot_mut(job).take() {
            self.transact(club, -(old.wage / 4), crate::finance::Cat::Severance);
            self.staff_pool.push(old);
        }
    }

    /// The summer on the staff market: everyone is a year older, the old
    /// retire, clubs with a weak man in a job replace him with the best
    /// one free, and new faces come looking for work. Returns what
    /// happened at the user's club.
    pub(crate) fn staff_summer(&mut self, first: &[&str], last: &[&str]) -> Vec<String> {
        let me = self.watch;
        let mut news = Vec::new();
        for s in &mut self.staff_pool {
            s.age += 1;
        }
        self.staff_pool.retain(|s| s.age < 66);
        for club in 0..self.teams.len() {
            for job in Job::ALL {
                let Some(s) = self.teams[club].office.slot_mut(job).as_mut() else {
                    continue;
                };
                s.age += 1;
                if s.age >= 68 {
                    let name = s.name.clone();
                    *self.teams[club].office.slot_mut(job) = None;
                    if me == Some(club) {
                        news.push(format!(
                            "{name} has retired. You need a new {}.",
                            job.label().to_lowercase()
                        ));
                    }
                }
            }
        }
        // New people before the clubs shop, so there is someone to hire.
        while self.staff_pool.len() < POOL {
            let job = Job::HIRED[self.rng.range(0, Job::HIRED.len() as u32) as usize];
            self.staff_pool
                .push(Staffer::generate(&mut self.rng, job, first, last));
        }
        for club in 0..self.teams.len() {
            if me == Some(club) {
                continue;
            }
            for job in Job::ALL {
                let have = self.teams[club].office.staffer(job).map(|s| s.rating(job));
                if have.is_some_and(|r| r >= 9) {
                    continue;
                }
                let best =
                    (0..self.staff_pool.len()).max_by_key(|&i| self.staff_pool[i].rating(job));
                if let Some(i) = best
                    && self.staff_pool[i].rating(job) > have.unwrap_or(0) + 2
                {
                    let _ = self.hire(club, job, i);
                    // A new head coach brings his own system.
                    if job == Job::HeadCoach {
                        self.install_system(club);
                    }
                }
            }
        }
        while self.staff_pool.len() < POOL {
            let job = Job::HIRED[self.rng.range(0, Job::HIRED.len() as u32) as usize];
            self.staff_pool
                .push(Staffer::generate(&mut self.rng, job, first, last));
        }
        news
    }

    /// Set a club's tactic to what its head coach likes to run.
    pub(crate) fn install_system(&mut self, club: TeamId) {
        let team = &mut self.teams[club];
        let Some(head) = team.office.staffer(Job::HeadCoach) else {
            return;
        };
        let (offense, defense) = (head.offense, head.defense);
        let t = &mut team.playbook[team.active].tactics;
        (t.offense, t.defense) = (offense, defense);
    }
}

impl World {
    /// What `club`'s general manager believes a player is worth: the market
    /// value seen through his judgement and his plans.
    pub fn value_to(&self, club: TeamId, p: PlayerId) -> i64 {
        let gm = &self.teams[club].office.gm;
        let player = &self.players[p as usize];
        let misread = skew(p, 7000 + club as u32) * (20 - gm.judgement.min(20)) as f32 * 0.018;
        let age = match player.age {
            ..=23 => 0.22,
            24..=28 => 0.0,
            _ => -0.22,
        };
        // A name is worth more to a club that needs one.
        let name = 1.0 + self.commercial_value(club, p);
        let v = self.value(p) as f32 * (1.0 + misread) * (1.0 + age * gm.lean()) * name;
        ((v as i64) / 1000).max(3) * 1000
    }

    /// What a pick is worth to `club`: more to a builder, less to a club
    /// that wants to win this year.
    pub fn pick_value_to(&self, club: TeamId, i: usize) -> i64 {
        let lean = self.teams[club].office.gm.lean();
        (self.pick_value(i) as f32 * (1.0 + 0.3 * lean)) as i64 / 1000 * 1000
    }

    /// How much `club`'s front office likes a prospect, on the attribute
    /// scale. Poor scouting sees things that are not there.
    pub(crate) fn draft_grade(&self, club: TeamId, p: PlayerId) -> f32 {
        let office = &self.teams[club].office;
        let player = &self.players[p as usize];
        let blur = (20 - office.scouting().min(20)) as f32 * 0.11;
        let ceiling = 0.55 + 0.15 * office.gm.lean();
        (player.overall() + skew(p, 8000 + club as u32) * blur) * (1.0 - ceiling)
            + (player.potential + skew(p, 8500 + club as u32) * blur) * ceiling
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inbox::Deal;

    /// The same offer gets a different answer from a different man.
    #[test]
    fn the_general_manager_decides_the_deal() {
        let mut w = World::generate(6, 8);
        w.take_charge(0);
        let pick = |w: &World, t: TeamId, best: bool| {
            let ov = |p: &&PlayerId| (w.players[**p as usize].overall() * 100.0) as i32;
            let r = &w.teams[t].roster;
            *if best {
                r.iter().max_by_key(ov)
            } else {
                r.iter().min_by_key(ov)
            }
            .unwrap()
        };
        let mut deal = Deal {
            club: 1,
            give: vec![pick(&w, 0, false)],
            get: vec![pick(&w, 1, true)],
            ..Deal::default()
        };
        // This is about the two men's play, not their names.
        for p in w.players.iter_mut() {
            p.renown.fame = 0.0;
        }
        // Keep the salary cap out of it: this is about the two men.
        let pay = w.players[deal.give[0] as usize].contract.salary;
        w.players[deal.get[0] as usize].contract.salary = pay;
        let set = |w: &mut World, judgement, negotiation, patience| {
            let gm = &mut w.teams[1].office.gm;
            (gm.judgement, gm.negotiation, gm.patience) = (judgement, negotiation, patience);
        };
        // A perfect judge with no lean values both at market: find the
        // cash that makes it exactly fair to him.
        set(&mut w, 20, 10, 10);
        let (ours, theirs) = (w.value_to(1, deal.give[0]), w.value_to(1, deal.get[0]));
        assert_eq!(
            (ours, theirs),
            (w.value(deal.give[0]), w.value(deal.get[0]))
        );
        // Their best player carries the premium for a key man.
        let going = theirs * 14 / 10;
        deal.cash = going - ours + going * 5 / 100 + 1000;
        w.teams[0].budget = 10_000_000;
        w.teams[1].budget = 10_000_000;
        assert_eq!(w.trade_verdict(&deal).err(), None);
        // A hard bargainer wants more than that; a soft one took less.
        set(&mut w, 20, 20, 10);
        assert!(w.trade_verdict(&deal).is_err());
        deal.cash -= going * 8 / 100;
        assert!(w.trade_verdict(&deal).is_err());
        set(&mut w, 20, 1, 10);
        assert!(w.trade_verdict(&deal).is_ok());
        // A builder pays more for a first-round pick than a win-now man.
        let first = w
            .picks
            .iter()
            .position(|p| p.owner == 0 && p.round == 1)
            .unwrap();
        set(&mut w, 20, 10, 20);
        let builder = w.pick_value_to(1, first);
        set(&mut w, 20, 10, 1);
        assert!(builder > w.pick_value_to(1, first));
        // A poor judge is simply wrong about people, one way or the other.
        set(&mut w, 1, 10, 10);
        let wrong = w.teams[1]
            .roster
            .iter()
            .filter(|&&p| (w.value_to(1, p) - w.value(p)).abs() * 10 > w.value(p))
            .count();
        assert!(
            wrong >= 3,
            "{wrong} of his own players misjudged by over 10%"
        );
    }
}
