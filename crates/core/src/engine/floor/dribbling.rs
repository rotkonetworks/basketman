//! The dribble is an exposed ball. While a man dribbles the ball leaves his
//! hand and comes back on a rhythm; how far out it goes, and for how long,
//! is his handling (and his weaker hand). A defender whose hand gets to it
//! in that window pokes it loose or takes it; one who reaches and misses is
//! out of position, and one who hits the arm has fouled.

use super::*;
use crate::engine::hands;
use crate::replay::{Dribble, TurnoverKind};

/// Metres from the ball within which a hand has touched it.
const POKE_HIT: f32 = 0.20;
/// How likely a defender within reach goes for the ball in a bounce, at
/// rating 10, and how it moves with his steal rating and aggression.
const GO_BASE: f32 = 0.20;
const GO_SKILL: f32 = 0.02;
const GO_AGGRESSION: f32 = 0.10;
/// The chance a touch takes the ball cleanly, at equal ratings.
const TAKE_BASE: f32 = 0.22;
/// A hand reaching and missing costs this much balance, and this long frozen.
const MISS_BALANCE: f32 = 0.10;
const MISS_STUN: f32 = 0.35;
/// Chance a touch is on the arm and not the ball, at rating 10 handling.
const ARM_BASE: f32 = 0.07;

#[derive(Copy, Clone)]
pub(super) struct Poke {
    /// Which bounce these tries belong to.
    pub cycle: i32,
    /// Each defender's aim error for this bounce, if he goes for it.
    pub tries: [Option<crate::engine::ball::V3>; 5],
    /// The nearest each hand has come to the ball this bounce.
    pub close: [f32; 5],
}

impl Poke {
    pub const NONE: Poke = Poke { cycle: -1, tries: [None; 5], close: [9.0; 5] };
}

impl Game {
    /// One tick of the dribble: who goes for the ball, and whether a hand
    /// gets to it. Returns the end of the play if the ball is lost.
    pub(super) fn dribble_tick(&mut self) -> Option<End> {
        let o = self.play.o;
        let Some((t, h)) = self.handler.who else { return None };
        if t != o
            || self.handler.state != Dribble::Live
            || self.play.holder != Some(h)
            || self.play.dead > 0.0
            || self.play.throw.is_some()
            || self.play.shot.is_some_and(|s| self.play.t >= s.start)
            || matches!(self.play.phase, Phase::Gather { .. } | Phase::Air | Phase::Line { .. })
        {
            self.poke = Poke::NONE;
            return None;
        }
        let me = self.m[o][h].pos;
        // A new bounce: who goes for it.
        let speed = self.m[o][h].vel.len().min(7.0);
        let period = (0.64 - 0.025 * speed).max(0.38);
        let cycle = (self.anim / period).floor() as i32;
        if cycle != self.poke.cycle {
            self.poke.cycle = cycle;
            self.poke.tries = [None; 5];
            self.poke.close = [9.0; 5];
            for dk in 0..5 {
                if self.play.stun[dk] > 0.0 {
                    continue;
                }
                let q = self.dp(dk);
                let (arm, _, _) = self.arm_of(1 - o, dk);
                let reach = arm + 0.35 + 0.3;
                if q.dist(me) > reach + 0.35 {
                    continue;
                }
                let steal = self.de(dk, Attr::Steal);
                let p = (GO_BASE + GO_SKILL * (steal - 10.0) + GO_AGGRESSION * self.def().aggression()).clamp(0.02, 0.6)
                    * (1.3 - 0.5 * (q.dist(me) / (reach + 0.35)))
                    * (1.0 + 0.5 * (1.0 - self.oe(h, Attr::Handling) / 20.0));
                if self.rng.chance(p) {
                    let sig = 0.17 * (1.6 - steal / 20.0 * 1.1).max(0.4);
                    self.poke.tries[dk] = Some(crate::engine::ball::v3(
                        shot::gauss(&mut self.rng) * sig,
                        shot::gauss(&mut self.rng) * sig,
                        shot::gauss(&mut self.rng) * sig * 0.8,
                    ));
                }
            }
        }
        // The hands against the ball, in fine steps through the tick.
        let steps = 5;
        for s in 1..=steps {
            let u = s as f32 / steps as f32;
            let bt = self.bounce_at(o, h, self.anim - DT * (1.0 - u));
            if !bt.exposed {
                continue;
            }
            let ball = crate::engine::ball::v3(bt.at.x, bt.at.y, bt.z);
            for dk in 0..5 {
                if self.poke.tries[dk].is_none() {
                    continue;
                }
                for i in 0..2 {
                    let h0 = self.hands_prev[1 - o][dk].h[i].p;
                    let h1 = self.hands[1 - o][dk].h[i].p;
                    let hp = crate::engine::ball::v3(
                        h0.x + (h1.x - h0.x) * u,
                        h0.y + (h1.y - h0.y) * u,
                        h0.z + (h1.z - h0.z) * u,
                    );
                    let d = (crate::engine::ball::V3 { x: hp.x - ball.x, y: hp.y - ball.y, z: hp.z - ball.z }).len();
                    self.poke.close[dk] = self.poke.close[dk].min(d);
                    if d <= POKE_HIT {
                        self.poke.tries[dk] = None;
                        return self.touch_dribble(h, dk, d, bt);
                    }
                }
            }
        }
        // A reach that is over and has found nothing: he has lunged.
        let exposed_now = self.bounce(o, h).exposed;
        if !exposed_now {
            for dk in 0..5 {
                if self.poke.tries[dk].take().is_some() && self.poke.cycle == cycle {
                    // Only a try that was pressed home costs him.
                    let q = self.dp(dk);
                    if q.dist(me) < 1.6 && self.poke.close[dk] < 0.5 {
                        self.st[1 - o][dk].balance = (self.st[1 - o][dk].balance - MISS_BALANCE).max(0.2);
                        self.play.stun[dk] = self.play.stun[dk].max(MISS_STUN * self.rng.f32());
                        // Reaching in with the hand past the ball: the arm.
                        let temper = 1.5 - self.sides[1 - o].slots[self.sides[1 - o].on[dk]].p.personality.temperament as f32 / 20.0;
                        let raw = 0.05 * temper * self.def().foul_mult(self.def().on[dk]) * self.off().draw_mult(self.off().on[h]) / CALL_BASE;
                        let hit = self.rng.chance(raw.min(0.9));
                        if self.call(hit, 1.0, FoulKind::ReachIn, (1 - o, dk), (o, h)) {
                            return self.floor_foul(dk, h, FoulKind::ReachIn);
                        }
                    }
                }
            }
        }
        None
    }

