//! Pre-blur ("squint") of the L\* map (spec F2, §6).
//!
//! A Gaussian blur is approximated by three successive box blurs, which is
//! `O(pixels)` regardless of radius — cheap even for a strong squint on a large
//! buffer (spec §6 permits the box-blur approximation). Each box pass is
//! separable (horizontal then vertical) and clamps to the edge. The blur runs
//! on the L\* map only, before the histogram/threshold/posterize stages.

use crate::{Error, Result};

/// Blurs the L\* map `l` in place, using `scratch` (same length as `l`) as the
/// ping-pong buffer. `sigma_px` is the Gaussian sigma in pixels; the caller
/// derives it from the blur percentage (blur% / 100 × the short side, §6).
///
/// `sigma_px <= 0` (and NaN) is a no-op, so blur "off" leaves the map untouched.
/// Borders use clamp-to-edge.
///
/// # Errors
/// - [`Error::ZeroSize`] if `w` or `h` is zero.
/// - [`Error::TooLarge`] if `w × h` overflows `usize`.
/// - [`Error::BufferLength`] if `l` isn't `w × h` values, or `scratch` isn't the
///   same length as `l`.
pub fn blur_lightness(
    l: &mut [f32],
    scratch: &mut [f32],
    w: u32,
    h: u32,
    sigma_px: f32,
) -> Result<()> {
    if w == 0 || h == 0 {
        return Err(Error::ZeroSize {
            width: w,
            height: h,
        });
    }
    let pixels = (w as usize)
        .checked_mul(h as usize)
        .ok_or(Error::TooLarge {
            width: w,
            height: h,
        })?;
    if l.len() != pixels {
        return Err(Error::BufferLength {
            expected: pixels,
            actual: l.len(),
        });
    }
    if scratch.len() != l.len() {
        return Err(Error::BufferLength {
            expected: l.len(),
            actual: scratch.len(),
        });
    }
    // No-op for "off", negatives, and NaN.
    if sigma_px <= 0.0 || sigma_px.is_nan() {
        return Ok(());
    }

    let (w, h) = (w as usize, h as usize);
    // Each pass is a horizontal then a vertical box blur, so the result lands
    // back in `l` and `scratch` is only ever the intermediate.
    for &r in &box_radii(sigma_px) {
        box_blur_h(l, scratch, w, h, r);
        box_blur_v(scratch, l, w, h, r);
    }
    Ok(())
}

/// The three box-blur radii that approximate a Gaussian of the given sigma,
/// after Kovesi, *Fast Almost-Gaussian Filtering* (2010). Widths are odd so
/// each box is centred; a radius can be 0 (an identity pass) for a tiny sigma.
fn box_radii(sigma: f32) -> [usize; 3] {
    const N: f64 = 3.0;
    let sigma = f64::from(sigma);
    // Ideal (real-valued) averaging-filter width for N boxes.
    let w_ideal = (12.0 * sigma * sigma / N + 1.0).sqrt();
    // Nearest lower odd width, and the next odd width up.
    let mut wl = w_ideal.floor() as i64;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    // How many of the N boxes use the lower width to best match the sigma.
    let wl_f = wl as f64;
    let m = ((12.0 * sigma * sigma - N * wl_f * wl_f - 4.0 * N * wl_f - 3.0 * N)
        / (-4.0 * wl_f - 4.0))
        .round() as i64;
    let mut radii = [0usize; 3];
    for (i, radius) in radii.iter_mut().enumerate() {
        let width = if (i as i64) < m { wl } else { wu };
        *radius = ((width - 1) / 2).max(0) as usize;
    }
    radii
}

/// Horizontal box blur of radius `r`, reading `src` row by row into `dst`,
/// with a sliding window sum and clamp-to-edge.
fn box_blur_h(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize) {
    debug_assert_eq!(src.len(), w * h);
    let win = (2 * r + 1) as f32;
    let n = w as i64;
    let ri = r as i64;
    for (s, d) in src.chunks_exact(w).zip(dst.chunks_exact_mut(w)) {
        let mut sum = 0.0f32;
        for i in -ri..=ri {
            sum += s[i.clamp(0, n - 1) as usize];
        }
        d[0] = sum / win;
        for (x, dx) in d.iter_mut().enumerate().skip(1) {
            let add = (x as i64 + ri).clamp(0, n - 1) as usize;
            let sub = (x as i64 - ri - 1).clamp(0, n - 1) as usize;
            sum += s[add] - s[sub];
            *dx = sum / win;
        }
    }
}

