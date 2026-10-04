//! Read-only view of an RGBA8 image.

use crate::error::{Error, Result};

/// Bytes per RGBA8 pixel.
pub const BYTES_PER_PIXEL: usize = 4;

/// A borrowed, validated RGBA8 image: `width × height` pixels, row-major,
/// 4 bytes per pixel, no row padding.
///
/// The fields are private, so the only way to get an `Image` is
/// [`Image::new`], which checks the size and buffer length once. Functions
/// taking `&Image` can rely on that without re-checking. The pixels are
/// borrowed, never copied, and can't be modified while the view exists (D15).
#[derive(Debug, Clone, Copy)]
pub struct Image<'a> {
    rgba: &'a [u8],
    width: u32,
    height: u32,
}

impl<'a> Image<'a> {
    /// Wraps `rgba` as a `width × height` image.
    ///
    /// # Errors
    /// - [`Error::ZeroSize`] if `width` or `height` is 0.
    /// - [`Error::TooLarge`] if `width × height × 4` overflows `usize`.
    /// - [`Error::BufferLength`] if `rgba.len()` isn't `width × height × 4`.
    pub fn new(rgba: &'a [u8], width: u32, height: u32) -> Result<Self> {
        check_rgba_len(rgba.len(), width, height)?;
        Ok(Image {
            rgba,
            width,
            height,
        })
    }

    /// The pixel bytes, `[r, g, b, a, r, g, b, a, ...]`.
    pub fn rgba(&self) -> &'a [u8] {
        self.rgba
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// `width × height`. Can't overflow: `new` checked `width × height × 4`.
    pub fn pixel_count(&self) -> usize {
        self.rgba.len() / BYTES_PER_PIXEL
    }
}

/// Checks that `len` bytes hold exactly a `width × height` RGBA8 image.
/// Shared by [`Image::new`] and functions that take a mutable buffer.
pub(crate) fn check_rgba_len(len: usize, width: u32, height: u32) -> Result<()> {
    if width == 0 || height == 0 {
        return Err(Error::ZeroSize { width, height });
    }
    let expected = rgba_len(width, height).ok_or(Error::TooLarge { width, height })?;
    if len != expected {
        return Err(Error::BufferLength {
            expected,
            actual: len,
        });
    }
    Ok(())
}

/// `width × height × 4`, or `None` if it overflows `usize`.
fn rgba_len(width: u32, height: u32) -> Option<usize> {
    usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(BYTES_PER_PIXEL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_image() {
        let buf = [0u8; 3 * 2 * 4];
        let img = Image::new(&buf, 3, 2).unwrap();
        assert_eq!(img.width(), 3);
        assert_eq!(img.height(), 2);
        assert_eq!(img.pixel_count(), 6);
        assert_eq!(img.rgba().len(), 24);
    }

    #[test]
    fn single_pixel() {
        let buf = [10, 20, 30, 255];
        let img = Image::new(&buf, 1, 1).unwrap();
        assert_eq!(img.pixel_count(), 1);
        assert_eq!(img.rgba(), &[10, 20, 30, 255]);
    }

    #[test]
    fn zero_width() {
        assert_eq!(
            Image::new(&[], 0, 5).unwrap_err(),
            Error::ZeroSize {
                width: 0,
                height: 5
            }
        );
    }

    #[test]
    fn zero_height() {
        assert_eq!(
            Image::new(&[], 5, 0).unwrap_err(),
            Error::ZeroSize {
                width: 5,
                height: 0
            }
        );
    }

    #[test]
    fn buffer_too_short() {
        let buf = [0u8; 23];
        assert_eq!(
            Image::new(&buf, 3, 2).unwrap_err(),
            Error::BufferLength {
                expected: 24,
                actual: 23
            }
        );
    }

    #[test]
    fn buffer_too_long() {
        let buf = [0u8; 25];
        assert_eq!(
            Image::new(&buf, 3, 2).unwrap_err(),
            Error::BufferLength {
                expected: 24,
                actual: 25
            }
        );
    }

    #[test]
    fn too_large_overflows() {
        // u32::MAX² × 4 overflows even a 64-bit usize.
        assert_eq!(
            Image::new(&[], u32::MAX, u32::MAX).unwrap_err(),
            Error::TooLarge {
                width: u32::MAX,
                height: u32::MAX
            }
        );
    }

    #[test]
    fn error_messages_are_readable() {
        let e = Error::BufferLength {
            expected: 24,
            actual: 23,
        };
        assert_eq!(e.to_string(), "buffer has 23 bytes, expected 24");
    }
}
