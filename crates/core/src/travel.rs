//! Travel (filled in below).
use crate::league::{TeamId, World};

impl World {
    /// Day of the week (0 = Monday) the club's sessions are lost to travel
    /// before a game on `day`, if they are.
    pub(crate) fn travel_day_lost(&self, _team: TeamId, _day: u32) -> Option<usize> {
        None
    }
}
