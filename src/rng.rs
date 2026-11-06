//! Small deterministic random number generator (xoshiro256++ seeded by SplitMix64).
//!
//! Each purpose (initial state, colors, pattern choice) uses its own stream, so
//! adding or changing one never shifts the numbers another one sees.

/// Independent random streams derived from one seed.
#[derive(Clone, Copy)]
pub enum Stream {
    InitialState = 0,
    Palette = 1,
    Pattern = 2,
}

pub struct Rng {
    state: [u64; 4],
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    pub fn new(seed: u64, stream: Stream) -> Self {
        // Hash the seed first so that (seed, stream) pairs do not overlap simply.
        let mut mixer = seed;
        let hashed = splitmix64(&mut mixer);
        let mut sm = hashed ^ (stream as u64).wrapping_mul(0xD1B5_4A32_D192_ED03);
        let state = [
            splitmix64(&mut sm),
            splitmix64(&mut sm),
            splitmix64(&mut sm),
            splitmix64(&mut sm),
        ];
        Self { state }
    }

    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.state;
        let result = s[0].wrapping_add(s[3]).rotate_left(23).wrapping_add(s[0]);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// Uniform in [0, 1) with 53 bits of precision.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform in [low, high).
    pub fn uniform(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.next_f64()
    }

    /// Unbiased integer in [0, bound); `bound` must be positive.
    pub fn below(&mut self, bound: u64) -> u64 {
        assert!(bound > 0);
        // Lemire's multiply-and-reject method.
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let product = u128::from(self.next_u64()) * u128::from(bound);
            if (product as u64) >= threshold {
                return (product >> 64) as u64;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_and_stream_repeat() {
        let mut a = Rng::new(42, Stream::Palette);
        let mut b = Rng::new(42, Stream::Palette);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn streams_and_seeds_differ() {
        let first = Rng::new(42, Stream::InitialState).next_u64();
        assert_ne!(first, Rng::new(42, Stream::Palette).next_u64());
        assert_ne!(first, Rng::new(42, Stream::Pattern).next_u64());
        assert_ne!(first, Rng::new(43, Stream::InitialState).next_u64());
    }

    #[test]
    fn ranges_are_respected_and_covered() {
        let mut rng = Rng::new(7, Stream::InitialState);
        let mut seen = [false; 5];
        for _ in 0..10_000 {
            let x = rng.next_f64();
            assert!((0.0..1.0).contains(&x));
            let y = rng.uniform(-0.02, 0.02);
            assert!((-0.02..0.02).contains(&y));
            seen[rng.below(5) as usize] = true;
        }
        assert!(seen.iter().all(|&s| s));
        assert_eq!(Rng::new(0, Stream::Pattern).below(1), 0);
    }
}
