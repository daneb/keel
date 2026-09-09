//! `keel metrics` — the harness measured over time, not one run at a time.
//!
//! The aggregation itself lives in `crate::metrics` so `keel report`'s
//! executive summary can call the same computation; this module is the
//! text-printing surface over it.

use crate::config::Config;
use crate::metrics::{Metrics, compute};
use crate::paths::Paths;
use anyhow::Result;

pub fn run(threshold: usize, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let m = compute(&paths, threshold)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&m)?);
        return Ok(0);
    }
    report(&m, &cfg);
    Ok(0)
}

fn report(m: &Metrics, cfg: &Config) {
    println!("keel metrics — {} run(s)\n", m.runs);
    if m.runs == 0 {
        println!("  nothing recorded yet");
        return;
    }

    println!("gate verdicts");
    for (gate, verdicts) in &m.gate_verdicts {
        let total: usize = verdicts.values().sum();
        let passed = verdicts.get("pass").copied().unwrap_or(0);
        println!(
            "  {gate:<6} {:>3} run(s)  {:>3.0}% pass  {}",
            total,
            if total == 0 { 0.0 } else { passed as f64 / total as f64 * 100.0 },
            verdicts.iter().map(|(v, n)| format!("{n} {v}")).collect::<Vec<_>>().join(", ")
        );
    }

    if !m.attribution.is_empty() {
        println!("\nfailure attribution");
        let total: usize = m.attribution.iter().map(|(_, n)| n).sum();
        for (code, n) in &m.attribution {
            println!("  {code:<16} {n:>4}  {:>4.0}%", *n as f64 / total as f64 * 100.0);
        }
        println!(
            "  unattributable {:.0}% (limit {:.0}%)",
            m.unattributable_rate * 100.0,
            cfg.learn.max_unattributable_rate * 100.0
        );
        println!("  harness-fixable {:.0}% of agentic failures", m.harness_fixable_rate * 100.0);
    }
    if !m.failure_classes.is_empty() {
        println!("\nfailure classes");
        for (code, n) in &m.failure_classes {
            println!("  {code:<16} {n:>4}");
        }
    }

    println!("\ncontext");
    println!("  {:>8} tokens total", m.tokens_total);
    println!("  {:>8.0} tokens per run", m.tokens_per_run);
    println!("  {:>8} human decision(s) recorded", m.human_decisions);
    println!(
        "  {:>8.0} minutes elapsed to a human decision, {:.0} per run",
        m.human_minutes_total, m.human_minutes_per_run
    );
    println!("           (elapsed wall clock, not effort — a decision made the next");
    println!("            morning counts the night)");
    if m.runs_awaiting_a_human > 0 {
        println!(
            "  {:>8} finished run(s) where G3 asked for a person and none answered",
            m.runs_awaiting_a_human
        );
    }

    println!("\nlessons");
    println!(
        "  {} in force, {} enforced as gate checks, {} have ever fired",
        m.lessons_in_force, m.lessons_enforced, m.lesson_fires
    );

    // The one the plan is explicit about.
    println!("\ngate theatre");
    if m.never_failed.is_empty() {
        println!("  every check has failed or blocked at least once — none is decorative");
    } else {
        println!(
            "  {} check(s) have never failed in {}+ runs:",
            m.never_failed.len(),
            m.theatre_threshold
        );
        for c in &m.never_failed {
            println!("    {c}");
        }
        println!("  PLAN.md §6: a gate that never fails is deleted or tightened.");
        println!("  Some of these are correctly always-true; the point is to look, not to delete blindly.");
    }
}
