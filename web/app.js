// Painting Prep: pick a photo, decode it to the preview size, and show its
// value study (spec F1, F2). All image logic runs in prep-core via wasm.

// debug.js first: it captures console output from the modules after it.
import { bindLongPress, setInfoProvider } from "./debug.js";
import init, { Processor, gridLines } from "./pkg/prep_wasm.js";

/** Preview working size: long side in px (spec §7). */
const PREVIEW_LONG_SIDE = 2048;
/** Export size cap: total pixels (spec §7, default 12 MP). Capped by area, not
 *  long side, so any aspect ratio stays under the canvas/memory limit. */
const EXPORT_MAX_PX = 12_000_000;

const $ = (id) => document.getElementById(id);
const els = {
  file: $("file"),
  chooseBig: $("choose-big"),
  previewArea: $("preview-area"),
  preview: $("preview"),
  compare: $("compare"),
  nValue: $("n-value"),
  blurValue: $("blur-value"),
  status: $("status"),
  error: $("error"),
  errorText: $("error-text"),
  errorClose: $("error-close"),
  updateBanner: $("update-banner"),
  updateReload: $("update-reload"),
  title: $("title"),
  // Top-bar F6 controls (M6); always reachable, never scroll (spec UI).
  undo: $("undo"),
  redo: $("redo"),
  reset: $("reset"),
  pin: $("pin"),
  // Dock below the preview: legend strip, compare bar, snapshot thumbnails.
  dock: $("dock"),
  legend: $("legend"),
  compareBar: $("compare-bar"),
  compareSource: $("compare-source"),
  compareMode: $("compare-mode"),
  compareHint: $("compare-hint"),
  snaps: $("snaps"),
  snapBanner: $("snap-banner"),
  snapBannerText: $("snap-banner-text"),
  snapBannerCancel: $("snap-banner-cancel"),
  legendExport: $("legend-export"),
  // Settings overlay (D25).
  settings: $("settings"),
  settingsOpen: $("settings-open"),
  settingsClose: $("settings-close"),
  // Grid controls (M3); counts are steppers now (D25).
  kValue: $("k-value"),
  rowsValue: $("rows-value"),
  colsValue: $("cols-value"),
  gridSquareRow: $("grid-square-row"),
  gridRcRow: $("grid-rc-row"),
  gridOptsRow: $("grid-opts-row"),
  gridLineRow: $("grid-line-row"),
  gridNoteRow: $("grid-note-row"),
  // Export controls (M4).
  exportBtn: $("export-btn"),
  shareBtn: $("share-btn"),
  exportDownload: $("export-download"),
  exportNoteRow: $("export-note-row"),
  exportNote: $("export-note"),
};

const state = {
  wasm: null,
  processor: null,
  loaded: false,
  n: Number(els.nValue.textContent),
  bucketMean: $("greys-mean").checked,
  // Bucketing algorithm (M5): equal | quantile | jenks.
  algo: document.querySelector('input[name="algo"]:checked').value,
  // Pre-blur ("squint") as a percentage of the short side (0 = off, §6).
  blur: Number(els.blurValue.textContent),
  // Include the legend strip in the exported image (F2, default off).
  legendInExport: false,
  renderQueued: false,
  paintQueued: false,
  // Undo/redo (F6/D15): an array of settings objects with a cursor. In memory,
  // capped at HISTORY_MAX, survives loading a new photo, never persisted.
  history: [],
  cursor: -1,
  // True while applySettings writes state+DOM, so handlers don't re-push history
  // or re-persist mid-apply.
  applying: false,
  // Pinned snapshots (F6/D16): { id, settings, bitmap }. ImageBitmaps live in
  // JS (not wasm, §7), max SNAP_MAX, cleared on new photo, never persisted.
  snapshots: [],
  pendingPin: false, // waiting for the user to pick a snapshot to replace
  // Compare control (F2/F6): source is "off" | "original" | "snap:<id>".
  compare: { source: "off", mode: "flip", flipped: false, peeking: false },
  // The composited-over-white preview pixels, kept so the grid can be drawn
  // over the original photo (not just the study). Replaced on each load.
  originalImageData: null,
  // The picked File, kept so export can re-decode at export resolution (the
  // preview pixels above are only ~2048 px). Replaced on each successful load.
  sourceFile: null,
  // The encoded export image, produced by the Export step, shared by the Share
  // step (two taps, spec F4). { file, url }.
  exportResult: null,
  grid: {
    mode: "off", // off | square | rowscols
    axis: "width", // width | height (square mode)
    k: Number(els.kValue.textContent),
    rows: Number(els.rowsValue.textContent),
    cols: Number(els.colsValue.textContent),
    diagonals: false,
    over: "study", // study | original
    line: "two", // black | white | grey | two
  },
};

// --- Settings object, control registry, undo/redo, persistence (F5/F6) ---

/** The longest undo history kept in memory (spec F6). */
const HISTORY_MAX = 50;
/** localStorage key + schema version for remembered settings (F5/D7). */
const SETTINGS_KEY = "painting-prep:settings";
const SETTINGS_VERSION = 1;

/**
 * A JSON-safe clone of just the undoable / persisted settings (spec F5, F6).
 * An explicit literal (not a clone of `state`) so non-settings fields like the
 * image buffers and history never leak into the undo stack or localStorage.
 */
function captureSettings() {
  return {
    n: state.n,
    bucketMean: state.bucketMean,
    algo: state.algo,
    blur: state.blur,
    legendInExport: state.legendInExport,
    grid: { ...state.grid },
  };
}

/** The pristine defaults, captured once from the initial (DOM-default) state,
 *  before any F5 restore. Reset returns here (F6). */
const DEFAULTS = captureSettings();

/** Syncers re-apply `state` to the DOM controls. Each control registers one so
 *  undo/redo/reset/restore can refresh the UI without firing change handlers. */
const syncers = [];
function syncControls() {
  for (const fn of syncers) fn();
}

