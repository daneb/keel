//! `keel metrics` — the harness measured over time, not one run at a time.
//!
//! PLAN.md §6 names the metric that matters most and it is not a quality
//! number: *"a gate that never fails in 20 runs is deleted or tightened."*
//! Gate theatre is invisible from inside a single run and obvious across
//! twenty, which is the whole reason this surface exists.

use crate::config::Config;
use crate::failure;
use crate::gate::Verdict;
use crate::lesson;
use crate::paths::Paths;
use crate::run::Run;
use crate::trajectory;
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Default, Serialize)]
pub struct CheckStats {
    pub gate: String,
    pub check: String,
    pub runs: usize,
    pub passed: usize,
    pub failed: usize,
    pub blocked: usize,
}

impl CheckStats {
    /// A check that has never failed across enough runs to have had the chance.
    fn is_theatre(&self, threshold: usize) -> bool {
        self.runs >= threshold && self.failed == 0 && self.blocked == 0
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Metrics {
    pub runs: usize,
    pub gate_verdicts: BTreeMap<String, BTreeMap<String, usize>>,
    pub checks: Vec<CheckStats>,
    pub failure_classes: Vec<(String, usize)>,
    pub attribution: Vec<(String, usize)>,
    pub unattributable_rate: f64,
    pub tokens_total: usize,
    pub tokens_per_run: f64,
    pub human_decisions: usize,
    /// Minutes between a run starting and each human decision on it — the
    /// closest honest proxy keel has for what a person's involvement cost.
    pub human_minutes_total: f64,
    pub human_minutes_per_run: f64,
    pub runs_awaiting_a_human: usize,
    pub lessons_in_force: usize,
    pub lessons_enforced: usize,
    pub lesson_fires: usize,
    /// Checks that have never failed in `theatre_threshold` runs.
    pub never_failed: Vec<String>,
    pub theatre_threshold: usize,
}

pub fn run(threshold: usize, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let ids = crate::run::list(&paths)?;

    let mut m = Metrics { theatre_threshold: threshold, ..Default::default() };
    let mut by_check: BTreeMap<(String, String), CheckStats> = BTreeMap::new();
    let mut episodes = Vec::new();

    for id in &ids {
        let Ok(run) = Run::load(&paths, id) else { continue };
        m.runs += 1;

        for g in run.gate_results()? {
            *m.gate_verdicts
                .entry(g.gate.clone())
                .or_default()
                .entry(g.verdict.glyph().to_lowercase())
                .or_default() += 1;

            for c in &g.checks {
                let e = by_check
                    .entry((g.gate.clone(), c.id.clone()))
                    .or_insert_with(|| CheckStats {
                        gate: g.gate.clone(),
                        check: c.id.clone(),
                        ..Default::default()
                    });
                e.runs += 1;
                match c.verdict {
                    Verdict::Pass => e.passed += 1,
                    Verdict::Fail => e.failed += 1,
                    Verdict::Blocked => e.blocked += 1,
                }
            }
        }

        let events = trajectory::read(&run.trajectory_path()).unwrap_or_default();
        m.tokens_total += trajectory::token_total(&events);
        let humans: Vec<&crate::trajectory::Event> =
            events.iter().filter(|e| e.payload.kind() == "human").collect();
        m.human_decisions += humans.len();

        // Wall-clock from the run starting to a person deciding. It is a proxy
        // and a generous one — a decision made the next morning counts the night
        // — so it is reported as elapsed time, never as effort.
        if let (Some(first), Some(last)) = (events.first(), humans.last())
            && let (Ok(start), Ok(end)) = (
                chrono::DateTime::parse_from_rfc3339(&first.t),
                chrono::DateTime::parse_from_rfc3339(&last.t),
            )
        {
            let minutes = (end - start).num_seconds() as f64 / 60.0;
            if minutes >= 0.0 {
                m.human_minutes_total += minutes;
            }
        }
        // A finished run whose gates asked for a person and never got one.
        if humans.is_empty()
            && run.meta.finished_at.is_some()
            && events.iter().any(|e| matches!(&e.payload,
                crate::trajectory::Payload::Gate { gate, verdict, .. }
                    if gate == "G3" && verdict != "pass"))
        {
            m.runs_awaiting_a_human += 1;
        }
        episodes.extend(failure::extract(&paths, &run)?);
    }

    m.tokens_per_run = if m.runs == 0 { 0.0 } else { m.tokens_total as f64 / m.runs as f64 };
    m.human_minutes_per_run = if m.runs == 0 { 0.0 } else { m.human_minutes_total / m.runs as f64 };

    let d = failure::distribution(&episodes);
    m.failure_classes = d.by_class;
    m.attribution = d.by_attribution;
    m.unattributable_rate = d.unattributable_rate;

    let lessons = lesson::list(&paths)?;
    m.lessons_in_force = lessons.len();
    m.lessons_enforced = lessons.iter().filter(|l| l.oracle().is_some()).count();
    let ledger = lesson::usage::Ledger::load(&paths)?;
    m.lesson_fires = lessons.iter().filter(|l| ledger.last_used(&l.front.id).is_some()).count();

    m.never_failed = by_check
        .values()
        .filter(|c| c.is_theatre(threshold))
        .map(|c| format!("{}/{}", c.gate, c.check))
        .collect();
    m.checks = by_check.into_values().collect();

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
