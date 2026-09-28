//! `prep`: desktop harness for prep-core. Decodes with `image` (added in M1).

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("prep {}: no commands yet", env!("CARGO_PKG_VERSION"));
    ExitCode::FAILURE
}
