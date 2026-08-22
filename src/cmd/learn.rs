//! `keel learn`, `keel lessons`, `keel lesson promote|reject|demote`.

use crate::config::Config;
use crate::failure::{self, Attribution, Episode};
use crate::gate;
use crate::lesson::{self, Candidate};
use crate::paths::Paths;
use crate::run::Run;
use anyhow::{Result, bail};
use std::path::PathBuf;

fn candidates_path(paths: &Paths) -> PathBuf {
    paths.keel().join("candidates.json")
}

/// Episodes across every run, so occurrences are counted where they actually
/// accumulate — one run can never establish a recurrence.
fn all_episodes(paths: &Paths) -> Result<Vec<Episode>> {
    let mut out = Vec::new();
    for id in crate::run::list(paths)? {
        if let Ok(run) = Run::load(paths, &id) {
            out.extend(failure::extract(paths, &run)?);
        }
    }
    Ok(out)
}

pub fn learn(run_id: Option<String>, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;

    // The named run is what gets reported; candidates are counted across all
    // runs, because a recurrence is by definition not visible from inside one.
    let focus = crate::run::resolve(&paths, run_id)?;
    let run = Run::load(&paths, &focus)?;
    let this_run = failure::extract(&paths, &run)?;
    let everything = all_episodes(&paths)?;
    let existing = lesson::list(&paths)?;
    let candidates = lesson::propose(&everything, &existing);

    std::fs::write(
        candidates_path(&paths),
        format!("{}\n", serde_json::to_string_pretty(&candidates)?),
    )?;
    run.write_evidence("episodes.json", &serde_json::to_string_pretty(&this_run)?)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "run": focus,
            "episodes": this_run,
            "distribution": failure::distribution(&this_run),
            "candidates": candidates,
        }))?);
        return Ok(0);
    }

    println!("run {focus} — {} failure episode(s)\n", this_run.len());
    for e in &this_run {
        println!(
            "  {:<14} {:<14} {:<12} {}",
            e.attribution.code(),
            e.class.map(|c| c.code()).unwrap_or("—"),
            e.scope,
            e.signal.describe()
        );
        println!("                 {}", e.rationale);
        if let Some(r) = &e.recovery {
            println!("                 recovery: {} — {}", r.kind, r.summary);
        }
    }

    print_distribution(&this_run, &cfg);
    print_candidates(&candidates, &everything);
    Ok(0)
}

fn print_distribution(episodes: &[Episode], cfg: &Config) {
    let d = failure::distribution(episodes);
    if d.total == 0 {
        return;
    }
    println!("\nattribution");
    for (code, n) in &d.by_attribution {
        let share = *n as f64 / d.total as f64 * 100.0;
        println!("  {code:<16} {n:>3}  {share:>5.0}%");
    }
    if !d.by_class.is_empty() {
        println!("class");
        for (code, n) in &d.by_class {
            println!("  {code:<16} {n:>3}");
        }
    }
    println!(
        "\n  unattributable {:.0}% (limit {:.0}%) · {:.0}% of agentic failures are harness-fixable",
        d.unattributable_rate * 100.0,
        cfg.learn.max_unattributable_rate * 100.0,
        d.harness_fixable_rate * 100.0
    );
    if d.harness_fixable_rate < 0.5 && !episodes.is_empty() {
        println!("  mostly EDIT-* — you are measuring the model, not the harness");
    }
}

fn print_candidates(candidates: &[Candidate], all: &[Episode]) {
    println!("\ncandidates (from {} episode(s) across all runs)", all.len());
    if candidates.is_empty() {
        println!("  none — nothing agentic has recurred");
        return;
    }
    for (n, c) in candidates.iter().enumerate() {
        println!(
            "\n  [{}] {} in {} — {} run(s), {} occurrence(s){}",
            n + 1,
            c.class.code(),
            c.scope,
            c.runs.len(),
            c.occurrences,
            if c.promotable { "  PROMOTABLE" } else { "" }
        );
        println!("      rule: {}", c.rule);
        if let Some(o) = &c.oracle {
            println!("      oracle: {o}  (becomes a gate check)");
        }
        for b in &c.blocked_by {
            println!("      blocked: {b}");
        }
    }
    if candidates.iter().any(|c| c.promotable) {
        println!("\n  `keel lesson promote <n>` to accept, `keel lesson reject <n>` to decline");
    }
}

fn load_candidates(paths: &Paths) -> Result<Vec<Candidate>> {
    let p = candidates_path(paths);
    if !p.exists() {
        bail!("no candidates yet — run `keel learn` first");
    }
    Ok(serde_json::from_str(&std::fs::read_to_string(&p)?)?)
}

pub fn promote(index: usize, force: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let mut candidates = load_candidates(&paths)?;
    let Some(c) = candidates.get(index.wrapping_sub(1)).cloned() else {
        bail!("no candidate [{index}] — `keel learn` lists them");
    };

    let lesson = lesson::promote(&paths, &c, force)?;
    println!("  promoted {} — {}", lesson.front.id, c.rule);
    println!("  {}", paths.rel(&lesson.path).display());
    if lesson.oracle().is_some() {
        println!("  it has an oracle, so it is now a G2 check and costs no context");
    } else {
        println!("  no oracle, so it is injected as a prompt — give it one when you can");
    }

    // A promoted candidate is decided; leaving it in the list would have G4
    // keep asking for a decision that has been made.
    candidates.remove(index - 1);
    std::fs::write(
        candidates_path(&paths),
        format!("{}\n", serde_json::to_string_pretty(&candidates)?),
    )?;
    Ok(0)
}

