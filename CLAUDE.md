# CLAUDE.md — Painting Prep

Personal PWA for iPhone: grid overlays and value studies for painting reference photos.
Dev machine: Ubuntu laptop. No Mac, so no Safari Web Inspector. Use the in-app debug panel and Playwright WebKit (spec §7).
Read `docs/spec.md` before planning any change. Log significant decisions in `docs/decisions.md`.
Start Claude Code in this folder (`painting-prep/`), not its parent `PaintingPrepApp/`. The parent isn't a git repo, so `/diff` and git commands there see nothing.

## Status
Update this at the end of each session. A readable end-of-session handoff is in `docs/handoff.md`.
- Done: M0; M1 (core L\*, histogram, equal-interval thresholds, greys, posterize, legend counts; `prep values` CLI). M2 — complete (2026-11-08): UI, `prep-wasm` `Processor`, WebKit smoke test, CI `webkit` job, large-photo fix (D23); confirmed on the iPhone. **M3 — complete** (2026-11-09, pushed `ebf472a`..`a24c5b9`): grid overlay across all three crates (`grid_lines` geometry, `prep grid` CLI, wasm `gridLines`, Canvas 2D web overlay; both modes, diagonals, 4 line styles, over study/original, partial-cell flag). Open question 2 resolved (D24). Plan in `docs/m3-plan.md`. **M3.5 UI rework — implemented, confirmed on the iPhone 2026-11-15, NOT yet committed/pushed** (D25): photo full-screen; Settings button opens a full-screen overlay holding all controls; − / + steppers replace the n/grid sliders. web-only + WebKit-test update; full check green, 4/4 WebKit tests pass.
- Open now, in this order:
  1. **Commit + push M3.5** (ask the user before pushing). Then confirm CI green on GitHub (Actions tab; `gh` isn't installed).
  2. **Large-photo retest on the iPhone (deferred from M2, do before M4 export).** With the M2.8 fix, confirm a 64 MP photo (8793 × 7331) and a 102 MP photo (11375 × 8992) no longer give a white study / crash the page. If the larger one still crashes, refuse sources above a measured pixel limit before decoding (size is known from the `load` event). Record results in D23.
  3. **M4: export** — two-step share sheet, PWA manifest + service worker, Actions deploy to Pages; measure the real canvas size limit (open question 3 remnant). Approved plan: `/home/tester/.claude/plans/i-would-like-to-ethereal-spindle.md` (export + deploy first, service worker second; export UI lives in the Settings overlay).
- To run/test on the phone: see `docs/running.md` (build, serve, find the DHCP LAN IP). Safari caches `app.js`, so close and reopen the tab after changes. The user sends debug-panel screenshots into the parent folder `../`.

## Layout
- `crates/core` (package `prep-core`) — all image logic. Pure Rust, no wasm/web/decoding deps. RGBA8 in, RGBA8 out.
- `crates/cli` (package `prep-cli`, binary `prep`) — desktop harness for core (uses `image` for decode/encode).
- `crates/wasm` (package `prep-wasm`) — thin wasm-bindgen wrapper. No logic here.
- Never name a package `core`: it shadows Rust's built-in `core` crate (D8).
- Real test photos live in `../test_photos/` (outside the repo, never committed); CLI output goes to `../test_photos/out/`.
- `web/` — vanilla HTML/CSS/JS (ES modules), manifest, service worker. No framework, no bundler. `web/pkg/` is generated, not committed.

## Commands
Keep these commands up to date as tooling is set up.
- Full check (same as CI, `.github/workflows/ci.yml`): `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && cargo check -p prep-wasm --target wasm32-unknown-unknown`
- CLI (binary `prep`): `cargo run --release -p prep-cli -- values -n 5 --greys both in.jpg out.png`. `--greys even|mean|both`; `both` writes original | evenly spaced | bucket mean side by side. Prints the legend. `--help` lists all flags.
- CLI grid (M3): `cargo run --release -p prep-cli -- grid --axis width -k 6 --diagonals in.jpg out.png`. `--mode square|rowscols`, `--axis width|height`, `-k/--cells 2..=16`, `--rows/--cols 1..=12`, `--colour black|white|grey`, `--line-px 0` (0 = auto). For eyeballing geometry; single colour, no AA (two-tone/AA are Canvas-only). Prints a summary with cell size and any partial-cell note.
- wasm build: `scripts/build-web.sh` (cargo build for `wasm32-unknown-unknown`, then `wasm-bindgen --target web` into `web/pkg/`; D22). Needs `cargo install wasm-bindgen-cli --version 0.2.129 --locked`; the script fails if the CLI doesn't match `Cargo.lock`. Bump the crate pin and the CLI together.
- Dev server: `python3 -m http.server 8000 --bind 0.0.0.0 -d web` (after the wasm build). Plain HTTP on the LAN: no service worker or Web Share (spec §7).
- Browser test (Playwright WebKit, spec §8): once, `python3 -m venv .venv && .venv/bin/pip install -r tests/web/requirements.txt && .venv/bin/playwright install webkit` (Ubuntu 26.04 needs Playwright ≥ 1.61, plus `sudo apt-get install libflite1 libavif16 libmanette-0.2-0`). Then `scripts/build-web.sh && .venv/bin/python tests/web/test_smoke.py`. It compares the page's study with `prep values` on `tests/web/fixture.png` (from `make_fixture.py`), byte for byte.

## Toolchain (D17)
- Rust is pinned in `rust-toolchain.toml` (1.98.1, with rustfmt, clippy, wasm32). Bump it deliberately, together with `rust-version` in `Cargo.toml`.
- The dev machine's rustup default is `system` (apt Rust 1.93.1), because the user's Suricata work depends on it. Only this repo uses the pinned toolchain. Never run `rustup default`, `rustup update` or `rustup self uninstall`, and never modify `~/.cargo/config.toml`.
- This repo fetches crates from crates.io. The user's work mirror is configured only in the work project folders' own `.cargo/config.toml` (D18). Don't add a registry config to this repo. Cargo reads `.cargo/config.toml` from every parent directory, so if a dependency fetch shows "Updating `mirror` index", a global or parent config has come back.
- Shells started before rustup was installed may not have `~/.cargo/bin` on PATH. If `cargo fmt` or `cargo clippy` is missing, run `export PATH="$HOME/.cargo/bin:$PATH"`.
- `prep-core` denies `clippy::unwrap_used` and `clippy::expect_used` outside tests, so clippy enforces the no-unwrap rule.

## Workflow
- One feature per commit. Several small features in one session are fine. Start a new session for large features or anything touching more than one crate, and use plan mode for multi-crate work.
- Run the full check command before proposing a commit. Keep commits small. Commit messages start with the milestone step (`M1.6: ...`).
- Claude can push with plain `git push origin main` (works since 2026-10-04). Push only when the user asks.
- If implementation contradicts the spec, stop and propose a spec change rather than silently diverging.
- End each session by updating this file (new conventions, gotchas) and `docs/decisions.md`.

## Conventions
- Never name work infrastructure in this repo (mirror hostnames, work repositories, ticket IDs, internal IPs). It's a public personal project; say "work mirror" or "work projects".
- Core functions are deterministic and allocation-conscious. Reuse buffers where practical.
- No `unwrap`/`expect` in library code paths of `core`. Return `Result` for invalid input (zero size, buffer length mismatch).
- Colour values: `f32` L\* in 0..=100; sRGB as `u8`. Name conversions explicitly (`srgb_to_linear`, `y_to_lstar`).
- Every new algorithm gets known-value tests and at least one property test.
- Pixel loops use `as_chunks::<BYTES_PER_PIXEL>()` / `as_chunks_mut`, which give `[u8; 4]` arrays. The pinned clippy rejects `chunks_exact(4)` with a constant size (`chunks_exact_to_as_chunks`), and CI fails on it.
- Validate a mutable RGBA buffer with `image::check_rgba_len`, the same rule `Image::new` uses.
- Prefer fixed-size array types over runtime length checks where the size is a constant (e.g. `&mut [u32; BINS]`).
- `histogram::lstar_to_bin` is the only L\* → bin mapping. Posterize and thresholds must use it, never their own rounding (D12).
- The page exposes hooks the WebKit test relies on: `body[data-wasm=ready]`, `#preview[data-rendered="<n>,<even|mean>"]` after each render, and `window.prepDebug.errorCount`. Keep them when changing the page.
- Segmented controls are radio inputs hidden behind their labels. In Playwright, click the `label[for=…]`, not the input.
- Controls live in the full-screen Settings overlay (`#settings`, D25); it toggles via the `hidden` attribute (needs the `.overlay[hidden]{display:none}` guard, like `.row`/`.banner`). The `#file` input stays outside the overlay so file-picking works when it's closed. Integer counts (n, grid k/rows/cols) use `bindStepper(id, min, max, get, apply, after)` (`− <id>-value +`), not sliders. The WebKit test must open the overlay (`#settings-open`, only while hidden — the open overlay covers the button) before touching controls, and drive the n stepper by tapping `#n-inc`/`#n-dec`.
- Test the web app on the phone in a Safari **Private tab** (no HTTP cache): a stale cached `app.js` against new HTML throws on load and breaks every control (`docs/running.md`).
- Web layout uses named `grid-template-areas`. Elements that are sometimes `hidden` (the error banner) must have a named area, or the rows below them shift. A `.row` (or any element with a non-`none` `display`) that is sometimes `hidden` needs an explicit `[hidden] { display: none }` rule, since the class's `display` beats the UA `[hidden]` rule.
- Returning a non-pixel array across the wasm boundary: flatten it to a `Vec<f32>`/`Vec<u8>`/`Vec<f64>` and let wasm-bindgen copy it out, as `counts()`, `greys()` and `gridLines` do. No `Vec<struct>`, no serde. (Pixel buffers still cross via `output_ptr`/`output_len`, zero-copy.)
- Web render path: `render()` re-posterizes (n/greys/photo changed), `paint()` repaints the base + grid without re-posterizing (grid-only changes, via `queuePaint()`), per the spec §5 caching table. Keep `#preview[data-rendered]` set at the end of a successful `paint()`, and keep the grid off by default so the WebKit byte-exact study test is unaffected.
- A `pub(crate)` function that only tests use yet gets `#[cfg_attr(not(test), expect(dead_code, reason = "used by ..."))]`. Delete the attribute when the compiler warns that it's no longer needed.

## Don't
- Don't compute lightness as the RGB average or luma on gamma-encoded values. Use sRGB → linear → Y → L\*.
- Don't approximate the sRGB transfer function with a plain 2.2 gamma.
- Don't handle alpha after load. `composite_over_white` flattens it once on the stored sRGB bytes (D19), and everything later assumes opaque pixels. Other transparency options are deferred enhancements listed in D19.
- Don't decode images in Rust on the web path. The browser decodes (orientation, HEIC, colour space).
- Don't add JS dependencies, frameworks, or a bundler without asking.
- Don't store images in localStorage/IndexedDB. Only the small versioned settings object (spec F5).
- Don't introduce wasm threads, SharedArrayBuffer, or anything needing COOP/COEP headers.
- Don't assume Chrome behaviour equals Safari. Check WebKit for anything touching canvas, file input, or share.
- Don't allocate canvases above the size limits in spec §7. Decode straight to the working size; never draw a full-resolution photo into a canvas.
- Don't modify the original RGBA or L\* buffers. Everything is recomputed from them; undo restores settings, not pixels (D15).
- Don't draw grid lines into the pixel buffer. Core returns geometry; Canvas 2D draws it (D9).
- Don't call `navigator.share` after a slow render in the same tap handler. Export is two steps: render, then Share (spec F4).

## Where the AI was wrong
Record concrete cases here, with date and what the fix was. This is part of the learning log.
- 2026-10-03: proposed a repo `.cargo/config.toml` (later a separate `CARGO_HOME`) to bypass the global crates.io mirror. Neither works: config files can't delete inherited keys, and Cargo reads parent-directory configs up to `~/.cargo`. Fix: the user moved the mirror config into the work folders (D18). Lesson: test config overrides with a dry run before planning around them.
- 2026-10-04: the first M2 page layout placed grid items implicitly. When the error banner was hidden, the preview moved into an `auto` row: the title bar vanished and the controls overflowed. Caught in a Playwright screenshot before committing. Fix: named grid areas. Lesson: screenshot new layouts in WebKit at phone size before calling them done.
- 2026-10-04: `decodeToWorkingSize` used `img.decode()` just to read the photo's size. That forced a full-resolution decode (about 260 MB at 64 MP), so a 64 MP photo gave a blank white study and a 102 MP photo crashed the page on the iPhone. Desktop WebKit had enough memory and didn't show it. Fix: use the `load` event for the size, and treat a canvas that reads back non-opaque as out of memory (D23). Lesson: check every browser call on the large-image path against the "never full resolution" rule, and test big photos on the device early.
