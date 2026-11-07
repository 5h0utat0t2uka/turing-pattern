//! Gray–Scott reaction-diffusion patterns in random colors, shown with chafa.

pub mod display;
pub mod render;
pub mod rng;
pub mod sim;
pub mod text;

use render::{Palette, downsample, render};
use sim::{Grid, Pattern};

/// Smallest grid that is simulated. A pattern feature spans roughly 10–20
/// cells, so on smaller periodic grids the patterns often fade to a flat
/// color; smaller images are simulated at this size and shrunk.
pub const MIN_SIMULATION_SIZE: usize = 64;

/// Simulate and render a `size`×`size` RGB image.
pub fn generate(
    size: usize,
    seed: u64,
    steps: u32,
    pattern: Pattern,
    palette: Palette,
) -> Result<Vec<u8>, String> {
    let simulated = size.max(MIN_SIMULATION_SIZE);
    let mut grid = Grid::new(simulated, seed)?;
    grid.advance(steps, pattern)?;
    Ok(downsample(&render(&grid.v, palette), simulated, size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::random_palette;

    #[test]
    fn small_images_always_show_a_pattern() {
        for pattern in Pattern::ALL {
            for size in [8, 16, 24, 32, 48] {
                for seed in 0..8 {
                    let pixels = generate(size, seed, 4000, pattern, random_palette(seed)).unwrap();
                    assert_eq!(pixels.len(), size * size * 3);
                    // The two palette colors differ a lot in brightness, so a
                    // visible pattern means a wide brightness range.
                    let brightness: Vec<u32> = pixels
                        .chunks(3)
                        .map(|p| p.iter().map(|&c| u32::from(c)).sum())
                        .collect();
                    let range = brightness.iter().max().unwrap() - brightness.iter().min().unwrap();
                    assert!(
                        range > 90,
                        "{pattern:?} size={size} seed={seed} range={range}"
                    );
                }
            }
        }
    }
}
