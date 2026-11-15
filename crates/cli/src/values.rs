//! `prep values`: the value-study pipeline on an RGBA buffer, without file
//! I/O, so it can be tested in memory.

use std::fmt::Write as _;

use prep_core::{
    Algorithm, BINS, BYTES_PER_PIXEL, Error, Image, OutputGreys, Result, blur_lightness,
    bucket_counts, bucket_greys, histogram, lightness_map, posterize, thresholds,
};

/// The L\* map and histogram of an image: computed once, then shared by
/// every study of it.
pub struct Analysis {
    l: Vec<f32>,
    hist: Box<[u32; BINS]>,
    width: u32,
    height: u32,
}

/// One value study: the output image and its legend.
pub struct Study {
    pub algo: Algorithm,
    pub mode: OutputGreys,
    /// Pre-blur applied, as a percentage of the short side (0 = none).
    pub blur_pct: f32,
    pub n: u8,
    pub rgba: Vec<u8>,
    pub greys: Vec<u8>,
    pub counts: Vec<u64>,
}

/// Analyses an opaque RGBA image (already composited over white, D19).
pub fn analyse(rgba: &[u8], width: u32, height: u32) -> Result<Analysis> {
    let img = Image::new(rgba, width, height)?;
    let mut l = vec![0.0; img.pixel_count()];
    lightness_map(&img, &mut l)?;
    let mut hist = Box::new([0; BINS]);
    histogram(&l, &mut hist);
    Ok(Analysis {
        l,
        hist,
        width,
        height,
    })
}

impl Analysis {
    /// A study with `n` buckets, `algo` bucketing, `mode` greys, and an optional
    /// pre-blur of `blur_pct`% of the short side (0 = none, §6).
    ///
    /// When blurred, the squint is applied to a copy of the L\* map and the
    /// histogram rebuilt from it, so thresholds, greys, and counts all reflect
    /// the blurred image.
    pub fn study(&self, n: u8, algo: Algorithm, mode: OutputGreys, blur_pct: f32) -> Result<Study> {
        let blurred = if blur_pct > 0.0 {
            let mut l = self.l.clone();
            let mut scratch = vec![0.0; l.len()];
            let sigma = blur_pct / 100.0 * self.width.min(self.height) as f32;
            blur_lightness(&mut l, &mut scratch, self.width, self.height, sigma)?;
            let mut hist = Box::new([0; BINS]);
            histogram(&l, &mut hist);
            Some((l, hist))
        } else {
            None
        };
        let (l, hist): (&[f32], &[u32; BINS]) = match &blurred {
            Some((l, hist)) => (l, hist),
            None => (&self.l, &self.hist),
        };

        let t = thresholds(hist, n, algo)?;
        let greys = bucket_greys(hist, &t, mode);
        let counts = bucket_counts(hist, &t);
        let mut rgba = vec![0; l.len() * BYTES_PER_PIXEL];
        posterize(l, &t, &greys, &mut rgba, self.width, self.height)?;
        Ok(Study {
            algo,
            mode,
            blur_pct,
            n,
            rgba,
            greys,
            counts,
        })
    }
}

/// Joins same-size RGBA panels left to right. Returns the strip and its
/// width.
pub fn side_by_side(panels: &[&[u8]], width: u32, height: u32) -> Result<(Vec<u8>, u32)> {
    let too_large = Error::TooLarge { width, height };
    let strip_width = u32::try_from(panels.len())
        .ok()
        .and_then(|k| width.checked_mul(k))
        .ok_or(too_large)?;
    for &p in panels {
        Image::new(p, width, height)?;
    }
    let row = width as usize * BYTES_PER_PIXEL;
    let mut out = Vec::with_capacity(panels.iter().map(|p| p.len()).sum());
    for y in 0..height as usize {
        for p in panels {
            out.extend_from_slice(&p[y * row..(y + 1) * row]);
        }
    }
    Ok((out, strip_width))
}

/// The legend of a study as a text table: each bucket's grey and its share
/// of the pixels, darkest first.
pub fn legend(study: &Study) -> String {
    let algo = match study.algo {
        Algorithm::EqualIntervals => "equal intervals",
        Algorithm::Quantile => "quantiles",
        Algorithm::NaturalBreaks => "natural breaks",
    };
    let name = match study.mode {
        OutputGreys::EvenlySpaced => "evenly spaced",
        OutputGreys::BucketMean => "bucket mean",
    };
    let blur = if study.blur_pct > 0.0 {
        format!(", blur {}%", study.blur_pct)
    } else {
        String::new()
    };
    let total = study.counts.iter().sum::<u64>().max(1);
    let mut s = format!(
        "{algo}, {name} greys, n = {}{blur}\nbucket     L*  sRGB  pixels\n",
        study.n
    );
    for (i, (&g, &c)) in study.greys.iter().zip(&study.counts).enumerate() {
        let pct = 100.0 * c as f64 / total as f64;
        // Writing to a String can't fail.
        let _ = writeln!(s, "{i:>6} {:>6.1} {g:>5} {pct:>6.1}%", grey_lstar(g));
    }
    s
}

