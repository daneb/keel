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

    let cfg_path = paths.config();
    if !cfg_path.exists() {
        Config::default().save(&cfg_path)?;
        println!("  created {}", paths.rel(&cfg_path).display());
    }

    let detected = detect_stack(&paths.repo);
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
        if root.join(file).exists() {
            found.push(label.to_string());
        }
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
    let entries = [
        "# keel: the symbol index is a build artefact, rebuilt by `keel map`",
        ".keel/store/map/index.sqlite",
        ".keel/store/map/*.sqlite-*",
        ".keel/store/map/index.sqlite.tmp",
    ];
    if existing.contains(".keel/store/map/index.sqlite") {
        return Ok(());
    }
    let mut out = existing;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&entries.join("\n"));
    out.push('\n');
    std::fs::write(&path, out)?;
    println!("  updated .gitignore");
    Ok(())
}
