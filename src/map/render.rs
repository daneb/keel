//! Budget-fitted rendering of the map.
//!
//! Both outputs are fitted the same way: choose how much of the repository to
//! cover, then binary-search the detail level down until the rendered result
//! fits inside a hard line budget. A map that silently exceeded its budget
//! would be exactly the "catastrophic remembering" failure P2 is guarding
//! against, so the budget wins and the detail gives way.

use crate::config::MapConfig;
use crate::map::extract::{FileFacts, Symbol};
use crate::paths::Paths;
use crate::store::frontmatter::FrontMatter;
use crate::store::{StoreDoc, today};
use anyhow::Result;
use std::collections::BTreeMap;

/// Fraction of total PageRank mass a structure map aims to cover.
const COVERAGE_TARGET: f64 = 0.85;
const MIN_KEY_FILES: usize = 10;
const MAX_KEY_FILES: usize = 120;
/// Detail ladder, richest first.
const DETAIL_LADDER: &[usize] = &[14, 10, 8, 6, 5, 4, 3, 2, 1];
/// Share of the structure budget the directory overview may claim.
const LAYOUT_SHARE_PCT: usize = 30;

pub struct FileView<'a> {
    pub facts: &'a FileFacts,
    pub rank: f64,
    pub in_degree: usize,
}

pub struct View<'a> {
    /// All files, sorted by rank descending.
    pub ranked: Vec<FileView<'a>>,
    pub total_files: usize,
    pub total_symbols: usize,
}

impl<'a> View<'a> {
    pub fn new(facts: &'a [FileFacts], ranks: &[f64], in_degrees: &[usize]) -> Self {
        let mut ranked: Vec<FileView<'a>> = facts
            .iter()
            .enumerate()
            .map(|(i, f)| FileView {
                facts: f,
                rank: ranks.get(i).copied().unwrap_or(0.0),
                in_degree: in_degrees.get(i).copied().unwrap_or(0),
            })
            .collect();
        ranked.sort_by(|a, b| {
            b.rank.partial_cmp(&a.rank).unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.facts.rel.cmp(&b.facts.rel))
        });
        Self {
            total_files: facts.len(),
            total_symbols: facts.iter().map(|f| f.symbols.len()).sum(),
            ranked,
        }
    }

    /// Smallest prefix of the ranked list covering `COVERAGE_TARGET` of the
    /// rank mass — the "important part" of the repository.
    fn coverage_target(&self) -> usize {
        let total: f64 = self.ranked.iter().map(|f| f.rank).sum();
        if total <= 0.0 {
            return self.ranked.len().min(MAX_KEY_FILES);
        }
        let mut acc = 0.0;
        let mut n = 0;
        for f in &self.ranked {
            acc += f.rank;
            n += 1;
            if acc / total >= COVERAGE_TARGET { break; }
        }
        n.clamp(MIN_KEY_FILES.min(self.ranked.len()), MAX_KEY_FILES.min(self.ranked.len()))
    }
}

// ---------------------------------------------------------------------------
// structure.md
// ---------------------------------------------------------------------------

pub fn structure_md(paths: &Paths, view: &View, budget: usize) -> String {
    let head = structure_head(paths, view);
    // The layout section is cheap and orients a reader fast, but it must not
    // eat the budget the actual symbols need.
    let layout = clip_lines(&layout_section(view), budget * LAYOUT_SHARE_PCT / 100);
    let fixed = head.lines().count() + layout.lines().count() + 4;
    let remaining = budget.saturating_sub(fixed);

    let (n, detail) = fit(view, remaining, view.coverage_target());

    let mut out = String::new();
    out.push_str(&head);
    out.push_str(&layout);
    out.push_str("\n## Key files\n\n");
    out.push_str(&key_files_section(view, n, detail));
    if n < view.total_files {
        out.push_str(&format!(
            "\n_{} further files omitted at this budget; see per-directory CODEMAPs._\n",
            view.total_files - n
        ));
    }
    // The budget is an invariant, not a target. Fitting gets us close; this
    // makes it true even when the preamble alone would overrun a tiny budget.
    hard_clip(out, budget)
}

/// Truncate to `max` lines, keeping whole lines.
fn clip_lines(s: &str, max: usize) -> String {
    if s.lines().count() <= max { return s.to_string(); }
    let mut out: String = s.lines().take(max).collect::<Vec<_>>().join("\n");
    out.push('\n');
    out
}

