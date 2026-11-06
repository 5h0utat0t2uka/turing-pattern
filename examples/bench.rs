//! Simulation time per size: `cargo run --release --example bench [steps]`

use std::time::Instant;

use turing_pattern::sim::{DEFAULT_STEPS, Grid, Pattern};

fn main() {
    let steps = std::env::args()
        .nth(1)
        .map(|s| s.parse().expect("steps must be an integer"))
        .unwrap_or(DEFAULT_STEPS);
    for size in [64, 128, 256, 512, 1024, 2048] {
        let mut grid = Grid::new(size, 1).expect("allocation");
        let start = Instant::now();
        grid.advance(steps, Pattern::Coral).expect("finite");
        println!("{size:>5}: {:.3}s", start.elapsed().as_secs_f64());
    }
}
