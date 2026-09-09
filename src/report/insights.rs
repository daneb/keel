//! The executive view: trends, rankings and comparisons across every spec.
//!
//! [`super::Report`] answers "where does this one feature stand." `Insights`
//! answers the questions that only show up once you look across all of
//! them — is the pass rate moving, which check is decorative, which spec is
//! stuck, which lesson has gone quiet. Every number here is either read
//! straight off [`crate::metrics::compute`] (the same aggregation `keel
//! metrics` already trusts) or built by adding one thing that computation
//! doesn't have: a time axis. Nothing is re-derived that already exists.
//!
//! # Not on the spine
//!
//! `keel.report/1` is additive-only under the spine freeze, and deliberately
//! thin. `Insights` is new and still settling — it does not join that schema.
//! `keel.insights/1` is versioned in name only; treat it, like the per-run
//! JSON under `/api/run/`, as this view's private wire rather than a
//! contract (see ROADMAP.md's "spine freeze" section).

use crate::lesson;
use crate::metrics::{self, CheckStats};
use crate::paths::Paths;
use crate::pipeline::Stage;
use crate::report::Report;
use anyhow::Result;
use chrono::{DateTime, Weekday};
use serde::Serialize;

pub const SCHEMA: &str = "keel.insights/1";

/// A gate check crossing this many runs with zero failures is worth a look —
/// same default PLAN.md §6 and `keel metrics --threshold` use.
const THEATRE_THRESHOLD: usize = 20;

#[derive(Debug, Serialize)]
pub struct Insights {
    pub schema: &'static str,
    pub generated_at: String,
    pub overview: Overview,
    /// Oldest week first.
    pub trend: Vec<WeekBucket>,
    pub attribution: Vec<(String, usize)>,
    pub failure_classes: Vec<(String, usize)>,
    pub harness_fixable_rate: f64,
    /// Worst pass rate first — the ranking `keel metrics`'s text report makes
    /// you compute yourself by eye.
    pub checks: Vec<CheckStats>,
    pub theatre_threshold: usize,
    pub specs: Vec<SpecSummary>,
    pub lessons: Vec<LessonSummary>,
}

#[derive(Debug, Default, Serialize)]
pub struct Overview {
    pub specs_total: usize,
    pub specs_complete: usize,
    pub runs_total: usize,
    pub pass_rate: f64,
    pub tokens_total: usize,
    pub tokens_this_week: usize,
    pub human_decisions: usize,
    pub runs_awaiting_human: usize,
    pub lessons_in_force: usize,
    pub theatre_count: usize,
}

#[derive(Debug, Serialize)]
pub struct WeekBucket {
    /// ISO date of the Monday the week starts on.
    pub week_start: String,
    pub runs: usize,
    pub passed: usize,
    pub failed: usize,
    pub blocked: usize,
    pub tokens: usize,
}

#[derive(Debug, Serialize)]
pub struct SpecSummary {
    pub slug: String,
    pub stage: &'static str,
    pub runs: usize,
    pub pass_rate: f64,
    pub tokens_total: usize,
    pub last_run_at: Option<String>,
    /// First run's start to the most recent run's finish, only once the spec
    /// has reached `Complete`. keel does not replay historical stage
    /// transitions, so this is a proxy — first attempt to most recent
    /// finish — rather than "the run that actually completed it."
    pub cycle_time_days: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct LessonSummary {
    pub id: String,
    pub class: String,
    pub occurrences: usize,
    pub enforced: bool,
    pub last_used: Option<String>,
    pub idle_days: i64,
    pub decay_days: u64,
}

impl Insights {
    pub fn build(paths: &Paths) -> Result<Self> {
        let m = metrics::compute(paths, THEATRE_THRESHOLD)?;
        let report = Report::build(paths, None)?;

        let mut checks = m.checks.clone();
        checks.sort_by(|a, b| {
            a.pass_rate()
                .partial_cmp(&b.pass_rate())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.runs.cmp(&a.runs))
        });

        let trend = build_trend(&report);
        let specs = build_spec_summaries(&report);
        let lessons = build_lesson_summaries(paths)?;

        let runs_total = specs.iter().map(|s| s.runs).sum();
        let passed_total: usize = report
            .specs
            .iter()
            .flat_map(|s| &s.runs)
            .filter(|r| r.meta.verdict.as_deref() == Some("pass"))
            .count();
        let this_week = week_start(chrono::Local::now().naive_local().date());
        let tokens_this_week = trend
            .iter()
            .find(|b| b.week_start == this_week)
            .map(|b| b.tokens)
            .unwrap_or(0);

