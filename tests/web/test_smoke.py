"""WebKit smoke test (spec §8): the page's value study must match the CLI's,
byte for byte, on a lossless sRGB fixture.

Setup (once):
    python3 -m venv .venv
    .venv/bin/pip install -r tests/web/requirements.txt
    .venv/bin/playwright install --with-deps webkit
Run (after scripts/build-web.sh):
    .venv/bin/python tests/web/test_smoke.py
"""

import base64
import functools
import http.server
import pathlib
import subprocess
import sys
import tempfile
import threading
import unittest

from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parents[2]
WEB = ROOT / "web"
FIXTURE = pathlib.Path(__file__).with_name("fixture.png")

CASES = [(n, greys) for n in (2, 5, 10) for greys in ("even", "mean")]

# Decodes the expected PNG in the page and counts bytes that differ from the
# preview canvas. Both are opaque, so canvas readback is exact.
COMPARE_JS = """async (b64) => {
  const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
  const bitmap = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
  const preview = document.getElementById("preview");
  if (bitmap.width !== preview.width || bitmap.height !== preview.height) {
    return `size ${preview.width} × ${preview.height}, expected ${bitmap.width} × ${bitmap.height}`;
  }
  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  const ctx = canvas.getContext("2d", { colorSpace: "srgb" });
  ctx.drawImage(bitmap, 0, 0);
  const want = ctx.getImageData(0, 0, canvas.width, canvas.height, { colorSpace: "srgb" }).data;
  const got = preview.getContext("2d", { colorSpace: "srgb" })
    .getImageData(0, 0, canvas.width, canvas.height, { colorSpace: "srgb" }).data;
  let diff = 0;
  for (let i = 0; i < want.length; i++) if (want[i] !== got[i]) diff++;
  return diff;
}"""


# Compares the encoded export PNG (fetched from the Save link's blob URL) with
# the CLI study. The fixture is below the export cap, so export size == preview
# size and the exported study must byte-match the CLI exactly.
EXPORT_COMPARE_JS = """async (b64) => {
  const want = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
  const wantBmp = await createImageBitmap(new Blob([want], { type: "image/png" }));
  const href = document.getElementById("export-download").href;
  const gotBmp = await createImageBitmap(await (await fetch(href)).blob());
  if (gotBmp.width !== wantBmp.width || gotBmp.height !== wantBmp.height) {
    return `size ${gotBmp.width} × ${gotBmp.height}, expected ${wantBmp.width} × ${wantBmp.height}`;
  }
  const toData = (bmp) => {
    const c = document.createElement("canvas");
    c.width = bmp.width;
    c.height = bmp.height;
    const x = c.getContext("2d", { colorSpace: "srgb" });
    x.drawImage(bmp, 0, 0);
    return x.getImageData(0, 0, c.width, c.height, { colorSpace: "srgb" }).data;
  };
  const a = toData(wantBmp);
  const b = toData(gotBmp);
  let diff = 0;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) diff++;
  return diff;
}"""


class _QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def cli_study(n, greys, out_dir):
    """The CLI's study of the fixture, as PNG bytes."""
    out = pathlib.Path(out_dir) / f"expected-{n}-{greys}.png"
    subprocess.run(
        ["cargo", "run", "--release", "--quiet", "-p", "prep-cli", "--",
         "values", "-n", str(n), "--greys", greys, str(FIXTURE), str(out)],
        cwd=ROOT, check=True, stdout=subprocess.DEVNULL,
    )
    return out.read_bytes()


class WebKitSmokeTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not (WEB / "pkg" / "prep_wasm.js").exists():
            raise unittest.SkipTest("web/pkg is missing: run scripts/build-web.sh first")
        cls.tmp = tempfile.TemporaryDirectory()
        cls.expected = {case: cli_study(*case, cls.tmp.name) for case in CASES}
        handler = functools.partial(_QuietHandler, directory=WEB)
        cls.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()
        cls.pw = sync_playwright().start()
        cls.browser = cls.pw.webkit.launch()

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.pw.stop()
        cls.server.shutdown()
        cls.tmp.cleanup()

    def setUp(self):
        self.page = self.browser.new_page()
        self.page_errors = []
        self.page.on("pageerror", lambda e: self.page_errors.append(str(e)))
        self.page.goto(f"http://127.0.0.1:{self.server.server_port}/")
        self.page.wait_for_selector("body[data-wasm=ready]")

    def tearDown(self):
        self.page.close()

    def load_fixture(self):
        self.page.set_input_files("#file", str(FIXTURE))
        self.page.wait_for_function(
            "document.getElementById('preview').dataset.rendered && !document.body.dataset.busy"
        )

    def open_settings(self):
        """Opens the Settings overlay if it's closed (the controls live in it,
        D25). The open button is in the top bar, which the open overlay covers,
        so only click it while the overlay is hidden."""
        if self.page.get_attribute("#settings", "hidden") is not None:
            self.page.click("#settings-open")
            self.page.wait_for_selector("#settings:not([hidden])")

    def close_settings(self):
        """Closes the Settings overlay if it's open, so the top-bar F6 controls
        (undo/redo/reset/pin) are reachable."""
        if self.page.get_attribute("#settings", "hidden") is None:
            self.page.click("#settings-close")
            self.page.wait_for_selector("#settings", state="hidden")

    def set_n(self, n):
        """Drives the n stepper (− / +) to `n` by tapping, as a user would."""
        for _ in range(12):
            current = int(self.page.text_content("#n-value"))
            if current == n:
                return
            self.page.click("#n-inc" if current < n else "#n-dec")
        raise AssertionError(f"could not set n to {n}")

    def set_study(self, n, greys):
        self.open_settings()
        self.set_n(n)
        self.page.click(f"label[for=greys-{greys}]")
        # data-rendered is "n,greys,algo,blur"; set_study leaves algo/blur default.
        self.page.wait_for_function(
            f"document.getElementById('preview').dataset.rendered === '{n},{greys},equal,0'"
        )

    def preview_signature(self):
        """A weighted sum over every preview byte: changes if any pixel does."""
        return self.page.evaluate("""() => {
          const p = document.getElementById('preview');
          const d = p.getContext('2d', { colorSpace: 'srgb' })
            .getImageData(0, 0, p.width, p.height, { colorSpace: 'srgb' }).data;
          let s = 0;
          for (let i = 0; i < d.length; i++) s = (s + d[i] * ((i % 251) + 1)) % 2147483647;
          return s;
        }""")

    def test_study_matches_cli(self):
        self.load_fixture()
        for n, greys in CASES:
            with self.subTest(n=n, greys=greys):
                self.set_study(n, greys)
                b64 = base64.b64encode(self.expected[(n, greys)]).decode()
                self.assertEqual(self.page.evaluate(COMPARE_JS, b64), 0)
        self.assertEqual(self.page.evaluate("prepDebug.errorCount"), 0)
        self.assertEqual(self.page_errors, [])

    def test_grid_overlay_draws_and_clears(self):
        self.load_fixture()
        self.set_study(5, "even")  # leaves the Settings overlay open
        base = self.preview_signature()

        # Enabling a square grid must change the preview.
        self.page.click("label[for=grid-square]")
        self.assertNotEqual(self.preview_signature(), base)

        # Exercise diagonals, a plain line style, over-original, and rows × cols;
        # none of these should raise.
        for label in ("diag-on", "line-black", "over-orig", "grid-rc"):
            self.page.click(f"label[for={label}]")

        # Turning the grid off returns the preview to the plain study.
        self.page.click("label[for=grid-off]")
        self.assertEqual(self.preview_signature(), base)

        self.assertEqual(self.page.evaluate("prepDebug.errorCount"), 0)
        self.assertEqual(self.page_errors, [])

    def test_algorithm_and_blur_run(self):
        self.load_fixture()
        self.set_study(5, "even")  # 5,even,equal,0; leaves the overlay open
        base = self.preview_signature()

        # The algorithm control is wired: switching it re-posterizes (the hook
        # updates), and on this varied fixture the preview changes.
        self.page.click("label[for=algo-jenks]")
        self.page.wait_for_function(
            "document.getElementById('preview').dataset.rendered === '5,even,jenks,0'"
        )
        self.assertNotEqual(self.preview_signature(), base)

        # Blur (the squint) visibly changes the preview too; one tap = 0.25%.
        blurred_base = self.preview_signature()
        self.page.click("#blur-inc")
        self.page.wait_for_function(
            "document.getElementById('preview').dataset.rendered === '5,even,jenks,0.25'"
        )
        self.assertNotEqual(self.preview_signature(), blurred_base)

        self.assertEqual(self.page.evaluate("prepDebug.errorCount"), 0)
        self.assertEqual(self.page_errors, [])

    def test_export_study_matches_cli(self):
        self.load_fixture()
        self.set_study(5, "even")  # leaves the Settings overlay open
        self.page.click("#export-btn")
        # Step 1 done: the Save link appears and the busy flag clears.
        self.page.wait_for_function(
            "!document.getElementById('export-download').hidden && !document.body.dataset.busy"
        )
        b64 = base64.b64encode(self.expected[(5, "even")]).decode()
        self.assertEqual(self.page.evaluate(EXPORT_COMPARE_JS, b64), 0)
        self.assertEqual(self.page.evaluate("prepDebug.errorCount"), 0)
        self.assertEqual(self.page_errors, [])

    def test_bad_file_shows_error_and_keeps_the_study(self):
        self.load_fixture()
        with tempfile.NamedTemporaryFile(suffix=".jpg") as bad:
            bad.write(b"not a jpeg")
            bad.flush()
            self.page.set_input_files("#file", bad.name)
            self.page.wait_for_selector("#error:not([hidden])")
        self.assertIn("Couldn't open that photo", self.page.text_content("#error-text"))
        self.assertTrue(self.page.is_visible("#preview"))
        self.assertEqual(self.page_errors, [])

    def test_failed_canvas_is_an_error_not_a_white_study(self):
        # Under memory pressure WebKit's canvas silently reads back as
        # transparent black (D23). Simulate that for the off-screen canvas.
        self.page.add_init_script("""
          const original = CanvasRenderingContext2D.prototype.getImageData;
          CanvasRenderingContext2D.prototype.getImageData = function (x, y, w, h, s) {
            return this.canvas.isConnected ? original.call(this, x, y, w, h, s) : new ImageData(w, h);
          };""")
        self.page.reload()
        self.page.wait_for_selector("body[data-wasm=ready]")
        self.page.set_input_files("#file", str(FIXTURE))
        self.page.wait_for_selector("#error:not([hidden])")
        self.assertIn("not enough memory", self.page.text_content("#error-text"))
        self.assertFalse(self.page.is_visible("#preview"))
        self.assertEqual(self.page_errors, [])


    # --- M5 Phase B: legend, compare, remembered settings (F2/F5) ---

    def test_legend_strip_shows_and_tracks_n(self):
        self.load_fixture()
        # The legend appears after the first render, with one cell per bucket.
        self.page.wait_for_selector("#legend:not([hidden])")
        self.assertEqual(self.page.get_attribute("#legend", "data-count"), "5")
        self.assertEqual(
            self.page.eval_on_selector_all("#legend .legend-cell", "els => els.length"), 5
        )
        # Changing n re-renders and the legend follows.
        self.set_study(3, "even")
        self.page.wait_for_function(
            "document.getElementById('legend').dataset.count === '3'"
        )
        self.assertEqual(self.page.evaluate("prepDebug.errorCount"), 0)
        self.assertEqual(self.page_errors, [])

    def test_compare_original_is_display_only(self):
        self.load_fixture()
        self.set_study(5, "even")
        self.close_settings()
        base = self.preview_signature()
        # Side-by-side so the compare layer is shown without a flip gesture.
        self.page.click("#compare-mode [data-mode=side]")
        self.page.click("#compare-source [data-source=original]")
        self.assertFalse(self.page.is_hidden("#compare"))
        self.assertEqual(self.page.get_attribute("#preview-area", "data-compare"), "original")
        # Compare is a separate layer: the live preview pixels are untouched.
        self.assertEqual(self.preview_signature(), base)
        # Turning compare off hides the layer again.
        self.page.click("#compare-source [data-source=off]")
        self.assertTrue(self.page.is_hidden("#compare"))
        self.assertEqual(self.page.evaluate("prepDebug.errorCount"), 0)
        self.assertEqual(self.page_errors, [])

    def test_settings_are_remembered_across_reload(self):
        self.load_fixture()
        self.set_study(7, "mean")
        self.page.click("label[for=algo-quantile]")
        self.page.wait_for_function(
            "document.getElementById('preview').dataset.rendered === '7,mean,quantile,0'"
        )
        # A reload keeps the same storage; settings come back (F5).
        self.page.reload()
        self.page.wait_for_selector("body[data-wasm=ready]")
        self.assertEqual(int(self.page.text_content("#n-value")), 7)
        self.assertTrue(self.page.is_checked("#greys-mean"))
        self.assertTrue(self.page.is_checked("#algo-quantile"))
        self.assertEqual(self.page.evaluate("prepDebug.errorCount"), 0)
        self.assertEqual(self.page_errors, [])

    def test_corrupt_settings_fall_back_to_defaults(self):
        # Garbage in storage must not break startup (F5): defaults load silently.
        self.page.evaluate("localStorage.setItem('painting-prep:settings', '{not json')")
        self.page.reload()
        self.page.wait_for_selector("body[data-wasm=ready]")
        self.assertEqual(int(self.page.text_content("#n-value")), 5)
        self.assertTrue(self.page.is_checked("#greys-even"))
        self.assertEqual(self.page.evaluate("prepDebug.errorCount"), 0)
        self.assertEqual(self.page_errors, [])

    # --- M6: undo/redo, reset, snapshots (F6) ---

    def test_undo_redo_round_trip(self):
        # The milestone "Done when": 3 → 4 → undo gives a pixel-identical study.
        self.load_fixture()
        self.set_study(3, "even")
        sig3 = self.preview_signature()
        self.set_n(4)
        self.page.wait_for_function(
            "document.getElementById('preview').dataset.rendered === '4,even,equal,0'"
        )
        self.close_settings()
        self.page.click("#undo")
        self.page.wait_for_function(
            "document.getElementById('preview').dataset.rendered === '3,even,equal,0'"
        )
        self.assertEqual(self.preview_signature(), sig3)  # pixel-identical (D15)
        self.page.click("#redo")
        self.page.wait_for_function(
            "document.getElementById('preview').dataset.rendered === '4,even,equal,0'"
        )
        self.assertEqual(self.page.evaluate("prepDebug.errorCount"), 0)
        self.assertEqual(self.page_errors, [])

    def test_reset_is_undoable(self):
        self.load_fixture()
        self.set_study(5, "even")
        # Move several settings away from their defaults.
        self.set_n(8)
        self.page.click("label[for=greys-mean]")
        self.page.click("label[for=algo-jenks]")
        self.page.click("label[for=grid-square]")
        self.page.wait_for_function(
            "document.getElementById('preview').dataset.rendered === '8,mean,jenks,0'"
        )
        self.close_settings()
        # Reset returns everything to defaults (grid off, 5/even/equal/0).
        self.page.click("#reset")
        self.page.wait_for_function(
            "document.getElementById('preview').dataset.rendered === '5,even,equal,0'"
        )
        self.assertTrue(self.page.is_checked("#grid-off"))
        # Reset is an ordinary step, so it is undoable.
        self.assertEqual(self.page.get_attribute("#undo", "disabled"), None)
        self.page.click("#undo")
        self.page.wait_for_function(
            "document.getElementById('preview').dataset.rendered === '8,mean,jenks,0'"
        )
        self.assertTrue(self.page.is_checked("#grid-square"))
        self.assertEqual(self.page.evaluate("prepDebug.errorCount"), 0)
        self.assertEqual(self.page_errors, [])

    def pin_to(self, count):
        """Clicks Pin and waits for the strip to reach `count` (pin is async)."""
        self.page.click("#pin")
        self.page.wait_for_function(
            f"document.getElementById('snaps').dataset.count === '{count}'"
        )

    def test_snapshot_pin_compare_and_cap(self):
        self.load_fixture()
        self.set_study(5, "even")
        self.close_settings()
        self.pin_to("1")
        self.assertFalse(self.page.is_hidden("#snaps"))
        self.assertEqual(self.page.evaluate("prepDebug.snapshotCount"), 1)
        # Compare the live view against the snapshot (side-by-side), display-only.
        base = self.preview_signature()
        self.page.click("#compare-mode [data-mode=side]")
        self.page.click("#compare-source [data-source^=snap]")
        self.assertFalse(self.page.is_hidden("#compare"))
        self.assertEqual(self.preview_signature(), base)
        self.page.click("#compare-source [data-source=off]")
        # Fill to the cap of 4.
        self.pin_to("2")
        self.pin_to("3")
        self.pin_to("4")
        # A 5th pin asks to replace one instead of adding a 5th (D16).
        self.page.click("#pin")
        self.page.wait_for_selector("#snap-banner:not([hidden])")
        self.assertEqual(self.page.get_attribute("#snaps", "data-count"), "4")
        # Tapping a thumbnail in replace mode swaps it and clears the prompt.
        self.page.click(".snap[data-snap-index='0']")
        self.page.wait_for_selector("#snap-banner", state="hidden")
        self.assertEqual(self.page.get_attribute("#snaps", "data-count"), "4")
        self.assertEqual(self.page.evaluate("prepDebug.errorCount"), 0)
        self.assertEqual(self.page_errors, [])

    def test_snapshots_cleared_on_new_photo(self):
        self.load_fixture()
        self.close_settings()
        self.pin_to("1")
        # Loading a photo again clears the snapshots (D16).
        self.load_fixture()
        self.page.wait_for_function(
            "document.getElementById('snaps').dataset.count === '0'"
        )
        self.assertTrue(self.page.is_hidden("#snaps"))
        self.assertEqual(self.page.evaluate("prepDebug.snapshotCount"), 0)
        self.assertEqual(self.page_errors, [])


if __name__ == "__main__":
    sys.exit(unittest.main(verbosity=2))
