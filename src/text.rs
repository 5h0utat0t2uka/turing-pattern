//! Deterministic seeds from text, for Identicon-like use.
//!
//! The same text always gives the same seed, and therefore the same pattern,
//! colors, and shape. The prefix carries a version for this text-to-seed step
//! only; changes to what the seed feeds into bump `GENERATOR_VERSION` in
//! `id.rs` instead.

use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

const DOMAIN: &[u8] = b"turing-pattern:v1:";

/// SHA-256 of the versioned prefix and the NFC-normalized text; the first
/// 8 bytes, little-endian, become the seed.
///
/// Only NFC normalization is applied: text that looks identical but is encoded
/// differently gives the same seed, while case and whitespace are significant.
pub fn seed_from_text(text: &str) -> u64 {
    let normalized: String = text.nfc().collect();
    let digest = Sha256::new()
        .chain_update(DOMAIN)
        .chain_update(normalized.as_bytes())
        .finalize();
    u64::from_le_bytes(digest[..8].try_into().expect("SHA-256 is 32 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonically_equivalent_text_gives_the_same_seed() {
        // "が" as one code point, and as "か" plus a combining voiced mark.
        assert_eq!(
            seed_from_text("\u{304C}"),
            seed_from_text("\u{304B}\u{3099}")
        );
        // "é" as one code point, and as "e" plus a combining acute accent.
        assert_eq!(seed_from_text("caf\u{E9}"), seed_from_text("cafe\u{301}"));
    }

    #[test]
    fn case_and_whitespace_are_significant() {
        let base = seed_from_text("alice");
        assert_ne!(base, seed_from_text("Alice"));
        assert_ne!(base, seed_from_text("alice "));
        assert_ne!(base, seed_from_text(" alice"));
    }

    #[test]
    fn seeds_are_fixed_across_releases() {
        // Changing these values changes every text-derived image; bump DOMAIN instead.
        assert_eq!(seed_from_text(""), PINNED_EMPTY);
        assert_eq!(seed_from_text("alice@example.com"), PINNED_ALICE);
    }

    // Independently: first 8 bytes (little-endian) of `printf 'turing-pattern:v1:<text>' | shasum -a 256`.
    const PINNED_EMPTY: u64 = 16_487_043_545_047_252_160;
    const PINNED_ALICE: u64 = 15_188_035_183_127_902_847;
}
