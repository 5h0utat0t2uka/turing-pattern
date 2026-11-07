use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use turing_pattern::id::ImageSpec;
use turing_pattern::render::{encode_png, hex, random_palette, tile};
use turing_pattern::save::{self, Saved};
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

    /// Also save the PNG in DIR (default: current directory), named from the
    /// pattern, seed, size, and steps; existing files are never overwritten
    #[arg(long, value_name = "DIR", num_args = 0..=1, default_missing_value = ".")]
    save: Option<PathBuf>,
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
    if let Some(dir) = &args.save {
        save::check_directory(dir)?;
    }
    let seed = match (args.seed, &args.text) {
        (Some(seed), _) => seed,
        (None, Some(text)) => seed_from_text(text),
        (None, None) => random_seed()?,
    };
    let palette = random_palette(seed);
    let size = args.size.unwrap_or(if args.all { 256 } else { 512 }) as usize;
    let pattern = args.pattern.unwrap_or_else(|| Pattern::random(seed));
    let spec = ImageSpec {
        pattern: if args.all { "all" } else { pattern.name() },
        seed,
        size,
        steps: args.steps,
    };
    let id = spec.id();
    if args.all {
        eprintln!(
            "Generating {} patterns, {size}×{size} each, seed={seed}, steps={}, id={id}",
            Pattern::ALL.len(),
            args.steps
        );
    } else {
        eprintln!(
            "Generating {size}×{size}, seed={seed}, steps={}, pattern={}, id={id}",
            args.steps,
            pattern.name()
        );
    }

    let (pixels, width, height, alpha) = if args.all {
        let tiles = generate_all(size, seed, args.steps, palette)?;
        let rows: Vec<String> = Pattern::ALL
            .chunks(GALLERY_COLUMNS)
            .map(|row| row.iter().map(|p| p.name()).collect::<Vec<_>>().join(" "))
            .collect();
        eprintln!("Order (left to right): {}", rows.join(" / "));
        let (pixels, width, height) = tile(&tiles, size, GALLERY_COLUMNS, (size / 16).max(2));
        (pixels, width, height, true)
    } else {
        (
            generate(size, seed, args.steps, pattern, palette)?,
            size,
            size,
            false,
        )
    };
    eprintln!("Colors: {} / {}", hex(palette[0]), hex(palette[1]));
    // The inputs travel with the image, so a saved file can be traced and remade.
    let text = [("turing-pattern", format!("{} id={id}", spec.describe()))];
    let png = encode_png(&pixels, width, height, alpha, &text)?;
    let name = save::file_name(&id);
    if let Some(dir) = &args.save {
        match save::save(dir, &name, &png)? {
            Saved::Created(path) => eprintln!("Saved: {}", path.display()),
            Saved::Existing(path) => eprintln!("Already saved: {}", path.display()),
        }
    }
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
