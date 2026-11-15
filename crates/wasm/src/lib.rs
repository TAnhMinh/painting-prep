//! wasm-bindgen bindings over prep-core. Thin wrapper only; all logic lives in
//! prep-core (spec §5). Target: `wasm32-unknown-unknown`, single-threaded (D5).
//!
//! [`Processor`] owns the buffers of the preview pipeline (spec §5) and keeps
//! them between photos and renders, because wasm memory never shrinks
//! (spec §7). JS reads the output in place: it builds an `ImageData` on a
//! view of wasm memory at [`Processor::output_ptr`], after [`Processor::render`]
//! and before the next call that may allocate.

use prep_core::{
    Algorithm, Axis, BINS, BYTES_PER_PIXEL, GridMode, GridSpec, OutputGreys, blur_lightness,
    bucket_counts, bucket_greys, composite_over_white, grid_lines, histogram, lightness_map,
    posterize, thresholds,
};
use wasm_bindgen::prelude::*;

/// The value-study pipeline for one photo at a time.
#[wasm_bindgen]
pub struct Processor {
    /// L\* map of the loaded (preview-size) photo; never modified after load (D15).
    l: Vec<f32>,
    /// Blurred copy of `l` for the current preview render; only built when blur
    /// is on, so the no-blur path never touches it (keeps `l` pristine, D15).
    l_blur: Vec<f32>,
    /// Scratch buffer for the separable blur (preview and export share it).
    blur_scratch: Vec<f32>,
    /// Scratch L\* map for an export-size render; separate from `l` so export
    /// never touches the preview's map (D15).
    l_export: Vec<f32>,
    /// Histogram of the last preview render: of `l` when blur is off, of
    /// `l_blur` when on. Export reuses it so the saved study matches (D11).
    hist: Box<[u32; BINS]>,
    /// Whether `hist` currently reflects a blurred map, so a later unblurred
    /// render knows to rebuild it from `l` (and the no-blur steady state skips
    /// the rebuild, preserving the M4 timing).
    hist_is_blurred: bool,
    /// The last rendered preview study, RGBA8.
    out: Vec<u8>,
    /// The last export-size study, RGBA8. Kept separate so an export never
    /// clobbers `out`, which `paint()` reads at preview dimensions.
    out_export: Vec<u8>,
    greys: Vec<u8>,
    counts: Vec<u64>,
    /// 0 × 0 until a photo is loaded, so `render` fails with `ZeroSize`.
    width: u32,
    height: u32,
}

impl Default for Processor {
    fn default() -> Self {
        Processor {
            l: Vec::new(),
            l_blur: Vec::new(),
            blur_scratch: Vec::new(),
            l_export: Vec::new(),
            hist: Box::new([0; BINS]),
            hist_is_blurred: false,
            out: Vec::new(),
            out_export: Vec::new(),
            greys: Vec::new(),
            counts: Vec::new(),
            width: 0,
            height: 0,
        }
    }
}

#[wasm_bindgen]
impl Processor {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Processor {
        Processor::default()
    }

    /// Loads a photo: `rgba` is `width × height` RGBA8, as read from a
    /// canvas. Composites it over white (D19), then computes its L\* map and
    /// histogram. Clears the last study.
    #[wasm_bindgen(js_name = load)]
    pub fn load_js(&mut self, rgba: Vec<u8>, width: u32, height: u32) -> Result<(), JsError> {
        Ok(self.load(rgba, width, height)?)
    }

    /// Renders the study with `n` buckets (2..=10) into the output buffer.
    /// `algo` is `"equal"`, `"quantile"`, or `"jenks"`; `bucket_mean` picks the
    /// output greys (D20); `blur_pct` is the pre-blur as a percentage of the
    /// short side (0 = off, §6).
    #[wasm_bindgen(js_name = render)]
    pub fn render_js(
        &mut self,
        n: u8,
        bucket_mean: bool,
        algo: &str,
        blur_pct: f32,
    ) -> Result<(), JsError> {
        Ok(self.render(n, bucket_mean, algo_from_str(algo)?, blur_pct)?)
    }

