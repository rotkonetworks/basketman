use super::simulate_recorded;
use crate::court::{HOOP, v};
use crate::league::{World, lineup};
use crate::replay::{Action, ShotStage, to_court};
use crate::rng::Rng;

#[test]
#[ignore]
fn diag_release_facing() {
    let w = World::generate(7, 12);
    let mut hist = [0usize; 5];
    let mut hist_raw = [0usize; 5];
    let mut total = 0;
    let mut gather_len = Vec::new();
    for g in 0..8 {
        let a = g % 12;
        let b = (g * 5 + 1) % 12;
        let hl = lineup(&w.teams[a], &w.players);
        let al = lineup(&w.teams[b], &w.players);
        let rec = simulate_recorded(&hl, &al, &w.rules, &mut Rng::new(300 + g as u64)).1;
        let mut face = [[v(1.0, 0.0); 5]; 2];
        let mut last = [[false; 5]; 2];
        let mut gstart = [[0.0f32; 5]; 2];
        for i in 1..rec.frames.len() {
            let (pf, f) = (&rec.frames[i - 1], &rec.frames[i]);
            let o = f.offense as usize;
            let rim = to_court(HOOP, o);
            for s in 0..2 { for k in 0..5 {
                let p = &f.pose[s][k];
                let vel = (f.pos[s][k] - pf.pos[s][k]) * (1.0 / (f.time - pf.time).max(0.01));
                let spd = vel.len();
                let mut t = if spd > 0.01 { vel.norm() } else { v(0.0, 0.0) };
                // ball dir when standing
                if spd < 1.6 { let tb = f.ball - f.pos[s][k]; if tb.len() > 0.01 { t = t + tb.norm() * (1.0 - spd / 1.6); } }
                if let Some(fc) = p.face() { t = fc; }
                if t.len() < 0.001 { t = v(1.0, 0.0); }
                let t = t.norm();
                let kf = if spd > 0.5 { 0.33 } else { 0.16 };
                let m = face[s][k] * (1.0 - kf) + t * kf;
                face[s][k] = if m.len() < 0.05 { t } else { m.norm() };
                let rel = p.action == Action::Shooting && p.stage == ShotStage::Release;
                if s == o && rel && !last[s][k] {
                    let to = (rim - f.pos[s][k]).norm();
                    let bin = |d: f32| if d > 0.9 { 0 } else if d > 0.7 { 1 } else if d > 0.0 { 2 } else if d > -0.7 { 3 } else { 4 };
                    if (rim - f.pos[s][k]).len() > 1.0 {
                        hist[bin(face[s][k].dot(to))] += 1;
                        hist_raw[bin(t.dot(to))] += 1;
                        total += 1;
                        gather_len.push(f.time - gstart[s][k]);
                    }
                }
                last[s][k] = rel;
                if p.action == Action::Shooting && p.stage == ShotStage::Gather && p.progress < 15 { gstart[s][k] = f.time; }
            }}
        }
    }
    gather_len.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("drawn (smoothed) cos-to-rim bins >.9,>.7,>0,>-.7,rest: {hist:?} of {total}; recorded-only: {hist_raw:?}; gather->release median {:.2}", gather_len[gather_len.len()/2]);
}