/**
 * Writes a settings object back into `state` and the DOM, then recomputes.
 * Guarded by `state.applying` so it never pushes history or persists mid-apply
 * (setting `.checked`/`.textContent` programmatically also doesn't fire change).
 * `render()` is a no-op until a photo is loaded, so restoring before load is fine.
 */
function applySettings(s) {
  state.applying = true;
  state.n = s.n;
  state.bucketMean = s.bucketMean;
  state.algo = s.algo;
  state.blur = s.blur;
  state.legendInExport = s.legendInExport;
  Object.assign(state.grid, s.grid);
  syncControls();
  updateGridVisibility();
  state.applying = false;
  render();
}

function sameSettings(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}

/**
 * Records the current settings as one undo step (spec F6). The single push site:
 * every finished control change calls it after mutating state and recomputing.
 * Drops the redo tail, dedups no-ops, caps the history, and persists (F5).
 */
function commit() {
  if (state.applying) return;
  const snap = captureSettings();
  if (state.cursor >= 0 && sameSettings(snap, state.history[state.cursor])) {
    saveSettings(); // settings unchanged for history, but persist defensively
    return;
  }
  state.history.length = state.cursor + 1; // drop any redo tail
  state.history.push(snap);
  if (state.history.length > HISTORY_MAX) state.history.shift();
  state.cursor = state.history.length - 1;
  updateHistoryButtons();
  saveSettings();
}

function undo() {
  if (state.cursor <= 0) return;
  state.cursor--;
  applySettings(state.history[state.cursor]);
  updateHistoryButtons();
  saveSettings();
}

function redo() {
  if (state.cursor >= state.history.length - 1) return;
  state.cursor++;
  applySettings(state.history[state.cursor]);
  updateHistoryButtons();
  saveSettings();
}

/** Reset every parameter to its default (spec F6). An ordinary undoable step. */
function reset() {
  applySettings(DEFAULTS);
  commit();
}

/** Reflects history depth on the buttons and the debug hooks. */
function updateHistoryButtons() {
  els.undo.disabled = state.cursor <= 0;
  els.redo.disabled = state.cursor >= state.history.length - 1;
  if (window.prepDebug) {
    window.prepDebug.undoDepth = state.cursor;
    window.prepDebug.redoDepth = state.history.length - 1 - state.cursor;
  }
}

/** Seeds history with the current settings as the base step (survives loads). */
function seedHistory() {
  state.history = [captureSettings()];
  state.cursor = 0;
  updateHistoryButtons();
}

/** Persists the current settings as one small versioned object (F5/D7). Images
 *  are never stored. Wrapped in try/catch: storage may be full or unavailable. */
function saveSettings() {
  try {
    localStorage.setItem(
      SETTINGS_KEY,
      JSON.stringify({ version: SETTINGS_VERSION, ...captureSettings() }),
    );
  } catch {
    /* private mode / quota / disabled: settings just won't persist (F5). */
  }
}

/** Reads remembered settings (F5). Missing, corrupt, or old-version data falls
 *  back to defaults without an error. Returns true if anything was applied. */
function restoreSettings() {
  let data;
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (!raw) return false;
    data = JSON.parse(raw);
  } catch {
    return false;
  }
  if (!data || data.version !== SETTINGS_VERSION) return false;
  applySettings(sanitizeSettings(data));
  return true;
}

/** Validates each field of stored settings, falling back to the default per
 *  field, so partial or tampered data can never break the app (F5). */
function sanitizeSettings(d) {
  const clampInt = (v, min, max, dflt) =>
    Number.isFinite(v) && v >= min && v <= max ? Math.round(v) : dflt;
  const oneOf = (v, allowed, dflt) => (allowed.includes(v) ? v : dflt);
  const bool = (v, dflt) => (typeof v === "boolean" ? v : dflt);
  const g = d.grid ?? {};
  const dg = DEFAULTS.grid;
  return {
    n: clampInt(d.n, 2, 10, DEFAULTS.n),
    bucketMean: bool(d.bucketMean, DEFAULTS.bucketMean),
    algo: oneOf(d.algo, ["equal", "quantile", "jenks"], DEFAULTS.algo),
    blur: Number.isFinite(d.blur) && d.blur >= 0 && d.blur <= 3 ? d.blur : DEFAULTS.blur,
    legendInExport: bool(d.legendInExport, DEFAULTS.legendInExport),
    grid: {
      mode: oneOf(g.mode, ["off", "square", "rowscols"], dg.mode),
      axis: oneOf(g.axis, ["width", "height"], dg.axis),
      k: clampInt(g.k, 2, 16, dg.k),
      rows: clampInt(g.rows, 1, 12, dg.rows),
      cols: clampInt(g.cols, 1, 12, dg.cols),
      diagonals: bool(g.diagonals, dg.diagonals),
      over: oneOf(g.over, ["study", "original"], dg.over),
      line: oneOf(g.line, ["black", "white", "grey", "two"], dg.line),
    },
  };
}

/** Facts about the current photo and the last run, for the debug panel. */
const stats = {
  source: null, // { width, height }
  working: null, // { width, height }
  decodePath: null,
  export: null, // { width, height } of the last successful export (canvas-limit probe, §7)
  timings: {}, // step → ms
};

/** Runs `fn` and records its duration under `label`. */
async function timed(label, fn) {
  const t0 = performance.now();
  try {
    return await fn();
  } finally {
    stats.timings[label] = performance.now() - t0;
  }
}

function showError(message) {
  els.errorText.textContent = message;
  els.error.hidden = false;
}

function clearError() {
  els.error.hidden = true;
}

/** Working size for a `w × h` source: long side at most `limit`, never upscaled. */
function workingSize(w, h, limit) {
  const scale = Math.min(1, limit / Math.max(w, h));
  return {
    width: Math.max(1, Math.round(w * scale)),
    height: Math.max(1, Math.round(h * scale)),
  };
}

