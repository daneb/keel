//! **G0 — is this spec buildable?**
//!
//! Checks (PLAN.md §4.4): every criterion in EARS form; every criterion has an
//! oracle; ambiguity count = 0; store drift = none; scope budget declared.
//!
//! Plus the anti-bloat ceilings from the Phase 1 risk note, because the
//! observed failure of spec-driven workflows is not too little ceremony but
//! far too much of it.

use super::{Check, GateResult, run_plugins};
use crate::config::Config;
use crate::paths::Paths;
use crate::projection::drift;
use crate::spec::ears::{self, Conformance};
use crate::spec::placeholder;
use crate::spec::{Criterion, SPEC_SCHEMA, Spec};
use crate::store;
use anyhow::Result;

pub fn run(paths: &Paths, cfg: &Config, spec: &Spec) -> Result<GateResult> {
    let mut checks = vec![
        schema(spec),
        criteria_present(spec),
        criterion_ids_unique(spec),
        no_placeholders(spec),
        ears_conformance(spec),
        oracle_presence(spec),
        oracle_wellformed(spec),
        ambiguity(spec, cfg),
        criterion_budget(spec, cfg),
        spec_length(spec, cfg),
        scope_declared(spec),
        change_budget_declared(spec),
        oracle_mix(spec),
        human_cost(spec),
        store_drift(paths, cfg)?,
    ];
    checks.extend(run_plugins(paths, cfg, "G0", Some(&spec.front.slug)));
    Ok(GateResult::new("G0", Some(spec.front.slug.clone()), checks))
}

fn schema(spec: &Spec) -> Check {
    if spec.front.schema != SPEC_SCHEMA {
        return Check::fail("schema", SPEC_SCHEMA, spec.front.schema.clone());
    }
    if spec.front.id.trim().is_empty() || spec.front.slug.trim().is_empty() {
        return Check::fail("schema", "front matter has id and slug", "one is empty");
    }
    Check::pass("schema", format!("{} ({})", spec.front.id, SPEC_SCHEMA))
}

fn criteria_present(spec: &Spec) -> Check {
    if spec.criteria.is_empty() {
        return Check::fail(
            "criteria-present",
            "at least one `### AC-n` acceptance criterion",
            "none found",
        );
    }
    Check::pass("criteria-present", format!("{} criteria", spec.criteria.len()))
}

fn criterion_ids_unique(spec: &Spec) -> Check {
    let mut seen: Vec<&str> = Vec::new();
    let mut dupes: Vec<&str> = Vec::new();
    for c in &spec.criteria {
        if seen.contains(&c.id.as_str()) {
            if !dupes.contains(&c.id.as_str()) {
                dupes.push(&c.id);
            }
        } else {
            seen.push(&c.id);
        }
    }
    if dupes.is_empty() {
        return Check::pass("criterion-ids-unique", "no duplicate ids");
    }
    // Duplicate ids silently break traceability at G1, so this is a hard fail.
    Check::fail("criterion-ids-unique", "unique criterion ids", format!("duplicated: {}", dupes.join(", ")))
}

/// The check that keel's own `spec new` template failed to trip, and should
/// have. See `spec::placeholder` for the incident this encodes.
fn no_placeholders(spec: &Spec) -> Check {
    let mut found: Vec<String> = Vec::new();
    for c in &spec.criteria {
        for p in placeholder::scan(&c.statement) {
            found.push(format!("{} statement: `{p}`", c.id));
        }
        for o in &c.oracles {
            for p in placeholder::scan(o.payload()) {
                found.push(format!("{} oracle: `{p}`", c.id));
            }
        }
    }
    if found.is_empty() {
        return Check::pass("no-placeholders", "no unfilled scaffold text");
    }
    Check::fail(
        "no-placeholders",
        "criteria and oracles contain no scaffold placeholders",
        format!("{} — fill these in (backtick real paths to exempt them)", super::join_capped(&found, 6)),
    )
}

fn ears_conformance(spec: &Spec) -> Check {
    let mut bad: Vec<String> = Vec::new();
    for c in &spec.criteria {
        if let Conformance::Bad(why) = ears::classify(&c.statement) {
            bad.push(format!("{} (line {}): {why}", c.id, c.line));
        }
    }
    if bad.is_empty() {
        let mut kinds: Vec<String> = Vec::new();
        for c in &spec.criteria {
            if let Conformance::Ok(p) = ears::classify(&c.statement) {
                kinds.push(p.name().to_string());
            }
        }
        kinds.sort();
        kinds.dedup();
        return Check::pass(
            "ears-conformance",
            format!("{}/{} conform ({})", spec.criteria.len(), spec.criteria.len(), kinds.join(", ")),
        );
    }
    Check::fail("ears-conformance", "every criterion in EARS form", bad.join("; "))
}

fn oracle_presence(spec: &Spec) -> Check {
    let missing: Vec<String> = spec
        .criteria
        .iter()
        .filter(|c| c.oracles.is_empty())
        .map(|c| format!("{} (line {})", c.id, c.line))
        .collect();
    if missing.is_empty() {
        let total: usize = spec.criteria.iter().map(|c| c.oracles.len()).sum();
        return Check::pass("oracle-presence", format!("{total} oracles across {} criteria", spec.criteria.len()));
    }
    Check::fail(
        "oracle-presence",
        "every criterion names a machine-checkable oracle",
        format!("no oracle on: {}", super::join_capped(&missing, 5)),
    )
}

