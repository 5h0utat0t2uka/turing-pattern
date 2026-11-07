//! Gray–Scott reaction-diffusion patterns in random colors, shown with chafa.

pub mod animate;
pub mod display;
pub mod id;
pub mod render;
pub mod rng;
pub mod save;
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

/// Generate every pattern type from the same seed (same colors and initial
/// layout), in `Pattern::ALL` order. Patterns are computed concurrently.
pub fn generate_all(
    size: usize,
    seed: u64,
    steps: u32,
    palette: Palette,
) -> Result<Vec<Vec<u8>>, String> {
    use rayon::prelude::*;
    Pattern::ALL
        .par_iter()
        .map(|&pattern| generate(size, seed, steps, pattern, palette))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::random_palette;

    #[test]
    fn generate_all_matches_individual_runs_in_order() {
        let palette = random_palette(5);
        let all = generate_all(20, 5, 200, palette).unwrap();
        assert_eq!(all.len(), Pattern::ALL.len());
        for (pixels, &pattern) in all.iter().zip(&Pattern::ALL) {
            assert_eq!(*pixels, generate(20, 5, 200, pattern, palette).unwrap());
        }
    }

    #[test]
    fn small_images_always_show_a_pattern() {
        for pattern in Pattern::ALL {
            for size in [8, 16, 24, 32, 48] {
                for seed in 0..8 {
                    let pixels = generate(size, seed, 4000, pattern, random_palette(seed)).unwrap();
                    assert_eq!(pixels.len(), size * size * 3);
                    // Each pixel is a blend of the two palette colors. A faded,
                    // flat image spans almost no range of blend amounts; the
                    // lowest measured for a visible pattern is about 0.33
                    // (holes, which has low contrast, shrunk to 8 pixels).
                    let palette = random_palette(seed).map(|c| c.map(f64::from));
                    let axis: Vec<f64> = (0..3).map(|k| palette[1][k] - palette[0][k]).collect();
                    let norm: f64 = axis.iter().map(|d| d * d).sum();
                    let amounts: Vec<f64> = pixels
                        .chunks(3)
                        .map(|p| {
                            (0..3)
                                .map(|k| (f64::from(p[k]) - palette[0][k]) * axis[k])
                                .sum::<f64>()
                                / norm
                        })
                        .collect();
                    let max = amounts.iter().copied().fold(f64::MIN, f64::max);
                    let min = amounts.iter().copied().fold(f64::MAX, f64::min);
                    assert!(
                        max - min > 0.25,
                        "{pattern:?} size={size} seed={seed} range={:.2}",
                        max - min
                    );
                }
            }
        }
    }
}
