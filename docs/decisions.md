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

## D17 — Pinned rustup toolchain per repo; system Rust stays the default (2026-09-28)
- **Context:** The dev machine had only Ubuntu's apt Rust 1.93.1, which has no rustfmt, clippy, or wasm32 target. The same machine builds the user's work projects (Suricata-based), which expect that apt Rust.
- **Decision:** Install rustup with `--default-toolchain none`, link the apt Rust as the `system` toolchain, and make it the default. This repo pins `1.98.1` (then the current stable) in `rust-toolchain.toml`, with rustfmt, clippy, and `wasm32-unknown-unknown`. CI installs from the same file.
- **Alternatives:** apt rustfmt/clippy packages (no pin, CI drift, no wasm std); rustup with latest stable as the global default (would silently change the compiler for Suricata builds).
- **Why:** Local and CI get an identical compiler and lint set. Other projects on the machine are unaffected.
- **Also in M0:** CI (GitHub Actions) runs fmt, clippy, test, plus `cargo check -p prep-wasm --target wasm32-unknown-unknown`, so a core change that breaks wasm is caught before M2. Dependencies are added when first used: clap, image, and proptest in M1, and wasm-bindgen in M2.
- **Open:** `~/.cargo/config.toml` routes crates.io through a work mirror. Decide in M1 whether this repo overrides it. *Resolved by D18.*

## D18 — Work crates.io mirror scoped to work checkouts; this repo uses crates.io (2026-10-03)
- **Context:** `~/.cargo/config.toml` replaced crates.io with a work mirror. It applied to this personal repo too, and the mirror isn't always reachable from the dev machine.
- **Decision:** The user moved the mirror block out of `~/.cargo/config.toml` into `.cargo/config.toml` in each work project folder (template: `~/.cargo/config.toml.bak`). This repo has no Cargo registry config and uses crates.io, as CI does.
- **Tried and failed:**
  - A repo `.cargo/config.toml` can't undo the redirect. A config file can override keys but not delete them. Pointing `replace-with` at a second crates.io source fails ("already defined by `crates-io`"), at `crates-io` itself fails ("cycle of `replace-with` sources"), and redefining the `mirror` source as crates.io fails the same way.
  - A separate `CARGO_HOME` doesn't help. Cargo also reads every `.cargo/config.toml` in the parent directories of the working directory, so a repo under `$HOME` always picks up `~/.cargo/config.toml`.
- **Consequence:** New work project folders need the mirror config copied in, or their builds go to crates.io. `Cargo.lock` records crates.io as the source either way, so lock files are portable.


## D19 — Transparent input is composited over white in core (2026-10-04)
- **Context:** A pixel's alpha (A) says how much of whatever is behind it shows through, but an image file doesn't say what is behind it. L\* needs one final colour per pixel, so the app has to choose a background. The RGB of fully transparent pixels is often junk (usually black). The app's input is photos (JPEG/HEIC), which have no alpha, so transparency only comes from the occasional PNG.
- **Decision:** Keep spec F1: blend every pixel over white once on load (`shown = colour × a + white × (1 − a)`, a = A/255), then set A = 255. This is a `prep-core` function (`composite_over_white`), called by both the CLI and wasm, with known-value and property tests. The web path may also fill the canvas white before drawing, as a second guard.
- **Blend space:** the stored sRGB bytes, not linear light. That is how browsers, Canvas 2D and image editors composite, so the result looks the way the PNG looked to whoever made it, and it matches the canvas guard exactly. Linear-light blending would be physically more correct but would differ from both. Integer math, rounded to the nearest byte; opaque pixels are skipped, so photos pay only a single comparison per pixel.
- **Why:** Simplest option, and enough for photos. Everything after this step stays alpha-free. Doing it in core means the CLI (which decodes PNGs with `image`, not a canvas) and the phone give identical results.
- **Known downside:** A transparent background counts as white pixels. With equal intervals (D10) this only inflates the lightest bucket's legend percentage. With quantiles or natural breaks, a large background shifts the thresholds.
- **Rejected:** Using RGB as stored and ignoring alpha (junk RGB turns transparent areas black). Rejecting images with alpha (unfriendly, and costs as much as compositing).
- **Deferred enhancements**, if non-photo input (cut-outs, digital art) ever matters:
  1. *Exclude transparent pixels from the histogram:* still composite over white for display, but skip pixels with A = 0 when building the histogram. A cheap half-step that fixes the threshold shift.
  2. *Other background colour:* composite over black, mid-grey (less disturbing for a value study), or a colour the user picks. Needs a setting (F5).
  3. *Full alpha support:* weight the histogram by alpha, carry alpha through blur (transparent edges need care), posterize and export (PNG with alpha), and define how the preview shows transparent areas. The most correct option for cut-outs, and the most work.

