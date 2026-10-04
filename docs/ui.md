# UI sketch

*Draft for M2 (2026-10-04). Layout only: sizes are rough, and the look is decided while building. Update this file when the real screens prove it wrong.*

## Principles

- **The photo comes first.** The preview gets as much of the screen as possible. Controls take a compact panel, not a long scrolling page.
- **Controls sit within thumb reach.** In portrait they go at the bottom; the top of an iPhone screen is hard to reach one-handed.
- **Undo, redo, reset and pin are always visible** (spec UI). They live in the top bar, which never scrolls.
- **One control section at a time.** Tabs switch between Photo, Values, Grid and Export, so each panel stays two or three rows tall.
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

There are no tabs yet. M2 shows the photo button and the Values panel stacked under the preview. Tabs arrive once a third section exists (Grid in M3).
