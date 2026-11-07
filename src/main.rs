use std::io::Read;
use std::process::ExitCode;

use clap::Parser;

use turing_pattern::render::{encode_png, hex, random_palette, tile};
use turing_pattern::sim::{DEFAULT_STEPS, Pattern};
use turing_pattern::text::seed_from_text;
use turing_pattern::{display, generate, generate_all};

/// Show a random Gray–Scott reaction-diffusion pattern in the terminal via chafa.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Side length in pixels [default: 512, or 256 per pattern with --all]
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    size: Option<u32>,

    /// Random seed (0 to 2^64-1); generated when omitted
    #[arg(long, conflicts_with = "text")]
    seed: Option<u64>,

    /// Derive the seed from text (NFC-normalized; case and spaces matter)
    #[arg(long)]
    text: Option<String>,

    /// Simulation update count
    #[arg(long, default_value_t = DEFAULT_STEPS, value_parser = clap::value_parser!(u32).range(1..))]
    steps: u32,

    /// Pattern type; chosen from the seed when omitted
    #[arg(long, value_enum, conflicts_with = "all")]
    pattern: Option<Pattern>,

    /// Show every pattern type side by side, with the same seed and colors
    #[arg(long)]
    all: bool,
}

/// Patterns per row with --all.
const GALLERY_COLUMNS: usize = 4;

fn random_seed() -> Result<u64, String> {
    let mut bytes = [0u8; 8];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| format!("cannot generate a seed: {e}"))?;
    Ok(u64::from_le_bytes(bytes))
}

fn run(args: Args) -> Result<(), String> {
    let chafa = display::find_chafa()?;
    let seed = match (args.seed, &args.text) {
        (Some(seed), _) => seed,
        (None, Some(text)) => seed_from_text(text),
        (None, None) => random_seed()?,
    };
    let palette = random_palette(seed);
    let png = if args.all {
        let size = args.size.unwrap_or(256) as usize;
        eprintln!(
            "Generating {} patterns, {size}×{size} each, seed={seed}, steps={}",
            Pattern::ALL.len(),
            args.steps
        );
        let tiles = generate_all(size, seed, args.steps, palette)?;
        eprintln!("Colors: {} / {}", hex(palette[0]), hex(palette[1]));
        let rows: Vec<String> = Pattern::ALL
            .chunks(GALLERY_COLUMNS)
            .map(|row| row.iter().map(|p| p.name()).collect::<Vec<_>>().join(" "))
            .collect();
        eprintln!("Order (left to right): {}", rows.join(" / "));
        let (pixels, width, height) = tile(&tiles, size, GALLERY_COLUMNS, (size / 16).max(2));
        encode_png(&pixels, width, height, true)?
    } else {
        let size = args.size.unwrap_or(512) as usize;
        let pattern = args.pattern.unwrap_or_else(|| Pattern::random(seed));
        eprintln!(
            "Generating {size}×{size}, seed={seed}, steps={}, pattern={}",
            args.steps,
            pattern.name()
        );
        let pixels = generate(size, seed, args.steps, pattern, palette)?;
        eprintln!("Colors: {} / {}", hex(palette[0]), hex(palette[1]));
        encode_png(&pixels, size, size, false)?
    };
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