/** Export size for a `w × h` source: at most `EXPORT_MAX_PX` total pixels,
 *  never upscaled (spec §7). Capped by area so any aspect ratio stays safe. */
function exportSize(w, h) {
  const scale = Math.min(1, Math.sqrt(EXPORT_MAX_PX / (w * h)));
  return {
    width: Math.max(1, Math.round(w * scale)),
    height: Math.max(1, Math.round(h * scale)),
  };
}

/**
 * Decodes `file` straight to the size `computeTarget(source)` returns, and gives
 * back its sRGB pixels (spec F1). Never draws or decodes the full-resolution
 * photo on purpose. Used for both the preview and the export, at their own sizes.
 */
async function decodeToSize(file, computeTarget) {
  const url = URL.createObjectURL(file);
  const img = new Image();
  try {
    // The load event gives the oriented natural size (EXIF applied) from the
    // header. Not img.decode(): that decodes the full-resolution pixels, about
    // 260 MB for a 64 MP photo, and iOS then runs out of memory (D23).
    await timed("header", () => loadImage(img, url));
    const source = { width: img.naturalWidth, height: img.naturalHeight };
    const target = computeTarget(source);
    const tooBig = () =>
      new Error(
        `not enough memory for a ${source.width} × ${source.height} photo. ` +
          "Try a smaller one.",
      );

    const canvas = document.createElement("canvas");
    canvas.width = target.width;
    canvas.height = target.height;
    const ctx = canvas.getContext("2d", { colorSpace: "srgb", willReadFrequently: true });
    if (!ctx) throw tooBig();
    // White under the photo: a second guard for transparent input (D19), and
    // the marker for a failed canvas (see below).
    ctx.fillStyle = "#fff";
    ctx.fillRect(0, 0, target.width, target.height);
    ctx.imageSmoothingQuality = "high";

    const decodePath = await timed("draw", () => drawScaled(ctx, file, img, source, target));
    const imageData = await timed("getImageData", () =>
      ctx.getImageData(0, 0, target.width, target.height, { colorSpace: "srgb" }),
    );
    // Release the canvas backing store now rather than at GC.
    canvas.width = canvas.height = 0;
    // Under memory pressure WebKit doesn't throw: the canvas silently reads back
    // as transparent black. Over an opaque white fill every pixel must be opaque.
    if (!allOpaque(imageData.data)) throw tooBig();
    return { imageData, source, target, decodePath };
  } finally {
    img.src = ""; // drop any decoded pixels
    URL.revokeObjectURL(url);
  }
}

/** Resolves when `img` has loaded `url`'s header; rejects if it isn't an image. */
function loadImage(img, url) {
  return new Promise((resolve, reject) => {
    img.onload = () => resolve();
    img.onerror = () => reject(new Error("this browser can't read it as an image."));
    img.src = url;
  });
}

/** True if every pixel of RGBA8 `data` has alpha 255. */
function allOpaque(data) {
  for (let i = 3; i < data.length; i += 4) {
    if (data[i] !== 255) return false;
  }
  return true;
}

/**
 * Draws the photo into `ctx` at `target` size. Prefers a resizing
 * `createImageBitmap` (decodes once at the small size), and falls back to
 * `drawImage` with a destination size. Returns which path ran, which answers
 * spec open question 3 on the device.
 */
async function drawScaled(ctx, file, img, source, target) {
  if (target.width === source.width && target.height === source.height) {
    ctx.drawImage(img, 0, 0);
    return "drawImage, no resize";
  }
  let bitmap = null;
  try {
    // Only the width is given, so the height shows whether the bitmap kept the
    // oriented aspect ratio. It doesn't if orientation or resizing is ignored.
    bitmap = await createImageBitmap(file, {
      resizeWidth: target.width,
      resizeQuality: "high",
      imageOrientation: "from-image",
    });
    if (bitmap.width === target.width && Math.abs(bitmap.height - target.height) <= 1) {
      ctx.drawImage(bitmap, 0, 0, target.width, target.height);
      return "createImageBitmap resize";
    }
    console.warn(
      `createImageBitmap gave ${bitmap.width} × ${bitmap.height}, ` +
        `expected ${target.width} × ${target.height}; using drawImage`,
    );
  } catch (e) {
    console.warn(`createImageBitmap resize failed (${e}); using drawImage`);
  } finally {
    bitmap?.close();
  }
  ctx.drawImage(img, 0, 0, target.width, target.height);
  return "drawImage resize";
}

async function loadPhoto(file) {
  clearError();
  document.body.dataset.busy = "true";
  const previousStatus = els.status.textContent;
  els.status.textContent = "Loading…";
  // Keep the shown photo's timings if this load fails.
  const previousTimings = stats.timings;
  stats.timings = {};
  try {
    const { imageData, source, target, decodePath } = await decodeToSize(file, (s) =>
      workingSize(s.width, s.height, PREVIEW_LONG_SIDE),
    );
    state.loaded = false;
    try {
      await timed("load", () =>
        state.processor.load(new Uint8Array(imageData.data.buffer), target.width, target.height),
      );
    } catch (e) {
      // A trap (e.g. out of memory) can leave the processor unusable; start over.
      state.processor.free();
      state.processor = new Processor();
      els.preview.hidden = true;
      els.chooseBig.hidden = false;
      throw e;
    }
    Object.assign(stats, { source, working: target, decodePath });
    els.preview.width = target.width;
    els.preview.height = target.height;
    // Keep the original pixels for "grid over original" (already over white),
    // and the File so export can re-decode at export resolution.
    state.originalImageData = imageData;
    state.sourceFile = file;
    resetExportUI(); // a new photo invalidates any pending export
    clearSnapshots(); // the previous photo's snapshots don't apply (D16)
    els.exportBtn.disabled = false;
    els.pin.disabled = false;
    state.loaded = true;
    updateDockVisibility();
    render();
    renderCompareSources(); // "original" source now points at this photo
    refreshCompare();
    els.preview.hidden = false;
    els.chooseBig.hidden = true;
    els.status.textContent =
      `${source.width} × ${source.height}` +
      (decodePath === "drawImage, no resize" ? "" : ` → ${target.width} × ${target.height}`);
    console.info(`loaded ${els.status.textContent} via ${decodePath}`, stats.timings);
  } catch (e) {
    console.error(e);
    if (state.loaded) {
      stats.timings = previousTimings;
      els.status.textContent = previousStatus;
    } else {
      els.status.textContent = "";
    }
    showError(`Couldn't open that photo: ${e.message ?? e}`);
  } finally {
    delete document.body.dataset.busy;
    els.file.value = ""; // lets the same file be picked again
  }
}