## D20 — Both output-grey modes stay user-selectable (2026-10-04)
- **Alternatives:** Pick one mode in M1 and drop the other.
- **Decision:** Evenly spaced and bucket mean are both an option in the app and the CLI (`--greys even|mean|both`), persisted with the other settings (F5). **Evenly spaced is the default** (open question 1, decided 2026-10-04).
- **Why:** They answer different questions. Evenly spaced is a fixed value scale you can pre-mix paints for, the same for every photo; bucket mean shows how light each region really is. Which one is useful depends on the photo and the painting. Core already takes the mode as a parameter, so keeping both costs only one UI control.
- **Default chosen by comparing both on 3 real photos with the CLI:** evenly spaced separates the big shapes clearly (true black and white, the same scale for every photo), which is what a value study is for. Bucket mean squeezed the greys into about L\* 9–89, and on a flat overcast photo its middle buckets (L\* 31, 51, 68) blended together.

## D21 — CLI: clap, `image` with JPEG/PNG only, no ICC conversion (2026-10-04)
- **Decision:** `prep` parses arguments with clap (derive), and decodes with `image` 0.25 with only the `jpeg` and `png` features (no rayon, no other formats). EXIF orientation is applied on load, as the browser does. The pipeline runs at full resolution.
- **Why:** clap gives help text and validation for free, and the `grid` subcommand (M3) slots in. Neither crate reaches the wasm build. JPEG and PNG cover phone photos and lossless test fixtures.
- **Known difference from the web app:** `image` doesn't apply embedded ICC profiles (it converts only between CICP colour spaces). A Display P3 iPhone JPEG is read as if it were sRGB, so it looks slightly duller than after the browser's conversion (F1). Fine for eyeballing. If CLI output must ever match the phone exactly, convert with `moxcms` (already a dependency of `image`) from `ImageDecoder::icc_profile()`. Playwright fixtures are profile-less sRGB PNGs anyway (spec §8).


## D22 — Wasm build: the wasm-bindgen CLI directly, pinned (2026-10-04)
- **Decision:** `scripts/build-web.sh` runs `cargo build --release -p prep-wasm --target wasm32-unknown-unknown`, then `wasm-bindgen --target web` into `web/pkg/`. The crate is pinned with `wasm-bindgen = "=0.2.129"`, and the script refuses to run unless the installed CLI matches the version in `Cargo.lock`. Install it with `cargo install wasm-bindgen-cli --version 0.2.129 --locked`. To upgrade, bump both together.
- **Alternatives:** wasm-pack (still maintained, now in the wasm-bindgen org), which wraps the same CLI but produces an npm package (`package.json`, README copy) that a no-bundler static site doesn't need. trunk, which is built for Rust-frontend apps and wants to own `index.html`.
- **Why:** `--target web` gives plain ES modules that `app.js` imports directly, matching spec §4 (no bundler) and §5. One less tool to install, and the version pin is explicit and checked.
- **Wasm API:** a stateful `Processor` owns the pipeline buffers and reuses them between photos. JS reads the output in place through `output_ptr()`/`output_len()` on the module's `memory` (spec §7). Inner methods return `prep_core::Result` and are tested natively; the `#[wasm_bindgen]` wrappers only convert errors to `JsError` (which panics off wasm).

## D23 — M2 on-device results: decode path and preview timings (2026-10-04)
- **Device:** iPhone, iOS 18.7, Safari 18.7.2, devicePixelRatio 2, viewport 414 × 714, over plain HTTP on the LAN.
- **Decode path (open question 3):** `createImageBitmap(file, { resizeWidth, resizeQuality: "high", imageOrientation: "from-image" })` is honoured. A 4032 × 3024 photo decoded straight to 2048 × 1536 with the right aspect ratio, so the `drawImage` fallback didn't run. Desktop Playwright WebKit 1.63 behaves the same.
- **Timings (ms), 12 MP photo → 2048 × 1536 preview:**

  | decode | draw (resize) | getImageData | load (composite, L\*, histogram) | render (posterize) | putImageData |
  |---|---|---|---|---|---|
  | 143 | 102 | 165 | 62 | 17–41 | 3–5 |

  Loading a photo takes about 0.6 s in total. A slider change costs one render plus putImageData, about 20–45 ms, so the live preview is fast enough on the main thread (D13 holds).
- **Memory:** wasm memory is 37.3 MB after a 2048 × 1536 preview.
- **Observations:**
  - `getImageData` is the slowest step, not wasm. This matches spec §7: buffer transfer, not computation, is the bottleneck.
  - Export at 12 MP will be roughly 4× these numbers, which justifies the busy indicator (F4).
