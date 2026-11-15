//! wasm-bindgen bindings over prep-core. Thin wrapper only; all logic lives in
//! prep-core (spec §5). Target: `wasm32-unknown-unknown`, single-threaded (D5).
//!
//! [`Processor`] owns the buffers of the preview pipeline (spec §5) and keeps
//! them between photos and renders, because wasm memory never shrinks
//! (spec §7). JS reads the output in place: it builds an `ImageData` on a
//! view of wasm memory at [`Processor::output_ptr`], after [`Processor::render`]
//! and before the next call that may allocate.

use prep_core::{
    Algorithm, Axis, BINS, BYTES_PER_PIXEL, GridMode, GridSpec, OutputGreys, bucket_counts,
    bucket_greys, composite_over_white, grid_lines, histogram, lightness_map, posterize,
    thresholds,
};
use wasm_bindgen::prelude::*;

/// The value-study pipeline for one photo at a time.
#[wasm_bindgen]
pub struct Processor {
    /// L\* map of the loaded (preview-size) photo; never modified after load (D15).
    l: Vec<f32>,
    /// Scratch L\* map for an export-size render; separate from `l` so export
    /// never touches the preview's map (D15).
    l_export: Vec<f32>,
    hist: Box<[u32; BINS]>,
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
            l_export: Vec::new(),
            hist: Box::new([0; BINS]),
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

    /// Renders the equal-interval study with `n` buckets (2..=10) into the
    /// output buffer. `bucket_mean` picks the output greys (D20).
    #[wasm_bindgen(js_name = render)]
    pub fn render_js(&mut self, n: u8, bucket_mean: bool) -> Result<(), JsError> {
        Ok(self.render(n, bucket_mean)?)
    }

    /// Renders the study at an arbitrary export size (spec F4). `rgba` is the
    /// photo decoded `width × height` at export resolution. It reuses the
    /// **preview's** thresholds and greys — recomputed deterministically from the
    /// retained preview histogram with the same `n`/`bucket_mean`, so the saved
    /// image matches what was on screen (D11, D12) — and posterizes the
    /// export-size L\* map into the separate export buffer. Call after a preview
    /// `render`; read the result via `export_ptr`/`export_len`.
    #[wasm_bindgen(js_name = renderExport)]
    pub fn render_export_js(
        &mut self,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        n: u8,
        bucket_mean: bool,
    ) -> Result<(), JsError> {
        Ok(self.render_export(rgba, width, height, n, bucket_mean)?)
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

    fn render(&mut self, n: u8, bucket_mean: bool) -> prep_core::Result<()> {
        let mode = if bucket_mean {
            OutputGreys::BucketMean
        } else {
            OutputGreys::EvenlySpaced
        };
        let t = thresholds(&self.hist, n, Algorithm::EqualIntervals)?;
        self.greys = bucket_greys(&self.hist, &t, mode);
        self.counts = bucket_counts(&self.hist, &t);
        self.out.resize(self.l.len() * BYTES_PER_PIXEL, 0);
        posterize(
            &self.l,
            &t,
            &self.greys,
            &mut self.out,
            self.width,
            self.height,
        )
    }

    fn render_export(
        &mut self,
        mut rgba: Vec<u8>,
        width: u32,
        height: u32,
        n: u8,
        bucket_mean: bool,
    ) -> prep_core::Result<()> {
        let mode = if bucket_mean {
            OutputGreys::BucketMean
        } else {
            OutputGreys::EvenlySpaced
        };
        // Reuse the preview's histogram → the same thresholds and greys as the
        // on-screen study (D11); equal-interval cut points and the greys are a
        // deterministic function of that histogram (D12).
        let t = thresholds(&self.hist, n, Algorithm::EqualIntervals)?;
        let greys = bucket_greys(&self.hist, &t, mode);
        composite_over_white(&mut rgba, width, height)?;
        let img = prep_core::Image::new(&rgba, width, height)?;
        self.l_export.resize(img.pixel_count(), 0.0);
        lightness_map(&img, &mut self.l_export)?;
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
        p.render(2, false).unwrap();
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
        p.render(2, true).unwrap();
        assert_eq!(p.out, row(&[30, 220]));
    }

    #[test]
    fn export_matches_preview_thresholds_at_a_larger_size() {
        let mut p = Processor::new();
        p.load(row(&[30, 220]), 2, 1).unwrap(); // preview-size photo
        p.render(2, false).unwrap();
        assert_eq!(p.out, row(&[0, 255]));
        // Export a larger buffer of the same two tones: same thresholds (D11).
        p.render_export(row(&[30, 30, 220, 220]), 4, 1, 2, false)
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
        p.render(2, true).unwrap(); // bucket-mean: greys are the photo's tones
        p.render_export(row(&[30, 220]), 2, 1, 2, true).unwrap();
        assert_eq!(p.out_export, row(&[30, 220]));
    }

    #[test]
    fn a_plain_render_after_export_is_unaffected() {
        let mut p = Processor::new();
        p.load(row(&[30, 220]), 2, 1).unwrap();
        p.render(2, false).unwrap();
        p.render_export(row(&[30, 30, 220, 220]), 4, 1, 2, false)
            .unwrap();
        // Export didn't corrupt l/hist/out: a later preview render still works.
        p.render(2, true).unwrap();
        assert_eq!(p.out, row(&[30, 220]));
        assert_eq!(p.output_len(), 8);
    }

    #[test]
    fn transparent_input_is_composited_over_white() {
        let mut p = Processor::new();
        p.load(vec![0, 0, 0, 0, 0, 0, 0, 255], 2, 1).unwrap();
        p.render(2, false).unwrap();
        assert_eq!(p.out, row(&[255, 0]));
    }

    #[test]
    fn bad_bucket_count_is_an_error() {
        let mut p = Processor::new();
        p.load(row(&[0]), 1, 1).unwrap();
        assert_eq!(p.render(11, false), Err(Error::BucketCount { n: 11 }));
    }

    #[test]
    fn render_before_load_is_an_error() {
        let mut p = Processor::new();
        assert_eq!(
            p.render(5, false),
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
        p.render(2, false).unwrap();
        assert!(p.load(row(&[0]), 2, 1).is_err());
        assert_eq!((p.width(), p.height(), p.output_len()), (0, 0, 0));
        assert!(p.render(2, false).is_err());
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
        p.render(5, false).unwrap();
        let (l_cap, out_cap) = (p.l.capacity(), p.out.capacity());
        p.load(row(&[200; 16]), 4, 4).unwrap();
        p.render(5, false).unwrap();
        assert_eq!((p.l.capacity(), p.out.capacity()), (l_cap, out_cap));
        assert_eq!(p.output_len(), 64);
    }
}
