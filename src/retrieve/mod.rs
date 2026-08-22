//! The retrieval layer (PLAN.md P4, §4.8).
//!
//! > The agent navigates by structure and pulls symbols. It does not open files
//! > to find out what is in them, and it never exceeds its context budget
//! > silently.
//!
//! Progressive disclosure is the default: outline before source, signature
//! before body, metadata before implementation. Every answer carries its token
//! cost, because a retrieval layer whose savings you cannot measure is a
//! retrieval layer you have no reason to trust.
//!
//! The index is an **accelerator, never a dependency**. When it is absent,
//! stale, or the language has no grammar, every query falls through to ripgrep
//! and says so. Claude Code's own designers concluded agentic search beat
//! indexed RAG for their case; the honest position is to ship both, cheap one
//! first, and let the caller see which answered.

pub mod budget;
pub mod fallback;

use crate::map::blast;
use crate::map::db::Index;
use crate::paths::Paths;
use anyhow::{Context, Result, bail};
use rusqlite::params;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Index,
    /// The index could not answer; ripgrep did.
    Ripgrep,
}

#[derive(Debug, Clone, Serialize)]
pub struct Answer {
    pub query: String,
    pub source: Source,
    pub tokens: usize,
    pub text: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

impl Answer {
    pub fn from_index(query: &str, text: String) -> Self {
        Self {
            query: query.to_string(),
            tokens: crate::trajectory::event::estimate_tokens(&text),
            source: Source::Index,
            text,
            truncated: false,
        }
    }

    pub fn from_ripgrep(query: &str, text: String) -> Self {
        Self {
            query: query.to_string(),
            tokens: crate::trajectory::event::estimate_tokens(&text),
            source: Source::Ripgrep,
            text,
            truncated: false,
        }
    }

    /// Apply a token budget, keeping the head and saying what was cut.
    ///
    /// The "what was cut" note is paid for out of the budget, not added on top:
    /// a ceiling you can exceed by telling the reader you exceeded it is not a
    /// ceiling.
    pub fn fit(mut self, budget: usize) -> Self {
        if budget == 0 || self.tokens <= budget {
            return self;
        }
        let total_lines = self.text.lines().count();
        let note = |cut: usize| format!("\n… {cut} more line(s); narrow the query or raise the budget.\n");
        let reserve = crate::trajectory::event::estimate_tokens(&note(total_lines));
        let keep_chars = budget.saturating_sub(reserve) * 4;

        let mut kept = String::new();
        for line in self.text.lines() {
            if kept.len() + line.len() + 1 > keep_chars {
                break;
            }
            kept.push_str(line);
            kept.push('\n');
        }
        kept.push_str(&note(total_lines - kept.lines().count()));
        self.truncated = true;
        self.tokens = crate::trajectory::event::estimate_tokens(&kept);
        self.text = kept;
        self
    }
}

/// A retrieval session: an index if there is a usable one, plus the fallback.
pub struct Retriever {
    pub paths: Paths,
    index: Option<Index>,
    /// Why the index is not being used, when it is not.
    pub degraded: Option<String>,
}

impl Retriever {
    pub fn open(paths: &Paths) -> Result<Self> {
        let db = paths.index_db();
        if !db.exists() {
            return Ok(Self {
                paths: paths.clone(),
                index: None,
                degraded: Some("no index — run `keel map`".into()),
            });
        }
        match Index::open(&db) {
            Ok(index) => {
                let schema = index.meta("schema").ok().flatten();
                if schema.as_deref() != Some(crate::map::db::SCHEMA_VERSION) {
                    return Ok(Self {
                        paths: paths.clone(),
                        index: None,
                        degraded: Some(format!(
                            "index schema is {} not {} — run `keel map --full`",
                            schema.unwrap_or_else(|| "unknown".into()),
                            crate::map::db::SCHEMA_VERSION
                        )),
                    });
                }
                Ok(Self { paths: paths.clone(), index: Some(index), degraded: None })
            }
            Err(e) => Ok(Self {
                paths: paths.clone(),
                index: None,
                degraded: Some(format!("index unreadable ({e}) — run `keel map --full`")),
            }),
        }
    }

    fn index(&self) -> Option<&Index> {
        self.index.as_ref()
    }

    // -----------------------------------------------------------------------
    // outline — the file skeleton, no bodies
    // -----------------------------------------------------------------------

    pub fn outline(&self, path: &str) -> Result<Answer> {
        let Some(index) = self.index() else {
            return fallback::outline(&self.paths, path);
        };
        let mut stmt = index.conn.prepare(
            "SELECT s.kind, s.name, s.parent, s.start_line, s.end_line, s.signature, s.doc
             FROM symbols s JOIN files f ON f.id = s.file_id
             WHERE f.path = ?1 ORDER BY s.start_line",
        )?;
        let rows = stmt.query_map(params![path], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        })?;

