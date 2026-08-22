//! `keel hook install|uninstall` — wire `keel store check` into pre-commit.
//!
//! P1 in miniature: the store discipline is worth nothing as an intention and
//! quite a lot as a thing that refuses to let you commit.

use crate::paths::Paths;
use anyhow::{Result, bail};
use std::path::PathBuf;

const MARKER: &str = "# >>> keel pre-commit >>>";
const END: &str = "# <<< keel pre-commit <<<";

/// The hook records the absolute path of the binary that installed it and
/// falls back to `PATH`. If neither resolves, it **fails** rather than skipping:
/// a check that silently passes when it could not run is the "gate theatre"
/// failure mode in PLAN.md §6, and it is worse than no hook at all because it
/// looks like protection.
fn body(keel_bin: &str) -> String {
    format!(
        "{MARKER}\n\
         keel_bin=\"{keel_bin}\"\n\
         if [ ! -x \"$keel_bin\" ]; then\n\
         \x20 keel_bin=\"$(command -v keel 2>/dev/null)\"\n\
         fi\n\
         if [ -z \"$keel_bin\" ] || [ ! -x \"$keel_bin\" ]; then\n\
         \x20 echo \"keel: cannot find the keel binary, so the store check did not run.\" >&2\n\
         \x20 echo \"      Put keel on PATH, re-run \\`keel hook install\\`, or commit with --no-verify.\" >&2\n\
         \x20 exit 1\n\
         fi\n\
         if ! \"$keel_bin\" store check; then\n\
         \x20 echo >&2\n\
         \x20 echo \"keel: projections are not current. Fix them, or commit with --no-verify.\" >&2\n\
         \x20 exit 1\n\
         fi\n\
         {END}\n"
    )
}

/// Absolute path of the running binary, for the hook to call back into.
fn current_binary() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "keel".to_string())
}

fn hook_path(paths: &Paths) -> Result<PathBuf> {
    let git = paths.repo.join(".git");
    if !git.exists() {
        bail!("no .git directory at {} — nothing to hook", paths.repo.display());
    }
    // Worktrees and submodules keep .git as a file pointing at the real dir.
    let dir = if git.is_file() {
        let content = std::fs::read_to_string(&git)?;
        let rest = content.trim().strip_prefix("gitdir:").unwrap_or("").trim();
        let p = PathBuf::from(rest);
        if p.is_absolute() { p } else { paths.repo.join(p) }
    } else {
        git
    };
    Ok(dir.join("hooks").join("pre-commit"))
}

pub fn install() -> Result<()> {
    let paths = Paths::require_init()?;
    let path = hook_path(&paths)?;
    std::fs::create_dir_all(path.parent().unwrap())?;

    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.contains(MARKER) {
        println!("  already installed: {}", path.display());
        return Ok(());
    }
    let bin = current_binary();
    let content = if existing.trim().is_empty() {
        format!("#!/bin/sh\n\n{}", body(&bin))
    } else {
        // Append rather than replace: other hooks live here too.
        let mut s = existing;
        if !s.ends_with('\n') { s.push('\n'); }
        s.push('\n');
        s.push_str(&body(&bin));
        s
    };
    std::fs::write(&path, content)?;
    make_executable(&path)?;
    println!("  installed {}", path.display());
    println!("  calls {bin}");
    println!("  a drifted, stale or over-budget projection now blocks the commit");
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let paths = Paths::require_init()?;
    let path = hook_path(&paths)?;
    let Ok(existing) = std::fs::read_to_string(&path) else {
        println!("  no pre-commit hook installed");
        return Ok(());
    };
    let Some(start) = existing.find(MARKER) else {
        println!("  keel block not found in {}", path.display());
        return Ok(());
    };
    let end = existing[start..].find(END).map(|i| start + i + END.len()).unwrap_or(existing.len());
    let mut out = String::new();
    out.push_str(&existing[..start]);
    out.push_str(existing[end..].trim_start_matches('\n'));
    let cleaned = out.trim_end().to_string();
    if cleaned.trim() == "#!/bin/sh" {
        std::fs::remove_file(&path)?;
        println!("  removed {}", path.display());
    } else {
        std::fs::write(&path, format!("{cleaned}\n"))?;
        println!("  removed keel block from {}", path.display());
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o111);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) -> Result<()> { Ok(()) }
