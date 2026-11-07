//! Text-derived images must stay the same across releases and platforms.
//!
//! These hashes cover the whole pipeline: text → seed → pattern choice,
//! initial state, simulation, palette, and pixels. If one changes on purpose,
//! bump the version prefix in `src/text.rs` and update the values here.

use sha2::{Digest, Sha256};

use turing_pattern::generate;
use turing_pattern::render::random_palette;
use turing_pattern::sim::Pattern;
use turing_pattern::text::seed_from_text;

fn pixel_hash(text: &str, size: usize, steps: u32) -> String {
    let seed = seed_from_text(text);
    let pixels = generate(
        size,
        seed,
        steps,
        Pattern::random(seed),
        random_palette(seed),
    )
    .unwrap();
    Sha256::digest(&pixels)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn downsampled_image_is_pinned() {
    // Below 64 the grid is simulated at 64 and shrunk.
    assert_eq!(
        pixel_hash("alice@example.com", 48, 500),
        "699d6a338bad6bcfa4bc4dbc3e12313a1c6d831a24fb869b6fcd90e3db5fadc9"
    );
}

#[test]
fn single_threaded_image_is_pinned() {
    assert_eq!(
        pixel_hash("alice@example.com", 96, 300),
        "1e3d56f721dc34e8ead025607fee6cf5dd93773b17347f9283fc6f3cf869dd4b"
    );
}

#[test]
fn parallel_image_is_pinned() {
    // 256 and above use the multi-threaded path.
    assert_eq!(
        pixel_hash("チューリング", 256, 100),
        "13c4ca4eb592eff1ddc98b07bea8f6e211a0e83297590b58ae3287630f83a50d"
    );
}
