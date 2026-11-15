//! What the people upstairs make of the manager.
//!
//! Every club's board keeps a confidence number for its manager. It moves
//! after every game (against what the club was expected to do), when the
//! books are looked at, when a promise is kept or broken, and when the
//! season ends. How far it moves, and how low it may fall, is decided by
//! the owner and the chairman: a patient man tolerates a long bad run, an
//! ambitious one is hard to please, a generous one lets the money flow.
//! Below the warning line the manager is told; below the dismissal line,
//! after a warning, he goes.
//!
//! Front offices change too: owners sell, chairmen and general managers are
//! replaced, and the new people bring their own ways.
//!
//! Everything here draws from streams of its own.

use crate::club::Person;
use crate::inbox::Kind;
use crate::league::{FIRST_NAMES, LAST_NAMES, TeamId, World};
use crate::rng::Rng;
use crate::staff::FrontOffice;

/// Rounds that must have been played before a board will dismiss anyone.
pub const GRACE_ROUNDS: usize = 8;

/// The confidence below which a dismissal can happen, for this patience.
pub fn sack_line(patience: u8) -> f32 {
    10.0 + (20 - patience.min(20)) as f32
}

/// The confidence below which the manager is warned.
pub fn warn_line(patience: u8) -> f32 {
    sack_line(patience) + 15.0
}

/// How hard a bad result hits: below one for a patient board.
pub fn harshness(patience: u8) -> f32 {
    1.5 - 0.05 * patience.min(20) as f32
}

impl World {
    /// Where the board wants the club to finish, 1 = champions. The squad
    /// and the name of the club say what is reasonable, and the owner's
    /// ambition moves it.
    pub fn board_target(&self, club: TeamId) -> usize {
        let n = self.teams.len();
        let strength = |t: TeamId| -> f32 {
            let mut ov: Vec<f32> = self.teams[t]
                .roster
                .iter()
                .map(|&p| self.players[p as usize].overall())
                .collect();
            ov.sort_by(|a, b| b.total_cmp(a));
            ov.iter().take(8).sum()
        };
        let mine = strength(club);
        let squad = 1 + (0..n).filter(|&t| strength(t) > mine).count();
        let name = self.stature(club);
        let known = 1 + (0..n).filter(|&t| self.stature(t) > name).count();
        let base = (squad + known).div_ceil(2) as i32;
        let ambition = self.club.clubs[club].board.ambition() as i32;
        let shift = (10 - ambition) * n as i32 / 40;
        let worst = n.saturating_sub(2).max(1) as i32;
        (base + shift).clamp(1, worst) as usize
    }

    /// Match results against what was expected.
    pub(crate) fn club_after_round(&mut self, played: &[usize]) {
        let n = self.teams.len();
        let (season, round) = (self.season, self.round as u16);
        for &i in played {
            let f = &self.fixtures[i];
            let Some(r) = f.result.as_ref().filter(|_| !f.playoff) else {
                continue;
            };
            let home_won = r.score[0] > r.score[1];
            let margin = (r.score[0] as i32 - r.score[1] as i32).abs();
            let (home, away) = (f.home, f.away);
            self.board_game(home, away, home_won, margin, true);
            self.board_game(away, home, !home_won, margin, false);
        }
        if self.round.is_multiple_of(4) {
            for club in 0..n {
                self.board_books(club);
            }
        }
        for club in 0..n {
            self.board_verdict(club, season, round);
        }
    }

