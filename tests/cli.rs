use assert_cmd::Command;
use std::time::Instant;

/// Flags that must be present in --help output (kept in sync with README.md).
const REQUIRED_FLAGS: &[&str] = &[
    "--output",
    "--output-dir",
    "--site",
    "--match",
    "--prune",
    "--query",
    "--citations",
    "--refs",
    "--raw",
    "--css",
    "--full",
    "--deep",
    "--pages",
    "--no-links",
    "--no-images",
    "--timeout",
    "--concurrency",
    "--max-depth",
    "--browser",
    "--ignore-robots",
    "--no-metadata",
    "--verbose",
];

#[test]
fn help_lists_every_required_flag() {
    let out = Command::cargo_bin("wcl")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    for flag in REQUIRED_FLAGS {
        assert!(help.contains(flag), "missing flag in --help: {flag}");
    }
}

#[test]
fn help_is_fast() {
    // Locate the built binary. If release binary exists, use that; otherwise use
    // the binary built for the current test run via `cargo_bin`.
    let release_bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/wcl");
    let bin = if release_bin.exists() {
        release_bin
    } else {
        assert_cmd::cargo::cargo_bin("wcl")
    };
    // Warm the OS page cache once; the first cold exec on macOS pays one-time
    // dyld/page-compile cost that is not representative of steady-state startup.
    std::process::Command::new(&bin)
        .arg("--help")
        .output()
        .unwrap();
    let start = Instant::now();
    std::process::Command::new(&bin)
        .arg("--help")
        .output()
        .unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_millis() < 150,
        "cold start too slow: {elapsed:?}"
    );
}

#[test]
fn concurrency_is_clamped_to_ceiling() {
    let out = Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "https://example.com",
            "--concurrency",
            "9999",
            "--print-options",
        ])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("concurrency: 256"), "got: {s}");
}
