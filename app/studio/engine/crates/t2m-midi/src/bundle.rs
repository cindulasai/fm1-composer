//! Putting an export on disk: every file whole, or none of them.

use std::fs;
use std::path::{Path, PathBuf};

use crate::export::{ExportBundle, MidiError};

fn io(message: String) -> MidiError {
    MidiError::Io { message }
}

/// Writes the bundle's files into `dir` (created if missing) and returns their paths.
///
/// Each file is written under a temporary name first and renamed into place once all of
/// them were written, so a failure leaves no partial file and removes the temporary ones.
pub fn write_bundle(dir: &Path, bundle: &ExportBundle) -> Result<Vec<PathBuf>, MidiError> {
    fs::create_dir_all(dir).map_err(|e| io(format!("cannot create {}: {e}", dir.display())))?;
    let temp = |name: &str| dir.join(format!(".{name}.tmp{}", std::process::id()));
    let mut written: Vec<PathBuf> = Vec::new();
    let clean = |paths: &[PathBuf]| {
        for stale in paths {
            let _ = fs::remove_file(stale);
        }
    };
    for (name, bytes) in &bundle.files {
        let path = temp(name);
        let result = fs::write(&path, bytes);
        written.push(path);
        if let Err(e) = result {
            clean(&written);
            return Err(io(format!(
                "cannot write {}: {e}",
                dir.join(name).display()
            )));
        }
    }
    // A target that is a folder would fail the rename after others had been placed: refuse first.
    if let Some((name, _)) = bundle
        .files
        .iter()
        .find(|(name, _)| dir.join(name).is_dir())
    {
        clean(&written);
        return Err(io(format!(
            "cannot write {}: it is a folder",
            dir.join(name).display()
        )));
    }
    let mut finals: Vec<PathBuf> = Vec::new();
    for ((name, _), from) in bundle.files.iter().zip(&written) {
        let to = dir.join(name);
        if let Err(e) = fs::rename(from, &to) {
            clean(&written);
            clean(&finals);
            return Err(io(format!("cannot write {}: {e}", to.display())));
        }
        finals.push(to);
    }
    Ok(finals)
}

#[cfg(test)]
#[path = "bundle_tests.rs"]
mod tests;
