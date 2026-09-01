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
        // Not `--waves`-specific any more: this also backs a single-task run's
        // gate base, so the message no longer names one caller.
        Err(_) => bail!(
            "keel needs at least one commit to diff a run against — this repository has none yet.\n\
             Make one commit and try again."
        ),
    }
}

/// The commit a gate should diff against.
///
/// With a driver, the agent's work is uncommitted, so HEAD is the right base.
/// Without one (`--no-driver`) the work is usually already committed on a
/// branch — and diffing that against HEAD compares a tree with itself, so every
/// diff-based check passes on an empty diff. A gate that reports green because
/// it looked at nothing is worse than one that fails.
///
/// So when gating a branch, the base is where that branch left the trunk.
/// `explicit` (from `--base`) always wins; if no trunk can be identified, this
/// falls back to HEAD and the caller is no worse off than before.
pub fn gate_base(paths: &Paths, explicit: Option<&str>, branch_point: bool) -> Result<String> {
    if let Some(r) = explicit {
        let resolved = git(&paths.repo, &["rev-parse", r])
            .with_context(|| format!("--base {r} is not a commit this repository knows"))?;
        return Ok(resolved.trim().to_string());
    }
    let head = base_commit(paths)?;
    if !branch_point {
        return Ok(head);
    }

    let current = git(&paths.repo, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_default()
        .trim()
        .to_string();
    let trunks = trunk_candidates(paths);

    // If we're already on a trunk branch, there is no branch point to find --
    // searching anyway would merge-base against origin/<trunk>, which (with
    // unpushed local commits) resolves to a stale ancestor, not HEAD. Compare
    // on the local name (`origin/<x>` stripped to `<x>`) so this catches
    // "current is master" even though the candidate list only ever holds
    // remote-qualified names for it.
    if trunks.iter().any(|t| t.strip_prefix("origin/").unwrap_or(t) == current) {
        return Ok(head);
    }

    for trunk in trunks {
        if trunk == current {
            continue;
        }
        // Resolve the trunk to a commit so we can detect the case where the
        // branch points at the same commit as trunk — there is no divergence
        // even though the names differ, and merge-base against a stale
        // remote ref would return an ancestor that predates work already on
        // trunk. This is the common case for a branch just created off master
        // with only uncommitted changes.
        let trunk_commit = git(&paths.repo, &["rev-parse", &trunk])
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if trunk_commit == head {
            return Ok(head);
        }
        if let Ok(mb) = git(&paths.repo, &["merge-base", &trunk, "HEAD"]) {
            let mb = mb.trim().to_string();
            // Only useful if the branch actually diverged from that trunk.
            if !mb.is_empty() && mb != head {
                return Ok(mb);
            }
        }
    }
    Ok(head)
}

/// Trunk names to try, most authoritative first.
///
/// Remote-qualified names come before bare local ones. A remote-tracking ref
/// only moves on `git fetch`, so it reflects the shared trunk as of the last
/// sync; a local `main`/`master` branch can sit unchecked-out and un-advanced
/// for months while its owner works entirely on feature branches. Trying the
/// stale local name first meant merge-basing against wherever that branch was
/// left at clone time, silently pulling everything a teammate has pushed
/// since into "the diff" as if the caller had written it themselves.
fn trunk_candidates(paths: &Paths) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(sym) = git(&paths.repo, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]) {
        let s = sym.trim().to_string();
        if !s.is_empty() {
            out.push(s);
        }
    }
    for name in ["origin/main", "origin/master", "main", "master"] {
        if git(&paths.repo, &["rev-parse", "--verify", name]).is_ok() {
            out.push(name.to_string());
        }
    }
    out
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
    fn gate_base_finds_the_branch_point_not_head() {
        let p = repo();
        let trunk_commit = base_commit(&p).unwrap();
        git(&p.repo, &["checkout", "-q", "-b", "feature"]).unwrap();
        std::fs::write(p.repo.join("a.txt"), "one\ntwo\n").unwrap();
        git(&p.repo, &["add", "-A"]).unwrap();
        git(&p.repo, &["commit", "-q", "-m", "feature work"]).unwrap();

        // branch_point = true (as --no-driver passes) must land on the trunk
        // commit, not on the feature branch's own HEAD -- landing on HEAD is
        // exactly what makes a committed branch diff against itself and every
        // check pass on nothing.
        let base = gate_base(&p, None, true).unwrap();
        assert_eq!(base, trunk_commit, "should diff from where the branch left master, not from HEAD");

        // branch_point = false (a driver run) keeps the old HEAD behaviour --
        // an agent's uncommitted work is diffed from where it started.
        let head_base = gate_base(&p, None, false).unwrap();
        assert_eq!(head_base, base_commit(&p).unwrap());
        assert_ne!(head_base, trunk_commit);

        let _ = std::fs::remove_dir_all(&p.repo);
    }

    #[test]
    fn gate_base_on_trunk_with_unpushed_commits_stays_at_head() {
        let p = repo();
        let trunk = git(&p.repo, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap().trim().to_string();
        let pushed_commit = base_commit(&p).unwrap();

        // A real origin remote, with the first commit pushed to it -- this is
        // the shape the bug needs: origin/<trunk> exists, and is behind HEAD.
        let origin = p.repo.with_file_name(format!(
            "{}-origin",
            p.repo.file_name().unwrap().to_string_lossy()
        ));
        git(&p.repo, &["init", "-q", "--bare", &origin.to_string_lossy()]).unwrap();
        git(&p.repo, &["remote", "add", "origin", &origin.to_string_lossy()]).unwrap();
        git(&p.repo, &["push", "-q", "origin", &trunk]).unwrap();

        // A second commit that is NOT pushed -- still on the trunk branch
        // directly, no feature branch involved.
        std::fs::write(p.repo.join("a.txt"), "one\ntwo\n").unwrap();
        git(&p.repo, &["add", "-A"]).unwrap();
        git(&p.repo, &["commit", "-q", "-m", "unpushed work"]).unwrap();
        let head = base_commit(&p).unwrap();
        assert_ne!(head, pushed_commit);

        // Being on the trunk branch itself, gate_base must return HEAD, not
        // merge-base(origin/<trunk>, HEAD) -- which would resolve to the
        // stale, already-pushed commit and hide the unpushed work from the
        // diff-based gate checks.
        let base = gate_base(&p, None, true).unwrap();
        assert_eq!(
            base, head,
            "on trunk with unpushed commits, gate_base must not fall back to a stale origin merge-base"
        );

        let _ = std::fs::remove_dir_all(&p.repo);
        let _ = std::fs::remove_dir_all(&origin);
    }

    #[test]
    fn gate_base_prefers_the_fetched_remote_trunk_over_a_stale_local_branch() {
        let p = repo();
        let trunk = git(&p.repo, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap().trim().to_string();
        let stale_local_trunk = base_commit(&p).unwrap();

        let origin = p.repo.with_file_name(format!(
            "{}-origin",
            p.repo.file_name().unwrap().to_string_lossy()
        ));
        git(&p.repo, &["init", "-q", "--bare", &origin.to_string_lossy()]).unwrap();
        git(&p.repo, &["remote", "add", "origin", &origin.to_string_lossy()]).unwrap();
        git(&p.repo, &["push", "-q", "origin", &trunk]).unwrap();

        // A second clone stands in for a teammate: it advances the trunk on
        // origin without ever touching this repository's own local branch —
        // the exact shape of a repo whose local `main`/`master` has sat
        // unchecked-out since it was cloned.
        let other = p.repo.with_file_name(format!(
            "{}-other",
            p.repo.file_name().unwrap().to_string_lossy()
        ));
        git(&p.repo, &["clone", "-q", &origin.to_string_lossy(), &other.to_string_lossy()]).unwrap();
        git(&other, &["config", "user.email", "t@example.com"]).unwrap();
        git(&other, &["config", "user.name", "Test"]).unwrap();
        std::fs::write(other.join("teammate.txt"), "teammate work\n").unwrap();
        git(&other, &["add", "-A"]).unwrap();
        git(&other, &["commit", "-q", "-m", "teammate work"]).unwrap();
        git(&other, &["push", "-q", "origin", &trunk]).unwrap();

        // Fetching moves the remote-tracking ref here without moving the
        // local trunk branch.
        git(&p.repo, &["fetch", "-q", "origin"]).unwrap();
        let fetched_trunk =
            git(&p.repo, &["rev-parse", &format!("origin/{trunk}")]).unwrap().trim().to_string();
        assert_ne!(fetched_trunk, stale_local_trunk, "the fetch must actually have moved origin/<trunk>");
        // A modern `git fetch` auto-populates refs/remotes/origin/HEAD when it
        // is missing, which would otherwise mask exactly the bug this test
        // guards: drop it so the fallback candidate order — the actual fix —
        // is what gets exercised, the way an older git or a repo that has
        // pruned this ref would leave it.
        git(&p.repo, &["symbolic-ref", "--delete", "refs/remotes/origin/HEAD"]).unwrap();

        git(&p.repo, &["checkout", "-q", "-b", "feature", &format!("origin/{trunk}")]).unwrap();
        std::fs::write(p.repo.join("a.txt"), "one\ntwo\n").unwrap();
        git(&p.repo, &["add", "-A"]).unwrap();
        git(&p.repo, &["commit", "-q", "-m", "feature work"]).unwrap();

        // The feature branch forked from the fetched, up-to-date trunk. A
        // stale local `<trunk>` left behind at the first push is still an
        // ancestor of HEAD, so it "diverges" too -- just at the wrong,
        // much older point, dragging the teammate's commit into the diff.
        let base = gate_base(&p, None, true).unwrap();
        assert_eq!(
            base, fetched_trunk,
            "must diff from the fetched origin/<trunk>, not the stale local branch left behind at clone time"
        );

        let _ = std::fs::remove_dir_all(&p.repo);
        let _ = std::fs::remove_dir_all(&origin);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn explicit_base_always_wins() {
        let p = repo();
        let trunk_commit = base_commit(&p).unwrap();
        git(&p.repo, &["checkout", "-q", "-b", "feature"]).unwrap();
        std::fs::write(p.repo.join("a.txt"), "changed\n").unwrap();
        git(&p.repo, &["add", "-A"]).unwrap();
        git(&p.repo, &["commit", "-q", "-m", "work"]).unwrap();

        let explicit = gate_base(&p, Some("HEAD"), true).unwrap();
        assert_ne!(explicit, trunk_commit, "--base HEAD must not be overridden by branch-point inference");
        let _ = std::fs::remove_dir_all(&p.repo);
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