/// The L\* of an sRGB grey, through the same path as the photo's pixels.
fn grey_lstar(g: u8) -> f32 {
    let mut l = [0.0];
    match Image::new(&[g, g, g, u8::MAX], 1, 1).and_then(|img| lightness_map(&img, &mut l)) {
        Ok(()) => l[0],
        Err(_) => f32::NAN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One row of opaque greys.
    fn row(greys: &[u8]) -> Vec<u8> {
        greys.iter().flat_map(|&g| [g, g, g, 255]).collect()
    }

    #[test]
    fn two_tone_study_with_two_buckets() {
        let rgba = row(&[30, 220, 220, 220]);
        let a = analyse(&rgba, 4, 1).unwrap();
        let s = a
            .study(2, Algorithm::EqualIntervals, OutputGreys::EvenlySpaced, 0.0)
            .unwrap();
        assert_eq!(s.rgba, row(&[0, 255, 255, 255]));
        assert_eq!(s.greys, vec![0, 255]);
        assert_eq!(s.counts, vec![1, 3]);
    }

    #[test]
    fn bucket_mean_study_keeps_the_photo_greys() {
        let rgba = row(&[30, 220]);
        let s = analyse(&rgba, 2, 1)
            .unwrap()
            .study(2, Algorithm::EqualIntervals, OutputGreys::BucketMean, 0.0)
            .unwrap();
        assert_eq!(s.rgba, row(&[30, 220]));
    }

    #[test]
    fn rejects_bad_bucket_count() {
        let a = analyse(&row(&[0]), 1, 1).unwrap();
        assert_eq!(
            a.study(
                11,
                Algorithm::EqualIntervals,
                OutputGreys::EvenlySpaced,
                0.0
            )
            .err(),
            Some(Error::BucketCount { n: 11 })
        );
    }

    #[test]
    fn panels_are_joined_left_to_right() {
        // Two 2 × 2 panels.
        let a = row(&[1, 2, 3, 4]);
        let b = row(&[5, 6, 7, 8]);
        let (out, w) = side_by_side(&[&a, &b], 2, 2).unwrap();
        assert_eq!(w, 4);
        assert_eq!(out, row(&[1, 2, 5, 6, 3, 4, 7, 8]));
    }

    #[test]
    fn mismatched_panel_is_an_error() {
        let a = row(&[1, 2]);
        let b = row(&[1]);
        assert_eq!(
            side_by_side(&[&a, &b], 2, 1),
            Err(Error::BufferLength {
                expected: 8,
                actual: 4
            })
        );
    }

    #[test]
    fn too_wide_strip_is_an_error() {
        assert_eq!(
            side_by_side(&[&[], &[], &[]], u32::MAX / 2, 1),
            Err(Error::TooLarge {
                width: u32::MAX / 2,
                height: 1
            })
        );
    }

    #[test]
    fn legend_table() {
        let s = Study {
            algo: Algorithm::EqualIntervals,
            mode: OutputGreys::EvenlySpaced,
            blur_pct: 0.0,
            n: 3,
            rgba: vec![],
            greys: vec![0, 119, 255],
            counts: vec![1, 0, 3],
        };
        assert_eq!(
            legend(&s),
            "equal intervals, evenly spaced greys, n = 3\n\
             bucket     L*  sRGB  pixels\n\
             \x20    0    0.0     0   25.0%\n\
             \x20    1   50.0   119    0.0%\n\
             \x20    2  100.0   255   75.0%\n"
        );
    }

    #[test]
    fn runs_under_every_algorithm() {
        let a = analyse(&row(&[10, 40, 80, 120, 160, 200, 240, 255]), 8, 1).unwrap();
        for algo in [
            Algorithm::EqualIntervals,
            Algorithm::Quantile,
            Algorithm::NaturalBreaks,
        ] {
            let s = a.study(5, algo, OutputGreys::EvenlySpaced, 0.0).unwrap();
            assert_eq!(s.greys.len(), 5, "{algo:?}");
            assert_eq!(s.counts.iter().sum::<u64>(), 8, "{algo:?}");
        }
    }

    #[test]
    fn blur_removes_a_thin_bright_line() {
        // 9×9 dark field with one bright column. Without blur that column is its
        // own (light) bucket; a squint washes it into the dark field, so fewer
        // pixels land in the light bucket.
        let (w, h) = (9u32, 9u32);
        let mut rgba = Vec::new();
        for _y in 0..h {
            for x in 0..w {
                let g = if x == 4 { 220 } else { 70 };
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
        }
        let a = analyse(&rgba, w, h).unwrap();
        let sharp = a
            .study(2, Algorithm::EqualIntervals, OutputGreys::EvenlySpaced, 0.0)
            .unwrap();
        let blurred = a
            .study(
                2,
                Algorithm::EqualIntervals,
                OutputGreys::EvenlySpaced,
                30.0,
            )
            .unwrap();
        assert_eq!(sharp.counts[1], 9); // the bright column
        assert!(blurred.counts[1] < sharp.counts[1], "{:?}", blurred.counts);
    }
}
