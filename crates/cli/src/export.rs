//! `basketman --export-game <seed> <out-file>`: simulate one recorded game
//! and write it in the line-based text format read by `viewer3d/match.psh`.
//! See `viewer3d/README.md` for the format.

use std::fmt::Write;

use basketman_core::engine::simulate_recorded;
use basketman_core::league::{World, lineup};
use basketman_core::player::{Attr, Hand};
use basketman_core::replay::FRAME_RATE;
use basketman_core::rng::Rng;

fn cm(m: f32) -> i32 {
    (m * 100.0).round() as i32
}

pub fn export_game(seed: u64, path: &str) -> std::io::Result<()> {
    let w = World::generate(seed, 12);
    let f = &w.fixtures[0];
    let (home, away) = (f.home, f.away);
    let hl = lineup(&w.teams[home], &w.players);
    let al = lineup(&w.teams[away], &w.players);
    let (result, rec) = simulate_recorded(&hl, &al, &w.rules, &mut Rng::new(seed));

    // Player table: box score order, home first. Frames refer to it by index.
    let mut table: Vec<(usize, u32)> = Vec::new();
    for side in 0..2 {
        for (pid, _) in &result.boxes[side] {
            table.push((side, *pid as u32));
        }
    }
    let name = |pid: u32| w.players[pid as usize].name.as_str();

    let mut o = String::new();
    writeln!(o, "BMREPLAY 2").unwrap();
    // The file format keeps five frames a second: every other recorded frame
    // goes in (and the first after a cut).
    let keep = (FRAME_RATE / 5.0).round().max(1.0) as usize;
    let frames: Vec<_> = rec
        .frames
        .iter()
        .enumerate()
        .filter(|(i, f)| i % keep == 0 || f.cut)
        .map(|(_, f)| f)
        .collect();
    writeln!(o, "rate 5").unwrap();
    writeln!(o, "teams {}|{}", w.teams[home].name, w.teams[away].name).unwrap();
    writeln!(o, "players {}", table.len()).unwrap();
    // Player table, one line per player:
    //   side|name|id|height cm|shoulder cm|weight kg|wingspan cm|reach cm|hand cm|hand|speed|vertical|strength|stamina|handling
    // hand is 0 right, 1 left; attributes are the 1..20 rating times ten.
    for (side, pid) in &table {
        let pl = &w.players[*pid as usize];
        let b = &pl.body;
        let at = |a: Attr| (pl.attrs[a] * 10.0).round() as i32;
        writeln!(
            o,
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            side,
            name(*pid),
            pid,
            b.height_cm.round() as i32,
            b.shoulder_width_cm.round() as i32,
            b.weight_kg.round() as i32,
            b.wingspan_cm.round() as i32,
            b.standing_reach_cm.round() as i32,
            b.hand_length_cm.round() as i32,
            if b.hand == Hand::Left { 1 } else { 0 },
            at(Attr::Speed),
            at(Attr::Vertical),
            at(Attr::Strength),
            at(Attr::Stamina),
            at(Attr::Handling),
        )
        .unwrap();
    }
    writeln!(o, "frames {}", frames.len()).unwrap();

    // Frame lines. Positions are in cm, as second differences: every frame
    // line is "a holder" and then 23 numbers (ball x, ball y, ball height,
    // then ten players x y), each the change in that value's velocity.
    // `a` is the frame time step in cs minus 20, `holder` the slot 0..9 of
    // the ball carrier or -1. Extra lines before a frame:
    //   w i0..i9     who is on the floor now (indices into the player table)
    //   m per s0 s1 off    period (0 based), score, side with the ball
    //   c clock_cs   game clock, only when it is not the previous one minus
    //                the time step
    let mut prev_who: Option<[usize; 10]> = None;
    let mut prev = [0i32; 23];
    let mut vel = [0i32; 23];
    let mut prev_meta = None;
    let mut prev_time = 0;
    let mut prev_clock = 0;
    for fr in frames {
        let mut who = [0usize; 10];
        for side in 0..2 {
            for k in 0..5 {
                who[side * 5 + k] = table_index(&result, side, fr.who[side][k]);
            }
        }
        if prev_who != Some(who) {
            let s: Vec<String> = who.iter().map(|i| i.to_string()).collect();
            writeln!(o, "w {}", s.join(" ")).unwrap();
            prev_who = Some(who);
        }
        let meta = (fr.period, fr.score, fr.offense);
        if prev_meta != Some(meta) {
            writeln!(
                o,
                "m {} {} {} {}",
                fr.period, fr.score[0], fr.score[1], fr.offense
            )
            .unwrap();
            prev_meta = Some(meta);
        }
        let time = cm(fr.time);
        let dt = time - prev_time;
        let clock = cm(fr.clock);
        if clock != prev_clock - dt {
            writeln!(o, "c {}", clock).unwrap();
        }
        prev_time = time;
        prev_clock = clock;
        let holder = match fr.holder {
            Some((s, k)) => (s as usize * 5 + k as usize) as i32,
            None => -1,
        };
        let mut cur = [0i32; 23];
        cur[0] = cm(fr.ball.x);
        cur[1] = cm(fr.ball.y);
        cur[2] = cm(fr.ball_height);
        for side in 0..2 {
            for k in 0..5 {
                let i = 3 + (side * 5 + k) * 2;
                cur[i] = cm(fr.pos[side][k].x);
                cur[i + 1] = cm(fr.pos[side][k].y);
            }
        }
        write!(o, "{} {}", dt - 20, holder).unwrap();
        for i in 0..23 {
            let v = cur[i] - prev[i];
            write!(o, " {}", v - vel[i]).unwrap();
            vel[i] = v;
        }
        o.push('\n');
        prev = cur;
    }

    writeln!(o, "events {}", rec.events.len()).unwrap();
    for e in &rec.events {
        let t = e.text(|p| name(p as u32));
        writeln!(o, "{}|{}", cm(e.time), t.replace(['\n', '|'], " ")).unwrap();
    }
    std::fs::write(path, o)
}

fn table_index(r: &basketman_core::engine::GameResult, side: usize, k: u8) -> usize {
    (0..side).map(|s| r.boxes[s].len()).sum::<usize>() + k as usize
}
