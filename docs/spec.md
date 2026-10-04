# Painting Prep — Specification

*Status: draft v0.5 (4 Oct 2026). The repo is the source of truth: update this file when implementation proves it wrong, and log the reason in `docs/decisions.md`.*

## 1. Purpose

A personal phone tool that prepares reference photos for painting. It does two things well:

1. **Grid overlay** for transferring a composition to canvas.
2. **Value studies**: reduce a photo to *n* shades of grey, with a choice of bucketing algorithms. Notan is simply a value study with n = 2 or 3.

Secondary goal: learn the agentic coding workflow (spec → plan → one feature per iteration → verify → commit).

## 2. Context and constraints

- Single user, on an iPhone. All iOS browsers use WebKit, so Safari behaviour is what counts.
- Fully local. No backend, no accounts, and photos never leave the device.
- Delivered as an installable PWA ("Add to Home Screen"), hosted as static files over HTTPS (e.g. GitHub Pages).
- The app only ever reads a copy of the photo. The original in the Photos library is never modified.

## 3. Features

### F1 — Load photo
- Pick a photo from Photos or the camera via `<input type="file" accept="image/*">`.
- The browser decodes the image, not Rust (see D2).
- Orientation must be correct. EXIF rotation is applied by the browser decode path.
- The image is converted to an sRGB RGBA8 buffer via canvas. iPhone photos are often Display P3; the canvas must use `colorSpace: "srgb"` so the core can assume sRGB. Out-of-gamut P3 colours clip; that is acceptable.
- **Large images.** The decoded image is never drawn into a full-size canvas. It is drawn directly into a canvas at the working size (§7): the preview size on load, and the export size on export. Check whether WebKit honours `createImageBitmap` resize options; otherwise use `drawImage` with a destination size.
- **Alpha.** Transparent input is composited over white, once on load, in `prep-core` (`composite_over_white`), so the CLI and the web app give the same result. Output alpha is always 255. Photos have no alpha, so this is a guard for the occasional PNG; other ways of handling transparency are deferred (D19).
- **Errors.** A failed decode or an allocation failure shows a readable message and leaves the app usable. It must not leave a blank screen.
- **Session loss is accepted.** iOS may kill the backgrounded PWA. The loaded photo and the current view are then gone; the user reloads the photo, and settings come back from F5. No image is persisted to survive this (D7, D14).

### F2 — Value study
- Input: n buckets, from 2 to 10.
- Legend strip: a bar of the n output greys, dark to light, with the percentage of pixels in each bucket. It is shown next to the preview; including it in the export is a toggle (default off).
- Algorithm (see §6):
  - **Equal intervals** over the full L\* scale 0–100 (default). The scale is absolute, not stretched to the image's own range, so studies of different photos are comparable. A very dark or very light photo uses only some of the n greys, and that is intended (D10).
  - **Quantiles**: equal pixel counts per bucket.
  - **Natural breaks**: optimal 1-D k-means / Jenks on the L\* histogram.
- Optional pre-blur ("squint"): Gaussian blur on L\* only, with sigma given as % of the image's short side (0 = off). Because it's relative, the preview and export look alike. A separable blur or a triple box-blur approximation is acceptable.
- Output greys, a user setting with two modes (D20):
  - **Evenly spaced:** bucket *i* is displayed as an evenly spaced L\* value from 0 to 100, converted back to sRGB. A painter's value scale, the same for every photo.
  - **Bucket mean:** each bucket is displayed as the mean L\* of its pixels, truer to the photo.
  - Evenly spaced is the default (D20).
- Live preview while adjusting n, algorithm, and blur.
- Non-destructive: every change is recomputed from the original (§5), so going from 3 to 4 values and back to 3 gives the identical study.
- Compare: hold to see the original, or show it side by side. A pinned snapshot (F6) can be the comparison instead of the original.