/// Final enforcement of a hard line budget, with the overrun stated rather
/// than hidden — a map that quietly self-truncates is worse than a short one.
fn hard_clip(s: String, budget: usize) -> String {
    let total = s.lines().count();
    if total <= budget { return s; }
    if budget == 0 { return String::new(); }
    let keep = budget.saturating_sub(1);
    let mut out: String = s.lines().take(keep).collect::<Vec<_>>().join("\n");
    if !out.is_empty() { out.push('\n'); }
    out.push_str(&format!("_… {} lines cut at the budget; raise it in .keel/keel.toml._\n",
        total - keep));
    clip_lines(&out, budget)
}

/// Largest detail level from the ladder that fits `budget` at `n` files;
/// if none fits, shrink the file count instead (binary search, monotonic in n).
fn fit(view: &View, budget: usize, n_target: usize) -> (usize, usize) {
    for &detail in DETAIL_LADDER {
        if key_files_section(view, n_target, detail).lines().count() <= budget {
            return (n_target, detail);
        }
    }
    let detail = 1;
    let (mut lo, mut hi) = (0usize, n_target);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if key_files_section(view, mid, detail).lines().count() <= budget {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    (lo, detail)
}

fn structure_head(paths: &Paths, view: &View) -> String {
    let mut langs: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &view.ranked {
        *langs.entry(f.facts.lang.name()).or_default() += 1;
    }
    let lang_summary = langs
        .iter()
        .map(|(k, v)| format!("{k} {v}"))
        .collect::<Vec<_>>()
        .join(", ");
    let total_lines: usize = view.ranked.iter().map(|f| f.facts.lines).sum();
    let _ = paths;
    format!(
        "# Repository structure\n\n\
         <!-- generated by `keel map`. Do not edit: your changes will be overwritten. -->\n\n\
         **{} files · {} symbols · {} lines** — {}\n\n\
         Files are ordered by import-graph centrality, not alphabetically. \
         Signatures only; read a body with the file path and line number. \
         Per-directory detail lives in `.keel/store/map/<dir>/CODEMAP.md`.\n",
        view.total_files, view.total_symbols, total_lines, lang_summary
    )
}

/// One row of the directory overview.
#[derive(Default)]
struct DirSummary<'a> {
    files: usize,
    lines: usize,
    /// (basename, rank), for naming the directory's most central files.
    members: Vec<(&'a str, f64)>,
}

fn layout_section(view: &View) -> String {
    let mut dirs: BTreeMap<&str, DirSummary> = BTreeMap::new();
    for f in &view.ranked {
        let dir = f.facts.rel.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
        let e = dirs.entry(dir).or_default();
        e.files += 1;
        e.lines += f.facts.lines;
        let base = f.facts.rel.rsplit('/').next().unwrap_or(&f.facts.rel);
        e.members.push((base, f.rank));
    }
    let mut out = String::from("\n## Layout\n\n");
    for (dir, DirSummary { files, lines, mut members }) in dirs {
        members.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let names: Vec<&str> = members.iter().take(3).map(|(n, _)| *n).collect();
        out.push_str(&format!(
            "- `{}/` — {} file{}, {} lines · {}\n",
            dir, files, if files == 1 { "" } else { "s" }, lines, names.join(", ")
        ));
    }
    out
}

fn key_files_section(view: &View, n: usize, detail: usize) -> String {
    let mut out = String::new();
    for f in view.ranked.iter().take(n) {
        out.push_str(&file_block(f, detail));
    }
    out
}

fn file_block(f: &FileView, detail: usize) -> String {
    let mut out = String::new();
    let imported = if f.in_degree > 0 {
        format!(" · imported by {}", f.in_degree)
    } else {
        String::new()
    };
    let unparsed = if f.facts.parse_ok { "" } else { " · ⚠ not fully parsed" };
    out.push_str(&format!(
        "**`{}`** · {} lines{}{}\n",
        f.facts.rel, f.facts.lines, imported, unparsed
    ));
    for s in select_symbols(&f.facts.symbols, detail) {
        out.push_str(&format!("- {}\n", symbol_line(s)));
    }
    out.push('\n');
    out
}

fn symbol_line(s: &Symbol) -> String {
    let qualified = match &s.parent {
        Some(p) if !p.is_empty() => format!("{p}::{}", s.name),
        _ => s.name.clone(),
    };
    let sig = if s.signature.len() > 4 && s.signature.contains(&s.name) {
        s.signature.clone()
    } else {
        qualified.clone()
    };
    let sig = clip(&sig, 110);
    match &s.doc {
        Some(d) if !d.is_empty() => format!("`{}` — {}  <sub>L{}</sub>", sig, clip(d, 80), s.start_line),
        _ => format!("`{}`  <sub>L{}</sub>", sig, s.start_line),
    }
}

/// Which symbols earn a line when we cannot show them all: public surface
/// first (types before functions before methods), then the substantial ones.
fn select_symbols(symbols: &[Symbol], budget: usize) -> Vec<&Symbol> {
    if budget == 0 { return vec![]; }
    let mut scored: Vec<(&Symbol, i64)> = symbols
        .iter()
        .map(|s| {
            let kind_score = match s.kind {
                "struct" | "class" | "trait" | "interface" | "enum" | "type" | "record" => 100,
                "fn" => 80,
                "impl" | "mod" => 60,
                "method" | "ctor" => 50,
                "const" | "static" | "macro" => 30,
                _ => 20,
            };
            let top_level = if s.parent.is_none() { 40 } else { 0 };
            let public = if s.signature.starts_with("pub ") || s.signature.starts_with("export ")
                || s.signature.contains("public ") { 30 } else { 0 };
            let documented = if s.doc.is_some() { 15 } else { 0 };
            let span = (s.end_line.saturating_sub(s.start_line) as i64).min(60) / 4;
            (s, kind_score + top_level + public + documented + span)
        })
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.start_line.cmp(&b.0.start_line)));
    let mut chosen: Vec<&Symbol> = scored.into_iter().take(budget).map(|(s, _)| s).collect();
    chosen.sort_by_key(|s| s.start_line);
    chosen
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max { return s.to_string(); }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub fn write_structure(paths: &Paths, body: &str) -> Result<()> {
    let front = FrontMatter {
        id: Some("STRUCT-0001".into()),
        scope: Some("repo".into()),
        owner: Some("agent".into()),
        verified_at: Some(today()),
        generated: true,
        ..Default::default()
    };
    StoreDoc::write(&paths.structure(), &front, body)
}

