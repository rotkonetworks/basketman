//! Where an offensive set wants its five players.
//!
//! Each set is a template of five spots. Slot 0 belongs to the `lead` (the
//! ball handler, or the star in an isolation), slot 4 to the `big` (post
//! player or screener); the other three are filled in position order.
//! `flip` mirrors the set so it is not always run to the same side.

use crate::court::{Vec2, v};
use crate::tactics::{Board, OffenseSet};

const CORNER: Vec2 = v(6.95, 1.0);

pub(crate) fn template(set: OffenseSet) -> [Vec2; 5] {
    match set {
        // Four out, one in: two slots, two wings, a post on the ball side.
        OffenseSet::Motion => [
            v(2.9, 8.3),
            v(6.3, 4.9),
            v(-2.9, 8.3),
            v(-6.3, 4.9),
            v(2.6, 2.9),
        ],
        // Spread pick and roll: shooters in both corners and on a wing, the
        // screener waiting above the ball.
        OffenseSet::PickAndRoll => [
            v(0.0, 8.9),
            CORNER,
            v(-6.4, 5.0),
            v(-CORNER.x, CORNER.y),
            v(1.6, 7.2),
        ],
        // Entry from the wing to the block, high post opposite.
        OffenseSet::PostUp => [
            v(6.3, 5.0),
            v(0.3, 8.7),
            v(-6.3, 4.8),
            v(-2.4, 5.7),
            v(2.7, 2.9),
        ],
        // The star gets a side to himself, everyone else flattens out.
        OffenseSet::Isolation => [v(-4.2, 7.6), v(3.4, 8.4), v(6.5, 4.6), CORNER, v(2.1, 1.3)],
        OffenseSet::FiveOut => [
            v(0.0, 8.9),
            v(6.3, 5.2),
            v(-6.3, 5.2),
            CORNER,
            v(-CORNER.x, CORNER.y),
        ],
    }
}

/// Which place of a set each of the five on the floor takes: the lead the
/// first, the big the last, the rest in order.
pub(crate) fn places(lead: usize, big: usize) -> [usize; 5] {
    let mut out = [0; 5];
    let mut next = 1;
    for (k, place) in out.iter_mut().enumerate() {
        *place = if k == lead {
            0
        } else if k == big {
            4
        } else {
            next += 1;
            next - 1
        };
    }
    out
}

/// Where the five stand, and the run each has been drawn, by court index.
pub(super) fn spots(
    board: &Board,
    flip: f32,
    lead: usize,
    big: usize,
) -> ([Vec2; 5], [Option<Vec2>; 5]) {
    let mut out = ([Vec2::ZERO; 5], [None; 5]);
    for (k, place) in places(lead, big).into_iter().enumerate() {
        let spot = board.spots[place];
        out.0[k] = v(spot.x * flip, spot.y);
        out.1[k] = board.runs[place].map(|p| v(p.x * flip, p.y));
    }
    out
}
