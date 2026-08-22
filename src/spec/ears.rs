//! EARS conformance and the ambiguity scan (PLAN.md P3, G0).
//!
//! EARS (Easy Approach to Requirements Syntax) constrains a requirement to one
//! of a handful of shapes. The value here is not the grammar for its own sake —
//! it is that a sentence which cannot be forced into one of these shapes is
//! almost always a sentence that has not decided what it means.

/// The EARS pattern a criterion statement matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    /// `THE SYSTEM SHALL <response>`
    Ubiquitous,
    /// `WHEN <trigger> THE SYSTEM SHALL <response>`
    EventDriven,
    /// `WHILE <state> THE SYSTEM SHALL <response>`
    StateDriven,
    /// `IF <condition> THEN THE SYSTEM SHALL <response>`
    Unwanted,
    /// `WHERE <feature> THE SYSTEM SHALL <response>`
    Optional,
    /// Two or more preconditions before the response.
    Complex,
}

impl Pattern {
    pub fn name(&self) -> &'static str {
        match self {
            Pattern::Ubiquitous => "ubiquitous",
            Pattern::EventDriven => "event-driven",
            Pattern::StateDriven => "state-driven",
            Pattern::Unwanted => "unwanted-behaviour",
            Pattern::Optional => "optional-feature",
            Pattern::Complex => "complex",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Conformance {
    Ok(Pattern),
    Bad(String),
}

const SHALL: &str = "THE SYSTEM SHALL";

/// Classify a criterion statement, or say precisely what is missing.
///
/// The check is deliberately case-sensitive on the keywords. Shouting `SHALL`
/// is not decoration: it is the visible difference between a requirement and a
/// description, and lowercasing it is how "should" creeps back in.
pub fn classify(statement: &str) -> Conformance {
    let s = normalise(statement);

    let Some(shall_at) = s.find(SHALL) else {
        if s.to_uppercase().contains("THE SYSTEM SHALL") {
            return Conformance::Bad(
                "`THE SYSTEM SHALL` must be upper case — lower case reads as a description, not a requirement".into(),
            );
        }
        if s.to_uppercase().contains("SHOULD") {
            return Conformance::Bad(
                "uses SHOULD; EARS criteria state obligations with `THE SYSTEM SHALL`".into(),
            );
        }
        return Conformance::Bad("no `THE SYSTEM SHALL` clause".into());
    };

    let response = s[shall_at + SHALL.len()..].trim();
    if response.is_empty() {
        return Conformance::Bad("nothing follows `THE SYSTEM SHALL`".into());
    }

    let prefix = s[..shall_at].trim();
    if prefix.is_empty() {
        return Conformance::Ok(Pattern::Ubiquitous);
    }

    let starts = |kw: &str| prefix.starts_with(&format!("{kw} "));
    let keyword_count = ["WHEN ", "WHILE ", "IF ", "WHERE "]
        .iter()
        .filter(|kw| prefix.contains(*kw) || prefix.starts_with(kw.trim_end()))
        .count();

    if starts("IF") {
        if !prefix.contains("THEN") {
            return Conformance::Bad("`IF` requires a `THEN` before `THE SYSTEM SHALL`".into());
        }
        return Conformance::Ok(if keyword_count > 1 { Pattern::Complex } else { Pattern::Unwanted });
    }
    if keyword_count > 1 {
        return Conformance::Ok(Pattern::Complex);
    }
    if starts("WHEN") {
        return Conformance::Ok(Pattern::EventDriven);
    }
    if starts("WHILE") {
        return Conformance::Ok(Pattern::StateDriven);
    }
    if starts("WHERE") {
        return Conformance::Ok(Pattern::Optional);
    }
    Conformance::Bad(format!(
        "text before `THE SYSTEM SHALL` must start with WHEN, WHILE, IF…THEN or WHERE (found: `{}`)",
        truncate(prefix, 40)
    ))
}

/// Phrases that let a criterion look decided without being decided.
///
/// Each entry has cost a real project a real argument. `handle` and `support`
/// are the worst offenders because they read like verbs while naming no
/// observable behaviour.
const AMBIGUOUS: &[&str] = &[
    "appropriate", "appropriately", "as needed", "as necessary", "as required",
    "correctly", "efficient", "efficiently", "etc.", "etc ", "and so on",
    "fast", "flexible", "gracefully", "if possible", "intuitive",
    "properly", "reasonable", "reasonably", "robust", "scalable", "seamless",
    "sensible", "simple", "smooth", "sufficient", "suitable", "user-friendly",
    "where appropriate", "and/or", "tbd", "tbc", "to be decided",
    "some", "several", "various", "many", "few", "most",
    "quickly", "slowly", "large", "small", "minimal", "maximal", "optimal",
    "better", "improved", "enhanced", "clean", "nice", "good",
    "handle", "handles", "support", "supports", "manage", "manages",
    "may", "might", "could", "should", "possibly", "ideally", "generally",
    "usually", "typically", "normally", "roughly", "approximately", "about",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ambiguity {
    pub term: String,
    /// The words either side, so the report can be argued with.
    pub context: String,
}

/// Find ambiguous phrasing in a criterion statement.
///
/// Matching is on whole words so that `manages` does not fire on `management`
/// and `about` does not fire on a URL. Text inside backticks is exempt: a
/// criterion may legitimately name a function called `handle_request`.
pub fn ambiguities(statement: &str) -> Vec<Ambiguity> {
    let stripped = strip_code_spans(statement);
    let lower = stripped.to_lowercase();
    let mut found: Vec<Ambiguity> = Vec::new();
    for term in AMBIGUOUS {
        let term = term.trim_end();
        let mut from = 0usize;
        while let Some(rel) = lower[from..].find(term) {
            let at = from + rel;
            from = at + term.len();
            if !is_whole_word(&lower, at, term.len()) {
                continue;
            }
            let ctx_start = lower[..at].rfind(' ').map(|i| i + 1).unwrap_or(0);
            let ctx_end = lower[at + term.len()..]
                .find(' ')
                .map(|i| at + term.len() + i)
                .unwrap_or(lower.len());
            let already = found.iter().any(|a| a.term == term);
            if !already {
                found.push(Ambiguity {
                    term: term.to_string(),
                    context: truncate(stripped[ctx_start..ctx_end].trim(), 60),
                });
            }
        }
    }
    found
}

fn is_whole_word(s: &str, at: usize, len: usize) -> bool {
    let before = s[..at].chars().next_back();
    let after = s[at + len..].chars().next();
    let boundary = |c: Option<char>| match c {
        None => true,
        Some(c) => !c.is_alphanumeric() && c != '_' && c != '-',
    };
    boundary(before) && boundary(after)
}

/// Remove `` `code spans` `` — identifiers are not prose and must not be
/// scanned as prose.
fn strip_code_spans(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_code = false;
    for c in s.chars() {
        if c == '`' {
            in_code = !in_code;
            out.push(' ');
            continue;
        }
        out.push(if in_code { ' ' } else { c });
    }
    out
}

fn normalise(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        let space = c.is_whitespace();
        if space {
            if !prev_space { out.push(' '); }
        } else {
            out.push(c);
        }
        prev_space = space;
    }
    out.trim().to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { return s.to_string(); }
    s.chars().take(max.saturating_sub(1)).chain(['…']).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(s: &str) -> Pattern {
        match classify(s) {
            Conformance::Ok(p) => p,
            Conformance::Bad(why) => panic!("`{s}` rejected: {why}"),
        }
    }

    fn rejection(s: &str) -> String {
        match classify(s) {
            Conformance::Ok(p) => panic!("`{s}` wrongly accepted as {}", p.name()),
            Conformance::Bad(why) => why,
        }
    }

    #[test]
    fn accepts_the_five_ears_patterns() {
        assert_eq!(pattern("THE SYSTEM SHALL log every gate verdict."), Pattern::Ubiquitous);
        assert_eq!(
            pattern("WHEN a projection is hand-edited THE SYSTEM SHALL report drift."),
            Pattern::EventDriven
        );
        assert_eq!(
            pattern("WHILE the index is absent THE SYSTEM SHALL fall back to ripgrep."),
            Pattern::StateDriven
        );
        assert_eq!(
            pattern("IF a criterion has no oracle THEN THE SYSTEM SHALL fail G0."),
            Pattern::Unwanted
        );
        assert_eq!(
            pattern("WHERE the kiro adapter is enabled THE SYSTEM SHALL write .kiro/steering/keel.md."),
            Pattern::Optional
        );
    }

    #[test]
    fn recognises_complex_criteria() {
        assert_eq!(
            pattern("WHILE a run is active WHEN a gate fails THE SYSTEM SHALL stop the run."),
            Pattern::Complex
        );
    }

    #[test]
    fn rejects_prose_that_only_looks_like_a_requirement() {
        assert!(rejection("The system should handle errors.").contains("SHOULD"));
        assert!(rejection("the system shall do the thing").contains("upper case"));
        assert!(rejection("Rate limiting is added to the API.").contains("no `THE SYSTEM SHALL`"));
        assert!(rejection("THE SYSTEM SHALL").contains("nothing follows"));
        assert!(rejection("IF the limit is exceeded THE SYSTEM SHALL reject.").contains("THEN"));
        assert!(rejection("After midnight THE SYSTEM SHALL rotate logs.").contains("WHEN"));
    }

    #[test]
    fn multiline_statements_are_normalised() {
        assert_eq!(
            pattern("WHEN a client exceeds the limit\n  THE SYSTEM SHALL respond with 429."),
            Pattern::EventDriven
        );
    }

    #[test]
    fn ambiguity_scan_finds_weasel_words() {
        let a = ambiguities("THE SYSTEM SHALL handle errors appropriately and be reasonably fast.");
        let terms: Vec<&str> = a.iter().map(|x| x.term.as_str()).collect();
        assert!(terms.contains(&"handle"), "{terms:?}");
        assert!(terms.contains(&"appropriately"), "{terms:?}");
        assert!(terms.contains(&"fast"), "{terms:?}");
    }

    #[test]
    fn ambiguity_scan_is_clean_on_a_precise_criterion() {
        let a = ambiguities(
            "WHEN a client exceeds 100 requests per minute THE SYSTEM SHALL respond with HTTP 429.",
        );
        assert!(a.is_empty(), "false positives: {a:?}");
    }

    #[test]
    fn identifiers_in_backticks_are_not_prose() {
        let a = ambiguities("WHEN `handle_request` returns Err THE SYSTEM SHALL emit exit code 1.");
        assert!(a.is_empty(), "backticked identifier flagged: {a:?}");
    }

    #[test]
    fn whole_word_matching_avoids_false_positives() {
        // "management" contains "manage"; "somewhere" contains "some".
        let a = ambiguities("THE SYSTEM SHALL write the management record somewhere-fixed.");
        let terms: Vec<&str> = a.iter().map(|x| x.term.as_str()).collect();
        assert!(!terms.contains(&"manage"), "{terms:?}");
        assert!(!terms.contains(&"some"), "{terms:?}");
    }

    #[test]
    fn each_ambiguous_term_is_reported_once() {
        let a = ambiguities("THE SYSTEM SHALL handle this and handle that.");
        assert_eq!(a.iter().filter(|x| x.term == "handle").count(), 1);
    }
}
