//! The manager's own career.
//!
//! He has a reputation that rises and falls with results measured against
//! what the club was expected to do. He keeps a record of the jobs he has
//! held. When another club's manager goes, a club that suits his
//! reputation may call. Taking the job is a small negotiation: he says
//! which of his staff he wants with him and which of the new club's staff
//! he keeps, and the staff have their own minds. A manager who is
//! dismissed, or resigns, is out of work until somebody calls.
//!
//! `World::watch` is the club he manages now, `None` when he is out of
//! work. Everything that belongs to the job (the desk, the scouts, the
//! tapes, the standing instructions) is cleared when he leaves it.

use serde::{Deserialize, Serialize};

use crate::inbox::{Kind, OfferState};
use crate::league::{FIRST_NAMES, LAST_NAMES, TeamId, World};
use crate::rng::Rng;
use crate::scouting::Scouting;
use crate::staff::{Job, Staffer};

/// Rounds a club takes to appoint a new manager.
pub const VACANCY_ROUNDS: u32 = 2;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ending {
    Current,
    Resigned,
    Dismissed,
    /// Left for another club.
    Moved,
}

/// One job the manager has held.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Spell {
    pub club: TeamId,
    pub from_season: u16,
    pub to_season: Option<u16>,
    pub wins: u16,
    pub losses: u16,
    /// Season and final league place.
    pub places: Vec<(u16, u8)>,
    pub titles: u8,
    pub ended: Ending,
    /// Season and division (0 is the top) of each final place in `places`.
    #[serde(default)]
    pub levels: Vec<(u16, u8)>,
    #[serde(default)]
    pub promotions: u8,
    #[serde(default)]
    pub relegations: u8,
}

/// A club without a manager, for a few rounds.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vacancy {
    pub club: TeamId,
    pub why: String,
    /// `season * 1000 + round` at which somebody is appointed.
    pub fills_at: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobOffer {
    pub club: TeamId,
    pub season: u16,
    pub day: u32,
    pub why: String,
    pub state: OfferState,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Career {
    /// 0..=100. Compare with `World::stature`.
    pub reputation: i32,
    pub spells: Vec<Spell>,
    pub vacancies: Vec<Vacancy>,
    pub offers: Vec<JobOffer>,
    /// Rounds played and days passed since the last job ended.
    pub idle_rounds: u32,
    /// The division of the club he left last, 0 being the top.
    #[serde(default)]
    pub last_division: usize,
}

/// What a member of staff makes of the move.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StaffVerdict {
    pub job: &'static str,
    pub name: String,
    pub willing: bool,
    pub reasons: Vec<String>,
}

/// What a deal would come to, before it is signed.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Terms {
    pub club: TeamId,
    /// Why the club called.
    pub why: String,
    /// The people you would bring, and what each says.
    pub bring: Vec<StaffVerdict>,
    /// The new club's staff and whether they stay.
    pub release: Vec<String>,
    /// Set when the owner will not agree.
    pub refused: Option<String>,
}

fn stable(s: &Staffer, salt: u64) -> i32 {
    let mut h = s.name.bytes().fold(0xCBF2_9CE4_8422_2325u64 ^ salt, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100_0000_01B3)
    });
    h ^= (s.age as u64) << 20;
    h = (h ^ (h >> 31)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    ((h >> 40) % 11) as i32
}

impl World {
    pub fn jobless(&self) -> bool {
        self.watch.is_none() && !self.career.spells.is_empty()
    }

    fn key(&self) -> u32 {
        self.season as u32 * 1000 + self.round as u32
    }

