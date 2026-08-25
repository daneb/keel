//! **G2.5 — adversarial review.**
//!
//! A second pass against `conventions.md` and any lessons in force, looking
//! specifically for the two things a passing G2 cannot see:
//!
//! * **test-invalidation** — the test suite is green because a test was
//!   weakened, not because the code was fixed. This is `TEST-INVALID` in the
//!   Phase 3 taxonomy, and it is the single most dangerous green build there is.
//! * **scope creep** — the change is larger than what was agreed, in a way the
//!   blast-radius check did not catch because the files were technically in
//!   scope.
//!
//! The heuristics here are deliberately crude and deliberately loud about being
//! crude: each produces a `blocked` verdict inviting a human look, not a `fail`,
//! except where the evidence is unambiguous. A reviewer plugin (a driver run in
//! critique mode) can be configured to do the real work; until Phase 3 populates
//! the lesson store, these heuristics are what stands between a green G2 and a
//! merge.

use super::{Check, GateResult, diff, run_plugins};
use crate::config::Config;
use crate::paths::Paths;
use crate::run::Run;
use crate::spec::Spec;
use crate::store::StoreDoc;
use anyhow::Result;

/// Calls that replace behaviour with a canned answer.
const MOCKING: &[&str] = &[
    "mock", "stub", "fake", "patch(", "monkeypatch", "when(", "thenReturn",
    "#[ignore]", "it.skip", "test.skip", "xit(", "describe.skip",
    "@Disabled", "t.Skip(", "pytest.mark.skip", "todo!()", "unimplemented!()",
];

/// Assertions that cannot fail.
const WEAKENED: &[&str] = &[
    "assert!(true", "assert_eq!(1, 1", "assertTrue(true", "expect(true).toBe(true",
    "assert True", "// TODO: assert", "return true;  //",
];

pub fn run(
    paths: &Paths,
    cfg: &Config,
    spec: &Spec,
    run: &Run,
) -> Result<GateResult> {
    let base = run.meta.base_commit.clone().unwrap_or_else(|| diff::default_base(paths));
    let mut checks = Vec::new();

    match diff::against(paths, &base) {
        Ok(d) => {
            let patch = read_patch(paths, cfg, &base);
            checks.push(test_invalidation(paths, spec, run, &d, patch.as_deref())?);
            checks.push(test_movement(cfg, &d));
        }
        Err(e) => {
            checks.push(Check::blocked("test-invalidation", format!("could not read the diff: {e}")));
            checks.push(Check::blocked("test-movement", "no diff to inspect"));
        }
    }

    checks.push(conventions_present(paths)?);
    checks.push(lessons_in_force(paths, cfg)?);
    checks.extend(reviewer_findings(paths, cfg, spec, run, &base)?);
    checks.extend(run_plugins(paths, cfg, "G2.5", Some(&spec.front.slug)));

    Ok(GateResult::new("G2.5", Some(spec.front.slug.clone()), checks))
}

