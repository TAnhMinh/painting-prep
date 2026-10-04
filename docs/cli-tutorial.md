# Value studies with the `prep` CLI

A short guide to building the desktop CLI and making value studies of your own photos. The CLI runs the same `prep-core` code the phone app will use.

## 1. Prerequisites

- **rustup.** The repo's `rust-toolchain.toml` pins the Rust version. The first `cargo` command inside the repo downloads that version automatically; your system Rust is untouched.
- **A terminal in the repo folder** (`painting-prep/`).

If the terminal says `cargo: command not found`, add rustup's tools to `PATH` for this shell:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

## 2. Build

```sh
cargo build --release -p prep-cli
```

This creates the binary `target/release/prep`. Use `--release`: the debug build is many times slower on full-size photos. Rebuild after pulling new code.

## 3. Make a value study

```sh
./target/release/prep values -n 5 ../test_photos/IMG_6640.jpeg ../test_photos/out/IMG_6640-n5.png
```

- The first path is your photo (JPEG or PNG), the second is the output. The output format follows its extension; PNG is best, because it's lossless.
- The output folder must exist: `mkdir -p ../test_photos/out`.
- Keep photos and results outside the repo (for example in `../test_photos/`), so they are never committed.

`cargo run --release -p prep-cli -- values …` also works. It builds first if needed, then runs. Everything after `--` goes to `prep`.

## 4. Options

| Option | Values | Default | Meaning |
|---|---|---|---|
| `-n`, `--buckets` | 2–10 | 5 | Number of shades of grey. 2 or 3 gives a notan. |
| `--greys` | `even`, `mean`, `both` | `even` | How each shade is painted (see below). |

- **`even`:** evenly spaced greys from black to white (L\* 0, 25, 50, 75, 100 for n = 5). The same scale for every photo, so you can pre-mix paints for it.
- **`mean`:** each shade is the average lightness of its pixels. Closer to the photo, but softer.
- **`both`:** one image with three panels side by side: **original | even | mean**.

`./target/release/prep values --help` lists all of this.

## 5. Reading the legend

The CLI prints a legend for each study:

```
evenly spaced greys, n = 5
bucket     L*  sRGB  pixels
     0    0.0     0   18.6%
     1   24.9    59   20.7%
     2   50.0   119   22.2%
     3   75.1   185   26.7%
     4  100.0   255   11.8%
```

- **bucket:** 0 is the darkest.
- **L\*:** the grey's perceptual lightness, 0 (black) to 100 (white).
- **sRGB:** the grey's value (R = G = B), for matching it in an image editor.
- **pixels:** how much of the photo is in that bucket.

The buckets cut the L\* scale into equal intervals (0–20, 20–40, …, for n = 5) for every photo. A dark photo therefore fills only the dark buckets. That's intentional, so studies of different photos stay comparable.

## 6. Recipes

```sh
# Notan: 3 values
./target/release/prep values -n 3 ../test_photos/IMG_6640.jpeg ../test_photos/out/notan.png

# Compare both grey modes with the original
./target/release/prep values -n 5 --greys both ../test_photos/IMG_6640.jpeg ../test_photos/out/compare.png

# All photos in a folder
for f in ../test_photos/*.jpeg; do
  ./target/release/prep values -n 5 "$f" "../test_photos/out/$(basename "${f%.*}")-n5.png"
done

# Open a result
xdg-open ../test_photos/out/notan.png
```

## 7. Notes

- **Speckle** on textured surfaces (pavement, brick) is expected for now. A "squint" blur option is planned (spec M5).
- **Colours:** the CLI doesn't apply embedded colour profiles. A Display P3 iPhone photo looks slightly duller than on the phone (D21). Photos exported as JPEG without a profile are unaffected.
- **Rotation:** EXIF orientation is applied, so portrait photos come out upright.
- **Transparent PNGs** are placed on white first (D19).
