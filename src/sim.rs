//! Gray–Scott reaction-diffusion on a square grid with periodic boundaries.

use rayon::prelude::*;

use crate::rng::{Rng, Stream};

pub const DIFFUSION_U: f32 = 1.0;
pub const DIFFUSION_V: f32 = 0.5;
pub const DEFAULT_STEPS: u32 = 4000;
/// Below this side length, thread synchronization costs more than it saves.
pub const PARALLEL_MIN_SIZE: usize = 256;

/// Pattern types, each a pair of feed (F) and kill (k) rates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Pattern {
    Coral,
    Maze,
    Spots,
    Holes,
    Worms,
}

impl Pattern {
    pub const ALL: [Pattern; 5] = [
        Pattern::Coral,
        Pattern::Maze,
        Pattern::Spots,
        Pattern::Holes,
        Pattern::Worms,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Pattern::Coral => "coral",
            Pattern::Maze => "maze",
            Pattern::Spots => "spots",
            Pattern::Holes => "holes",
            Pattern::Worms => "worms",
        }
    }

    /// (F, k)
    pub fn rates(self) -> (f64, f64) {
        match self {
            Pattern::Coral => (0.0545, 0.062),
            Pattern::Maze => (0.029, 0.057),
            Pattern::Spots => (0.0367, 0.0649),
            Pattern::Holes => (0.039, 0.058),
            Pattern::Worms => (0.058, 0.065),
        }
    }

    /// Pick a pattern from the seed, independently of the initial state and colors.
    pub fn random(seed: u64) -> Pattern {
        let mut rng = Rng::new(seed, Stream::Pattern);
        Pattern::ALL[rng.below(Pattern::ALL.len() as u64) as usize]
    }
}

/// Coefficients in the exact single-precision form used by every update.
#[derive(Clone, Copy)]
struct Coefficients {
    du: f32,
    dv: f32,
    near: f32,
    far: f32,
    feed: f32,
    decay: f32,
}

impl Coefficients {
    fn new(pattern: Pattern) -> Self {
        let (feed, kill) = pattern.rates();
        Self {
            du: DIFFUSION_U,
            dv: DIFFUSION_V,
            near: 0.2,
            far: 0.05,
            feed: feed as f32,
            decay: (feed + kill) as f32,
        }
    }
}

/// Concentrations `u` and `v`, stored row-major, plus buffers for the next step.
pub struct Grid {
    size: usize,
    pub u: Vec<f32>,
    pub v: Vec<f32>,
    next_u: Vec<f32>,
    next_v: Vec<f32>,
}

fn allocate(len: usize, value: f32) -> Result<Vec<f32>, String> {
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(len)
        .map_err(|_| "not enough memory for this size".to_string())?;
    buffer.resize(len, value);
    Ok(buffer)
}

impl Grid {
    /// Create a fresh state; randomness is local to this initialization.
    pub fn new(size: usize, seed: u64) -> Result<Self, String> {
        assert!(size > 0);
        let len = size
            .checked_mul(size)
            .ok_or_else(|| "size is too large".to_string())?;
        let mut grid = Self {
            size,
            u: allocate(len, 1.0)?,
            v: allocate(len, 0.0)?,
            next_u: allocate(len, 0.0)?,
            next_v: allocate(len, 0.0)?,
        };

        // Finite perturbations initiate the reaction throughout a larger image.
        // A single central seed would need more time to reach distant edges.
        let mut rng = Rng::new(seed, Stream::InitialState);
        for _ in 0..(len / 1024).max(1) {
            let y = rng.below(size as u64) as usize;
            let x = rng.below(size as u64) as usize;
            let radius = 3 + rng.below(5) as usize;
            for row in y.saturating_sub(radius)..=(y + radius).min(size - 1) {
                for col in x.saturating_sub(radius)..=(x + radius).min(size - 1) {
                    let noise = rng.uniform(-0.02, 0.02) as f32;
                    grid.u[row * size + col] = 0.5 + noise;
                    grid.v[row * size + col] = 0.25 + noise;
                }
            }
        }
        Ok(grid)
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// Advance the state in place; callers may continue it later.
    pub fn advance(&mut self, steps: u32, pattern: Pattern) -> Result<(), String> {
        self.advance_with(steps, pattern, self.size >= PARALLEL_MIN_SIZE);
        if self.u.iter().chain(&self.v).all(|x| x.is_finite()) {
            Ok(())
        } else {
            Err("simulation produced non-finite concentrations".to_string())
        }
    }

    fn advance_with(&mut self, steps: u32, pattern: Pattern, parallel: bool) {
        let c = Coefficients::new(pattern);
        let n = self.size;
        for _ in 0..steps {
            let (u, v) = (&self.u[..], &self.v[..]);
            if parallel {
                // A few rows per task keeps scheduling overhead small.
                let rows = self
                    .next_u
                    .par_chunks_mut(n)
                    .zip(self.next_v.par_chunks_mut(n))
                    .enumerate()
                    .with_min_len((8192 / n).max(1));
                rows.for_each(|(i, (out_u, out_v))| update_row(u, v, n, i, out_u, out_v, &c));
            } else {
                let rows = self.next_u.chunks_mut(n).zip(self.next_v.chunks_mut(n));
                for (i, (out_u, out_v)) in rows.enumerate() {
                    update_row(u, v, n, i, out_u, out_v, &c);
                }
            }
            std::mem::swap(&mut self.u, &mut self.next_u);
            std::mem::swap(&mut self.v, &mut self.next_v);
        }
    }
}

/// Rows above, at, and below the row being updated, for one concentration.
struct Rows<'a> {
    up: &'a [f32],
    mid: &'a [f32],
    down: &'a [f32],
}

