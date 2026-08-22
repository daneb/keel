//! The failure taxonomy (PLAN.md §4.6).
//!
//! **Attribution comes first, always.** Peralta et al. inspected 353 rejected
//! agentic PRs and found only 35.7% were clear agentic failures: 31.2% were
//! workflow-driven and 33.1% had no observable rationale at all. A harness that
//! learns from all three buckets is training on noise, so `UNATTRIBUTABLE`
//! exists as a first-class outcome that is counted and never promoted.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Attribution {
    /// Observable technical failure caused by the agent's output.
    Agentic,
    /// Workflow or environment: flaky infra, a missing tool, a superseded run.
    Process,
    /// A person changed their mind, redirected, or renegotiated scope.
    Human,
    /// No observable rationale. Counted, never learned from.
    Unattributable,
}

impl Attribution {
    pub fn code(&self) -> &'static str {
        match self {
            Attribution::Agentic => "AGENTIC",
            Attribution::Process => "PROCESS",
            Attribution::Human => "HUMAN",
            Attribution::Unattributable => "UNATTRIBUTABLE",
        }
    }

    /// Only agentic failures may become lessons (promotion rule 1).
    pub fn is_promotable(&self) -> bool {
        *self == Attribution::Agentic
    }

    pub fn all() -> &'static [Attribution] {
        &[
            Attribution::Agentic,
            Attribution::Process,
            Attribution::Human,
            Attribution::Unattributable,
        ]
    }
}

/// The locus of an agentic failure. Only meaningful when attribution is
/// `AGENTIC`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Class {
    /// Spec was ambiguous — G0's ambiguity check, or a late "what did you mean".
    SpecAmbig,
    /// A criterion was missing and had to be added mid-implementation.
    SpecMissing,
    /// Edited the wrong file, or a symbol outside the blast radius.
    LocWrong,
    /// Acted on a store or map entry older than the code.
    CtxStale,
    /// Contradicted a fact established earlier in the same run.
    CtxDrift,
    /// Build failure.
    EditCompile,
    /// Test failure, assertion, exception.
    EditRuntime,
    /// Tests pass but mock away the behaviour under test.
    TestInvalid,
    /// Diff exceeded the declared blast radius or budget.
    ScopeCreep,
    /// Lint or house-rule breach, naming, layering.
    ConvViolation,
}

impl Class {
    pub fn code(&self) -> &'static str {
        match self {
            Class::SpecAmbig => "SPEC-AMBIG",
            Class::SpecMissing => "SPEC-MISSING",
            Class::LocWrong => "LOC-WRONG",
            Class::CtxStale => "CTX-STALE",
            Class::CtxDrift => "CTX-DRIFT",
            Class::EditCompile => "EDIT-COMPILE",
            Class::EditRuntime => "EDIT-RUNTIME",
            Class::TestInvalid => "TEST-INVALID",
            Class::ScopeCreep => "SCOPE-CREEP",
            Class::ConvViolation => "CONV-VIOLATION",
        }
    }

    pub fn parse(s: &str) -> Option<Class> {
        Some(match s.trim().to_ascii_uppercase().as_str() {
            "SPEC-AMBIG" => Class::SpecAmbig,
            "SPEC-MISSING" => Class::SpecMissing,
            "LOC-WRONG" => Class::LocWrong,
            "CTX-STALE" => Class::CtxStale,
            "CTX-DRIFT" => Class::CtxDrift,
            "EDIT-COMPILE" => Class::EditCompile,
            "EDIT-RUNTIME" => Class::EditRuntime,
            "TEST-INVALID" => Class::TestInvalid,
            "SCOPE-CREEP" => Class::ScopeCreep,
            "CONV-VIOLATION" => Class::ConvViolation,
            _ => return None,
        })
    }

    /// The locus, for reporting: where in the pipeline this went wrong.
    pub fn locus(&self) -> &'static str {
        match self {
            Class::SpecAmbig | Class::SpecMissing => "spec",
            Class::LocWrong | Class::CtxStale => "retrieval",
            Class::CtxDrift => "context",
            Class::EditCompile | Class::EditRuntime => "edit",
            Class::TestInvalid => "verification",
            Class::ScopeCreep => "plan",
            Class::ConvViolation => "conventions",
        }
    }

    /// Whether this class is one gates can plausibly fix, as opposed to one
    /// that mostly measures the model.
    ///
    /// PLAN.md §4.6: "If your taxonomy is mostly firing on `EDIT-RUNTIME`, you
    /// are measuring the model. If it fires on `SCOPE-CREEP` and
    /// `CONV-VIOLATION`, you are measuring the harness."
    pub fn is_harness_fixable(&self) -> bool {
        matches!(
            self,
            Class::ScopeCreep
                | Class::ConvViolation
                | Class::SpecAmbig
                | Class::SpecMissing
                | Class::LocWrong
                | Class::CtxStale
                | Class::TestInvalid
        )
    }

    pub fn all() -> &'static [Class] {
        &[
            Class::SpecAmbig, Class::SpecMissing, Class::LocWrong, Class::CtxStale,
            Class::CtxDrift, Class::EditCompile, Class::EditRuntime, Class::TestInvalid,
            Class::ScopeCreep, Class::ConvViolation,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_agentic_failures_are_promotable() {
        assert!(Attribution::Agentic.is_promotable());
        for a in [Attribution::Process, Attribution::Human, Attribution::Unattributable] {
            assert!(!a.is_promotable(), "{} must never become a lesson", a.code());
        }
    }

    #[test]
    fn every_class_code_round_trips() {
        for c in Class::all() {
            assert_eq!(Class::parse(c.code()), Some(*c), "{} did not round trip", c.code());
        }
        assert_eq!(Class::parse("NOT-A-CLASS"), None);
        assert_eq!(Class::parse("scope-creep"), Some(Class::ScopeCreep), "parsing is case-insensitive");
    }

    #[test]
    fn codes_are_serialised_as_written_in_the_plan() {
        assert_eq!(
            serde_json::to_string(&Attribution::Unattributable).unwrap(),
            "\"UNATTRIBUTABLE\""
        );
        assert_eq!(serde_json::to_string(&Class::ScopeCreep).unwrap(), "\"scope-creep\"");
    }

    #[test]
    fn harness_fixable_classes_are_the_ones_gates_can_reach() {
        assert!(Class::ScopeCreep.is_harness_fixable());
        assert!(Class::ConvViolation.is_harness_fixable());
        // A compile error is the model's problem, not the harness's.
        assert!(!Class::EditCompile.is_harness_fixable());
        assert!(!Class::EditRuntime.is_harness_fixable());
    }

    #[test]
    fn every_class_names_a_locus() {
        for c in Class::all() {
            assert!(!c.locus().is_empty(), "{} has no locus", c.code());
        }
    }
}
