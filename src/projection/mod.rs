//! Projections: `CLAUDE.md`, `AGENTS.md`, Kiro steering, Copilot instructions.
//!
//! These are **outputs, not inputs** (PLAN.md P2). Each carries a provenance
//! header with two hashes so `keel store check` can tell apart the two ways a
//! projection goes wrong:
//!
//! * **drift** — the file's own body no longer matches what keel wrote. A human
//!   (or an agent) edited a generated file. Re-rendering would destroy work, so
//!   this is a hard stop until it is reconciled.
//! * **stale** — the body is intact but the store has moved on. Harmless;
//!   `keel store render` fixes it.
//!
//! Conflating the two is how single-store designs quietly lose human edits.

pub mod drift;
pub mod sections;

use crate::config::{Adapter, Config};
use crate::hashing::{sha256_hex, short};
use crate::paths::Paths;
use anyhow::{Context, Result, bail};
use std::path::PathBuf;

pub const PROJECTION_SCHEMA: &str = "keel.projection/1";

#[derive(Debug, Clone)]
pub struct Rendered {
    pub adapter: String,
    pub out: PathBuf,
    pub body: String,
    pub lines: usize,
    pub budget: usize,
    pub trimmed: bool,
}

/// Render one adapter's projection body (without the provenance header).
pub fn render(paths: &Paths, cfg: &Config, adapter: &Adapter) -> Result<Rendered> {
    let out = paths.repo.join(&adapter.out);

    let (body, trimmed) = match &adapter.cmd {
        Some(cmd) => (run_plugin(paths, adapter, cmd)?, false),
        None => sections::render_builtin(paths, cfg, adapter)?,
    };

    let lines = body.lines().count();
    Ok(Rendered {
        adapter: adapter.id.clone(),
        out,
        body,
        lines,
        budget: adapter.budget,
        trimmed,
    })
}

/// P7 escape hatch: an adapter may delegate rendering to a subprocess that
/// prints the projection body on stdout.
fn run_plugin(paths: &Paths, adapter: &Adapter, cmd: &str) -> Result<String> {
    let mut parts = shell_words(cmd);
    if parts.is_empty() {
        bail!("adapter `{}` has an empty cmd", adapter.id);
    }
    let program = parts.remove(0);
    let output = std::process::Command::new(&program)
        .args(&parts)
        .current_dir(&paths.repo)
        .env("KEEL_REPO", &paths.repo)
        .env("KEEL_STORE", paths.store())
        .env("KEEL_ADAPTER", &adapter.id)
        .env("KEEL_BUDGET", adapter.budget.to_string())
        .output()
        .with_context(|| format!("running adapter plugin `{cmd}`"))?;
    if !output.status.success() {
        bail!(
            "adapter plugin `{}` exited with {}: {}",
            cmd,
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Minimal argv splitting — enough for `my-renderer --json`, and it refuses
/// rather than guessing when quoting gets interesting.
fn shell_words(cmd: &str) -> Vec<String> {
    cmd.split_whitespace().map(|s| s.to_string()).collect()
}

/// The full file content: provenance header + body.
pub fn with_header(rendered: &Rendered, store_hash: &str) -> String {
    let body_hash = sha256_hex(rendered.body.as_bytes());
    format!(
        "<!-- keel:generated schema={} adapter={} store={} body={} -->\n\
         <!-- Source of truth: .keel/store/ — regenerate with `keel store render`. \
         Edits here are drift and will be reported by `keel store check`. -->\n\n{}",
        PROJECTION_SCHEMA,
        rendered.adapter,
        short(store_hash),
        short(&body_hash),
        rendered.body
    )
}

pub fn write(rendered: &Rendered, store_hash: &str) -> Result<()> {
    if let Some(parent) = rendered.out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&rendered.out, with_header(rendered, store_hash))
        .with_context(|| format!("writing {}", rendered.out.display()))?;
    Ok(())
}

pub fn enabled_adapters(cfg: &Config) -> impl Iterator<Item = &Adapter> {
    cfg.adapters.iter().filter(|a| a.enabled)
}
