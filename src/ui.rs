//! Terminal presentation: colour and glyphs, or nothing at all.
//!
//! keel's output is read two ways — by a person watching a gate run, and by a
//! pipe. The second is not a degraded case of the first: `keel status | head`
//! is a supported thing to do, `restore_sigpipe` exists for it, and the test
//! suite reads stdout through a pipe. So styling is decided per write against
//! the real stdout handle, and when stdout is not a terminal every helper here
//! emits exactly the bytes it emitted before this module existed.
//!
//! The palette and glyphs are `release.sh`'s, so the release script and the
//! binary read as one product rather than two tools that ship together.

use std::io::IsTerminal;
use std::sync::OnceLock;

const RED: &str = "\x1b[0;31m";
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[0;33m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const OFF: &str = "\x1b[0m";

/// Whether to emit escape sequences on stdout.
///
/// `NO_COLOR` and `TERM=dumb` are honoured because they are the conventions
/// people actually set; `CLICOLOR_FORCE` is honoured so the styled output can
/// be demonstrated, recorded or piped into a pager on purpose.
pub fn styled() -> bool {
    static DECIDED: OnceLock<bool> = OnceLock::new();
    *DECIDED.get_or_init(|| {
        if std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| v != "0") {
            return true;
        }
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        if std::env::var("TERM").is_ok_and(|t| t == "dumb") {
            return false;
        }
        std::io::stdout().is_terminal()
    })
}

fn paint(colour: &str, text: &str) -> String {
    if styled() {
        format!("{colour}{text}{OFF}")
    } else {
        text.to_string()
    }
}

pub fn red(text: &str) -> String {
    paint(RED, text)
}

pub fn green(text: &str) -> String {
    paint(GREEN, text)
}

pub fn yellow(text: &str) -> String {
    paint(YELLOW, text)
}

pub fn dim(text: &str) -> String {
    paint(DIM, text)
}

pub fn bold(text: &str) -> String {
    paint(BOLD, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The suite runs through a pipe, so this is the shape every existing
    /// golden assertion depends on.
    #[test]
    fn a_pipe_gets_no_escape_sequences() {
        assert!(!styled(), "styling was enabled for a non-terminal stdout");
        for painted in [red("x"), green("x"), yellow("x"), dim("x"), bold("x")] {
            assert_eq!(painted, "x", "an escape sequence reached a pipe");
        }
    }
}