pub fn reject(index: usize, note: Option<String>) -> Result<i32> {
    let paths = Paths::require_init()?;
    let mut candidates = load_candidates(&paths)?;
    if index == 0 || index > candidates.len() {
        bail!("no candidate [{index}]");
    }
    let c = candidates.remove(index - 1);
    std::fs::write(
        candidates_path(&paths),
        format!("{}\n", serde_json::to_string_pretty(&candidates)?),
    )?;
    println!(
        "  rejected {} in {}{}",
        c.class.code(),
        c.scope,
        note.map(|n| format!(" — {n}")).unwrap_or_default()
    );
    Ok(0)
}

pub fn demote(id: String, reason: Option<String>) -> Result<i32> {
    let paths = Paths::require_init()?;
    let reason = reason.unwrap_or_else(|| "unused past its decay period".to_string());
    let dest = lesson::demote(&paths, &id, &reason)?;
    println!("  demoted {id} — {reason}");
    println!("  archived at {}", paths.rel(&dest).display());
    println!("  run `keel store render` so the projections drop it");
    Ok(0)
}

pub fn list(json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let lessons = lesson::list(&paths)?;
    let ledger = lesson::usage::Ledger::load(&paths)?;

    if json {
        let rows: Vec<serde_json::Value> = lessons
            .iter()
            .map(|l| {
                serde_json::json!({
                    "id": l.front.id,
                    "class": l.front.class,
                    "scope": l.front.scope,
                    "occurrences": l.front.occurrences,
                    "rule_kind": l.front.rule_kind,
                    "enforced": l.oracle().is_some(),
                    "idle_days": ledger.idle_days(&l.front.id, &l.front.verified_at),
                    "decay": l.front.decay,
                    "sources": l.front.sources,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(0);
    }

    if lessons.is_empty() {
        println!("  no lessons in force — `keel learn` proposes them");
        return Ok(0);
    }

    let mut stale = 0;
    for l in &lessons {
        let idle = ledger.idle_days(&l.front.id, &l.front.verified_at);
        let overdue = idle as u64 > l.decay_days();
        if overdue {
            stale += 1;
        }
        println!(
            "  {:<8} {:<16} {:<18} {:>2} run(s)  {:<14} idle {:>3}d{}",
            l.front.id,
            l.front.class,
            l.front.scope,
            l.front.occurrences,
            if l.oracle().is_some() { "gate-check" } else { "prompt" },
            idle,
            if overdue { "  DECAYED" } else { "" }
        );
        if let Some(t) = l.trigger() {
            println!("           when: {t}");
        }
        if let Some(r) = l.rule() {
            println!("           rule: {r}");
        }
    }
    let enforced = lessons.iter().filter(|l| l.oracle().is_some()).count();
    println!(
        "\n  {} lesson(s), {enforced} enforced as gate checks, {} injected as prompts",
        lessons.len(),
        lessons.len() - enforced
    );
    if stale > 0 {
        println!("  {stale} past decay — `keel lesson demote <id>`");
    }
    Ok(0)
}

/// `keel gate g4` — the learning gate.
pub fn g4(run_id: Option<String>, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let id = crate::run::resolve(&paths, run_id)?;
    let run = Run::load(&paths, &id)?;

    let episodes = failure::extract(&paths, &run)?;
    let candidates = match load_candidates(&paths) {
        Ok(c) => c,
        // G4 before `keel learn` has nothing to ratify; propose on the fly so
        // the gate is never blocked on bookkeeping.
        Err(_) => lesson::propose(&all_episodes(&paths)?, &lesson::list(&paths)?),
    };

    let result = gate::g4::run(&paths, &cfg, &run, &episodes, &candidates)?;
    result.write(&run.gates_dir())?;

    let mut traj = run.open_trajectory()?;
    traj.append(crate::trajectory::Payload::Gate {
        gate: "G4".into(),
        verdict: result.verdict.glyph().to_lowercase(),
        result: "gates/G4.json".into(),
    })?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(result.verdict.exit_code());
    }

    println!("G4 — {} (run {})\n", run.meta.spec, result.run);
    for c in &result.checks {
        println!("{}", c.line());
    }
    let (p, f, b) = result.counts();
    println!("\nG4 {} — {p} passed, {f} failed, {b} blocked", result.verdict.glyph());

    // The number the plan insists stays on a dashboard.
    let d = failure::distribution(&episodes);
    if d.total > 0 {
        println!(
            "\nunattributable {:.0}% of {} episode(s) — counted, never learned from",
            d.unattributable_rate * 100.0,
            d.total
        );
    }
    Ok(result.verdict.exit_code())
}

/// Episodes across every run, for `keel failures`.
pub fn failures(json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let episodes = all_episodes(&paths)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "episodes": episodes,
            "distribution": failure::distribution(&episodes),
        }))?);
        return Ok(0);
    }
    if episodes.is_empty() {
        println!("  no failure episodes recorded");
        return Ok(0);
    }
    println!("{} episode(s) across {} run(s)", episodes.len(),
        episodes.iter().map(|e| &e.run).collect::<std::collections::BTreeSet<_>>().len());
    print_distribution(&episodes, &cfg);

    let unattributable: Vec<&Episode> = episodes
        .iter()
        .filter(|e| e.attribution == Attribution::Unattributable)
        .collect();
    if !unattributable.is_empty() {
        println!("\nunattributable — counted, never learned from:");
        for e in unattributable.iter().take(8) {
            println!("  {} {}", e.run, e.signal.describe());
        }
    }
    Ok(0)
}
