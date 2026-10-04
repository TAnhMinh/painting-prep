// Painting Prep: pick a photo, decode it to the preview size, and show its
// value study (spec F1, F2). All image logic runs in prep-core via wasm.

// debug.js first: it captures console output from the modules after it.
import { bindLongPress, setInfoProvider } from "./debug.js";
import init, { Processor } from "./pkg/prep_wasm.js";

/** Preview working size: long side in px (spec §7). */
const PREVIEW_LONG_SIDE = 2048;

const $ = (id) => document.getElementById(id);
const els = {
  file: $("file"),
  chooseBig: $("choose-big"),
  preview: $("preview"),
  n: $("n"),
  nValue: $("n-value"),
  nTicks: $("n-ticks"),
  status: $("status"),
  error: $("error"),
  errorText: $("error-text"),
  errorClose: $("error-close"),
  title: $("title"),
};

const state = {
  wasm: null,
  processor: null,
  loaded: false,
  n: Number(els.n.value),
  bucketMean: $("greys-mean").checked,
  renderQueued: false,
};

/** Facts about the current photo and the last run, for the debug panel. */
const stats = {
  source: null, // { width, height }
  working: null, // { width, height }
  decodePath: null,
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

/**
 * Decodes `file` straight to the working size and returns its sRGB pixels
 * (spec F1). Never draws or decodes the full-resolution photo on purpose.
 */
async function decodeToWorkingSize(file) {
  const url = URL.createObjectURL(file);
  const img = new Image();
  try {
    // The load event gives the oriented natural size (EXIF applied) from the
    // header. Not img.decode(): that decodes the full-resolution pixels, about
    // 260 MB for a 64 MP photo, and iOS then runs out of memory (D23).
    await timed("header", () => loadImage(img, url));
    const source = { width: img.naturalWidth, height: img.naturalHeight };
    const target = workingSize(source.width, source.height, PREVIEW_LONG_SIDE);
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
    const { imageData, source, target, decodePath } = await decodeToWorkingSize(file);
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

/** Renders the study for the current settings into the preview canvas. */
function render() {
  if (!state.loaded) return;
  try {
    const p = state.processor;
    timedSync("render", () => p.render(state.n, state.bucketMean));
    timedSync("putImageData", () => {
      // A view on wasm memory, made after render's allocations (spec §7).
      const view = new Uint8ClampedArray(state.wasm.memory.buffer, p.output_ptr(), p.output_len());
      const ctx = els.preview.getContext("2d", { colorSpace: "srgb" });
      ctx.putImageData(new ImageData(view, p.width(), p.height()), 0, 0);
    });
    // Signals the finished render to the WebKit smoke test.
    els.preview.dataset.rendered = `${state.n},${state.bucketMean ? "mean" : "even"}`;
  } catch (e) {
    console.error(e);
    showError(`Couldn't render the study: ${e.message ?? e}`);
  }
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

function bindEvents() {
  els.file.addEventListener("change", () => {
    const file = els.file.files?.[0];
    if (file) loadPhoto(file);
  });
  els.n.addEventListener("input", () => {
    state.n = Number(els.n.value);
    els.nValue.textContent = state.n;
    queueRender();
  });
  for (const radio of document.querySelectorAll('input[name="greys"]')) {
    radio.addEventListener("change", () => {
      state.bucketMean = radio.value === "mean" && radio.checked;
      render();
    });
  }
  els.errorClose.addEventListener("click", clearError);
}

/** Rows for the debug panel (spec §7). */
function debugInfo() {
  const size = (s) => (s ? `${s.width} × ${s.height}` : "–");
  const rows = {
    source: size(stats.source),
    working: size(stats.working),
    decodePath: stats.decodePath ?? "–",
    settings: `n = ${state.n}, greys = ${state.bucketMean ? "mean" : "even"}`,
    // Wasm memory only grows (spec §7), so this is the session's peak.
    wasmMemory: state.wasm ? `${(state.wasm.memory.buffer.byteLength / 2 ** 20).toFixed(1)} MB` : "–",
  };
  for (const [step, ms] of Object.entries(stats.timings)) {
    rows[`${step} (ms)`] = ms.toFixed(1);
  }
  return rows;
}

/** One tick per slider value, so the ticks always match min and max. */
function drawTicks(slider, container) {
  const count = Number(slider.max) - Number(slider.min) + 1;
  container.replaceChildren(...Array.from({ length: count }, () => document.createElement("span")));
}

async function main() {
  els.nValue.textContent = state.n; // the browser may restore form values
  drawTicks(els.n, els.nTicks);
  setInfoProvider(debugInfo);
  bindLongPress(els.title);
  bindEvents();
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
