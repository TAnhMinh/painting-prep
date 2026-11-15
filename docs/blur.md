# Pre-blur ("squint") — what it is and why

A plain-language guide to the blur that runs before a value study (spec F2, §6).
Companion to `docs/algorithms.md` (the bucketing algorithms). The code is in
`crates/core/src/blur.rs`.

## The purpose: squinting before you measure values

Painters squint to throw away fine detail and see the big value masses. The blur
is that squint, applied **before** the study is computed. It handles the two
things the bucketing algorithms can't:

- **Speckle** — foliage, gravel, skin pores, fabric weave: thousands of tiny
  light/dark pixels. Posterizing them directly gives salt-and-pepper, not shapes.
- **Smooth gradients** — a clear sky, a softly lit cheek: a threshold cutting
  through one leaves a hard contour line (banding).

Blur melts both away so the study reads as coherent masses.

## The pieces

### The L\* map
L\* is perceptual lightness, 0 (black) → 100 (white). The **L\* map** is one L\*
number per pixel — a greyscale lightness version of the photo (sRGB → linear →
luminance → L\*). We blur *this*, not the RGB colours: a value study only cares
about lightness, and one channel is a third of the work.

### Blur = local averaging
Replace each pixel with a **weighted average of its neighbours**. Detail smaller
than the averaging area smears together; big shapes survive.

### Gaussian, and sigma (σ)
A **Gaussian blur** makes the weights follow a bell curve: the pixel counts most,
neighbours count less the farther away they are. Smooth, round, no artifacts.

**Sigma (σ)** is the width of that bell, in **pixels** — the single "how hard do I
squint" knob:
- small σ → tight bell → only immediate neighbours blend → subtle.
- large σ → wide bell → detail over many pixels dissolves → heavy squint.
- rule of thumb: features smaller than ~2–3 × σ disappear.

In the app you set a **blur %**, and σ is derived as
`blur% / 100 × the short side of the image`. Expressing it as a percentage (not
raw pixels) is deliberate: the ~2048-px preview and the 12 MP export then get the
*same visual squint* despite different pixel sizes (§6). The **"requested
Gaussian"** is just the Gaussian with that σ — the exact blur the % asks for.

### Box blur
The crudest blur: average over a flat square window — every neighbour within
radius *r* counts **equally** (no bell curve). Very fast (slide the window: add the
entering pixel, subtract the leaving one), but on its own slightly blocky.

### Triple box blur ≈ the requested Gaussian
Do a box blur **three times** and the result converges to a Gaussian (the same
reason averaging many things gives a bell shape). Three passes is visually
indistinguishable from a true Gaussian here, and it stays **O(pixels) no matter
how large σ is** — each pass is a sliding sum. A real Gaussian kernel slows down
as σ grows; the triple box doesn't. So we approximate the requested Gaussian with
three cheap boxes.

### Kovesi's formula
Whole-pixel box widths can't hit an arbitrary σ, so **Kovesi's formula**
(*Fast Almost-Gaussian Filtering*, 2010) picks the three box sizes whose combined
effect best matches the requested σ: the right odd width, and how many of the
three boxes use it versus the next size up. It's the bridge from "σ the user
wants" → "three concrete integer box widths."

### Two implementation words
- **Separable** — a 2-D blur is done as a horizontal 1-D pass then a vertical one
  (same result, far cheaper).
- **Clamp-to-edge** — at the border, pixels "off the edge" are treated as copies of
  the edge pixel, so edges don't darken toward black or wrap around.

## A picture: a lone tree in a meadow

**A tree in a meadow under a bright sky**: a leafy canopy, grassy foreground,
smooth sky.

**No blur, n = 4:**
- The canopy (thousands of sunlit/shadowed leaves) becomes **salt-and-pepper
  confetti** of 2–3 values — no readable shape.
- The grass texture does the same on a smaller scale.
- The smooth sky gets a faint **contour band** where a cut passes through it.

**Blur ~2%** (1536-px-short preview → σ ≈ 31 px; detail under ~60–90 px dissolves):
- The canopy's leaf-speckle averages into **two clean masses** — the lit top and
  the shadowed underside — exactly what you'd paint.
- The grass settles into one or two smooth values.
- The sky flattens, so the band disappears.

**Too far (~8%):** σ gets so wide the tree's silhouette softens and canopy and
grass bleed together — squinted past the useful point. That trade-off is what the
control lets you feel.

## "Smooth borders"? — a refinement

Blur runs **before** bucketing, so it isn't smoothing the border *line* as a
finishing step. It **reshapes the lightness field** so that (a) detail smaller than
your squint vanishes into coherent masses, and (b) the few boundaries the threshold
then draws land along the big forms and come out clean instead of jagged or
speckled. As a side effect it **deepens the valleys** in the histogram, which helps
natural breaks place even better cuts (see `docs/algorithms.md`). So the clean
borders are the result of blur + bucketing together: **blur decides what counts as
detail; the algorithm decides where the masses divide.**