    /// Starting a job, the first or a later one.
    pub(crate) fn career_arrive(&mut self, club: TeamId) {
        if self.career.spells.is_empty() {
            self.career.reputation = (self.stature(club) as i32 * 85 / 100).max(20);
        }
        self.career.idle_rounds = 0;
        self.career.spells.push(Spell {
            club,
            from_season: self.season,
            to_season: None,
            wins: 0,
            losses: 0,
            places: Vec::new(),
            titles: 0,
            ended: Ending::Current,
            levels: Vec::new(),
            promotions: 0,
            relegations: 0,
        });
        // A board hires a name with more hope than it hires a stranger.
        let hope = (self.career.reputation - self.stature(club) as i32) / 5;
        let b = &mut self.club.clubs[club].board;
        b.confidence = (60.0 + hope as f32).clamp(45.0, 75.0);
    }

    fn spell_mut(&mut self) -> Option<&mut Spell> {
        self.career
            .spells
            .last_mut()
            .filter(|s| s.ended == Ending::Current)
    }

    /// The manager's season is over: his reputation follows how it went.
    pub(crate) fn career_close_season(&mut self, tables: &[Vec<TeamId>], champion: TeamId) {
        let Some(me) = self.watch else {
            if self.jobless() {
                self.career.reputation = (self.career.reputation - 2).max(0);
            }
            return;
        };
        let division = self.teams[me].division;
        let table: &[TeamId] = tables.get(division).map_or(&[], |t| t.as_slice());
        let n = table.len();
        let place = table.iter().position(|&t| t == me).map_or(n, |p| p + 1);
        let e = self.expected_finish(me);
        let champ = if division == 0 { me == champion } else { place == 1 };
        let mut d = (e as i32 - place as i32) * 2;
        if champ {
            d += 6;
        } else if place <= 4 {
            d += 1;
        }
        // Doing well at a club far below one's name proves little; doing
        // well at a small club proves a lot.
        if d > 0 && (self.stature(me) as i32) < self.career.reputation - 25 {
            d /= 2;
        }
        let d = d.clamp(-8, 8);
        self.career.reputation = (self.career.reputation + d).clamp(0, 100);
        let (w, l) = (self.standings[me].wins, self.standings[me].losses);
        let season = self.season;
        if let Some(s) = self.spell_mut() {
            s.wins += w;
            s.losses += l;
            s.places.push((season, place as u8));
            s.levels.push((season, division as u8));
            s.titles += champ as u8;
        }
        // Doing well below the top gets noticed above.
        if d > 0 && division > 0 && (champ || place <= 2) {
            self.approach_from_above(me, division);
        }
    }

    /// A club of the division above loses its manager and calls the one who
    /// did well: not every year.
    fn approach_from_above(&mut self, me: TeamId, division: usize) {
        let mut rng = Rng::new(self.econ.seed ^ 0xA99_0000 ^ ((self.season as u64) << 24));
        if !rng.chance(0.6) {
            return;
        }
        let rep = self.career.reputation;
        let pick = self
            .division_teams(division - 1)
            .into_iter()
            .filter(|&c| c != me && !self.career.vacancies.iter().any(|v| v.club == c))
            .min_by_key(|&c| ((self.stature(c) as i32 - (rep + 5)).abs(), c));
        if let Some(c) = pick {
            let from = self.teams[me].name.clone();
            // The call comes first, so the offer carries the real reason.
            self.make_offer(c, &format!("Your results at {from} caught their eye."));
            self.manager_out(c, "the manager left for another job");
        }
    }

    // ---- other clubs' managers ------------------------------------------

    /// A club's manager goes: dismissed, or he leaves. For the user's own
    /// club it is a dismissal.
    pub(crate) fn manager_out(&mut self, club: TeamId, why: &str) {
        if self.watch == Some(club) {
            self.manager_dismissed(why);
            return;
        }
        if self.career.vacancies.iter().any(|v| v.club == club) {
            return;
        }
        let name = self.teams[club].name.clone();
        if let Some(head) = self.teams[club].office.slot_mut(Job::HeadCoach).take() {
            self.staff_pool.push(head);
        }
        let b = &mut self.club.clubs[club].board;
        b.confidence = 50.0;
        b.warned = false;
        b.losing_run = 0;
        b.winning_run = 0;
        let fills_at = self.key() + VACANCY_ROUNDS;
        self.career.vacancies.push(Vacancy {
            club,
            why: why.to_string(),
            fills_at,
        });
        if self.watch.is_some() || self.jobless() {
            self.post(
                self.day,
                Kind::League,
                "League office",
                format!("{name} lose their manager"),
                format!("{name} have parted with their manager: {why}. They are looking for a new one."),
                vec![],
            );
        }
        self.consider_vacancy(club);
    }

