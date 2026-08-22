//! The budget governor (PLAN.md P4).
//!
//! > Budgets are declared per stage and enforced, not advised. A full-file read
//! > above `N` lines requires a recorded justification, which the review gate
//! > can see.
//!
//! The justification is the interesting half. keel cannot stop an agent reading
//! a 2,000-line file — it can make doing so cost a sentence that ends up in the
//! trajectory, where a reviewer will see how often "just read the whole thing"
//! was the strategy.

use crate::config::Config;
use crate::paths::Paths;
use crate::run::Run;
use crate::trajectory::Payload;
use anyhow::{Result, bail};

/// Record a large read against the active run, or refuse it.
pub fn account_for_read(
    paths: &Paths,
    cfg: &Config,
    what: &str,
    lines: usize,
    justification: Option<&str>,
) -> Result<()> {
    let limit = cfg.retrieve.max_unjustified_lines;
    if lines <= limit {
        return Ok(());
    }
    let Some(reason) = justification.filter(|r| !r.trim().is_empty()) else {
        bail!(
            "{what} is {lines} lines, over the {limit}-line limit for an unjustified read.\n\
             Use `keel outline` or `keel symbol` first, or pass --justify \"<why the whole body is needed>\".",
        );
    };

    // A justification nobody can see is not a justification.
    if let Some(id) = crate::run::latest(paths)?
        && let Ok(run) = Run::load(paths, &id)
        && run.meta.finished_at.is_none()
    {
        let mut traj = run.open_trajectory()?;
        traj.append(Payload::Command {
            cmd: format!("read {what} ({lines} lines)"),
            exit_code: 0,
            evidence: Some(format!("justified: {reason}")),
        })?;
    }
    Ok(())
}

/// The token budget for a stage.
pub fn for_stage(cfg: &Config, stage: &str) -> usize {
    match stage {
        "slice" => cfg.retrieve.slice_tokens,
        _ => cfg.retrieve.query_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> Paths {
        Paths { repo: std::env::temp_dir().join("keel-budget-none") }
    }

    #[test]
    fn a_small_read_needs_no_justification() {
        let cfg = Config::default();
        assert!(account_for_read(&paths(), &cfg, "x", 10, None).is_ok());
    }

    #[test]
    fn a_large_read_without_a_reason_is_refused_with_the_alternative() {
        let cfg = Config::default();
        let err = account_for_read(&paths(), &cfg, "src/big.rs", 5000, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("5000 lines"), "{err}");
        assert!(err.contains("keel outline"), "the refusal must name the cheaper path: {err}");
        assert!(err.contains("--justify"), "{err}");
    }

    #[test]
    fn an_empty_justification_does_not_count() {
        let cfg = Config::default();
        assert!(account_for_read(&paths(), &cfg, "x", 5000, Some("   ")).is_err());
    }

    #[test]
    fn a_justified_read_is_allowed() {
        let cfg = Config::default();
        assert!(account_for_read(&paths(), &cfg, "x", 5000, Some("tracing a panic")).is_ok());
    }

    #[test]
    fn stage_budgets_differ() {
        let cfg = Config::default();
        assert!(for_stage(&cfg, "slice") > for_stage(&cfg, "symbol"));
    }
}