- **Large photos (first try, M2.7 build):**
  - 8793 × 7331 (64 MP): no error was reported, but the study was solid white, briefly black at first. `draw` took 1169 ms and `getImageData` 1261 ms. Likely cause: memory pressure. WebKit doesn't throw when a canvas can't get memory; it reads back transparent black, which `composite_over_white` turns white.
  - 11375 × 8992 (102 MP): iOS killed the page ("the web page was reloaded"). JS can't catch this.
  - Contributing bug: the app called `img.decode()` only to learn the size. That decodes the full-resolution image (about 260 MB at 64 MP) while `createImageBitmap` decodes again.
- **Fix (M2.8):**
  - The size now comes from the `<img>` `load` event, which reads the header without forcing a decode (6 ms for 48 MP on desktop WebKit). The `<img>` is released right after.
  - After drawing, the app checks that every pixel is opaque. The canvas is filled with opaque white first, so any transparent pixel means the canvas failed, and the app shows "not enough memory" instead of a wrong study. The WebKit smoke test simulates this case.
- **M2 closed (2026-11-08):** M2.8 pushed, CI `webkit` job confirmed green on GitHub. M2 is complete.
- **Still open (deferred follow-ups, not blocking M3):**
  - Retest 64 MP and 102 MP photos on the device with the M2.8 build, **before M4 export**. If the larger one still kills the page, refuse sources above a measured pixel limit before decoding (size is known from the `load` event).
  - The real canvas size limit (measure in M4, with export).
  - The native CLI side of the §7 benchmark.

## D24 — M3 grid geometry and open question 2 (2026-11-09)
- **Open question 2 resolved:** in mode A (square cells), the non-reference axis gets as many whole cells as fit, and the leftover is drawn as a smaller **partial last cell at the far edge**, flagged in the UI. Centring the grid (splitting the leftover into two half-cells) is **not** offered. Chosen over centring because it's spec F3's stated default, is simplest to reason about, and keeps one predictable origin corner; centring stays an easy later addition if it proves useful on the device.
- **`grid_lines` returns `Result<Vec<Line>>`**, refining the spec sketch's bare `Vec<Line>` (`spec.md:159`, "to be refined in plan mode"). It matches every other fallible core function and validates defensively: `Error::ZeroSize` for a zero dimension, and a new `Error::GridCount { count }` for a zero `k`/`rows`/`cols`. The CLI relies on clap ranges and the web on slider `min`s, but core still checks.
- **Geometry conventions:** `Line` holds `f32` image-relative pixel coordinates (non-integer cell boundaries like `width / k` must land exactly, and Canvas 2D scales them). Outer borders are included. Diagonals, when on, cross every **whole** cell with both diagonals (an X); partial cells get none, which also keeps all endpoints trivially in bounds.
- **Drawing stays out of core (D9).** `prep-core` returns geometry only. The web draws it with Canvas 2D; the CLI rasterizes it with a small in-crate routine (axis-aligned fills + a thickness-box DDA for diagonals, no new dependency). Two-tone (a dark outline under a light core) and anti-aliasing are Canvas features and aren't reproduced in the CLI, which draws one `--colour`.
- **wasm boundary:** `gridLines` is a free `#[wasm_bindgen]` function (geometry needs no loaded photo), returning the lines flattened to a `Vec<f32>` `[x1,y1,x2,y2,…]` — the same copy-out idiom as `counts()`/`greys()`, no `Vec<struct>` or serde. Mode/axis cross as strings. Its error paths build a `JsError`, which panics off wasm (D22), so they're unit-tested in `prep-core`, not the wasm crate.
- **UI:** grid controls stack as rows under the Values controls; the tab bar sketched in `ui.md` is **deferred** (not needed yet). The render path splits into `render()` (re-posterize) and `paint()` (repaint base + grid), so grid-only changes skip posterizing (spec §5 caching). The study-only path stays byte-identical, so the WebKit byte-exact test is unaffected (grid defaults off). *(UI superseded by D25.)*

