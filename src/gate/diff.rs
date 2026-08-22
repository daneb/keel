//! What actually changed, according to git rather than according to the agent.
//!
//! G2 checks the diff against the declared blast radius and the declared line
//! budget. Both checks are only worth anything if the diff is observed rather
//! than reported: `SCOPE-CREEP` in the Phase 3 taxonomy is precisely the case
//! where what happened and what was claimed diverge.

use crate::paths::Paths;
use anyhow::{Result, bail};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct FileChange {
    pub path: String,
    pub added: usize,
    pub removed: usize,
    /// Binary files report no line counts.
    pub binary: bool,
}

impl FileChange {
    pub fn churn(&self) -> usize {
        self.added + self.removed
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Diff {
    /// What the diff was taken against.
    pub base: String,
    pub files: Vec<FileChange>,
    pub added: usize,
    pub removed: usize,
}

impl Diff {
    /// Lines of change, which is what a budget is denominated in.
    pub fn churn(&self) -> usize {
        self.added + self.removed
    }

    pub fn stat(&self) -> String {
        let mut s = String::new();
        for f in &self.files {
            if f.binary {
                s.push_str(&format!("{:>6} {:>6}  {}\n", "-", "-", f.path));
            } else {
                s.push_str(&format!("{:>6} {:>6}  {}\n", f.added, f.removed, f.path));
            }
        }
        s.push_str(&format!(
            "\n{} file(s), +{} -{} ({} lines of churn) against {}\n",
            self.files.len(), self.added, self.removed, self.churn(), self.base
        ));
        s
    }
}

/// The working-tree diff against `base` (a commit-ish), including untracked
/// files — an agent that adds a new file has changed the blast radius just as
/// much as one that edits an old one, and `git diff` alone would not see it.
pub fn against(paths: &Paths, base: &str) -> Result<Diff> {
    if !is_git_repo(paths) {
        bail!("not a git repository — keel cannot observe the diff");
    }
    let mut files: Vec<FileChange> = Vec::new();

    let numstat = git(paths, &["diff", "--numstat", base, "--"])?;
    for line in numstat.lines() {
        if let Some(change) = parse_numstat(line) {
            files.push(change);
        }
    }

    for path in untracked(paths)? {
        if files.iter().any(|f| f.path == path) {
            continue;
        }
        let added = std::fs::read_to_string(paths.repo.join(&path))
            .map(|s| s.lines().count())
            .unwrap_or(0);
        let binary = added == 0 && paths.repo.join(&path).metadata().map(|m| m.len() > 0).unwrap_or(false);
        files.push(FileChange { path, added, removed: 0, binary });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    let added = files.iter().map(|f| f.added).sum();
    let removed = files.iter().map(|f| f.removed).sum();
    Ok(Diff { base: base.to_string(), files, added, removed })
}

fn parse_numstat(line: &str) -> Option<FileChange> {
    let mut parts = line.split('\t');
    let a = parts.next()?;
    let r = parts.next()?;
    let path = parts.next()?.trim().to_string();
    if path.is_empty() {
        return None;
    }
    // git writes `-` for binary files.
    let binary = a == "-" || r == "-";
    Some(FileChange {
        path,
        added: a.parse().unwrap_or(0),
        removed: r.parse().unwrap_or(0),
        binary,
    })
}

fn untracked(paths: &Paths) -> Result<Vec<String>> {
    let out = git(paths, &["ls-files", "--others", "--exclude-standard"])?;
    Ok(out.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
}

pub fn is_git_repo(paths: &Paths) -> bool {
    paths.repo.join(".git").exists()
}

/// A base to diff against when the run recorded none.
pub fn default_base(paths: &Paths) -> String {
    if git(paths, &["rev-parse", "HEAD"]).is_ok() {
        "HEAD".to_string()
    } else {
        // An empty repository has no HEAD; git's empty-tree object always
        // exists and makes "everything is new" expressible.
        "4b825dc642cb6eb9a060e54bf8d69288fbee4904".to_string()
    }
}

fn git(paths: &Paths, args: &[&str]) -> Result<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(&paths.repo)
        .output()?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numstat_lines() {
        let c = parse_numstat("12\t3\tsrc/api.rs").unwrap();
        assert_eq!(c.path, "src/api.rs");
        assert_eq!(c.added, 12);
        assert_eq!(c.removed, 3);
        assert!(!c.binary);
        assert_eq!(c.churn(), 15);
    }

    #[test]
    fn binary_files_are_marked_not_counted_as_zero_change() {
        let c = parse_numstat("-\t-\tlogo.png").unwrap();
        assert!(c.binary);
        assert_eq!(c.churn(), 0);
    }

    #[test]
    fn junk_lines_are_ignored() {
        assert!(parse_numstat("").is_none());
        assert!(parse_numstat("not a numstat line").is_none());
    }

    #[test]
    fn churn_counts_both_directions() {
        let d = Diff {
            base: "HEAD".into(),
            files: vec![
                FileChange { path: "a.rs".into(), added: 10, removed: 2, binary: false },
                FileChange { path: "b.rs".into(), added: 0, removed: 5, binary: false },
            ],
            added: 10,
            removed: 7,
        };
        assert_eq!(d.churn(), 17, "a deletion is a change");
        assert!(d.stat().contains("17 lines of churn"));
    }
}