        let mut out = String::new();
        let mut n = 0;
        for row in rows {
            let (kind, name, parent, start, end, sig, doc) = row?;
            n += 1;
            let qualified = match &parent {
                Some(p) if !p.is_empty() => format!("{p}::{name}"),
                _ => name.clone(),
            };
            out.push_str(&format!("L{start}-{end}  {kind:<9} {sig}\n"));
            if let Some(d) = doc {
                out.push_str(&format!("           {d}\n"));
            }
            let _ = qualified;
        }
        if n == 0 {
            // A file keel indexed but found no symbols in is a real answer;
            // a file it has never seen is not.
            let known: i64 = index.conn.query_row(
                "SELECT COUNT(*) FROM files WHERE path = ?1",
                params![path],
                |r| r.get(0),
            )?;
            if known == 0 {
                return fallback::outline(&self.paths, path);
            }
            out.push_str("(indexed, no symbols extracted)\n");
        }
        Ok(Answer::from_index(&format!("outline {path}"), format!("{path}\n{out}")))
    }

    // -----------------------------------------------------------------------
    // symbol — signature, doc, location. Never the body.
    // -----------------------------------------------------------------------

    pub fn symbol(&self, name: &str) -> Result<Answer> {
        let Some(index) = self.index() else {
            return fallback::symbol(&self.paths, name);
        };
        let mut stmt = index.conn.prepare(
            "SELECT f.path, s.kind, s.name, s.parent, s.start_line, s.end_line, s.signature, s.doc
             FROM symbols s JOIN files f ON f.id = s.file_id
             WHERE s.name = ?1 ORDER BY f.rank DESC, f.path",
        )?;
        let rows = stmt.query_map(params![name], |r| {
            Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?, r.get::<_, Option<String>>(7)?,
            ))
        })?;

        let mut out = String::new();
        let mut n = 0;
        for row in rows {
            let (path, kind, sname, parent, start, end, sig, doc) = row?;
            n += 1;
            out.push_str(&format!(
                "{path}:{start}-{end}  {kind}  {}\n  {sig}\n",
                parent.map(|p| format!("{p}::{sname}")).unwrap_or(sname)
            ));
            if let Some(d) = doc {
                out.push_str(&format!("  {d}\n"));
            }
            out.push('\n');
        }
        if n == 0 {
            return fallback::symbol(&self.paths, name);
        }
        Ok(Answer::from_index(&format!("symbol {name}"), out))
    }

    // -----------------------------------------------------------------------
    // source — the body, on demand only
    // -----------------------------------------------------------------------

    /// Read a symbol's body. This is the expensive call, and the one the budget
    /// governor guards.
    pub fn source(&self, name: &str, occurrence: usize) -> Result<(Answer, usize)> {
        let Some(index) = self.index() else {
            bail!("no index — `keel source` needs one; use `keel outline` or ripgrep");
        };
        let mut stmt = index.conn.prepare(
            "SELECT f.path, s.start_line, s.end_line
             FROM symbols s JOIN files f ON f.id = s.file_id
             WHERE s.name = ?1 ORDER BY f.rank DESC, f.path",
        )?;
        let rows: Vec<(String, i64, i64)> = stmt
            .query_map(params![name], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<std::result::Result<_, _>>()?;

        if rows.is_empty() {
            bail!("no indexed symbol named `{name}`");
        }
        if occurrence == 0 || occurrence > rows.len() {
            bail!(
                "`{name}` is defined in {} place(s); pass --nth 1..{}",
                rows.len(),
                rows.len()
            );
        }
        // A name defined in several places is the case where an agent quietly
        // reads the wrong body and is confidently wrong afterwards. Returning
        // the highest-ranked one is the useful default; saying so is what stops
        // it being a trap.
        let ambiguity = if rows.len() > 1 {
            let others: Vec<String> = rows
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != occurrence - 1)
                .map(|(i, (p, l, _))| format!("--nth {} → {p}:{l}", i + 1))
                .collect();
            format!(
                "`{name}` is defined in {} places; showing {} of {}. Others: {}\n",
                rows.len(), occurrence, rows.len(), others.join(", ")
            )
        } else {
            String::new()
        };

        let (path, start, end) = &rows[occurrence - 1];
        let abs = self.paths.repo.join(path);
        let content = std::fs::read_to_string(&abs)
            .with_context(|| format!("reading {}", abs.display()))?;
        let body: String = content
            .lines()
            .skip((*start as usize).saturating_sub(1))
            .take((*end - *start + 1) as usize)
            .collect::<Vec<_>>()
            .join("\n");
        let lines = body.lines().count();
        Ok((
            Answer::from_index(
                &format!("source {name}"),
                format!("{ambiguity}{path}:{start}\n{body}\n"),
            ),
            lines,
        ))
    }

    // -----------------------------------------------------------------------
    // refs / importers
    // -----------------------------------------------------------------------

    pub fn refs(&self, name: &str) -> Result<Answer> {
        let Some(index) = self.index() else {
            return fallback::refs(&self.paths, name);
        };
        let mut stmt = index.conn.prepare(
            "SELECT f.path, r.line, r.count FROM refs r JOIN files f ON f.id = r.file_id
             WHERE r.name = ?1 ORDER BY r.count DESC, f.path",
        )?;
        let rows = stmt.query_map(params![name], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
        })?;
        let mut out = String::new();
        let mut total = 0i64;
        let mut files = 0;
        for row in rows {
            let (path, line, count) = row?;
            files += 1;
            total += count;
            out.push_str(&format!("{path}:{line}  ×{count}\n"));
        }
        if files == 0 {
            return fallback::refs(&self.paths, name);
        }
        Ok(Answer::from_index(
            &format!("refs {name}"),
            format!("{name} — {total} use(s) across {files} file(s)\n{out}"),
        ))
    }

    pub fn importers(&self, path: &str) -> Result<Answer> {
        let Some(index) = self.index() else {
            return fallback::importers(&self.paths, path);
        };
        let mut stmt = index.conn.prepare(
            "SELECT src.path FROM edges e
             JOIN files dst ON dst.id = e.dst
             JOIN files src ON src.id = e.src
             WHERE dst.path = ?1 ORDER BY src.rank DESC, src.path",
        )?;
        let rows = stmt.query_map(params![path], |r| r.get::<_, String>(0))?;
        let mut out = String::new();
        let mut n = 0;
        for row in rows {
            n += 1;
            out.push_str(&format!("{}\n", row?));
        }
        Ok(Answer::from_index(
            &format!("importers {path}"),
            format!("{n} file(s) import {path}\n{out}"),
        ))
    }

    // -----------------------------------------------------------------------
    // slice — the bundle for one task, budget-fitted
    // -----------------------------------------------------------------------

    /// Everything one task needs, and nothing else: the criteria it satisfies,
    /// the outline of each file it touches, and the blast radius around them.
    pub fn slice(&self, slug: &str, task_id: &str, budget_tokens: usize) -> Result<Answer> {
        let spec = crate::spec::Spec::load(&self.paths, slug)?;
        let tasks = crate::plan::Tasks::load(&self.paths, slug)?;
        let task = tasks
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("no task `{task_id}` in {slug}"))?;

        let mut out = String::new();
        out.push_str(&format!("# {} {} — {}\n\n", spec.front.slug, task.id, task.title));

        out.push_str("## Criteria\n\n");
        for id in &task.criteria {
            if let Some(c) = spec.criteria.iter().find(|c| &c.id == id) {
                out.push_str(&format!("### {} {}\n{}\n", c.id, c.title, c.statement));
                for o in &c.oracles {
                    out.push_str(&format!("oracle: {}\n", o.summary()));
                }
                out.push('\n');
            }
        }

        out.push_str("## Files\n\n");
        for f in &task.files {
            match self.outline(f) {
                Ok(a) => {
                    out.push_str(&a.text);
                    out.push('\n');
                }
                Err(_) => out.push_str(&format!("{f} (not indexed — new file?)\n\n")),
            }
        }

        if let Some(index) = self.index() {
            let radius = blast::compute(index, &task.files, 1)?;
            let downstream = radius.beyond_scope();
            if !downstream.is_empty() {
                out.push_str("## Downstream (1 hop)\n\n");
                for i in downstream.iter().take(20) {
                    out.push_str(&format!("{} ({} lines)\n", i.path, i.lines));
                }
                out.push('\n');
            }
        }

        out.push_str(&format!(
            "## Budget\n\n{} lines of diff. Done when: {}\n",
            task.budget.unwrap_or(0),
            task.exit.clone().unwrap_or_default()
        ));

        Ok(Answer::from_index(&format!("slice {slug}/{task_id}"), out).fit(budget_tokens))
    }

    /// Total indexed size, for the bench report.
    pub fn totals(&self) -> Result<(usize, usize)> {
        match self.index() {
            Some(i) => i.counts(),
            None => Ok((0, 0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_reports_its_own_token_cost() {
        let a = Answer::from_index("q", "hello world".repeat(10));
        assert!(a.tokens > 0);
        assert_eq!(a.source, Source::Index);
    }

    #[test]
    fn fitting_to_a_budget_truncates_and_says_so() {
        let text = (0..500).map(|i| format!("line {i}\n")).collect::<String>();
        let a = Answer::from_index("q", text).fit(50);
        assert!(a.truncated, "a budget was exceeded silently");
        assert!(a.text.contains("more line(s)"), "{}", a.text);
    }

    #[test]
    fn the_budget_is_a_ceiling_including_the_truncation_note() {
        let text = (0..2000).map(|i| format!("line {i} with some content\n")).collect::<String>();
        for budget in [30usize, 50, 100, 500, 1000] {
            let a = Answer::from_index("q", text.clone()).fit(budget);
            assert!(
                a.tokens <= budget,
                "budget {budget} produced {} tokens", a.tokens
            );
        }
    }

    #[test]
    fn an_answer_inside_its_budget_is_untouched() {
        let a = Answer::from_index("q", "short\n".into());
        let fitted = a.clone().fit(1000);
        assert!(!fitted.truncated);
        assert_eq!(fitted.text, a.text);
    }

    #[test]
    fn a_zero_budget_means_unbounded_not_empty() {
        let a = Answer::from_index("q", "content\n".into()).fit(0);
        assert!(!a.truncated);
        assert_eq!(a.text, "content\n");
    }
}