### F3 — Grid overlay
- Mode A, square cells: choose a reference axis (width or height) and a count k. Cell size = axis length / k. The other axis gets as many whole cells as fit, and a partial last cell is drawn and flagged in the UI.
- Mode B, rows × columns: choose both independently, giving non-square cells that divide the image exactly.
- The outer border lines are drawn as well as the inner lines.
- Optional diagonals per cell.
- Line width is relative to the image size (default ≈ 0.15% of the short side, minimum 1 px). Line style: black, white, mid-grey, or **two-tone** (a light line with a dark outline, visible on any background).
- The grid can be drawn over either the original or the value study.
- Rendering: the core computes line geometry in image-relative units; JS draws it with Canvas 2D in both preview and export (D9).
- Optional cell labels (A–Z, 1–n) along the edges. Nice-to-have, not v1.

### F4 — Export
- Render the current view (study and/or grid) at export resolution.
- Export reuses the thresholds computed for the preview, so the saved image matches what was on screen (D11). Only blur and posterize rerun at export size.
- **Two steps.** Tapping "Export" renders the image and encodes it to a `File`. Then a "Share" button appears. Web Share requires a recent user tap, and a slow render between the tap and `navigator.share` would be rejected (`NotAllowedError`).
- Save through the iOS share sheet using the Web Share API with a file (`navigator.share({ files })`), which offers "Save Image" to Photos. Web Share needs a secure context (HTTPS), so it can't be tested over plain-HTTP LAN.
- `<a download>` is only a fallback, because on iOS it saves to Files, not Photos.

### F5 — Remember settings
- Last-used settings (n, algorithm, blur, output-grey mode, grid mode and parameters, line style, legend toggle, export size cap) persist between sessions.
- Stored as one small versioned JSON object in `localStorage`, read and written inside try/catch. Missing, corrupt, or old-version settings fall back to defaults without an error.
- Images are never persisted.

### F6 — Undo, reset, and snapshots
- **Undo/redo of settings.** Each finished change is one step: a slider released, an option picked, a toggle flipped. Intermediate slider positions are not steps. The history holds settings objects only, never pixels. It lives in memory, is capped at 50 steps, and survives loading a new photo. It is not persisted.
- **Reset to defaults.** One button sets every parameter back to its default. It is an ordinary step, so it can be undone.
- **Snapshots.** "Pin" stores the current settings plus the rendered preview. Up to 4 snapshots, shown as thumbnails.
  - Compare the live view against a snapshot: flip (tap or hold) or side by side, the same as comparing with the original.
  - Tapping a snapshot restores its settings. That is an ordinary undoable step.
  - Snapshots are cleared when a new photo is loaded and are never persisted (D7). Pinning a fifth snapshot asks you to replace one.
- Selecting "original" in the compare control shows the unprocessed photo at any time.

### UI
- Layout: see `docs/ui.md` (sketched for M2). It covers control placement, where the legend goes, safe areas (notch, home indicator), and portrait vs. landscape.
- Undo, redo, reset, and pin controls are always reachable without scrolling.
- Long-press gestures ("hold to compare", debug panel) need `-webkit-touch-callout: none` and `user-select: none` on the target, or iOS shows its own callout and selection.

## 4. Non-goals (v1)

- Native app, App Store, or Tauri wrapper. Revisit later (D1).
- Android or desktop support. It will probably work, but it isn't tested or promised.
- Crop to canvas aspect ratio. This is a candidate for v2.
- RAW/ProRAW input, colour studies, layers, or cloud sync.
- Transparency-aware studies (other background colours, excluding transparent pixels, transparent output). Candidates for later; see D19.
- Persisted undo history or snapshots. Both are in memory only (F6).
- Restoring the working session after iOS kills the app (F1).
- Any JS framework or bundler (React, Vite, etc.) in v1.

## 5. Architecture

