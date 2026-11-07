//! Saving images under names derived from what determines their content.
//!
//! The name carries the image id, a hash of every input that affects the
//! pixels, so different images never share a name, and generating the same
//! image again finds the existing file instead of piling up copies.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

/// `turing-<id>.png`, where the id is a hash of every input (see `id.rs`).
pub fn file_name(id: &str) -> String {
    format!("turing-{id}.png")
}

pub enum Saved {
    /// A new file was written.
    Created(PathBuf),
    /// A file with identical content already existed.
    Existing(PathBuf),
}

/// Check up front that the directory can be used, before any computation.
pub fn check_directory(dir: &Path) -> Result<(), String> {
    match fs::metadata(dir) {
        Ok(m) if m.is_dir() => Ok(()),
        Ok(_) => Err(format!("not a directory: {}", dir.display())),
        Err(e) => Err(format!("cannot use directory {}: {e}", dir.display())),
    }
}

/// Write `data` as `dir/name` without ever overwriting a file.
///
/// If that name holds identical bytes, nothing is written. If it holds
/// something else (for example an image made by an older version), the next
/// free name `…-2.png`, `…-3.png`, … is used instead.
pub fn save(dir: &Path, name: &str, data: &[u8]) -> Result<Saved, String> {
    let stem = name.strip_suffix(".png").unwrap_or(name);
    // Write the complete file under a temporary name in the same directory,
    // then link it into place: linking is atomic and fails if the name exists,
    // so a partially written image never appears under the final name.
    let temp = dir.join(format!(".{stem}.{}.tmp", std::process::id()));
    let result = write_temp(&temp, data).and_then(|()| link_unique(&temp, dir, stem, data));
    let _ = fs::remove_file(&temp);
    result
}

fn write_temp(temp: &Path, data: &[u8]) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)
        .map_err(|e| format!("cannot write {}: {e}", temp.display()))?;
    file.write_all(data)
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("cannot write {}: {e}", temp.display()))
}

fn link_unique(temp: &Path, dir: &Path, stem: &str, data: &[u8]) -> Result<Saved, String> {
    for n in 1u32.. {
        let path = match n {
            1 => dir.join(format!("{stem}.png")),
            _ => dir.join(format!("{stem}-{n}.png")),
        };
        match fs::hard_link(temp, &path) {
            Ok(()) => return Ok(Saved::Created(path)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                if fs::read(&path).is_ok_and(|existing| existing == data) {
                    return Ok(Saved::Existing(path));
                }
            }
            Err(e) => return Err(format!("cannot save {}: {e}", path.display())),
        }
    }
    unreachable!("ran out of file names")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("turing-save-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn name_is_the_id() {
        assert_eq!(file_name("7f485ec22c6b"), "turing-7f485ec22c6b.png");
    }

    #[test]
    fn same_content_is_not_duplicated() {
        let dir = scratch("same");
        assert!(matches!(
            save(&dir, "a.png", b"x").unwrap(),
            Saved::Created(_)
        ));
        assert!(matches!(
            save(&dir, "a.png", b"x").unwrap(),
            Saved::Existing(_)
        ));
        assert_eq!(names(&dir), ["a.png"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn different_content_never_overwrites() {
        let dir = scratch("different");
        fs::write(dir.join("a.png"), b"old").unwrap();
        fs::write(dir.join("a-2.png"), b"older").unwrap();
        let Saved::Created(path) = save(&dir, "a.png", b"new").unwrap() else {
            panic!("expected a new file");
        };
        assert_eq!(path, dir.join("a-3.png"));
        assert_eq!(fs::read(dir.join("a.png")).unwrap(), b"old");
        assert_eq!(fs::read(dir.join("a-2.png")).unwrap(), b"older");
        assert_eq!(fs::read(&path).unwrap(), b"new");
        // No temporary file is left behind.
        assert_eq!(names(&dir), ["a-2.png", "a-3.png", "a.png"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unusable_directory_is_reported() {
        let dir = scratch("file");
        let file = dir.join("not-a-dir");
        fs::write(&file, b"").unwrap();
        assert!(check_directory(&file).is_err());
        assert!(check_directory(&dir.join("missing")).is_err());
        assert!(check_directory(&dir).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }
}
