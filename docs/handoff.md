# Handoff

The quick "where are we and what's next" note. The **live, authoritative** status
is always the Status section of `CLAUDE.md`; this file is the human-and-Claude
readable snapshot at the end of a session. Decisions live in `docs/decisions.md`.

_There is no external ticket tracker for this personal project — work is tracked
here, in `CLAUDE.md` Status, and in `docs/decisions.md`. The repo is private on
GitHub; `gh` isn't installed, so CI is checked by hand in the Actions tab._

---

## Session of 2026-11-15 — M3.5 UI rework + M4 export (COMPLETE)

Milestones done: **M0, M1, M2, M3**, **M3.5** (pre-M4 UI refinement), and **M4**
(export + GitHub Pages deploy + PWA install/offline). **M0–M4 all done.** Next:
**M5** (algorithms, blur, compare, legend, remembered settings). The app is live
and installable at `https://tanhminh.github.io/painting-prep/`.

### M4 Step 2 — PWA manifest + service worker (D27), pushed
- **Manifest + generated icon** (`web/manifest.webmanifest`, `web/icons/`, apple
  meta tags) → "Add to Home Screen" gives a full-screen app with an icon.
- **Service worker** (`web/sw.js.tmpl` → git-ignored `web/sw.js`, SHA-stamped by
  `build-web.sh`): precache/offline, versioned cache, **"reload to update"**
  banner (no skipWaiting). HTTPS-only registration (smoke test unaffected).
- **On device (2026-11-15): install + offline confirmed.** Update-hint mechanism
  verified in CI; observe on the phone after a 2nd deploy.

### M4 Step 1 — export + deploy (D26), pushed
- **B1 `renderExport` (wasm):** export-resolution study that reuses the preview's
  thresholds/greys (recomputed from the retained histogram, D11/D12); separate
  export buffers (D15). +3 tests.
- **B2 web two-step export:** `decodeToSize` at a 12 MP area cap, offscreen render
  (study via `renderExport`, or grid over original), PNG encode, **Export → Share**
  with an `<a download>` **Save** fallback (F4). `drawGrid(ctx, w, h)` now scales
  to export size. WebKit smoke +1 (exported PNG byte-matches the CLI).
- **B3 deploy:** `.github/workflows/deploy.yml` (Actions Pages source; OIDC, no
  secret). Repo made **public** 2026-11-15 (clean history scan) so Pages works on
  Free. Deploy green; export-to-Photos confirmed on the live HTTPS site.
- **Deferred ideas:** `BACKLOG.md` (custom Pages domain — base-path caveat).

### What happened
- On-device M3 check: grid overlay works, but in portrait the stacked controls
  left the photo too small and the grid sliders were too small to tap.
- **M3.5 (D25), web-only:** photo is full-screen; a **Settings** button opens a
  **full-screen overlay** with all value + grid controls (**Done** closes it);
  **− / + steppers** replace the n/grid sliders. Rows with two controls wrap at
  phone width. The `#file` input stays outside the overlay.
  - `web/index.html`, `web/style.css`, `web/app.js` (new `bindStepper`; removed
    `drawTicks`/`bindGridSlider`; overlay open/close), `tests/web/test_smoke.py`
    (opens the overlay, taps the n stepper), docs (D25, `ui.md` M3.5, `running.md`
    Private-tab note, this file, `CLAUDE.md`).
- Verified: full check green; **4/4 WebKit tests**; WebKit screenshots at 390 px;
  **confirmed on the iPhone** (after clearing a stale `app.js` cache — test in a
  Safari **Private tab**, now documented).

### Still open
- **Canvas size limit / large-photo check** (64 MP / 102 MP, deferred from M2):
  basic export works, the real limit is unmeasured. Do it on the live site;
  record in D23. Lower `EXPORT_MAX_PX` / refuse above a measured limit if a big
  photo fails.

