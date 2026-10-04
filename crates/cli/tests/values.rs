//! End to end: `prep values` on a PNG written to a temporary directory.

use std::path::PathBuf;
use std::process::Command;

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("prep-cli-test-{}-{name}", std::process::id()))
}

#[test]
fn values_both_writes_original_and_two_studies() {
    let input = temp_path("in.png");
    let output = temp_path("out.png");
    // A transparent pixel (composited over white, D19) and an opaque black one.
    let rgba = [0, 0, 0, 0, 0, 0, 0, 255];
    image::save_buffer(&input, &rgba, 2, 1, image::ExtendedColorType::Rgba8).unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_prep"))
        .args(["values", "-n", "2", "--greys", "both"])
        .args([&input, &output])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(stdout.contains("evenly spaced greys, n = 2"), "{stdout}");
    assert!(stdout.contains("bucket mean greys, n = 2"), "{stdout}");
    assert!(stdout.contains("50.0%"), "{stdout}");

    let out = image::open(&output).unwrap().into_rgb8();
    assert_eq!(out.dimensions(), (6, 1));
    let reds: Vec<u8> = out.pixels().map(|p| p.0[0]).collect();
    // original | evenly spaced | bucket mean
    assert_eq!(reds, [255, 0, 255, 0, 255, 0]);

    let _ = std::fs::remove_file(input);
    let _ = std::fs::remove_file(output);
}

#[test]
fn rejects_bucket_count_out_of_range() {
    let run = Command::new(env!("CARGO_BIN_EXE_prep"))
        .args(["values", "-n", "11", "in.png", "out.png"])
        .output()
        .unwrap();
    assert!(!run.status.success());
}

#[test]
fn missing_input_is_a_readable_error() {
    let missing = temp_path("missing.png");
    let run = Command::new(env!("CARGO_BIN_EXE_prep"))
        .args(["values"])
        .args([&missing, &temp_path("unused.png")])
        .output()
        .unwrap();
    assert!(!run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.starts_with("prep: can't read"), "{stderr}");
}
