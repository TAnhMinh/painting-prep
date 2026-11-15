# Value-study algorithms — thresholds, quantiles, natural breaks

A plain-language guide to the three bucketing algorithms (spec §6, D3/D10–D12).
It explains what each one does to a photo and when to reach for it. The code is
in `crates/core/src/thresholds.rs`.

## The shared idea: a histogram and some cut points

Every algorithm works on the **L\* histogram** — a tally of how many pixels sit at
each lightness, from 0 (black) to 100 (white). To make an *n*-value study you need
**n − 1 cut points** on that 0–100 line. Those cuts *are* the "thresholds": every
pixel between two neighbouring cuts collapses to one flat grey. The algorithms
disagree only on **where to put the cuts**. (A pixel exactly on a cut goes to the
lighter side — D12.)

## One picture to run through them

An **overcast coastal photo**: a dark rocky headland, a band of mid-grey sea, and
a big pale sky that fills most of the frame. Its histogram has three humps with
low valleys between them, and it leans heavily to the light end:

```
 pixels
   |                                        ███
   |                                        ███ ← sky  (~60% of pixels, L* ~80)
   |                        ██              ███
   |     █                  ██              ███
   |     █ ← rocks          ██ ← sea        ███
   +--|----|----|----|----|----|----|----|----|----|--- L*
   0  10   20   30   40   50   60   70   80   90  100
        ▲gap at ~30         ▲gap at ~65
```

### Equal intervals — chop the *scale* into n equal slices
Ignores the histogram. For n = 5 the cuts are fixed at **20 / 40 / 60 / 80**, the
same for every photo.
- Rocks (L\*15) → darkest bucket; sea (L\*50) → the middle bucket; sky (L\*~80)
  **straddles the cut at 80**, so the smooth sky is sliced into two greys with a
  contour running through it. Several buckets sit nearly empty.
- **Upside:** it's *absolute* — a dark photo looks dark, a light photo looks
  light, and two different photos' studies are directly comparable (the default,
  D10). **Downside:** cuts can fall inside a big smooth region, and tones that
  don't fill the 0–100 range waste buckets.

### Quantiles — give every bucket the same *number of pixels*
Cuts fall so each bucket holds 1/n of the pixels. For n = 5, each bucket = 20% of
the frame.
- Because 60% of pixels are sky, the sky alone is subdivided into ~3 buckets (lovely
  cloud gradation) while rocks and sea get crushed into the darkest bucket or two.
- **Upside:** maximum separation wherever the pixels actually are. **Downside:** it
  will carve a smooth area just to hit its pixel quota — inventing mid-greys and
  banding across flat regions — and it crushes whatever is sparse.

### Natural breaks (Jenks) — put the cuts in the *valleys*
Finds the division into n contiguous groups that keeps each group as tonally tight
as possible, which drops the cuts into the **low-density gaps** between clusters.
- For n = 3 it's a perfect fit: cuts land at **~30 and ~65** (the valleys). Rocks →
  one flat dark, sea → one flat mid, sky → one flat light, with borders exactly at
  the edges between regions.

## Varying n (the painterly part)

| | **n = 2 (notan)** | **n = 3** | **n = 5** |
|---|---|---|---|
| **Equal** | Cut at 50 → rocks dark, sea+sky light. Honest, absolute. | Cuts 33/67 → clean here *by luck* (clusters fall in separate thirds). | Cuts 20/40/60/80 → sky split by the 80 cut (banding); empty buckets. |
| **Quantile** | Forces 50/50, so the cut lands *inside the sky* (~L\*76): the bottom of the sky joins the darks — a notan that ignores the real shapes. | 33/33/33 — sky still over-subdivided, darks merged. | Rich gradation in the sky, darks crushed, mild banding on flat areas. |
| **Jenks** | Picks the single most natural divide (the big ~65 valley): rocks+sea vs sky. Cleanest notan. | **Ideal:** rocks / sea / sky, cuts in the valleys. | Keeps the 3 valley-cuts, then spends the 2 **extra** buckets sub-dividing the largest humps at their minor internal dips. |

## When does Jenks give the clean edges you want?

Its strength — **clean borders, no stray patches of a wrong value** — shows **when
the picture has distinct tonal regions separated by gaps**. The cut sits in a
valley where almost no pixels live, so almost none are near enough to flip across
it. Two caveats:

1. **No gap → no rescue.** A *smooth continuous gradient with no valley* (a clear
   sky, a studio backdrop, a softly lit cheek) has no low-density point to hide the
   cut in, so you still get a contour band. The fix is the **blur / "squint"**
   (M5.2), which melts micro-variation so the whole area lands in one bucket. Blur
   also kills **speckle** from noise or texture (foliage, gravel): those stray
   pixels straddle a cut, and no algorithm removes them, but a squint does. Bonus:
   blurring deepens the histogram valleys, which makes Jenks's job easier.
   **Division of labour: Jenks decides where the few big boundaries sit; blur
   controls speckle and gradients.**
2. **Don't ask for more values than you see.** With 3 real regions and n = 5, Jenks
   is *forced* to use the two spare buckets, carving cuts *inside* otherwise-clean
   regions — reintroducing the banding you were avoiding. Match n to the number of
   values you actually perceive.

## Rule of thumb

- **Equal intervals** — an honest, absolute read you can compare across photos.
- **Quantiles** — a very high- or low-key photo where you want to pull detail out
  of the crowded end of the range.
- **Natural breaks** — a scene with clear value regions, when you want each to read
  as one clean shape; pair it with a touch of blur for anything smooth or noisy.
