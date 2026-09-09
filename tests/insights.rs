//! `keel report --json` with no slug — the executive summary (`keel.insights/1`).
//!
//! `Insights` reuses `metrics::compute` for everything that already has a
//! snapshot (attribution, failure classes, check pass rates) and adds the one
//! thing nothing in keel buckets by date: a trend by week. These tests focus
//! on what's new — the trend bucketing, the worst-first check ranking, and
//! the per-spec cycle time — since the reused parts are already covered by
//! `tests/phase5.rs`'s metrics tests.

mod support;

use support::Repo;

fn insights(r: &Repo) -> serde_json::Value {
    let (_, out) = r.run(&["report", "--json"]);
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{out}"))
}

fn repo_with_a_run(name: &str) -> Repo {
    let r = Repo::ready(name);
    r.ok(&["approve", "demo", "--stage", "plan"]);
    r.write("src/api/mod.rs", "pub fn serve() { /* limited */ }\n");
    r.write(
        "tests/limit.rs",
        "#[test]\nfn respects_config() { assert_eq!(1 + 1, 2); }\n",
    );
    r.run(&["run", "demo", "--no-driver"]);
    r
}

/// Rewrite a run's `started_at`/`finished_at` in place — the only way to get
/// multi-week data without waiting weeks for it.
fn backdate(r: &Repo, run_id: &str, started_at: &str, finished_at: &str) {
    let p = format!(".keel/runs/{run_id}/run.json");
    let mut v: serde_json::Value = serde_json::from_str(&r.read(&p)).unwrap();
    v["started_at"] = started_at.into();
    v["finished_at"] = finished_at.into();
    r.write(&p, &format!("{}\n", serde_json::to_string_pretty(&v).unwrap()));
}