impl<'a> Rows<'a> {
    fn new(field: &'a [f32], n: usize, i: usize) -> Self {
        let up = if i == 0 { n - 1 } else { i - 1 };
        let down = if i + 1 == n { 0 } else { i + 1 };
        Self {
            up: &field[up * n..][..n],
            mid: &field[i * n..][..n],
            down: &field[down * n..][..n],
        }
    }

    /// Nine-point Laplacian at column `j`, with neighbouring columns `l` and `r`.
    #[inline(always)]
    fn laplacian(&self, l: usize, j: usize, r: usize, c: &Coefficients) -> f32 {
        let adjacent = self.up[j] + self.down[j] + self.mid[l] + self.mid[r];
        let diagonal = self.up[l] + self.up[r] + self.down[l] + self.down[r];
        -self.mid[j] + c.near * adjacent + c.far * diagonal
    }
}

#[inline(always)]
fn update_cell(
    u: &Rows,
    v: &Rows,
    (l, j, r): (usize, usize, usize),
    c: &Coefficients,
) -> (f32, f32) {
    let (uc, vc) = (u.mid[j], v.mid[j]);
    let reaction = uc * vc * vc;
    // Both derivatives use the current state only.
    let du = c.du * u.laplacian(l, j, r, c) - reaction + c.feed * (1.0 - uc);
    let dv = c.dv * v.laplacian(l, j, r, c) + reaction - c.decay * vc;
    (uc + du, vc + dv)
}

