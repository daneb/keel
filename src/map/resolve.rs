//! Resolving raw import strings to files in this repository.
//!
//! Deliberately heuristic and deliberately silent on failure: an import that
//! points outside the repo (a crate, a stdlib module, an npm package) is not an
//! error, it is simply not an edge. The graph only needs to be good enough to
//! rank files, and a missed edge costs a little ranking accuracy, nothing more.

use crate::map::lang::Lang;
use std::collections::HashMap;

pub struct Resolver {
    /// rel path → index into the file list
    by_path: HashMap<String, usize>,
    /// "stem" candidates → indices, e.g. "src/api/routes" → [i]
    by_stem: HashMap<String, Vec<usize>>,
    /// bare module/basename → indices, e.g. "routes" → [i, j]
    by_name: HashMap<String, Vec<usize>>,
}

impl Resolver {
    pub fn new(paths: &[String]) -> Self {
        let mut by_path = HashMap::new();
        let mut by_stem: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();

        for (i, p) in paths.iter().enumerate() {
            by_path.insert(p.clone(), i);
            let stem = strip_ext(p);
            by_stem.entry(stem.clone()).or_default().push(i);
            // `a/b/mod.rs`, `a/b/index.ts` and `a/b/__init__.py` all *are* `a/b`.
            if let Some(dir) = package_dir(&stem) {
                by_stem.entry(dir).or_default().push(i);
            }
            if let Some(base) = stem.rsplit('/').next() {
                by_name.entry(base.to_string()).or_default().push(i);
            }
        }
        Self { by_path, by_stem, by_name }
    }

    /// Resolve one import as seen from `from_rel`. `None` = external.
    pub fn resolve(&self, from_rel: &str, lang: Lang, raw: &str) -> Option<usize> {
        match lang {
            Lang::JavaScript | Lang::TypeScript | Lang::Tsx => self.resolve_relative(from_rel, raw),
            Lang::Python => self.resolve_python(from_rel, raw),
            Lang::Rust => self.resolve_rust(from_rel, raw),
            Lang::Java => self.resolve_java(raw),
            Lang::Go => self.resolve_go(raw),
            Lang::CSharp => self.resolve_csharp(raw),
        }
    }

    fn lookup_stem(&self, stem: &str) -> Option<usize> {
        let stem = stem.trim_start_matches("./").trim_start_matches('/');
        if let Some(i) = self.by_path.get(stem) { return Some(*i); }
        self.by_stem.get(stem).and_then(|v| v.first().copied())
    }

    fn resolve_relative(&self, from_rel: &str, raw: &str) -> Option<usize> {
        if !raw.starts_with('.') { return None; }
        let base = parent_dir(from_rel);
        let joined = normalise(&format!("{base}/{raw}"));
        self.lookup_stem(&joined).or_else(|| self.lookup_stem(&format!("{joined}/index")))
    }

    fn resolve_python(&self, from_rel: &str, raw: &str) -> Option<usize> {
        if let Some(rest) = raw.strip_prefix('.') {
            // `.mod` = sibling, `..mod` = parent, and so on.
            let mut dir = parent_dir(from_rel).to_string();
            let mut rest = rest;
            while let Some(r) = rest.strip_prefix('.') {
                dir = parent_dir(&dir).to_string();
                rest = r;
            }
            let tail = rest.replace('.', "/");
            let joined = normalise(&if tail.is_empty() { dir } else { format!("{dir}/{tail}") });
            return self.lookup_stem(&joined).or_else(|| self.lookup_stem(&format!("{joined}/__init__")));
        }
        let as_path = raw.replace('.', "/");
        self.lookup_stem(&as_path)
            .or_else(|| self.lookup_stem(&format!("src/{as_path}")))
            .or_else(|| self.unique_by_name(raw.rsplit('.').next().unwrap_or(raw)))
    }