    pub(crate) fn board_game(&mut self, club: TeamId, opp: TeamId, won: bool, margin: i32, home: bool) {
        let n = self.teams.len().max(2) as i32;
        let e = self.expected_finish(club).clamp(1, n as usize) as i32;
        let e_opp = self.expected_finish(opp).clamp(1, n as usize) as i32;
        let expect = (80 - 60 * (e - 1) / (n - 1) + 30 * (e_opp - e) / (n - 1)).clamp(15, 90);
        let rival = self.club.clubs[club].identity.rival == opp;
        let (season, round) = (self.season, self.round as u16);
        let opp_name = self.teams[opp].name.clone();
        let b = &mut self.club.clubs[club].board;
        let harsh = harshness(b.patience());
        // An involved owner takes a result personally.
        let involved = match b.interference() {
            14.. => 1.3,
            ..=7 => 0.7,
            _ => 1.0,
        };
        let mut delta = if won {
            4.0 * (100 - expect) as f32 / 100.0
        } else {
            -4.0 * expect as f32 / 100.0 * harsh
        };
        let mut why: Option<String> = None;
        if won {
            b.losing_run = 0;
            b.winning_run = b.winning_run.saturating_add(1);
        } else {
            b.winning_run = 0;
            b.losing_run = b.losing_run.saturating_add(1);
        }
        if rival {
            delta *= 1.5 * involved;
            why = Some(if won {
                format!("Pleased with the win over {opp_name}, our rival.")
            } else {
                format!("Unhappy with the defeat by {opp_name}, our rival.")
            });
        } else if margin >= 20 {
            delta += if won { 1.0 } else { -1.5 * harsh * involved };
            why = Some(if won {
                format!("Pleased with the big win over {opp_name}.")
            } else {
                format!(
                    "Unhappy with the heavy defeat {}{opp_name}.",
                    if home { "by " } else { "at " }
                )
            });
        }
        if !won && b.losing_run == 3 {
            delta -= 1.5 * harsh * involved;
            why = Some("Worried: three defeats in a row.".into());
        }
        if won && b.winning_run == 4 {
            delta += 2.0;
            why = Some("Pleased with four wins in a row.".into());
        }
        b.confidence = (b.confidence + delta).clamp(0.0, 100.0);
        if let Some(w) = why {
            b.push_reason(season, round, delta, w);
        }
        let nag = !won && b.losing_run == 3 && b.interference() >= 14;
        if nag && self.watch == Some(club) {
            let from = self.club.clubs[club].board.face().name.clone();
            self.post(
                self.day,
                Kind::Board,
                &from,
                "Three defeats",
                "I have watched all three. Change something. I mean it.".to_string(),
                vec![],
            );
        }
    }

    /// The books, every four rounds: debt and the wage bill.
    fn board_books(&mut self, club: TeamId) {
        let (season, round) = (self.season, self.round as u16);
        let overdrawn = self.teams[club].budget < 0;
        let wage_over = self.econ.books.get(club).is_some_and(|b| b.wage_budget > 0)
            && self.wage_left(club) < 0;
        let healthy = self.teams[club].budget > 400_000 && !wage_over;
        let b = &mut self.club.clubs[club].board;
        // A free spender worries less about money.
        let worry = (21 - b.spending().min(20)) as f32 / 10.0;
        if overdrawn {
            let d = -2.0 * worry;
            b.confidence = (b.confidence + d).max(0.0);
            b.push_reason(season, round, d, "Worried about the overdraft.");
        } else if wage_over {
            let d = -1.5 * worry;
            b.confidence = (b.confidence + d).max(0.0);
            b.push_reason(season, round, d, "Worried about the wage bill.");
        } else if healthy {
            b.confidence = (b.confidence + 0.8).min(100.0);
        }
    }

    /// A promise to the press kept or broken.
    pub(crate) fn board_pledge(&mut self, bold: bool, won: bool) {
        let Some(me) = self.watch else { return };
        if !bold {
            return;
        }
        let (season, round) = (self.season, self.round as u16);
        let b = &mut self.club.clubs[me].board;
        let (d, text) = if won {
            (3.0, "Pleased that you kept your word to the press.")
        } else {
            (-3.0 * harshness(b.patience()), "Angry about the promise you made and broke.")
        };
        b.confidence = (b.confidence + d).clamp(0.0, 100.0);
        b.push_reason(season, round, d, text);
    }

    /// Warn, or dismiss, whoever has lost the board.
    pub(crate) fn board_verdict(&mut self, club: TeamId, season: u16, round: u16) {
        let b = &self.club.clubs[club].board;
        let p = b.patience();
        if b.confidence < warn_line(p) && !b.warned {
            self.club.clubs[club].board.warned = true;
            if self.watch == Some(club) {
                let face = self.club.clubs[club].board.face().clone();
                let body = format!(
                    "{}\n\nI am not happy. The results are not what we agreed. I need \
                     to see an improvement soon. This is a warning.",
                    face.name
                );
                self.club.clubs[club].board.push_reason(
                    season,
                    round,
                    0.0,
                    "The board has warned you.",
                );
                self.post(
                    self.day,
                    Kind::Board,
                    &face.name.clone(),
                    "A warning",
                    body,
                    vec![],
                );
            }
        } else if b.confidence < sack_line(p) - self.first_season_grace(club)
            && b.warned
            && self.round >= GRACE_ROUNDS
            && !self.season_over()
            && self.in_the_bottom_third(club)
        {
            self.manager_out(club, "The results did not come and the board lost patience.");
        }
    }