    /// Whether the job suits the manager's name, and if so a call comes.
    pub(crate) fn consider_vacancy(&mut self, club: TeamId) {
        if self.career.spells.is_empty() {
            return;
        }
        let rep = self.career.reputation;
        let s = self.stature(club) as i32;
        let division = self.teams[club].division;
        let fits = match self.watch {
            // In work he listens to equals and betters, and to the
            // division above his own, never to a leap of two.
            Some(me) => {
                s <= rep + 10
                    && s >= self.stature(me) as i32 - 5
                    && s >= rep - 15
                    && division + 1 >= self.teams[me].division
            }
            // Out of work, the calls come from below, from his own level
            // and, now and then, from one level up.
            None => {
                let last = self.career.last_division;
                let step_up = division < last;
                let luck = Rng::new(self.econ.seed ^ 0x0FFE_4000 ^ ((club as u64) << 16) ^ self.season as u64)
                    .chance(0.25);
                s <= rep + 10 && s >= rep - 50 && (!step_up || (division + 1 == last && luck))
            }
        };
        // Nobody is called back to the job he has just left.
        let left = self
            .career
            .spells
            .last()
            .is_some_and(|s| s.club == club && s.ended != Ending::Current);
        if fits && self.watch != Some(club) && !(left && self.watch.is_none()) {
            self.make_offer(club, "Your name was the first they thought of.");
        }
    }

    pub(crate) fn make_offer(&mut self, club: TeamId, why: &str) {
        if self
            .career
            .offers
            .iter()
            .any(|o| o.club == club && o.state == OfferState::Open)
        {
            return;
        }
        let i = self.career.offers.len();
        self.career.offers.push(JobOffer {
            club,
            season: self.season,
            day: self.day,
            why: why.to_string(),
            state: OfferState::Open,
        });
        let face = self.club.clubs[club].board.face().name.clone();
        let name = self.teams[club].name.clone();
        let rep = self.career.reputation;
        let rank = self.stature(club) as i32;
        let div = self.division_name(self.teams[club].division);
        let step = if rank > self.watch.map_or(0, |m| self.stature(m) as i32) + 5 {
            "It would be a step up."
        } else if self.watch.is_none() {
            "It is work, and you need it."
        } else {
            "It would be a sideways move."
        };
        let body = format!(
            "{name} need a manager. {why}\n\nI would like to talk to you. {step} You would \
             arrive with a reputation of {rep}; the club stands at {rank} and plays in the {div}.\n\nLook at the \
             club, the squad and the money on the Career page, and tell me which of your \
             people you want with you. The job will not wait long.\n\n{face}"
        );
        let m = self.post(
            self.day,
            Kind::Board,
            &face,
            format!("{name}: a job offer"),
            body,
            vec![],
        );
        m.job = Some(i);
    }