    fn resolve_rust(&self, from_rel: &str, raw: &str) -> Option<usize> {
        let cleaned = raw.trim_start_matches("use ").trim_end_matches(';').trim();
        let segments: Vec<&str> = cleaned
            .split("::")
            .map(|s| s.trim())
            .take_while(|s| !s.starts_with('{') && *s != "*")
            .collect();
        if segments.is_empty() { return None; }

        // `mod foo;` — a bare identifier resolves against the current directory.
        if segments.len() == 1 && !matches!(segments[0], "crate" | "self" | "super" | "std" | "core" | "alloc") {
            let dir = module_dir(from_rel);
            if let Some(i) = self.lookup_stem(&format!("{dir}/{}", segments[0])) { return Some(i); }
            if let Some(i) = self.lookup_stem(&format!("{dir}/{}/mod", segments[0])) { return Some(i); }
        }

        let mut path_segs: Vec<String> = Vec::new();
        let mut base = String::new();
        for (n, seg) in segments.iter().enumerate() {
            match *seg {
                "crate" if n == 0 => base = crate_root(from_rel),
                "self" if n == 0 => base = module_dir(from_rel),
                "super" if n == 0 => base = parent_dir(&module_dir(from_rel)).to_string(),
                "std" | "core" | "alloc" if n == 0 => return None,
                s => path_segs.push(s.to_string()),
            }
        }
        if base.is_empty() { base = crate_root(from_rel); }

        // Trailing segments may name an item rather than a module, so try the
        // longest path first and shorten until something matches.
        while !path_segs.is_empty() {
            let joined = normalise(&format!("{base}/{}", path_segs.join("/")));
            if let Some(i) = self.lookup_stem(&joined) { return Some(i); }
            if let Some(i) = self.lookup_stem(&format!("{joined}/mod")) { return Some(i); }
            path_segs.pop();
        }
        None
    }

    fn resolve_java(&self, raw: &str) -> Option<usize> {
        let as_path = raw.replace('.', "/");
        // `com.acme.Foo` → any file ending in `com/acme/Foo.java`.
        self.by_stem.iter()
            .filter(|(stem, _)| stem.ends_with(&as_path))
            .min_by_key(|(stem, _)| stem.len())
            .and_then(|(_, v)| v.first().copied())
    }

    fn resolve_go(&self, raw: &str) -> Option<usize> {
        // Go imports name a package directory somewhere under the module path;
        // match the longest directory suffix we actually have.
        let mut best: Option<(usize, usize)> = None;
        for (stem, idxs) in &self.by_stem {
            let dir = parent_dir(stem);
            if dir.is_empty() { continue; }
            if raw.ends_with(dir) {
                let score = dir.len();
                if best.is_none_or(|(s, _)| score > s)
                    && let Some(i) = idxs.first()
                {
                    best = Some((score, *i));
                }
            }
        }
        best.map(|(_, i)| i)
    }

    fn resolve_csharp(&self, raw: &str) -> Option<usize> {
        // A `using` names a namespace, which conventionally mirrors a directory
        // -- but unlike a Go import path it carries no repository prefix, so the
        // directory ends with the namespace rather than the other way round.
        //
        // A namespace spans several files and this returns one, so blast radius
        // through a `using` is a floor, not a ceiling.
        let as_path = raw.replace('.', "/");
        self.by_stem
            .iter()
            .filter(|(stem, _)| parent_dir(stem).ends_with(&as_path))
            .min_by_key(|(stem, _)| stem.len())
            .and_then(|(_, v)| v.first().copied())
    }

    fn unique_by_name(&self, name: &str) -> Option<usize> {
        match self.by_name.get(name) {
            Some(v) if v.len() == 1 => Some(v[0]),
            _ => None,
        }
    }
}

fn strip_ext(p: &str) -> String {
    match p.rfind('.') {
        Some(i) if !p[i..].contains('/') => p[..i].to_string(),
        _ => p.to_string(),
    }
}

fn package_dir(stem: &str) -> Option<String> {
    for marker in ["/mod", "/index", "/__init__"] {
        if let Some(d) = stem.strip_suffix(marker) {
            return Some(d.to_string());
        }
    }
    None
}

fn parent_dir(p: &str) -> &str {
    match p.rfind('/') { Some(i) => &p[..i], None => "" }
}

/// The directory that holds a Rust file's *submodules*. For `mod.rs`, `lib.rs`
/// and `main.rs` the file already is the directory's module, so submodules are
/// siblings; for any other file they live in a directory named after it.
fn module_dir(from_rel: &str) -> String {
    let base = from_rel.rsplit('/').next().unwrap_or(from_rel);
    if matches!(base, "mod.rs" | "lib.rs" | "main.rs") {
        parent_dir(from_rel).to_string()
    } else {
        strip_ext(from_rel)
    }
}

