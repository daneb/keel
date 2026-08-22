//! Tree-sitter extraction: one source file in, symbols and raw imports out.
//!
//! Everything here is best-effort. A file that fails to parse cleanly still
//! yields whatever the error-tolerant parse found, and a file too large to be
//! worth parsing is still indexed as metadata. The map degrades, it never fails
//! the run (P4: accelerator, not dependency).

use crate::map::lang::Lang;
use tree_sitter::{Node, Parser, QueryCursor, StreamingIterator};

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: &'static str,
    pub parent: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: String,
    pub doc: Option<String>,
}

/// An identifier used in a file, with where it first appears and how often.
#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    pub name: String,
    pub line: usize,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct FileFacts {
    pub rel: String,
    pub lang: Lang,
    pub lines: usize,
    pub bytes: u64,
    pub sha: String,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<String>,
    /// Identifiers used here. Filtered to repo-known symbols before indexing.
    pub refs: Vec<Reference>,
    pub parse_ok: bool,
}

/// Reusable per-thread parse state. Compiling a query is expensive relative to
/// parsing a small file, so queries are compiled once per language per thread.
pub struct Extractor {
    parser: Parser,
    cache: Vec<(Lang, Option<crate::map::lang::Compiled>)>,
}

impl Extractor {
    pub fn new() -> Self {
        Self { parser: Parser::new(), cache: Vec::new() }
    }

    fn queries(&mut self, lang: Lang) -> Option<&crate::map::lang::Compiled> {
        if let Some(i) = self.cache.iter().position(|(l, _)| *l == lang) {
            return self.cache[i].1.as_ref();
        }
        let compiled = lang.compile().ok();
        self.cache.push((lang, compiled));
        self.cache.last().unwrap().1.as_ref()
    }

    pub fn extract(&mut self, rel: &str, lang: Lang, source: &[u8]) -> FileFacts {
        let lines = count_lines(source);
        let sha = crate::hashing::sha256_hex(source);
        let mut facts = FileFacts {
            rel: rel.to_string(),
            lang,
            lines,
            bytes: source.len() as u64,
            sha,
            symbols: Vec::new(),
            imports: Vec::new(),
            refs: Vec::new(),
            parse_ok: false,
        };

        if self.parser.set_language(&lang.language()).is_err() {
            return facts;
        }
        let Some(tree) = self.parser.parse(source, None) else { return facts };
        facts.parse_ok = !tree.root_node().has_error();

        let Some(q) = self.queries(lang) else { return facts };
        let (defs, imports, refs_q) = (&q.defs, &q.imports, &q.refs);

        let mut cursor = QueryCursor::new();
        let root = tree.root_node();

        // --- definitions -------------------------------------------------
        let mut raw: Vec<(Node, Node)> = Vec::new();
        let mut m = cursor.matches(defs, root, source);
        while let Some(mat) = m.next() {
            let mut def = None;
            let mut name = None;
            for cap in mat.captures {
                match defs.capture_names()[cap.index as usize] {
                    "def" => def = Some(cap.node),
                    "name" => name = Some(cap.node),
                    _ => {}
                }
            }
            if let (Some(d), Some(n)) = (def, name) {
                raw.push((d, n));
            }
        }
        // Innermost-first ordering lets the parent scan below run in one pass.
        raw.sort_by_key(|(d, _)| (d.start_byte(), std::cmp::Reverse(d.end_byte())));

        let ranges: Vec<(usize, usize, String)> = raw
            .iter()
            .map(|(d, n)| (d.start_byte(), d.end_byte(), text(n, source)))
            .collect();

        for (def, name) in &raw {
            let Some(kind) = lang.symbol_kind(def.kind()) else { continue };
            let name_text = text(name, source);
            if name_text.is_empty() { continue; }
            let parent = ranges
                .iter()
                .filter(|(s, e, _)| *s < def.start_byte() && *e >= def.end_byte())
                .min_by_key(|(s, e, _)| e - s)
                .map(|(_, _, n)| n.clone());
            facts.symbols.push(Symbol {
                name: name_text,
                kind,
                parent,
                start_line: def.start_position().row + 1,
                end_line: def.end_position().row + 1,
                signature: signature(def, source),
                doc: doc_comment(def, source, lang),
            });
        }
        facts.symbols.sort_by_key(|s| s.start_line);

        // --- imports ------------------------------------------------------
        let mut cursor = QueryCursor::new();
        let mut m = cursor.matches(imports, root, source);
        while let Some(mat) = m.next() {
            for cap in mat.captures {
                if imports.capture_names()[cap.index as usize] != "path" { continue; }
                let t = text(&cap.node, source);
                let t = t.trim_matches(['"', '`', '\''].as_ref()).to_string();
                if !t.is_empty() && !facts.imports.contains(&t) {
                    facts.imports.push(t);
                }
            }
        }

        // --- references ---------------------------------------------------
        // Every identifier used in the file, minus the ranges that *are* the
        // definition names. Resolution to actual symbols happens repo-wide in
        // map::build, once every definition in the repo is known.
        let def_name_spans: Vec<(usize, usize)> =
            raw.iter().map(|(_, n)| (n.start_byte(), n.end_byte())).collect();

        let mut cursor = QueryCursor::new();
        let mut seen: std::collections::HashMap<String, (usize, usize)> = Default::default();
        let mut m = cursor.matches(refs_q, root, source);
        while let Some(mat) = m.next() {
            for cap in mat.captures {
                if refs_q.capture_names()[cap.index as usize] != "ref" {
                    continue;
                }
                let node = cap.node;
                if def_name_spans.contains(&(node.start_byte(), node.end_byte())) {
                    continue;
                }
                let name = text(&node, source);
                if name.is_empty() {
                    continue;
                }
                let line = node.start_position().row + 1;
                let e = seen.entry(name).or_insert((line, 0));
                e.0 = e.0.min(line);
                e.1 += 1;
            }
        }
        facts.refs = seen
            .into_iter()
            .map(|(name, (line, count))| Reference { name, line, count })
            .collect();
        facts.refs.sort_by(|a, b| a.name.cmp(&b.name));

        facts
    }
}

