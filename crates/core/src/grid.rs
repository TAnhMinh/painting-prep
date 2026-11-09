//! Grid line geometry in image-relative units (spec F3, D9).
//!
//! `prep-core` returns line geometry only. The caller draws it: Canvas 2D on
//! the web, a small rasterizer in the CLI. Lines are never written into a pixel
//! buffer here (D9). Coordinates are in image pixels, so the caller scales them
//! to the preview or export size it is drawing at.

use crate::{Error, Result};

/// Positions within [`EPS_PX`] of each other count as the same line, so
/// floating-point division doesn't spawn a spurious partial cell.
const EPS_PX: f32 = 1e-3;

/// A line segment in image-relative pixel coordinates.
///
/// Every endpoint lies within `0.0..=w` horizontally and `0.0..=h` vertically.
/// Coordinates are `f32` so non-integer cell boundaries (e.g. `width / k`) land
/// exactly and Canvas 2D can draw them at any scale (D9).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

/// Which axis sets the square-cell size in [`GridMode::Square`] (spec F3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Cell size is `width / k`.
    Width,
    /// Cell size is `height / k`.
    Height,
}

/// How the grid divides the image (spec F3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridMode {
    /// Mode A: square cells of side `axis_len / k`. The reference `axis` gets
    /// exactly `k` cells; the other axis gets as many whole cells as fit, plus a
    /// smaller partial last cell closed by the far border line, drawn and
    /// flagged in the UI (open question 2 → D24).
    Square { axis: Axis, k: u32 },
    /// Mode B: `rows × cols` cells that divide the image exactly, generally not
    /// square.
    RowsCols { rows: u32, cols: u32 },
}

/// A grid request: how to divide the image, and whether to cross each whole
/// cell with its two diagonals (spec F3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSpec {
    pub mode: GridMode,
    /// Draw both diagonals of every whole cell when true. Partial cells get
    /// none.
    pub diagonals: bool,
}

/// Grid lines for a `w × h` image, in image-relative pixel coordinates.
///
/// The outer border lines are included along with the inner lines (spec F3).
/// In [`GridMode::Square`], a partial last cell on the non-reference axis is
/// bounded by the far border line (open question 2 → D24). Diagonals, when
/// requested, cross every whole cell only.
///
/// # Errors
/// - [`Error::ZeroSize`] if `w` or `h` is 0.
/// - [`Error::GridCount`] if `k`, `rows`, or `cols` is 0.
pub fn grid_lines(w: u32, h: u32, spec: &GridSpec) -> Result<Vec<Line>> {
    if w == 0 || h == 0 {
        return Err(Error::ZeroSize {
            width: w,
            height: h,
        });
    }
    let (wf, hf) = (w as f32, h as f32);
    match spec.mode {
        GridMode::RowsCols { rows, cols } => {
            if rows == 0 || cols == 0 {
                return Err(Error::GridCount {
                    count: rows.min(cols),
                });
            }
            let xs: Vec<f32> = (0..=cols).map(|i| wf * i as f32 / cols as f32).collect();
            let ys: Vec<f32> = (0..=rows).map(|j| hf * j as f32 / rows as f32).collect();
            Ok(assemble(wf, hf, &xs, &ys, spec.diagonals, None))
        }
        GridMode::Square { axis, k } => {
            if k == 0 {
                return Err(Error::GridCount { count: k });
            }
            let (ref_len, other_len) = match axis {
                Axis::Width => (wf, hf),
                Axis::Height => (hf, wf),
            };
            let cell = ref_len / k as f32;
            // The reference axis gets exactly k cells. The far border is set to
            // ref_len rather than k × cell so it lands exactly even when the
            // division isn't exact (e.g. 100 / 3).
            let refs: Vec<f32> = (0..=k)
                .map(|i| if i == k { ref_len } else { cell * i as f32 })
                .collect();
            // The other axis gets the whole cells that fit; a remainder is
            // closed by the far border, forming the flagged partial cell. The
            // small epsilon keeps a near-exact division from adding a sliver.
            let full = (other_len / cell + 1e-4).floor() as u32;
            let mut others: Vec<f32> = (0..=full).map(|j| cell * j as f32).collect();
            if other_len - cell * full as f32 > EPS_PX {
                others.push(other_len);
            }
            let (xs, ys) = match axis {
                Axis::Width => (refs, others),
                Axis::Height => (others, refs),
            };
            Ok(assemble(wf, hf, &xs, &ys, spec.diagonals, Some(cell)))
        }
    }
}

