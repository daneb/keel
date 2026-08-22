//! Repository root discovery and the canonical `.keel/` layout.
//!
//! Every path in keel is derived from a single `Paths` value so that the layout
//! in PLAN.md §4.1 exists in exactly one place in the code.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    pub repo: PathBuf,
}

impl Paths {
    /// Find the repo root: nearest ancestor holding `.keel/`, else `.git/`, else cwd.
    pub fn discover() -> Result<Self> {
        let cwd = std::env::current_dir().context("cannot read current directory")?;
        let mut cur = cwd.as_path();
        loop {
            if cur.join(".keel").is_dir() {
                return Ok(Self { repo: cur.to_path_buf() });
            }
            match cur.parent() {
                Some(p) => cur = p,
                None => break,
            }
        }
        let mut cur = cwd.as_path();
        loop {
            if cur.join(".git").exists() {
                return Ok(Self { repo: cur.to_path_buf() });
            }
            match cur.parent() {
                Some(p) => cur = p,
                None => break,
            }
        }
        Ok(Self { repo: cwd })
    }

    /// Like `discover`, but fails if keel has not been initialised here.
    pub fn require_init() -> Result<Self> {
        let p = Self::discover()?;
        if !p.keel().is_dir() {
            bail!("no .keel/ found (looked upward from the current directory) — run `keel init` first");
        }
        Ok(p)
    }

    pub fn keel(&self) -> PathBuf { self.repo.join(".keel") }
    pub fn config(&self) -> PathBuf { self.keel().join("keel.toml") }
    pub fn store(&self) -> PathBuf { self.keel().join("store") }
    pub fn steering(&self) -> PathBuf { self.store().join("steering") }
    pub fn map_dir(&self) -> PathBuf { self.store().join("map") }
    pub fn index_db(&self) -> PathBuf { self.map_dir().join("index.sqlite") }
    pub fn lessons(&self) -> PathBuf { self.store().join("lessons") }
    pub fn decisions(&self) -> PathBuf { self.store().join("decisions") }
    pub fn specs(&self) -> PathBuf { self.keel().join("specs") }
    pub fn runs(&self) -> PathBuf { self.keel().join("runs") }
    pub fn inbox(&self) -> PathBuf { self.store().join("inbox") }

    pub fn product(&self) -> PathBuf { self.steering().join("product.md") }
    pub fn tech(&self) -> PathBuf { self.steering().join("tech.md") }
    pub fn structure(&self) -> PathBuf { self.steering().join("structure.md") }
    pub fn conventions(&self) -> PathBuf { self.steering().join("conventions.md") }

    /// Present `p` relative to the repo root when possible, for display.
    pub fn rel<'a>(&self, p: &'a Path) -> &'a Path {
        p.strip_prefix(&self.repo).unwrap_or(p)
    }

    pub fn scaffold(&self) -> Result<()> {
        for d in [
            self.keel(), self.store(), self.steering(), self.map_dir(),
            self.lessons(), self.decisions(), self.specs(), self.runs(), self.inbox(),
        ] {
            std::fs::create_dir_all(&d)
                .with_context(|| format!("creating {}", d.display()))?;
        }
        Ok(())
    }
}
