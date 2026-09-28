# Decision log

Format: decision, alternatives considered, and why. Newest last. Mark superseded entries rather than deleting them.

## D1 — Deliver as a PWA, not a native/Tauri app (2026-09-27)
- **Alternatives:** Swift native app with Rust static lib; Tauri 2 iOS; Flutter.
- **Why:** The target is an iPhone. Native iOS builds need a Mac and Xcode, and free signing expires after 7 days. A PWA runs in Safari/WebKit, installs from a URL, and needs no toolchain beyond Rust and a static host.
- **Revisit if:** WebKit limits (memory, canvas size, share sheet) block a core feature.

## D2 — Browser decodes images; Rust core works on raw RGBA8 (2026-09-27)
- **Alternatives:** Decode in Rust with the `image` crate inside wasm.
- **Why:** The browser handles EXIF orientation, HEIC, and Display P3 → sRGB conversion. Rust `image` has no HEIC support, and decoding would bloat the wasm. The core stays pure and portable.

## D3 — All quantization algorithms operate on an L\* histogram (2026-09-27)
- **Alternatives:** Per-pixel k-means; sorting pixels for quantiles.
- **Why:** Deterministic, fast on large images, allows exact natural breaks via DP, and makes tests small.

## D4 — Rust workspace with core / cli / wasm crates (2026-09-27)
- **Why:** The CLI gives a fast desktop feedback loop. The wasm crate is a thin adapter. A future native shell can reuse the core unchanged.

## D5 — Build target `wasm32-unknown-unknown`, single-threaded (2026-09-28)
- **Alternatives:** WASI targets (not for browsers); wasm threads via SharedArrayBuffer.
- **Why:** This is the standard browser target. Threads need COOP/COEP headers, which GitHub Pages can't send, and performance needs don't justify the complexity. `simd128` is optional later, adopted only after a benchmark.

## D6 — Debugging without a Mac (2026-09-28)
- **Context:** Only an Ubuntu laptop and an iPhone are available. Safari Web Inspector needs macOS.
- **Decision:** In-app debug panel from M2; Playwright WebKit on Ubuntu as the automated WebKit check; `ios-webkit-debug-proxy` as a best-effort extra.
- **Consequence:** Strengthens D1. Any native or Tauri iOS build would need macOS (borrowed or cloud).

## D7 — Persist settings, never images (2026-09-28)
- **Why:** Re-entering n and grid parameters every time is tedious. A small versioned JSON object in `localStorage` is enough. Images stay in memory only, for privacy and storage limits.

## D8 — Package names `prep-core`, `prep-cli`, `prep-wasm` (2026-09-28)
- **Alternatives:** Packages named `core`, `cli`, `wasm`, matching the directories.
- **Why:** A package named `core` shadows Rust's built-in `core` crate for every crate that depends on it, which breaks macro-generated `::core::` paths (serde, proptest). Generic names also give generic wasm output files (`wasm_bg.wasm`). Directories stay `crates/core` etc.; the CLI binary is `prep`.

## D9 — Grid is geometry from core, drawn with Canvas 2D (2026-09-28)
- **Supersedes:** spec v0.2 §5 rule "grid lines are drawn by the core into the buffer".
- **Alternatives:** Core rasterizes lines into the RGBA buffer.
- **Why:** Canvas 2D anti-aliases lines (including diagonals) for free and redraws without reprocessing the image. "Grid over original" doesn't need an extra full-size buffer. Preview and export stay consistent because both draw the same image-relative geometry at their own scale. The geometry stays in core, so it stays unit- and property-tested.

## D10 — Equal intervals span the full L\* scale 0–100 (2026-09-28)
- **Alternatives:** Stretch the intervals over the image's own min–max (or percentile-clipped) range.
- **Why:** Studies must be comparable across photos. An overall dark or light photo should look dark or light in its study, even if that leaves some of the n greys unused. Quantiles and natural breaks already adapt to the image.

## D11 — Export reuses the preview's thresholds (2026-09-28)
- **Alternatives:** Recompute the histogram and thresholds at export resolution.
- **Why:** Quantiles and natural breaks computed on the downscaled preview differ slightly from full-resolution ones. Reusing them means the saved image matches what was on screen. Blur and posterize still rerun at export size.

## D12 — Thresholds are histogram bin indices (2026-09-28)
- **Alternatives:** f32 L\* cut points compared against each pixel's L\*.
- **Why:** Mapping pixels to bins first makes the tie rule exact (no float-equality cases), guarantees that pixels in one bin share a bucket, and makes legend percentages match the histogram exactly.

## D13 — Processing on the main thread in v1 (2026-09-28)
- **Alternatives:** Run wasm in a Web Worker (with OffscreenCanvas or transferred buffers).
- **Why:** Simpler, and the ~2048 px preview is expected to be fast enough. Export shows a busy indicator. A worker needs no COOP/COEP, so it is independent of D5 and is the first step if the UI stutters.

## D14 — Session loss on iOS app kill is accepted (2026-09-28)
- **Context:** iOS may kill the backgrounded PWA, which drops the in-memory photo and current view.
- **Alternatives:** Keep the current photo in IndexedDB as session restore (an exception to D7).
- **Why:** The original is never at risk. It stays in the Photos library, and the app only reads a copy. Reloading the photo is cheap, and settings come back through F5. Keeps D7 intact.

## D15 — Non-destructive processing; undo restores settings, not pixels (2026-09-28)
- **Context:** The user wants to try 3 values, then 4, and still get back to earlier results and the original.
- **Alternatives:** A pixel-based undo stack of rendered images.
- **Why:** The pipeline already recomputes everything from the untouched original, so a settings history is enough. It is tiny (JSON-sized objects), deterministic (undo gives pixel-identical output), and needs no image storage. Replaces "undo history" in the v1 non-goals. Reset to defaults is an ordinary undoable step.

## D16 — Snapshots hold settings plus a rendered preview, in memory, max 4 (2026-09-28)
- **Alternatives:** Settings only, recomputed on demand (less memory, slower flip); unlimited snapshots; persisting them in IndexedDB.
- **Why:** Flipping between versions should be instant to be useful for comparison, so each snapshot keeps its preview as an `ImageBitmap` (~12 MB). A cap of 4 keeps memory bounded on iOS. Not persisted, consistent with D7 and D14. Cleared when a new photo is loaded.
