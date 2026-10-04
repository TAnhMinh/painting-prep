//! Value study output: bucket greys and the posterized RGBA image (spec F2).

use crate::colour::{linear_to_srgb, lstar_to_y};
use crate::histogram::{BINS, BINS_PER_LSTAR, lstar_to_bin};
use crate::image::{BYTES_PER_PIXEL, check_rgba_len};
use crate::thresholds::Thresholds;
use crate::{Error, Result};

/// How each bucket's output grey is chosen (spec F2, open question 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputGreys {
    /// Bucket `i` of `n` is L\* `100 × i / (n - 1)`: a painter's value scale
    /// from black to white.
    EvenlySpaced,
    /// Each bucket is the mean L\* of its pixels, truer to the photo. An
    /// empty bucket keeps its evenly spaced grey (spec §6).
    BucketMean,
}

/// The sRGB grey of each of the `t.n()` buckets, darkest bucket first.
///
/// The mean in [`OutputGreys::BucketMean`] uses bin centres, so it is
/// within 0.05 L\* of the pixels' true mean (spec §6: histogram first).
pub fn bucket_greys(hist: &[u32; BINS], t: &Thresholds, mode: OutputGreys) -> Vec<u8> {
    let n = usize::from(t.n());
    let evenly_spaced = |i: usize| 100.0 * i as f32 / (n - 1) as f32;
    let lstars: Vec<f32> = match mode {
        OutputGreys::EvenlySpaced => (0..n).map(evenly_spaced).collect(),
        OutputGreys::BucketMean => {
            // Per bucket: (pixel count, sum of bin × count).
            let mut sums = vec![(0u64, 0u64); n];
            for (bin, &count) in hist.iter().enumerate() {
                let (c, s) = &mut sums[t.bucket(bin)];
                *c += u64::from(count);
                *s += bin as u64 * u64::from(count);
            }
            sums.iter()
                .enumerate()
                .map(|(i, &(c, s))| match c {
                    0 => evenly_spaced(i),
                    _ => (s as f64 / c as f64) as f32 / BINS_PER_LSTAR,
                })
                .collect()
        }
    };
    lstars.into_iter().map(lstar_to_grey).collect()
}

/// The sRGB grey (R = G = B) of an L\* value: L\* → Y → sRGB (spec §6).
fn lstar_to_grey(lstar: f32) -> u8 {
    linear_to_srgb(lstar_to_y(lstar))
}

