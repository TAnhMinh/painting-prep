//! Image logic for Painting Prep.
//!
//! Pure Rust: RGBA8 in, RGBA8 out. No wasm, web, or image-decoding
//! dependencies (spec §5).

#![forbid(unsafe_code)]
// CLAUDE.md: no `unwrap`/`expect` in library code; return `Result` instead.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

// Only the tests use colour so far; lightness_map (M1 substep 4) will.
// `expect` warns once that changes, as a reminder to delete this attribute.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by lightness_map from M1 substep 4")
)]
mod colour;
mod composite;
mod error;
mod image;

pub use composite::composite_over_white;
pub use error::{Error, Result};
pub use image::{BYTES_PER_PIXEL, Image};
