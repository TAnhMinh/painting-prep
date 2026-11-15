# UI sketch

*Draft for M2 (2026-10-04). Layout only: sizes are rough, and the look is decided while building. Update this file when the real screens prove it wrong.*

## Principles

- **The photo comes first.** The preview gets as much of the screen as possible. Controls take a compact panel, not a long scrolling page.
- **Controls sit within thumb reach.** In portrait they go at the bottom; the top of an iPhone screen is hard to reach one-handed.
- **Undo, redo, reset and pin are always visible** (spec UI). They live in the top bar, which never scrolls.
- **Controls live in a full-screen Settings overlay** (D25, from M3.5). The photo is full-screen; a Settings button opens the overlay with all controls, and closing it reveals the photo with the choices applied. (This replaced the tab-bar idea below; see the M3.5 update.)
- **Safe areas:** `<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">`, and the page is padded with `env(safe-area-inset-*)`. Nothing interactive goes under the notch, the Dynamic Island or the home indicator.

## Portrait (primary)

```
┌─────────────────────────────────┐
│░░░░░░░░ safe area (notch) ░░░░░░│
├─────────────────────────────────┤
│ Painting Prep      ↶  ↷  ⟲  📌 │  top bar: title (long-press → debug panel),
├─────────────────────────────────┤           undo, redo, reset, pin
│                                 │
│                                 │
│                                 │
│            preview              │  study / original / grid; fits the box,
│      (hold to see original)     │  keeps aspect ratio
│                                 │
│                                 │
│                                 │
├─────────────────────────────────┤
│ ▇▇▇▇▇ ▆▆▆▆▆ ▅▅▅▅▅ ▃▃▃▃▃ ▁▁▁▁▁  │  legend strip: n greys, dark → light,
│  12%   31%   27%   20%   10%    │  % of pixels under each
├─────────────────────────────────┤
│ [snap1] [snap2] [snap3] [snap4] │  snapshots (only shown when there are any)
├─────────────────────────────────┤
│ Values                          │  panel for the selected tab
│ n      ──────●──────────  5     │
│ algo   [Equal] Quant  Jenks     │
│ greys  [Even]  Mean             │
│ blur   ●────────────────  0 %   │
├─────────────────────────────────┤
│  Photo   Values   Grid  Export  │  tab bar
│░░░░░░░ safe area (home bar) ░░░░│
└─────────────────────────────────┘
```

### Tab panels

```
Photo                               Grid
┌───────────────────────────────┐   ┌───────────────────────────────┐
│ [ Choose photo… ]             │   │ grid    Off [Square] Rows×Cols│
│ 4032 × 3024 → 2048 × 1536     │   │ axis    [Width] Height   k 6  │
│ compare [Original] Snapshot   │   │ diag    ☐    over  [Study] Orig│
└───────────────────────────────┘   │ line    Black White Grey [Two]│
                                    └───────────────────────────────┘
Export
┌───────────────────────────────┐
│ size cap  [12 MP]  ☐ legend   │
│ [ Export ]   → then [ Share ] │   two steps (spec F4)
└───────────────────────────────┘
```

- **Photo** opens first when no photo is loaded. The preview area then shows a large "Choose photo" button instead of an empty box.
- **Compare** (spec F2, F6): hold on the preview for a quick flip, or switch to side by side here. The source is the original or a snapshot.
- **Export** shows a busy indicator while rendering. After that, "Share" appears and needs a fresh tap (spec F4).

## Landscape

Preview on the left, controls in a column on the right. The top bar stays across the top. The safe areas are on the left and right sides now.

```
┌──────────────────────────────────────────────────────────┐
│ Painting Prep                              ↶  ↷  ⟲  📌  │
├────────────────────────────────────┬─────────────────────┤
│░│                                  │ Photo Values Grid Ex│
│░│                                  ├─────────────────────┤
│░│             preview              │ n    ────●─────  5  │
│░│                                  │ algo [Eq] Qu  Jk    │
│░│                                  │ greys [Even] Mean   │
│░│                                  │ blur ●─────────  0% │
│░├──────────────────────────────────┤                     │
│░│ ▇▇▇ ▆▆▆ ▅▅▅ ▃▃▃ ▁▁▁   legend     │ [s1][s2][s3][s4]    │
└────────────────────────────────────┴─────────────────────┘
```

