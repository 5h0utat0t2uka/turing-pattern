//! Externally visible CLI behavior, using a stand-in for chafa.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_turing-pattern");

/// A fresh directory under Cargo's per-target temporary directory.
fn scratch(name: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}-{id}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// `cat` from the test runner's PATH; the program under test gets a restricted PATH.
fn cat() -> PathBuf {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join("cat"))
        .find(|candidate| candidate.is_file())
        .expect("cat in PATH")
}

fn write_script(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A directory with a fake `chafa` that saves its standard input to `received.png`.
fn fake_chafa() -> (PathBuf, PathBuf) {
    let bin = scratch("bin");
    let received = bin.join("received.png");
    let body = format!(
        "[ \"$1\" = - ] || exit 9\n'{}' > '{}' || exit 8\necho shown\n",
        cat().display(),
        received.display()
    );
    write_script(&bin.join("chafa"), &body);
    (bin, received)
}

fn run(args: &[&str], path: &Path, cwd: &Path) -> Output {
    Command::new(BIN)
        .args(args)
        .env("PATH", path)
        .current_dir(cwd)
        .output()
        .unwrap()
}

fn png_size(path: &Path) -> (u32, u32, png::ColorType) {
    let reader = png::Decoder::new(std::io::BufReader::new(fs::File::open(path).unwrap()))
        .read_info()
        .unwrap();
    let info = reader.info();
    (info.width, info.height, info.color_type)
}

#[test]
fn square_color_image_is_sent_to_chafa_without_writing_files() {
    let (bin, received) = fake_chafa();
    for size in ["1", "2", "17", "64", "200"] {
        let cwd = scratch("cwd");
        let out = run(&["--size", size, "--steps", "2", "--seed", "0"], &bin, &cwd);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"shown\n");
        let n: u32 = size.parse().unwrap();
        assert_eq!(png_size(&received), (n, n, png::ColorType::Rgb));
        assert_eq!(
            fs::read_dir(&cwd).unwrap().count(),
            0,
            "no files in the working directory"
        );
    }
}

#[test]
fn seed_and_pattern_are_reported_and_reproducible() {
    let (bin, received) = fake_chafa();
    let cwd = scratch("cwd");
    let args = ["--size", "32", "--steps", "50", "--seed", "42"];
    let first = run(&args, &bin, &cwd);
    assert_eq!(png_size(&received), (32, 32, png::ColorType::Rgb));
    let first_png = fs::read(&received).unwrap();
    let second = run(&args, &bin, &cwd);
    assert_eq!(first.stderr, second.stderr);
    assert_eq!(first_png, fs::read(&received).unwrap());
    let log = String::from_utf8(first.stderr).unwrap();
    assert!(log.contains("seed=42"), "{log}");
    assert!(log.contains("pattern="), "{log}");
    assert!(log.contains("Colors: #"), "{log}");

    let chosen = run(
        &["--size", "8", "--steps", "1", "--pattern", "worms"],
        &bin,
        &cwd,
    );
    assert!(
        String::from_utf8(chosen.stderr)
            .unwrap()
            .contains("pattern=worms")
    );
}

#[test]
fn missing_chafa_is_reported_before_simulating() {
    let empty = scratch("empty");
    let out = run(&["--size", "4", "--steps", "1"], &empty, &empty);
    assert_eq!(out.status.code(), Some(1));
    let log = String::from_utf8(out.stderr).unwrap();
    assert!(log.contains("chafa not found"), "{log}");
    assert!(!log.contains("Generating"), "{log}");
}

#[test]
fn chafa_failure_is_reported() {
    let bin = scratch("failing");
    write_script(
        &bin.join("chafa"),
        &format!("'{}' > /dev/null\nexit 3\n", cat().display()),
    );
    let out = run(&["--size", "4", "--steps", "1"], &bin, &bin);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8(out.stderr).unwrap().contains("status 3"));
}

#[test]
fn text_gives_the_same_image_as_its_seed() {
    let (bin, received) = fake_chafa();
    let common = ["--size", "24", "--steps", "30"];
    let image = |extra: &[&str]| {
        let args: Vec<&str> = common.iter().chain(extra).copied().collect();
        let out = run(&args, &bin, &bin);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        (
            String::from_utf8(out.stderr).unwrap(),
            fs::read(&received).unwrap(),
        )
    };

    let (log, from_text) = image(&["--text", "alice@example.com"]);
    // The seed is reported, so `--seed` reproduces the same image.
    assert!(log.contains("seed=15188035183127902847"), "{log}");
    assert_eq!(from_text, image(&["--seed", "15188035183127902847"]).1);

    // Canonically equivalent text ("が" composed vs decomposed) gives the same image.
    assert_eq!(
        image(&["--text", "\u{304C}"]).1,
        image(&["--text", "\u{304B}\u{3099}"]).1
    );
    // Case matters.
    assert_ne!(from_text, image(&["--text", "Alice@example.com"]).1);
}

#[test]
fn invalid_arguments_are_rejected() {
    let (bin, _) = fake_chafa();
    let cases: &[&[&str]] = &[
        &["--size", "0"],
        &["--size", "-1"],
        &["--size", "2.5"],
        &["--steps", "0"],
        &["--seed", "-1"],
        &["--seed", "18446744073709551616"],
        &["--width", "32"],
        &["--siz", "32"],
        &["--output", "pattern.png"],
        &["--pattern", "unknown"],
        &["--text", "a", "--seed", "1"],
        &["--text"],
    ];
    for args in cases {
        let out = run(args, &bin, &bin);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
}
