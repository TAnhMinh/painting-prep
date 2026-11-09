# M3 — Grid overlay

## Context

M2 is complete: the PWA shows a live value study on the iPhone. M3 adds the
second core feature (spec §1): a **grid overlay** for transferring a composition
to canvas. Per D9, `prep-core` returns line *geometry* in image-relative units
and Canvas 2D draws it — grid lines are never written into the pixel buffer on
the web path. The milestone spans three crates (`prep-core` geometry, `prep-cli`
`prep grid`, `web/` UI + drawing), which is why it's planned as multi-crate work.

Decisions taken for this plan (user-confirmed 2026-11-09):
- **Grid controls stack as rows under the existing Values controls** — no tab bar
  yet (ui.md's tabs are deferred to a later milestone).
- **Open question 2 → draw & flag the partial last cell** (spec F3's stated
  default). Centring the grid is *not* built now; it stays a possible later
  option. This resolves spec §10 open question 2.
- **Full F3** this milestone (except the explicitly-deferred cell labels): both
  modes, per-cell diagonals, all four line styles, grid over study *or* original.

Scope notes: settings persistence (F5) stays out — grid settings live in the JS
`state` object, consistent with M2 (F5 is an M5 item). Snapshots/undo (F6) are
out (M6).

---

## 1. `prep-core` — grid geometry (new module `crates/core/src/grid.rs`)

New public types + function, re-exported from `lib.rs` (alphabetised in the
`pub use` list), following the crate's doc/enum conventions (`#[derive(Debug,
Clone, Copy, …)]`, `///` on every variant citing spec/decision, module header
`//! Grid line geometry in image-relative units (spec F3, D9).`).

```rust
/// A line segment in image-relative pixel coords (0..=w, 0..=h). f32 so
/// non-integer cell boundaries land exactly for Canvas 2D (D9).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line { pub x1: f32, pub y1: f32, pub x2: f32, pub y2: f32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis { Width, Height }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridMode {
    /// Mode A: square cells sized axis_len / k; the other axis gets whole cells
    /// plus a partial last cell (spec F3; open question 2 → drawn & flagged).
    Square { axis: Axis, k: u32 },
    /// Mode B: rows × cols, non-square cells dividing the image exactly.
    RowsCols { rows: u32, cols: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSpec { pub mode: GridMode, pub diagonals: bool }

/// Image-relative grid lines, outer borders included (spec F3, D9).
/// # Errors
/// - [`Error::ZeroSize`] if w or h is 0.
/// - [`Error::GridCount`] if k, rows, or cols is 0.
pub fn grid_lines(w: u32, h: u32, spec: &GridSpec) -> Result<Vec<Line>>;
```

