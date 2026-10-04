//! Flattening transparent input onto white (spec F1, D19).
//!
//! L\* needs one final colour per pixel, but alpha only says how much of an
//! unknown background shows through. We pick white paper, blend once on
//! load, and everything after this step can ignore alpha.

use crate::error::Result;
use crate::image::{BYTES_PER_PIXEL, check_rgba_len};

/// Blends every pixel of a `width × height` RGBA8 buffer over white, in
/// place, and sets its alpha to 255.
///
/// Per channel: `out = c × a + 255 × (1 − a)`, with `a = A / 255`, rounded
/// to the nearest byte. The blend works on the stored sRGB bytes, not on
/// linear light, the same way browsers and the Canvas 2D "fill white, then
/// draw" path composite, so the CLI and the web app agree (D19). Opaque
/// pixels are unchanged, and fully transparent ones become white whatever
/// their stored RGB.
///
/// Call it once on load, before the buffer becomes the untouched original
/// (D15).
///
/// # Errors
/// Same as [`Image::new`](crate::Image::new): zero size, size overflow, or
/// a buffer length that isn't `width × height × 4`.
pub fn composite_over_white(rgba: &mut [u8], width: u32, height: u32) -> Result<()> {
    check_rgba_len(rgba.len(), width, height)?;
    for px in rgba.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
        let a = px[3];
        if a == u8::MAX {
            continue; // Photos: every pixel takes this path.
        }
        for c in &mut px[..3] {
            *c = blend_channel_over_white(*c, a);
        }
        px[3] = u8::MAX;
    }
    Ok(())
}

/// One channel `c` with alpha `a` over white, rounded to the nearest byte.
///
/// `c·a + 255·(255 − a)` equals `255² − (255 − c)·a`, so the result is
/// `255 − round((255 − c)·a / 255)`. For non-negative `x`,
/// `(x + 127) / 255` rounds `x / 255` exactly: 255 is odd, so there are no
/// halfway cases.
fn blend_channel_over_white(c: u8, a: u8) -> u8 {
    let darkening = (u32::from(u8::MAX - c) * u32::from(a) + 127) / 255;
    // darkening ≤ 255 − c, so this neither underflows nor truncates.
    u8::MAX - darkening as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use proptest::prelude::*;

    /// Composites a single pixel and returns it.
    fn one(px: [u8; 4]) -> [u8; 4] {
        let mut buf = px;
        composite_over_white(&mut buf, 1, 1).unwrap();
        buf
    }

    // Known values.

    #[test]
    fn opaque_is_unchanged() {
        assert_eq!(one([10, 20, 30, 255]), [10, 20, 30, 255]);
    }

    #[test]
    fn fully_transparent_becomes_white_whatever_its_rgb() {
        assert_eq!(one([0, 0, 0, 0]), [255, 255, 255, 255]);
        assert_eq!(one([200, 10, 90, 0]), [255, 255, 255, 255]);
    }

    #[test]
    fn half_transparent_black_is_mid_byte() {
        // 255 × (1 − 128/255) = 127 exactly.
        assert_eq!(one([0, 0, 0, 128]), [127, 127, 127, 255]);
        // 255 × (1 − 127/255) = 128 exactly.
        assert_eq!(one([0, 0, 0, 127]), [128, 128, 128, 255]);
    }

    #[test]
    fn white_stays_white_at_any_alpha() {
        for a in 0..=255u8 {
            assert_eq!(one([255, 255, 255, a]), [255, 255, 255, 255], "alpha {a}");
        }
    }

    #[test]
    fn channels_blend_independently() {
        // Red at a = 51 (20%): R stays 255, G and B become 255 × 0.8 = 204.
        assert_eq!(one([255, 0, 0, 51]), [255, 204, 204, 255]);
    }

    #[test]
    fn every_pixel_is_processed() {
        let mut buf = [
            0, 0, 0, 0, // transparent
            10, 20, 30, 255, // opaque
            0, 0, 0, 128, // half
            50, 60, 70, 255, // opaque
        ];
        composite_over_white(&mut buf, 2, 2).unwrap();
        assert_eq!(
            buf,
            [
                255, 255, 255, 255, 10, 20, 30, 255, 127, 127, 127, 255, 50, 60, 70, 255
            ]
        );
    }

    #[test]
    fn matches_exact_formula_for_every_channel_and_alpha() {
        for c in 0..=255u8 {
            for a in 0..=255u8 {
                let (cf, af) = (f64::from(c), f64::from(a) / 255.0);
                let want = (cf * af + 255.0 * (1.0 - af)).round();
                assert_eq!(
                    f64::from(blend_channel_over_white(c, a)),
                    want,
                    "c {c}, a {a}"
                );
            }
        }
    }

    // Invalid input: same rules as Image::new.

    #[test]
    fn rejects_wrong_length() {
        let mut buf = [0u8; 23];
        assert_eq!(
            composite_over_white(&mut buf, 3, 2).unwrap_err(),
            Error::BufferLength {
                expected: 24,
                actual: 23
            }
        );
    }

    #[test]
    fn rejects_zero_size() {
        assert_eq!(
            composite_over_white(&mut [], 0, 1).unwrap_err(),
            Error::ZeroSize {
                width: 0,
                height: 1
            }
        );
    }

    #[test]
    fn rejected_buffer_is_untouched() {
        let mut buf = [0u8; 7];
        let before = buf;
        assert!(composite_over_white(&mut buf, 1, 2).is_err());
        assert_eq!(buf, before);
    }

    // Properties.

    proptest! {
        #[test]
        fn output_is_opaque_and_never_darker(
            pixels in prop::collection::vec(any::<[u8; 4]>(), 1..64)
        ) {
            let mut buf: Vec<u8> = pixels.iter().flatten().copied().collect();
            let width = u32::try_from(pixels.len()).unwrap();
            composite_over_white(&mut buf, width, 1).unwrap();
            for (out, inp) in buf.as_chunks::<4>().0.iter().zip(&pixels) {
                prop_assert_eq!(out[3], 255);
                if inp[3] == 255 {
                    prop_assert_eq!(out, inp);
                }
                for ch in 0..3 {
                    // White paper can only lighten a channel.
                    prop_assert!(out[ch] >= inp[ch], "{:?} -> {:?}", inp, out);
                }
            }
        }

        #[test]
        fn is_idempotent(pixels in prop::collection::vec(any::<[u8; 4]>(), 1..64)) {
            let mut buf: Vec<u8> = pixels.iter().flatten().copied().collect();
            let width = u32::try_from(pixels.len()).unwrap();
            composite_over_white(&mut buf, width, 1).unwrap();
            let once = buf.clone();
            composite_over_white(&mut buf, width, 1).unwrap();
            prop_assert_eq!(buf, once);
        }

        #[test]
        fn more_opaque_never_gets_lighter(c: u8, a1: u8, a2: u8) {
            // More coverage means less paper shows through.
            let (lo, hi) = if a1 <= a2 { (a1, a2) } else { (a2, a1) };
            prop_assert!(blend_channel_over_white(c, hi) <= blend_channel_over_white(c, lo));
        }

        #[test]
        fn lighter_input_never_gets_darker(c1: u8, c2: u8, a: u8) {
            let (lo, hi) = if c1 <= c2 { (c1, c2) } else { (c2, c1) };
            prop_assert!(blend_channel_over_white(lo, a) <= blend_channel_over_white(hi, a));
        }
    }
}
