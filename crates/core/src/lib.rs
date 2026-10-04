//! Image logic for Painting Prep.
//!
//! Pure Rust: RGBA8 in, RGBA8 out. No wasm, web, or image-decoding
//! dependencies (spec §5).

#![forbid(unsafe_code)]
// CLAUDE.md: no `unwrap`/`expect` in library code; return `Result` instead.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod colour;
mod composite;
mod error;
mod image;
mod lightness;

pub use composite::composite_over_white;
pub use error::{Error, Result};
pub use image::{BYTES_PER_PIXEL, Image};
pub use lightness::lightness_map;
