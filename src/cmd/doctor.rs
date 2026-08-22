//! `keel doctor` — is this repository's harness actually in working order?
//!
//! The individual answers already exist, scattered across `status`, `lessons`,
//! `store check` and `driver check`. Scattered is the problem: nobody runs five
//! commands to find out whether the thing they are about to trust is healthy.
//!
//! Every finding names the command that fixes it. A diagnostic that tells you
//! something is wrong without telling you what to do is a way of feeling
//! thorough.

use crate::approval::{self, Standing};
use crate::config::Config;
use crate::gate::{Check, Verdict, roll_up};
use crate::lesson;
use crate::paths::Paths;
use crate::projection::drift;
use crate::store;
use anyhow::Result;

pub fn run(json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;

    let mut checks = vec![
        index(&paths)?,
        projections(&paths, &cfg)?,
        verify_configured(&cfg),
        drivers(&paths, &cfg),
        lessons(&paths)?,
        specs(&paths)?,
        runs(&paths)?,
        shared(&paths, &cfg),
    ];
    checks.retain(|c| !c.id.is_empty());
    let verdict = roll_up(&checks);

    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "schema": "keel.doctor/1",
            "verdict": verdict,
            "checks": checks,
        }))?);
        return Ok(verdict.exit_code());
    }

    println!("keel doctor — {}\n", paths.repo.display());
    for c in &checks {
        println!("{}", c.line());
    }
    println!("\n{}", match verdict {
        Verdict::Pass => "healthy".to_string(),
        Verdict::Blocked => "degraded — some checks could not run".to_string(),
        Verdict::Fail => "unhealthy — fix the failures above before trusting a run".to_string(),
    });
    Ok(verdict.exit_code())
}

fn index(paths: &Paths) -> Result<Check> {
    let db = paths.index_db();
    if !db.exists() {
        return Ok(Check::fail("index", "a symbol index", "absent — run `keel map`"));
    }
    let index = match crate::map::db::Index::open(&db) {
        Ok(i) => i,
        Err(e) => return Ok(Check::fail("index", "a readable index", format!("{e} — run `keel map --full`"))),
    };
    let schema = index.meta("schema")?.unwrap_or_default();
    if schema != crate::map::db::SCHEMA_VERSION {
        return Ok(Check::fail(
            "index",
            crate::map::db::SCHEMA_VERSION,
            format!("{schema} — run `keel map --full`"),
        ));
    }
    let (files, symbols) = index.counts()?;
    let built = index.meta("generated_at")?.unwrap_or_else(|| "unknown".into());

    // An index older than the newest source file is answering about the past.
    let stale = newest_source_after(paths, &built);
    if let Some(path) = stale {
        return Ok(Check::fail(
            "index",
            "an index at least as new as the code",
            format!("built {built}, but {path} is newer — run `keel map`"),
        ));
    }
    Ok(Check::pass("index", format!("{files} files, {symbols} symbols, built {built}")))
}

/// A source file modified after the index was built, if any.
fn newest_source_after(paths: &Paths, built: &str) -> Option<String> {
    let built_date = chrono::NaiveDate::parse_from_str(built.trim(), "%Y-%m-%d").ok()?;
    let cfg = crate::config::MapConfig::default();
    let candidates = crate::map::walk::candidates(&paths.repo, &cfg).ok()?;
    for c in candidates {
        let Ok(meta) = std::fs::metadata(&c.abs) else { continue };
        let Ok(modified) = meta.modified() else { continue };
        let modified: chrono::DateTime<chrono::Local> = modified.into();
        if modified.date_naive() > built_date {
            return Some(c.rel);
        }
    }
    None
}

fn projections(paths: &Paths, cfg: &Config) -> Result<Check> {
    let hash = store::store_hash_with_shared(paths, cfg)?;
    let reports = drift::check_all(paths, cfg, &hash)?;
    let drifted: Vec<String> = reports
        .iter()
        .filter(|r| matches!(r.state, drift::State::Drift | drift::State::Foreign))
        .map(|r| r.path.clone())
        .collect();
    let stale: Vec<String> = reports
        .iter()
        .filter(|r| matches!(r.state, drift::State::Stale | drift::State::Missing))
        .map(|r| r.path.clone())
        .collect();

    if !drifted.is_empty() {
        return Ok(Check::fail(
            "projections",
            "no hand-edited projections",
            format!("{} — run `keel store reconcile {}`", drifted.join(", "), drifted.join(" ")),
        ));
    }
    if !stale.is_empty() {
        return Ok(Check::fail(
            "projections",
            "projections current with the store",
            format!("{} stale — run `keel store render`", stale.len()),
        ));
    }
    Ok(Check::pass("projections", format!("{} current", reports.len())))
}

