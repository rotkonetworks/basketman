use serde::{Deserialize, Serialize};

/// xoshiro256++ seeded through splitmix64. Owned here (rather than the `rand`
/// crate) so results are bit-identical on every target and across upgrades.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut z = seed;
        let mut next = || {
            z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            x ^ (x >> 31)
        };
        Rng {
            s: [next(), next(), next(), next()],
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.s;
        let out = s[0].wrapping_add(s[3]).rotate_left(23).wrapping_add(s[0]);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        out
    }

    /// Uniform in `[0, 1)`.
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    /// Uniform integer in `[lo, hi)`.
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        debug_assert!(hi > lo);
        lo + (self.next_u64() % (hi - lo) as u64) as u32
    }

    /// Index chosen proportionally to `weights`.
    pub fn weighted(&mut self, weights: &[f32]) -> usize {
        let total: f32 = weights.iter().sum();
        let mut x = self.f32() * total;
        for (i, w) in weights.iter().enumerate() {
            if x < *w {
                return i;
            }
            x -= w;
        }
        weights.len() - 1
    }
}
