//! Reference driver scripts, embedded at compile time.
//!
//! Before this, the scripts lived only in this repository's own `.keel/drivers/`
//! — hand-authored for keel's self-hosted use, and excluded from the published
//! crate by `Cargo.toml`'s `exclude = [".keel/"]`. `keel init` never wrote
//! them anywhere else, so `cargo install keel-harness` followed by `keel init`
//! produced a config pointing at `.keel/drivers/claude-code` — the *default*
//! driver — with no such file on disk, on every machine that was not this
//! repository. Found by a user on a fresh install who wanted `kiro`.
//!
//! `assets/drivers/*` is the single source of truth now; this repository's own
//! `.keel/drivers/` is generated from it like any other user's, by the same
//! `keel driver scaffold`.

/// One reference script and, where it names a runnable driver, the `[[driver]]`
/// entry `scaffold` adds for it.
pub struct Builtin {
    /// Filename under `.keel/drivers/`.
    pub filename: &'static str,
    pub content: &'static str,
    /// `None` for `_common.sh`, sourced by the others but not itself a driver.
    pub driver_id: Option<&'static str>,
    pub default: bool,
    pub timeout_secs: u64,
}

pub const ALL: &[Builtin] = &[
    Builtin {
        filename: "_common.sh",
        content: include_str!("../../assets/drivers/_common.sh"),
        driver_id: None,
        default: false,
        timeout_secs: 0,
    },
    Builtin {
        filename: "claude-code",
        content: include_str!("../../assets/drivers/claude-code"),
        driver_id: Some("claude-code"),
        default: true,
        timeout_secs: 900,
    },
    Builtin {
        filename: "codex",
        content: include_str!("../../assets/drivers/codex"),
        driver_id: Some("codex"),
        default: false,
        timeout_secs: 900,
    },
    Builtin {
        filename: "copilot",
        content: include_str!("../../assets/drivers/copilot"),
        driver_id: Some("copilot"),
        default: false,
        timeout_secs: 900,
    },
    Builtin {
        filename: "kiro",
        content: include_str!("../../assets/drivers/kiro"),
        driver_id: Some("kiro"),
        default: false,
        timeout_secs: 900,
    },
    Builtin {
        filename: "noop",
        content: include_str!("../../assets/drivers/noop"),
        driver_id: Some("noop"),
        default: false,
        timeout_secs: 30,
    },
];

/// Assets that are not drivers but ship the same way: written into `.keel/` by
/// `keel init`, embedded so they exist on a fresh install rather than only in
/// this repository.
pub struct Asset {
    /// Path under `.keel/`, e.g. `sast/semgrep`.
    pub rel: &'static str,
    pub content: &'static str,
}

pub const ASSETS: &[Asset] = &[Asset {
    rel: "sast/semgrep",
    content: include_str!("../../assets/sast/semgrep"),
}];
