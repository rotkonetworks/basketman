//! The assistant coach speaks up: short observations about the game as it
//! stands, each from the numbers, each with a thing to do about it where
//! there is one. Rate limited, and never the same thing twice in a row.

use serde::Serialize;

use crate::engine::{Orders, Shout};
use crate::game_flow::{Heat, Read};
use crate::league::LiveInput;
use crate::player::PlayerId;
use crate::replay::{EventKind, Source};
use crate::shotchart::{self, Zone};
use crate::tactics::DefenseScheme;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Topic {
    Tired,
    FoulTrouble,
    Run,
    Streak,
    GivingUpThrees,
    OpenCorner,
    Turnovers,
    Hot,
    Cold,
}

/// What a note proposes, in terms of the live inputs.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Suggestion {
    pub label: String,
    pub input: SuggestedInput,
}

/// A live input the game can build on its own; changes of defense need the
/// squad's familiarity, which only the world knows, so they are named.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum SuggestedInput {
    Timeout,
    Swap { off: PlayerId, on: PlayerId },
    Defense(DefenseScheme),
    Shout { shout: Shout, target: Option<PlayerId> },
}

impl SuggestedInput {
    /// The input for it. `orders` are the side's current ones and
    /// `familiarity` the squad's with a change of defense.
    pub fn input(&self, orders: Orders, familiarity: f32) -> LiveInput {
        match self {
            SuggestedInput::Timeout => LiveInput::Timeout,
            SuggestedInput::Swap { off, on } => LiveInput::Swap { off: *off, on: *on },
            SuggestedInput::Shout { shout, target } => LiveInput::Shout {
                shout: *shout,
                target: *target,
            },
            SuggestedInput::Defense(d) => LiveInput::Orders {
                orders: Orders {
                    defense: *d,
                    ..orders
                },
                familiarity,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Note {
    pub topic: Topic,
    pub subject: Option<PlayerId>,
    /// Replay time it was said.
    pub time: f32,
    pub period: u8,
    pub clock: f32,
    pub text: String,
    pub suggestion: Option<Suggestion>,
}

/// What the observer needs beyond the recording.
pub struct Setting<'a> {
    pub name: &'a dyn Fn(PlayerId) -> String,
    /// Both sides' orders; index by side.
    pub orders: [Orders; 2],
    pub timeouts_left: u8,
    pub timeout_wanted: bool,
    pub foul_limit: u16,
}

/// Remembers what was said, so as not to say it again.
#[derive(Clone, Debug, Default)]
pub struct Observer {
    /// Topic, subject, when, how strong it was.
    said: Vec<(Topic, Option<PlayerId>, f32, i32)>,
    last: f32,
    pub count: usize,
}

/// Seconds between any two notes, and before the same one is repeated.
const GAP: f32 = 60.0;
const URGENT_GAP: f32 = 20.0;
const REPEAT: f32 = 240.0;
/// Streaks of one shooter are only worth saying once in a long while.
const REPEAT_SHOOTER: f32 = 480.0;
const FIRST: f32 = 120.0;
const MAX_NOTES: usize = 12;

struct Found {
    topic: Topic,
    subject: Option<PlayerId>,
    level: i32,
    urgent: bool,
    text: String,
    suggestion: Option<Suggestion>,
}

impl Observer {
    /// At most one new note for the game as it stands.
    pub fn observe(&mut self, read: &Read, set: &Setting) -> Option<Note> {
        let now = read.time;
        if self.count >= MAX_NOTES || now < FIRST {
            return None;
        }
        let frame = read.rec.frames.get(read.rec.frame_at(now))?;
        let mut found = candidates(read, set, frame.period);
        found.sort_by_key(|f| (!f.urgent, f.topic, std::cmp::Reverse(f.level)));
        for f in found {
            let gap = if f.urgent { URGENT_GAP } else { GAP };
            if now - self.last < gap && self.count > 0 {
                continue;
            }
            let prior = self
                .said
                .iter()
                .filter(|s| s.0 == f.topic && s.1 == f.subject)
                .max_by(|a, b| a.2.total_cmp(&b.2));
            if let Some(&(_, _, at, level)) = prior {
                // What stays true is said again only when it has got
                // clearly worse; what happens, when long enough has passed.
                let (more, wait) = match f.topic {
                    Topic::Tired => (3, GAP * 2.0),
                    Topic::FoulTrouble => (1, GAP * 2.0),
                    Topic::OpenCorner | Topic::GivingUpThrees => (2, REPEAT),
                    Topic::Hot | Topic::Cold => (6, REPEAT_SHOOTER),
                    Topic::Run | Topic::Streak | Topic::Turnovers => (0, REPEAT),
                };
                let fresh = if more == 0 { true } else { f.level >= level + more };
                if now - at < wait || !fresh {
                    continue;
                }
            }
            self.said.push((f.topic, f.subject, now, f.level));
            self.last = now;
            self.count += 1;
            return Some(Note {
                topic: f.topic,
                subject: f.subject,
                time: now,
                period: frame.period,
                clock: frame.clock,
                text: f.text,
                suggestion: f.suggestion,
            });
        }
        None
    }
}

fn candidates(read: &Read, set: &Setting, period: u8) -> Vec<Found> {
    let (me, them) = (read.me, 1 - read.me);
    let name = |p: PlayerId| (set.name)(p);
    let mut out = Vec::new();
    let forms = read.forms();
    let bench_best = |off: PlayerId| -> Option<PlayerId> {
        let live = read.live?;
        let mut bench: Vec<_> = live[me]
            .iter()
            .filter(|p| !p.on_court && !p.fouled_out && p.id != off)
            .collect();
        bench.sort_by(|a, b| b.energy.total_cmp(&a.energy).then(a.id.cmp(&b.id)));
        bench.first().map(|p| p.id)
    };
    let swap = |off: PlayerId| {
        bench_best(off).map(|on| Suggestion {
            label: format!("Sub {} off", name(off)),
            input: SuggestedInput::Swap { off, on },
        })
    };
    let timeout = || {
        (set.timeouts_left > 0 && !set.timeout_wanted).then(|| Suggestion {
            label: "Call timeout".into(),
            input: SuggestedInput::Timeout,
        })
    };

    // Exhausted players on the floor.
    if let Some(live) = read.live {
        for p in live[me].iter().filter(|p| p.on_court && p.energy < 0.3) {
            out.push(Found {
                topic: Topic::Tired,
                subject: Some(p.id),
                level: ((0.3 - p.energy) * 20.0) as i32 + 1,
                urgent: p.energy < 0.2,
                text: format!("{} is exhausted.", name(p.id)),
                suggestion: swap(p.id),
            });
        }
    }

    // Foul trouble: three in the first half, or one from the limit.
    for f in forms.iter().filter(|f| f.side as usize == me && f.on_court != Some(false)) {
        let trouble = f.fouls + 1 >= set.foul_limit || (f.fouls >= 3 && period < 2);
        if trouble && f.fouls > 0 {
            out.push(Found {
                topic: Topic::FoulTrouble,
                subject: Some(f.id),
                level: f.fouls as i32,
                urgent: f.fouls + 1 >= set.foul_limit,
                text: format!("{} has {} fouls.", name(f.id), word(f.fouls)),
                suggestion: if f.on_court == Some(true) { swap(f.id) } else { None },
            });
        }
    }

    // Runs.
    let flow = read.flow();
    if let Some(run) = flow.runs.iter().rev().find(|r| r.on) {
        let ours = run.side as usize == me;
        out.push(Found {
            topic: Topic::Run,
            subject: None,
            level: run.points as i32 / 2,
            urgent: !ours && run.points >= 10,
            text: format!(
                "{} on {} {}-{} run.",
                if ours { "We are" } else { "They are" },
                if matches!(run.points, 8 | 11 | 18) { "an" } else { "a" },
                run.points,
                run.against
            ),
            suggestion: if ours { None } else { timeout() },
        });
    }

    // Their scoring streak, possession by possession.
    let (streak, source) = streak(read, them);
    if streak >= 5 {
        let (phrase, inside) = match source {
            Some(Source::Drive) => (" off the dribble drive", true),
            Some(Source::Roll) => (" out of the pick and roll", true),
            Some(Source::Post) => (" in the post", true),
            Some(Source::Putback) => (" on putbacks", true),
            Some(Source::CatchAndShoot) => (" on catch-and-shoot looks", false),
            Some(Source::PullUp) => (" on pull-ups", false),
            Some(Source::Transition) => (" in transition", false),
            _ => ("", false),
        };
        let current = set.orders[me].defense;
        let change = if source.is_none() || matches!(source, Some(Source::Transition)) {
            None
        } else if inside {
            Some(DefenseScheme::PackLine)
        } else {
            Some(DefenseScheme::ManToMan)
        }
        .filter(|d| *d != current);
        out.push(Found {
            topic: Topic::Streak,
            subject: None,
            level: streak as i32,
            urgent: false,
            text: format!("They have scored on {} straight possessions{phrase}.", word(streak)),
            suggestion: change
                .map(|d| Suggestion {
                    label: format!("Switch to {}", d.label().to_lowercase()),
                    input: SuggestedInput::Defense(d),
                })
                .or_else(timeout),
        });
    }

    // The corner, open for us or for them.
    let ids = read.ids;
    let shots = shotchart::shots(read.rec, ids);
    let upto = read.upto();
    let shots: Vec<_> = shots.into_iter().filter(|s| s.event < upto).collect();
    for (side, mine) in [(me, true), (them, false)] {
        for zone in [Zone::CornerThree, Zone::AboveBreakThree] {
            let z = shotchart::zones(shots.iter().filter(|s| s.side as usize == side && s.zone == zone))
                [Zone::ALL.iter().position(|&x| x == zone).unwrap_or(0)];
            if z.att >= 5 && z.made * 10 >= z.att * 7 {
                let place = if zone == Zone::CornerThree { "the corner" } else { "above the break" };
                if mine {
                    let zoned = matches!(
                        set.orders[them].defense,
                        DefenseScheme::Zone23 | DefenseScheme::Zone32
                    );
                    out.push(Found {
                        topic: Topic::OpenCorner,
                        subject: None,
                        level: z.made as i32,
                        urgent: false,
                        text: format!(
                            "{} {} giving up {place}: {} of {} from there.",
                            if zoned { "Their zone is" } else { "They are" },
                            "",
                            z.made,
                            z.att
                        )
                        .replace("  ", " "),
                        suggestion: None,
                    });
                } else {
                    let current = set.orders[me].defense;
                    let change = match current {
                        DefenseScheme::ManToMan => None,
                        _ => Some(DefenseScheme::ManToMan),
                    };
                    out.push(Found {
                        topic: Topic::GivingUpThrees,
                        subject: None,
                        level: z.made as i32,
                        urgent: false,
                        text: format!("They are {} of {} from {place} against us.", z.made, z.att),
                        suggestion: change
                            .map(|d| Suggestion {
                                label: format!("Switch to {}", d.label().to_lowercase()),
                                input: SuggestedInput::Defense(d),
                            })
                            .or_else(timeout),
                    });
                }
            }
        }
    }

    // Shooters.
    for f in &forms {
        let on = f.on_court != Some(false);
        let (n, made) = (f.last.len(), f.last.iter().filter(|&&m| m).count());
        match f.heat {
            Some(Heat::Cold) if f.side as usize == me && on => out.push(Found {
                topic: Topic::Cold,
                subject: Some(f.id),
                level: f.attempts as i32,
                urgent: false,
                text: format!("{} has missed {} of his last {n}.", name(f.id), n - made),
                suggestion: Some(Suggestion {
                    label: format!("Encourage {}", name(f.id)),
                    input: SuggestedInput::Shout {
                        shout: Shout::Encourage,
                        target: Some(f.id),
                    },
                }),
            }),
            Some(Heat::Hot) if f.side as usize == them && on => out.push(Found {
                topic: Topic::Hot,
                subject: Some(f.id),
                level: f.attempts as i32,
                urgent: false,
                text: format!("{} has hit {made} of his last {n}.", name(f.id)),
                suggestion: Some(Suggestion {
                    label: "Shout: concentrate".into(),
                    input: SuggestedInput::Shout {
                        shout: Shout::Focus,
                        target: None,
                    },
                }),
            }),
            _ => {}
        }
    }

    // Our turnovers over the last five minutes.
    let tovs = read.rec.events[..upto]
        .iter()
        .filter(|e| read.time - e.time <= 300.0)
        .filter(|e| matches!(&e.kind, EventKind::Turnover { who, .. } if read.side_of(*who) == Some(me)))
        .count();
    if tovs >= 5 {
        out.push(Found {
            topic: Topic::Turnovers,
            subject: None,
            level: tovs as i32,
            urgent: false,
            text: format!("We have turned it over {tovs} times in five minutes."),
            suggestion: Some(Suggestion {
                label: "Shout: calm down".into(),
                input: SuggestedInput::Shout {
                    shout: Shout::CalmDown,
                    target: None,
                },
            }),
        });
    }
    out
}

/// How many possessions in a row `side` has scored on, and where most of
/// those scores came from. A possession ends in a make, a turnover, or a
/// miss the shooter's side does not get back; free throws that follow a
/// make are the same possession.
fn streak(read: &Read, side: usize) -> (u16, Option<Source>) {
    let events = &read.rec.events[..read.upto()];
    // Outcomes of `side`'s possessions, newest last: made with a source, or
    // not.
    let mut out: Vec<Option<Option<Source>>> = Vec::new();
    let mut n = 0;
    while n < events.len() {
        let e = &events[n];
        match &e.kind {
            EventKind::Shot {
                who,
                made,
                source,
                fouled,
                ..
            } if read.side_of(*who) == Some(side) => {
                if *made {
                    out.push(Some(Some(*source)));
                } else if !*fouled {
                    // Does he get it back?
                    let back = events[n + 1..]
                        .iter()
                        .find(|x| matches!(x.kind, EventKind::Rebound { .. }))
                        .is_some_and(|x| {
                            matches!(&x.kind, EventKind::Rebound { who, offensive: true }
                                if read.side_of(*who) == Some(side))
                        });
                    if !back {
                        out.push(None);
                    }
                }
            }
            EventKind::FreeThrows { who, made, .. }
                if read.side_of(*who) == Some(side) && *made > 0 =>
            {
                // After a make in the same moment the trip is already counted.
                let same = events[..n].iter().rev().take(3).any(|x| {
                    matches!(&x.kind, EventKind::Shot { made: true, who, .. } if read.side_of(*who) == Some(side))
                        && e.time - x.time < 3.0
                });
                if !same {
                    out.push(Some(None));
                }
            }
            EventKind::Turnover { who, .. } if read.side_of(*who) == Some(side) => out.push(None),
            _ => {}
        }
        n += 1;
    }
    let run: Vec<_> = out.iter().rev().take_while(|o| o.is_some()).collect();
    let mut tally: Vec<(Source, u16)> = Vec::new();
    for s in run.iter().filter_map(|o| o.as_ref().and_then(|s| *s)) {
        match tally.iter_mut().find(|t| t.0 == s) {
            Some(t) => t.1 += 1,
            None => tally.push((s, 1)),
        }
    }
    let top = tally.iter().max_by_key(|t| t.1).copied();
    let source = top.filter(|t| t.1 as usize * 10 >= run.len() * 6).map(|t| t.0);
    (run.len() as u16, source)
}

fn word(n: u16) -> String {
    match n {
        1 => "one".into(),
        2 => "two".into(),
        3 => "three".into(),
        4 => "four".into(),
        5 => "five".into(),
        6 => "six".into(),
        7 => "seven".into(),
        8 => "eight".into(),
        9 => "nine".into(),
        n => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::league::World;

    #[test]
    fn notes_come_from_the_game_and_are_rate_limited() {
        let mut w = World::new_career_tiers(5, 8, 3);
        w.apply(Command::TakeCharge(1)).unwrap();
        w.apply(Command::PlayRound).unwrap();
        let (i, rec) = w.replay.clone().unwrap();
        let r = w.fixtures[i].result.clone().unwrap();
        let ids = [0, 1].map(|s| r.boxes[s].iter().map(|b| b.0).collect::<Vec<_>>());
        let name = |p: PlayerId| w.players[p as usize].name.clone();
        let orders = Orders::of(w.teams[0].tactics());
        let set = Setting {
            name: &name,
            orders: [orders, orders],
            timeouts_left: 5,
            timeout_wanted: false,
            foul_limit: w.rules.foul_limit,
        };
        let mut obs = Observer::default();
        let mut notes: Vec<Note> = Vec::new();
        let mut t = 0.0;
        while t < rec.duration() {
            t += 2.0;
            let read = Read {
                rec: &rec,
                ids: &ids,
                me: 0,
                time: t,
                live: None,
            };
            notes.extend(obs.observe(&read, &set));
        }
        eprintln!("{} notes", notes.len());
        for n in &notes {
            eprintln!("{:6.0} P{} {:5.0}  {}", n.time, n.period + 1, n.clock, n.text);
        }
        assert!(notes.len() <= MAX_NOTES);
        assert!(notes.windows(2).all(|p| p[1].time - p[0].time >= URGENT_GAP));
        // The same thing about the same man is not said twice in a row.
        for p in notes.windows(2) {
            assert!(
                !(p[0].topic == p[1].topic && p[0].subject == p[1].subject)
                    || p[1].time - p[0].time >= GAP * 2.0
            );
        }
        // Said again, it is silent.
        let read = Read {
            rec: &rec,
            ids: &ids,
            me: 0,
            time: t,
            live: None,
        };
        let mut again = Observer::default();
        let a = again.observe(&read, &set);
        let b = again.observe(&read, &set);
        assert!(b.is_none() || a.is_none() || b.unwrap().topic != a.unwrap().topic);
    }
}