    /// A new manager is given his first season unless it is a disaster:
    /// the dismissal line is this much lower for him.
    pub(crate) fn first_season_grace(&self, club: TeamId) -> f32 {
        let new = self.watch == Some(club)
            && self
                .career
                .spells
                .last()
                .is_some_and(|s| s.from_season == self.season);
        if new { 10.0 } else { 0.0 }
    }

    /// A board does not sack a manager whose club is in the top two
    /// thirds of the table, whatever the numbers say.
    pub(crate) fn in_the_bottom_third(&self, club: TeamId) -> bool {
        let n = self.teams.len();
        let place = self.table().iter().position(|&t| t == club).unwrap_or(0);
        place * 3 >= n * 2
    }

    /// The season is over: judge it, cool the tempers, and change the
    /// people upstairs where it is time.
    pub(crate) fn club_close_season(&mut self, table: &[TeamId], champion: TeamId) {
        let n = self.teams.len();
        let season = self.season;
        let seed = self.econ.seed ^ 0xB0A2_D000 ^ ((season as u64) << 24);
        for club in 0..n {
            let place = table.iter().position(|&t| t == club).map_or(n, |p| p + 1);
            let e = self.expected_finish(club);
            let (budget, wage_over) = (
                self.teams[club].budget,
                self.econ.books.get(club).is_some_and(|b| b.wage_budget > 0) && self.wage_left(club) < 0,
            );
            let mut rng = Rng::new(seed ^ ((club as u64) << 8));
            let grace = self.first_season_grace(club);
            let b = &mut self.club.clubs[club].board;
            let diff = e as i32 - place as i32;
            let mut swing = diff as f32 * 3.0;
            if swing < 0.0 {
                swing *= harshness(b.patience()) * (0.7 + 0.03 * b.ambition() as f32);
            }
            if club == champion {
                swing += 8.0;
            } else if place <= 4 {
                swing += 2.0;
            }
            let text = if club == champion {
                format!("Delighted: champions. We were asked for {}.", crate::budget::ordinal(e))
            } else if diff >= 0 {
                format!(
                    "Pleased: finished {}, asked for {}.",
                    crate::budget::ordinal(place),
                    crate::budget::ordinal(e)
                )
            } else {
                format!(
                    "Disappointed: finished {}, asked for {}.",
                    crate::budget::ordinal(place),
                    crate::budget::ordinal(e)
                )
            };
            b.confidence = (b.confidence + swing).clamp(0.0, 100.0);
            b.push_reason(season, n as u16 * 2, swing, text);
            if budget < 0 || wage_over {
                let d = -2.0 * (21 - b.spending().min(20)) as f32 / 10.0;
                b.confidence = (b.confidence + d).max(0.0);
                b.push_reason(season, n as u16 * 2, d, "Worried about the finances.");
            }
            let line = sack_line(b.patience()) - grace;
            let sacked = (b.confidence < line || (b.confidence < line + 8.0 && b.warned))
                && place * 3 > n * 2 - 1;
            let leaves = rng.chance(0.05);
            // Summer cools tempers.
            b.confidence = 50.0 + (b.confidence - 50.0) * 0.6;
            b.warned = false;
            b.losing_run = 0;
            b.winning_run = 0;
            if self.watch == Some(club) {
                if sacked {
                    self.club.pending = Some(
                        "The season ended below what the board asked, and it has run out of patience."
                            .to_string(),
                    );
                }
            } else if sacked || leaves {
                let why = if sacked {
                    "dismissed after a poor season"
                } else {
                    "left for a job abroad"
                };
                self.manager_out(club, why);
            }
        }
        self.front_office_changes();
        self.career_close_season(table, champion);
        if let Some(me) = self.watch {
            self.desk.expectation = self.board_target(me);
        }
    }