    /// Open jobs get filled, and offers for them lapse.
    pub(crate) fn career_fill_vacancies(&mut self) {
        let key = self.key();
        let due: Vec<TeamId> = self
            .career
            .vacancies
            .iter()
            .filter(|v| v.fills_at <= key)
            .map(|v| v.club)
            .collect();
        for club in due {
            self.career.vacancies.retain(|v| v.club != club);
            self.appoint_head_coach(club);
            for o in &mut self.career.offers {
                if o.club == club && o.state == OfferState::Open {
                    o.state = OfferState::Lapsed;
                }
            }
        }
        // Out of work for a long time with nobody calling: a small club
        // changes its manager.
        if self.jobless() {
            self.career.idle_rounds += 1;
            let waiting = self
                .career
                .offers
                .iter()
                .any(|o| o.state == OfferState::Open);
            if !waiting && self.career.idle_rounds >= 3 && self.career.idle_rounds % 3 == 0 {
                let rep = self.career.reputation;
                let last = self.career.last_division;
                let pick = (0..self.teams.len())
                    .filter(|&c| self.teams[c].division >= last)
                    .filter(|&c| !self.career.vacancies.iter().any(|v| v.club == c))
                    .min_by_key(|&c| ((self.stature(c) as i32 - (rep - 10)).abs(), c));
                if let Some(c) = pick {
                    self.manager_out(c, "the manager resigned");
                    self.make_offer(c, "The job is open and nobody better has called.");
                }
            }
        }
    }

    /// Put a head coach on a club's bench: the best free one, or a new face.
    pub(crate) fn appoint_head_coach(&mut self, club: TeamId) {
        if self.teams[club].office.staffer(Job::HeadCoach).is_some() {
            return;
        }
        let best = (0..self.staff_pool.len())
            .max_by_key(|&i| (self.staff_pool[i].tactics, std::cmp::Reverse(i)))
            .filter(|&i| self.staff_pool[i].tactics >= 9);
        let coach = match best {
            Some(i) => self.staff_pool.remove(i),
            None => {
                let mut rng = Rng::new(
                    self.econ.seed ^ 0xC0AC_4000 ^ ((self.season as u64) << 24) ^ ((club as u64) << 8)
                        ^ self.round as u64,
                );
                Staffer::generate(&mut rng, Job::HeadCoach, &FIRST_NAMES, &LAST_NAMES)
            }
        };
        *self.teams[club].office.slot_mut(Job::HeadCoach) = Some(coach);
        self.install_system(club);
    }

    /// Every empty job at an AI club goes to the best free person, if there
    /// is anyone worth having.
    fn refill_staff(&mut self, club: TeamId) {
        for job in Job::HIRED {
            if self.teams[club].office.staffer(job).is_some() {
                continue;
            }
            let best = (0..self.staff_pool.len())
                .max_by_key(|&i| (self.staff_pool[i].rating(job), std::cmp::Reverse(i)));
            if let Some(i) = best {
                let s = self.staff_pool.remove(i);
                *self.teams[club].office.slot_mut(job) = Some(s);
            }
        }
    }

    // ---- leaving a job ---------------------------------------------------

    /// Everything that belongs to the manager's job is cleared, and the
    /// club carries on as an AI club with a new man.
    fn leave_club(&mut self, old: TeamId, how: Ending) {
        let season = self.season;
        let (w, l) = (self.standings[old].wins, self.standings[old].losses);
        if let Some(s) = self.spell_mut() {
            s.to_season = Some(season);
            s.ended = how;
            // The season in progress counts.
            s.wins += w;
            s.losses += l;
        }
        self.career.last_division = self.teams[old].division;
        self.watch = None;
        // His desk: the transfer business, the questions, the promises.
        self.desk.offers.clear();
        self.desk.bids.clear();
        self.desk.press.clear();
        self.desk.pledge = None;
        self.desk.room = 3;
        self.desk.expectation = 0;
        for m in &mut self.desk.messages {
            m.offer = None;
            m.press = None;
            m.sponsor = None;
            m.appearance = None;
        }
        self.fame.asks.clear();
        self.scouting = Scouting::new(self.players.len());
        self.replay = None;
        self.tapes.clear();
        // The club: standing rules and plans were his.
        let team = &mut self.teams[old];
        team.instructions = Default::default();
        team.delegate_training = true;
        team.assistant_notes.clear();
        let mood = match how {
            Ending::Dismissed => 0.02,
            _ => -0.04,
        };
        team.belief = (team.belief + if how == Ending::Dismissed { 0.0 } else { -0.08 }).clamp(0.0, 1.0);
        for p in self.teams[old].roster.clone() {
            let pl = &mut self.players[p as usize];
            pl.morale = (pl.morale + mood).clamp(0.0, 1.0);
        }
        let b = &mut self.club.clubs[old].board;
        b.confidence = 55.0;
        b.warned = false;
        b.losing_run = 0;
        b.winning_run = 0;
        b.reasons.clear();
        self.appoint_head_coach(old);
        self.refill_staff(old);
        self.career.idle_rounds = 0;
    }

