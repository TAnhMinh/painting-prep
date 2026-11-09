//! `prep grid`: rasterize a grid overlay onto an RGBA buffer, without file I/O.
//!
//! The geometry comes from [`prep_core::grid_lines`]; the web path draws the
//! same lines with Canvas 2D (D9). This desktop version draws single-colour,
//! aliased lines for eyeballing geometry on real photos — two-tone and
//! anti-aliasing are canvas features and aren't reproduced here.

use std::fmt::Write as _;

use prep_core::{Axis, GridMode, GridSpec, Line, Result, grid_lines};

/// Draws `spec`'s grid over the `width × height` RGBA8 `rgba`, in `colour` with
/// the given pixel `thickness`. Returns the number of lines drawn.
///
/// # Errors
/// Propagates [`prep_core::Error`] from [`grid_lines`] (zero size or count).
pub fn draw(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    spec: &GridSpec,
    colour: [u8; 3],
    thickness: u32,
) -> Result<usize> {
    let lines = grid_lines(width, height, spec)?;
    for line in &lines {
        draw_line(rgba, width, height, line, colour, thickness);
    }
    Ok(lines.len())
}

/// Stamps a `thickness`-wide `colour` segment into `rgba`, sampling pixel
/// centres along the longer axis so axis-aligned and diagonal lines are handled
/// the same way. Endpoints are clamped to the last pixel, so a border line at
/// x = width lands on the right edge; pixels outside the image are skipped.
fn draw_line(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    line: &Line,
    colour: [u8; 3],
    thickness: u32,
) {
    let (w, h) = (width as i64, height as i64);
    let half = i64::from(thickness) / 2;
    let clamp_x = |x: f32| (x.round() as i64).clamp(0, w - 1);
    let clamp_y = |y: f32| (y.round() as i64).clamp(0, h - 1);
    let (x1, y1) = (clamp_x(line.x1), clamp_y(line.y1));
    let (x2, y2) = (clamp_x(line.x2), clamp_y(line.y2));
    let steps = (x2 - x1).abs().max((y2 - y1).abs()).max(1);
    for s in 0..=steps {
        let t = s as f64 / steps as f64;
        let cx = x1 + ((x2 - x1) as f64 * t).round() as i64;
        let cy = y1 + ((y2 - y1) as f64 * t).round() as i64;
        for py in (cy - half)..=(cy + half) {
            if py < 0 || py >= h {
                continue;
            }
            for px in (cx - half)..=(cx + half) {
                if px < 0 || px >= w {
                    continue;
                }
                let idx = ((py * w + px) as usize) * 4;
                rgba[idx] = colour[0];
                rgba[idx + 1] = colour[1];
                rgba[idx + 2] = colour[2];
                rgba[idx + 3] = 255;
            }
        }
    }
}

/// A short description of the grid for stdout, in the `values` legend style:
/// mode, cell size, a partial-cell note (open question 2), diagonals, and the
/// line count and thickness.
pub fn summary(width: u32, height: u32, spec: &GridSpec, thickness: u32, lines: usize) -> String {
    let mut s = String::new();
    // Writing to a String can't fail.
    match spec.mode {
        GridMode::Square { axis, k } => {
            let (ref_len, other_len, ref_name, other_name) = match axis {
                Axis::Width => (width, height, "width", "height"),
                Axis::Height => (height, width, "height", "width"),
            };
            let cell = ref_len as f32 / k as f32;
            let full = (other_len as f32 / cell + 1e-4).floor().max(0.0) as u32;
            let remainder = other_len as f32 - cell * full as f32;
            let _ = writeln!(
                s,
                "mode: square, {k} cells on {ref_name}, cell {cell:.1} px"
            );
            if remainder > 0.5 {
                let _ = writeln!(
                    s,
                    "  {other_name}: {full} whole cells + a partial last cell of {remainder:.1} px"
                );
            } else {
                let _ = writeln!(s, "  {other_name}: {full} whole cells (divides evenly)");
            }
        }
        GridMode::RowsCols { rows, cols } => {
            let _ = writeln!(
                s,
                "mode: rows × cols = {rows} × {cols}, cells {:.1} × {:.1} px",
                width as f32 / cols as f32,
                height as f32 / rows as f32
            );
        }
    }
    let _ = writeln!(
        s,
        "diagonals: {}",
        if spec.diagonals { "yes" } else { "no" }
    );
    let _ = write!(s, "lines: {lines}, thickness {thickness} px");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white(w: usize, h: usize) -> Vec<u8> {
        vec![255; w * h * 4]
    }

    fn px(buf: &[u8], w: usize, x: usize, y: usize) -> [u8; 4] {
        let i = (y * w + x) * 4;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    #[test]
    fn draws_outer_border_and_leaves_interior() {
        let mut buf = white(5, 5);
        let spec = GridSpec {
            mode: GridMode::RowsCols { rows: 1, cols: 1 },
            diagonals: false,
        };
        let n = draw(&mut buf, 5, 5, &spec, [0, 0, 0], 1).unwrap();
        assert_eq!(n, 4); // 2 verticals + 2 horizontals
        assert_eq!(px(&buf, 5, 0, 0), [0, 0, 0, 255]); // top-left corner
        assert_eq!(px(&buf, 5, 4, 4), [0, 0, 0, 255]); // bottom-right (x = 5 clamped to 4)
        assert_eq!(px(&buf, 5, 2, 0), [0, 0, 0, 255]); // top edge
        assert_eq!(px(&buf, 5, 0, 2), [0, 0, 0, 255]); // left edge
        assert_eq!(px(&buf, 5, 2, 2), [255, 255, 255, 255]); // interior untouched
    }

    #[test]
    fn thickness_widens_lines() {
        let mut buf = white(7, 7);
        let spec = GridSpec {
            mode: GridMode::RowsCols { rows: 1, cols: 1 },
            diagonals: false,
        };
        draw(&mut buf, 7, 7, &spec, [0, 0, 0], 3).unwrap();
        // Left border, centre col 0, half = 1 → cols 0 and 1 painted; col 2 not.
        assert_eq!(px(&buf, 7, 0, 3), [0, 0, 0, 255]);
        assert_eq!(px(&buf, 7, 1, 3), [0, 0, 0, 255]);
        assert_eq!(px(&buf, 7, 2, 3), [255, 255, 255, 255]);
    }

    #[test]
    fn colour_is_applied() {
        let mut buf = white(3, 3);
        let spec = GridSpec {
            mode: GridMode::RowsCols { rows: 1, cols: 1 },
            diagonals: false,
        };
        draw(&mut buf, 3, 3, &spec, [128, 128, 128], 1).unwrap();
        assert_eq!(px(&buf, 3, 0, 0), [128, 128, 128, 255]);
    }

    #[test]
    fn summary_flags_partial_cell() {
        let spec = GridSpec {
            mode: GridMode::Square {
                axis: Axis::Width,
                k: 2,
            },
            diagonals: false,
        };
        let text = summary(100, 120, &spec, 1, 7);
        assert!(text.contains("partial last cell"), "{text}");
    }

    #[test]
    fn summary_notes_even_division() {
        let spec = GridSpec {
            mode: GridMode::Square {
                axis: Axis::Width,
                k: 2,
            },
            diagonals: false,
        };
        let text = summary(100, 100, &spec, 1, 6);
        assert!(text.contains("divides evenly"), "{text}");
    }
}