/** Re-posterizes the study for the current n/greys/algorithm/blur, then repaints. */
function render() {
  if (!state.loaded) return;
  try {
    timedSync("render", () =>
      state.processor.render(state.n, state.bucketMean, state.algo, state.blur),
    );
  } catch (e) {
    console.error(e);
    showError(`Couldn't render the study: ${e.message ?? e}`);
    return;
  }
  renderLegend();
  paint();
}

/**
 * Repaints the preview: the base image (study, or the original photo when the
 * grid is drawn over it), then the grid on top. Doesn't re-posterize, so it's
 * the cheap path for grid-only changes (spec §5 caching table).
 */
function paint() {
  if (!state.loaded) return;
  try {
    const p = state.processor;
    const ctx = els.preview.getContext("2d", { colorSpace: "srgb" });
    timedSync("putImageData", () => {
      const overOriginal =
        state.grid.mode !== "off" && state.grid.over === "original" && state.originalImageData;
      if (overOriginal) {
        ctx.putImageData(state.originalImageData, 0, 0);
      } else {
        // A view on wasm memory, made after render's allocations (spec §7).
        const view = new Uint8ClampedArray(state.wasm.memory.buffer, p.output_ptr(), p.output_len());
        ctx.putImageData(new ImageData(view, p.width(), p.height()), 0, 0);
      }
    });
    timedSync("grid", () => drawGrid(ctx, p.width(), p.height()));
    // Flag a partial last cell in Square mode (open question 2 → D24).
    if (state.grid.mode !== "off") {
      els.gridNoteRow.hidden = partialRemainder(p.width(), p.height(), state.grid) <= 0;
    }
    // Signals the finished paint to the WebKit smoke test.
    els.preview.dataset.rendered =
      `${state.n},${state.bucketMean ? "mean" : "even"},${state.algo},${state.blur}`;
  } catch (e) {
    console.error(e);
    showError(`Couldn't draw the preview: ${e.message ?? e}`);
  }
}

/** Draws the grid overlay at `w × h` (the target canvas's size, so the same
 *  image-relative geometry scales to preview or export, D9). No-op when off. */
function drawGrid(ctx, w, h) {
  const g = state.grid;
  if (g.mode === "off") return;
  const coords = gridLines(w, h, g.mode, g.axis, g.k, g.rows, g.cols, g.diagonals);
  const path = new Path2D();
  for (let i = 0; i + 3 < coords.length; i += 4) {
    path.moveTo(coords[i], coords[i + 1]);
    path.lineTo(coords[i + 2], coords[i + 3]);
  }
  // Line width ≈ 0.15% of the short side, at least 1 px (spec F3).
  strokeGrid(ctx, path, Math.max(1, 0.0015 * Math.min(w, h)), g.line);
}

/** Strokes `path`, with two-tone = a dark outline under a light core so the
 *  grid stays visible on any background (spec F3). */
function strokeGrid(ctx, path, lineWidth, style) {
  if (style === "two") {
    ctx.lineWidth = lineWidth + 2;
    ctx.strokeStyle = "rgba(0, 0, 0, 0.85)";
    ctx.stroke(path);
    ctx.lineWidth = lineWidth;
    ctx.strokeStyle = "rgba(255, 255, 255, 0.95)";
    ctx.stroke(path);
    return;
  }
  const colours = { black: "#000000", white: "#ffffff", grey: "#808080" };
  ctx.lineWidth = lineWidth;
  ctx.strokeStyle = colours[style] ?? "#000000";
  ctx.stroke(path);
}

/** Leftover pixels on the non-reference axis in Square mode (0 if it divides
 *  evenly or the mode isn't Square). Mirrors prep-core's geometry. */
function partialRemainder(w, h, g) {
  if (g.mode !== "square") return 0;
  const refLen = g.axis === "width" ? w : h;
  const otherLen = g.axis === "width" ? h : w;
  const cell = refLen / g.k;
  const full = Math.floor(otherLen / cell + 1e-4);
  const rem = otherLen - cell * full;
  return rem > 0.5 ? rem : 0;
}

/** Clears any pending export: hides Share/Save and revokes the blob URL. */
function resetExportUI() {
  if (state.exportResult) {
    URL.revokeObjectURL(state.exportResult.url);
    state.exportResult = null;
  }
  els.shareBtn.hidden = true;
  els.exportDownload.hidden = true;
  els.exportNoteRow.hidden = true;
  els.exportNote.textContent = "";
}

/** Encodes `canvas` to a PNG `File`. PNG is lossless and a value study is a few
 *  flat greys, so it stays small; a grid-over-original photo is larger. */
function canvasToFile(canvas, name) {
  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => {
      if (blob) resolve(new File([blob], name, { type: blob.type }));
      else reject(new Error("couldn't encode the image"));
    }, "image/png");
  });
}

/**
 * Export step 1 (spec F4): render the current view (study and/or grid) at export
 * resolution and encode it to a `File`. Posterize reuses the preview's
 * thresholds via `renderExport` (D11); the grid geometry scales to export size
 * (D9). Step 2 (Share) then appears and runs in its own tap.
 */