/// The nearest plausible crate root for a Rust file: the directory holding
/// `main.rs`/`lib.rs`, approximated as the first `src/` on the path.
fn crate_root(from_rel: &str) -> String {
    match from_rel.find("src/") {
        Some(i) => from_rel[..i + 3].trim_end_matches('/').to_string(),
        None => parent_dir(from_rel).to_string(),
    }
}

/// Collapse `.` and `..` segments; the walk never produces them, so any that
/// survive would silently break every lookup.
fn normalise(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => { out.pop(); }
            s => out.push(s),
        }
    }
    out.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Vec<String> {
        ["src/main.rs", "src/api/mod.rs", "src/api/routes.rs", "src/core/auth.rs",
         "web/app/index.ts", "web/app/alpha.ts", "pkg/thing/thing.go",
         "py/pkg/__init__.py", "py/pkg/util.py",
         "java/com/acme/Foo.java",
         "cs/Acme/Widgets/Widget.cs", "cs/Acme/Widgets/Spinner.cs"]
            .iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn csharp_using_resolves_through_the_namespace_directory() {
        let r = Resolver::new(&files());
        // The namespace maps onto the directory holding it.
        let hit = r.resolve("cs/Acme/App.cs", Lang::CSharp, "Acme.Widgets");
        assert!(hit.is_some(), "Acme.Widgets should resolve into cs/Acme/Widgets");

        // A namespace we do not have is external, not a wrong guess.
        assert_eq!(r.resolve("cs/Acme/App.cs", Lang::CSharp, "System.Text.Json"), None);
        assert_eq!(r.resolve("cs/Acme/App.cs", Lang::CSharp, "System"), None);
    }

    #[test]
    fn rust_crate_and_mod_paths() {
        let f = files();
        let r = Resolver::new(&f);
        assert_eq!(r.resolve("src/main.rs", Lang::Rust, "crate::api::routes"), Some(2));
        assert_eq!(r.resolve("src/main.rs", Lang::Rust, "api"), Some(1));
        assert_eq!(r.resolve("src/api/mod.rs", Lang::Rust, "self::routes"), Some(2));
        assert_eq!(r.resolve("src/api/routes.rs", Lang::Rust, "super::core::auth"), None);
        assert_eq!(r.resolve("src/main.rs", Lang::Rust, "std::collections::HashMap"), None);
    }

    #[test]
    fn rust_super_is_relative_to_the_module_not_the_directory() {
        let f = files();
        let r = Resolver::new(&f);
        // src/api/routes.rs is module api::routes, so `super` is api — and
        // api::core::auth does not exist even though src/core/auth.rs does.
        assert_eq!(r.resolve("src/api/routes.rs", Lang::Rust, "super::core::auth"), None);
        // From src/api/mod.rs, `super` is the crate root, so it resolves.
        assert_eq!(r.resolve("src/api/mod.rs", Lang::Rust, "super::core::auth"), Some(3));
    }

    #[test]
    fn rust_item_suffix_falls_back_to_module() {
        let r = Resolver::new(&files());
        // `auth::AuthGuard` — AuthGuard is an item, not a file.
        assert_eq!(r.resolve("src/main.rs", Lang::Rust, "crate::core::auth::AuthGuard"), Some(3));
    }

    #[test]
    fn js_relative_and_index() {
        let r = Resolver::new(&files());
        assert_eq!(r.resolve("web/app/index.ts", Lang::TypeScript, "./alpha"), Some(5));
        assert_eq!(r.resolve("web/app/alpha.ts", Lang::TypeScript, "."), Some(4));
        assert_eq!(r.resolve("web/app/alpha.ts", Lang::TypeScript, "react"), None);
    }

    #[test]
    fn python_relative_and_absolute() {
        let r = Resolver::new(&files());
        assert_eq!(r.resolve("py/pkg/__init__.py", Lang::Python, ".util"), Some(8));
        assert_eq!(r.resolve("other.py", Lang::Python, "py.pkg.util"), Some(8));
    }

    #[test]
    fn java_and_go_suffix_matching() {
        let r = Resolver::new(&files());
        assert_eq!(r.resolve("x.java", Lang::Java, "com.acme.Foo"), Some(9));
        assert_eq!(r.resolve("x.go", Lang::Go, "github.com/me/repo/pkg/thing"), Some(6));
    }
}