// ---------------------------------------------------------------------------
// per-directory CODEMAP.md
// ---------------------------------------------------------------------------

pub fn write_codemaps(paths: &Paths, view: &View, cfg: &MapConfig) -> Result<usize> {
    clear_old_codemaps(&paths.map_dir())?;

    let mut by_dir: BTreeMap<&str, Vec<&FileView>> = BTreeMap::new();
    for f in &view.ranked {
        let dir = f.facts.rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        by_dir.entry(dir).or_default().push(f);
    }

    let mut written = 0;
    for (dir, files) in by_dir {
        if files.len() < cfg.codemap_min_files { continue; }
        let body = codemap_md(dir, &files, cfg.codemap_budget_lines);
        let out = if dir.is_empty() {
            paths.map_dir().join("CODEMAP.md")
        } else {
            paths.map_dir().join(dir).join("CODEMAP.md")
        };
        let front = FrontMatter {
            id: Some(format!("MAP-{}", slug(dir))),
            scope: Some(if dir.is_empty() { "repo".into() } else { format!("dir:{dir}") }),
            owner: Some("agent".into()),
            verified_at: Some(today()),
            generated: true,
            ..Default::default()
        };
        StoreDoc::write(&out, &front, &body)?;
        written += 1;
    }
    Ok(written)
}

fn codemap_md(dir: &str, files: &[&FileView], budget: usize) -> String {
    let label = if dir.is_empty() { "(repository root)" } else { dir };
    let total_lines: usize = files.iter().map(|f| f.facts.lines).sum();
    let head = format!(
        "# CODEMAP — `{}`\n\n\
         <!-- generated by `keel map`. Do not edit: your changes will be overwritten. -->\n\n\
         {} files · {} lines · {} symbols\n\n",
        label,
        files.len(),
        total_lines,
        files.iter().map(|f| f.facts.symbols.len()).sum::<usize>()
    );
    let remaining = budget.saturating_sub(head.lines().count());

    let render = |detail: usize| -> String {
        files.iter().map(|f| file_block(f, detail)).collect::<String>()
    };
    let mut chosen = 0usize;
    for &detail in DETAIL_LADDER {
        if render(detail).lines().count() <= remaining {
            chosen = detail;
            break;
        }
    }
    let mut body = head;
    body.push_str(&render(chosen));
    if chosen == 0 {
        body.push_str("_Directory too large for its CODEMAP budget; raise `map.codemap_budget_lines`._\n");
    }
    hard_clip(body, budget)
}