async function exportImage() {
  if (!state.loaded || !state.sourceFile) return;
  resetExportUI();
  els.exportBtn.disabled = true;
  document.body.dataset.busy = "true";
  els.exportNoteRow.hidden = false;
  els.exportNote.textContent = "Rendering…";
  try {
    const { imageData, target, decodePath } = await timed("export-decode", () =>
      decodeToSize(state.sourceFile, (s) => exportSize(s.width, s.height)),
    );
    stats.export = target; // canvas-limit probe (§7): this size decoded opaque
    const g = state.grid;
    const overOriginal = g.mode !== "off" && g.over === "original";

    // Optional legend band at the bottom (F2). Read the greys/counts of the
    // current preview study before the export render (which doesn't change them).
    let legendGreys = [];
    let legendCounts = [];
    if (state.legendInExport) {
      try {
        legendGreys = state.processor.greys();
        legendCounts = state.processor.counts();
      } catch (e) {
        console.warn(`legend unavailable for export: ${e}`);
      }
    }
    const bandH = legendGreys.length ? legendBandHeight(target.height) : 0;

    const canvas = document.createElement("canvas");
    canvas.width = target.width;
    canvas.height = target.height + bandH;
    const ctx = canvas.getContext("2d", { colorSpace: "srgb" });
    if (!ctx) throw new Error("couldn't get an export canvas");

    timedSync("export-render", () => {
      if (overOriginal) {
        ctx.putImageData(imageData, 0, 0);
      } else {
        state.processor.renderExport(
          new Uint8Array(imageData.data.buffer),
          target.width,
          target.height,
          state.n,
          state.bucketMean,
          state.algo,
          state.blur,
        );
        const p = state.processor;
        // View made after renderExport's allocations (spec §7).
        const view = new Uint8ClampedArray(state.wasm.memory.buffer, p.export_ptr(), p.export_len());
        ctx.putImageData(new ImageData(view, target.width, target.height), 0, 0);
      }
      drawGrid(ctx, target.width, target.height);
      if (bandH) drawLegendBand(ctx, legendGreys, legendCounts, 0, target.height, target.width, bandH);
    });

    const name = `painting-prep-${g.mode === "off" ? "study" : "grid"}.png`;
    const file = await timed("export-encode", () => canvasToFile(canvas, name));
    canvas.width = canvas.height = 0; // release the backing store
    const url = URL.createObjectURL(file);
    state.exportResult = { file, url };

    // Step 2: Share (to Photos) when supported; a Save link always (saves to
    // Files on iOS — the fallback when Web Share is absent, spec F4).
    els.exportDownload.href = url;
    els.exportDownload.download = name;
    els.exportDownload.hidden = false;
    const canShare = navigator.canShare?.({ files: [file] }) ?? false;
    els.shareBtn.hidden = !canShare;
    const size = `${target.width} × ${target.height}`;
    els.exportNote.textContent = canShare
      ? `Ready (${size}). Tap Share → Save Image.`
      : `Ready (${size}). Tap Save (saving to Photos needs the installed HTTPS app).`;
    console.info(`exported ${size} via ${decodePath}`, stats.timings);
  } catch (e) {
    console.error(e);
    resetExportUI();
    showError(`Couldn't export: ${e.message ?? e}`);
  } finally {
    els.exportBtn.disabled = false;
    delete document.body.dataset.busy;
  }
}

/** Export step 2 (spec F4): share the encoded File through the iOS share sheet,
 *  in its own tap so Web Share isn't rejected after the slow render. */
async function shareExport() {
  const res = state.exportResult;
  if (!res) return;
  if (!(navigator.canShare?.({ files: [res.file] }) ?? false)) {
    showError("Sharing isn't available here; use Save instead.");
    return;
  }
  try {
    await navigator.share({ files: [res.file], title: "Painting Prep" });
  } catch (e) {
    if (e.name !== "AbortError") showError(`Couldn't share: ${e.message ?? e}`);
  }
}

/** Shows only the grid sub-rows that apply to the current mode. */
function updateGridVisibility() {
  const on = state.grid.mode !== "off";
  els.gridSquareRow.hidden = state.grid.mode !== "square";
  els.gridRcRow.hidden = state.grid.mode !== "rowscols";
  els.gridOptsRow.hidden = !on;
  els.gridLineRow.hidden = !on;
  if (!on) els.gridNoteRow.hidden = true;
}

// --- Legend strip (F2) ---

/** The on-screen legend band height in export pixels, when included (F2). */
function legendBandHeight(targetH) {
  return Math.max(48, Math.round(targetH * 0.06));
}

/**
 * Draws the legend strip (F2): the n output greys dark→light, each labelled with
 * the percentage of pixels in its bucket. Reads the last render's greys/counts
 * (valid after `processor.render`); empty buckets show 0% (§6).
 */
function renderLegend() {
  if (!state.loaded) {
    els.legend.hidden = true;
    return;
  }
  let greys, counts;
  try {
    greys = state.processor.greys();
    counts = state.processor.counts();
  } catch (e) {
    console.warn(`legend unavailable: ${e}`);
    els.legend.hidden = true;
    return;
  }
  const total = counts.reduce((a, b) => a + b, 0) || 1;
  els.legend.replaceChildren(
    ...Array.from(greys, (g, i) => {
      const cell = document.createElement("div");
      cell.className = "legend-cell";
      cell.style.background = `rgb(${g},${g},${g})`;
      const pct = document.createElement("span");
      pct.className = "legend-pct";
      pct.style.color = g < 110 ? "#fff" : "#000"; // readable on its own grey
      pct.textContent = `${Math.round((counts[i] / total) * 100)}%`;
      cell.append(pct);
      return cell;
    }),
  );
  els.legend.dataset.count = String(greys.length);
  els.legend.hidden = false;
}

/** Draws the legend as a band `h` px tall at the bottom of an export canvas
 *  (F2, when the toggle is on). Mirrors the on-screen strip. */
