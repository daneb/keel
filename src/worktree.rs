//! Git worktrees, for running tasks in isolation.
//!
//! `keel tasks` computes which tasks could proceed together. Actually running
//! them together needs each agent to have its own tree — two drivers editing
//! one checkout produce a diff nobody wrote and nobody can review.
//!
//! A worktree per task gives each driver a real, complete checkout at the same
//! commit. Their patches are collected and applied to the main tree afterwards,
//! one at a time, so a conflict is reported as a conflict rather than resolved
//! by whichever process wrote last.

use crate::paths::Paths;
use anyhow::{Context, Result, bail};
use std::path::PathBuf;

pub struct Worktree {
    pub task: String,
    pub paths: Paths,
    repo: PathBuf,
    removed: bool,
}

impl Worktree {
    /// Create a detached worktree at `base` for one task.
    pub fn create(paths: &Paths, task: &str, base: &str) -> Result<Self> {
        let dir = worktrees_root(paths).join(sanitise(task));
        if dir.exists() {
            // A leftover from a crashed run. Removing it is safe: worktrees are
            // derived state, and keeping a stale one would silently run the
            // driver against last time's code.
            let _ = remove_worktree(&paths.repo, &dir);
            let _ = std::fs::remove_dir_all(&dir);
        }
        std::fs::create_dir_all(worktrees_root(paths))?;

        git(
            &paths.repo,
            &["worktree", "add", "--detach", &dir.to_string_lossy(), base],
        )
        .with_context(|| format!("creating a worktree for {task} at {base}"))?;

        Ok(Self {
            task: task.to_string(),
            paths: Paths { repo: dir.clone() },
            repo: paths.repo.clone(),
            removed: false,
        })
    }

    /// Everything the driver changed here, as a patch against the base commit.
    ///
    /// Untracked files are staged first: a new file is a change, and
    /// `git diff` alone would not see it.
    pub fn patch(&self) -> Result<String> {
        git(&self.paths.repo, &["add", "-A"])
            .with_context(|| format!("staging {}'s changes", self.task))?;
        git(&self.paths.repo, &["diff", "--cached", "--binary"])
            .with_context(|| format!("reading {}'s patch", self.task))
    }

    /// Paths the driver touched, for reporting and overlap detection.
    pub fn changed_files(&self) -> Result<Vec<String>> {
        git(&self.paths.repo, &["add", "-A"])
            .with_context(|| format!("staging {}'s changes", self.task))?;
        let out = git(&self.paths.repo, &["diff", "--cached", "--name-only"])?;
        Ok(out.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
    }

    pub fn remove(&mut self) -> Result<()> {
        if self.removed {
            return Ok(());
        }
        remove_worktree(&self.repo, &self.paths.repo)?;
        self.removed = true;
        Ok(())
    }
}

impl Drop for Worktree {
    fn drop(&mut self) {
        // A leaked worktree makes `git worktree list` lie and the next run fail
        // for a confusing reason.
        let _ = self.remove();
    }
}

fn remove_worktree(repo: &std::path::Path, dir: &std::path::Path) -> Result<()> {
    let _ = git(repo, &["worktree", "remove", "--force", &dir.to_string_lossy()]);
    if dir.exists() {
        let _ = std::fs::remove_dir_all(dir);
    }
    let _ = git(repo, &["worktree", "prune"]);
    Ok(())
}

pub fn worktrees_root(paths: &Paths) -> PathBuf {
    paths.keel().join("worktrees")
}

/// Apply a patch to the main tree.
///
/// `--3way` so an overlapping-but-compatible change merges rather than being
/// rejected outright; a genuine conflict still fails, which is the point.
pub fn apply(paths: &Paths, patch: &str) -> Result<()> {
    if patch.trim().is_empty() {
        return Ok(());
    }
    let tmp = paths.keel().join("patch.tmp");
    std::fs::write(&tmp, patch)?;
    let result = git(&paths.repo, &["apply", "--3way", "--whitespace=nowarn", &tmp.to_string_lossy()]);
    let _ = std::fs::remove_file(&tmp);
    result.map(|_| ()).context("applying a task's patch to the working tree")
}

/// The commit a wave's worktrees branch from.
///
/// A worktree must branch from something, so `--waves` needs one commit to
/// exist. That is a precondition rather than a limitation — but the raw git
/// error for it is three lines about ambiguous arguments, which tells an
/// operator nothing about what to do.
pub fn base_commit(paths: &Paths) -> Result<String> {
    match git(&paths.repo, &["rev-parse", "HEAD"]) {
        Ok(head) => Ok(head.trim().to_string()),
        Err(_) => bail!(
            "`--waves` gives each task its own git worktree, and a worktree must branch \
             from a commit — this repository has none yet.\n\
             Make one commit, or run without `--waves` to execute tasks serially."
        ),
    }
}

/// Whether the tree has changes that a wave would fold its own patches into.
///
/// Not fatal — but the operator should know that what comes back is their work
/// plus several agents', interleaved.
pub fn is_dirty(paths: &Paths) -> bool {
    git(&paths.repo, &["status", "--porcelain"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

fn sanitise(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect()
}

fn git(dir: &std::path::Path, args: &[&str]) -> Result<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn repo() -> Paths {
        static C: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "keel-wt-{}-{}",
            std::process::id(),
            C.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join(".keel")).unwrap();
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        std::fs::write(dir.join("b.txt"), "one\n").unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "t@example.com"],
            vec!["config", "user.name", "Test"],
            vec!["add", "-A"],
            vec!["commit", "-q", "-m", "base"],
        ] {
            git(&dir, &args).unwrap();
        }
        Paths { repo: dir }
    }