```
painting-prep/
├── CLAUDE.md
├── docs/{spec.md, decisions.md, ui.md}
├── crates/
│   ├── core/   # package prep-core: colour math, histogram, quantization, blur, grid geometry
│   ├── cli/    # package prep-cli, binary `prep`: decodes with `image`, calls prep-core
│   └── wasm/   # package prep-wasm: wasm-bindgen bindings over prep-core; no logic of its own
└── web/        # index.html, app.js (vanilla ES modules), style.css,
                # manifest.webmanifest, sw.js, icons, generated pkg/ (not committed)
```

Package names carry a `prep-` prefix. A package named `core` would shadow Rust's built-in `core` crate and break macro-generated `::core::` paths (D8).

Data flow on the phone:

```
Photos picker → browser decode (orientation, sRGB) → canvas at working size → getImageData()
  → RGBA8 buffer → wasm (prep-core) → RGBA8 buffer → putImageData → grid via Canvas 2D → display / export
```

Pipeline stages and caching (preview):

| Stage | Reruns when |
|---|---|
| RGBA → L\* map | a new photo is loaded |
| blur → blurred L\* map + histogram | blur changes |
| thresholds | n or algorithm changes |
| posterize → RGBA | any of the above, or output-grey mode changes |
| grid (Canvas 2D) | grid settings change; never reruns the stages above |

Rules:
- **Non-destructive.** The decoded original RGBA and its L\* map are never modified. Every output is recomputed from them, and undo restores settings, not pixels (D15).
- `prep-core` has no dependency on wasm, web, or image decoding. It works on `&[u8]` RGBA8 plus width and height. This keeps it testable and reusable by the CLI and by any future Tauri or native shell.
- The grid is not drawn into the pixel buffer. `prep-core` returns line geometry; JS draws it with Canvas 2D (anti-aliased, and cheap to redraw), scaled to the target resolution (D9).
- Processing runs on the main thread in v1, with a busy indicator during export. A Web Worker (which needs no COOP/COEP) is the first step if the UI stutters (D13).

### Build configuration (D5)

- Rust target `wasm32-unknown-unknown`, JS glue generated by wasm-bindgen as ES modules.
- The `wasm-bindgen` CLI version must match the `wasm-bindgen` crate version exactly. Pin both.
- Single-threaded. Wasm threads need COOP/COEP headers, which static hosts like GitHub Pages can't set.
- Release profile optimized for speed (`opt-level = 3`, LTO). An optional `wasm-opt` pass comes later.
- `simd128` is a later optimization, adopted only if benchmarks against the scalar version show a real gain.
- Deploy: GitHub Actions builds the wasm and publishes `web/` to GitHub Pages. `web/pkg/` is git-ignored.

### Core API sketch (to be refined in plan mode)

Callers own the buffers so they can be reused between runs. The wasm crate wraps this in a stateful `Processor` that owns the buffers for the pipeline stages above.

```rust
pub struct Image<'a> { rgba: &'a [u8], width: u32, height: u32 } // private; only Image::new(rgba, w, h) -> Result<Image> builds one, after validating

pub fn composite_over_white(rgba: &mut [u8], w: u32, h: u32) -> Result<()>; // once on load, before the buffer becomes the original (D19)
pub fn lightness_map(img: &Image, out: &mut [f32]) -> Result<()>;      // L* per pixel, 0..=100
pub fn blur_lightness(l: &mut [f32], scratch: &mut [f32], w: u32, h: u32, sigma_px: f32) -> Result<()>;
pub fn histogram(l: &[f32], hist: &mut [u32; BINS]);                   // BINS = 1001, bin = round(L* × 10), clamped
pub fn thresholds(hist: &[u32; BINS], n: u8, algo: Algorithm) -> Result<Thresholds>; // n-1 cut points as bin indices; n outside 2..=10 is an error
pub fn bucket_greys(hist: &[u32; BINS], t: &Thresholds, mode: OutputGreys) -> Vec<u8>; // n sRGB greys, dark to light
pub fn bucket_counts(hist: &[u32; BINS], t: &Thresholds) -> Vec<u64>;     // pixels per bucket, for the legend
pub fn posterize(l: &[f32], t: &Thresholds, greys: &[u8], out_rgba: &mut [u8], w: u32, h: u32) -> Result<()>;
pub fn grid_lines(w: u32, h: u32, spec: &GridSpec) -> Vec<Line>;        // geometry only
```

