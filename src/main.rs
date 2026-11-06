use std::io::Read;
use std::process::ExitCode;

use clap::Parser;

use turing_pattern::display;
use turing_pattern::render::{encode_png, hex, random_palette, render};
use turing_pattern::sim::{DEFAULT_STEPS, Grid, Pattern};

/// Show a random Gray–Scott reaction-diffusion pattern in the terminal via chafa.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Side length in pixels
    #[arg(long, default_value_t = 512, value_parser = clap::value_parser!(u32).range(1..))]
    size: u32,

    /// Random seed (0 to 2^64-1); generated when omitted
    #[arg(long)]
    seed: Option<u64>,

    /// Simulation update count
    #[arg(long, default_value_t = DEFAULT_STEPS, value_parser = clap::value_parser!(u32).range(1..))]
    steps: u32,

    /// Pattern type; chosen from the seed when omitted
    #[arg(long, value_enum)]
    pattern: Option<Pattern>,
}

fn random_seed() -> Result<u64, String> {
    let mut bytes = [0u8; 8];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| format!("cannot generate a seed: {e}"))?;
    Ok(u64::from_le_bytes(bytes))
}

fn run(args: Args) -> Result<(), String> {
    let chafa = display::find_chafa()?;
    let seed = match args.seed {
        Some(seed) => seed,
        None => random_seed()?,
    };
    let pattern = args.pattern.unwrap_or_else(|| Pattern::random(seed));
    let size = args.size as usize;
    eprintln!(
        "Generating {size}×{size}, seed={seed}, steps={}, pattern={}",
        args.steps,
        pattern.name()
    );

    let mut grid = Grid::new(size, seed)?;
    grid.advance(args.steps, pattern)?;
    let palette = random_palette(seed);
    eprintln!("Colors: {} / {}", hex(palette[0]), hex(palette[1]));
    let png = encode_png(&render(&grid.v, palette), size)?;
    display::show(chafa.as_os_str(), &png)
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("turing-pattern: {message}");
            ExitCode::FAILURE
        }
    }
}
