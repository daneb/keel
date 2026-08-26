//! `keel init` — scaffold `.keel/`, seed steering, build the first map.
//!
//! The interview is deliberately two questions long. A long interview produces
//! a beautiful `product.md` that nobody maintains; two questions plus detected
//! facts produces a file the human will actually correct.

use crate::config::Config;
use crate::paths::Paths;
use crate::store::frontmatter::FrontMatter;
use crate::store::{StoreDoc, today};
use anyhow::{Result, bail};
use std::io::{IsTerminal, Write};
use std::path::Path;

pub fn run(force: bool, assume_yes: bool) -> Result<()> {
    let paths = Paths::discover()?;
    if paths.keel().exists() && !force {
        bail!(
            "{} already exists — pass --force to re-scaffold (existing store files are kept)",
            paths.rel(&paths.keel()).display()
        );
    }
    paths.scaffold()?;

    let detected = detect_stack(&paths.repo);

    let cfg_path = paths.config();
    let is_new_config = !cfg_path.exists();
    if is_new_config {
        Config::for_stack(&detected).save(&cfg_path)?;
        println!("  created {}", paths.rel(&cfg_path).display());
    }

    // Drivers ship as reference scripts embedded in the binary, not as files
    // `init` writes once and forgets. A fresh config already has claude-code's
    // [[driver]] entry from Config::default; scaffold writes the *script* that
    // entry points at, and appends entries for the others so `--driver kiro`
    // is a flag away instead of a hand-edited TOML block. It edits keel.toml
    // by appending text, not by reloading and re-saving Config, which would
    // otherwise be indistinguishable from is_new_config below and would strip
    // any comment already in the file.
    let cfg = Config::load(&cfg_path)?;
    for line in crate::driver::scaffold(&paths, &cfg, false)? {
        println!("{line}");
    }

    let interactive = !assume_yes && std::io::stdin().is_terminal();

    let purpose = if interactive {
        ask("What is this repository for, in one sentence?")?
    } else {
        String::new()
    };
    let users = if interactive {
        ask("Who uses it, and what breaks for them if it is wrong?")?
    } else {
        String::new()
    };

    seed(&paths.product(), "PROD-0001", "human", &product_md(&purpose, &users))?;
    seed(&paths.tech(), "TECH-0001", "human", &tech_md(&detected))?;
    seed(&paths.conventions(), "CONV-0001", "human", CONVENTIONS_MD)?;
    seed_gitignore(&paths)?;

    println!("\n  store seeded at {}", paths.rel(&paths.store()).display());
    if !detected.is_empty() {
        println!("  detected: {}", detected.join(", "));
    }

    let cfg = Config::load(&cfg_path)?;
    println!("\nBuilding the first map…");
    let report = crate::map::build(&paths, &cfg, None)?;
    crate::cmd::map::print_report(&report);

    println!("\nRendering projections…");
    crate::cmd::store::render(false, None)?;

    println!(
        "\nkeel is initialised.\n\n\
         Next:\n  \
         1. Correct {} and {} — they are seeded, not right.\n  \
         2. `keel hook install` so a drifted projection cannot be committed.\n  \
         3. `keel status` any time you want to know if the picture is current.\n",
        paths.rel(&paths.product()).display(),
        paths.rel(&paths.tech()).display(),
    );
    Ok(())
}

/// Write a store file only if it is absent: `--force` re-scaffolds the layout,
/// never the human's words.
fn seed(path: &Path, id: &str, owner: &str, body: &str) -> Result<()> {
    if path.exists() {
        println!("  kept    {}", path.file_name().unwrap().to_string_lossy());
        return Ok(());
    }
    let front = FrontMatter {
        id: Some(id.to_string()),
        scope: Some("repo".into()),
        owner: Some(owner.to_string()),
        verified_at: Some(today()),
        ..Default::default()
    };
    StoreDoc::write(path, &front, body)?;
    println!("  created {}", path.file_name().unwrap().to_string_lossy());
    Ok(())
}

fn ask(question: &str) -> Result<String> {
    print!("{question}\n> ");
    std::io::stdout().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    Ok(buf.trim().to_string())
}

fn detect_stack(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let markers: &[(&str, &str)] = &[
        ("Cargo.toml", "Rust (Cargo)"),
        ("bun.lock", "Bun/TypeScript"),
        ("bun.lockb", "Bun/TypeScript"),
        ("package.json", "Node/TypeScript (npm)"),
        ("pyproject.toml", "Python (pyproject)"),
        ("requirements.txt", "Python (requirements)"),
        ("go.mod", "Go (modules)"),
        ("pom.xml", "Java (Maven)"),
        ("build.gradle", "Java (Gradle)"),
        ("build.gradle.kts", "Java/Kotlin (Gradle)"),
        ("Dockerfile", "Docker"),
        ("Makefile", "Make"),
        (".github/workflows", "GitHub Actions"),
    ];
    for (file, label) in markers {
        if root.join(file).exists() && !found.iter().any(|f: &String| f == label) {
            // A Bun repo has a package.json too; the lockfile is the stronger
            // signal and is listed first, so do not also claim npm.
            if label.starts_with("Node/") && found.iter().any(|f| f.starts_with("Bun/")) {
                continue;
            }
            found.push(label.to_string());
        }
    }
    // A .sln or .csproj is named after the project, not after itself, so it
    // needs a scan rather than a fixed filename.
    if let Ok(entries) = std::fs::read_dir(root)
        && entries.flatten().any(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.ends_with(".sln") || n.ends_with(".csproj")
        })
    {
        found.push("C# (.NET)".to_string());
    }
    found
}