## 6. Algorithms and domain rules

- **Lightness.** Decode sRGB to linear (the piecewise sRGB transfer function, not a 2.2 gamma), compute relative luminance Y = 0.2126 R + 0.7152 G + 0.0722 B, then convert to L\* with the CIE formula (including its linear segment near black). L\* depends only on Y, so a full CIELAB conversion is unnecessary.
- **Never** use the naive RGB average or the Rec.601 luma of gamma-encoded values for lightness.
- **Output greys.** Target L\* → Y → sRGB-encoded grey, with R = G = B.
- **Histogram first.** Every algorithm works on a histogram of L\* (1001 bins of 0.1 L\*; a value goes to bin round(L\* × 10), clamped to 0–1000), never on raw pixels. That makes it fast, deterministic, and easy to test. Natural breaks uses exact dynamic programming over histogram bins.
- **Thresholds are bin indices.** A pixel is first mapped to its histogram bin, and the bucket is chosen by comparing bin indices. Pixels in the same bin always land in the same bucket, and legend percentages match the histogram exactly (D12).
- **Ties and edges.** A bin exactly on a threshold goes to the upper bucket. Empty buckets are allowed; for example, quantiles on a flat image yield fewer distinct greys. An empty bucket keeps its evenly spaced grey in the legend, at 0%.
- **Monotonicity.** A lighter input pixel must never map to a darker bucket than a darker input pixel.
- **Blur scale.** sigma_px = (blur % / 100) × short side of the buffer being processed. So the preview and export use different pixel sigmas for the same visual blur.

## 7. Platform constraints (iOS / WebKit)

- **Canvas size.** iOS Safari has historically capped canvas area at roughly 16.7 MP. Verify on the device. Recent iPhones can shoot 24–48 MP, so decoding goes straight to the working size (F1).
- **Working resolutions.**
  - Preview: downscale to about 2048 px on the long side.
  - Export: cap at a configurable limit, default 12 MP. Tiled full-resolution export is v2.
- **Memory.** Avoid holding more than about three full-size buffers at once. The f32 L\* map, its blur scratch buffer, and the RGBA output each count as one (about 48 MB each at 12 MP). Snapshots (F6) are extra: each holds one preview-size RGBA image (about 12 MB at 2048 × 1536), so 4 add about 50 MB. Keep them as `ImageBitmap`s in JS, not in wasm memory. Reuse wasm memory between runs. Wasm memory never shrinks, so after one 12 MP export it stays large for the rest of the session.
- **Avoid copies.** Build output `ImageData` on a view of wasm linear memory instead of copying the result into a new JS array. Create the view after all allocations for the run, because it becomes invalid when wasm memory grows. Buffer transfer, not computation, is the expected bottleneck.
- **Benchmark.** Time the same core function natively (CLI) and in the browser on the same image, and record the results in `docs/decisions.md`.
- **HEIC.** iOS usually hands the file input a JPEG-compatible image, and the browser decodes HEIC anyway. Verify on the device; no HEIC code in Rust.
- **HTTPS.** Service workers and Web Share need HTTPS. On the LAN over plain HTTP the app works but isn't installable or offline, and it can't export. Test installability and export on the hosted copy.
- **Service worker updates.** Precache a versioned list of files; the cache name includes the build version, and old caches are deleted on `activate`. When a new version is waiting, the app shows a "reload to update" hint instead of switching silently mid-session.
- **Debugging (no Mac available).** Safari Web Inspector for an iPhone needs a Mac, so the development setup is an Ubuntu laptop plus the iPhone:
  - The app includes a debug panel from M2 onward, opened by a long-press on the title. It shows captured `console` output, uncaught errors, image dimensions, and per-step timings.
  - WebKit behaviour is approximated on Ubuntu with Playwright's WebKit build and/or GNOME Web (Epiphany). Chrome and Firefox are not enough, because WebKit-specific bugs are the main risk.
  - Remote inspection from Linux via `ios-webkit-debug-proxy` may work but depends on iOS version. Worth one try; don't rely on it.