impl Default for Extractor {
    fn default() -> Self { Self::new() }
}

/// Metadata-only entry for a file that was never parsed (too large, or an
/// unavailable grammar). It still counts toward the structure map.
pub fn unparsed(rel: &str, lang: Lang, source_len: u64, sha: String, lines: usize) -> FileFacts {
    FileFacts {
        rel: rel.to_string(), lang, lines, bytes: source_len, sha,
        symbols: Vec::new(), imports: Vec::new(), refs: Vec::new(), parse_ok: false,
    }
}

fn text(node: &Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}

fn count_lines(source: &[u8]) -> usize {
    if source.is_empty() { return 0; }
    let n = source.iter().filter(|b| **b == b'\n').count();
    if source.last() == Some(&b'\n') { n } else { n + 1 }
}

/// The declaration line, minus its body: enough to know how to call the thing
/// without pulling the implementation (P4, progressive disclosure).
fn signature(node: &Node, source: &[u8]) -> String {
    let full = node.utf8_text(source).unwrap_or("");
    let cut = full
        .find(['{', '\n'])
        .unwrap_or(full.len());
    let mut sig = full[..cut].trim().to_string();
    if sig.is_empty() {
        sig = full.lines().next().unwrap_or("").trim().to_string();
    }
    let sig = collapse_ws(&sig);
    truncate(&sig, 140)
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        let is_space = c.is_whitespace();
        if is_space {
            if !prev_space { out.push(' '); }
        } else {
            out.push(c);
        }
        prev_space = is_space;
    }
    out.trim().to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { return s.to_string(); }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// First line of the doc attached to a definition: preceding comment block for
/// most languages, leading docstring for Python.
fn doc_comment(node: &Node, source: &[u8], lang: Lang) -> Option<String> {
    if lang == Lang::Python
        && let Some(d) = python_docstring(node, source)
    {
        return Some(d);
    }
    let mut lines: Vec<String> = Vec::new();
    let mut cur = node.prev_sibling();
    while let Some(n) = cur {
        if !n.kind().contains("comment") { break; }
        // A blank line between comment and definition detaches it.
        if n.end_position().row + 1 < node.start_position().row { break; }
        lines.push(text(&n, source));
        cur = n.prev_sibling();
    }
    lines.reverse();
    let cleaned: Vec<String> = lines
        .iter()
        .flat_map(|l| l.lines().map(strip_comment_markers).collect::<Vec<_>>())
        .filter(|l| !l.is_empty())
        .collect();
    cleaned.first().map(|l| truncate(l, 120))
}

fn python_docstring(node: &Node, source: &[u8]) -> Option<String> {
    let body = node.child_by_field_name("body")?;
    let first = body.named_child(0)?;
    let s = if first.kind() == "expression_statement" { first.named_child(0)? } else { first };
    if s.kind() != "string" { return None; }
    let raw = text(&s, source);
    let raw = raw.trim_matches(|c| c == '"' || c == '\'' || c == 'r' || c == 'f' || c == 'b');
    raw.lines().map(|l| l.trim()).find(|l| !l.is_empty()).map(|l| truncate(l, 120))
}

