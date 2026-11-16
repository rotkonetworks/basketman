//! What the assistant tells the coach when play stops: three or four plain
//! lines from the real numbers since the last stoppage. Pure arithmetic over
//! the recording (see `game_flow::Read`), so it is the same for the screen
//! and for an agent.

use crate::court::ShotKind;
use crate::engine::Orders;
use crate::game_flow::{Heat, Read};
use crate::player::PlayerId;
use crate::replay::EventKind;

/// What the briefing needs beyond the recording.
pub struct Setting<'a> {
    pub name: &'a dyn Fn(PlayerId) -> String,
    /// Replay time of the previous stoppage (0 for the tip-off).
    pub since: f32,
    pub foul_limit: u16,
    /// What the other bench is running now.
    pub theirs: Orders,
}

/// How a side has scored and shot since a moment.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Stretch {
    pub points: u16,
    /// (made, attempted) at the rim, from mid-range and from three.
    pub rim: (u16, u16),
    pub mid: (u16, u16),
    pub three: (u16, u16),
}

/// Both sides' scoring since `since`, index by side.
pub fn stretch(read: &Read, since: f32) -> [Stretch; 2] {
    let mut out = [Stretch::default(); 2];
    let upto = read.upto();
    for e in read.rec.events[..upto].iter().filter(|e| e.time > since) {
        match &e.kind {
            EventKind::Shot {
                who,
                kind,
                made,
                fouled,
                ..
            } => {
                let Some(s) = read.side_of(*who) else { continue };
                if *fouled && !made {
                    continue;
                }
                let line = match kind {
                    ShotKind::Rim => &mut out[s].rim,
                    ShotKind::Mid => &mut out[s].mid,
                    ShotKind::Three => &mut out[s].three,
                };
                line.1 += 1;
                if *made {
                    line.0 += 1;
                    out[s].points += kind.points();
                }
            }
            EventKind::FreeThrows { who, made, .. } => {
                if let Some(s) = read.side_of(*who) {
                    out[s].points += *made as u16;
                }
            }
            _ => {}
        }
    }
    out
}

/// Three or four lines about how the game stands. `read.me` is the coach's
/// side.
pub fn briefing(read: &Read, set: &Setting) -> Vec<String> {
    let me = read.me;
    let them = 1 - me;
    let mut lines = Vec::new();
    let st = stretch(read, set.since);
    let flow = read.flow();

    // The score since we last stopped, and a run if there is one going.
    let mut first = format!(
        "Since the last stop it is {} to {} for us.",
        st[me].points, st[them].points
    );
    if let Some(run) = flow.runs.last().filter(|r| r.on) {
        let ours = run.side as usize == me;
        first = format!(
            "{} are on a {}-{} run.{}",
            if ours { "We" } else { "They" },
            run.points,
            run.against,
            if ours { "" } else { " Time to stop it." }
        );
    }
    lines.push(first);

    // What they score from, if it is worth saying.
    let t = st[them];
    let share = |k: (u16, u16)| if k.1 == 0 { 0.0 } else { k.0 as f32 / k.1 as f32 };
    let all = t.rim.1 + t.mid.1 + t.three.1;
    if t.three.1 >= 4 && share(t.three) >= 0.5 {
        lines.push(format!(
            "They are hitting from outside: {} of {} threes.",
            t.three.0, t.three.1
        ));
    } else if t.rim.1 >= 4 && share(t.rim) >= 0.6 {
        lines.push(format!(
            "They are getting to the rim: {} of {} there.",
            t.rim.0, t.rim.1
        ));
    } else if all >= 6 {
        let (m, a) = (t.rim.0 + t.mid.0 + t.three.0, all);
        lines.push(format!(
            "They shot {m} of {a} since the last stop. They run {} and defend in {}.",
            set.theirs.offense.label().to_lowercase(),
            set.theirs.defense.label().to_lowercase()
        ));
    }

    // Who is hot, cold, tired or in trouble.
    let forms = read.forms();
    let mut people: Vec<String> = Vec::new();
    for f in forms.iter().filter(|f| f.side as usize == them) {
        if f.heat == Some(Heat::Hot) && f.on_court != Some(false) {
            let made = f.last.iter().filter(|&&m| m).count();
            people.push(format!(
                "{} is hot, {} of his last {}.",
                (set.name)(f.id),
                made,
                f.last.len()
            ));
        }
    }
    for f in forms.iter().filter(|f| f.side as usize == me) {
        let on = f.on_court == Some(true);
        if on && f.energy.is_some_and(|e| e < 0.5) {
            people.push(format!(
                "{} is tired, {:.0}% left.",
                (set.name)(f.id),
                f.energy.unwrap_or(0.0) * 100.0
            ));
        } else if on && f.fouls + 1 >= set.foul_limit.max(2) {
            people.push(format!(
                "{} has {} fouls: one more and he is out.",
                (set.name)(f.id),
                f.fouls
            ));
        } else if on && f.heat == Some(Heat::Cold) {
            people.push(format!("{} is cold.", (set.name)(f.id)));
        } else if on && f.heat == Some(Heat::Hot) {
            people.push(format!("{} is hot.", (set.name)(f.id)));
        }
    }
    lines.extend(people.into_iter().take(2));

    // A man who is being beaten.
    if lines.len() < 4 {
        for m in read.matchups() {
            if m.verdict() == Some("getting hurt")
                && let Some(g) = m.guards
            {
                lines.push(format!(
                    "{} is getting hurt guarding {}.",
                    (set.name)(m.mine),
                    (set.name)(g)
                ));
                break;
            }
        }
    }
    if lines.len() == 1 {
        lines.push("Nothing else stands out.".to_string());
    }
    lines.truncate(4);
    lines
}
