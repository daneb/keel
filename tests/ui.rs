//! The terminal presentation layer must be invisible to everything but a
//! terminal.
//!
//! keel is piped (`keel status | head`), redirected into evidence files, and
//! read by this test suite through `Command::output()`. An escape sequence that
//! reaches any of those is corruption, not decoration — so the interesting
//! property is not that colour works, it is that colour stays off by default
//! and can still be turned on deliberately.

mod support;

use std::process::Command;
use support::{BIN, Repo};

const ESC: char = '\x1b';

fn keel_with_env(r: &Repo, args: &[&str], env: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(BIN);
    cmd.args(args).current_dir(&r.dir);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("running keel");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The commands whose output carries a verdict, a state word or a step glyph —
/// everything the presentation layer actually touches.
fn styled_commands() -> Vec<Vec<&'static str>> {
    vec![
        vec!["gate", "g0", "demo"],
        vec!["next", "demo"],
        vec!["next"],
        vec!["status"],
        vec!["store", "check"],
        vec!["spec", "list"],
        vec!["doctor"],
    ]
}

#[test]
fn a_pipe_never_receives_an_escape_sequence() {
    let r = Repo::bare("ui-pipe");
    r.write_spec();

    for args in styled_commands() {
        let (_, out) = r.run(&args);
        assert!(
            !out.contains(ESC),
            "keel {args:?} wrote an escape sequence to a pipe:\n{out:?}"
        );
    }
}

#[test]
fn no_color_is_honoured_even_when_colour_is_forced_off_by_nothing_else() {
    let r = Repo::bare("ui-nocolor");
    r.write_spec();
    let out = keel_with_env(&r, &["gate", "g0", "demo"], &[("NO_COLOR", "1")]);
    assert!(!out.contains(ESC), "NO_COLOR was ignored:\n{out:?}");
}

#[test]
fn clicolor_force_turns_styling_on_through_a_pipe() {
    let r = Repo::bare("ui-forced");
    r.write_spec();
    let out = keel_with_env(&r, &["gate", "g0", "demo"], &[("CLICOLOR_FORCE", "1")]);
    assert!(out.contains(ESC), "CLICOLOR_FORCE produced no styling:\n{out:?}");
    // Green for the passing checks, and the reset that closes every span.
    assert!(out.contains("\x1b[0;32m"), "no pass colour in:\n{out:?}");
    assert!(out.contains("\x1b[0m"), "an unterminated colour span:\n{out:?}");
}

/// Asking for colour explicitly outranks the ambient "no colour" preference.
/// Both are honoured; this pins which wins when a user sets both.
#[test]
fn clicolor_force_takes_precedence_over_no_color() {
    let r = Repo::bare("ui-precedence");
    r.write_spec();
    let out = keel_with_env(
        &r,
        &["gate", "g0", "demo"],
        &[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "1")],
    );
    assert!(
        out.contains(ESC),
        "CLICOLOR_FORCE is checked first, so it wins. If that is now the wrong\n\
         call, change `ui::styled` and this test together.\n{out:?}"
    );
}

#[test]
fn styling_does_not_move_the_columns() {
    let r = Repo::bare("ui-columns");
    r.write_spec();

    // `store check` renders the same padded state column as a gate, without a
    // per-run identifier that would differ between the two invocations.
    let plain = r.run(&["store", "check"]).1;
    let forced = keel_with_env(&r, &["store", "check"], &[("CLICOLOR_FORCE", "1")]);
    assert!(forced.contains(ESC), "nothing was styled, so nothing is proven");

    // Strip every escape sequence back out; what remains must be what a pipe
    // saw, character for character — including the padding before each id.
    let stripped: String = {
        let mut out = String::new();
        let mut chars = forced.chars();
        while let Some(c) = chars.next() {
            if c == ESC {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    };
    assert_eq!(stripped, plain, "colour changed the layout, not just the colour");
}