    /// Renders the study at an arbitrary export size (spec F4). `rgba` is the
    /// photo decoded `width × height` at export resolution. It reuses the
    /// **preview's** thresholds and greys — recomputed deterministically from the
    /// retained preview histogram with the same `n`/`bucket_mean`/`algo`, so the
    /// saved image matches what was on screen (D11, D12). Only the blur and
    /// posterize rerun at export size (§6): the blur uses the export-scale sigma
    /// (`blur_pct` × the export short side). Call after a preview `render`; read
    /// the result via `export_ptr`/`export_len`.
    #[wasm_bindgen(js_name = renderExport)]
    #[expect(
        clippy::too_many_arguments,
        reason = "flat scalar args keep the wasm boundary simple; a params struct would export extra types for no gain"
    )]
    pub fn render_export_js(
        &mut self,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        n: u8,
        bucket_mean: bool,
        algo: &str,
        blur_pct: f32,
    ) -> Result<(), JsError> {
        Ok(self.render_export(
            rgba,
            width,
            height,
            n,
            bucket_mean,
            algo_from_str(algo)?,
            blur_pct,
        )?)
    }

    /// Address of the output RGBA8 in wasm memory. Valid until the next
    /// `load` or `render`.
    pub fn output_ptr(&self) -> *const u8 {
        self.out.as_ptr()
    }

    /// Length of the output in bytes: `width × height × 4` after a render.
    pub fn output_len(&self) -> usize {
        self.out.len()
    }

    /// Address of the export-size output RGBA8 in wasm memory. Valid until the
    /// next `render_export` (or any call that grows wasm memory).
    pub fn export_ptr(&self) -> *const u8 {
        self.out_export.as_ptr()
    }

    /// Length of the export output in bytes: export `width × height × 4`.
    pub fn export_len(&self) -> usize {
        self.out_export.len()
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The sRGB grey of each bucket of the last render, darkest first.
    pub fn greys(&self) -> Vec<u8> {
        self.greys.clone()
    }

    /// Pixels per bucket of the last render, darkest first. `f64` so JS gets
    /// plain numbers; exact for any image that fits in wasm memory.
    pub fn counts(&self) -> Vec<f64> {
        self.counts.iter().map(|&c| c as f64).collect()
    }
}

impl Processor {
    fn load(&mut self, mut rgba: Vec<u8>, width: u32, height: u32) -> prep_core::Result<()> {
        // Stay unloaded if any step fails.
        (self.width, self.height) = (0, 0);
        self.out.clear();
        self.greys.clear();
        self.counts.clear();
        composite_over_white(&mut rgba, width, height)?;
        let img = prep_core::Image::new(&rgba, width, height)?;
        self.l.resize(img.pixel_count(), 0.0);
        lightness_map(&img, &mut self.l)?;
        histogram(&self.l, &mut self.hist);
        (self.width, self.height) = (width, height);
        Ok(())
    }