    /// Owners sell. Chairmen and general managers are replaced. The user
    /// hears of it when it is his club.
    fn front_office_changes(&mut self) {
        let season = self.season;
        let n = self.teams.len();
        for club in 0..n {
            let mut rng = Rng::new(self.econ.seed ^ 0xF0F1_C000 ^ ((season as u64) << 24) ^ club as u64);
            let name = self.teams[club].name.clone();
            let debt = self.teams[club].budget < 0;
            let conf = self.club.clubs[club].board.confidence;
            let mut news: Vec<String> = Vec::new();
            let sells = rng.chance(if debt { 0.09 } else { 0.04 });
            let new_chair = rng.chance(if sells { 0.3 } else { 0.05 });
            let new_gm = rng.chance(if conf < 40.0 { 0.2 } else { 0.06 });
            let b = &mut self.club.clubs[club].board;
            if sells {
                let old = b.owner.name.clone();
                b.owner = Person::generate(&mut rng, 40, season + 1);
                news.push(format!(
                    "{old} has sold {name}. The new owner is {}. {}",
                    b.owner.name, b.owner.background
                ));
            }
            if new_chair && b.chairman.is_some() || (sells && rng.chance(0.3)) {
                let new = Person::generate(&mut rng, 40, season + 1);
                let old = b.chairman.replace(new.clone());
                news.push(match old {
                    Some(o) => format!("{} replaces {} as chairman of {name}.", new.name, o.name),
                    None => format!("{name} has appointed {} as chairman.", new.name),
                });
            }
            if new_gm {
                let mut gm = FrontOffice::generate(
                    self.econ.seed ^ 0x6A0_0000 ^ ((season as u64) << 16),
                    club,
                    &FIRST_NAMES,
                    &LAST_NAMES,
                )
                .gm;
                let office = &mut self.teams[club].office;
                std::mem::swap(&mut office.gm, &mut gm);
                news.push(format!(
                    "{} is the new general manager of {name}. {} has left.",
                    office.gm.name, gm.name
                ));
            }
            if !news.is_empty() && self.watch.is_some() {
                let rival = self.club.clubs[self.watch.unwrap()].identity.rival == club;
                if Some(club) == self.watch || rival || news.len() > 1 {
                    let mut body = news.join("\n\n");
                    if Some(club) == self.watch {
                        body += "\n\nNew people have their own ways. See the Club page.";
                    }
                    let subject = if sells { format!("New owner at {name}") } else { format!("Changes at {name}") };
                    let from = if Some(club) == self.watch { "The board" } else { "League office" };
                    self.post(0, Kind::Board, from, subject, body, vec![]);
                }
            }
        }
    }

    /// Before the new season: the dismissal the board decided on, if any.
    pub(crate) fn club_open_season(&mut self) {
        if let Some(why) = self.club.pending.take()
            && self.watch.is_some()
        {
            self.manager_dismissed(&why);
        }
    }

    /// The chairman's (or the owner's) first message to a new manager.
    pub(crate) fn club_welcome(&mut self, club: TeamId, practical: &str) {
        let n = self.teams.len();
        let name = self.teams[club].name.clone();
        let expect = self.desk.expectation.max(1);
        let id = self.identity(club).clone();
        let honours = self.honours(club);
        let b = &mut self.club.clubs[club].board;
        b.reasons.clear();
        b.warned = false;
        b.losing_run = 0;
        b.winning_run = 0;
        let face = self.club.clubs[club].board.face().clone();
        let mut body = format!("Welcome to {name}. I am {}.\n\n", face.name);
        body += &format!(
            "The club was founded in {}. {} We have won the title {} time{}.\n\n",
            id.founded,
            id.origin,
            honours.titles.len(),
            if honours.titles.len() == 1 { "" } else { "s" }
        );
        body += &face.greeting(&name, expect, n).join(" ");
        body += &format!("\n\n{}\n\n{practical}", self.fans_expect(club));
        self.post(
            self.day,
            Kind::Board,
            &face.name,
            "Welcome to the club",
            body,
            vec![],
        );
    }

    /// How much more or less than the usual the owner lets the budgets
    /// breathe, as a fraction of the wage bill.
    pub(crate) fn owner_room(&self, club: TeamId) -> f32 {
        (self.club.clubs[club].board.spending() as f32 - 10.0) * 0.004
    }

    /// Percent of the board's first budget that cannot be moved away.
    pub(crate) fn keep_pct(&self, club: TeamId) -> i64 {
        let s = self.club.clubs[club].board.spending() as i64;
        (crate::budget::KEEP + (10 - s) * 2).clamp(25, 75)
    }
}
