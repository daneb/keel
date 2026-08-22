//! Lesson cards (PLAN.md §4.7, P6).
//!
//! > Failures are classified, distilled into short lessons, and promoted into
//! > the store only by an explicit gate. Raw traces are never memory.
//!
//! The promotion rules are the whole design. In particular rule 2 — two
//! occurrences, or an explicit human override — is what stops the store filling
//! with confident rules derived from one flaky run, which the plan names as the
//! single strongest failure mode of learning harnesses.

pub mod usage;

use crate::failure::{Class, Episode};
use crate::paths::Paths;
use crate::store::frontmatter::{self};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const LESSON_SCHEMA: &str = "keel.lesson/1";
/// Promotion rule 5: "≤ 12 lines. Long lessons are specs in disguise."
pub const MAX_BODY_LINES: usize = 12;
/// Promotion rule 2.
pub const MIN_OCCURRENCES: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleKind {
    /// Enforced by a gate check. Preferred: an enforced lesson costs no context.
    GateCheck,
    /// Injected into the prompt at the relevant stage.
    PromptInjection,
    Both,
}

impl RuleKind {
    pub fn enforces(&self) -> bool {
        matches!(self, RuleKind::GateCheck | RuleKind::Both)
    }
    pub fn injects(&self) -> bool {
        matches!(self, RuleKind::PromptInjection | RuleKind::Both)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonFront {
    pub id: String,
    #[serde(default = "default_schema")]
    pub schema: String,
    pub class: String,
    /// `repo`, `dir:src/api`, `lang:rust`.
    pub scope: String,
    pub occurrences: usize,
    pub rule_kind: RuleKind,
    pub verified_at: String,
    /// e.g. `90d`. A lesson unused for this long goes to demotion review.
    #[serde(default = "default_decay")]
    pub decay: String,
    /// Run ids this lesson was derived from — "why does this rule exist?".
    #[serde(default)]
    pub sources: Vec<String>,
    /// Stages at which this lesson is injected.
    #[serde(default = "default_stages")]
    pub stages: Vec<String>,
    #[serde(flatten)]
    pub extra: serde_yaml::Mapping,
}

fn default_schema() -> String { LESSON_SCHEMA.to_string() }
fn default_decay() -> String { "90d".to_string() }
fn default_stages() -> Vec<String> { vec!["implement".to_string()] }

#[derive(Debug, Clone)]
pub struct Lesson {
    pub path: PathBuf,
    pub front: LessonFront,
    pub body: String,
}

impl Lesson {
    pub fn read(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let (front, body) = frontmatter::split_typed(&raw)
            .with_context(|| format!("in {}", path.display()))?;
        Ok(Self { path: path.to_path_buf(), front, body })
    }

    /// The `**Rule**` line — what the lesson actually requires.
    pub fn rule(&self) -> Option<String> {
        self.field("Rule")
    }

    /// The `**Oracle**` line, if the lesson is enforceable.
    pub fn oracle(&self) -> Option<String> {
        self.field("Oracle")
    }

    pub fn trigger(&self) -> Option<String> {
        self.field("Trigger")
    }

    fn field(&self, name: &str) -> Option<String> {
        let marker = format!("**{name}**");
        self.body
            .lines()
            .find(|l| l.trim_start().starts_with(&marker))
            .map(|l| l.trim().trim_start_matches(&marker).trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub fn class(&self) -> Option<Class> {
        Class::parse(&self.front.class)
    }

    pub fn body_lines(&self) -> usize {
        self.body.lines().filter(|l| !l.trim().is_empty()).count()
    }

    /// Whether this lesson applies to a path.
    pub fn applies_to(&self, path: &str) -> bool {
        scope_matches(&self.front.scope, path)
    }

    pub fn decay_days(&self) -> u64 {
        parse_days(&self.front.decay).unwrap_or(90)
    }
}

/// `repo` matches everything; `dir:x` matches paths under x; `lang:rust`
/// matches by extension.
pub fn scope_matches(scope: &str, path: &str) -> bool {
    match scope.split_once(':') {
        None => scope == "repo",
        Some(("dir", d)) => path.starts_with(&format!("{}/", d.trim_end_matches('/'))) || path == d,
        Some(("file", f)) => path == f,
        Some(("lang", l)) => crate::map::lang::Lang::from_path(Path::new(path))
            .map(|x| x.name() == l)
            .unwrap_or(false),
        _ => false,
    }
}

/// Two scopes overlap if either could apply to the same file.
pub fn scopes_overlap(a: &str, b: &str) -> bool {
    if a == b || a == "repo" || b == "repo" {
        return true;
    }
    match (a.split_once(':'), b.split_once(':')) {
        (Some(("dir", x)), Some(("dir", y))) => x.starts_with(y) || y.starts_with(x),
        (Some(("dir", d)), Some(("file", f))) | (Some(("file", f)), Some(("dir", d))) => {
            f.starts_with(&format!("{d}/"))
        }
        _ => false,
    }
}

fn parse_days(s: &str) -> Option<u64> {
    let t = s.trim();
    let n: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    let v: u64 = n.parse().ok()?;
    Some(match t.trim_start_matches(char::is_numeric).trim() {
        "d" | "" => v,
        "w" => v * 7,
        "m" => v * 30,
        "y" => v * 365,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// candidates
// ---------------------------------------------------------------------------

/// A proposed lesson, before a human has accepted it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    /// Stable across runs: the class and scope a lesson card is keyed by.
    pub signature: String,
    /// The distinct signals that produced this candidate — the evidence.
    #[serde(default)]
    pub signals: Vec<String>,
    pub class: Class,
    pub scope: String,
    pub occurrences: usize,
    /// Distinct runs this was seen in. Occurrences within one run do not count
    /// toward promotion: ten failures in one run is one mistake, not ten.
    pub runs: Vec<String>,
    pub trigger: String,
    pub observation: String,
    pub rule: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle: Option<String>,
    /// Whether promotion rule 2 is satisfied.
    pub promotable: bool,
    pub blocked_by: Vec<String>,
}

/// Distil episodes into candidates, counting occurrences across runs.
pub fn propose(episodes: &[Episode], existing: &[Lesson]) -> Vec<Candidate> {
    // Grouped by (class, scope), which is exactly what a lesson card is keyed
    // by. Grouping by the individual failing check instead produced two
    // identical SCOPE-CREEP lessons — one from `blast-radius`, one from
    // `line-budget` — for what is plainly one mistake.
    let mut groups: BTreeMap<String, Vec<&Episode>> = BTreeMap::new();
    for e in episodes {
        // Promotion rule 1: only agentic failures. PROCESS, HUMAN and
        // UNATTRIBUTABLE are counted elsewhere and never learned from.
        if !e.attribution.is_promotable() {
            continue;
        }
        if e.class.is_none() {
            continue;
        }
        groups.entry(e.signature()).or_default().push(e);
    }

    let mut out: Vec<Candidate> = Vec::new();
    for (signature, eps) in groups {
        let first = eps[0];
        let Some(class) = first.class else { continue };

        let mut runs: Vec<String> = eps.iter().map(|e| e.run.clone()).collect();
        runs.sort();
        runs.dedup();

        let mut blocked_by = Vec::new();
        if runs.len() < MIN_OCCURRENCES {
            blocked_by.push(format!(
                "seen in {} run; promotion needs {MIN_OCCURRENCES} (or --force)",
                runs.len()
            ));
        }
        // Promotion rule 4: no contradiction with an existing lesson in an
        // overlapping scope. A duplicate is a merge, not a second card.
        for l in existing {
            if l.class() == Some(class) && scopes_overlap(&l.front.scope, &first.scope) {
                blocked_by.push(format!(
                    "{} already covers {} in {}",
                    l.front.id, class.code(), l.front.scope
                ));
            }
        }

        let mut signals: Vec<String> = eps.iter().map(|e| e.signal.key()).collect();
        signals.sort();
        signals.dedup();

        let oracle = suggest_oracle(class, &first.scope);
        out.push(Candidate {
            signals,
            promotable: blocked_by.is_empty(),
            signature,
            class,
            scope: first.scope.clone(),
            occurrences: eps.len(),
            runs,
            trigger: trigger_for(class, &first.scope),
            observation: summarise(&eps),
            rule: rule_for(class, &first.scope),
            oracle,
            blocked_by,
        });
    }
    // Most-repeated first: the strongest evidence at the top.
    out.sort_by(|a, b| b.runs.len().cmp(&a.runs.len()).then(a.signature.cmp(&b.signature)));
    out
}

fn summarise(eps: &[&Episode]) -> String {
    let mut keys: Vec<&str> = eps.iter().map(|e| e.signal.key_ref()).collect();
    keys.sort();
    keys.dedup();
    let runs = eps.iter().map(|e| &e.run).collect::<std::collections::BTreeSet<_>>().len();
    format!(
        "caught by {} across {runs} run(s); first: {}",
        keys.join(", "),
        crate::gate::truncate(&eps[0].signal.describe(), 100)
    )
}

fn trigger_for(class: Class, scope: &str) -> String {
    let where_ = match scope.split_once(':') {
        Some(("dir", d)) => format!("under `{d}/`"),
        Some(("file", f)) => format!("to `{f}`"),
        _ => "in this repository".to_string(),
    };
    match class {
        Class::ScopeCreep => format!("A change {where_} that touches files outside its declared scope."),
        Class::ConvViolation => format!("A change {where_} that breaches a house rule or lint."),
        Class::TestInvalid => format!("A change {where_} that adds a mock or weakens an assertion."),
        Class::EditCompile => format!("A change {where_} that does not build."),
        Class::EditRuntime => format!("A change {where_} whose tests do not pass."),
        Class::SpecAmbig => "A spec whose criteria are not falsifiable.".to_string(),
        Class::SpecMissing => "A spec with a criterion that has no oracle, or no task.".to_string(),
        Class::CtxStale => "Work started against a store or map older than the code.".to_string(),
        Class::LocWrong => format!("An edit {where_} to a file outside the blast radius."),
        Class::CtxDrift => "A run that contradicts a fact it established earlier.".to_string(),
    }
}

fn rule_for(class: Class, scope: &str) -> String {
    let where_ = match scope.split_once(':') {
        Some(("dir", d)) => format!(" in `{d}/`"),
        Some(("file", f)) => format!(" in `{f}`"),
        _ => String::new(),
    };
    match class {
        Class::ScopeCreep => format!("Changes{where_} MUST stay inside the scope declared in the spec; widen the scope deliberately before editing."),
        Class::ConvViolation => format!("Changes{where_} MUST leave lint and the house rules clean."),
        Class::TestInvalid => format!("Tests{where_} MUST NOT be weakened to make a change pass."),
        Class::EditCompile => format!("Changes{where_} MUST build before the run ends."),
        Class::EditRuntime => format!("Every criterion's oracle{where_} MUST pass before the run ends."),
        Class::SpecAmbig => "Every criterion MUST be in EARS form with no vague phrasing.".to_string(),
        Class::SpecMissing => "Every criterion MUST name a machine-checkable oracle and be covered by a task.".to_string(),
        Class::CtxStale => "The map and projections MUST be current before work starts.".to_string(),
        Class::LocWrong => format!("Edits{where_} MUST stay inside the computed blast radius."),
        Class::CtxDrift => "A run MUST NOT contradict a fact it established earlier.".to_string(),
    }
}

/// A runnable check for classes that admit one.
///
/// Promotion rule 3 prefers an oracle every time: a lesson that is enforced
/// does not need to be read, and so costs no context at all.
fn suggest_oracle(class: Class, _scope: &str) -> Option<String> {
    Some(match class {
        Class::ConvViolation => "cmd `cargo clippy --all-targets -- -D warnings` exit 0".to_string(),
        Class::EditCompile => "cmd `cargo build --quiet` exit 0".to_string(),
        Class::EditRuntime => "cmd `cargo test --quiet` exit 0".to_string(),
        // ScopeCreep, TestInvalid and the spec classes are already enforced by
        // G0/G2/G2.5 directly; a lesson-level oracle would duplicate the gate.
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// the store
// ---------------------------------------------------------------------------

pub fn list(paths: &Paths) -> Result<Vec<Lesson>> {
    let dir = paths.lessons();
    if !dir.is_dir() {
        return Ok(vec![]);
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().and_then(|e| e.to_str()) == Some("md")
                && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("L-"))
        })
        .collect();
    files.sort();
    files.iter().map(|p| Lesson::read(p)).collect()
}

fn next_id(paths: &Paths) -> Result<String> {
    let mut max = 0usize;
    for l in list(paths)? {
        if let Some(n) = l.front.id.rsplit('-').next().and_then(|d| d.parse::<usize>().ok()) {
            max = max.max(n);
        }
    }
    Ok(format!("L-{:04}", max + 1))
}

/// Promote a candidate into a lesson card, enforcing the promotion rules.
pub fn promote(paths: &Paths, candidate: &Candidate, force: bool) -> Result<Lesson> {
    if !candidate.promotable && !force {
        bail!(
            "not promotable: {} — pass --force to override deliberately",
            candidate.blocked_by.join("; ")
        );
    }
    let id = next_id(paths)?;
    // Promotion rule 3: "A lesson with an oracle becomes a gate check and stops
    // being a prompt." Not `Both` — the whole payoff of enforcing a rule is that
    // it no longer has to be read, and injecting it anyway would spend context
    // restating something that cannot be violated without failing G2.
    let rule_kind = if candidate.oracle.is_some() {
        RuleKind::GateCheck
    } else {
        RuleKind::PromptInjection
    };

    let front = LessonFront {
        id: id.clone(),
        schema: LESSON_SCHEMA.to_string(),
        class: candidate.class.code().to_string(),
        scope: candidate.scope.clone(),
        occurrences: candidate.runs.len(),
        rule_kind,
        verified_at: crate::store::today(),
        decay: default_decay(),
        sources: candidate.runs.iter().map(|r| format!("runs/{r}")).collect(),
        stages: default_stages(),
        extra: Default::default(),
    };

    let mut body = String::new();
    body.push_str(&format!("**Trigger** {}\n\n", candidate.trigger));
    body.push_str(&format!("**Observation** {}\n\n", candidate.observation));
    body.push_str(&format!("**Rule** {}\n", candidate.rule));
    if let Some(o) = &candidate.oracle {
        body.push_str(&format!("\n**Oracle** {o}\n"));
    }

    let lesson = Lesson { path: paths.lessons().join(format!("{id}.md")), front, body };
    if lesson.body_lines() > MAX_BODY_LINES {
        bail!(
            "lesson body is {} lines; the limit is {MAX_BODY_LINES} (long lessons are specs in disguise)",
            lesson.body_lines()
        );
    }
    write(&lesson)?;
    Ok(lesson)
}

pub fn write(lesson: &Lesson) -> Result<()> {
    if let Some(p) = lesson.path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(&lesson.path, frontmatter::join_typed(&lesson.front, &lesson.body)?)
        .with_context(|| format!("writing {}", lesson.path.display()))?;
    Ok(())
}

/// Move a lesson out of force, keeping it as a record of what was once true.
pub fn demote(paths: &Paths, id: &str, reason: &str) -> Result<PathBuf> {
    let lesson = list(paths)?
        .into_iter()
        .find(|l| l.front.id == id)
        .ok_or_else(|| anyhow::anyhow!("no lesson `{id}`"))?;

    let archive = paths.lessons().join("demoted");
    std::fs::create_dir_all(&archive)?;
    let dest = archive.join(format!("{id}.md"));

    // Keep the reason and the date with the card: a demoted lesson that does
    // not say why it was demoted will be re-promoted by the next person.
    let mut front = lesson.front.clone();
    front.extra.insert(
        serde_yaml::Value::String("demoted_at".into()),
        serde_yaml::Value::String(crate::store::today()),
    );
    front.extra.insert(
        serde_yaml::Value::String("demoted_because".into()),
        serde_yaml::Value::String(reason.to_string()),
    );
    std::fs::write(&dest, frontmatter::join_typed(&front, &lesson.body)?)?;
    std::fs::remove_file(&lesson.path)?;
    Ok(dest)
}

/// Lessons that apply at a stage, for the given paths.
///
/// Injection is keel's job, not the agent's: documentation was the first
/// recovery move in only 5.4% of observed failure episodes, so a lesson left on
/// a shelf is a lesson nobody reads.
pub fn for_injection<'a>(lessons: &'a [Lesson], stage: &str, scope_paths: &[String]) -> Vec<&'a Lesson> {
    lessons
        .iter()
        .filter(|l| l.front.rule_kind.injects())
        .filter(|l| l.front.stages.iter().any(|s| s == stage))
        .filter(|l| {
            l.front.scope == "repo"
                || scope_paths.is_empty()
                || scope_paths.iter().any(|p| l.applies_to(p) || scopes_overlap(&l.front.scope, &format!("dir:{}", dir_of(p))))
        })
        .collect()
}

fn dir_of(p: &str) -> &str {
    p.trim_end_matches("/**").rsplit_once('/').map(|(d, _)| d).unwrap_or(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::failure::{Attribution, Signal};
    use crate::gate::Verdict;

    fn episode(run: &str, class: Class, scope: &str) -> Episode {
        Episode {
            id: format!("{run}#1"),
            run: run.into(),
            spec: "demo".into(),
            seq: None,
            signal: Signal::GateCheck {
                gate: "G2".into(), check: "blast-radius".into(), verdict: Verdict::Fail,
                expected: Some("in scope".into()), actual: Some("src/api/mod.rs".into()),
            },
            attribution: Attribution::Agentic,
            class: Some(class),
            rationale: String::new(),
            scope: scope.into(),
            recovery: None,
            evidence: None,
        }
    }

    #[test]
    fn one_occurrence_is_not_promotable() {
        let c = propose(&[episode("r1", Class::ScopeCreep, "dir:src/api")], &[]);
        assert_eq!(c.len(), 1);
        assert!(!c[0].promotable, "a single run produced a promotable lesson");
        assert!(c[0].blocked_by[0].contains("needs 2"), "{:?}", c[0].blocked_by);
    }

    #[test]
    fn two_checks_catching_one_mistake_make_one_lesson() {
        // `blast-radius` and `line-budget` both catch scope creep. They are one
        // lesson, not two identical ones.
        let mut budget = episode("r1", Class::ScopeCreep, "dir:src/api");
        budget.signal = Signal::GateCheck {
            gate: "G2".into(), check: "line-budget".into(), verdict: Verdict::Fail,
            expected: None, actual: None,
        };
        let c = propose(&[episode("r1", Class::ScopeCreep, "dir:src/api"), budget], &[]);
        assert_eq!(c.len(), 1, "one mistake produced {} candidates", c.len());
        assert_eq!(c[0].signals.len(), 2, "the evidence from both checks was lost");
    }

    #[test]
    fn repeats_within_one_run_do_not_count_as_occurrences() {
        // Ten failures in one run is one mistake, not ten.
        let eps: Vec<Episode> = (0..10).map(|_| episode("r1", Class::ScopeCreep, "dir:src/api")).collect();
        let c = propose(&eps, &[]);
        assert_eq!(c[0].occurrences, 10);
        assert_eq!(c[0].runs.len(), 1);
        assert!(!c[0].promotable, "ten failures in one run promoted a lesson");
    }

    #[test]
    fn a_second_run_makes_it_promotable() {
        let c = propose(
            &[
                episode("r1", Class::ScopeCreep, "dir:src/api"),
                episode("r2", Class::ScopeCreep, "dir:src/api"),
            ],
            &[],
        );
        assert!(c[0].promotable, "{:?}", c[0].blocked_by);
        assert_eq!(c[0].runs, vec!["r1", "r2"]);
    }

    #[test]
    fn non_agentic_episodes_never_become_candidates() {
        let mut process = episode("r1", Class::ScopeCreep, "repo");
        process.attribution = Attribution::Process;
        let mut unattributable = episode("r2", Class::ScopeCreep, "repo");
        unattributable.attribution = Attribution::Unattributable;
        assert!(propose(&[process, unattributable], &[]).is_empty());
    }

    #[test]
    fn an_existing_lesson_in_an_overlapping_scope_blocks_a_duplicate() {
        let existing = Lesson {
            path: PathBuf::from("L-0001.md"),
            front: LessonFront {
                id: "L-0001".into(), schema: LESSON_SCHEMA.into(),
                class: "SCOPE-CREEP".into(), scope: "dir:src".into(), occurrences: 2,
                rule_kind: RuleKind::PromptInjection, verified_at: "2026-08-21".into(),
                decay: "90d".into(), sources: vec![], stages: default_stages(),
                extra: Default::default(),
            },
            body: "**Rule** stay in scope\n".into(),
        };
        let c = propose(
            &[
                episode("r1", Class::ScopeCreep, "dir:src/api"),
                episode("r2", Class::ScopeCreep, "dir:src/api"),
            ],
            &[existing],
        );
        assert!(!c[0].promotable);
        assert!(c[0].blocked_by[0].contains("L-0001"), "{:?}", c[0].blocked_by);
    }

    #[test]
    fn classes_that_gates_already_enforce_get_no_duplicate_oracle() {
        assert!(suggest_oracle(Class::ScopeCreep, "repo").is_none());
        assert!(suggest_oracle(Class::ConvViolation, "repo").is_some());
    }

    #[test]
    fn scope_matching_covers_the_three_forms() {
        assert!(scope_matches("repo", "anything/at/all.rs"));
        assert!(scope_matches("dir:src/api", "src/api/mod.rs"));
        assert!(!scope_matches("dir:src/api", "src/core.rs"));
        assert!(scope_matches("file:src/main.rs", "src/main.rs"));
        assert!(scope_matches("lang:rust", "src/main.rs"));
        assert!(!scope_matches("lang:rust", "web/app.ts"));
    }

    #[test]
    fn overlapping_scopes_are_detected_in_both_directions() {
        assert!(scopes_overlap("repo", "dir:src/api"));
        assert!(scopes_overlap("dir:src", "dir:src/api"));
        assert!(scopes_overlap("dir:src/api", "dir:src"));
        assert!(!scopes_overlap("dir:src/api", "dir:web/app"));
        assert!(scopes_overlap("dir:src", "file:src/main.rs"));
    }

    #[test]
    fn decay_periods_parse() {
        assert_eq!(parse_days("90d"), Some(90));
        assert_eq!(parse_days("2w"), Some(14));
        assert_eq!(parse_days("6m"), Some(180));
        assert_eq!(parse_days("1y"), Some(365));
        assert_eq!(parse_days("soon"), None);
    }

    #[test]
    fn an_enforced_lesson_is_not_also_injected() {
        assert!(RuleKind::GateCheck.enforces());
        assert!(!RuleKind::GateCheck.injects(), "an enforced lesson must not also cost context");
    }

    #[test]
    fn rule_kinds_say_how_a_lesson_acts() {
        assert!(RuleKind::GateCheck.enforces() && !RuleKind::GateCheck.injects());
        assert!(RuleKind::PromptInjection.injects() && !RuleKind::PromptInjection.enforces());
        assert!(RuleKind::Both.enforces() && RuleKind::Both.injects());
    }
}