fn product_md(purpose: &str, users: &str) -> String {
    let purpose = if purpose.is_empty() {
        "_One sentence: what does this repository do? Replace this line._"
    } else {
        purpose
    };
    let users = if users.is_empty() {
        "_Who depends on it, and what goes wrong for them when it misbehaves?_"
    } else {
        users
    };
    format!(
        "# Product\n\n\
         ## Purpose\n\n{purpose}\n\n\
         ## Users and stakes\n\n{users}\n\n\
         ## Out of scope\n\n\
         _Name the things this repository deliberately does not do. \
         This section stops more wasted work than the two above combined._\n"
    )
}

fn tech_md(detected: &[String]) -> String {
    let stack = if detected.is_empty() {
        "_Not auto-detected. Name the language, framework and versions._".to_string()
    } else {
        detected.iter().map(|d| format!("- {d}")).collect::<Vec<_>>().join("\n")
    };
    format!(
        "# Tech\n\n\
         ## Stack\n\n{stack}\n\n\
         ## Build and test\n\n\
         _The exact commands. These become gate oracles in Phase 1, so write the\n\
         command you would actually run, not a description of it._\n\n\
         ```sh\n# build:\n# test:\n# lint:\n```\n\n\
         ## Constraints\n\n\
         _Versions that cannot move, platforms that must keep working, \
         dependencies that are not negotiable._\n"
    )
}

const CONVENTIONS_MD: &str = "\
# Conventions

House rules that apply to every change in this repository. Keep this list short
and specific: a rule nobody can violate mechanically is a rule that will be
violated. Where a rule can be checked by a command, say so — in Phase 3 those
become gate checks and stop costing context.

## Working agreement

- Match the surrounding code. Naming, comment density and idiom are local
  conventions, not global ones.
- Change the smallest surface that solves the problem. If a fix needs a wider
  blast radius, say so before making it, not after.
- A test that mocks away the behaviour under test is worse than no test.

## Rules

_Add rules as you find yourself repeating them. One line each, imperative mood._
";

fn seed_gitignore(paths: &Paths) -> Result<()> {
    let path = paths.repo.join(".gitignore");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    // Each block is seeded independently so a repo that already has an older
    // subset of these blocks (from an earlier `keel init`) still picks up the
    // ones it is missing, without duplicating the ones it already has.
    let blocks: &[&[&str]] = &[
        &[
            "# keel: the symbol index is a build artefact, rebuilt by `keel map`",
            ".keel/store/map/index.sqlite",
            ".keel/store/map/*.sqlite-*",
            ".keel/store/map/index.sqlite.tmp",
        ],
        &[
            "# Evidence bundles are regenerated from runs by `keel export`.",
            ".keel/bundles/",
        ],
        &[
            "# Transient: re-proposed by `keel learn` from the runs themselves.",
            ".keel/candidates.json",
        ],
        &[
            "# Worktrees are created and removed per wave; git tracks them itself.",
            ".keel/worktrees/",
        ],
        &[
            "# The manifest schema is regenerated by `keel export`.",
            ".keel/schemas/",
        ],
    ];

    let mut out = existing.clone();
    let mut changed = false;
    for block in blocks {
        // The first non-comment line is the block's sentinel: if it is
        // already present, treat the whole block as already seeded.
        let sentinel = block.iter().find(|l| !l.starts_with('#')).expect("block has a pattern line");
        if existing.contains(sentinel) {
            continue;
        }
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&block.join("\n"));
        out.push('\n');
        changed = true;
    }
    if !changed {
        return Ok(());
    }
    std::fs::write(&path, out)?;
    println!("  updated .gitignore");
    Ok(())
}

#[cfg(test)]
mod gitignore_tests {
    use super::*;

    fn temp_paths(name: &str) -> Paths {
        let dir = std::env::temp_dir().join(format!("keel-init-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Paths { repo: dir }
    }

    #[test]
    fn seeds_every_block_and_stays_stable_on_a_second_run() {
        let paths = temp_paths("fresh");
        seed_gitignore(&paths).unwrap();
        let first = std::fs::read_to_string(paths.repo.join(".gitignore")).unwrap();
        for needle in [
            ".keel/store/map/index.sqlite",
            ".keel/bundles/",
            ".keel/candidates.json",
            ".keel/worktrees/",
            ".keel/schemas/",
        ] {
            assert!(first.contains(needle), "missing {needle} after first seed:\n{first}");
        }

        seed_gitignore(&paths).unwrap();
        let second = std::fs::read_to_string(paths.repo.join(".gitignore")).unwrap();
        assert_eq!(first, second, "a second `keel init` duplicated .gitignore entries");
    }

    #[test]
    fn upgrades_a_repo_seeded_before_the_later_blocks_existed() {
        let paths = temp_paths("upgrade");
        let path = paths.repo.join(".gitignore");
        std::fs::write(
            &path,
            "# keel: the symbol index is a build artefact, rebuilt by `keel map`\n\
             .keel/store/map/index.sqlite\n\
             .keel/store/map/*.sqlite-*\n\
             .keel/store/map/index.sqlite.tmp\n",
        )
        .unwrap();

        seed_gitignore(&paths).unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        assert_eq!(out.matches(".keel/store/map/index.sqlite\n").count(), 1, "duplicated the old block");
        for needle in [".keel/bundles/", ".keel/candidates.json", ".keel/worktrees/", ".keel/schemas/"] {
            assert!(out.contains(needle), "upgrade did not add {needle}:\n{out}");
        }

        let after_first_upgrade = out;
        seed_gitignore(&paths).unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after_first_upgrade, out, "re-running the upgrade duplicated entries");
    }
}
