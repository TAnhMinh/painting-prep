//! `prep`: desktop harness for prep-core. Decodes with `image`, then runs
//! the same core pipeline as the web app.

mod grid;
mod image_io;
mod values;

use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use prep_core::{
    Algorithm, Axis, GridMode, GridSpec, MAX_BUCKETS, MIN_BUCKETS, OutputGreys,
    composite_over_white,
};

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
    /// Draw a grid overlay onto a photo: square cells (an axis and a count)
    /// or rows × columns, with optional diagonals.
    Grid(GridArgs),
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

    /// Bucketing algorithm: equal intervals over the L* scale, equal pixel
    /// counts (quantiles), or Jenks natural breaks.
    #[arg(long, value_enum, default_value_t = Algo::Equal)]
    algo: Algo,

    /// Pre-blur ("squint") before bucketing, as a percentage of the short
    /// side (0 = off).
    #[arg(long, default_value_t = 0.0, value_parser = parse_blur)]
    blur: f32,

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

#[derive(Clone, Copy, ValueEnum)]
enum Algo {
    Equal,
    Quantile,
    Jenks,
}

/// Parses `--blur`, rejecting negatives (and non-numbers).
fn parse_blur(s: &str) -> Result<f32, String> {
    match s.parse::<f32>() {
        Ok(v) if v >= 0.0 => Ok(v),
        Ok(_) => Err("blur must be at least 0".into()),
        Err(_) => Err(format!("`{s}` is not a number")),
    }
}

#[derive(Args)]
struct GridArgs {
    /// Grid mode: square cells (an axis and a count) or rows × columns.
    #[arg(long, value_enum, default_value_t = Mode::Square)]
    mode: Mode,

    /// Reference axis whose length sets the square cell size (mode square).
    #[arg(long, value_enum, default_value_t = AxisArg::Width)]
    axis: AxisArg,

    /// Cells along the reference axis (mode square).
    #[arg(
        short = 'k',
        long = "cells",
        default_value_t = 6,
        value_parser = clap::value_parser!(u32).range(2..=16),
    )]
    k: u32,

    /// Rows (mode rowscols).
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u32).range(1..=12))]
    rows: u32,

    /// Columns (mode rowscols).
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u32).range(1..=12))]
    cols: u32,

    /// Draw both diagonals of every whole cell.
    #[arg(long)]
    diagonals: bool,

    /// Line colour. The app also offers a two-tone style; the CLI draws one
    /// colour.
    #[arg(long, value_enum, default_value_t = Colour::Black)]
    colour: Colour,

    /// Line thickness in px; 0 picks about 0.15% of the short side (min 1).
    #[arg(long = "line-px", default_value_t = 0)]
    line_px: u32,

    /// Input image (JPEG or PNG).
    input: PathBuf,

    /// Output image; the format follows the extension (PNG recommended).
    output: PathBuf,
}

#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    Square,
    Rowscols,
}

#[derive(Clone, Copy, ValueEnum)]
enum AxisArg {
    Width,
    Height,
}

#[derive(Clone, Copy, ValueEnum)]
enum Colour {
    Black,
    White,
    Grey,
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Values(args) => run_values(&args),
        Command::Grid(args) => run_grid(&args),
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

    let algo = match args.algo {
        Algo::Equal => Algorithm::EqualIntervals,
        Algo::Quantile => Algorithm::Quantile,
        Algo::Jenks => Algorithm::NaturalBreaks,
    };
    let modes: &[OutputGreys] = match args.greys {
        Greys::Even => &[OutputGreys::EvenlySpaced],
        Greys::Mean => &[OutputGreys::BucketMean],
        Greys::Both => &[OutputGreys::EvenlySpaced, OutputGreys::BucketMean],
    };
    let studies = modes
        .iter()
        .map(|&mode| analysis.study(args.n, algo, mode, args.blur))
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

fn run_grid(args: &GridArgs) -> Result<(), Box<dyn Error>> {
    let (mut rgba, width, height) = image_io::load(&args.input)
        .map_err(|e| format!("can't read {}: {e}", args.input.display()))?;
    // Flatten any transparency to opaque so the saved PNG has a clean background
    // under the grid (D19), the same first step as `values`.
    composite_over_white(&mut rgba, width, height)?;

    let mode = match args.mode {
        Mode::Square => GridMode::Square {
            axis: match args.axis {
                AxisArg::Width => Axis::Width,
                AxisArg::Height => Axis::Height,
            },
            k: args.k,
        },
        Mode::Rowscols => GridMode::RowsCols {
            rows: args.rows,
            cols: args.cols,
        },
    };
    let spec = GridSpec {
        mode,
        diagonals: args.diagonals,
    };

    let thickness = if args.line_px == 0 {
        // Match the web default: ~0.15% of the short side, at least 1 px (F3).
        (0.0015 * width.min(height) as f32).round().max(1.0) as u32
    } else {
        args.line_px
    };
    let colour = match args.colour {
        Colour::Black => [0, 0, 0],
        Colour::White => [255, 255, 255],
        Colour::Grey => [128, 128, 128],
    };

    let lines = grid::draw(&mut rgba, width, height, &spec, colour, thickness)?;
    image_io::save(&args.output, &rgba, width, height)
        .map_err(|e| format!("can't write {}: {e}", args.output.display()))?;

    println!("{}: {width} × {height}", args.input.display());
    println!("{}", grid::summary(width, height, &spec, thickness, lines));
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