function drawLegendBand(ctx, greys, counts, x, y, w, h) {
  const total = counts.reduce((a, b) => a + b, 0) || 1;
  const cellW = w / greys.length;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.font = `${Math.max(10, Math.round(h * 0.3))}px -apple-system, system-ui, sans-serif`;
  for (let i = 0; i < greys.length; i++) {
    const g = greys[i];
    ctx.fillStyle = `rgb(${g},${g},${g})`;
    ctx.fillRect(x + i * cellW, y, Math.ceil(cellW), h);
    ctx.fillStyle = g < 110 ? "#fff" : "#000";
    ctx.fillText(`${Math.round((counts[i] / total) * 100)}%`, x + (i + 0.5) * cellW, y + h / 2);
  }
}

// --- Dock, snapshots, and compare (F6) ---

/** The dock (legend + compare bar + snapshots) appears once a photo is loaded. */
function updateDockVisibility() {
  els.dock.hidden = !state.loaded;
  els.compareBar.hidden = !state.loaded;
}

/** Max pinned snapshots (D16/§7): each is a preview-size ImageBitmap in JS. */
const SNAP_MAX = 4;
let nextSnapId = 0;

/** Pin the current preview (study + any grid) as a snapshot (F6/D16). */
async function pinSnapshot() {
  if (!state.loaded) return;
  if (state.snapshots.length >= SNAP_MAX) {
    enterReplaceMode();
    return;
  }
  try {
    const bitmap = await createImageBitmap(els.preview);
    state.snapshots.push({ id: ++nextSnapId, settings: captureSettings(), bitmap });
  } catch (e) {
    console.error(e);
    showError(`Couldn't pin a snapshot: ${e.message ?? e}`);
    return;
  }
  renderSnapshotStrip();
  renderCompareSources();
}

function enterReplaceMode() {
  state.pendingPin = true;
  els.snapBannerText.textContent = "Snapshots full — tap one to replace.";
  els.snapBanner.hidden = false;
}

function cancelReplaceMode() {
  state.pendingPin = false;
  els.snapBanner.hidden = true;
}

/** Replace snapshot `index` with the current preview (F6 max-4 flow). */
async function replaceSnapshot(index) {
  let bitmap;
  try {
    bitmap = await createImageBitmap(els.preview);
  } catch (e) {
    console.error(e);
    showError(`Couldn't replace the snapshot: ${e.message ?? e}`);
    cancelReplaceMode();
    return;
  }
  state.snapshots[index].bitmap.close();
  state.snapshots[index] = { id: ++nextSnapId, settings: captureSettings(), bitmap };
  cancelReplaceMode();
  renderSnapshotStrip();
  renderCompareSources();
}

/** Rebuilds the thumbnail strip. A tap restores a snapshot's settings (an
 *  undoable step); in replace mode a tap swaps that snapshot instead (F6). */
function renderSnapshotStrip() {
  els.snaps.replaceChildren(
    ...state.snapshots.map((snap, i) => {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "snap";
      btn.dataset.snapIndex = String(i);
      btn.setAttribute("aria-label", `Snapshot ${i + 1}`);
      const thumb = document.createElement("canvas");
      const tw = 64;
      thumb.width = tw;
      thumb.height = Math.max(1, Math.round((snap.bitmap.height / snap.bitmap.width) * tw));
      thumb.getContext("2d").drawImage(snap.bitmap, 0, 0, thumb.width, thumb.height);
      btn.append(thumb);
      btn.addEventListener("click", () => {
        if (state.pendingPin) {
          replaceSnapshot(i);
        } else {
          applySettings(snap.settings);
          commit();
        }
      });
      return btn;
    }),
  );
  els.snaps.dataset.count = String(state.snapshots.length);
  els.snaps.hidden = state.snapshots.length === 0;
  if (window.prepDebug) window.prepDebug.snapshotCount = state.snapshots.length;
}

/** Releases every snapshot bitmap and clears the strip (new photo, D16). */
function clearSnapshots() {
  for (const snap of state.snapshots) snap.bitmap.close();
  state.snapshots = [];
  cancelReplaceMode();
  if (state.compare.source.startsWith("snap:")) state.compare.source = "off";
  renderSnapshotStrip();
}

function sizeCompareToPreview() {
  els.compare.width = els.preview.width;
  els.compare.height = els.preview.height;
}

/** Paints the chosen comparison (original or a snapshot) into the compare ctx. */
function drawCompareInto(ctx) {
  const src = state.compare.source;
  if (src === "original") {
    if (state.originalImageData) ctx.putImageData(state.originalImageData, 0, 0);
  } else if (src.startsWith("snap:")) {
    const id = Number(src.slice(5));
    const snap = state.snapshots.find((s) => s.id === id);
    if (snap) ctx.drawImage(snap.bitmap, 0, 0);
  }
}

/** Whether the compare canvas should be shown right now. */
function compareVisible() {
  if (state.compare.source === "off") return false;
  if (state.compare.mode === "side") return true;
  return state.compare.flipped || state.compare.peeking; // flip mode
}

function refreshCompareVisibility() {
  els.compare.hidden = !compareVisible();
}

/**
 * Redraws the compare layer and updates layout + button states. Called when the
 * source, mode, or the underlying photo changes — never on a settings edit, so
 * the frozen comparison stays put while the live view updates underneath (F6).
 */
