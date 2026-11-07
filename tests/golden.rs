//! Text-derived images must stay the same across releases and platforms.
//!
//! These hashes cover the whole pipeline: text → seed → pattern choice,
//! initial state, simulation, palette, and pixels. If one changes on purpose,
//! bump `GENERATOR_VERSION` in `src/id.rs` and update the values here. (The
//! prefix in `src/text.rs` changes only if the text-to-seed step changes.)

use sha2::{Digest, Sha256};

use turing_pattern::animate::Animation;
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

/// Decode a GIF and hash every frame as shown (composited) plus its delay,
/// so the pin is independent of how the encoder compresses the data.
fn animation_hash(gif: &[u8]) -> String {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    let mut decoder = options.read_info(gif).unwrap();
    let width = decoder.width() as usize;
    let mut hasher = Sha256::new();
    hasher.update(decoder.global_palette().unwrap());
    let mut canvas = vec![0u8; width * decoder.height() as usize];
    while let Some(frame) = decoder.read_next_frame().unwrap() {
        let (left, top, w) = (
            frame.left as usize,
            frame.top as usize,
            frame.width as usize,
        );
        for (i, &index) in frame.buffer.iter().enumerate() {
            if Some(index) != frame.transparent {
                canvas[(top + i / w) * width + left + i % w] = index;
            }
        }
        hasher.update(frame.delay.to_le_bytes());
        hasher.update(&canvas);
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn animation_is_pinned() {
    let seed = seed_from_text("alice@example.com");
    let patterns = [Pattern::random(seed)];
    let gif = Animation {
        size: 80,
        seed,
        steps: 600,
        patterns: &patterns,
        columns: 4,
        gap: 2,
        palette: random_palette(seed),
        frames: 24,
        fps: 24,
        comment: "",
    }
    .encode()
    .unwrap();
    assert_eq!(
        animation_hash(&gif),
        "fba659adc4657d48821217329fd41172eb6ef4698e1039f9e4cc4e7b023c5338"
    );
}
