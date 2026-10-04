//! Bucket thresholds: where a value study cuts the L\* histogram (spec §6).

use crate::histogram::{BINS, lstar_to_bin};
use crate::{Error, Result};

/// Fewest buckets a value study can have (spec F2).
pub const MIN_BUCKETS: u8 = 2;

/// Most buckets a value study can have (spec F2).
pub const MAX_BUCKETS: u8 = 10;

/// Most cut points: one fewer than the most buckets.
const MAX_CUTS: usize = MAX_BUCKETS as usize - 1;

/// How the histogram is split into buckets (spec F2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Algorithm {
    /// Equal intervals over the full L\* scale 0–100, not the image's own
    /// range, so studies of different photos are comparable (D10). Ignores
    /// the histogram.
    #[default]
    EqualIntervals,
}

/// The `n - 1` cut points that split the histogram into `n` buckets.
///
/// Cuts are bin indices in non-decreasing order (D12). A bin exactly on a
/// cut goes to the upper bucket, so bucket `i` is the bins from cut `i - 1`
/// up to, but not including, cut `i`. Equal cuts give empty buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    cuts: [usize; MAX_CUTS],
    n: u8,
}

impl Thresholds {
    /// Number of buckets.
    pub fn n(&self) -> u8 {
        self.n
    }

    /// The `n - 1` cut points, as bin indices in non-decreasing order.
    pub fn cuts(&self) -> &[usize] {
        &self.cuts[..usize::from(self.n) - 1]
    }

    /// The bucket of histogram bin `bin`, in `0..n`: the number of cuts at
    /// or below it, so ties go to the upper bucket (spec §6).
    #[cfg_attr(not(test), expect(dead_code, reason = "used by posterize (M1.8)"))]
    pub(crate) fn bucket(&self, bin: usize) -> usize {
        self.cuts().partition_point(|&cut| cut <= bin)
    }
}

/// Computes the cut points that split `hist` into `n` buckets with `algo`.
///
/// Returns [`Error::BucketCount`] if `n` is outside
/// [`MIN_BUCKETS`]`..=`[`MAX_BUCKETS`].
pub fn thresholds(_hist: &[u32; BINS], n: u8, algo: Algorithm) -> Result<Thresholds> {
    // `_hist` is unused until quantiles and natural breaks arrive (M5).
    if !(MIN_BUCKETS..=MAX_BUCKETS).contains(&n) {
        return Err(Error::BucketCount { n });
    }
    match algo {
        Algorithm::EqualIntervals => Ok(equal_intervals(n)),
    }
}

