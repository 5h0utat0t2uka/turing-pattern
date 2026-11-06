//! Terminal output through chafa, without writing any file.

use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Locate chafa before the simulation runs, so a missing tool fails fast.
pub fn find_chafa() -> Result<PathBuf, String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|dir| dir.join("chafa"))
        .find(|candidate| is_executable(candidate))
        .ok_or_else(|| "chafa not found in PATH".to_string())
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Pass the PNG to chafa on standard input; chafa writes to our standard output.
pub fn show(chafa: &OsStr, png: &[u8]) -> Result<(), String> {
    let mut child = Command::new(chafa)
        .arg("-")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start chafa: {e}"))?;
    let written = child.stdin.take().expect("stdin is piped").write_all(png);
    let status = child.wait().map_err(|e| format!("chafa failed: {e}"))?;
    if !status.success() {
        return Err(match status.code() {
            Some(code) => format!("chafa exited with status {code}"),
            None => "chafa was terminated by a signal".to_string(),
        });
    }
    written.map_err(|e| format!("cannot send image to chafa: {e}"))
}