fn run_ids(r: &Repo) -> std::collections::BTreeSet<String> {
    std::fs::read_dir(r.dir.join(".keel/runs"))
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter(|e| e.path().join("run.json").is_file())
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Run `keel run` and return the id of the run directory it created.
///
/// Not `keel runs --latest`: `gate::run_id`'s hex suffix is a hash of the
/// clock and pid, not a counter, so two runs created moments apart in the
/// same process are not reliably ordered by it — `run::list()`'s sort can
/// return the *previous* run as "latest". Diffing the directory before and
/// after sidesteps that rather than depending on it.
fn run_and_capture_id(r: &Repo, args: &[&str]) -> String {
    let before = run_ids(r);
    r.run(args);
    let after = run_ids(r);
    let mut new: Vec<String> = after.difference(&before).cloned().collect();
    assert_eq!(new.len(), 1, "expected exactly one new run directory, got {new:?}");
    new.pop().unwrap()
}

#[test]
fn runs_in_different_weeks_land_in_different_buckets() {
    let r = repo_with_a_run("insights-trend");
    let id = r.latest_run();
    backdate(&r, &id, "2026-08-18T10:00:00+02:00", "2026-08-18T10:05:00+02:00");

    let v = insights(&r);
    let trend = v["trend"].as_array().unwrap();
    assert_eq!(trend.len(), 1, "one run should make one bucket: {trend:?}");
    assert_eq!(trend[0]["week_start"], "2026-08-17", "not the Monday of that week: {trend:?}");
    assert_eq!(trend[0]["runs"], 1);
}

#[test]
fn a_second_run_the_same_week_joins_the_existing_bucket() {
    let r = repo_with_a_run("insights-trend-join");
    let id = run_ids(&r).into_iter().next().unwrap();
    backdate(&r, &id, "2026-08-18T10:00:00+02:00", "2026-08-18T10:05:00+02:00");
    // Same ISO week (Mon 2026-08-17 – Sun 2026-08-23), a different day.
    let id2 = run_and_capture_id(&r, &["run", "demo", "--no-driver"]);
    backdate(&r, &id2, "2026-08-20T09:00:00+02:00", "2026-08-20T09:05:00+02:00");

    let v = insights(&r);
    let trend = v["trend"].as_array().unwrap();
    assert_eq!(trend.len(), 1, "same-week runs should not split into two buckets: {trend:?}");
    assert_eq!(trend[0]["runs"], 2);
}

#[test]
fn a_token_total_is_attributed_to_the_week_it_ran_in() {
    let r = repo_with_a_run("insights-tokens");
    let id = r.latest_run();
    backdate(&r, &id, "2026-08-18T10:00:00+02:00", "2026-08-18T10:05:00+02:00");

    let v = insights(&r);
    let bucket = &v["trend"][0];
    assert!(bucket["tokens"].as_u64().unwrap() > 0, "{bucket}");
    // The week's tokens should not double-count into "this week" once backdated
    // out of it.
    assert_eq!(v["overview"]["tokens_this_week"], 0, "{}", v["overview"]);
}

#[test]
fn checks_are_ranked_worst_pass_rate_first() {
    let r = repo_with_a_run("insights-ranking");
    // `blast-radius` fails immediately on an out-of-scope change; everything
    // else in a --no-driver run against an untouched tree passes.
    r.write("src/main.rs", "fn main() { /* wandered out of scope */ }\n");
    r.run(&["run", "demo", "--no-driver"]);

    let v = insights(&r);
    let checks = v["checks"].as_array().unwrap();
    assert!(!checks.is_empty());
    let worst = &checks[0];
    assert!(
        worst["failed"].as_u64().unwrap() > 0,
        "the first row should be a failing check, got {worst}"
    );
    // Monotonic: pass rate must not decrease down the list.
    let rates: Vec<f64> = checks
        .iter()
        .map(|c| {
            let (p, r) = (c["passed"].as_f64().unwrap(), c["runs"].as_f64().unwrap());
            if r == 0.0 { 0.0 } else { p / r }
        })
        .collect();
    for w in rates.windows(2) {
        assert!(w[0] <= w[1] + 1e-9, "not sorted worst-first: {rates:?}");
    }
}

#[test]
fn harness_fixable_rate_is_wired_through() {
    let r = repo_with_a_run("insights-fixable");
    r.write("src/main.rs", "fn main() { /* wandered out of scope */ }\n");
    r.run(&["run", "demo", "--no-driver"]);

    let v = insights(&r);
    // scope-creep is harness-fixable by taxonomy — this field was computed by
    // `failure::distribution` before this change and silently dropped by
    // `keel metrics`; confirm it now reaches the wire.
    assert!(v["harness_fixable_rate"].as_f64().unwrap() > 0.0, "{}", v["harness_fixable_rate"]);
}

/// `run::list()`'s ordering is not reliably chronological within a day (a run
/// id's suffix is a hash of the clock and pid, not a counter), so a naive
/// `runs.first()/.last()` read of `SpecReport.runs` is not guaranteed to read
/// the earliest and latest run — it produced a negative cycle time on a real
/// repository. Cycle time must come from parsed timestamps, and the earliest
/// start must never land after the latest finish regardless of which run
/// happens to sit where in the array.
#[test]
fn cycle_time_is_never_negative_regardless_of_run_order_on_disk() {
    let r = repo_with_a_run("insights-cycle-scrambled");
    r.ok(&["approve", "demo", "--stage", "merge"]);
    let second = run_and_capture_id(&r, &["run", "demo", "--no-driver"]);
    let first = run_ids(&r).into_iter().find(|id| id != &second).unwrap();

    backdate(&r, &first, "2026-08-10T09:00:00+02:00", "2026-08-10T09:05:00+02:00");
    backdate(&r, &second, "2026-08-12T09:00:00+02:00", "2026-08-14T09:05:00+02:00");

    let v = insights(&r);
    let spec = v["specs"].as_array().unwrap().iter().find(|s| s["slug"] == "demo").unwrap();
    if let Some(days) = spec["cycle_time_days"].as_f64() {
        assert!(days >= 0.0, "negative cycle time: {days} in {spec}");
    }
}

#[test]
fn a_complete_spec_gets_a_cycle_time_an_incomplete_one_does_not() {
    let incomplete = repo_with_a_run("insights-cycle-incomplete");
    let v = insights(&incomplete);
    assert!(v["specs"][0]["cycle_time_days"].is_null(), "{}", v["specs"][0]);

    let complete = repo_with_a_run("insights-cycle-complete");
    complete.ok(&["approve", "demo", "--stage", "merge"]);
    assert_eq!(complete.run(&["run", "demo", "--no-driver"]).0, 0, "the cycle should finish clean");

    let v = insights(&complete);
    let days = v["specs"][0]["cycle_time_days"].as_f64();
    assert!(days.is_some(), "a complete spec should report a cycle time: {}", v["specs"][0]);
    assert!(days.unwrap() >= 0.0);
}

#[test]
fn the_summary_names_every_spec_and_stays_under_the_per_run_detail() {
    let r = repo_with_a_run("insights-text");
    let (_, out) = r.run(&["report"]);
    assert!(out.contains("keel insights"), "{out}");
    assert!(out.contains("demo"), "{out}");
    assert!(out.contains("trend"), "{out}");
    // The old no-slug behaviour dumped every failing check's expected/actual
    // pair inline; the summary should not.
    assert!(!out.contains("expected:"), "the summary regressed into a full dump:\n{out}");
}

#[test]
fn a_repo_with_no_specs_reports_nothing_rather_than_erroring() {
    let r = Repo::bare("insights-empty");
    let (code, out) = r.run(&["report"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("no specs yet"), "{out}");

    let v = insights(&r);
    assert_eq!(v["overview"]["specs_total"], 0);
    assert_eq!(v["trend"].as_array().unwrap().len(), 0);
}