function refreshCompare() {
  const area = els.previewArea;
  area.dataset.compare =
    state.compare.source === "off"
      ? "off"
      : state.compare.source === "original"
        ? "original"
        : "snap";
  area.dataset.compareMode = state.compare.mode;
  area.classList.toggle(
    "side",
    state.compare.mode === "side" && state.compare.source !== "off",
  );
  if (state.compare.source !== "off") {
    sizeCompareToPreview();
    const ctx = els.compare.getContext("2d", { colorSpace: "srgb" });
    ctx.clearRect(0, 0, els.compare.width, els.compare.height);
    drawCompareInto(ctx);
  }
  refreshCompareVisibility();
  // The tap/hold gesture lives on the photo, so the view buttons do nothing
  // visible on their own; show a hint whenever that mode is armed with a source.
  els.compareHint.hidden = !(state.compare.source !== "off" && state.compare.mode === "flip");
  for (const b of els.compareSource.querySelectorAll("[data-source]")) {
    b.setAttribute("aria-pressed", String(b.dataset.source === state.compare.source));
  }
  for (const b of els.compareMode.querySelectorAll("[data-mode]")) {
    b.setAttribute("aria-pressed", String(b.dataset.mode === state.compare.mode));
  }
}

function setCompareSource(source) {
  state.compare.source = source;
  state.compare.flipped = false;
  state.compare.peeking = false;
  refreshCompare();
}

function setCompareMode(mode) {
  state.compare.mode = mode;
  state.compare.flipped = false;
  state.compare.peeking = false;
  refreshCompare();
}

/** Rebuilds the compare source buttons: Off, Orig, then one per snapshot (F6). */
function renderCompareSources() {
  const add = (value, label) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "seg-btn";
    b.dataset.source = value;
    b.textContent = label;
    b.setAttribute("aria-pressed", String(state.compare.source === value));
    b.addEventListener("click", () => setCompareSource(value));
    return b;
  };
  const buttons = [add("off", "Off"), add("original", "Orig")];
  state.snapshots.forEach((snap, i) => buttons.push(add(`snap:${snap.id}`, `S${i + 1}`)));
  els.compareSource.replaceChildren(...buttons);
}

/**
 * Tap the preview to flip live↔compare, or hold to peek, in flip mode (F2/F6).
 * Modelled on bindLongPress (debug.js). A no-op when compare is off or in side
 * mode. The `.no-callout` class + the contextmenu guard stop the iOS callout.
 */
const COMPARE_HOLD_MS = 180;
function bindCompareGesture(el) {
  let timer = null;
  let held = false;
  const active = () => state.compare.source !== "off" && state.compare.mode === "flip";
  el.addEventListener("pointerdown", () => {
    if (!active()) return;
    held = false;
    clearTimeout(timer);
    timer = setTimeout(() => {
      held = true;
      state.compare.peeking = true;
      refreshCompareVisibility();
    }, COMPARE_HOLD_MS);
  });
  const end = () => {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    if (!active()) return;
    if (held) {
      state.compare.peeking = false;
    } else {
      state.compare.flipped = !state.compare.flipped;
    }
    refreshCompareVisibility();
  };
  for (const type of ["pointerup", "pointercancel", "pointerleave"]) {
    el.addEventListener(type, end);
  }
  el.addEventListener("contextmenu", (e) => e.preventDefault());
}

function timedSync(label, fn) {
  const t0 = performance.now();
  try {
    return fn();
  } finally {
    stats.timings[label] = performance.now() - t0;
  }
}

/** Coalesces slider events into at most one render per frame. */
function queueRender() {
  if (state.renderQueued) return;
  state.renderQueued = true;
  requestAnimationFrame(() => {
    state.renderQueued = false;
    render();
  });
}

/** Coalesces grid-slider events into at most one repaint per frame (a repaint
 *  skips re-posterizing, spec §5). */
function queuePaint() {
  if (state.paintQueued) return;
  state.paintQueued = true;
  requestAnimationFrame(() => {
    state.paintQueued = false;
    paint();
  });
}

function bindEvents() {
  els.file.addEventListener("change", () => {
    const file = els.file.files?.[0];
    if (file) loadPhoto(file);
  });
  els.settingsOpen.addEventListener("click", () => {
    els.settings.hidden = false;
  });
  els.settingsClose.addEventListener("click", () => {
    els.settings.hidden = true;
  });
  // n changes the study → queueRender; grid counts are grid-only → queuePaint
  // (spec §5 caching). Every control below also commits one undo step (F6).
  bindStepper("n", 2, 10, () => state.n, (v) => (state.n = v), queueRender);
  bindRadio(
    "greys",
    () => (state.bucketMean ? "mean" : "even"),
    (v) => (state.bucketMean = v === "mean"),
    render,
  );
  // Algorithm + blur change the study, so they re-posterize (not just paint).
  bindRadio("algo", () => state.algo, (v) => (state.algo = v), render);
  // Blur is a % of the short side, so even ~1% is a strong squint; fine 0.25%
  // steps with a low cap keep the useful low end reachable (max 3% ≈ heavy).
  bindStepper("blur", 0, 3, () => state.blur, (v) => (state.blur = v), queueRender, 0.25);

  // Grid controls (M3). All are grid-only changes, so they repaint without
  // re-posterizing — except grid mode, which also toggles its sub-rows.
  bindRadio(
    "grid-mode",
    () => state.grid.mode,
    (v) => (state.grid.mode = v),
    () => {
      updateGridVisibility();
      paint();
    },
  );
  bindRadio("grid-axis", () => state.grid.axis, (v) => (state.grid.axis = v), paint);
  bindRadio(
    "grid-diag",
    () => (state.grid.diagonals ? "on" : "off"),
    (v) => (state.grid.diagonals = v === "on"),
    paint,
  );
  bindRadio("grid-over", () => state.grid.over, (v) => (state.grid.over = v), paint);
  bindRadio("grid-line", () => state.grid.line, (v) => (state.grid.line = v), paint);
  bindStepper("k", 2, 16, () => state.grid.k, (v) => (state.grid.k = v), queuePaint);
  bindStepper("rows", 1, 12, () => state.grid.rows, (v) => (state.grid.rows = v), queuePaint);
  bindStepper("cols", 1, 12, () => state.grid.cols, (v) => (state.grid.cols = v), queuePaint);

  // Legend-in-export toggle (F2): a plain checkbox, persisted (F5), undoable.
  syncers.push(() => {
    els.legendExport.checked = state.legendInExport;
  });
  els.legendExport.addEventListener("change", () => {
    state.legendInExport = els.legendExport.checked;
    commit();
  });

  // Top-bar F6 controls.
  els.undo.addEventListener("click", undo);
  els.redo.addEventListener("click", redo);
  els.reset.addEventListener("click", reset);
  els.pin.addEventListener("click", pinSnapshot);

  // Compare controls (F2/F6). Source buttons are built in renderCompareSources;
  // the mode (flip/side) buttons are static.
  for (const b of els.compareMode.querySelectorAll("[data-mode]")) {
    b.addEventListener("click", () => setCompareMode(b.dataset.mode));
  }
  els.snapBannerCancel.addEventListener("click", cancelReplaceMode);
  bindCompareGesture(els.preview);

  // Export is two taps (spec F4): Export renders + encodes, then Share saves.
  els.exportBtn.addEventListener("click", exportImage);
  els.shareBtn.addEventListener("click", shareExport);

  els.errorClose.addEventListener("click", clearError);
}

