//! Detecting scaffold text that was never filled in.
//!
//! This exists because of a real failure: keel's own `spec new` template shipped
//! `WHEN <trigger> THE SYSTEM SHALL <observable response>` with
//! ``oracle: cmd `<command that proves it>` exit 0``, and G0 passed it. Every
//! individual check was satisfied — the sentence *was* in EARS form, an oracle
//! *was* present — and the spec still said nothing.
//!
//! That is precisely the "gate theatre" failure in PLAN.md §6: a gate that
//! cannot fail on the most common input is documentation. A template must be
//! rejected by the gate it is a template for.

/// Placeholder markers found in prose, in order.
///
/// Text inside backticks is exempt, the same rule the ambiguity scan uses: a
/// criterion may legitimately name `.keel/runs/<id>/trajectory.jsonl`. Bare
/// `<id>` in prose is indistinguishable from bare `<trigger>`, so the gate errs
/// toward flagging and backticks are the author's escape hatch. A false
/// positive costs one pair of backticks; a false negative lets a template
/// through, which is the bug this module exists to fix.
pub fn scan(text: &str) -> Vec<String> {
    let text = &strip_code_spans(text);
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if !out.contains(&s) {
            out.push(s);
        }
    };

    // `<angle bracket placeholders>` — the dominant template idiom. Only prose
    // counts: `<(subshell)`, `2>&1` and `x < 5` must not fire.
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '<'
            && let Some(close) = chars[i + 1..].iter().position(|c| *c == '>')
        {
            let inner: String = chars[i + 1..i + 1 + close].iter().collect();
            if is_prose_placeholder(&inner) {
                push(format!("<{inner}>"));
            }
            i += close + 2;
            continue;
        }
        i += 1;
    }

    // `_italic scaffold prose_`, as emitted by keel's own templates.
    for token in split_italics(text) {
        push(format!("_{token}_"));
    }

    for marker in ["TODO", "FIXME", "TBD", "XXX", "???"] {
        if text.contains(marker) {
            push(marker.to_string());
        }
    }
    out
}

pub fn has_placeholder(text: &str) -> bool {
    !scan(text).is_empty()
}

/// Whole-string placeholders that are not bracketed at all, e.g. a `rollback:`
/// field left as the template's italic sentence.
pub fn is_placeholder_value(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || has_placeholder(t)
}

/// Blank out `` `code spans` `` so identifiers and paths are not read as prose.
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

fn is_prose_placeholder(inner: &str) -> bool {
    !inner.is_empty()
        && inner.len() <= 60
        && inner
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == ' ' || c == '-' || c == '_')
}

/// Find `_…_` italic spans, using markdown's own delimiter rules.
///
/// The naive version — "an underscore, then the next underscore" — reported a
/// placeholder in the sentence `appends driver_call and driver_result events`,
/// because the underscores of two snake_case identifiers form a span between
/// them. A gate that rejects a correct task for containing two identifiers is
/// worse than no gate: it teaches you to stop reading the failures.
///
/// So an opening `_` must be preceded by start-of-text or whitespace, and a
/// closing `_` must be followed by end-of-text, whitespace or punctuation.
/// Inside `driver_call`, the underscore is preceded by `r` and so opens nothing.
fn split_italics(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let opens = |i: usize| -> bool {
        (i == 0 || chars[i - 1].is_whitespace() || "([{\"'".contains(chars[i - 1]))
            && chars.get(i + 1).is_some_and(|c| !c.is_whitespace())
    };
    let closes = |i: usize| -> bool {
        (i + 1 == chars.len()
            || chars[i + 1].is_whitespace()
            || ".,;:!?)]}\"'".contains(chars[i + 1]))
            && i > 0
            && !chars[i - 1].is_whitespace()
    };

    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '_' && opens(i) {
            let mut j = i + 1;
            while j < chars.len() {
                if chars[j] == '_' && closes(j) {
                    let inner: String = chars[i + 1..j].iter().collect();
                    if !inner.is_empty() && inner.len() <= 120 {
                        out.push(inner);
                    }
                    i = j + 1;
                    break;
                }
                j += 1;
            }
            if j >= chars.len() { break; }
            continue;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catches_the_template_that_shipped() {
        assert!(has_placeholder("WHEN <trigger> THE SYSTEM SHALL <observable response>."));
        // The raw oracle line hides its placeholder in backticks; G0 scans the
        // parsed payload instead, which is where the scaffold actually is.
        assert!(has_placeholder("<command that proves it>"));
        assert!(has_placeholder("_name the files this task touches_"));
        assert!(has_placeholder("TODO: decide"));
    }

    #[test]
    fn reports_what_it_found() {
        let found = scan("WHEN <trigger> THE SYSTEM SHALL <observable response>.");
        assert_eq!(found, vec!["<trigger>".to_string(), "<observable response>".to_string()]);
    }

    #[test]
    fn a_real_criterion_is_clean() {
        assert!(!has_placeholder(
            "WHEN a client exceeds 100 requests per minute THE SYSTEM SHALL respond with HTTP 429."
        ));
        assert!(
            !has_placeholder("THE SYSTEM SHALL write `.keel/runs/<id>/trajectory.jsonl`"),
            "a backticked path is an identifier, not scaffold"
        );
        assert!(
            has_placeholder("THE SYSTEM SHALL write .keel/runs/<id>/trajectory.jsonl"),
            "unbackticked <id> is indistinguishable from scaffold and must be flagged"
        );
    }

    #[test]
    fn oracle_payloads_are_scanned_after_unquoting() {
        // What G0 actually scans is the parsed payload, which has no backticks
        // left to hide behind.
        assert!(has_placeholder("<command that proves it>"));
        assert!(!has_placeholder("cargo test --test trajectory"));
    }

    #[test]
    fn shell_syntax_is_not_a_placeholder() {
        for cmd in [
            "cargo test 2>&1",
            "diff <(keel map --json) expected.json",
            "sh -c 'test $x -lt 5'",
            "grep -c . < input.txt",
        ] {
            assert!(!has_placeholder(cmd), "flagged shell syntax: {cmd}");
        }
    }

    #[test]
    fn snake_case_identifiers_are_not_italics() {
        assert!(!has_placeholder("THE SYSTEM SHALL call write_all_records once."));
        assert!(!has_placeholder("`__init__.py` is indexed"));
    }

    #[test]
    fn two_snake_case_identifiers_do_not_form_a_span() {
        // The exact sentence that produced a false G1 failure.
        assert!(
            !has_placeholder("a claude-code round trip appends driver_call and driver_result events"),
            "two identifiers were read as an italic placeholder"
        );
        assert!(!has_placeholder("emit inject_event then gate_event"));
    }

    #[test]
    fn real_italic_scaffold_is_still_caught() {
        assert!(has_placeholder("- files: _name the files this task touches_"));
        assert!(has_placeholder("_the condition under which this task is done_"));
        assert!(has_placeholder("see _the design note_, then proceed"));
    }

    #[test]
    fn html_like_tags_are_not_prose_placeholders() {
        // A tag with attributes or punctuation is not scaffold prose.
        assert!(!has_placeholder("emit <a href=\"x\">"));
    }

    #[test]
    fn empty_values_count_as_unfilled() {
        assert!(is_placeholder_value("   "));
        assert!(!is_placeholder_value("git revert the merge commit"));
    }
}