/// Builds the border + inner lines at the given `xs` (vertical lines) and `ys`
/// (horizontal lines), then the per-cell diagonals when `diagonals` is set.
///
/// `min_cell` filters which cells get diagonals: `Some(cell)` draws them only in
/// cells at least `cell` wide and tall (so partial cells are skipped); `None`
/// treats every cell as whole.
fn assemble(
    w: f32,
    h: f32,
    xs: &[f32],
    ys: &[f32],
    diagonals: bool,
    min_cell: Option<f32>,
) -> Vec<Line> {
    let mut lines = Vec::new();
    for &x in xs {
        lines.push(Line {
            x1: x,
            y1: 0.0,
            x2: x,
            y2: h,
        });
    }
    for &y in ys {
        lines.push(Line {
            x1: 0.0,
            y1: y,
            x2: w,
            y2: y,
        });
    }
    if diagonals {
        for xw in xs.windows(2) {
            for yw in ys.windows(2) {
                let whole = match min_cell {
                    Some(c) => xw[1] - xw[0] >= c - EPS_PX && yw[1] - yw[0] >= c - EPS_PX,
                    None => true,
                };
                if whole {
                    lines.push(Line {
                        x1: xw[0],
                        y1: yw[0],
                        x2: xw[1],
                        y2: yw[1],
                    });
                    lines.push(Line {
                        x1: xw[0],
                        y1: yw[1],
                        x2: xw[1],
                        y2: yw[0],
                    });
                }
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn vertical(l: &Line) -> bool {
        l.x1 == l.x2
    }
    fn horizontal(l: &Line) -> bool {
        l.y1 == l.y2
    }
    fn diagonal(l: &Line) -> bool {
        l.x1 != l.x2 && l.y1 != l.y2
    }

    // Known values.

    #[test]
    fn rows_cols_borders_and_inner_lines() {
        let spec = GridSpec {
            mode: GridMode::RowsCols { rows: 2, cols: 2 },
            diagonals: false,
        };
        let lines = grid_lines(100, 100, &spec).unwrap();
        assert_eq!(lines.len(), 6); // 3 verticals + 3 horizontals
        for x in [0.0, 50.0, 100.0] {
            assert!(lines.contains(&Line {
                x1: x,
                y1: 0.0,
                x2: x,
                y2: 100.0
            }));
        }
        for y in [0.0, 50.0, 100.0] {
            assert!(lines.contains(&Line {
                x1: 0.0,
                y1: y,
                x2: 100.0,
                y2: y
            }));
        }
    }

    #[test]
    fn square_flags_partial_last_cell() {
        // Width axis, k = 2 → cell 50; height 120 → whole cells at 0,50,100 and
        // the partial last cell closed by the 120 border.
        let spec = GridSpec {
            mode: GridMode::Square {
                axis: Axis::Width,
                k: 2,
            },
            diagonals: false,
        };
        let lines = grid_lines(100, 120, &spec).unwrap();
        let verts: Vec<_> = lines.iter().filter(|l| vertical(l)).collect();
        let horiz: Vec<_> = lines.iter().filter(|l| horizontal(l)).collect();
        assert_eq!(verts.len(), 3); // x = 0, 50, 100
        assert_eq!(horiz.len(), 4); // y = 0, 50, 100, and the 120 border
        assert!(horiz.iter().any(|l| l.y1 == 100.0));
        assert!(horiz.iter().any(|l| l.y1 == 120.0));
    }

    #[test]
    fn square_has_no_partial_when_axis_divides_evenly() {
        let spec = GridSpec {
            mode: GridMode::Square {
                axis: Axis::Width,
                k: 2,
            },
            diagonals: false,
        };
        let lines = grid_lines(100, 100, &spec).unwrap();
        assert_eq!(lines.iter().filter(|l| horizontal(l)).count(), 3);
    }

    #[test]
    fn square_height_axis_sets_cell_size() {
        // Height axis, k = 2 → cell 50; width 120 → verticals 0,50,100,120.
        let spec = GridSpec {
            mode: GridMode::Square {
                axis: Axis::Height,
                k: 2,
            },
            diagonals: false,
        };
        let lines = grid_lines(120, 100, &spec).unwrap();
        assert_eq!(lines.iter().filter(|l| horizontal(l)).count(), 3); // y = 0,50,100
        assert_eq!(lines.iter().filter(|l| vertical(l)).count(), 4); // x = 0,50,100,120
    }

    #[test]
    fn diagonals_add_two_per_whole_cell() {
        let spec = GridSpec {
            mode: GridMode::RowsCols { rows: 2, cols: 2 },
            diagonals: true,
        };
        let lines = grid_lines(100, 100, &spec).unwrap();
        // 6 grid lines + 2 diagonals × 4 cells.
        assert_eq!(lines.len(), 14);
        assert_eq!(lines.iter().filter(|l| diagonal(l)).count(), 8);
    }

    #[test]
    fn partial_cells_get_no_diagonals() {
        // Width k = 2 (cell 50), height 120: two whole rows (y 0–50, 50–100) of
        // two cells get diagonals; the partial bottom row (100–120) gets none.
        // Without the partial exclusion this would be 3 rows × 2 cells × 2 = 12.
        let spec = GridSpec {
            mode: GridMode::Square {
                axis: Axis::Width,
                k: 2,
            },
            diagonals: true,
        };
        let lines = grid_lines(100, 120, &spec).unwrap();
        assert_eq!(lines.iter().filter(|l| diagonal(l)).count(), 8); // 4 whole cells × 2
    }

    #[test]
    fn zero_size_is_an_error() {
        let spec = GridSpec {
            mode: GridMode::RowsCols { rows: 2, cols: 2 },
            diagonals: false,
        };
        assert_eq!(
            grid_lines(0, 10, &spec),
            Err(Error::ZeroSize {
                width: 0,
                height: 10
            })
        );
    }

    #[test]
    fn zero_count_is_an_error() {
        let rows_cols = GridSpec {
            mode: GridMode::RowsCols { rows: 0, cols: 3 },
            diagonals: false,
        };
        assert_eq!(
            grid_lines(10, 10, &rows_cols),
            Err(Error::GridCount { count: 0 })
        );
        let square = GridSpec {
            mode: GridMode::Square {
                axis: Axis::Width,
                k: 0,
            },
            diagonals: false,
        };
        assert_eq!(
            grid_lines(10, 10, &square),
            Err(Error::GridCount { count: 0 })
        );
    }

    // Properties (spec §8: lines stay within bounds, and the count matches).

    proptest! {
        #[test]
        fn rows_cols_lines_stay_in_bounds(
            w in 1u32..=5000, h in 1u32..=5000,
            rows in 1u32..=12, cols in 1u32..=12, diagonals in any::<bool>(),
        ) {
            let spec = GridSpec { mode: GridMode::RowsCols { rows, cols }, diagonals };
            let lines = grid_lines(w, h, &spec).unwrap();
            let (wf, hf) = (w as f32, h as f32);
            for l in &lines {
                prop_assert!(l.x1 >= 0.0 && l.x1 <= wf);
                prop_assert!(l.x2 >= 0.0 && l.x2 <= wf);
                prop_assert!(l.y1 >= 0.0 && l.y1 <= hf);
                prop_assert!(l.y2 >= 0.0 && l.y2 <= hf);
            }
        }

        #[test]
        fn rows_cols_line_count_matches(
            w in 1u32..=5000, h in 1u32..=5000,
            rows in 1u32..=12, cols in 1u32..=12, diagonals in any::<bool>(),
        ) {
            let spec = GridSpec { mode: GridMode::RowsCols { rows, cols }, diagonals };
            let lines = grid_lines(w, h, &spec).unwrap();
            let expected = (cols + 1) + (rows + 1) + if diagonals { 2 * rows * cols } else { 0 };
            prop_assert_eq!(lines.len() as u32, expected);
        }

        #[test]
        fn square_lines_stay_in_bounds(
            w in 1u32..=5000, h in 1u32..=5000, k in 1u32..=16,
            width_axis in any::<bool>(), diagonals in any::<bool>(),
        ) {
            let axis = if width_axis { Axis::Width } else { Axis::Height };
            let spec = GridSpec { mode: GridMode::Square { axis, k }, diagonals };
            let lines = grid_lines(w, h, &spec).unwrap();
            let (wf, hf) = (w as f32, h as f32);
            for l in &lines {
                prop_assert!(l.x1 >= 0.0 && l.x1 <= wf + EPS_PX);
                prop_assert!(l.x2 >= 0.0 && l.x2 <= wf + EPS_PX);
                prop_assert!(l.y1 >= 0.0 && l.y1 <= hf + EPS_PX);
                prop_assert!(l.y2 >= 0.0 && l.y2 <= hf + EPS_PX);
            }
        }
    }
}