/**
 * Wires a segmented radio group. `get` returns the current value (for the DOM
 * syncer used by undo/redo/reset/restore), `apply` stores the picked value,
 * `after` recomputes (render or paint); each pick is one undo step (F6).
 */
function bindRadio(name, get, apply, after) {
  const group = document.querySelectorAll(`input[name="${name}"]`);
  syncers.push(() => {
    const v = String(get());
    for (const radio of group) radio.checked = radio.value === v;
  });
  for (const radio of group) {
    radio.addEventListener("change", () => {
      if (!radio.checked) return;
      apply(radio.value);
      after();
      commit();
    });
  }
}

/**
 * Wires a − / + number stepper (`<id>-dec`, `<id>-value`, `<id>-inc`). Each tap
 * clamps the value ±`stepBy` (default 1) to `[min, max]`, stores it via `apply`,
 * updates the output, disables a button at its limit, then runs `after` (render
 * or paint). Replaces the sliders, which were too small to hit on the phone
 * (D25). `stepBy` can be fractional for a fine control like blur (M5); use a
 * power-of-two fraction (e.g. 0.25) so the displayed value stays exact.
 */
function bindStepper(id, min, max, get, apply, after, stepBy = 1) {
  const dec = $(`${id}-dec`);
  const inc = $(`${id}-inc`);
  const out = $(`${id}-value`);
  const sync = () => {
    const v = get();
    out.textContent = v;
    dec.disabled = v <= min;
    inc.disabled = v >= max;
  };
  const change = (delta) => () => {
    const v = Math.min(max, Math.max(min, get() + delta));
    if (v === get()) return;
    apply(v);
    sync();
    after();
    commit(); // each finished tap is one undo step (F6)
  };
  dec.addEventListener("click", change(-stepBy));
  inc.addEventListener("click", change(stepBy));
  syncers.push(sync); // lets undo/redo/reset/restore refresh this stepper
  sync();
}

/** Rows for the debug panel (spec §7). */
function debugInfo() {
  const size = (s) => (s ? `${s.width} × ${s.height}` : "–");
  const rows = {
    source: size(stats.source),
    working: size(stats.working),
    export: size(stats.export),
    decodePath: stats.decodePath ?? "–",
    settings: `n = ${state.n}, greys = ${state.bucketMean ? "mean" : "even"}, algo = ${state.algo}, blur = ${state.blur}%, grid = ${state.grid.mode}`,
    history: `${state.cursor + 1}/${state.history.length}`,
    snapshots: `${state.snapshots.length}/${SNAP_MAX}`,
    compare: `${state.compare.source} (${state.compare.mode})`,
    // Wasm memory only grows (spec §7), so this is the session's peak.
    wasmMemory: state.wasm ? `${(state.wasm.memory.buffer.byteLength / 2 ** 20).toFixed(1)} MB` : "–",
  };
  for (const [step, ms] of Object.entries(stats.timings)) {
    rows[`${step} (ms)`] = ms.toFixed(1);
  }
  return rows;
}

/**
 * Registers the service worker for offline + install (spec M4). Skipped unless
 * on HTTPS — service workers need a secure context, and skipping on the
 * plain-HTTP LAN and on localhost keeps the WebKit smoke test (served over http)
 * free of cache interference. When a new deployed version finishes installing
 * behind the current one, show a "reload to update" hint instead of switching
 * mid-session (spec §7).
 */
function registerServiceWorker() {
  if (!("serviceWorker" in navigator) || location.protocol !== "https:") return;
  navigator.serviceWorker
    .register("./sw.js")
    .then((reg) => {
      reg.addEventListener("updatefound", () => {
        const sw = reg.installing;
        if (!sw) return;
        sw.addEventListener("statechange", () => {
          // installed + an existing controller = an update is waiting, not a
          // first install.
          if (sw.state === "installed" && navigator.serviceWorker.controller) {
            els.updateReload.onclick = () => reg.waiting?.postMessage("skip-waiting");
            els.updateBanner.hidden = false;
          }
        });
      });
    })
    .catch((e) => console.warn(`service worker registration failed: ${e}`));

  // When the waiting worker takes control (after skip-waiting), reload once.
  let reloading = false;
  navigator.serviceWorker.addEventListener("controllerchange", () => {
    if (reloading) return;
    reloading = true;
    location.reload();
  });
}

async function main() {
  updateGridVisibility();
  setInfoProvider(debugInfo);
  bindLongPress(els.title);
  bindEvents(); // also syncs each stepper's output + disabled state
  restoreSettings(); // remembered settings (F5); no-op if none/old/corrupt
  seedHistory(); // base undo step reflects the restored (or default) settings
  registerServiceWorker();
  try {
    state.wasm = await init();
    state.processor = new Processor();
    document.body.dataset.wasm = "ready";
  } catch (e) {
    console.error(e);
    showError(`Couldn't start the image engine: ${e.message ?? e}`);
  }
}

main();
