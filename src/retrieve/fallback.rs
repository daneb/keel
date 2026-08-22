//! Ripgrep fallback (PLAN.md P4).
//!
//! > Fallback path is mandatory: if the index is stale, absent, or the language
//! > has no grammar, fall through to ripgrep + read. The index is an
//! > accelerator, never a dependency.
//!
//! Every fallback answer is labelled `ripgrep`, so a caller can tell a
//! structural answer from a textual one. Silently degrading from symbols to
//! grep is how an agent ends up confidently wrong about a codebase.

use super::Answer;
use crate::paths::Paths;
use anyhow::{Result, bail};

/// The grep binary to use. ripgrep is preferred; plain grep is the floor.
fn grep(paths: &Paths, args: &[&str], pattern: &str) -> Result<String> {
    for (bin, base) in [("rg", vec!["--line-number", "--no-heading", "--color=never"]),
                        ("grep", vec!["-rn"])] {
        let mut cmd = std::process::Command::new(bin);
        cmd.args(&base).args(args).arg(pattern).arg(".");
        cmd.current_dir(&paths.repo);
        match cmd.output() {
            Ok(o) => {
                // grep exits 1 on "no matches", which is an answer, not a failure.
                if o.status.success() || o.status.code() == Some(1) {
                    return Ok(String::from_utf8_lossy(&o.stdout).to_string());
                }
            }
            Err(_) => continue,
        }
    }
    bail!("neither ripgrep nor grep is available, and the index could not answer")
}

pub fn outline(paths: &Paths, path: &str) -> Result<Answer> {
    let abs = paths.repo.join(path);
    if !abs.is_file() {
        bail!("{path} is neither indexed nor present on disk");
    }
    let content = std::fs::read_to_string(&abs)?;
    // Without a grammar, the honest skeleton is "lines that look like
    // declarations" — stated as a guess rather than presented as structure.
    let mut out = format!("{path} (no index — textual skeleton, not parsed)\n");
    for (n, line) in content.lines().enumerate() {
        let t = line.trim_start();
        let looks_declarative = ["fn ", "pub fn ", "struct ", "class ", "def ", "func ",
                                 "interface ", "type ", "impl ", "trait ", "enum "]
            .iter()
            .any(|k| t.starts_with(k) || t.starts_with(&format!("pub {k}")) || t.starts_with(&format!("export {k}")));
        if looks_declarative {
            out.push_str(&format!("L{}  {}\n", n + 1, t.trim_end()));
        }
    }
    Ok(Answer::from_ripgrep(&format!("outline {path}"), out))
}

pub fn symbol(paths: &Paths, name: &str) -> Result<Answer> {
    let hits = grep(paths, &["--word-regexp"], name)?;
    let text = if hits.trim().is_empty() {
        format!("no textual match for `{name}`\n")
    } else {
        let lines: Vec<&str> = hits.lines().take(40).collect();
        format!(
            "`{name}` — {} textual match(es), unparsed\n{}\n",
            hits.lines().count(),
            lines.join("\n")
        )
    };
    Ok(Answer::from_ripgrep(&format!("symbol {name}"), text))
}

pub fn refs(paths: &Paths, name: &str) -> Result<Answer> {
    let hits = grep(paths, &["--word-regexp", "--count"], name)?;
    let mut out = String::new();
    let mut files = 0;
    for line in hits.lines() {
        if let Some((path, count)) = line.rsplit_once(':') {
            files += 1;
            out.push_str(&format!("{}  ×{count}\n", path.trim_start_matches("./")));
        }
    }
    if files == 0 {
        out.push_str(&format!("no textual use of `{name}`\n"));
    }
    Ok(Answer::from_ripgrep(
        &format!("refs {name}"),
        format!("`{name}` across {files} file(s), unparsed\n{out}"),
    ))
}

pub fn importers(paths: &Paths, path: &str) -> Result<Answer> {
    // Without the import graph, the best available proxy is "files that mention
    // this path or its stem".
    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path);
    let hits = grep(paths, &["--word-regexp"], stem)?;
    let mut files: Vec<String> = hits
        .lines()
        .filter_map(|l| l.split(':').next().map(|p| p.trim_start_matches("./").to_string()))
        .filter(|p| p != path)
        .collect();
    files.sort();
    files.dedup();
    Ok(Answer::from_ripgrep(
        &format!("importers {path}"),
        format!(
            "{} file(s) mention `{stem}` (textual, not the import graph)\n{}\n",
            files.len(),
            files.join("\n")
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn tmp_repo() -> Paths {
        static C: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "keel-fallback-{}-{}",
            std::process::id(),
            C.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src/api.rs"),
            "pub fn serve() {}\nstruct Router;\nfn helper() { serve(); }\n",
        )
        .unwrap();
        std::fs::write(dir.join("src/main.rs"), "fn main() { api::serve(); }\n").unwrap();
        Paths { repo: dir }
    }

    #[test]
    fn outline_without_a_grammar_says_it_is_a_guess() {
        let p = tmp_repo();
        let a = outline(&p, "src/api.rs").unwrap();
        assert_eq!(a.source, super::super::Source::Ripgrep);
        assert!(a.text.contains("not parsed"), "the answer does not admit it is textual");
        assert!(a.text.contains("pub fn serve"), "{}", a.text);
        assert!(a.text.contains("struct Router"), "{}", a.text);
        let _ = std::fs::remove_dir_all(&p.repo);
    }

    #[test]
    fn outline_of_a_missing_file_is_an_error_not_an_empty_answer() {
        let p = tmp_repo();
        assert!(outline(&p, "src/nope.rs").is_err());
        let _ = std::fs::remove_dir_all(&p.repo);
    }

    #[test]
    fn symbol_falls_through_to_text_and_labels_itself() {
        let p = tmp_repo();
        let a = symbol(&p, "serve").unwrap();
        assert_eq!(a.source, super::super::Source::Ripgrep);
        assert!(a.text.contains("unparsed"), "{}", a.text);
        assert!(a.text.contains("api.rs"), "{}", a.text);
        let _ = std::fs::remove_dir_all(&p.repo);
    }

    #[test]
    fn a_name_with_no_matches_is_reported_not_faked() {
        let p = tmp_repo();
        let a = symbol(&p, "NoSuchThingAnywhere").unwrap();
        assert!(a.text.contains("no textual match"), "{}", a.text);
        let _ = std::fs::remove_dir_all(&p.repo);
    }

    #[test]
    fn refs_counts_files_textually() {
        let p = tmp_repo();
        let a = refs(&p, "serve").unwrap();
        assert!(a.text.contains("unparsed"), "{}", a.text);
        assert!(a.text.contains("api.rs"), "{}", a.text);
        let _ = std::fs::remove_dir_all(&p.repo);
    }
}