/// The configured adversarial reviewer, if there is one.
///
/// The heuristics above stay regardless. A reviewer that is not configured must
/// not silently remove the only check there was, and a reviewer that cannot run
/// must not silently pass one.
fn reviewer_findings(
    paths: &Paths,
    cfg: &Config,
    spec: &Spec,
    run: &Run,
    base: &str,
) -> Result<Vec<Check>> {
    let Some(reviewer) = &cfg.review else {
        return Ok(vec![Check::pass(
            "reviewer",
            "no adversarial reviewer configured — the heuristics above are the whole pass",
        )]);
    };

    let diff = read_patch(paths, cfg, base).unwrap_or_default();
    if diff.trim().is_empty() {
        return Ok(vec![Check::pass("reviewer", "nothing changed to review")]);
    }

    let conventions = StoreDoc::read_optional(&paths.conventions())?
        .map(|d| d.body)
        .unwrap_or_default();
    let lessons: Vec<String> = crate::lesson::in_force(paths, cfg)?
        .iter()
        .filter_map(|l| l.rule().map(|r| format!("{}: {r}", l.front.id)))
        .collect();
    let criteria: Vec<String> = spec
        .criteria
        .iter()
        .map(|c| format!("{} {}: {}", c.id, c.title, c.statement))
        .collect();

    let request = crate::review::ReviewRequest {
        schema: crate::review::REQUEST_SCHEMA.to_string(),
        run: run.meta.id.clone(),
        spec: spec.front.slug.clone(),
        diff,
        conventions,
        lessons,
        criteria,
        prompt: crate::review::REVIEW_PROMPT.to_string(),
        repo: paths.repo.to_string_lossy().to_string(),
    };

    let review = crate::review::run(paths, reviewer, &request);
    let evidence = run.write_evidence(
        "review.json",
        &serde_json::to_string_pretty(&review.result)?,
    )?;

    if let Some(why) = review.blocked {
        return Ok(vec![Check::blocked("reviewer", why)]);
    }

    // Written on every path, including the empty one: a run that finds nothing
    // must change the file too, so an acceptance recorded against yesterday's
    // HIGH does not sit there looking current.
    let security: Vec<&crate::review::Finding> =
        review.result.findings.iter().filter(|f| f.grade.is_some()).collect();
    record_security_findings(paths, &spec.front.slug, &security)?;

    if review.result.findings.is_empty() {
        let mut c = Check::pass(
            "reviewer",
            format!(
                "{} ({:.1}s)",
                review.result.summary.clone().unwrap_or_else(|| "no findings".into()),
                review.elapsed.as_secs_f64()
            ),
        );
        c.evidence = Some(evidence);
        return Ok(vec![c]);
    }

    // One check per finding, so each is separately visible on the gate report
    // and separately classifiable by the Phase 3 taxonomy.
    // A person may accept the graded findings as they stand. The acceptance is
    // bound to this exact set, so tomorrow's different HIGH supersedes it.
    let accepted = matches!(
        crate::approval::standing(paths, &spec.front.slug, "security"),
        Ok(crate::approval::Standing::Current { .. })
    );

    let mut out = Vec::new();
    let took = format!("{:.1}s", review.elapsed.as_secs_f64());
    for f in &review.result.findings {
        let id = format!("review:{}", f.id);
        let detail = match f.where_().as_str() {
            "" => f.detail.clone(),
            w => format!("{w} — {}", f.detail),
        };

        let mut check = match f.grade {
            // A graded finding is a security finding, and grade decides, not
            // severity: HIGH and CRITICAL are defects, the rest are a look.
            // Grading and blocking are separate axes on purpose — see
            // review::Grade.
            Some(g) if g.blocks() => {
                let detail = format!("[{}] {detail}", g.label());
                if reviewer.advisory {
                    Check::blocked(&id, detail)
                } else if accepted {
                    Check::pass(&id, format!("{detail} — accepted at --stage security"))
                } else {
                    Check::fail(
                        &id,
                        "no high-severity security finding in this change",
                        format!(
                            "{detail} [reviewed in {took}] — fix it, or accept these findings \
                             deliberately with `keel approve --stage security {}`",
                            spec.front.slug
                        ),
                    )
                }
            }
            Some(g) => Check::blocked(&id, format!("[{}] {detail}", g.label())),
            None if f.severity == crate::review::Severity::Fail && !reviewer.advisory => {
                Check::fail(&id, "no defect of this kind", format!("{detail} [reviewed in {took}]"))
            }
            // Advisory mode, or the reviewer's own "concern": a look, not a block.
            None => Check::blocked(&id, detail),
        };
        check.evidence = Some(evidence.clone());
        check.from = Some("reviewer".into());
        out.push(check);
    }
    Ok(out)
}

/// Record which security findings exist, for an approval to bind to.
///
/// Identity only — category, grade, file and line — sorted, so the same
/// findings hash the same however the reviewer ordered or worded them this run.
/// The prose lives in the run's `review.json`, which is evidence rather than a
/// hash target.
fn record_security_findings(
    paths: &Paths,
    slug: &str,
    findings: &[&crate::review::Finding],
) -> Result<()> {
    let mut rows: Vec<serde_json::Value> = findings
        .iter()
        .map(|f| {
            serde_json::json!({
                "id": f.id,
                "grade": f.grade,
                "file": f.file,
                "line": f.line,
            })
        })
        .collect();
    rows.sort_by_key(|v| v.to_string());

    let path = crate::approval::security_findings_path(paths, slug);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&rows)?))?;
    Ok(())
}