fn strip_comment_markers(line: &str) -> String {
    let l = line.trim();
    let l = l.strip_prefix("///").or_else(|| l.strip_prefix("//!"))
        .or_else(|| l.strip_prefix("/**")).or_else(|| l.strip_prefix("/*"))
        .or_else(|| l.strip_prefix("//")).or_else(|| l.strip_prefix("#"))
        .unwrap_or(l);
    let l = l.trim_start_matches('*').trim();
    l.trim_end_matches("*/").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_rust_symbols_with_parents_and_docs() {
        let src = br#"
/// Adds two numbers.
pub fn add(a: i32, b: i32) -> i32 { a + b }

pub struct Widget { pub id: u32 }

impl Widget {
    /// Renders it.
    pub fn render(&self) -> String { String::new() }
}
"#;
        let mut ex = Extractor::new();
        let f = ex.extract("src/lib.rs", Lang::Rust, src);
        assert!(f.parse_ok);
        let add = f.symbols.iter().find(|s| s.name == "add").expect("add");
        assert_eq!(add.kind, "fn");
        assert_eq!(add.doc.as_deref(), Some("Adds two numbers."));
        assert!(add.signature.starts_with("pub fn add(a: i32"));
        let render = f.symbols.iter().find(|s| s.name == "render").expect("render");
        assert_eq!(render.parent.as_deref(), Some("Widget"));
        assert_eq!(render.doc.as_deref(), Some("Renders it."));
    }

    #[test]
    fn extracts_rust_imports() {
        let mut ex = Extractor::new();
        let f = ex.extract("src/lib.rs", Lang::Rust, b"use crate::alpha::Beta;\nmod gamma;\n");
        assert!(f.imports.iter().any(|i| i.contains("alpha")));
        assert!(f.imports.iter().any(|i| i == "gamma"));
    }

    #[test]
    fn extracts_python_docstring_and_class_methods() {
        let src = b"class Thing:\n    def go(self):\n        \"\"\"Does the thing.\"\"\"\n        return 1\n";
        let mut ex = Extractor::new();
        let f = ex.extract("a.py", Lang::Python, src);
        let go = f.symbols.iter().find(|s| s.name == "go").expect("go");
        assert_eq!(go.parent.as_deref(), Some("Thing"));
        assert_eq!(go.doc.as_deref(), Some("Does the thing."));
    }

    #[test]
    fn extracts_js_requires_and_imports() {
        let mut ex = Extractor::new();
        let f = ex.extract("a.js", Lang::JavaScript,
            b"import x from './alpha.js';\nconst y = require('./beta');\nfunction go(){}\n");
        assert!(f.imports.contains(&"./alpha.js".to_string()));
        assert!(f.imports.contains(&"./beta".to_string()));
        assert!(f.symbols.iter().any(|s| s.name == "go"));
    }

    #[test]
    fn references_exclude_the_definitions_themselves() {
        let mut ex = Extractor::new();
        let f = ex.extract(
            "a.rs",
            Lang::Rust,
            b"fn helper() {}\nfn main() { helper(); helper(); }\n",
        );
        let by_name = |n: &str| f.refs.iter().find(|r| r.name == n).cloned();
        let helper = by_name("helper").expect("helper is used and must be a reference");
        assert_eq!(helper.count, 2, "call sites were not counted");
        assert_eq!(helper.line, 2, "the first use is on line 2");
        assert!(by_name("main").is_none(), "a definition name was reported as a reference");
    }

    #[test]
    fn references_are_found_in_every_language() {
        let cases: Vec<(Lang, &[u8], &str)> = vec![
            (Lang::Python, b"def helper():\n    pass\ndef go():\n    return helper()\n", "helper"),
            (Lang::JavaScript, b"function helper(){}\nfunction go(){ helper(); }\n", "helper"),
            (Lang::Go, b"package m\nfunc helper() {}\nfunc Go() { helper() }\n", "helper"),
        ];
        for (lang, src, name) in cases {
            let mut ex = Extractor::new();
            let f = ex.extract("a", lang, src);
            assert!(
                f.refs.iter().any(|r| r.name == name),
                "{} found no reference to {name}", lang.name()
            );
        }
    }

    #[test]
    fn broken_source_still_yields_what_it_can() {
        let mut ex = Extractor::new();
        let f = ex.extract("a.rs", Lang::Rust, b"pub fn ok() {}\npub fn broken( {\n");
        assert!(!f.parse_ok);
        assert!(f.symbols.iter().any(|s| s.name == "ok"));
    }
}
