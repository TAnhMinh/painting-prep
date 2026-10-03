//! Colour math: sRGB ↔ linear light, luminance Y, and CIE L\* (spec §6).
//!
//! Pipeline for lightness: sRGB byte → linear (per channel) → Y → L\*.
//! The way back, for output greys: L\* → Y → sRGB byte (R = G = B).

/// CIE ε = (6/29)³: below this Y, L\* uses its linear segment.
const EPSILON: f32 = 216.0 / 24389.0;
/// CIE κ = (29/3)³: slope of L\*'s linear segment near black.
const KAPPA: f32 = 24389.0 / 27.0;

/// Decodes an sRGB byte to linear light in 0..=1, with the piecewise sRGB
/// transfer function (not a plain 2.2 gamma).
pub(crate) fn srgb_to_linear(v: u8) -> f32 {
    let c = f32::from(v) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Encodes linear light to the nearest sRGB byte. Input is clamped to 0..=1.
pub(crate) fn linear_to_srgb(linear: f32) -> u8 {
    let l = linear.clamp(0.0, 1.0);
    let c = if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    };
    // In range after the clamp, so the cast can't truncate.
    (c * 255.0).round() as u8
}

/// Relative luminance Y (Rec. 709 / sRGB primaries) from *linear* R, G, B.
pub(crate) fn luminance(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// CIE L\* in 0..=100 from luminance Y in 0..=1 (white Y = 1).
pub(crate) fn y_to_lstar(y: f32) -> f32 {
    if y > EPSILON {
        116.0 * y.cbrt() - 16.0
    } else {
        KAPPA * y
    }
}

/// Luminance Y in 0..=1 from CIE L\* in 0..=100. Inverse of [`y_to_lstar`].
pub(crate) fn lstar_to_y(lstar: f32) -> f32 {
    // κ·ε = 8: the L* where the two segments meet.
    if lstar > KAPPA * EPSILON {
        ((lstar + 16.0) / 116.0).powi(3)
    } else {
        lstar / KAPPA
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// L* of an sRGB colour through our pipeline.
    fn lstar(r: u8, g: u8, b: u8) -> f32 {
        y_to_lstar(luminance(
            srgb_to_linear(r),
            srgb_to_linear(g),
            srgb_to_linear(b),
        ))
    }

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    // Known values (spec §8).

    #[test]
    fn black_is_lstar_0() {
        assert_eq!(lstar(0, 0, 0), 0.0);
    }

    #[test]
    fn white_is_lstar_100() {
        assert!(close(lstar(255, 255, 255), 100.0, 1e-4));
    }

    #[test]
    fn srgb_119_is_about_lstar_50() {
        assert!(close(lstar(119, 119, 119), 50.0, 0.1));
    }

    #[test]
    fn green_is_lighter_than_red_lighter_than_blue() {
        let (r, g, b) = (lstar(255, 0, 0), lstar(0, 255, 0), lstar(0, 0, 255));
        assert!(g > r && r > b, "R {r}, G {g}, B {b}");
        // Reference values (CIELAB, D65): R 53.24, G 87.73, B 32.30.
        assert!(close(r, 53.24, 0.01));
        assert!(close(g, 87.73, 0.01));
        assert!(close(b, 32.30, 0.01));
    }

    #[test]
    fn lstar_50_is_srgb_119() {
        assert_eq!(linear_to_srgb(lstar_to_y(50.0)), 119);
    }

    // Exhaustive over all 256 bytes.

    #[test]
    fn srgb_round_trip_is_exact() {
        for v in 0..=255u8 {
            assert_eq!(linear_to_srgb(srgb_to_linear(v)), v, "byte {v}");
        }
    }

    #[test]
    fn srgb_to_linear_strictly_increases() {
        for v in 0..255u8 {
            assert!(srgb_to_linear(v) < srgb_to_linear(v + 1), "byte {v}");
        }
    }

    // The two segments of each piecewise function meet without a jump.

    #[test]
    fn srgb_segments_meet() {
        let c: f32 = 0.04045;
        let linear_part = c / 12.92;
        let curve_part = ((c + 0.055) / 1.055).powf(2.4);
        assert!(close(linear_part, curve_part, 1e-6));
    }

    #[test]
    fn lstar_segments_meet() {
        let linear_part = KAPPA * EPSILON;
        let curve_part = 116.0 * EPSILON.cbrt() - 16.0;
        assert!(close(linear_part, curve_part, 1e-4));
        assert!(close(linear_part, 8.0, 1e-4));
    }

    #[test]
    fn linear_to_srgb_clamps() {
        assert_eq!(linear_to_srgb(-0.5), 0);
        assert_eq!(linear_to_srgb(1.5), 255);
    }

    // Independent cross-check against the `palette` crate.

    fn palette_lstar(r: u8, g: u8, b: u8) -> f32 {
        use palette::{IntoColor, Lab, Srgb};
        let lab: Lab = Srgb::new(r, g, b).into_format::<f32>().into_color();
        lab.l
    }

    #[test]
    fn matches_palette_for_all_greys() {
        for v in 0..=255u8 {
            let (ours, theirs) = (lstar(v, v, v), palette_lstar(v, v, v));
            assert!(close(ours, theirs, 0.01), "grey {v}: {ours} vs {theirs}");
        }
    }

    #[test]
    fn matches_palette_for_colours() {
        let steps = [0u8, 1, 10, 50, 100, 128, 200, 254, 255];
        for &r in &steps {
            for &g in &steps {
                for &b in &steps {
                    let (ours, theirs) = (lstar(r, g, b), palette_lstar(r, g, b));
                    assert!(
                        close(ours, theirs, 0.01),
                        "({r},{g},{b}): {ours} vs {theirs}"
                    );
                }
            }
        }
    }

    proptest! {
        #[test]
        fn lstar_round_trip(l in 0.0f32..=100.0) {
            let back = y_to_lstar(lstar_to_y(l));
            prop_assert!(close(back, l, 1e-3), "{l} -> {back}");
        }

        #[test]
        fn y_to_lstar_never_decreases(a in 0.0f32..=1.0, b in 0.0f32..=1.0) {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            prop_assert!(y_to_lstar(lo) <= y_to_lstar(hi));
        }

        #[test]
        fn lstar_stays_in_range(r: u8, g: u8, b: u8) {
            let l = lstar(r, g, b);
            prop_assert!((0.0..=100.0 + 1e-4).contains(&l), "({r},{g},{b}) -> {l}");
        }
    }
}