## 8. Verification

- `prep-core` unit tests with known values:
  - sRGB 0 → L\* 0, 255 → L\* 100, 119 ≈ L\* 50.
  - Synthetic gradients give expected bucket boundaries.
  - A two-tone image with any algorithm and n = 2 separates the two tones.
  - Equal intervals on a dark-only image (all L\* < 40) with n = 5 uses only the lower buckets.
- Property tests (`proptest`), on the posterize path with blur off and no grid:
  - Monotonicity.
  - Output has at most n distinct greys.
  - Results don't depend on pixel order.
- Property tests for the grid: lines stay within bounds, and the count matches the spec.
- CLI for eyeballing real photos on the desktop: `prep values -n 5 --algo quantile in.jpg out.png`, `prep grid --axis width -k 6 in.jpg out.png`.
- Cross-check L\* against an independent implementation (e.g. `palette` crate or Python `colour-science`) in a test.
- On-device checklist per milestone: orientation, colours, speed, and export to Photos.
- Playwright WebKit smoke test on Ubuntu, from M2: load the page, feed a test image, and check that output pixels match the CLI result for the same input. The test image is a lossless sRGB PNG with no colour profile, because JPEG decoders differ by ±1 per channel.

## 9. Milestones

Each milestone is one or a few Claude Code sessions, committed when green.

| # | Deliverable | Done when |
|---|---|---|
| M0 | Workspace skeleton (`prep-core`, `prep-cli`, `prep-wasm`), CI (fmt, clippy, test) | `cargo test` passes on an empty core |
| M1 | Core: L\*, histogram, equal-interval posterize + CLI | Known-value and property tests pass; CLI output looks right on 3 real photos |
| M2 | UI sketch; wasm bindings + minimal page: pick photo, slider for n, debug panel | Playwright WebKit test passes; works on the iPhone via LAN, including a 48 MP (or largest available) photo |
| M3 | Grid overlay, modes A and B, Canvas 2D rendering | Tests pass; grid visible on dark and light areas |
| M4 | Two-step export via share sheet; PWA manifest + service worker; Actions deploy to Pages | Installed on the iPhone, works offline, image saved to Photos, update hint appears after a redeploy |
| M5 | Quantile and natural-breaks algorithms, blur, compare view, legend strip, remembered settings | All three algorithms selectable; blur visibly removes speckle; settings survive an app restart |
| M6 | Undo/redo, reset to defaults, snapshots (F6) | 3 → 4 → undo gives a pixel-identical study; 4 snapshots flip instantly on the iPhone; reset is undoable |

## 10. Open questions

1. ~~Default output greys~~: resolved in v0.5, evenly spaced (D20).
2. Grid mode A: highlight the partial last cell, or offer to centre the grid instead? **Decide in M3** after trying both.
3. Does WebKit honour `createImageBitmap` resize options, and what is the real canvas limit on the device? **Check in M2.**

Resolved in v0.5: default output greys are evenly spaced; both modes stay selectable (D20).

Resolved in v0.2: no Mac available (→ §7 debugging, D6); n range 2–10 with legend strip (F2); settings persist (F5, D7).

Resolved in v0.4: undo, reset, and snapshots (F6, D15, D16); non-destructive processing stated as a rule.

Resolved in v0.3: crate names (D8); grid drawn with Canvas 2D (D9); equal intervals over the full L\* scale (D10); export reuses preview thresholds (D11); thresholds as bin indices (D12); main thread in v1 (D13); session loss on app kill accepted (D14); two-step export, large-image decode path, service-worker updates, test fixture format.