fn clear_old_codemaps(map_dir: &std::path::Path) -> Result<()> {
    if !map_dir.is_dir() { return Ok(()); }
    for entry in std::fs::read_dir(map_dir)? {
        let p = entry?.path();
        if p.is_dir() {
            std::fs::remove_dir_all(&p)?;
        } else if p.file_name().and_then(|n| n.to_str()) == Some("CODEMAP.md") {
            std::fs::remove_file(&p)?;
        }
    }
    Ok(())
}

fn slug(dir: &str) -> String {
    if dir.is_empty() { return "root".into(); }
    dir.replace('/', "-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::extract::Extractor;
    use crate::map::lang::Lang;

    fn sample_facts() -> Vec<FileFacts> {
        let mut ex = Extractor::new();
        let a = ex.extract(
            "src/api/routes.rs",
            Lang::Rust,
            br#"
/// Registers a route.
pub fn register(path: &str) {}
/// A route table.
pub struct Table { pub n: usize }
impl Table {
    pub fn insert(&mut self) {}
    pub fn remove(&mut self) {}
    fn private_helper(&self) {}
}
pub const MAX: usize = 10;
pub enum Method { Get, Post }
pub trait Handler {}
pub type Alias = usize;
"#,
        );
        let b = ex.extract("src/main.rs", Lang::Rust, b"fn main() {}\n");
        vec![a, b]
    }

    #[test]
    fn structure_respects_a_hard_budget() {
        let facts = sample_facts();
        let ranks = vec![0.7, 0.3];
        let degs = vec![2, 0];
        let view = View::new(&facts, &ranks, &degs);
        for budget in [0usize, 1, 3, 8, 12, 20, 40, 400] {
            let md = structure_md(&Paths { repo: ".".into() }, &view, budget);
            let n = md.lines().count();
            assert!(n <= budget, "budget {budget} exceeded: rendered {n} lines\n{md}");
        }
    }

    #[test]
    fn richer_budget_shows_more_symbols() {
        let facts = sample_facts();
        let ranks = vec![0.7, 0.3];
        let degs = vec![2, 0];
        let view = View::new(&facts, &ranks, &degs);
        let small = structure_md(&Paths { repo: ".".into() }, &view, 25);
        let large = structure_md(&Paths { repo: ".".into() }, &view, 400);
        assert!(large.lines().count() > small.lines().count());
        assert!(large.contains("Table"));
    }

    #[test]
    fn selection_prefers_public_types_over_private_methods() {
        let facts = sample_facts();
        let chosen = select_symbols(&facts[0].symbols, 3);
        let names: Vec<&str> = chosen.iter().map(|s| s.name.as_str()).collect();
        assert!(!names.contains(&"private_helper"), "picked private helper: {names:?}");
    }

    #[test]
    fn codemap_respects_its_budget() {
        let facts = sample_facts();
        let ranks = vec![0.7, 0.3];
        let degs = vec![2, 0];
        let view = View::new(&facts, &ranks, &degs);
        let refs: Vec<&FileView> = view.ranked.iter().collect();
        for budget in [0usize, 2, 8, 15, 150] {
            let md = codemap_md("src", &refs, budget);
            assert!(md.lines().count() <= budget, "codemap budget {budget} exceeded");
        }
    }

    #[test]
    fn ranked_order_is_deterministic_on_ties() {
        let facts = sample_facts();
        let ranks = vec![0.5, 0.5];
        let degs = vec![0, 0];
        let a = View::new(&facts, &ranks, &degs);
        let b = View::new(&facts, &ranks, &degs);
        let names_a: Vec<&str> = a.ranked.iter().map(|f| f.facts.rel.as_str()).collect();
        let names_b: Vec<&str> = b.ranked.iter().map(|f| f.facts.rel.as_str()).collect();
        assert_eq!(names_a, names_b);
        assert_eq!(names_a[0], "src/api/routes.rs");
    }
}