## D25 — Full-screen Settings overlay + number steppers (2026-11-15)
- **Context:** the on-device check of M3 (2026-11-15) showed the stacked controls panel left the **photo too small** in portrait, and the **grid sliders were too small to hit** on the phone.
- **Decision:** the photo is full-screen; a **Settings** button in the top bar opens a **full-screen overlay** (reusing the hidden-`<div>` pattern of the error banner / debug panel, not `<dialog>`) holding all value + grid controls; **Done** closes it to reveal the photo with the choices applied. The sliders for the integer counts (n 2–10, grid k 2–16, rows/cols 1–12) are replaced by **− / + steppers**: large tap targets, clamped to range, a button disabled at its limit. Segmented radios are unchanged. Export (M4) will be a section inside this overlay.
- **Supersedes** the tab-bar sketch in `ui.md` (Photo/Values/Grid/Export) and D24's "controls stack under the preview": the single overlay replaces both.
- **Alternatives:** an in-flow collapsible panel or a bottom tab bar (both keep controls in the page flow, shrinking the photo); a partial bottom sheet (photo visible while adjusting, but less photo room). The full-screen overlay was chosen (user, 2026-11-15) for maximal photo space and the set-then-reveal flow; live preview while adjusting is given up because the overlay covers the photo (renders still run on each change, so the result is ready on close).
- **Layout:** rows that carry two controls (axis+k, diag+over) wrap at phone width so nothing overflows (verified in WebKit at 390 px).
- **Testing:** the WebKit smoke test opens the overlay before touching controls and drives the n stepper by tapping − / +. The `#file` input stays **outside** the overlay so `set_input_files` works without opening it. Dev gotcha reaffirmed: test in a Safari **Private tab** to avoid a stale cached `app.js` running against new HTML (`running.md`) — the exact failure seen on 2026-11-15 (old `app.js` looked up the removed `#n` slider and threw, killing all listeners).

## D26 — M4 export: renderExport reuses preview thresholds; two-step share; Pages deploy (2026-11-15)
- **Scope:** M4 Step 1 — export the current view to Photos, and host over HTTPS. Step 2 (manifest + service worker) is separate. Legend-in-export (F2) stays M5.
- **Export reuses the preview's thresholds (D11, D12):** `Processor::renderExport(rgba, w, h, n, bucket_mean)` posterizes a photo decoded at export resolution, reusing the preview's thresholds and greys. Because thresholds are histogram **bin indices** (D12) and the preview histogram is retained, they're **recomputed deterministically from it** with the same `n`/mode rather than stored — byte-identical to the on-screen study, with no new field or error. A **separate** `l_export`/`out_export` buffer pair keeps export from touching the preview buffers `paint()` reads (D15). Chosen over storing a `Thresholds` + adding a "not rendered" error: less state, same guarantee.
- **Export size — area cap (spec §7):** `exportSize(w, h)` scales by `sqrt(EXPORT_MAX_PX / (w·h))`, default 12 MP, never upscaled — capped by **area** (any aspect ratio stays under the limit), unlike the preview's long-side cap (2048). `decodeToWorkingSize` generalised to `decodeToSize(file, computeTarget)`, reused for both paths incl. the out-of-memory guard.
- **Format PNG:** lossless, and a value study is a few flat greys so it stays tiny; a grid-over-original photo export is larger. Revisit JPEG (q≈0.92) only if on-device size is a problem.
- **Two-step share (F4):** **Export** renders + encodes to a `File` (busy indicator); then **Share** (`navigator.share({ files })`, guarded by `canShare`) runs in its **own** tap, so Web Share isn't rejected after the slow render. An `<a download>` **Save** link is the fallback — saves to Files on iOS, and is the only option on the plain-HTTP LAN where `canShare` is undefined.
- **`drawGrid(ctx, w, h)`** now takes explicit dimensions, so the same image-relative geometry draws at preview *or* export scale (D9); the partial-cell note update moved to `paint()`.
- **Deploy — GitHub Actions Pages source (not "deploy from a branch"):** `.github/workflows/deploy.yml` builds the wasm (pinned `wasm-bindgen`, like the `webkit` job) and publishes `web/` on push to `main`. Chosen because the app has a Rust→wasm build step and `web/pkg/` is generated/git-ignored (D22): Actions builds it fresh each deploy, so no artifacts are committed and the live site always matches source. Branch-deploy can't build, and would force committing `web/pkg/`. Auth uses a short-lived **OIDC** token (`id-token: write`) — no stored secret. **Repo made public 2026-11-15** so Pages works on the Free plan; a full-history scan found no secrets, work-infra names, personal photos, or the user's real email first.
- **Verified:** full check green; wasm +3 tests (export reuse, bucket-mean parity, preview unaffected after export); WebKit smoke +1 (the exported PNG byte-matches the CLI study — the fixture is below the cap, so export size == preview size). On device: export + Save/Share to Photos confirmed on the live HTTPS site `https://tanhminh.github.io/painting-prep/`.
- **Still open:** the real canvas size limit (§7 / open question 3) is unmeasured — basic export works, the large-photo stress test is deferred (→ D23 when done). Custom Pages domain noted in `BACKLOG.md`.