### Next
**M5** (spec §9): quantile + natural-breaks algorithms, pre-blur ("squint"),
compare view, legend strip UI (+ the legend-in-export toggle), and remembered
settings (F5). Multi-crate (core + wasm + web) — plan it first. Also outstanding:
observe the update hint on the phone (needs a 2nd deploy), and the canvas-limit
check above.

---

## Session of 2026-11-09 — M3 grid overlay: COMPLETE and PUSHED

Milestones done: **M0, M1, M2, M3.** Next: **M4 (export).**

### What shipped this session
Grid overlay across all three crates (spec F3), pushed to `origin/main`
(`ebf472a..a24c5b9`):

- **M3.1 `f1b2e03`** — `prep-core/src/grid.rs`: `GridSpec` / `GridMode` / `Axis`
  / `Line` and `grid_lines(w, h, spec) -> Result<Vec<Line>>`. Mode A (square
  cells, partial last cell drawn & flagged), mode B (rows × cols), outer borders,
  optional per-whole-cell diagonals. `Error::GridCount` added. 11 tests.
- **M3.2 `0739ff9`** — `prep grid` CLI (`crates/cli/src/grid.rs`): rasterizes the
  geometry into a PNG with a small in-crate routine (no new dependency). Prints a
  summary. Integration tests in `crates/cli/tests/grid.rs`.
- **M3.3 `567c196`** — wasm `gridLines` binding: flat `Vec<f32>` boundary.
- **M3.4 `8666ef9`** — web overlay (`web/app.js`, `index.html`, `style.css`):
  Canvas 2D drawing, controls stacked under Values (no tab bar), both modes,
  diagonals, four line styles (two-tone = dark outline + light core), grid over
  study or original, partial-cell flag. `render()`→`paint()` split so grid-only
  changes skip re-posterizing.
- **Docs `a24c5b9`** — D24 (resolves open question 2), spec/ui/CLAUDE updates,
  `docs/m3-plan.md`, `docs/running.md`.

### Verified
- Full check green: `cargo fmt --all && cargo clippy --workspace --all-targets
  -- -D warnings && cargo test --workspace && cargo check -p prep-wasm --target
  wasm32-unknown-unknown`.
- WebKit smoke test: **4 tests pass** (study still byte-matches the CLI; new grid
  test draws/clears with `errorCount` 0).
- Eyeballed the CLI on a real photo; screenshotted both grid modes in WebKit at
  phone width — clean layout, two-tone legible on light and dark.
- Not yet verified on the actual iPhone (see next steps).

### Key decision
Open question 2 → **draw the partial last cell at the far edge and flag it; don't
centre the grid.** Full rationale and the grid conventions are in **D24**.

---

## To resume next session (in order)

1. **Confirm CI is green** on GitHub after the M3 push (Actions tab — needs you;
   `gh` isn't installed).
2. **Try M3 on the iPhone.** Build + serve + open (see `docs/running.md`; the LAN
   IP is DHCP, read it fresh each time — it was `10.0.0.143` on 2026-11-09).
   Check: both grid modes, over study and original, diagonals, the four line
   styles (two-tone legible on light *and* dark), and the partial-cell flag in
   Square mode.
3. **Large-photo retest** (deferred from M2, do before M4). With the M2.8 fix,
   confirm a 64 MP (8793 × 7331) and a 102 MP (11375 × 8992) photo no longer give
   a white study / crash the page. If the 102 MP one still crashes, refuse
   sources above a measured pixel limit before decoding. Record in D23.
4. **M4: export** — two-step share sheet, PWA manifest + service worker, Actions
   deploy to Pages; measure the real canvas size limit here. Plan it first
   (multi-part: it touches `web/` and adds CI deploy).

## How to run it (short version — full details in `docs/running.md`)
```bash
./scripts/build-web.sh                              # build wasm into web/pkg/
python3 -m http.server 8000 --bind 0.0.0.0 -d web  # serve on the LAN
ip -4 addr show enp0s8 | grep -oP 'inet \K[\d.]+'  # IP for the phone URL
```
Then open `http://<that-ip>:8000` on the iPhone (reopen the Safari tab after a
rebuild — `app.js` is cached).