        let overview = Overview {
            specs_total: report.specs.len(),
            specs_complete: specs.iter().filter(|s| s.stage == Stage::Complete.key()).count(),
            runs_total,
            pass_rate: if runs_total == 0 { 0.0 } else { passed_total as f64 / runs_total as f64 },
            tokens_total: m.tokens_total,
            tokens_this_week,
            human_decisions: m.human_decisions,
            runs_awaiting_human: m.runs_awaiting_a_human,
            lessons_in_force: m.lessons_in_force,
            theatre_count: m.never_failed.len(),
        };

        Ok(Self {
            schema: SCHEMA,
            generated_at: chrono::Local::now().to_rfc3339(),
            overview,
            trend,
            attribution: m.attribution,
            failure_classes: m.failure_classes,
            harness_fixable_rate: m.harness_fixable_rate,
            checks,
            theatre_threshold: THEATRE_THRESHOLD,
            specs,
            lessons,
        })
    }
}

/// The Monday of the week `date` falls in, as `YYYY-MM-DD`.
fn week_start(date: chrono::NaiveDate) -> String {
    date.week(Weekday::Mon).first_day().format("%Y-%m-%d").to_string()
}

fn build_trend(report: &Report) -> Vec<WeekBucket> {
    use std::collections::BTreeMap;
    let mut buckets: BTreeMap<String, WeekBucket> = BTreeMap::new();

    for spec in &report.specs {
        for run in &spec.runs {
            let Ok(started) = DateTime::parse_from_rfc3339(&run.meta.started_at) else { continue };
            let key = week_start(started.naive_local().date());
            let b = buckets.entry(key.clone()).or_insert_with(|| WeekBucket {
                week_start: key,
                runs: 0,
                passed: 0,
                failed: 0,
                blocked: 0,
                tokens: 0,
            });
            b.runs += 1;
            b.tokens += run.tokens;
            match run.meta.verdict.as_deref() {
                Some("pass") => b.passed += 1,
                Some("fail") => b.failed += 1,
                Some("blocked") => b.blocked += 1,
                _ => {} // still in flight, or never finished
            }
        }
    }
    buckets.into_values().collect()
}

fn build_spec_summaries(report: &Report) -> Vec<SpecSummary> {
    report
        .specs
        .iter()
        .map(|spec| {
            let runs = spec.runs.len();
            let passed = spec.runs.iter().filter(|r| r.meta.verdict.as_deref() == Some("pass")).count();
            let tokens_total = spec.runs.iter().map(|r| r.tokens).sum();

            // `SpecReport.runs` is not reliably chronological — it inherits
            // `run::list()`'s lexicographic sort, and a run id's suffix is a
            // hash of the clock and pid, not a same-day-monotonic counter.
            // "First" and "last" here mean earliest/latest by parsed
            // timestamp, never array position — array order previously
            // produced a negative cycle time on a real repository.
            let mut started: Vec<DateTime<chrono::FixedOffset>> = spec
                .runs
                .iter()
                .filter_map(|r| DateTime::parse_from_rfc3339(&r.meta.started_at).ok())
                .collect();
            started.sort();
            let last_run_at = started.last().map(|d| d.to_rfc3339());

            let ends: Vec<DateTime<chrono::FixedOffset>> = spec
                .runs
                .iter()
                .filter_map(|r| {
                    let end = r.meta.finished_at.as_deref().unwrap_or(&r.meta.started_at);
                    DateTime::parse_from_rfc3339(end).ok()
                })
                .collect();

            let cycle_time_days = if spec.complete {
                match (started.first(), ends.iter().max()) {
                    (Some(f), Some(l)) => Some((*l - *f).num_seconds() as f64 / 86_400.0),
                    _ => None,
                }
            } else {
                None
            };

            SpecSummary {
                slug: spec.slug.clone(),
                stage: spec.stage,
                runs,
                pass_rate: if runs == 0 { 0.0 } else { passed as f64 / runs as f64 },
                tokens_total,
                last_run_at,
                cycle_time_days,
            }
        })
        .collect()
}

fn build_lesson_summaries(paths: &Paths) -> Result<Vec<LessonSummary>> {
    let lessons = lesson::list(paths)?;
    let ledger = lesson::usage::Ledger::load(paths)?;
    Ok(lessons
        .iter()
        .map(|l| LessonSummary {
            id: l.front.id.clone(),
            class: l.front.class.clone(),
            occurrences: l.front.occurrences,
            enforced: l.oracle().is_some(),
            last_used: ledger.last_used(&l.front.id),
            idle_days: ledger.idle_days(&l.front.id, &l.front.verified_at),
            decay_days: l.decay_days(),
        })
        .collect())
}
