//! The symbol index: `.keel/store/map/index.sqlite`.
//!
//! This is the Phase 0 half of the retrieval layer (PLAN.md §4.8). Phase 4
//! promotes it to a queryable service; the schema is written now so that
//! promotion is a new front end over the same tables, not a re-index.

use crate::map::extract::FileFacts;
use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;

pub const SCHEMA_VERSION: &str = "keel.index/2";

const SCHEMA: &str = r#"
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE files (
    id           INTEGER PRIMARY KEY,
    path         TEXT NOT NULL UNIQUE,
    dir          TEXT NOT NULL,
    lang         TEXT NOT NULL,
    lines        INTEGER NOT NULL,
    bytes        INTEGER NOT NULL,
    sha          TEXT NOT NULL,
    parse_ok     INTEGER NOT NULL,
    symbol_count INTEGER NOT NULL,
    rank         REAL NOT NULL,
    in_degree    INTEGER NOT NULL
);
CREATE TABLE symbols (
    id         INTEGER PRIMARY KEY,
    file_id    INTEGER NOT NULL REFERENCES files(id),
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL,
    parent     TEXT,
    start_line INTEGER NOT NULL,
    end_line   INTEGER NOT NULL,
    signature  TEXT NOT NULL,
    doc        TEXT
);
CREATE TABLE imports (
    id             INTEGER PRIMARY KEY,
    file_id        INTEGER NOT NULL REFERENCES files(id),
    raw            TEXT NOT NULL,
    resolved_file  INTEGER REFERENCES files(id)
);
CREATE TABLE edges (
    src INTEGER NOT NULL REFERENCES files(id),
    dst INTEGER NOT NULL REFERENCES files(id),
    PRIMARY KEY (src, dst)
);
CREATE TABLE refs (
    id      INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id),
    name    TEXT NOT NULL,
    line    INTEGER NOT NULL,
    count   INTEGER NOT NULL
);
CREATE INDEX idx_refs_name ON refs(name);
CREATE INDEX idx_refs_file ON refs(file_id);
CREATE INDEX idx_symbols_name ON symbols(name);
CREATE INDEX idx_symbols_file ON symbols(file_id);
CREATE INDEX idx_files_dir  ON files(dir);
CREATE INDEX idx_files_rank ON files(rank DESC);
CREATE INDEX idx_edges_dst  ON edges(dst);
"#;

pub struct Index {
    pub conn: Connection,
}