    /// The board has had enough.
    pub(crate) fn manager_dismissed(&mut self, why: &str) {
        let Some(me) = self.watch else { return };
        let face = self.club.clubs[me].board.face().name.clone();
        let name = self.teams[me].name.clone();
        let body = format!(
            "{why}\n\nThe board has decided to end your contract with {name}, with immediate \
             effect. Thank you for what you gave the club. Clear your desk today.\n\n{face}"
        );
        self.post(self.day, Kind::Board, &face, "You are dismissed", body, vec![]);
        self.career.reputation = (self.career.reputation - 8).max(0);
        self.leave_club(me, Ending::Dismissed);
        // Open jobs may suit him now.
        let open: Vec<TeamId> = self.career.vacancies.iter().map(|v| v.club).collect();
        for c in open {
            self.consider_vacancy(c);
        }
    }

    fn can_move(&self) -> Result<(), String> {
        if self.draft.is_some() {
            return Err("The draft is on. Finish it first.".into());
        }
        Ok(())
    }

    /// The manager walks away.
    pub fn resign(&mut self) -> Result<String, String> {
        let me = self.watch.ok_or("You have no job to leave.")?;
        self.can_move()?;
        let name = self.teams[me].name.clone();
        let face = self.club.clubs[me].board.face().name.clone();
        self.post(
            self.day,
            Kind::Board,
            &face,
            "Your resignation",
            format!("The board has accepted your resignation from {name}. We are sorry to see you go.\n\n{face}"),
            vec![],
        );
        self.leave_club(me, Ending::Resigned);
        let open: Vec<TeamId> = self.career.vacancies.iter().map(|v| v.club).collect();
        for c in open {
            self.consider_vacancy(c);
        }
        Ok(format!("You have left {name}."))
    }

    // ---- the deal -----------------------------------------------------------

    /// What `s` thinks of coming along to `to`.
    pub fn staff_will(&self, to: TeamId, job: Job, s: &Staffer) -> StaffVerdict {
        let from = self.watch;
        let tenure = self
            .career
            .spells
            .last()
            .map_or(0, |sp| (self.season - sp.from_season) as i32);
        let to_you = stable(s, 1) + tenure * 2 + (self.career.reputation >= 60) as i32 * 2;
        let to_club = stable(s, 2) + (s.age >= 55) as i32 * 3;
        let step = (self.stature(to) as i32 - from.map_or(0, |f| self.stature(f) as i32)) / 8;
        let pay = (self.club.clubs[to].board.spending() as i32 - 10) / 3;
        let score = to_you - to_club + step + pay;
        let mut reasons = Vec::new();
        if to_you >= 9 {
            reasons.push("devoted to you".to_string());
        } else if to_you <= 3 {
            reasons.push("owes you little".to_string());
        }
        if to_club >= 8 {
            reasons.push("rooted at the club".to_string());
        }
        if step >= 3 {
            reasons.push("sees a step up".to_string());
        } else if step <= -3 {
            reasons.push("sees a step down".to_string());
        }
        if pay >= 2 {
            reasons.push("the new owner pays well".to_string());
        } else if pay <= -2 {
            reasons.push("the new owner pays badly".to_string());
        }
        StaffVerdict {
            job: job.label(),
            name: s.name.clone(),
            willing: score >= 2,
            reasons,
        }
    }

