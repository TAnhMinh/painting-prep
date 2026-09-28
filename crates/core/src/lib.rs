//! Image logic for Painting Prep.
//!
//! Pure Rust: RGBA8 in, RGBA8 out. No wasm, web, or image-decoding
//! dependencies (spec §5).

#![forbid(unsafe_code)]
// CLAUDE.md: no `unwrap`/`expect` in library code; return `Result` instead.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
