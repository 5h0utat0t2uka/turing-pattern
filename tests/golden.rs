//! Text-derived images must stay the same across releases and platforms.
//!
//! These hashes cover the whole pipeline: text → seed → pattern choice,
//! initial state, simulation, palette, and pixels. If one changes on purpose,
//! bump `GENERATOR_VERSION` in `src/id.rs` and update the values here. (The
//! prefix in `src/text.rs` changes only if the text-to-seed step changes.)

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
        "3197bf1eead29541b427fbeb96087c0653ba19b8a8920002e61ad22b5837afb3"
    );
}

#[test]
fn single_threaded_image_is_pinned() {
    assert_eq!(
        pixel_hash("alice@example.com", 96, 300),
        "ba0fab45eb04b87124bdc95c9735a46fd8014b000b942d9249a668d1545d8bee"
    );
}

#[test]
fn parallel_image_is_pinned() {
    // 256 and above use the multi-threaded path.
    assert_eq!(
        pixel_hash("チューリング", 256, 100),
        "7d7cf4a4824aeeff2a6229f7a1a98e80e403aa1edd7a73b5c58b0ca38b4bc13c"
    );
}