    #[test]
    fn a_worktree_is_a_real_checkout_at_the_base_commit() {
        let p = repo();
        let base = base_commit(&p).unwrap();
        let wt = Worktree::create(&p, "T-1", &base).unwrap();
        assert!(wt.paths.repo.join("a.txt").is_file(), "the worktree has no checkout");
        assert_eq!(std::fs::read_to_string(wt.paths.repo.join("a.txt")).unwrap(), "one\n");
        let _ = std::fs::remove_dir_all(&p.repo);
    }

    #[test]
    fn a_patch_carries_edits_and_new_files() {
        let p = repo();
        let base = base_commit(&p).unwrap();
        let wt = Worktree::create(&p, "T-1", &base).unwrap();
        std::fs::write(wt.paths.repo.join("a.txt"), "two\n").unwrap();
        std::fs::write(wt.paths.repo.join("new.txt"), "fresh\n").unwrap();

        let files = wt.changed_files().unwrap();
        assert!(files.contains(&"a.txt".to_string()), "{files:?}");
        assert!(files.contains(&"new.txt".to_string()), "an untracked file was not seen: {files:?}");

        let patch = wt.patch().unwrap();
        assert!(patch.contains("a.txt"), "{patch}");
        assert!(patch.contains("new.txt"), "{patch}");
        let _ = std::fs::remove_dir_all(&p.repo);
    }

    #[test]
    fn patches_from_two_worktrees_both_land() {
        let p = repo();
        let base = base_commit(&p).unwrap();

        let wt1 = Worktree::create(&p, "T-1", &base).unwrap();
        std::fs::write(wt1.paths.repo.join("a.txt"), "from one\n").unwrap();
        let patch1 = wt1.patch().unwrap();

        let wt2 = Worktree::create(&p, "T-2", &base).unwrap();
        std::fs::write(wt2.paths.repo.join("b.txt"), "from two\n").unwrap();
        let patch2 = wt2.patch().unwrap();

        apply(&p, &patch1).unwrap();
        apply(&p, &patch2).unwrap();

        assert_eq!(std::fs::read_to_string(p.repo.join("a.txt")).unwrap(), "from one\n");
        assert_eq!(std::fs::read_to_string(p.repo.join("b.txt")).unwrap(), "from two\n");
        let _ = std::fs::remove_dir_all(&p.repo);
    }

    #[test]
    fn a_genuine_conflict_is_an_error_not_a_silent_last_writer_wins() {
        let p = repo();
        let base = base_commit(&p).unwrap();

        let wt1 = Worktree::create(&p, "T-1", &base).unwrap();
        std::fs::write(wt1.paths.repo.join("a.txt"), "one side\n").unwrap();
        let patch1 = wt1.patch().unwrap();

        let wt2 = Worktree::create(&p, "T-2", &base).unwrap();
        std::fs::write(wt2.paths.repo.join("a.txt"), "other side\n").unwrap();
        let patch2 = wt2.patch().unwrap();

        apply(&p, &patch1).unwrap();
        let err = apply(&p, &patch2).unwrap_err();
        assert!(
            format!("{err:#}").contains("apply"),
            "a conflicting patch was absorbed silently: {err:#}"
        );
        let _ = std::fs::remove_dir_all(&p.repo);
    }

    #[test]
    fn a_worktree_is_cleaned_up_when_dropped() {
        let p = repo();
        let base = base_commit(&p).unwrap();
        let dir;
        {
            let wt = Worktree::create(&p, "T-1", &base).unwrap();
            dir = wt.paths.repo.clone();
            assert!(dir.exists());
        }
        assert!(!dir.exists(), "a worktree leaked past its scope");
        let list = git(&p.repo, &["worktree", "list"]).unwrap();
        assert_eq!(list.lines().count(), 1, "git still lists a removed worktree:\n{list}");
        let _ = std::fs::remove_dir_all(&p.repo);
    }
}
