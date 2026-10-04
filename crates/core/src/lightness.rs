//! Per-pixel lightness: RGBA8 → L\* map (spec §6).

use crate::colour::{luminance, srgb_to_linear, y_to_lstar};
use crate::error::{Error, Result};
use crate::image::{BYTES_PER_PIXEL, Image};

/// Writes the CIE L\* (0..=100) of every pixel of `img` to `out`, in the
/// same row-major order.
///
/// Each pixel goes sRGB → linear → Y → L\*. Alpha is ignored: transparent
/// input is composited first with
/// [`composite_over_white`](crate::composite_over_white), so every pixel is
/// opaque here (D19).
///
/// # Errors
/// [`Error::BufferLength`] if `out.len()` isn't `img.pixel_count()`.
pub fn lightness_map(img: &Image, out: &mut [f32]) -> Result<()> {
    let expected = img.pixel_count();
    if out.len() != expected {
        return Err(Error::BufferLength {
            expected,
            actual: out.len(),
        });
    }
    // A byte has only 256 values, so decode each once instead of three
    // `powf` per pixel. The entries are `srgb_to_linear` itself, so the
    // result is identical to calling it directly.
    let linear: [f32; 256] = core::array::from_fn(|v| srgb_to_linear(v as u8));
    let (pixels, _) = img.rgba().as_chunks::<BYTES_PER_PIXEL>();
    for (l, &[r, g, b, _]) in out.iter_mut().zip(pixels) {
        let y = luminance(
            linear[usize::from(r)],
            linear[usize::from(g)],
            linear[usize::from(b)],
        );
        *l = y_to_lstar(y);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    /// L\* map of a 1-row image built from RGB triples (alpha 255).
    fn map(pixels: &[[u8; 3]]) -> Vec<f32> {
        let rgba: Vec<u8> = pixels
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect();
        let img = Image::new(&rgba, u32::try_from(pixels.len()).unwrap(), 1).unwrap();
        let mut out = vec![f32::NAN; pixels.len()];
        lightness_map(&img, &mut out).unwrap();
        out
    }

    /// The pipeline without the lookup table.
    fn reference(r: u8, g: u8, b: u8) -> f32 {
        y_to_lstar(luminance(
            srgb_to_linear(r),
            srgb_to_linear(g),
            srgb_to_linear(b),
        ))
    }

    // Known values (spec §8).

    #[test]
    fn known_greys_and_primaries() {
        let l = map(&[
            [0, 0, 0],
            [255, 255, 255],
            [119, 119, 119],
            [255, 0, 0],
            [0, 255, 0],
            [0, 0, 255],
        ]);
        assert_eq!(l[0], 0.0);
        assert!(close(l[1], 100.0, 1e-4), "white {}", l[1]);
        assert!(close(l[2], 50.0, 0.1), "grey 119 {}", l[2]);
        // CIELAB (D65): R 53.24, G 87.73, B 32.30.
        assert!(close(l[3], 53.24, 0.01), "red {}", l[3]);
        assert!(close(l[4], 87.73, 0.01), "green {}", l[4]);
        assert!(close(l[5], 32.30, 0.01), "blue {}", l[5]);
    }

    #[test]
    fn keeps_row_major_order() {
        // 2 × 2: black, white / white, black.
        let rgba = [
            0, 0, 0, 255, 255, 255, 255, 255, //
            255, 255, 255, 255, 0, 0, 0, 255,
        ];
        let img = Image::new(&rgba, 2, 2).unwrap();
        let mut out = [f32::NAN; 4];
        lightness_map(&img, &mut out).unwrap();
        assert_eq!(out[0], 0.0);
        assert!(close(out[1], 100.0, 1e-4));
        assert!(close(out[2], 100.0, 1e-4));
        assert_eq!(out[3], 0.0);
    }

    #[test]
    fn lookup_table_matches_direct_pipeline_for_every_byte() {
        // Every byte value in every channel position, bit for bit.
        let pixels: Vec<[u8; 3]> = (0..=255u8)
            .flat_map(|v| {
                [
                    [v, v, v],
                    [v, 0, 0],
                    [0, v, 0],
                    [0, 0, v],
                    [v, 255 - v, v / 2],
                ]
            })
            .collect();
        for (&[r, g, b], l) in pixels.iter().zip(map(&pixels)) {
            assert_eq!(l, reference(r, g, b), "({r},{g},{b})");
        }
    }

    #[test]
    fn matches_palette() {
        use palette::{IntoColor, Lab, Srgb};
        let steps = [0u8, 1, 10, 50, 100, 128, 200, 254, 255];
        let mut pixels = Vec::new();
        for &r in &steps {
            for &g in &steps {
                for &b in &steps {
                    pixels.push([r, g, b]);
                }
            }
        }
        for (&[r, g, b], ours) in pixels.iter().zip(map(&pixels)) {
            let lab: Lab = Srgb::new(r, g, b).into_format::<f32>().into_color();
            assert!(
                close(ours, lab.l, 0.01),
                "({r},{g},{b}): {ours} vs {}",
                lab.l
            );
        }
    }

    #[test]
    fn ignores_alpha() {
        let rgba = [100, 150, 200, 255, 100, 150, 200, 0, 100, 150, 200, 77];
        let img = Image::new(&rgba, 3, 1).unwrap();
        let mut out = [f32::NAN; 3];
        lightness_map(&img, &mut out).unwrap();
        assert_eq!(out[0], out[1]);
        assert_eq!(out[0], out[2]);
    }

    // Invalid input.

    #[test]
    fn rejects_short_output() {
        let rgba = [0u8; 2 * 2 * 4];
        let img = Image::new(&rgba, 2, 2).unwrap();
        let mut out = [0.0f32; 3];
        assert_eq!(
            lightness_map(&img, &mut out).unwrap_err(),
            Error::BufferLength {
                expected: 4,
                actual: 3
            }
        );
    }

    #[test]
    fn rejects_long_output() {
        let rgba = [0u8; 4];
        let img = Image::new(&rgba, 1, 1).unwrap();
        let mut out = [0.0f32; 2];
        assert_eq!(
            lightness_map(&img, &mut out).unwrap_err(),
            Error::BufferLength {
                expected: 1,
                actual: 2
            }
        );
    }

    // Properties.

    proptest! {
        #[test]
        fn output_is_in_range(pixels in prop::collection::vec(any::<[u8; 3]>(), 1..64)) {
            for (p, l) in pixels.iter().zip(map(&pixels)) {
                prop_assert!((0.0..=100.0 + 1e-4).contains(&l), "{:?} -> {}", p, l);
            }
        }

        #[test]
        fn each_pixel_is_independent_of_the_others(
            pixels in prop::collection::vec(any::<[u8; 3]>(), 1..64)
        ) {
            // Same result as mapping each pixel alone, so pixel order and
            // neighbours don't matter.
            for (&p, l) in pixels.iter().zip(map(&pixels)) {
                prop_assert_eq!(l, map(&[p])[0]);
            }
        }

        #[test]
        fn lighter_grey_never_gets_lower_lstar(a: u8, b: u8) {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let l = map(&[[lo, lo, lo], [hi, hi, hi]]);
            prop_assert!(l[0] <= l[1]);
        }

        #[test]
        fn raising_a_channel_never_lowers_lstar(r: u8, g: u8, b: u8, ch in 0usize..3, d: u8) {
            let mut up = [r, g, b];
            up[ch] = up[ch].saturating_add(d);
            let l = map(&[[r, g, b], up]);
            prop_assert!(l[0] <= l[1], "{:?} {} -> {:?} {}", [r, g, b], l[0], up, l[1]);
        }
    }
}