fn update_row(
    u: &[f32],
    v: &[f32],
    n: usize,
    i: usize,
    out_u: &mut [f32],
    out_v: &mut [f32],
    c: &Coefficients,
) {
    let (u, v) = (Rows::new(u, n, i), Rows::new(v, n, i));
    let out_u = &mut out_u[..n];
    let out_v = &mut out_v[..n];
    if n < 3 {
        for j in 0..n {
            let cols = ((j + n - 1) % n, j, (j + 1) % n);
            (out_u[j], out_v[j]) = update_cell(&u, &v, cols, c);
        }
        return;
    }
    // Edges wrap around; the interior has no branches so it can be vectorized.
    (out_u[0], out_v[0]) = update_cell(&u, &v, (n - 1, 0, 1), c);
    for j in 1..n - 1 {
        (out_u[j], out_v[j]) = update_cell(&u, &v, (j - 1, j, j + 1), c);
    }
    (out_u[n - 1], out_v[n - 1]) = update_cell(&u, &v, (n - 2, n - 1, 0), c);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Straightforward definition of one step, written independently of the fast path.
    fn reference_step(u: &mut [f32], v: &mut [f32], n: usize, pattern: Pattern) {
        let c = Coefficients::new(pattern);
        let at = |f: &[f32], y: usize, x: usize| f[(y % n) * n + (x % n)];
        let laplacian = |f: &[f32], y: usize, x: usize| {
            let (y, x) = (y + n, x + n);
            let adjacent = at(f, y - 1, x) + at(f, y + 1, x) + at(f, y, x - 1) + at(f, y, x + 1);
            let diagonal = at(f, y - 1, x - 1)
                + at(f, y - 1, x + 1)
                + at(f, y + 1, x - 1)
                + at(f, y + 1, x + 1);
            -at(f, y, x) + c.near * adjacent + c.far * diagonal
        };
        let mut du = vec![0.0; n * n];
        let mut dv = vec![0.0; n * n];
        for y in 0..n {
            for x in 0..n {
                let k = y * n + x;
                let reaction = u[k] * v[k] * v[k];
                du[k] = c.du * laplacian(u, y, x) - reaction + c.feed * (1.0 - u[k]);
                dv[k] = c.dv * laplacian(v, y, x) + reaction - c.decay * v[k];
            }
        }
        for k in 0..n * n {
            u[k] += du[k];
            v[k] += dv[k];
        }
    }

    fn ptp(values: &[f32]) -> f32 {
        let max = values.iter().copied().fold(f32::MIN, f32::max);
        let min = values.iter().copied().fold(f32::MAX, f32::min);
        max - min
    }

    fn std(values: &[f32]) -> f64 {
        let mean = values.iter().map(|&x| f64::from(x)).sum::<f64>() / values.len() as f64;
        let var = values
            .iter()
            .map(|&x| (f64::from(x) - mean).powi(2))
            .sum::<f64>();
        (var / values.len() as f64).sqrt()
    }

    #[test]
    fn fast_paths_match_reference_exactly() {
        for parallel in [false, true] {
            for pattern in Pattern::ALL {
                for size in [1, 2, 3, 17, 40] {
                    let mut grid = Grid::new(size, 11).unwrap();
                    let (mut u, mut v) = (grid.u.clone(), grid.v.clone());
                    grid.advance_with(300, pattern, parallel);
                    for _ in 0..300 {
                        reference_step(&mut u, &mut v, size, pattern);
                    }
                    assert_eq!(grid.u, u, "u: {pattern:?} size={size} parallel={parallel}");
                    assert_eq!(grid.v, v, "v: {pattern:?} size={size} parallel={parallel}");
                }
            }
        }
    }

    #[test]
    fn seed_reproduces_state() {
        let mut a = Grid::new(65, 42).unwrap();
        let mut b = Grid::new(65, 42).unwrap();
        a.advance(100, Pattern::Coral).unwrap();
        b.advance(100, Pattern::Coral).unwrap();
        assert_eq!(a.u, b.u);
        assert_eq!(a.v, b.v);
    }

    #[test]
    fn different_seeds_change_initial_state() {
        assert_ne!(Grid::new(65, 42).unwrap().v, Grid::new(65, 43).unwrap().v);
    }

    #[test]
    fn continuing_state_matches_one_run() {
        let mut a = Grid::new(33, 7).unwrap();
        let mut b = Grid::new(33, 7).unwrap();
        a.advance(100, Pattern::Maze).unwrap();
        a.advance(100, Pattern::Maze).unwrap();
        b.advance(200, Pattern::Maze).unwrap();
        assert_eq!(a.u, b.u);
        assert_eq!(a.v, b.v);
    }

    #[test]
    fn periodic_laplacian_conserves_total_concentration() {
        let n = 9;
        let mut field = vec![0.0f32; n * n];
        field[0] = 1.0;
        let c = Coefficients::new(Pattern::Coral);
        let mut total = 0.0f64;
        let mut values = vec![0.0f32; n * n];
        for i in 0..n {
            let rows = Rows::new(&field, n, i);
            for j in 0..n {
                values[i * n + j] = rows.laplacian((j + n - 1) % n, j, (j + 1) % n, &c);
                total += f64::from(values[i * n + j]);
            }
        }
        assert!(total.abs() < 1e-6);
        assert!((values[n - 1] - 0.2).abs() < 1e-6); // wraps to the last column
        assert!((values[n * n - 1] - 0.05).abs() < 1e-6); // wraps to the opposite corner
    }

    #[test]
    fn uniform_equilibrium_is_preserved() {
        let mut grid = Grid::new(9, 0).unwrap();
        grid.u.fill(1.0);
        grid.v.fill(0.0);
        grid.advance(100, Pattern::Coral).unwrap();
        assert!(grid.u.iter().all(|&x| (x - 1.0).abs() < 1e-6));
        assert!(grid.v.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn default_duration_forms_every_pattern() {
        for pattern in Pattern::ALL {
            for size in [64, 96] {
                let mut grid = Grid::new(size, 42).unwrap();
                grid.advance(DEFAULT_STEPS, pattern).unwrap();
                assert!(
                    ptp(&grid.v) > 0.2,
                    "{pattern:?} size={size} ptp={}",
                    ptp(&grid.v)
                );
                assert!(
                    std(&grid.v) > 0.05,
                    "{pattern:?} size={size} std={}",
                    std(&grid.v)
                );
            }
        }
    }

    #[test]
    fn patterns_differ() {
        let results: Vec<Vec<f32>> = Pattern::ALL
            .iter()
            .map(|&pattern| {
                let mut grid = Grid::new(48, 3).unwrap();
                grid.advance(500, pattern).unwrap();
                grid.v
            })
            .collect();
        for (i, first) in results.iter().enumerate() {
            for second in &results[i + 1..] {
                assert_ne!(first, second);
            }
        }
    }

    #[test]
    fn pattern_choice_is_reproducible_and_covers_all() {
        assert_eq!(Pattern::random(42), Pattern::random(42));
        let seen: std::collections::HashSet<_> =
            (0..200).map(Pattern::random).map(Pattern::name).collect();
        assert_eq!(seen.len(), Pattern::ALL.len());
    }

    #[test]
    fn overflow_is_detected() {
        let mut grid = Grid::new(8, 0).unwrap();
        grid.v.fill(1e30);
        assert!(grid.advance(10, Pattern::Coral).is_err());
    }
}