fn oracle_wellformed(spec: &Spec) -> Check {
    let bad: Vec<String> = spec
        .criteria
        .iter()
        .flat_map(|c: &Criterion| {
            c.bad_oracles.iter().map(move |(raw, why)| format!("{}: `{raw}` — {why}", c.id))
        })
        .collect();
    if bad.is_empty() {
        return Check::pass("oracle-wellformed", "all oracles parse");
    }
    Check::fail("oracle-wellformed", "every oracle parses", bad.join("; "))
}

fn ambiguity(spec: &Spec, cfg: &Config) -> Check {
    let mut found: Vec<String> = Vec::new();
    for c in &spec.criteria {
        for a in ears::ambiguities(&c.statement) {
            found.push(format!("{} `{}` in “{}”", c.id, a.term, a.context));
        }
    }
    let max = cfg.spec.max_ambiguities;
    if found.len() <= max {
        return Check::pass(
            "ambiguity",
            if found.is_empty() {
                "no ambiguous phrasing in criteria".to_string()
            } else {
                format!("{} ambiguities, within the tolerance of {max}", found.len())
            },
        );
    }
    Check::fail(
        "ambiguity",
        format!("at most {max} ambiguous phrases in criteria"),
        format!("{}: {}", found.len(), found.join("; ")),
    )
}

fn criterion_budget(spec: &Spec, cfg: &Config) -> Check {
    let n = spec.criteria.len();
    let hard = cfg.spec.max_criteria;
    // A spec may declare a tighter self-imposed budget, never a looser one.
    let declared = spec.front.budget.criteria.unwrap_or(hard).min(hard);
    if n > declared {
        return Check::fail(
            "criterion-budget",
            format!("at most {declared} criteria"),
            format!("{n} criteria — split the spec or raise the budget deliberately"),
        );
    }
    Check::pass("criterion-budget", format!("{n}/{declared} criteria"))
}

fn spec_length(spec: &Spec, cfg: &Config) -> Check {
    let max = cfg.spec.max_lines;
    if spec.lines > max {
        return Check::fail(
            "spec-length",
            format!("at most {max} lines"),
            format!("{} lines — spec ceremony is the documented failure mode here", spec.lines),
        );
    }
    Check::pass("spec-length", format!("{}/{max} lines", spec.lines))
}

fn scope_declared(spec: &Spec) -> Check {
    let scope: Vec<&String> = spec.front.scope.iter().filter(|s| !s.trim().is_empty()).collect();
    if scope.is_empty() {
        return Check::fail(
            "scope-declared",
            "front matter declares `scope:` globs",
            "scope is empty — G1 cannot compute a blast radius without it",
        );
    }
    Check::pass("scope-declared", format!("{} glob(s): {}", scope.len(),
        scope.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")))
}

fn change_budget_declared(spec: &Spec) -> Check {
    match spec.front.budget.lines {
        Some(n) if n > 0 => Check::pass("change-budget", format!("{n} lines of diff budgeted")),
        _ => Check::fail(
            "change-budget",
            "front matter declares `budget.lines`",
            "no diff budget — G2 has nothing to ratchet against",
        ),
    }
}

/// Not a failure: the distribution of oracle kinds, so the runnable fraction of
/// a spec is a number rather than an impression. P3's whole claim is that prose
/// is not an oracle; this is where you see how much of the spec escaped that.
fn oracle_mix(spec: &Spec) -> Check {
    let mut counts: std::collections::BTreeMap<&'static str, usize> = Default::default();
    for c in &spec.criteria {
        for o in &c.oracles {
            *counts.entry(o.kind()).or_default() += 1;
        }
    }
    if counts.is_empty() {
        return Check::blocked("oracle-mix", "no oracles to classify");
    }
    let total: usize = counts.values().sum();
    let runnable = total - counts.get("human").copied().unwrap_or(0);
    let summary = counts.iter().map(|(k, n)| format!("{n} {k}")).collect::<Vec<_>>().join(", ");
    Check::pass("oracle-mix", format!("{runnable}/{total} runnable ({summary})"))
}

/// Not a failure: human oracles are legal. This check exists so the human cost
/// of a spec is a number on the record rather than a discovery.
fn human_cost(spec: &Spec) -> Check {
    let human_only = spec.human_only_criteria();
    let total_human: usize = spec
        .criteria
        .iter()
        .map(|c| c.oracles.iter().filter(|o| o.is_human()).count())
        .sum();
    if human_only.is_empty() {
        return Check::pass("human-cost", format!("{total_human} human oracle(s), none load-bearing"));
    }
    let detail: Vec<String> = human_only
        .iter()
        .map(|c| {
            let what = c.oracles.first().map(|o| o.summary()).unwrap_or_default();
            format!("{} ({what})", c.id)
        })
        .collect();
    Check::pass(
        "human-cost",
        format!(
            "{} criteria verifiable only by a person — budget review time: {}",
            human_only.len(),
            detail.join("; ")
        ),
    )
}

fn store_drift(paths: &Paths, cfg: &Config) -> Result<Check> {
    let hash = store::store_hash(paths)?;
    let reports = drift::check_all(paths, cfg, &hash)?;
    let bad: Vec<String> = reports
        .iter()
        .filter(|r| r.state.is_blocking())
        .map(|r| format!("{} ({})", r.path, r.state.glyph()))
        .collect();
    if bad.is_empty() {
        return Ok(Check::pass("store-drift", format!("{} projections current", reports.len())));
    }
    Ok(Check::fail(
        "store-drift",
        "no drifted, stale or missing projections",
        bad.join(", "),
    ))
}