    /// What taking offer `offer` with `bring` (your people who come) and
    /// `keep` (the new club's who stay) would come to.
    pub fn job_terms(&self, offer: usize, bring: &[Job], keep: &[Job]) -> Result<Terms, String> {
        let o = self.career.offers.get(offer).ok_or("No such offer.")?;
        let club = o.club;
        let mut verdicts = Vec::new();
        for &job in bring {
            if job == Job::HeadCoach {
                return Err("The head coach is you.".into());
            }
            let me = self.watch.ok_or("You have no staff to bring.")?;
            let s = self.teams[me]
                .office
                .staffer(job)
                .ok_or_else(|| format!("You have no {} to bring.", job.label().to_lowercase()))?;
            verdicts.push(self.staff_will(club, job, s));
        }
        let mut release = Vec::new();
        for job in Job::HIRED {
            let Some(s) = self.teams[club].office.staffer(job) else {
                continue;
            };
            let replaced = bring
                .iter()
                .zip(&verdicts)
                .any(|(&j, v)| j == job && v.willing);
            if !keep.contains(&job) || replaced {
                release.push(format!("{} ({})", s.name, job.label()));
            }
        }
        let loyalty = self.club.clubs[club].board.loyalty();
        let allowed = match loyalty {
            15.. => 1,
            10..=14 => 3,
            _ => 5,
        };
        let refused = (release.len() > allowed).then(|| {
            format!(
                "{} will not have that many of his people sent away. At most {allowed}.",
                self.club.clubs[club].board.face().name
            )
        });
        Ok(Terms {
            club,
            why: o.why.clone(),
            bring: verdicts,
            release,
            refused,
        })
    }

    /// Take the job.
    pub fn accept_job(&mut self, offer: usize, bring: &[Job], keep: &[Job]) -> Result<String, String> {
        self.can_move()?;
        let o = self.career.offers.get(offer).ok_or("No such offer.")?;
        if o.state != OfferState::Open {
            return Err("That offer is no longer open.".into());
        }
        let club = o.club;
        if !self.career.vacancies.iter().any(|v| v.club == club) {
            return Err("The club has found somebody else.".into());
        }
        let terms = self.job_terms(offer, bring, keep)?;
        if let Some(why) = terms.refused {
            return Err(why);
        }
        // Your people who will come, taken out of the old club.
        let mut coming: Vec<(Job, Staffer)> = Vec::new();
        let mut stayed: Vec<String> = Vec::new();
        if let Some(me) = self.watch {
            for (&job, v) in bring.iter().zip(&terms.bring) {
                if v.willing {
                    if let Some(s) = self.teams[me].office.slot_mut(job).take() {
                        coming.push((job, s));
                    }
                } else {
                    stayed.push(format!("{} ({})", v.name, v.job.to_lowercase()));
                }
            }
        }
        let old = self.watch;
        let old_name = old.map(|o| self.teams[o].name.clone());
        let mid_season = !self.season_over() && self.round > 0;
        if let Some(old) = old {
            let face = self.club.clubs[old].board.face().clone();
            let body = if mid_season && face.loyalty >= 12 {
                "You leave us in the middle of the season. I will not forget how you did it."
            } else if face.loyalty >= 12 {
                "I am sorry to see you go. I wish you well."
            } else {
                "The club goes on. We will find somebody."
            };
            self.post(
                self.day,
                Kind::Board,
                &face.name,
                "You have left the club",
                body.to_string(),
                vec![],
            );
            self.leave_club(old, Ending::Moved);
        }
        // The new club's people: those not kept are paid off.
        for job in Job::HIRED {
            let go = self.teams[club].office.staffer(job).is_some()
                && (!keep.contains(&job) || coming.iter().any(|(j, _)| *j == job));
            if go {
                self.dismiss(club, job);
            }
        }
        for (job, s) in coming {
            if let Some(prev) = self.teams[club].office.slot_mut(job).replace(s) {
                self.staff_pool.push(prev);
            }
        }
        self.career.vacancies.retain(|v| v.club != club);
        for (i, of) in self.career.offers.iter_mut().enumerate() {
            if i == offer {
                of.state = OfferState::Accepted;
            } else if of.state == OfferState::Open {
                of.state = OfferState::Rejected;
            }
        }
        self.take_charge(club);
        let name = self.teams[club].name.clone();
        let mut note = format!("You are the manager of {name}.");
        if let Some(o) = old_name {
            note += &format!(" You have left {o}.");
        }
        if !stayed.is_empty() {
            note += &format!(" Staying behind: {}.", stayed.join(", "));
        }
        Ok(note)
    }

