//! End to end: `prep grid` on a PNG written to a temporary directory.

use std::path::PathBuf;
use std::process::Command;

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("prep-cli-grid-test-{}-{name}", std::process::id()))
}

/// A `w × h` all-white opaque PNG.
fn white_png(path: &PathBuf, w: u32, h: u32) {
    let rgba = vec![255u8; (w * h * 4) as usize];
    image::save_buffer(path, &rgba, w, h, image::ExtendedColorType::Rgba8).unwrap();
}

#[test]
fn grid_square_writes_overlay_and_summary() {
    let input = temp_path("in.png");
    let output = temp_path("out-square.png");
    white_png(&input, 100, 120);

    let run = Command::new(env!("CARGO_BIN_EXE_prep"))
        .args(["grid", "--mode", "square", "--axis", "width", "-k", "2"])
        .args([&input, &output])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    // 100 / 2 = 50 px cells; height 120 leaves a partial last cell (open q 2).
    assert!(stdout.contains("partial last cell"), "{stdout}");

    let out = image::open(&output).unwrap().into_rgb8();
    assert_eq!(out.dimensions(), (100, 120));
    // The top-left corner sits on the outer border, so it's black, not white.
    assert_eq!(out.get_pixel(0, 0).0, [0, 0, 0]);
    // A cell interior stays white.
    assert_eq!(out.get_pixel(25, 25).0, [255, 255, 255]);

    let _ = std::fs::remove_file(input);
    let _ = std::fs::remove_file(output);
}

#[test]
fn grid_rowscols_with_diagonals_runs() {
    let input = temp_path("in2.png");
    let output = temp_path("out-rc.png");
    white_png(&input, 60, 60);

    let run = Command::new(env!("CARGO_BIN_EXE_prep"))
        .args([
            "grid",
            "--mode",
            "rowscols",
            "--rows",
            "3",
            "--cols",
            "3",
            "--diagonals",
            "--colour",
            "grey",
        ])
        .args([&input, &output])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("diagonals: yes"), "{stdout}");

    let out = image::open(&output).unwrap().into_rgb8();
    assert_eq!(out.dimensions(), (60, 60));
    // Grey lines were drawn somewhere.
    assert!(out.pixels().any(|p| p.0 == [128, 128, 128]));

    let _ = std::fs::remove_file(input);
    let _ = std::fs::remove_file(output);
}

#[test]
fn rejects_cell_count_out_of_range() {
    let run = Command::new(env!("CARGO_BIN_EXE_prep"))
        .args(["grid", "-k", "99", "in.png", "out.png"])
        .output()
        .unwrap();
    assert!(!run.status.success());
}