    fn render(
        &mut self,
        n: u8,
        bucket_mean: bool,
        algo: Algorithm,
        blur_pct: f32,
    ) -> prep_core::Result<()> {
        let mode = if bucket_mean {
            OutputGreys::BucketMean
        } else {
            OutputGreys::EvenlySpaced
        };
        // Blur reruns the histogram (spec §5). With blur off, reuse the
        // unblurred histogram computed on load, rebuilding it only if the last
        // render left a blurred one behind (so the no-blur path keeps M4 timing).
        let blurred = blur_pct > 0.0;
        if blurred {
            self.l_blur.clone_from(&self.l);
            self.blur_scratch.resize(self.l.len(), 0.0);
            let sigma = sigma_px(blur_pct, self.width, self.height);
            blur_lightness(
                &mut self.l_blur,
                &mut self.blur_scratch,
                self.width,
                self.height,
                sigma,
            )?;
            histogram(&self.l_blur, &mut self.hist);
            self.hist_is_blurred = true;
        } else if self.hist_is_blurred {
            histogram(&self.l, &mut self.hist);
            self.hist_is_blurred = false;
        }
        let src = if blurred { &self.l_blur } else { &self.l };

        let t = thresholds(&self.hist, n, algo)?;
        self.greys = bucket_greys(&self.hist, &t, mode);
        self.counts = bucket_counts(&self.hist, &t);
        self.out.resize(src.len() * BYTES_PER_PIXEL, 0);
        posterize(src, &t, &self.greys, &mut self.out, self.width, self.height)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors render_export_js; grouping the export parameters would add a struct for no gain"
    )]
    fn render_export(
        &mut self,
        mut rgba: Vec<u8>,
        width: u32,
        height: u32,
        n: u8,
        bucket_mean: bool,
        algo: Algorithm,
        blur_pct: f32,
    ) -> prep_core::Result<()> {
        let mode = if bucket_mean {
            OutputGreys::BucketMean
        } else {
            OutputGreys::EvenlySpaced
        };
        // Reuse the preview's histogram → the same thresholds and greys as the
        // on-screen study (D11); the cut points and greys are a deterministic
        // function of that histogram (D12). With blur on, that histogram is
        // already the blurred-preview one, so the saved study matches.
        let t = thresholds(&self.hist, n, algo)?;
        let greys = bucket_greys(&self.hist, &t, mode);
        composite_over_white(&mut rgba, width, height)?;
        let img = prep_core::Image::new(&rgba, width, height)?;
        self.l_export.resize(img.pixel_count(), 0.0);
        lightness_map(&img, &mut self.l_export)?;
        // Only blur and posterize rerun at export size (§6); the blur uses the
        // export-scale sigma so the squint looks the same as the preview's.
        if blur_pct > 0.0 {
            self.blur_scratch.resize(self.l_export.len(), 0.0);
            let sigma = sigma_px(blur_pct, width, height);
            blur_lightness(
                &mut self.l_export,
                &mut self.blur_scratch,
                width,
                height,
                sigma,
            )?;
        }
        self.out_export
            .resize(self.l_export.len() * BYTES_PER_PIXEL, 0);
        posterize(
            &self.l_export,
            &t,
            &greys,
            &mut self.out_export,
            width,
            height,
        )
    }
}

/// Pixel sigma for a blur percentage: `blur_pct / 100 × short side` (§6).
fn sigma_px(blur_pct: f32, width: u32, height: u32) -> f32 {
    blur_pct / 100.0 * width.min(height) as f32
}

/// Maps the JS algorithm string to the core [`Algorithm`]; an unknown value is a
/// `JsError` (panics off wasm, D22), so this isn't unit-tested here.
fn algo_from_str(algo: &str) -> Result<Algorithm, JsError> {
    match algo {
        "equal" => Ok(Algorithm::EqualIntervals),
        "quantile" => Ok(Algorithm::Quantile),
        "jenks" => Ok(Algorithm::NaturalBreaks),
        other => Err(JsError::new(&format!("unknown algorithm {other:?}"))),
    }
}