    pub fn decline_job(&mut self, offer: usize) -> Result<String, String> {
        let o = self.career.offers.get_mut(offer).ok_or("No such offer.")?;
        if o.state != OfferState::Open {
            return Err("That offer is no longer open.".into());
        }
        o.state = OfferState::Rejected;
        Ok("You have turned the job down.".into())
    }

    /// Offers that can still be answered.
    pub fn open_job_offers(&self) -> Vec<usize> {
        (0..self.career.offers.len())
            .filter(|&i| {
                let o = &self.career.offers[i];
                o.state == OfferState::Open && self.career.vacancies.iter().any(|v| v.club == o.club)
            })
            .collect()
    }
}

// ---- what the screens and the agents see -----------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct SpellView {
    pub club: TeamId,
    pub club_name: String,
    pub from_year: u16,
    pub to_year: Option<u16>,
    pub wins: u16,
    pub losses: u16,
    pub best_place: Option<u8>,
    /// The division of the best place, 0 being the top.
    pub best_division: u8,
    pub division_name: String,
    pub promotions: u8,
    pub relegations: u8,
    pub titles: u8,
    pub ended: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct OfferView {
    pub offer: usize,
    pub club: TeamId,
    pub club_name: String,
    pub stature: u8,
    pub division: usize,
    pub division_name: String,
    pub why: String,
    pub state: &'static str,
    pub open: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CareerView {
    pub reputation: i32,
    pub reputation_word: &'static str,
    pub jobless: bool,
    pub club: Option<TeamId>,
    pub spells: Vec<SpellView>,
    pub offers: Vec<OfferView>,
    /// Clubs that are looking for a manager now.
    pub vacancies: Vec<TeamId>,
}

#[derive(Clone, Debug, Serialize)]
pub struct JobOfferView {
    pub offer: usize,
    pub summary: crate::club::ClubSummary,
    pub terms: Terms,
    pub your_staff: Vec<crate::club::StaffLine>,
    pub their_staff: Vec<crate::club::StaffLine>,
    pub cash: i64,
    pub wage_budget: i64,
    pub transfer_budget: i64,
    pub your_club_stature: Option<u8>,
}

pub fn reputation_word(r: i32) -> &'static str {
    match r {
        80.. => "a famous name",
        60..=79 => "well known",
        40..=59 => "respected",
        20..=39 => "little known",
        _ => "an unknown",
    }
}

fn staff_lines(office: &crate::staff::FrontOffice) -> Vec<crate::club::StaffLine> {
    Job::HIRED
        .iter()
        .filter_map(|&j| {
            office.staffer(j).map(|s| crate::club::StaffLine {
                job: j.label(),
                name: s.name.clone(),
                rating: s.rating(j),
                skill: j.skill(),
            })
        })
        .collect()
}

impl World {
    /// The best finish of a spell: division and place, the top division
    /// first.
    fn best_finish(&self, s: &Spell) -> Option<(u8, u8)> {
        s.places
            .iter()
            .map(|&(season, place)| {
                let d = s.levels.iter().find(|l| l.0 == season).map_or(0, |l| l.1);
                (d, place)
            })
            .min()
    }