fn verify_configured(cfg: &Config) -> Check {
    let missing: Vec<&str> = [
        ("build", &cfg.verify.build),
        ("test", &cfg.verify.test),
        ("lint", &cfg.verify.lint),
    ]
    .iter()
    .filter(|(_, v)| v.as_ref().is_none_or(|s| s.trim().is_empty()))
    .map(|(n, _)| *n)
    .collect();

    if missing.is_empty() {
        return Check::pass("verify", "build, test and lint commands configured");
    }
    // Not a failure of the repository — a failure to have told keel anything.
    // G2 will block on exactly these, so surfacing it here saves a run.
    Check::fail(
        "verify",
        "verify.build, verify.test and verify.lint set",
        format!("{} unset — G2 will block; set them in .keel/keel.toml", missing.join(", ")),
    )
}

fn drivers(paths: &Paths, cfg: &Config) -> Check {
    if cfg.drivers.is_empty() {
        return Check::fail("drivers", "at least one driver", "none configured");
    }
    let unreachable: Vec<String> = cfg
        .drivers
        .iter()
        .filter(|d| {
            let first = d.cmd.split_whitespace().next().unwrap_or("");
            let local = paths.repo.join(first);
            !local.is_file()
                && !std::process::Command::new("sh")
                    .args(["-c", &format!("command -v {first}")])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false)
        })
        .map(|d| format!("{} ({})", d.id, d.cmd))
        .collect();

    if unreachable.is_empty() {
        return Check::pass("drivers", format!("{} reachable", cfg.drivers.len()));
    }
    // Blocked, not failed: an adapter for a tool you have not installed is a
    // perfectly reasonable thing to have in config.
    Check::blocked(
        "drivers",
        format!(
            "{} of {} not on this machine: {} — `keel driver check` for detail",
            unreachable.len(),
            cfg.drivers.len(),
            crate::gate::join_capped(&unreachable, 3)
        ),
    )
}

fn lessons(paths: &Paths) -> Result<Check> {
    let all = lesson::list(paths)?;
    if all.is_empty() {
        return Ok(Check::pass("lessons", "none in force"));
    }
    let ledger = lesson::usage::Ledger::load(paths)?;
    let decayed: Vec<String> = all
        .iter()
        .filter(|l| ledger.idle_days(&l.front.id, &l.front.verified_at) as u64 > l.decay_days())
        .map(|l| l.front.id.clone())
        .collect();
    if decayed.is_empty() {
        let enforced = all.iter().filter(|l| l.oracle().is_some()).count();
        return Ok(Check::pass(
            "lessons",
            format!("{} in force, {enforced} enforced as checks", all.len()),
        ));
    }
    Ok(Check::fail(
        "lessons",
        "no lesson past its decay period",
        format!("{} — `keel lesson demote <id>` or re-verify", decayed.join(", ")),
    ))
}

/// A spec with no plan, or an approval nobody can still vouch for.
fn specs(paths: &Paths) -> Result<Check> {
    let slugs = crate::spec::list(paths)?;
    if slugs.is_empty() {
        return Ok(Check::pass("specs", "none yet"));
    }
    let mut orphaned = Vec::new();
    let mut superseded = Vec::new();
    for slug in &slugs {
        if !crate::plan::Plan::path_for(paths, slug).exists() {
            orphaned.push(format!("{slug} (no plan)"));
            continue;
        }
        for stage in approval::STAGES {
            if let Standing::Superseded { .. } = approval::standing(paths, slug, stage)? {
                superseded.push(format!("{slug}/{stage}"));
            }
        }
    }
    if !orphaned.is_empty() {
        return Ok(Check::fail(
            "specs",
            "every spec has a plan",
            format!("{} — run `keel plan <slug>`", orphaned.join(", ")),
        ));
    }
    if !superseded.is_empty() {
        return Ok(Check::fail(
            "specs",
            "no approval superseded by a later edit",
            format!("{} — re-approve", crate::gate::join_capped(&superseded, 4)),
        ));
    }
    Ok(Check::pass("specs", format!("{} specced and planned", slugs.len())))
}

fn shared(paths: &Paths, cfg: &Config) -> Check {
    let stores = store::shared(paths, cfg);
    if stores.is_empty() {
        return Check::pass("", "");  // nothing configured: not worth a line
    }
    let missing: Vec<String> = stores
        .iter()
        .filter(|s| s.missing)
        .map(|s| format!("{}{}", s.id, if s.required { " (required)" } else { "" }))
        .collect();
    if missing.is_empty() {
        return Check::pass("shared", format!("{} store(s) resolved", stores.len()));
    }
    if stores.iter().any(|s| s.missing && s.required) {
        return Check::fail(
            "shared",
            "every required shared store resolves",
            format!("{} — its rules are not in force", missing.join(", ")),
        );
    }
    Check::blocked("shared", format!("absent: {}", missing.join(", ")))
}

fn runs(paths: &Paths) -> Result<Check> {
    let all = crate::run::list(paths)?;
    if all.is_empty() {
        return Ok(Check::pass("runs", "none yet"));
    }
    let prunable = crate::run::prune_plan(paths, 20)?
        .iter()
        .filter(|c| c.protected_by.is_none())
        .count();
    if prunable > 10 {
        return Ok(Check::blocked(
            "runs",
            format!("{} of {} run(s) prunable — `keel runs --prune`", prunable, all.len()),
        ));
    }
    Ok(Check::pass("runs", format!("{} recorded", all.len())))
}
