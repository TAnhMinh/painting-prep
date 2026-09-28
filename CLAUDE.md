# CLAUDE.md — Painting Prep

Personal PWA for iPhone: grid overlays and value studies for painting reference photos.
Dev machine: Ubuntu laptop. No Mac, so no Safari Web Inspector. Use the in-app debug panel and Playwright WebKit (spec §7).
Read `docs/spec.md` before planning any change. Log significant decisions in `docs/decisions.md`.

## Layout
- `crates/core` (package `prep-core`) — all image logic. Pure Rust, no wasm/web/decoding deps. RGBA8 in, RGBA8 out.
- `crates/cli` (package `prep-cli`, binary `prep`) — desktop harness for core (uses `image` for decode/encode).
- `crates/wasm` (package `prep-wasm`) — thin wasm-bindgen wrapper. No logic here.
- Never name a package `core`: it shadows Rust's built-in `core` crate (D8).
- `web/` — vanilla HTML/CSS/JS (ES modules), manifest, service worker. No framework, no bundler. `web/pkg/` is generated, not committed.

## Commands
Keep these commands up to date as tooling is set up.
- `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test --workspace`
- `cargo run -p prep-cli -- values -n 5 in.jpg out.png`
- wasm build: target `wasm32-unknown-unknown`, single-threaded (spec §5). Exact tool TBD in M2; verify the currently recommended toolchain (wasm-bindgen CLI / wasm-pack / trunk) before choosing. The `wasm-bindgen` CLI and crate versions must match exactly.
- Browser tests: Playwright WebKit on Ubuntu (set up in M2).

## Workflow
- One feature per session. Use plan mode for anything touching more than one crate.
- Run the full check command before proposing a commit. Keep commits small.
- If implementation contradicts the spec, stop and propose a spec change rather than silently diverging.
- End each session by updating this file (new conventions, gotchas) and `docs/decisions.md`.

## Conventions
- Core functions are deterministic and allocation-conscious. Reuse buffers where practical.
- No `unwrap`/`expect` in library code paths of `core`. Return `Result` for invalid input (zero size, buffer length mismatch).
- Colour values: `f32` L\* in 0..=100; sRGB as `u8`. Name conversions explicitly (`srgb_to_linear`, `y_to_lstar`).
- Every new algorithm gets known-value tests and at least one property test.

## Don't
- Don't compute lightness as the RGB average or luma on gamma-encoded values. Use sRGB → linear → Y → L\*.
- Don't approximate the sRGB transfer function with a plain 2.2 gamma.
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
- (none yet)