- **Refines the spec sketch** (`spec.md:159` showed a bare `Vec<Line>`): returns
  `Result<Vec<Line>>` to match every other fallible core fn. Add one
  `Error::GridCount { count: u32 }` variant to `error.rs` (doc: "A grid count
  (k, rows, or cols) is zero."), with its `Display` arm — mirrors
  `BucketCount { n }`.
- **Geometry:**
  - Mode B: vertical lines at `x = i*w/cols`, `i in 0..=cols`; horizontal at
    `y = j*h/rows`, `j in 0..=rows` (endpoints 0 and w/h = outer borders).
  - Mode A (axis = Width): `cell = w/k`; verticals at `i*cell`, `i in 0..=k`
    (ends exactly at w); `full = floor(h/cell)` horizontals at `j*cell`,
    `j in 0..=full`, **plus** the bottom border `y = h` when `h` isn't a whole
    multiple (that extra thin row is the flagged partial cell). Symmetric for
    axis = Height.
  - Diagonals (when set): both diagonals of each **whole** cell (an X per cell);
    partial cells get none, keeping endpoints trivially in-bounds.
- **Tests (inline `#[cfg(test)]`, required by CLAUDE.md):**
  - Known values: 100×100 mode B 2×2 → verticals x∈{0,50,100}, horizontals
    y∈{0,50,100}; mode A width k=2 on 100×120 → cell 50, horizontals
    y∈{0,50,100,120} (partial row flagged by the 120 border); diagonals count.
  - Properties (`proptest`, per spec §8): every endpoint within [0,w]×[0,h]; line
    count matches the closed-form (B: `(cols+1)+(rows+1)+diag`, A: computed),
    and `GridCount`/`ZeroSize` errors for zero inputs.

## 2. `prep-cli` — `prep grid` subcommand (new `crates/cli/src/grid.rs`)

- Add `Command::Grid(GridArgs)` arm in `main.rs` dispatch (→ `run_grid`,
  `Result<(), Box<dyn Error>>`, same `map_err(format!…)` I/O pattern as
  `run_values`).
- `GridArgs` mirrors `ValuesArgs` style (clap derive, doc-comment help,
  `value_parser` ranges, `ValueEnum`s, `input`/`output` positionals):
  `--mode square|rowscols` (default square), `--axis width|height`,
  `-k/--cells` (range 2..=16, default 6), `--rows`/`--cols` (range 1..=12,
  default 3), `--diagonals` (flag), `--colour black|white|grey` (default black),
  optional `--line-px` (0 = auto, default). clap enforces ranges (no new core
  error needed for the CLI path).
- `run_grid`: `image_io::load` → `composite_over_white` → build `GridSpec` →
  `prep_core::grid_lines` → **rasterize** lines into the owned RGBA `Vec<u8>` →
  `image_io::save`. Reuses `image_io` verbatim.
- **Rasterizer lives in the CLI, not core** (D9: core is geometry-only). Small
  hand-rolled routine (no new dependency — matches the crate's lean deps):
  axis-aligned segments = rect fill; diagonals = integer DDA with a
  thickness box, clamped to bounds. No anti-aliasing (Canvas 2D is the real
  renderer; the CLI is for eyeballing geometry). Two-tone is a canvas effect —
  the CLI draws a single `--colour`; note this in `--help`/docs.
- Print a stdout summary in the `values` legend style: input dims, mode, cell
  size, line count, and a "partial last cell" note when mode A leaves a
  remainder.
- Integration test `crates/cli/tests/grid.rs`: run on a fixture, assert output
  dims and that grid-colour pixels appear on expected borders; rely on clap for
  range rejection (as `tests/values.rs` does).

## 3. `prep-wasm` — geometry binding (`crates/wasm/src/lib.rs`)

- Add a **free** `#[wasm_bindgen]` fn (geometry needs no `Processor` state),
  thin wrapper only (CLAUDE.md §19):
  ```rust
  #[wasm_bindgen(js_name = gridLines)]
  pub fn grid_lines_js(width: u32, height: u32, mode: &str, axis: &str,
      k: u32, rows: u32, cols: u32, diagonals: bool) -> Result<Vec<f32>, JsError>
  ```
  Maps the scalar args to a `GridSpec`, calls `prep_core::grid_lines`, and
  **flattens to `Vec<f32>` `[x1,y1,x2,y2, …]`** — the established boundary idiom
  (wasm-bindgen copies it out as a `Float32Array`, exactly like `counts()`/
  `greys()`; no `Vec<struct>`, no serde, no new dep).
- Regenerate `web/pkg/` with `scripts/build-web.sh` before the web step.

## 4. `web/` — grid UI + Canvas 2D drawing

**Controls** (`index.html`, new `.row`s appended inside `.controls`, after the
greys row — reusing `.segmented` / `.ruler` / `.name`, radio-behind-label
pattern):
- `grid` mode: Off / Square / R×C  · `axis`: Width / Height + `k` ruler slider
  (2–16) · `rows` + `cols` ruler sliders (1–12) · `diag`: Off / On ·
  `over`: Study / Original · `line`: Black / White / Grey / Two.
- Axis+k rows show only in Square mode; rows+cols rows only in R×C (toggle the
  `hidden` attribute). These are rows inside `.controls`, not top-level grid
  areas, so hiding them just reflows the panel (the D-named-area caveat doesn't
  apply here).

**State** (`app.js` `state`): add `grid: { mode:"off", axis:"width", k:6,
rows:3, cols:3, diagonals:false, over:"study", line:"two" }` and
`originalImageData: null`. Register refs in `els`; wire listeners in
`bindEvents` like the greys loop. **Default mode "off"** so the WebKit
byte-exact comparison (grid untouched) stays identical.

**Render split** (honours the spec caching table — grid never reruns posterize):
- `loadPhoto`: stash `state.originalImageData = imageData` (the
  composited-over-white canvas pixels, ~12 MB) for "over original".
- `paint()`: draw the base — `putImageData` of the live wasm output
  (`output_ptr/len`, unchanged for "study") or of `originalImageData` (for
  "original") — then `drawGrid(ctx)` if mode≠off, then set
  `els.preview.dataset.rendered` **at the end, on success** (preserves the test
  hook semantics). Keep the study-only path byte-identical to M2.
- `render()` (n/greys change): `p.render()` then `paint()`.
- Grid-only change: `paint()` only (wasm output still valid → no re-posterize).
- `drawGrid`: call `gridLines(p.width(), p.height(), …)` (coords already in
  working-size px = preview intrinsic size, no scaling), build one `Path2D`,
  `lineWidth = max(1, 0.0015*min(w,h))` (spec §F3). Styles: black/white/grey =
  one stroke; **two-tone** = dark wider outline pass then light narrower pass.
- **Partial-cell flag** (open question 2): when mode A leaves a remainder, show a
  small note by the grid controls (e.g. `#grid-note` text "partial last cell").

**Hooks to preserve** (CLAUDE.md §Conventions): `body[data-wasm=ready]`,
`#preview[data-rendered="<n>,<even|mean>"]`, `window.prepDebug.errorCount`,
`body[data-busy]`. Nothing in `drawGrid` may throw before `dataset.rendered` is
set — keep it inside the existing try.

## 5. Docs (end of session)

- `docs/decisions.md`: **D24** — resolve open question 2 (draw & flag partial
  cell; centring deferred) + record grid geometry conventions (`grid_lines`
  returns `Result`; flat `Vec<f32>` wasm boundary; per-whole-cell diagonals;
  CLI rasterizes, core stays geometry-only per D9).
- `docs/spec.md`: mark §10 open question 2 resolved (→ D24); refine the Core API
  sketch line for `grid_lines` to `Result<Vec<Line>>`.
- `docs/ui.md`: note grid controls stack under Values for M3; tabs deferred.
- `CLAUDE.md`: Status → M3 done; add conventions (grid line-drawing idiom, wasm
  flat-array geometry boundary); keep the large-photo retest follow-up.

## Commits (one feature each, milestone-prefixed)

1. `M3.1: prep-core grid geometry` (grid.rs + Error::GridCount + tests)
2. `M3.2: prep grid CLI subcommand` (grid.rs rasterizer + args + test)
3. `M3.3: wasm gridLines binding`
4. `M3.4: web grid overlay` (controls, render/paint split, drawing, partial flag)

Run the full check before each (`cargo fmt --all && cargo clippy --workspace
--all-targets -- -D warnings && cargo test --workspace && cargo check -p
prep-wasm --target wasm32-unknown-unknown`). Push only when asked.

## Verification

1. **Core/CLI tests:** `cargo test --workspace` (geometry known-value +
   property tests; CLI integration test). Full check command green.
2. **CLI eyeball:** `cargo run --release -p prep-cli -- grid --axis width -k 6
   ../test_photos/<photo> ../test_photos/out/grid-a.png` and a mode-B /
   `--diagonals` run; open the PNGs and confirm borders, inner lines, diagonals,
   and the partial-cell geometry.
3. **WebKit smoke test stays green** (grid off by default → byte-exact study
   unchanged): `scripts/build-web.sh && .venv/bin/python tests/web/test_smoke.py`.
4. **On device** (`docs/running.md`): build + dev server, open on the iPhone;
   verify grid over study and over original, Square vs R×C, diagonals on/off,
   all four line styles (two-tone legible on light *and* dark areas, spec F3),
   the partial-cell flag in Square mode, and that slider changes stay smooth.