/// Grid line geometry for a `width × height` image, flattened to
/// `[x1, y1, x2, y2, …]` in image-relative pixels (spec F3, D9).
///
/// Pure geometry — it needs no loaded photo, so it's a free function rather than
/// a [`Processor`] method. `mode` is `"square"` or `"rowscols"`; `axis` is
/// `"width"` or `"height"` (square mode only). JS unpacks the floats in fours
/// and draws the lines with Canvas 2D, scaled to whatever it's drawing at.
#[wasm_bindgen(js_name = gridLines)]
#[expect(
    clippy::too_many_arguments,
    reason = "flat scalar args keep the wasm boundary simple; grouping them would export an extra struct for no gain"
)]
pub fn grid_lines_js(
    width: u32,
    height: u32,
    mode: &str,
    axis: &str,
    k: u32,
    rows: u32,
    cols: u32,
    diagonals: bool,
) -> Result<Vec<f32>, JsError> {
    let mode = match mode {
        "square" => {
            let axis = match axis {
                "width" => Axis::Width,
                "height" => Axis::Height,
                other => return Err(JsError::new(&format!("unknown axis {other:?}"))),
            };
            GridMode::Square { axis, k }
        }
        "rowscols" => GridMode::RowsCols { rows, cols },
        other => return Err(JsError::new(&format!("unknown grid mode {other:?}"))),
    };
    let lines = grid_lines(width, height, &GridSpec { mode, diagonals })?;
    Ok(lines
        .into_iter()
        .flat_map(|l| [l.x1, l.y1, l.x2, l.y2])
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use prep_core::Error;

    /// One row of opaque greys.
    fn row(greys: &[u8]) -> Vec<u8> {
        greys.iter().flat_map(|&g| [g, g, g, 255]).collect()
    }

    #[test]
    fn two_tone_study_with_two_buckets() {
        let mut p = Processor::new();
        p.load(row(&[30, 220, 220, 220]), 4, 1).unwrap();
        p.render(2, false, Algorithm::EqualIntervals, 0.0).unwrap();
        assert_eq!(p.out, row(&[0, 255, 255, 255]));
        assert_eq!(p.greys(), vec![0, 255]);
        assert_eq!(p.counts(), vec![1.0, 3.0]);
        assert_eq!((p.width(), p.height()), (4, 1));
        assert_eq!(p.output_len(), 16);
    }

    #[test]
    fn bucket_mean_keeps_the_photo_greys() {
        let mut p = Processor::new();
        p.load(row(&[30, 220]), 2, 1).unwrap();
        p.render(2, true, Algorithm::EqualIntervals, 0.0).unwrap();
        assert_eq!(p.out, row(&[30, 220]));
    }

    #[test]
    fn export_matches_preview_thresholds_at_a_larger_size() {
        let mut p = Processor::new();
        p.load(row(&[30, 220]), 2, 1).unwrap(); // preview-size photo
        p.render(2, false, Algorithm::EqualIntervals, 0.0).unwrap();
        assert_eq!(p.out, row(&[0, 255]));
        // Export a larger buffer of the same two tones: same thresholds (D11).
        p.render_export(
            row(&[30, 30, 220, 220]),
            4,
            1,
            2,
            false,
            Algorithm::EqualIntervals,
            0.0,
        )
        .unwrap();
        assert_eq!(p.out_export, row(&[0, 0, 255, 255]));
        assert_eq!(p.export_len(), 16);
        // The preview buffers are untouched (separate export scratch, D15).
        assert_eq!(p.out, row(&[0, 255]));
        assert_eq!((p.width(), p.height()), (2, 1));
    }

    #[test]
    fn export_reuses_bucket_mean_greys_from_the_preview() {
        let mut p = Processor::new();
        p.load(row(&[30, 220]), 2, 1).unwrap();
        p.render(2, true, Algorithm::EqualIntervals, 0.0).unwrap(); // bucket-mean: greys are the photo's tones
        p.render_export(
            row(&[30, 220]),
            2,
            1,
            2,
            true,
            Algorithm::EqualIntervals,
            0.0,
        )
        .unwrap();
        assert_eq!(p.out_export, row(&[30, 220]));
    }

    #[test]
    fn a_plain_render_after_export_is_unaffected() {
        let mut p = Processor::new();
        p.load(row(&[30, 220]), 2, 1).unwrap();
        p.render(2, false, Algorithm::EqualIntervals, 0.0).unwrap();
        p.render_export(
            row(&[30, 30, 220, 220]),
            4,
            1,
            2,
            false,
            Algorithm::EqualIntervals,
            0.0,
        )
        .unwrap();
        // Export didn't corrupt l/hist/out: a later preview render still works.
        p.render(2, true, Algorithm::EqualIntervals, 0.0).unwrap();
        assert_eq!(p.out, row(&[30, 220]));
        assert_eq!(p.output_len(), 8);
    }

    #[test]
    fn transparent_input_is_composited_over_white() {
        let mut p = Processor::new();
        p.load(vec![0, 0, 0, 0, 0, 0, 0, 255], 2, 1).unwrap();
        p.render(2, false, Algorithm::EqualIntervals, 0.0).unwrap();
        assert_eq!(p.out, row(&[255, 0]));
    }

    #[test]
    fn bad_bucket_count_is_an_error() {
        let mut p = Processor::new();
        p.load(row(&[0]), 1, 1).unwrap();
        assert_eq!(
            p.render(11, false, Algorithm::EqualIntervals, 0.0),
            Err(Error::BucketCount { n: 11 })
        );
    }

    #[test]
    fn render_before_load_is_an_error() {
        let mut p = Processor::new();
        assert_eq!(
            p.render(5, false, Algorithm::EqualIntervals, 0.0),
            Err(Error::ZeroSize {
                width: 0,
                height: 0
            })
        );
    }

    #[test]
    fn failed_load_leaves_the_processor_unloaded() {
        let mut p = Processor::new();
        p.load(row(&[0, 0]), 2, 1).unwrap();
        p.render(2, false, Algorithm::EqualIntervals, 0.0).unwrap();
        assert!(p.load(row(&[0]), 2, 1).is_err());
        assert_eq!((p.width(), p.height(), p.output_len()), (0, 0, 0));
        assert!(p.render(2, false, Algorithm::EqualIntervals, 0.0).is_err());
    }

    #[test]
    fn grid_lines_js_flattens_to_quads() {
        // 2 × 2 rows × cols on 100 × 100 → 6 lines → 24 floats.
        let n = grid_lines_js(100, 100, "rowscols", "width", 0, 2, 2, false);
        assert_eq!(n.map(|v| v.len()).unwrap_or(0), 24);
    }

    // The error paths (unknown mode/axis, zero size/count) build a JsError,
    // which panics on non-wasm targets (D22), so they aren't tested here; the
    // geometry errors are covered in prep-core.

    #[test]
    fn buffers_are_reused_across_photos() {
        let mut p = Processor::new();
        p.load(row(&[10; 64]), 8, 8).unwrap();
        p.render(5, false, Algorithm::EqualIntervals, 0.0).unwrap();
        let (l_cap, out_cap) = (p.l.capacity(), p.out.capacity());
        p.load(row(&[200; 16]), 4, 4).unwrap();
        p.render(5, false, Algorithm::EqualIntervals, 0.0).unwrap();
        assert_eq!((p.l.capacity(), p.out.capacity()), (l_cap, out_cap));
        assert_eq!(p.output_len(), 64);
    }

    /// A `w × h` checkerboard of two sRGB greys, opaque.
    fn checker(w: u32, h: u32, dark: u8, light: u8) -> Vec<u8> {
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let g = if (x + y) % 2 == 0 { dark } else { light };
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
        }
        rgba
    }

    #[test]
    fn algorithm_changes_the_output() {
        let mut p = Processor::new();
        // Every tone is below L* 50, so equal intervals (fixed cut at 50) puts
        // them all in the darkest bucket; quantiles split them by pixel count.
        p.load(row(&[60, 70, 80, 90, 100, 110]), 6, 1).unwrap();
        p.render(2, false, Algorithm::EqualIntervals, 0.0).unwrap();
        assert!(p.out.chunks(4).all(|px| px[0] == 0));
        let equal = p.out.clone();
        p.render(2, false, Algorithm::Quantile, 0.0).unwrap();
        assert_ne!(p.out, equal);
    }

    #[test]
    fn blur_changes_a_speckled_render() {
        let mut p = Processor::new();
        p.load(checker(6, 6, 20, 240), 6, 6).unwrap();
        p.render(2, false, Algorithm::EqualIntervals, 0.0).unwrap();
        let sharp = p.out.clone();
        p.render(2, false, Algorithm::EqualIntervals, 40.0).unwrap();
        assert_ne!(p.out, sharp);
        // Turning blur back off restores the sharp render (hist rebuilt from l).
        p.render(2, false, Algorithm::EqualIntervals, 0.0).unwrap();
        assert_eq!(p.out, sharp);
    }

    #[test]
    fn export_reuses_blurred_preview_thresholds() {
        // Same photo, size, and settings (with blur) → export matches preview,
        // because export reuses the blurred-preview histogram (D11) and reruns
        // the same blur at the same scale.
        let photo = checker(6, 6, 30, 210);
        let mut p = Processor::new();
        p.load(photo.clone(), 6, 6).unwrap();
        p.render(3, false, Algorithm::Quantile, 20.0).unwrap();
        let preview = p.out.clone();
        p.render_export(photo, 6, 6, 3, false, Algorithm::Quantile, 20.0)
            .unwrap();
        assert_eq!(p.out_export, preview);
    }
}
