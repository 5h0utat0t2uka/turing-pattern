//! Image ids: a short hash of everything that determines an image's pixels.
//!
//! Equal ids mean identical images; any difference in the inputs gives a
//! different id. The generator version is part of the hash, so an id made
//! after the generation method changes never matches one made before.

use sha2::{Digest, Sha256};

/// Bump when the same inputs would produce different pixels (random numbers,
/// initial state, simulation, palette, rendering, resizing, or layout).
/// v2: vivid two-hue palette blended in OKLCh, instead of one vivid and one
/// near-black color blended in RGB.
pub const GENERATOR_VERSION: u32 = 2;

/// Hex digits in an id (48 bits).
pub const ID_LEN: usize = 12;

/// The inputs that fully determine an image.
pub struct ImageSpec<'a> {
    /// Pattern name, or `all` for the side-by-side layout.
    pub pattern: &'a str,
    pub seed: u64,
    pub size: usize,
    pub steps: u32,
}

impl ImageSpec<'_> {
    /// Canonical text form, hashed for the id and stored in saved PNGs.
    pub fn describe(&self) -> String {
        format!(
            "turing-pattern v{GENERATOR_VERSION} pattern={} seed={} size={} steps={}",
            self.pattern, self.seed, self.size, self.steps
        )
    }

    pub fn id(&self) -> String {
        Sha256::digest(self.describe().as_bytes())
            .iter()
            .take(ID_LEN / 2)
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(pattern: &str, seed: u64, size: usize, steps: u32) -> ImageSpec<'_> {
        ImageSpec {
            pattern,
            seed,
            size,
            steps,
        }
    }

    #[test]
    fn describe_lists_every_input() {
        assert_eq!(
            spec("maze", 7, 24, 30).describe(),
            "turing-pattern v2 pattern=maze seed=7 size=24 steps=30"
        );
    }

    #[test]
    fn id_is_pinned() {
        // Independently: first 12 hex digits of
        // `printf 'turing-pattern v2 pattern=maze seed=7 size=24 steps=30' | shasum -a 256`.
        assert_eq!(spec("maze", 7, 24, 30).id(), PINNED);
    }

    const PINNED: &str = "7f485ec22c6b";

    #[test]
    fn every_input_changes_the_id() {
        let base = spec("maze", 7, 24, 30).id();
        assert_eq!(base.len(), ID_LEN);
        assert!(base.chars().all(|c| c.is_ascii_hexdigit()));
        for other in [
            spec("coral", 7, 24, 30),
            spec("all", 7, 24, 30),
            spec("maze", 8, 24, 30),
            spec("maze", 7, 25, 30),
            spec("maze", 7, 24, 31),
        ] {
            assert_ne!(other.id(), base, "{}", other.describe());
        }
    }
}
