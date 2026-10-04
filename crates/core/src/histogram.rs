//! L\* histogram: the input to every quantization algorithm (spec §6, D3).

/// Number of histogram bins: one per 0.1 L\*, from 0 to 100 inclusive.
pub const BINS: usize = 1001;

/// Bins per unit of L\*.
const BINS_PER_LSTAR: f32 = 10.0;

/// The bin of an L\* value: `round(l × 10)`, clamped to `0..BINS`.
///
/// Bin `i` is centred on L\* `i / 10`, so every value lands within 0.05 L\*
/// of its bin's centre. Values outside 0..=100 (float noise such as
/// 100.00001) go to the end bins, and NaN goes to bin 0. Posterize maps
/// pixels with this same function, so a pixel always falls in the bin it
/// was counted in (D12).
pub(crate) fn lstar_to_bin(l: f32) -> usize {
    // `as` saturates and maps NaN to 0. After the clamp the value is a
    // whole number in 0..=1000, so the cast is exact.
    (l * BINS_PER_LSTAR).round().clamp(0.0, (BINS - 1) as f32) as usize
}

/// Counts the L\* values of `l` into `hist`, one bin per 0.1 L\*.
///
/// `hist` is cleared first, so a buffer can be reused between runs. Counts
/// saturate at `u32::MAX` instead of wrapping; real images are far below
/// that.
pub fn histogram(l: &[f32], hist: &mut [u32; BINS]) {
    hist.fill(0);
    for &v in l {
        let bin = &mut hist[lstar_to_bin(v)];
        *bin = bin.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn hist_of(l: &[f32]) -> Box<[u32; BINS]> {
        let mut h = Box::new([0u32; BINS]);
        histogram(l, &mut h);
        h
    }

    /// The non-empty bins as (bin, count) pairs.
    fn nonzero(h: &[u32; BINS]) -> Vec<(usize, u32)> {
        h.iter()
            .enumerate()
            .filter(|&(_, &c)| c > 0)
            .map(|(i, &c)| (i, c))
            .collect()
    }

    // Known values.

    #[test]
    fn bin_of_known_values() {
        assert_eq!(lstar_to_bin(0.0), 0);
        assert_eq!(lstar_to_bin(50.0), 500);
        assert_eq!(lstar_to_bin(100.0), 1000);
        assert_eq!(lstar_to_bin(12.34), 123);
        assert_eq!(lstar_to_bin(12.36), 124);
    }

    #[test]
    fn bin_edges_round_to_nearest() {
        assert_eq!(lstar_to_bin(0.04), 0);
        assert_eq!(lstar_to_bin(0.06), 1);
        assert_eq!(lstar_to_bin(99.94), 999);
        assert_eq!(lstar_to_bin(99.96), 1000);
    }

    #[test]
    fn out_of_range_and_nan_are_clamped() {
        assert_eq!(lstar_to_bin(-0.001), 0);
        assert_eq!(lstar_to_bin(-50.0), 0);
        assert_eq!(lstar_to_bin(100.0001), 1000);
        assert_eq!(lstar_to_bin(1e30), 1000);
        assert_eq!(lstar_to_bin(f32::INFINITY), 1000);
        assert_eq!(lstar_to_bin(f32::NEG_INFINITY), 0);
        assert_eq!(lstar_to_bin(f32::NAN), 0);
    }

    #[test]
    fn counts_values() {
        let h = hist_of(&[0.0, 50.0, 50.0, 50.02, 100.0, 100.00001]);
        assert_eq!(nonzero(&h), vec![(0, 1), (500, 3), (1000, 2)]);
    }

    #[test]
    fn empty_input_gives_empty_histogram() {
        assert_eq!(nonzero(&hist_of(&[])), vec![]);
    }

    #[test]
    fn clears_previous_contents() {
        let mut h = Box::new([7u32; BINS]);
        histogram(&[25.0], &mut h);
        assert_eq!(nonzero(&h), vec![(250, 1)]);
    }

    #[test]
    fn white_from_lightness_map_lands_in_last_bin() {
        // lightness_map gives white as 100 ± float noise.
        let rgba = [255u8, 255, 255, 255];
        let img = crate::Image::new(&rgba, 1, 1).unwrap();
        let mut l = [0.0f32];
        crate::lightness_map(&img, &mut l).unwrap();
        assert_eq!(nonzero(&hist_of(&l)), vec![(1000, 1)]);
    }

    // Properties.

    proptest! {
        #[test]
        fn counts_sum_to_input_length(l in prop::collection::vec(any::<f32>(), 0..256)) {
            let total: u64 = hist_of(&l).iter().map(|&c| u64::from(c)).sum();
            prop_assert_eq!(total, l.len() as u64);
        }

        #[test]
        fn does_not_depend_on_order(l in prop::collection::vec(-10.0f32..110.0, 0..256)) {
            let mut rev = l.clone();
            rev.reverse();
            let mut sorted = l.clone();
            sorted.sort_by(f32::total_cmp);
            let h = hist_of(&l);
            prop_assert_eq!(&h, &hist_of(&rev));
            prop_assert_eq!(&h, &hist_of(&sorted));
        }

        #[test]
        fn bin_centre_is_within_half_a_bin(l in 0.0f32..=100.0) {
            let centre = lstar_to_bin(l) as f32 / BINS_PER_LSTAR;
            prop_assert!((centre - l).abs() <= 0.05 + 1e-4, "{} -> bin centre {}", l, centre);
        }

        #[test]
        fn higher_lstar_never_gets_lower_bin(a in -10.0f32..110.0, b in -10.0f32..110.0) {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            prop_assert!(lstar_to_bin(lo) <= lstar_to_bin(hi));
        }
    }
}