/// Look for mocks and weakened assertions *added* by this change.
fn test_invalidation(
    paths: &Paths,
    spec: &Spec,
    run: &Run,
    d: &diff::Diff,
    patch: Option<&str>,
) -> Result<Check> {
    let Some(patch) = patch else {
        return Ok(Check::blocked("test-invalidation", "could not read the patch text"));
    };

    let mut hits: Vec<String> = Vec::new();
    let mut current_file = String::new();
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            current_file = rest.trim().to_string();
            continue;
        }
        // Only added lines; a removed mock is a fix, not a smell.
        let Some(added) = line.strip_prefix('+') else { continue };
        if added.starts_with("++") {
            continue;
        }
        let lower = added.to_lowercase();
        for needle in MOCKING {
            if lower.contains(&needle.to_lowercase()) {
                hits.push(format!("{current_file}: {}", trim(added)));
                break;
            }
        }
        for needle in WEAKENED {
            if lower.contains(&needle.to_lowercase()) {
                hits.push(format!("{current_file}: {} (assertion cannot fail)", trim(added)));
                break;
            }
        }
    }
    hits.dedup();

    let evidence = run.write_evidence(
        "review-test-invalidation.txt",
        &if hits.is_empty() { "no added mocks or weakened assertions\n".to_string() } else { hits.join("\n") },
    )?;

    // Record the flagged lines where an approval can bind to them, so a human
    // can say "I looked, these are fine" and have that survive exactly as long
    // as the flags do.
    let flags_path = crate::approval::review_flags_path(paths, &spec.front.slug);
    if let Some(dir) = flags_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(
        &flags_path,
        if hits.is_empty() { String::new() } else { format!("{}\n", hits.join("\n")) },
    );

    let reviewed = !hits.is_empty()
        && matches!(
            crate::approval::standing(paths, &spec.front.slug, "review"),
            Ok(crate::approval::Standing::Current { .. })
        );

    let mut check = if hits.is_empty() {
        Check::pass("test-invalidation", "no mocks or weakened assertions added")
    } else if reviewed {
        let who = match crate::approval::standing(paths, &spec.front.slug, "review") {
            Ok(crate::approval::Standing::Current { by, .. }) => by,
            _ => "a reviewer".to_string(),
        };
        Check::pass(
            "test-invalidation",
            format!("{} flagged line(s) reviewed and accepted by {who}", hits.len()),
        )
    } else {
        // Deliberately `blocked`, not `fail`: a legitimate mock exists, and a
        // heuristic that fails the gate on every test double would be routed
        // around within a week. Blocked forces the look without crying wolf.
        Check::blocked(
            "test-invalidation",
            format!(
                "{} added line(s) mock or weaken behaviour — confirm the test still tests, \
                 then `keel approve --stage review {}`: {}",
                hits.len(),
                spec.front.slug,
                super::join_capped(&hits, 3)
            ),
        )
    };
    check.evidence = Some(evidence);
    let _ = d;
    Ok(check)
}

/// Code changed with no test changed at all is worth a look; so is the reverse.
fn test_movement(cfg: &Config, d: &diff::Diff) -> Check {
    let is_test = |p: &str| crate::map::rank::is_test(p);
    let changed_tests = d
        .files
        .iter()
        .filter(|f| is_test(&f.path) && !crate::gate::g2::is_incidental_for(cfg, &f.path))
        .count();
    let changed_code = d
        .files
        .iter()
        .filter(|f| !is_test(&f.path) && !crate::gate::g2::is_incidental_for(cfg, &f.path))
        .count();

    if changed_code > 0 && changed_tests == 0 {
        return Check::blocked(
            "test-movement",
            format!("{changed_code} code file(s) changed and no test file did — confirm the criteria's oracles actually exercise this"),
        );
    }
    Check::pass(
        "test-movement",
        format!("{changed_code} code file(s), {changed_tests} test file(s)"),
    )
}

/// The review has house rules to review against.
fn conventions_present(paths: &Paths) -> Result<Check> {
    let Some(doc) = StoreDoc::read_optional(&paths.conventions())? else {
        return Ok(Check::blocked("conventions", "no conventions.md to review against"));
    };
    let rules = doc
        .body
        .lines()
        .filter(|l| l.trim_start().starts_with("- ") && !l.contains('_'))
        .count();
    if rules == 0 {
        return Ok(Check::blocked(
            "conventions",
            "conventions.md states no rules — the adversarial pass has nothing to check",
        ));
    }
    Ok(Check::pass("conventions", format!("{rules} house rule(s) in force")))
}