/// Writes the value study of `l` to `out_rgba`: each pixel becomes the
/// grey of its bucket, opaque.
///
/// A pixel's bucket is found through its histogram bin (D12), so pixels in
/// one bin always share a grey. `greys` is usually from [`bucket_greys`].
///
/// # Errors
/// - [`Error::ZeroSize`] or [`Error::TooLarge`] for an invalid `w × h`.
/// - [`Error::BufferLength`] if `out_rgba` isn't `w × h × 4` bytes, `l`
///   isn't `w × h` values, or `greys` doesn't have one grey per bucket.
pub fn posterize(
    l: &[f32],
    t: &Thresholds,
    greys: &[u8],
    out_rgba: &mut [u8],
    w: u32,
    h: u32,
) -> Result<()> {
    check_rgba_len(out_rgba.len(), w, h)?;
    let (pixels, _) = out_rgba.as_chunks_mut::<BYTES_PER_PIXEL>();
    for (expected, actual) in [(pixels.len(), l.len()), (usize::from(t.n()), greys.len())] {
        if actual != expected {
            return Err(Error::BufferLength { expected, actual });
        }
    }
    // One grey per bin, so each pixel is a bin lookup and a table read.
    let lut: [u8; BINS] = core::array::from_fn(|bin| greys[t.bucket(bin)]);
    for (px, &v) in pixels.iter_mut().zip(l) {
        let g = lut[lstar_to_bin(v)];
        *px = [g, g, g, u8::MAX];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thresholds::{Algorithm, MAX_BUCKETS, MIN_BUCKETS, thresholds};
    use crate::{Image, histogram, lightness_map};
    use proptest::prelude::*;

    fn equal(n: u8) -> Thresholds {
        thresholds(&[0; BINS], n, Algorithm::EqualIntervals).unwrap()
    }

    fn hist_of(l: &[f32]) -> Box<[u32; BINS]> {
        let mut hist = Box::new([0; BINS]);
        histogram(l, &mut hist);
        hist
    }

    /// The full value-study path on an L\* map, one row high: histogram →
    /// equal-interval thresholds → greys → posterize. Returns one grey per
    /// pixel.
    fn study(l: &[f32], n: u8, mode: OutputGreys) -> Vec<u8> {
        let hist = hist_of(l);
        let t = thresholds(&hist, n, Algorithm::EqualIntervals).unwrap();
        let greys = bucket_greys(&hist, &t, mode);
        let mut out = vec![7; l.len() * BYTES_PER_PIXEL];
        let w = u32::try_from(l.len()).unwrap();
        posterize(l, &t, &greys, &mut out, w, 1).unwrap();
        let (pixels, _) = out.as_chunks::<BYTES_PER_PIXEL>();
        pixels
            .iter()
            .map(|&[r, g, b, a]| {
                assert!(r == g && g == b && a == u8::MAX, "{:?}", [r, g, b, a]);
                r
            })
            .collect()
    }

    // Known values.

    #[test]
    fn evenly_spaced_greys() {
        let hist = [0; BINS];
        let greys = |n| bucket_greys(&hist, &equal(n), OutputGreys::EvenlySpaced);
        assert_eq!(greys(2), vec![0, 255]);
        assert_eq!(greys(3), vec![0, 119, 255]);
        assert_eq!(greys(5), vec![0, 59, 119, 185, 255]);
    }

    #[test]
    fn bucket_mean_greys() {
        // Bucket 0 of 2: one pixel at L* 10, three at L* 30 → mean L* 25.
        // Bucket 1: all at L* 75.
        let hist = hist_of(&[10.0, 30.0, 30.0, 30.0, 75.0, 75.0]);
        let greys = bucket_greys(&hist, &equal(2), OutputGreys::BucketMean);
        assert_eq!(greys, vec![59, 185]);
    }

    #[test]
    fn empty_bucket_keeps_evenly_spaced_grey() {
        // Everything at L* 20: buckets 1 and 2 of 3 are empty.
        let hist = hist_of(&[20.0; 4]);
        let greys = bucket_greys(&hist, &equal(3), OutputGreys::BucketMean);
        assert_eq!(greys, vec![48, 119, 255]);
    }

    #[test]
    fn two_tone_image_separates_with_two_buckets() {
        let l = [20.0, 80.0, 80.0, 20.0];
        assert_eq!(
            study(&l, 2, OutputGreys::EvenlySpaced),
            vec![0, 255, 255, 0]
        );
        let [dark, light] = [20.0, 80.0].map(lstar_to_grey);
        assert_eq!(
            study(&l, 2, OutputGreys::BucketMean),
            vec![dark, light, light, dark]
        );
    }

    #[test]
    fn synthetic_gradient_gives_expected_boundaries() {
        // L* 0.0, 0.1, ..., 100.0: one pixel per bin.
        let l: Vec<f32> = (0..BINS).map(|i| i as f32 / 10.0).collect();
        let out = study(&l, 4, OutputGreys::EvenlySpaced);
        let [g0, g1, g2, g3] = [0.0, 100.0 / 3.0, 200.0 / 3.0, 100.0].map(lstar_to_grey);
        let first_of = |g| out.iter().position(|&x| x == g).unwrap();
        assert_eq!(
            [first_of(g0), first_of(g1), first_of(g2), first_of(g3)],
            [0, 250, 500, 750]
        );
    }

    #[test]
    fn dark_only_image_uses_only_lower_greys() {
        // Spec §8: all L* < 40 with n = 5 uses only the lower buckets (D10).
        let l: Vec<f32> = (0..=399).map(|i| i as f32 / 10.0).collect();
        let mut used = study(&l, 5, OutputGreys::EvenlySpaced);
        used.dedup();
        assert_eq!(used, vec![0, 59]);
    }

    #[test]
    fn overwrites_alpha_and_previous_contents() {
        let t = equal(2);
        let mut out = [1, 2, 3, 4, 5, 6, 7, 8];
        posterize(&[10.0, 90.0], &t, &[0, 255], &mut out, 2, 1).unwrap();
        assert_eq!(out, [0, 0, 0, 255, 255, 255, 255, 255]);
    }

    #[test]
    fn rejects_mismatched_buffers() {
        let t = equal(2);
        let greys = [0, 255];
        let mut out = [0; 8];
        assert_eq!(
            posterize(&[50.0, 50.0], &t, &greys, &mut out[..7], 2, 1),
            Err(Error::BufferLength {
                expected: 8,
                actual: 7
            })
        );
        assert_eq!(
            posterize(&[50.0], &t, &greys, &mut out, 2, 1),
            Err(Error::BufferLength {
                expected: 2,
                actual: 1
            })
        );
        assert_eq!(
            posterize(&[50.0, 50.0], &t, &[0, 128, 255], &mut out, 2, 1),
            Err(Error::BufferLength {
                expected: 2,
                actual: 3
            })
        );
        assert_eq!(
            posterize(&[], &t, &greys, &mut [], 0, 1),
            Err(Error::ZeroSize {
                width: 0,
                height: 1
            })
        );
    }

    // Properties, on the full path from RGBA (spec §8).

    fn modes() -> impl Strategy<Value = OutputGreys> {
        prop_oneof![
            Just(OutputGreys::EvenlySpaced),
            Just(OutputGreys::BucketMean)
        ]
    }

    /// L\* maps of random opaque RGB pixels.
    fn lstar_maps() -> impl Strategy<Value = Vec<f32>> {
        prop::collection::vec(any::<[u8; 3]>(), 1..128).prop_map(|rgb| {
            let rgba: Vec<u8> = rgb.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect();
            let img = Image::new(&rgba, u32::try_from(rgb.len()).unwrap(), 1).unwrap();
            let mut l = vec![0.0; rgb.len()];
            lightness_map(&img, &mut l).unwrap();
            l
        })
    }

    proptest! {
        #[test]
        fn lighter_never_gets_a_darker_grey(
            l in lstar_maps(),
            n in MIN_BUCKETS..=MAX_BUCKETS,
            mode in modes(),
        ) {
            let out = study(&l, n, mode);
            let mut by_lstar: Vec<(f32, u8)> = l.into_iter().zip(out).collect();
            by_lstar.sort_by(|a, b| a.0.total_cmp(&b.0));
            prop_assert!(by_lstar.windows(2).all(|w| w[0].1 <= w[1].1), "{:?}", by_lstar);
        }

        #[test]
        fn at_most_n_distinct_greys(
            l in lstar_maps(),
            n in MIN_BUCKETS..=MAX_BUCKETS,
            mode in modes(),
        ) {
            let mut out = study(&l, n, mode);
            out.sort_unstable();
            out.dedup();
            prop_assert!(out.len() <= usize::from(n));
        }

        #[test]
        fn does_not_depend_on_pixel_order(
            l in lstar_maps(),
            n in MIN_BUCKETS..=MAX_BUCKETS,
            mode in modes(),
        ) {
            let out = study(&l, n, mode);
            let mut rev = l.clone();
            rev.reverse();
            let mut rev_out = study(&rev, n, mode);
            rev_out.reverse();
            prop_assert_eq!(out, rev_out);
        }

        #[test]
        fn greys_are_dark_to_light(
            l in lstar_maps(),
            n in MIN_BUCKETS..=MAX_BUCKETS,
            mode in modes(),
        ) {
            // Bucket means are ordered too, because buckets are disjoint,
            // ordered bin ranges. An empty bucket's fallback grey needn't
            // fit between its neighbours' means, so in mean mode only
            // non-empty buckets are checked.
            let hist = hist_of(&l);
            let t = equal(n);
            let greys = bucket_greys(&hist, &t, mode);
            prop_assert_eq!(greys.len(), usize::from(n));
            let mut used = vec![false; usize::from(n)];
            for (bin, &c) in hist.iter().enumerate() {
                used[t.bucket(bin)] |= c > 0;
            }
            let non_empty: Vec<u8> = greys
                .iter()
                .zip(&used)
                .filter(|&(_, &u)| u || mode == OutputGreys::EvenlySpaced)
                .map(|(&g, _)| g)
                .collect();
            prop_assert!(non_empty.windows(2).all(|w| w[0] <= w[1]), "{:?}", greys);
        }
    }
}
