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
    /// Equal pixel counts per bucket: cut points fall where the cumulative
    /// histogram reaches each `k / n` fraction of the total (spec F2). Adapts
    /// to the image, so a flat or skewed photo spreads its tones across all
    /// buckets. A lump of pixels in one bin can leave buckets empty (spec §6).
    Quantile,
    /// Jenks natural breaks: the partition of the histogram into `n` contiguous
    /// classes that minimises the within-class variance of L\*, found by exact
    /// dynamic programming over the bins (spec §6, D3). Puts cuts in the gaps
    /// between clusters of tones.
    NaturalBreaks,
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
    pub(crate) fn bucket(&self, bin: usize) -> usize {
        self.cuts().partition_point(|&cut| cut <= bin)
    }
}

/// Computes the cut points that split `hist` into `n` buckets with `algo`.
///
/// Returns [`Error::BucketCount`] if `n` is outside
/// [`MIN_BUCKETS`]`..=`[`MAX_BUCKETS`].
pub fn thresholds(hist: &[u32; BINS], n: u8, algo: Algorithm) -> Result<Thresholds> {
    if !(MIN_BUCKETS..=MAX_BUCKETS).contains(&n) {
        return Err(Error::BucketCount { n });
    }
    Ok(match algo {
        Algorithm::EqualIntervals => equal_intervals(n),
        Algorithm::Quantile => quantile(hist, n),
        Algorithm::NaturalBreaks => natural_breaks(hist, n),
    })
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

/// Equal-pixel-count buckets: cut `k` is the first bin whose cumulative count
/// reaches `k / n` of the total (spec F2).
///
/// A single pass of the cumulative histogram, so cuts come out non-decreasing;
/// two cuts can land on the same bin when one bin holds more than a bucket's
/// share of the pixels, giving an empty bucket (spec §6). A bin exactly on the
/// share boundary goes to the upper bucket, matching [`Thresholds::bucket`]
/// (D12). An empty histogram (no pixels) falls back to [`equal_intervals`] so
/// the study still has `n` buckets.
fn quantile(hist: &[u32; BINS], n: u8) -> Thresholds {
    let total: u64 = hist.iter().map(|&c| u64::from(c)).sum();
    if total == 0 {
        return equal_intervals(n);
    }
    let n64 = u64::from(n);
    let mut cuts = [0; MAX_CUTS];
    // `prefix` is the pixel count in bins `0..bin`, advanced once across all
    // cuts (the targets `k · total / n` only increase, so `bin` never rewinds).
    let mut bin = 0;
    let mut prefix = 0u64;
    for (k, cut) in (1..n).zip(cuts.iter_mut()) {
        let target = u64::from(k) * total; // prefix ≥ k·total/n  ⇔  prefix·n ≥ k·total
        while bin < BINS && prefix * n64 < target {
            prefix += u64::from(hist[bin]);
            bin += 1;
        }
        *cut = bin;
    }
    Thresholds { cuts, n }
}

/// Jenks natural breaks: the split of the histogram into `n` contiguous classes
/// of bins that minimises the total within-class squared deviation of the bin
/// index, weighted by pixel count (spec §6, D3).
///
/// Exact dynamic programming over the `BINS` bins: `best[m][j]` is the least
/// cost of covering bins `0..=j` with `m + 1` classes, and `back[m][j]` the
/// first bin of that last class, from which the `n - 1` cut bins are recovered.
/// Class cost (the within-class deviation) is O(1) from prefix sums of count,
/// `count·bin`, and `count·bin²`. The classes are contiguous and ordered, so
/// the cuts are non-decreasing and monotonicity holds. An empty histogram falls
/// back to [`equal_intervals`]. f64 is used for the sums; two partitions whose
/// costs tie to the last bit could order differently across platforms, but a
/// value study doesn't need bit-exact breaks (cf. D11), and clusters that
/// matter are well separated.
fn natural_breaks(hist: &[u32; BINS], n: u8) -> Thresholds {
    let classes = usize::from(n);
    // Prefix sums over bins `0..=BINS`: counts, Σ(count·bin), Σ(count·bin²).
    let mut p1 = [0.0f64; BINS + 1];
    let mut px = [0.0f64; BINS + 1];
    let mut px2 = [0.0f64; BINS + 1];
    for (b, &c) in hist.iter().enumerate() {
        let (c, x) = (f64::from(c), b as f64);
        p1[b + 1] = p1[b] + c;
        px[b + 1] = px[b] + c * x;
        px2[b + 1] = px2[b] + c * x * x;
    }
    if p1[BINS] == 0.0 {
        return equal_intervals(n);
    }
    // Within-class squared deviation of bins `i..=j`: Σc·x² − (Σc·x)² / Σc.
    let cost = |i: usize, j: usize| -> f64 {
        let sw = p1[j + 1] - p1[i];
        if sw == 0.0 {
            return 0.0;
        }
        let swx = px[j + 1] - px[i];
        (px2[j + 1] - px2[i]) - swx * swx / sw
    };

    let at = |m: usize, j: usize| m * BINS + j;
    let mut best = vec![f64::INFINITY; classes * BINS];
    let mut back = vec![0usize; classes * BINS];
    // One class: bins `0..=j` in a single class.
    for j in 0..BINS {
        best[at(0, j)] = cost(0, j);
    }
    // `m + 1` classes: last class is `i..=j`, the rest cover `0..=i-1`.
    for m in 1..classes {
        for j in m..BINS {
            let (mut lo, mut start) = (f64::INFINITY, m);
            for i in m..=j {
                let c = best[at(m - 1, i - 1)] + cost(i, j);
                if c < lo {
                    (lo, start) = (c, i);
                }
            }
            best[at(m, j)] = lo;
            back[at(m, j)] = start;
        }
    }
    // Walk the back-pointers from the top class down to recover the cut bins.
    let mut cuts = [0; MAX_CUTS];
    let mut j = BINS - 1;
    for m in (1..classes).rev() {
        let start = back[at(m, j)];
        cuts[m - 1] = start;
        j = start - 1;
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

    // Quantile and natural breaks.

    /// A histogram with the given `(bin, count)` pairs set.
    fn hist_from(pairs: &[(usize, u32)]) -> Box<[u32; BINS]> {
        let mut h = Box::new([0u32; BINS]);
        for &(bin, c) in pairs {
            h[bin] = c;
        }
        h
    }

    fn cuts_of(hist: &[u32; BINS], n: u8, algo: Algorithm) -> Vec<usize> {
        thresholds(hist, n, algo).unwrap().cuts().to_vec()
    }

    /// Pixels per bucket, through the same bin → bucket map as posterize.
    fn bucket_counts(hist: &[u32; BINS], t: &Thresholds) -> Vec<u64> {
        let mut counts = vec![0u64; usize::from(t.n())];
        for (bin, &c) in hist.iter().enumerate() {
            counts[t.bucket(bin)] += u64::from(c);
        }
        counts
    }

    #[test]
    fn quantile_splits_at_the_median() {
        // One pixel per bin over L* 0..=40 (bins 0..=400): the median is ~bin
        // 200, where quantiles cut — not bin 500, where equal intervals cut.
        let hist = hist_from(&(0..=400).map(|b| (b, 1)).collect::<Vec<_>>());
        assert_eq!(cuts_of(&hist, 2, Algorithm::Quantile), vec![201]);
        assert_eq!(cuts_of(&hist, 2, Algorithm::EqualIntervals), vec![500]);
    }

    #[test]
    fn quantile_balances_bucket_counts() {
        // 400 pixels spread one per bin → 4 buckets of ~100 each.
        let hist = hist_from(&(0..400).map(|b| (b, 1)).collect::<Vec<_>>());
        let t = thresholds(&hist, 4, Algorithm::Quantile).unwrap();
        assert_eq!(bucket_counts(&hist, &t), vec![100, 100, 100, 100]);
    }

    #[test]
    fn quantile_puts_a_dominant_bin_in_one_bucket() {
        // 90% of pixels in one dark bin: with n = 2 it fills bucket 0 alone and
        // the sparse light tail is bucket 1 (an uneven but unavoidable split).
        let hist = hist_from(&[(100, 90), (900, 10)]);
        let t = thresholds(&hist, 2, Algorithm::Quantile).unwrap();
        assert_eq!(t.bucket(100), 0);
        assert_eq!(t.bucket(900), 1);
        assert_eq!(bucket_counts(&hist, &t), vec![90, 10]);
    }

    #[test]
    fn natural_breaks_finds_the_gaps() {
        // Three tight clusters → one bucket each, cuts in the empty gaps.
        let hist = hist_from(&[(100, 10), (500, 10), (900, 10)]);
        assert_eq!(cuts_of(&hist, 3, Algorithm::NaturalBreaks), vec![101, 501]);
    }

    #[test]
    fn two_tone_separates_under_every_algorithm() {
        // Spec §8: a two-tone image with any algorithm and n = 2 separates.
        let hist = hist_from(&[(200, 5), (800, 5)]);
        for algo in [
            Algorithm::EqualIntervals,
            Algorithm::Quantile,
            Algorithm::NaturalBreaks,
        ] {
            let t = thresholds(&hist, 2, algo).unwrap();
            assert_eq!(t.bucket(200), 0, "{algo:?}");
            assert_eq!(t.bucket(800), 1, "{algo:?}");
        }
    }

    #[test]
    fn empty_histogram_falls_back_to_equal_intervals() {
        let empty = [0u32; BINS];
        for n in MIN_BUCKETS..=MAX_BUCKETS {
            for algo in [Algorithm::Quantile, Algorithm::NaturalBreaks] {
                assert_eq!(cuts_of(&empty, n, algo), equal(n).cuts(), "{algo:?} n={n}");
            }
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

    /// A random histogram of `BINS` bins with small counts.
    fn any_hist(max: u32) -> impl Strategy<Value = Box<[u32; BINS]>> {
        prop::collection::vec(0..=max, BINS).prop_map(|counts| {
            let mut h = Box::new([0u32; BINS]);
            h.copy_from_slice(&counts);
            h
        })
    }

    /// Cuts of any algorithm are well-formed: the right count, non-decreasing,
    /// valid bin indices, and they bucket monotonically (lighter never darker).
    fn assert_cuts_valid(t: &Thresholds, n: u8) {
        assert_eq!(t.cuts().len(), usize::from(n) - 1);
        assert!(t.cuts().windows(2).all(|w| w[0] <= w[1]));
        assert!(t.cuts().iter().all(|&c| (1..=BINS).contains(&c)));
        assert!((0..BINS).map(|b| t.bucket(b)).is_sorted());
    }

    proptest! {
        #[test]
        fn quantile_cuts_are_well_formed(
            hist in any_hist(50),
            n in MIN_BUCKETS..=MAX_BUCKETS,
        ) {
            let t = thresholds(&hist, n, Algorithm::Quantile).unwrap();
            assert_cuts_valid(&t, n);
            // Counts match the output image exactly, and sum to every pixel.
            let counts = bucket_counts(&hist, &t);
            let total: u64 = hist.iter().map(|&c| u64::from(c)).sum();
            prop_assert_eq!(counts.iter().sum::<u64>(), total);
        }
    }

    proptest! {
        // The Jenks DP is O(n · BINS²); keep the case count low so the suite
        // stays quick in debug builds.
        #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]
        #[test]
        fn natural_breaks_cuts_are_well_formed(
            hist in any_hist(20),
            n in MIN_BUCKETS..=6u8,
        ) {
            let t = thresholds(&hist, n, Algorithm::NaturalBreaks).unwrap();
            assert_cuts_valid(&t, n);
        }
    }
}
