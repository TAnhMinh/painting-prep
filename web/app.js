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
  preview: $("preview"),
  nValue: $("n-value"),
  blurValue: $("blur-value"),
  status: $("status"),
  error: $("error"),
  errorText: $("error-text"),
  errorClose: $("error-close"),
  updateBanner: $("update-banner"),
  updateReload: $("update-reload"),
  title: $("title"),
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
  renderQueued: false,
  paintQueued: false,
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
    els.exportBtn.disabled = false;
    state.loaded = true;
    render();
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

    const canvas = document.createElement("canvas");
    canvas.width = target.width;
    canvas.height = target.height;
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
  // (spec §5 caching).
  bindStepper("n", 2, 10, () => state.n, (v) => (state.n = v), queueRender);
  for (const radio of document.querySelectorAll('input[name="greys"]')) {
    radio.addEventListener("change", () => {
      state.bucketMean = radio.value === "mean" && radio.checked;
      render();
    });
  }
  // Algorithm + blur change the study, so they re-posterize (not just paint).
  for (const radio of document.querySelectorAll('input[name="algo"]')) {
    radio.addEventListener("change", () => {
      if (!radio.checked) return;
      state.algo = radio.value;
      render();
    });
  }
  bindStepper("blur", 0, 10, () => state.blur, (v) => (state.blur = v), queueRender);

  // Grid controls (M3). All are grid-only changes, so they repaint without
  // re-posterizing — except nothing here touches n/greys.
  for (const radio of document.querySelectorAll('input[name="grid-mode"]')) {
    radio.addEventListener("change", () => {
      if (!radio.checked) return;
      state.grid.mode = radio.value;
      updateGridVisibility();
      paint();
    });
  }
  bindRadio("grid-axis", (value) => {
    state.grid.axis = value;
  });
  bindRadio("grid-diag", (value) => {
    state.grid.diagonals = value === "on";
  });
  bindRadio("grid-over", (value) => {
    state.grid.over = value;
  });
  bindRadio("grid-line", (value) => {
    state.grid.line = value;
  });
  bindStepper("k", 2, 16, () => state.grid.k, (v) => (state.grid.k = v), queuePaint);
  bindStepper("rows", 1, 12, () => state.grid.rows, (v) => (state.grid.rows = v), queuePaint);
  bindStepper("cols", 1, 12, () => state.grid.cols, (v) => (state.grid.cols = v), queuePaint);

  // Export is two taps (spec F4): Export renders + encodes, then Share saves.
  els.exportBtn.addEventListener("click", exportImage);
  els.shareBtn.addEventListener("click", shareExport);

  els.errorClose.addEventListener("click", clearError);
}

/** Wires a radio group: `apply` stores the picked value, then repaint. */
function bindRadio(name, apply) {
  for (const radio of document.querySelectorAll(`input[name="${name}"]`)) {
    radio.addEventListener("change", () => {
      if (!radio.checked) return;
      apply(radio.value);
      paint();
    });
  }
}

/**
 * Wires a − / + number stepper (`<id>-dec`, `<id>-value`, `<id>-inc`). Each tap
 * clamps the value ±1 to `[min, max]`, stores it via `apply`, updates the output,
 * disables a button at its limit, then runs `after` (render or paint). Replaces
 * the sliders, which were too small to hit on the phone (D25).
 */
function bindStepper(id, min, max, get, apply, after) {
  const dec = $(`${id}-dec`);
  const inc = $(`${id}-inc`);
  const out = $(`${id}-value`);
  const sync = () => {
    const v = get();
    out.textContent = v;
    dec.disabled = v <= min;
    inc.disabled = v >= max;
  };
  const step = (delta) => () => {
    const v = Math.min(max, Math.max(min, get() + delta));
    if (v === get()) return;
    apply(v);
    sync();
    after();
  };
  dec.addEventListener("click", step(-1));
  inc.addEventListener("click", step(1));
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
