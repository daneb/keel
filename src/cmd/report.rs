//! `keel report` — the whole life of a feature, in one place.
//!
//! With a slug, a single feature's spine and every run's failing checks —
//! unchanged since this command shipped. Without one, the executive summary:
//! stat tiles, the pass-rate trend by week, and every check ranked worst
//! first, all from `crate::report::insights::Insights` rather than a dump of
//! every spec's full run history.

use crate::gate::Verdict;
use crate::paths::Paths;
use crate::report::Report;
use crate::report::insights::Insights;
use anyhow::Result;

pub fn run(slug: Option<String>, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;

    let Some(slug) = slug else {
        return summary(&paths, json);
    };

    let report = Report::build(&paths, Some(&slug))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(0);
    }

    for spec in &report.specs {
        println!(
            "{}  {}",
            crate::ui::bold(&spec.slug),
            crate::ui::dim(spec.stage)
        );

        for g in &spec.gates {
            let (p, f, b) = g.counts();
            println!(
                "  {:<6} {}  {}",
                g.gate,
                g.verdict.glyph_styled(),
                crate::ui::dim(&format!("{p} passed, {f} failed, {b} blocked"))
            );
        }

        for stage in crate::approval::STAGES {
            if let Some(s) = spec.approvals.get(*stage)
                && let Some(state) = s.get("state").and_then(|v| v.as_str())
                && state != "absent"
            {
                println!("  {:<6} {}", stage, styled_state(state));
            }
        }

        for r in &spec.runs {
            let verdict = match r.meta.verdict.as_deref() {
                Some("pass") => Verdict::Pass.glyph_styled(),
                Some("fail") => Verdict::Fail.glyph_styled(),
                Some("blocked") => Verdict::Blocked.glyph_styled(),
                _ => crate::ui::dim("…"),
            };
            println!(
                "  {:<20} {}  {}",
                r.meta.id,
                verdict,
                crate::ui::dim(&format!("{} events, {} tokens", r.events, r.tokens))
            );
            // Every failing check, with what it wanted and what it got.
            for g in &r.gates {
                for c in g.checks.iter().filter(|c| c.verdict != Verdict::Pass) {
                    println!("    {}", c.line().trim_start());
                }
            }
            if !r.anomalies.is_empty() {
                println!(
                    "    {}",
                    crate::ui::yellow(&format!(
                        "{} trajectory event(s) could not be read",
                        r.anomalies.len()
                    ))
                );
            }
        }
        println!();
    }
    Ok(0)
}

/// `keel report` with no slug — the executive summary.
fn summary(paths: &Paths, json: bool) -> Result<i32> {
    let insights = Insights::build(paths)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&insights)?);
        return Ok(0);
    }

    if insights.overview.specs_total == 0 {
        println!("  no specs yet — `keel spec new <slug>`");
        return Ok(0);
    }

    let o = &insights.overview;
    println!(
        "{}\n",
        crate::ui::bold(&format!("keel insights — {} spec(s), {} run(s)", o.specs_total, o.runs_total))
    );
    println!("  specs         {} total, {} complete", o.specs_total, o.specs_complete);
    println!("  pass rate     {:.0}%", o.pass_rate * 100.0);
    println!("  tokens        {} total ({} this week)", o.tokens_total, o.tokens_this_week);
    println!("  human         {} decision(s), {} awaiting", o.human_decisions, o.runs_awaiting_human);
    println!("  lessons       {} in force", o.lessons_in_force);
    if o.theatre_count > 0 {
        println!(
            "  gate theatre  {} check(s) never failed in {}+ runs",
            o.theatre_count, insights.theatre_threshold
        );
    }

    if !insights.trend.is_empty() {
        println!("\ntrend, by week");
        for w in &insights.trend {
            let rate = if w.runs == 0 { 0.0 } else { w.passed as f64 / w.runs as f64 * 100.0 };
            println!(
                "  {}   {:>3} run(s)   {:>4.0}% pass   {:>7} tokens",
                w.week_start, w.runs, rate, w.tokens
            );
        }
    }

    if !insights.attribution.is_empty() {
        println!("\nfailure attribution");
        let total: usize = insights.attribution.iter().map(|(_, n)| n).sum();
        for (code, n) in &insights.attribution {
            println!("  {code:<16} {n:>4}  {:>4.0}%", *n as f64 / total as f64 * 100.0);
        }
        println!("  harness-fixable {:.0}% of agentic failures", insights.harness_fixable_rate * 100.0);
    }
    if !insights.failure_classes.is_empty() {
        println!("\nfailure classes");
        for (code, n) in &insights.failure_classes {
            println!("  {code:<16} {n:>4}");
        }
    }

    if !insights.checks.is_empty() {
        println!("\nchecks, worst pass rate first");
        for c in insights.checks.iter().take(10) {
            let flag = if c.is_theatre(insights.theatre_threshold) { "  (theatre)" } else { "" };
            println!(
                "  {}/{:<24} {:>4.0}%  ({} runs){}",
                c.gate,
                c.check,
                c.pass_rate() * 100.0,
                c.runs,
                crate::ui::yellow(flag)
            );
        }
    }

    println!("\nspecs");
    for s in &insights.specs {
        let cycle = s
            .cycle_time_days
            .map(|d| format!("{d:.1}d to complete"))
            .unwrap_or_else(|| "in progress".to_string());
        println!(
            "  {:<20} {:<14} {:>4.0}% pass  {:>3} run(s)  {:>7} tokens  {}",
            s.slug,
            s.stage,
            s.pass_rate * 100.0,
            s.runs,
            s.tokens_total,
            crate::ui::dim(&cycle)
        );
    }

    if !insights.lessons.is_empty() {
        println!("\nlessons");
        for l in &insights.lessons {
            let enforced = if l.enforced { "enforced" } else { "advisory" };
            let stale = if l.idle_days > l.decay_days as i64 {
                crate::ui::yellow(" — past decay, review it")
            } else {
                String::new()
            };
            println!(
                "  {:<10} {:<16} {:>2} occurrence(s)  {enforced}  idle {}d{stale}",
                l.id, l.class, l.occurrences, l.idle_days
            );
        }
    }

    Ok(0)
}

fn styled_state(state: &str) -> String {
    match state {
        "current" => crate::ui::green(state),
        "rejected" | "superseded" => crate::ui::red(state),
        _ => crate::ui::yellow(state),
    }
}