## Overlays

- **Debug panel** (spec §7): opened by a long-press of about 600 ms on the title. It's a full-screen sheet over the app, with a Close button at the top right, below the safe area. It shows the console log, errors, image sizes, the decode path, step timings and wasm memory. The title has `-webkit-touch-callout: none; user-select: none`, so iOS doesn't show its own callout.
- **Errors** appear as a dismissible banner under the top bar. The app stays usable (spec F1).
- **Update hint** (M4): a banner in the same place: "New version — reload".

## What M2 builds

Only these parts, in the portrait layout. Landscape just has to stay usable (the controls may stack under the preview):

- the top bar with the title and the debug long-press (undo, redo, reset and pin come in M6)
- the preview
- a Values panel with **n** (slider, 2–10, default 5, with a tick at every value) and **greys** (Even / Mean)
- a "Choose photo" button (the Photo panel without its info rows)
- the error banner and the debug panel

There are no tabs yet. M2 shows the photo button and the Values panel stacked under the preview.

## M3 update (2026-11-09)

M3 added the Grid panel but **kept everything stacked** under the preview — the Values rows, then the Grid rows (grid mode, axis + k or rows + cols, diagonals + over, line style, and a partial-cell note). The tab bar sketched above is **deferred**: with two short panels the single scroll is fine on the phone, and tabs are extra UI and test surface for no clear gain yet. Revisit tabs when Export (M4) and the snapshot strip (M6) make the control area too tall. The grid controls follow the same patterns as M2 (segmented radios, `.ruler` sliders with ticks for k).

## M3.5 update (2026-11-15) — Settings overlay + steppers (D25)

The on-device M3 check showed the stacked controls ate the preview and the grid sliders were too small to tap. Portrait is now:

```
┌─────────────────────────────────┐        Settings overlay (full-screen, over the photo)
│ Painting Prep        [Settings] │        ┌─────────────────────────────────┐
├─────────────────────────────────┤        │ Settings               [ Done ] │
│                                 │        ├─────────────────────────────────┤
│                                 │        │ [ Choose photo ]   4032 × 3024  │
│            preview              │        │ n      [ − ]  5  [ + ]          │
│      (fills the screen)         │  tap → │ greys  [Even]  Mean             │
│                                 │        │ grid   Off [Square] R×C         │
│                                 │        │ axis   [Width] Height   k [−6+] │
│                                 │        │ diag   Off [On]   over [Study]… │
│                                 │        │ line   Black White Grey [Two]   │
└─────────────────────────────────┘        └─────────────────────────────────┘
```

- The **tab bar above is superseded** by this single overlay. Export (M4) becomes a section at the bottom of the overlay (two-step Export → Share, spec F4), not a separate tab.
- **Steppers** (`− value +`, clamped, button disabled at its limit) replace the `.ruler` sliders for n and the grid counts. Segmented radios are unchanged.
- The `#file` input stays **outside** the overlay (in the preview area) so both "Choose photo" labels reach it and the WebKit test can set files without opening the overlay.
- Rows with two controls (axis+k, diag+over) **wrap** at phone width.
- Debug panel and error banner are unchanged (the debug sheet sits above the Settings overlay so a title long-press still works).

## M5 Phase A update (2026-11-15) — algorithm + blur rows (D28)

Two study rows were added to the Settings overlay between **greys** and **grid**,
following the existing patterns:

- **algo** — a segmented control (Equal / Quant / Jenks), wired like **greys** (it
  re-posterizes, not just repaints).
- **blur** — a − / + stepper, **0–3% in 0.25% steps** (a % of the short side), with
  a **hint line** of recommended values underneath (~0.25% light · ~0.5% medium ·
  ~1% strong). On-device use showed 1% is already a strong squint, hence the fine
  low end and the low cap.

Verified in WebKit at 390 px: both rows fit without overflow. Still pending for
Phase B: the **legend strip** (sketched above, below the photo) + a
legend-in-export toggle, **compare** (hold the photo to peek the original), and
remembered settings (F5).
