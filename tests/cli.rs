//! Externally visible CLI behavior, using a stand-in for chafa.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use turing_pattern::id::ImageSpec;
use turing_pattern::sim::Pattern;
use turing_pattern::text::seed_from_text;

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
fn all_patterns_are_tiled_into_one_image() {
    let (bin, received) = fake_chafa();
    let cwd = scratch("cwd");
    let out = run(
        &["--all", "--size", "20", "--steps", "30", "--text", "a"],
        &bin,
        &cwd,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // 7 patterns, 4 per row, gaps of max(2, 20 / 16) = 2 pixels.
    assert_eq!(
        png_size(&received),
        (4 * 20 + 3 * 2, 2 * 20 + 2, png::ColorType::Rgba)
    );
    let log = String::from_utf8(out.stderr).unwrap();
    assert!(
        log.contains("Order (left to right): coral maze spots holes / worms dashes loops"),
        "{log}"
    );
    assert_eq!(fs::read_dir(&cwd).unwrap().count(), 0);

    // Without --size each tile is 256 pixels, with 16-pixel gaps.
    let out = run(&["--all", "--steps", "1"], &bin, &cwd);
    assert!(out.status.success());
    assert_eq!(
        png_size(&received),
        (4 * 256 + 3 * 16, 2 * 256 + 16, png::ColorType::Rgba)
    );
}

fn dir_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

/// File name for the given inputs, computed by the library.
fn saved_name(pattern: &str, seed: u64, size: usize, steps: u32) -> String {
    let id = ImageSpec {
        pattern,
        seed,
        size,
        steps,
    }
    .id();
    format!("turing-{id}.png")
}

fn png_text(path: &Path) -> Vec<(String, String)> {
    let reader = png::Decoder::new(std::io::BufReader::new(fs::File::open(path).unwrap()))
        .read_info()
        .unwrap();
    reader
        .info()
        .uncompressed_latin1_text
        .iter()
        .map(|chunk| (chunk.keyword.clone(), chunk.text.clone()))
        .collect()
}

#[test]
fn save_writes_the_displayed_image_under_its_id() {
    let (bin, received) = fake_chafa();
    let out_dir = scratch("saved");
    let dir = out_dir.to_str().unwrap();
    let args = [
        "--size",
        "24",
        "--steps",
        "30",
        "--seed",
        "7",
        "--pattern",
        "maze",
        "--save",
        dir,
    ];
    let out = run(&args, &bin, &bin);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Pinned: first 12 hex digits of the SHA-256 of the described inputs.
    let name = "turing-7f485ec22c6b.png";
    assert_eq!(name, saved_name("maze", 7, 24, 30));
    assert_eq!(dir_names(&out_dir), [name]);
    let saved = out_dir.join(name);
    assert_eq!(fs::read(&saved).unwrap(), fs::read(&received).unwrap());
    assert_eq!(png_size(&saved), (24, 24, png::ColorType::Rgb));
    assert_eq!(
        png_text(&saved),
        [(
            "turing-pattern".to_string(),
            "turing-pattern v2 pattern=maze seed=7 size=24 steps=30 id=7f485ec22c6b".to_string()
        )]
    );
    let log = String::from_utf8(out.stderr).unwrap();
    assert!(log.contains("id=7f485ec22c6b"), "{log}");
    assert!(
        log.contains(&format!("Saved: {}", saved.display())),
        "{log}"
    );

    // The same image again: no copy, and the existing file is reported.
    let again = run(&args, &bin, &bin);
    assert!(
        String::from_utf8(again.stderr)
            .unwrap()
            .contains("Already saved")
    );
    assert_eq!(dir_names(&out_dir), [name]);

    // Something else under that name is never overwritten.
    fs::write(&saved, b"not this image").unwrap();
    run(&args, &bin, &bin);
    assert_eq!(fs::read(&saved).unwrap(), b"not this image");
    assert_eq!(dir_names(&out_dir), ["turing-7f485ec22c6b-2.png", name]);
}

#[test]
fn same_text_gives_the_same_id_and_other_inputs_do_not() {
    let (bin, _) = fake_chafa();
    let out_dir = scratch("ids");
    let dir = out_dir.to_str().unwrap();
    let save = |args: &[&str]| {
        let mut all = vec!["--steps", "20", "--save", dir];
        all.extend_from_slice(args);
        let out = run(&all, &bin, &bin);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    save(&["--size", "16", "--text", "alice"]);
    save(&["--size", "16", "--text", "alice"]); // same id: no new file
    save(&["--size", "16", "--text", "bob"]);
    save(&["--size", "17", "--text", "alice"]);
    save(&["--size", "16", "--text", "alice", "--pattern", "loops"]);
    save(&["--size", "16", "--text", "alice", "--all"]);
    let alice = seed_from_text("alice");
    let alice_pattern = Pattern::random(alice).name();
    let mut expected = vec![
        saved_name(alice_pattern, alice, 16, 20),
        saved_name(
            Pattern::random(seed_from_text("bob")).name(),
            seed_from_text("bob"),
            16,
            20,
        ),
        saved_name(alice_pattern, alice, 17, 20),
        saved_name("loops", alice, 16, 20),
        saved_name("all", alice, 16, 20),
    ];
    expected.sort();
    expected.dedup(); // if alice's own pattern happens to be loops
    assert_eq!(dir_names(&out_dir), expected);
}

#[test]
fn save_without_directory_uses_the_working_directory() {
    let (bin, _) = fake_chafa();
    let cwd = scratch("cwd");
    let args = [
        "--size",
        "8",
        "--steps",
        "1",
        "--seed",
        "3",
        "--pattern",
        "spots",
        "--save",
    ];
    let out = run(&args, &bin, &cwd);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(dir_names(&cwd), [saved_name("spots", 3, 8, 1)]);
}

#[test]
fn unusable_save_directory_is_reported_before_simulating() {
    let (bin, _) = fake_chafa();
    let missing = scratch("parent").join("missing");
    let out = run(
        &["--size", "8", "--save", missing.to_str().unwrap()],
        &bin,
        &bin,
    );
    assert_eq!(out.status.code(), Some(1));
    let log = String::from_utf8(out.stderr).unwrap();
    assert!(log.contains("cannot use directory"), "{log}");
    assert!(!log.contains("Generating"), "{log}");
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
        &["--all", "--pattern", "worms"],
    ];
    for args in cases {
        let out = run(args, &bin, &bin);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
}