impl Index {
    /// Build the index from scratch. A rebuild writes to a temporary file and
    /// renames, so a crashed `keel map` never leaves a half-written index that
    /// later reads would trust.
    pub fn create(path: &Path) -> Result<(Self, std::path::PathBuf)> {
        if let Some(p) = path.parent() { std::fs::create_dir_all(p)?; }
        let tmp = path.with_extension("sqlite.tmp");
        let _ = std::fs::remove_file(&tmp);
        let conn = Connection::open(&tmp).with_context(|| format!("opening {}", tmp.display()))?;
        // No WAL while building: the index is renamed into place on success,
        // and a rename that leaves its -wal behind is a corrupt index. A crashed
        // build simply discards the temp file.
        conn.pragma_update(None, "journal_mode", "OFF")?;
        conn.pragma_update(None, "synchronous", "OFF")?;
        conn.execute_batch(SCHEMA)?;
        Ok((Self { conn }, tmp))
    }

    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        Ok(Self { conn })
    }

    pub fn write_all(
        &mut self,
        facts: &[FileFacts],
        ranks: &[f64],
        in_degrees: &[usize],
        edges: &[(usize, usize)],
        meta: &[(&str, String)],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut ins_file = tx.prepare(
                "INSERT INTO files (id, path, dir, lang, lines, bytes, sha, parse_ok, symbol_count, rank, in_degree)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            let mut ins_sym = tx.prepare(
                "INSERT INTO symbols (file_id, name, kind, parent, start_line, end_line, signature, doc)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            let mut ins_imp = tx.prepare(
                "INSERT INTO imports (file_id, raw, resolved_file) VALUES (?1, ?2, ?3)",
            )?;
            let mut ins_ref = tx.prepare(
                "INSERT INTO refs (file_id, name, line, count) VALUES (?1, ?2, ?3, ?4)",
            )?;
            let mut ins_edge = tx.prepare("INSERT OR IGNORE INTO edges (src, dst) VALUES (?1, ?2)")?;
            let mut ins_meta = tx.prepare("INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)")?;

            for (i, f) in facts.iter().enumerate() {
                let dir = f.rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
                ins_file.execute(params![
                    i as i64, f.rel, dir, f.lang.name(), f.lines as i64, f.bytes as i64,
                    f.sha, f.parse_ok as i64, f.symbols.len() as i64,
                    ranks.get(i).copied().unwrap_or(0.0),
                    in_degrees.get(i).copied().unwrap_or(0) as i64,
                ])?;
                for s in &f.symbols {
                    ins_sym.execute(params![
                        i as i64, s.name, s.kind, s.parent,
                        s.start_line as i64, s.end_line as i64, s.signature, s.doc
                    ])?;
                }
                for raw in &f.imports {
                    ins_imp.execute(params![i as i64, raw, None::<i64>])?;
                }
                for r in &f.refs {
                    ins_ref.execute(params![i as i64, r.name, r.line as i64, r.count as i64])?;
                }
            }
            for (a, b) in edges {
                ins_edge.execute(params![*a as i64, *b as i64])?;
            }
            ins_meta.execute(params!["schema", SCHEMA_VERSION])?;
            for (k, v) in meta {
                ins_meta.execute(params![*k, v])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        Ok(match rows.next()? {
            Some(r) => Some(r.get(0)?),
            None => None,
        })
    }

    /// Every indexed file's path and content hash, for skipping unchanged
    /// files on a reindex.
    pub fn shas(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut stmt = self.conn.prepare("SELECT path, sha FROM files")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = std::collections::HashMap::new();
        for row in rows {
            let (p, s) = row?;
            out.insert(p, s);
        }
        Ok(out)
    }

    /// Reconstruct one file's facts from the index, so an unchanged file need
    /// not be parsed again.
    pub fn facts_for(&self, path: &str) -> Result<Option<crate::map::extract::FileFacts>> {
        use crate::map::extract::{FileFacts, Reference, Symbol};
        let mut stmt = self.conn.prepare(
            "SELECT id, lang, lines, bytes, sha, parse_ok FROM files WHERE path = ?1",
        )?;
        let mut rows = stmt.query(params![path])?;
        let Some(row) = rows.next()? else { return Ok(None) };
        let id: i64 = row.get(0)?;
        let lang_name: String = row.get(1)?;
        let Some(lang) = crate::map::lang::Lang::all().iter().find(|l| l.name() == lang_name).copied()
        else {
            return Ok(None);
        };
        let mut facts = FileFacts {
            rel: path.to_string(),
            lang,
            lines: row.get::<_, i64>(2)? as usize,
            bytes: row.get::<_, i64>(3)? as u64,
            sha: row.get(4)?,
            symbols: Vec::new(),
            imports: Vec::new(),
            refs: Vec::new(),
            parse_ok: row.get::<_, i64>(5)? != 0,
        };

        let mut stmt = self.conn.prepare(
            "SELECT name, kind, parent, start_line, end_line, signature, doc
             FROM symbols WHERE file_id = ?1 ORDER BY start_line",
        )?;
        let syms = stmt.query_map(params![id], |r| {
            Ok(Symbol {
                name: r.get(0)?,
                // Kinds are a fixed vocabulary; leaking the DB string keeps the
                // struct's &'static str honest.
                kind: crate::map::lang::intern_kind(&r.get::<_, String>(1)?),
                parent: r.get(2)?,
                start_line: r.get::<_, i64>(3)? as usize,
                end_line: r.get::<_, i64>(4)? as usize,
                signature: r.get(5)?,
                doc: r.get(6)?,
            })
        })?;
        for s in syms {
            facts.symbols.push(s?);
        }

        let mut stmt = self.conn.prepare("SELECT raw FROM imports WHERE file_id = ?1")?;
        let imports = stmt.query_map(params![id], |r| r.get::<_, String>(0))?;
        for i in imports {
            facts.imports.push(i?);
        }

        let mut stmt = self.conn.prepare("SELECT name, line, count FROM refs WHERE file_id = ?1")?;
        let refs = stmt.query_map(params![id], |r| {
            Ok(Reference {
                name: r.get(0)?,
                line: r.get::<_, i64>(1)? as usize,
                count: r.get::<_, i64>(2)? as usize,
            })
        })?;
        for r in refs {
            facts.refs.push(r?);
        }
        Ok(Some(facts))
    }

    pub fn counts(&self) -> Result<(usize, usize)> {
        let files: i64 = self.conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
        let syms: i64 = self.conn.query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0))?;
        Ok((files as usize, syms as usize))
    }
}

/// Atomically replace the live index with a freshly built one.
pub fn promote(tmp: &Path, final_path: &Path) -> Result<()> {
    for suffix in ["-wal", "-shm"] {
        let side = tmp.with_file_name(format!("{}{}", tmp.file_name().unwrap().to_string_lossy(), suffix));
        let _ = std::fs::remove_file(side);
        let live = final_path.with_file_name(format!("{}{}", final_path.file_name().unwrap().to_string_lossy(), suffix));
        let _ = std::fs::remove_file(live);
    }
    std::fs::rename(tmp, final_path)
        .with_context(|| format!("promoting index to {}", final_path.display()))?;
    Ok(())
}
