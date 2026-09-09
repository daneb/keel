//! Writes a concurrent reader cannot observe half-finished.
//!
//! `std::fs::write` truncates before it writes, so any process reading the file
//! in that window sees an empty or partial one. keel has real readers in that
//! window: `keel run` rewrites `run.json` when it finishes, while the
//! pre-commit hook, `keel export`, `keel next` or a second terminal may be
//! reading the same file — and a JSON record that parses only sometimes is
//! exactly the kind of quiet unreliability the evidence trail cannot afford.
//!
//! Writing a sibling temporary and renaming over the target is atomic within a
//! filesystem, so a reader sees either the previous file or the new one.

use anyhow::{Context, Result};
use std::path::Path;

/// Replace `path` with `contents`, atomically.
///
/// The temporary is created in the target's own directory so the rename cannot
/// cross a filesystem boundary, and carries the process id so two keel
/// processes writing the same path cannot clobber each other's temporary.
pub fn write(path: &Path, contents: &str) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));

    if let Err(e) = std::fs::write(&tmp, contents) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("writing {}", tmp.display()));
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("replacing {}", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reader_never_sees_a_partial_file() {
        let dir = std::env::temp_dir().join(format!("keel-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("run.json");

        let small = "{}".to_string();
        let large = format!("{{\"pad\":\"{}\"}}", "x".repeat(400_000));
        write(&path, &large).unwrap();

        let reader = {
            let path = path.clone();
            std::thread::spawn(move || {
                for _ in 0..400 {
                    match std::fs::read_to_string(&path) {
                        // Every observation must be one of the two whole files.
                        Ok(s) => assert!(
                            s.ends_with("}") && (s.len() == 2 || s.len() > 400_000),
                            "observed a partial file of {} bytes",
                            s.len()
                        ),
                        Err(e) => assert_eq!(
                            e.kind(),
                            std::io::ErrorKind::NotFound,
                            "unexpected read error"
                        ),
                    }
                }
            })
        };

        for _ in 0..200 {
            write(&path, &small).unwrap();
            write(&path, &large).unwrap();
        }
        reader.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_leaves_no_temporary_behind() {
        let dir = std::env::temp_dir().join(format!("keel-atomic-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // The parent does not exist, so the temporary cannot be created.
        assert!(write(&dir.join("nope.json"), "{}").is_err());
        assert!(!dir.exists(), "a failed write created the directory");
    }
}