/// Cut `k` is the bin of L\* `100 × k / n`, for `k` in `1..n`.
///
/// For `n` in 2..=10, `1000 × k / n` is never exactly half a bin, so the
/// rounding in `lstar_to_bin` has no ties to worry about.
fn equal_intervals(n: u8) -> Thresholds {
    let mut cuts = [0; MAX_CUTS];
    for (k, cut) in (1..n).zip(cuts.iter_mut()) {
        *cut = lstar_to_bin(100.0 * f32::from(k) / f32::from(n));
    }
    Thresholds { cuts, n }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn equal(n: u8) -> Thresholds {
        thresholds(&[0; BINS], n, Algorithm::EqualIntervals).unwrap()
    }

    /// The bucket of each L\* value, through the histogram bin as posterize
    /// will do it.
    fn buckets_of(t: &Thresholds, l: &[f32]) -> Vec<usize> {
        l.iter().map(|&v| t.bucket(lstar_to_bin(v))).collect()
    }

    // Known values.

    #[test]
    fn equal_interval_cuts() {
        assert_eq!(equal(2).cuts(), &[500]);
        assert_eq!(equal(3).cuts(), &[333, 667]);
        assert_eq!(equal(4).cuts(), &[250, 500, 750]);
        assert_eq!(equal(5).cuts(), &[200, 400, 600, 800]);
        assert_eq!(
            equal(10).cuts(),
            &[100, 200, 300, 400, 500, 600, 700, 800, 900]
        );
    }

    #[test]
    fn rejects_bucket_count_out_of_range() {
        let hist = [0; BINS];
        for n in [0, 1, 11, u8::MAX] {
            assert_eq!(
                thresholds(&hist, n, Algorithm::EqualIntervals),
                Err(Error::BucketCount { n })
            );
        }
    }

    #[test]
    fn bucket_count_error_is_readable() {
        assert_eq!(
            Error::BucketCount { n: 11 }.to_string(),
            "11 buckets requested, expected 2 to 10"
        );
    }

    #[test]
    fn bin_on_a_cut_goes_to_upper_bucket() {
        let t = equal(2);
        assert_eq!(t.bucket(499), 0);
        assert_eq!(t.bucket(500), 1);
        assert_eq!(buckets_of(&t, &[49.94, 49.96, 50.0]), vec![0, 1, 1]);
    }

    #[test]
    fn end_bins_go_to_end_buckets() {
        for n in MIN_BUCKETS..=MAX_BUCKETS {
            let t = equal(n);
            assert_eq!(t.bucket(0), 0);
            assert_eq!(t.bucket(BINS - 1), usize::from(n) - 1);
        }
    }

    #[test]
    fn synthetic_gradient_gives_expected_boundaries() {
        // L* 0.0, 0.1, ..., 100.0: one value per bin.
        let l: Vec<f32> = (0..BINS).map(|i| i as f32 / 10.0).collect();
        let b = buckets_of(&equal(4), &l);
        let first_of = |bucket| b.iter().position(|&x| x == bucket).unwrap();
        assert_eq!(
            [first_of(0), first_of(1), first_of(2), first_of(3)],
            [0, 250, 500, 750]
        );
    }

    #[test]
    fn two_tone_image_separates_with_two_buckets() {
        assert_eq!(buckets_of(&equal(2), &[20.0, 80.0, 20.0]), vec![0, 1, 0]);
    }

    #[test]
    fn dark_only_image_uses_only_lower_buckets() {
        // Spec §8. All L* < 40 (and below 39.95, which would round into the
        // cut bin 400) stays in buckets 0 and 1 of 5 (D10).
        let l: Vec<f32> = (0..=399).map(|i| i as f32 / 10.0).collect();
        let b = buckets_of(&equal(5), &l);
        assert!(b.iter().all(|&x| x <= 1));
        assert!(b.contains(&0) && b.contains(&1));
    }

    #[test]
    fn ignores_the_histogram() {
        let mut dark = [0; BINS];
        dark[100] = 1000;
        for n in MIN_BUCKETS..=MAX_BUCKETS {
            assert_eq!(
                thresholds(&dark, n, Algorithm::EqualIntervals).unwrap(),
                equal(n)
            );
        }
    }

    // Properties.

    proptest! {
        #[test]
        fn cuts_are_strictly_increasing_inner_bins(n in MIN_BUCKETS..=MAX_BUCKETS) {
            let t = equal(n);
            prop_assert_eq!(t.cuts().len(), usize::from(n) - 1);
            prop_assert!(t.cuts().windows(2).all(|w| w[0] < w[1]));
            prop_assert!(t.cuts().iter().all(|&c| 0 < c && c < BINS - 1));
        }

        #[test]
        fn cut_is_within_half_a_bin_of_its_lstar(n in MIN_BUCKETS..=MAX_BUCKETS) {
            for (k, &cut) in (1..n).zip(equal(n).cuts()) {
                let exact = 1000.0 * f64::from(k) / f64::from(n);
                prop_assert!((cut as f64 - exact).abs() < 0.5, "n={} k={} cut={}", n, k, cut);
            }
        }

        #[test]
        fn every_bucket_is_used_by_the_full_scale(n in MIN_BUCKETS..=MAX_BUCKETS) {
            let t = equal(n);
            let mut used = vec![false; usize::from(n)];
            for bin in 0..BINS {
                used[t.bucket(bin)] = true;
            }
            prop_assert!(used.iter().all(|&u| u));
        }

        #[test]
        fn lighter_never_gets_a_darker_bucket(
            n in MIN_BUCKETS..=MAX_BUCKETS,
            a in -10.0f32..110.0,
            b in -10.0f32..110.0,
        ) {
            let t = equal(n);
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let [blo, bhi] = [lo, hi].map(|v| t.bucket(lstar_to_bin(v)));
            prop_assert!(blo <= bhi);
            prop_assert!(bhi < usize::from(n));
        }
    }
}
