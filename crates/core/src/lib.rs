//! Image logic for Painting Prep.
//!
//! Pure Rust: RGBA8 in, RGBA8 out. No wasm, web, or image-decoding
//! dependencies (spec §5).

#![forbid(unsafe_code)]
// CLAUDE.md: no `unwrap`/`expect` in library code; return `Result` instead.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod blur;
mod colour;
mod composite;
mod error;
mod grid;
mod histogram;
mod image;
mod lightness;
mod posterize;
mod thresholds;

pub use blur::blur_lightness;
pub use composite::composite_over_white;
pub use error::{Error, Result};
pub use grid::{Axis, GridMode, GridSpec, Line, grid_lines};
pub use histogram::{BINS, histogram};
pub use image::{BYTES_PER_PIXEL, Image};
pub use lightness::lightness_map;
pub use posterize::{OutputGreys, bucket_counts, bucket_greys, posterize};
pub use thresholds::{Algorithm, MAX_BUCKETS, MIN_BUCKETS, Thresholds, thresholds};
