//! `prep`: desktop harness for prep-core. Decodes with `image`, then runs
//! the same core pipeline as the web app.

mod image_io;
mod values;

use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use prep_core::{MAX_BUCKETS, MIN_BUCKETS, OutputGreys, composite_over_white};

#[derive(Parser)]
#[command(
    name = "prep",
    version,
    about = "Desktop harness for Painting Prep's image core"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Reduce a photo to n shades of grey (a value study), with equal
    /// intervals over the full L* scale, and print its legend.
    Values(ValuesArgs),
}

#[derive(Args)]
struct ValuesArgs {
    /// Number of buckets (shades of grey).
    #[arg(
        short = 'n',
        long = "buckets",
        default_value_t = 5,
        value_parser = clap::value_parser!(u8).range(i64::from(MIN_BUCKETS)..=i64::from(MAX_BUCKETS)),
    )]
    n: u8,

    /// Output greys: evenly spaced L*, each bucket's mean L*, or both, side
    /// by side with the original (original | even | mean).
    #[arg(long, value_enum, default_value_t = Greys::Even)]
    greys: Greys,

    /// Input image (JPEG or PNG).
    input: PathBuf,

    /// Output image; the format follows the extension (PNG recommended).
    output: PathBuf,
}

#[derive(Clone, Copy, ValueEnum)]
enum Greys {
    Even,
    Mean,
    Both,
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Values(args) => run_values(&args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("prep: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_values(args: &ValuesArgs) -> Result<(), Box<dyn Error>> {
    let (mut rgba, width, height) = image_io::load(&args.input)
        .map_err(|e| format!("can't read {}: {e}", args.input.display()))?;
    composite_over_white(&mut rgba, width, height)?;
    let analysis = values::analyse(&rgba, width, height)?;

    let modes: &[OutputGreys] = match args.greys {
        Greys::Even => &[OutputGreys::EvenlySpaced],
        Greys::Mean => &[OutputGreys::BucketMean],
        Greys::Both => &[OutputGreys::EvenlySpaced, OutputGreys::BucketMean],
    };
    let studies = modes
        .iter()
        .map(|&mode| analysis.study(args.n, mode))
        .collect::<prep_core::Result<Vec<_>>>()?;

    let mut panels: Vec<&[u8]> = Vec::new();
    if matches!(args.greys, Greys::Both) {
        panels.push(&rgba);
    }
    panels.extend(studies.iter().map(|s| s.rgba.as_slice()));
    let (out, out_width) = values::side_by_side(&panels, width, height)?;
    image_io::save(&args.output, &out, out_width, height)
        .map_err(|e| format!("can't write {}: {e}", args.output.display()))?;

    println!("{}: {width} × {height}", args.input.display());
    for s in &studies {
        print!("\n{}", values::legend(s));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