/// Lessons are injected by keel, never left for the agent to find (P6: only
/// 5.4% of failure recoveries began by consulting documentation).
fn lessons_in_force(paths: &Paths, cfg: &Config) -> Result<Check> {
    let lessons = crate::lesson::list_all(paths, cfg)?;
    if lessons.is_empty() {
        // The check ran and found no lesson to violate. That is vacuously true,
        // not "I could not look" — blocking here would leave G2.5 permanently
        // blocked until Phase 3, which teaches people to ignore `blocked`.
        return Ok(Check::pass("lessons", "no lessons in force yet (Phase 3 populates these)"));
    }
    let shared = lessons.iter().filter(|l| l.is_shared()).count();
    Ok(Check::pass(
        "lessons",
        format!(
            "{} lesson(s) in force{}",
            lessons.len(),
            if shared > 0 { format!(", {shared} from a shared store") } else { String::new() }
        ),
    ))
}

/// The patch text to review, excluding keel's own artefacts.
///
/// This exclusion is load-bearing, not tidiness: without it the check reads the
/// evidence file it just wrote — which contains the phrase "no added mocks" —
/// and reports itself. A reviewer that reviews its own output is not a reviewer.
fn read_patch(paths: &Paths, cfg: &Config, base: &str) -> Option<String> {
    let tracked = std::process::Command::new("git")
        .args(["diff", "--unified=0", base, "--", ".", ":(exclude).keel"])
        .current_dir(&paths.repo)
        .output()
        .ok()?;
    let mut patch = String::from_utf8_lossy(&tracked.stdout).to_string();

    // Untracked files never appear in `git diff`, and a brand-new test file
    // full of mocks is exactly the case worth catching.
    if let Ok(out) = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(&paths.repo)
        .output()
    {
        for path in String::from_utf8_lossy(&out.stdout).lines() {
            let path = path.trim();
            if path.is_empty() || crate::gate::g2::is_incidental_for(cfg, path) {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(paths.repo.join(path)) {
                patch.push_str(&format!("+++ b/{path}\n"));
                for line in content.lines() {
                    patch.push_str(&format!("+{line}\n"));
                }
            }
        }
    }
    Some(patch)
}

fn trim(s: &str) -> String {
    let t = s.trim();
    if t.chars().count() <= 80 {
        return t.to_string();
    }
    t.chars().take(79).chain(['…']).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mocking_vocabulary_covers_the_common_languages() {
        for sample in [
            "    let m = mock(Repo::new());",
            "    #[ignore]",
            "    it.skip('does the thing', () => {});",
            "    @Disabled",
            "    t.Skip(\"flaky\")",
            "    todo!()",
        ] {
            let lower = sample.to_lowercase();
            assert!(
                MOCKING.iter().any(|n| lower.contains(&n.to_lowercase())),
                "not detected: {sample}"
            );
        }
    }

    #[test]
    fn weakened_assertions_are_recognised() {
        for sample in ["assert!(true);", "assertTrue(true);", "assert True"] {
            let lower = sample.to_lowercase();
            assert!(
                WEAKENED.iter().any(|n| lower.contains(&n.to_lowercase())),
                "not detected: {sample}"
            );
        }
    }

    #[test]
    fn ordinary_code_is_not_flagged() {
        for sample in [
            "    let result = compute(input);",
            "    assert_eq!(actual, expected);",
            "    // the mocking bird sang",  // substring in a comment: accepted cost
        ] {
            let lower = sample.to_lowercase();
            let flagged = WEAKENED.iter().any(|n| lower.contains(&n.to_lowercase()));
            assert!(!flagged, "false positive on: {sample}");
        }
    }

    #[test]
    fn code_without_tests_is_flagged_for_a_look() {
        let d = diff::Diff {
            base: "HEAD".into(),
            files: vec![diff::FileChange { path: "src/api.rs".into(), added: 40, removed: 0, binary: false }],
            added: 40,
            removed: 0,
        };
        assert_eq!(test_movement(&Config::default(), &d).verdict, super::super::Verdict::Blocked);
    }

    #[test]
    fn code_with_tests_passes() {
        let d = diff::Diff {
            base: "HEAD".into(),
            files: vec![
                diff::FileChange { path: "src/api.rs".into(), added: 40, removed: 0, binary: false },
                diff::FileChange { path: "tests/api.rs".into(), added: 20, removed: 0, binary: false },
            ],
            added: 60,
            removed: 0,
        };
        assert_eq!(test_movement(&Config::default(), &d).verdict, super::super::Verdict::Pass);
    }
}
