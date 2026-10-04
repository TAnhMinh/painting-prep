//! Errors returned by prep-core for invalid input.

use core::fmt;

/// Invalid input to a prep-core function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Width or height is zero.
    ZeroSize { width: u32, height: u32 },
    /// A buffer's length doesn't match the image size.
    BufferLength { expected: usize, actual: usize },
    /// `width × height × 4` doesn't fit in `usize` (32 bits on wasm32).
    TooLarge { width: u32, height: u32 },
    /// A value study's bucket count is outside 2..=10.
    BucketCount { n: u8 },
}

/// Result type used throughout prep-core.
pub type Result<T> = core::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::ZeroSize { width, height } => {
                write!(f, "image has zero size ({width} × {height})")
            }
            Error::BufferLength { expected, actual } => {
                write!(f, "buffer has {actual} bytes, expected {expected}")
            }
            Error::TooLarge { width, height } => {
                write!(f, "image is too large to process ({width} × {height})")
            }
            Error::BucketCount { n } => {
                write!(f, "{n} buckets requested, expected 2 to 10")
            }
        }
    }
}

impl std::error::Error for Error {}
