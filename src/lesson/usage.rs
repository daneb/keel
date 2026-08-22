//! When each lesson was last used.
//!
//! Deliberately kept *outside* `.keel/store/`: the store hash feeds every
//! projection, so recording usage inside it would mark `CLAUDE.md` stale on
//! every single run. Usage is runtime data about the store, not part of it.

use crate::paths::Paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    pub injected: BTreeMap<String, String>,
    #[serde(default)]
    pub fired: BTreeMap<String, String>,
}

impl Ledger {
    pub fn load(paths: &Paths) -> Result<Self> {
        let p = paths.keel().join("lesson-usage.json");
        if !p.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_str(&std::fs::read_to_string(&p)?).unwrap_or_default())
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let p = paths.keel().join("lesson-usage.json");
        std::fs::write(&p, format!("{}\n", serde_json::to_string_pretty(self)?))?;
        Ok(())
    }

    pub fn record_injection(&mut self, id: &str) {
        self.injected.insert(id.to_string(), crate::store::today());
    }

    pub fn record_fire(&mut self, id: &str) {
        self.fired.insert(id.to_string(), crate::store::today());
    }

    /// The most recent day this lesson did anything at all.
    pub fn last_used(&self, id: &str) -> Option<String> {
        match (self.injected.get(id), self.fired.get(id)) {
            (Some(a), Some(b)) => Some(a.max(b).clone()),
            (Some(a), None) => Some(a.clone()),
            (None, Some(b)) => Some(b.clone()),
            (None, None) => None,
        }
    }

    /// Days since last use, counting from `verified_at` when never used —
    /// a lesson that has never fired is still aging.
    pub fn idle_days(&self, id: &str, verified_at: &str) -> i64 {
        let since = self.last_used(id).unwrap_or_else(|| verified_at.to_string());
        days_between(&since, &crate::store::today())
    }
}

fn days_between(from: &str, to: &str) -> i64 {
    let parse = |s: &str| chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok();
    match (parse(from), parse(to)) {
        (Some(a), Some(b)) => (b - a).num_days(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_latest_of_injection_and_fire_wins() {
        let mut l = Ledger::default();
        l.injected.insert("L-1".into(), "2026-01-01".into());
        l.fired.insert("L-1".into(), "2026-06-01".into());
        assert_eq!(l.last_used("L-1").as_deref(), Some("2026-06-01"));
    }

    #[test]
    fn an_unused_lesson_has_no_last_use() {
        assert_eq!(Ledger::default().last_used("L-1"), None);
    }

    #[test]
    fn idle_days_counts_from_verification_when_never_used() {
        let l = Ledger::default();
        let today = crate::store::today();
        assert_eq!(l.idle_days("L-1", &today), 0);
        assert!(l.idle_days("L-1", "2020-01-01") > 2000, "an unused lesson must still age");
    }

    #[test]
    fn day_arithmetic_is_calendar_correct() {
        assert_eq!(days_between("2026-01-01", "2026-03-01"), 59);
        assert_eq!(days_between("bad", "2026-01-01"), 0);
    }
}