/// Vertical box blur of radius `r`, reading `src` column by column into `dst`,
/// with a sliding window sum and clamp-to-edge.
fn box_blur_v(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize) {
    debug_assert_eq!(src.len(), w * h);
    let win = (2 * r + 1) as f32;
    let n = h as i64;
    let ri = r as i64;
    for x in 0..w {
        let mut sum = 0.0f32;
        for i in -ri..=ri {
            sum += src[i.clamp(0, n - 1) as usize * w + x];
        }
        dst[x] = sum / win;
        for y in 1..h {
            let add = (y as i64 + ri).clamp(0, n - 1) as usize;
            let sub = (y as i64 - ri - 1).clamp(0, n - 1) as usize;
            sum += src[add * w + x] - src[sub * w + x];
            dst[y * w + x] = sum / win;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn blur(l: &[f32], w: u32, h: u32, sigma: f32) -> Vec<f32> {
        let mut out = l.to_vec();
        let mut scratch = vec![0.0; l.len()];
        blur_lightness(&mut out, &mut scratch, w, h, sigma).unwrap();
        out
    }

    #[test]
    fn flat_image_is_unchanged() {
        let l = vec![42.0f32; 6 * 4];
        for sigma in [0.5, 2.0, 10.0] {
            for &v in &blur(&l, 6, 4, sigma) {
                assert!((v - 42.0).abs() < 1e-3, "sigma {sigma}: {v}");
            }
        }
    }

    #[test]
    fn non_positive_sigma_is_a_no_op() {
        let l: Vec<f32> = (0..12).map(|i| i as f32).collect();
        for sigma in [0.0, -1.0, f32::NAN] {
            assert_eq!(blur(&l, 4, 3, sigma), l, "sigma {sigma}");
        }
    }

    #[test]
    fn impulse_spreads_symmetrically_and_conserves_sum() {
        // 9×9, bright centre. The total spread radius (1+1+2 = 4) reaches the
        // edges but not beyond, so no mass is lost to clamping.
        let (w, h) = (9usize, 9usize);
        let mut l = vec![0.0f32; w * h];
        l[4 * w + 4] = 100.0;
        let out = blur(&l, w as u32, h as u32, 2.0);

        let sum: f32 = out.iter().sum();
        assert!((sum - 100.0).abs() < 1e-2, "sum {sum}");

        // 4-fold symmetry.
        for y in 0..h {
            for x in 0..w {
                assert!((out[y * w + x] - out[y * w + (w - 1 - x)]).abs() < 1e-4);
                assert!((out[y * w + x] - out[(h - 1 - y) * w + x]).abs() < 1e-4);
            }
        }

        // The centre is the brightest, and it spread to its neighbours.
        let centre = out[4 * w + 4];
        let brightest = out.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(centre < 100.0 && centre == brightest);
        assert!(out[4 * w + 5] > 0.0);
    }

    #[test]
    fn output_stays_within_input_range() {
        let l = vec![10.0, 90.0, 10.0, 90.0, 10.0, 90.0, 10.0, 90.0, 10.0];
        let out = blur(&l, 3, 3, 3.0);
        assert!(out.iter().all(|&v| (10.0..=90.0).contains(&v)), "{out:?}");
    }

    #[test]
    fn rejects_bad_sizes_and_buffers() {
        let mut l = vec![0.0; 4];
        let mut scratch = vec![0.0; 4];
        assert_eq!(
            blur_lightness(&mut l, &mut scratch, 0, 4, 1.0),
            Err(Error::ZeroSize {
                width: 0,
                height: 4
            })
        );
        // 3×3 needs 9 values, l has 4.
        assert_eq!(
            blur_lightness(&mut l, &mut scratch, 3, 3, 1.0),
            Err(Error::BufferLength {
                expected: 9,
                actual: 4
            })
        );
        // scratch shorter than l.
        let mut scratch_short = vec![0.0; 3];
        assert_eq!(
            blur_lightness(&mut l, &mut scratch_short, 2, 2, 1.0),
            Err(Error::BufferLength {
                expected: 4,
                actual: 3
            })
        );
    }

    proptest! {
        #[test]
        fn never_overshoots_the_input_range(
            vals in prop::collection::vec(0.0f32..=100.0, 1..40),
            sigma in 0.0f32..20.0,
        ) {
            let w = vals.len() as u32;
            let out = blur(&vals, w, 1, sigma);
            let lo = vals.iter().copied().fold(f32::INFINITY, f32::min);
            let hi = vals.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            prop_assert!(out.iter().all(|&v| v >= lo - 1e-3 && v <= hi + 1e-3), "{out:?} not in [{lo},{hi}]");
        }
    }
}