    /// A defender's hand is on the dribbled ball.
    fn touch_dribble(&mut self, h: usize, dk: usize, gap: f32, bt: super::hands_hooks::Bounce) -> Option<End> {
        let o = self.play.o;
        let steal = self.de(dk, Attr::Steal);
        let hands = self.de(dk, Attr::Handling);
        let handling = self.oe(h, Attr::Handling);
        let strength = self.oe(h, Attr::Strength);
        // On the arm and not the ball: a foul, if it is seen.
        let on_arm = ARM_BASE * (1.0 + (10.0 - handling) * 0.03) * (1.0 + 0.4 * self.def().aggression());
        let arm_hit = self.rng.chance(on_arm.clamp(0.02, 0.5) * (gap / POKE_HIT + 0.4));
        if arm_hit {
            let temper = 1.5 - self.sides[1 - o].slots[self.sides[1 - o].on[dk]].p.personality.temperament as f32 / 20.0;
            let raw = temper * self.def().foul_mult(self.def().on[dk]) * self.off().draw_mult(self.off().on[h]) / CALL_BASE;
            let hit = self.rng.chance(raw.min(0.95));
            if self.call(hit, 1.0, FoulKind::ReachIn, (1 - o, dk), (o, h)) {
                return self.floor_foul(dk, h, FoulKind::ReachIn);
            }
            return None;
        }
        // Taken cleanly, or knocked away.
        let take = (TAKE_BASE + 0.02 * (0.6 * steal + 0.4 * hands - 0.7 * handling - 0.3 * strength) + 0.15 * (1.0 - gap / POKE_HIT)).clamp(0.05, 0.8);
        if self.rng.chance(take) {
            self.steal_flew = 0.0;
            self.play.ball = bt.at;
            self.play.ball_h = bt.z;
            return self.turnover(h, TurnoverKind::Stripped, Some(dk), 1.0);
        }
        // Poked loose: away from the man and out the way the hand pushed.
        let away = (bt.at - self.m[o][h].pos).norm();
        let dir = if away.len() < 0.2 { self.st[o][h].body } else { away };
        let kick = dir * (2.0 + self.rng.f32() * 2.0) + self.m[o][h].vel * 0.4;
        let mut np = Path::new();
        np.fall_open(bt.at, bt.z.max(BALL_RADIUS + 0.02), -0.5, kick);
        self.play.holder = None;
        self.handler.who = None;
        self.handler.state = Dribble::None;
        self.play.def_touch = true;
        self.play.scramble = Some(Scramble { passer: h, to: h, tipped_by: Some(dk), kind: crate::replay::PassKind::Chest, cause: TurnoverKind::Stripped });
        self.play.ball = bt.at;
        self.play.ball_h = bt.z;
        self.go_live(np, 0.0, false)
    }
}

#[allow(dead_code)]
fn _hands_unused() -> usize {
    hands::LEFT
}