    pub fn view_career(&self) -> CareerView {
        let year = |s: u16| crate::club::START_YEAR + s;
        CareerView {
            reputation: self.career.reputation,
            reputation_word: reputation_word(self.career.reputation),
            jobless: self.jobless(),
            club: self.watch,
            spells: self
                .career
                .spells
                .iter()
                .map(|s| SpellView {
                    club: s.club,
                    club_name: self.teams[s.club].name.clone(),
                    from_year: year(s.from_season),
                    to_year: s.to_season.map(year),
                    // The season in progress counts for the job he holds.
                    wins: s.wins
                        + if s.ended == Ending::Current { self.standings[s.club].wins } else { 0 },
                    losses: s.losses
                        + if s.ended == Ending::Current { self.standings[s.club].losses } else { 0 },
                    best_place: self.best_finish(s).map(|b| b.1),
                    best_division: self.best_finish(s).map_or(0, |b| b.0),
                    division_name: self.division_name(self.teams[s.club].division),
                    promotions: s.promotions,
                    relegations: s.relegations,
                    titles: s.titles,
                    ended: match s.ended {
                        Ending::Current => "current",
                        Ending::Resigned => "resigned",
                        Ending::Dismissed => "dismissed",
                        Ending::Moved => "moved on",
                    },
                })
                .collect(),
            offers: self
                .career
                .offers
                .iter()
                .enumerate()
                .map(|(i, o)| OfferView {
                    offer: i,
                    club: o.club,
                    club_name: self.teams[o.club].name.clone(),
                    stature: self.stature(o.club),
                    division: self.teams[o.club].division,
                    division_name: self.division_name(self.teams[o.club].division),
                    why: o.why.clone(),
                    state: match o.state {
                        OfferState::Open => "open",
                        OfferState::Accepted => "accepted",
                        OfferState::Rejected => "declined",
                        OfferState::Lapsed => "lapsed",
                    },
                    open: self.open_job_offers().contains(&i),
                })
                .collect(),
            vacancies: self.career.vacancies.iter().map(|v| v.club).collect(),
        }
    }

    /// The new club as the manager would find it, and the deal as it
    /// stands for the staff he chooses.
    pub fn view_job_offer(
        &self,
        offer: usize,
        bring: &[Job],
        keep: &[Job],
    ) -> Result<JobOfferView, String> {
        let terms = self.job_terms(offer, bring, keep)?;
        let club = terms.club;
        let books = &self.econ.books[club];
        Ok(JobOfferView {
            offer,
            summary: self.club_summary(club),
            terms,
            your_staff: self.watch.map_or(Vec::new(), |m| staff_lines(&self.teams[m].office)),
            their_staff: staff_lines(&self.teams[club].office),
            cash: self.teams[club].budget,
            wage_budget: books.wage_budget,
            transfer_budget: books.transfer_budget,
            your_club_stature: self.watch.map(|m| self.stature(m)),
        })
    }
}

impl World {
    /// The board dismisses the manager now. Not a player action: for the
    /// screenshot harness and tests.
    pub fn stage_dismissal(&mut self) {
        self.manager_dismissed("The results did not come and the board lost patience.");
    }

    /// Make `club` lose its manager and call, whatever the manager's name
    /// is worth. Not a player action: for the screenshot harness and tests.
    pub fn stage_offer(&mut self, club: TeamId) -> Option<usize> {
        self.career.reputation = self.career.reputation.max(self.stature(club) as i32);
        self.manager_out(club, "staged");
        if self.open_job_offers().iter().all(|&i| self.career.offers[i].club != club) {
            self.make_offer(club, "Your name was the first they thought of.");
        }
        self.open_job_offers()
            .into_iter()
            .find(|&i| self.career.offers[i].club == club)
    }
}
