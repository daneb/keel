//! `keel spec new|list|prompt`.

use crate::config::Config;
use crate::gate;
use crate::paths::Paths;
use crate::spec::{self, SPEC_SCHEMA, Spec};
use crate::store::today;
use anyhow::{Result, bail};
use std::io::Write;

pub fn new(slug: &str, title: Option<String>, scope: Vec<String>, force: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;

    if !is_slug(slug) {
        bail!("`{slug}` is not a usable slug — use lower-case words joined by hyphens");
    }
    let path = Spec::path_for(&paths, slug);
    if path.exists() && !force {
        bail!("{} already exists — pass --force to overwrite", paths.rel(&path).display());
    }

    let id = next_spec_id(&paths)?;
    let title = title.unwrap_or_else(|| humanise(slug));
    let scope = if scope.is_empty() { vec!["src/**".to_string()] } else { scope };

    let content = match &cfg.spec.cmd {
        Some(cmd) if !cmd.trim().is_empty() => {
            println!("  authoring via `{cmd}`…");
            author_with(&paths, cmd, &prompt(&id, slug, &title, &scope, &cfg))?
        }
        _ => template(&id, slug, &title, &scope, &cfg),
    };

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, content)?;
    println!("  created {}", paths.rel(&path).display());

    println!("\nRunning G0 (it will fail until you fill the criteria in — that is the point):\n");
    crate::cmd::gate::g0(Some(slug.to_string()), false)
}

pub fn list() -> Result<i32> {
    let paths = Paths::require_init()?;
    let slugs = spec::list(&paths)?;
    if slugs.is_empty() {
        println!("  no specs yet — `keel spec new <slug>`");
        return Ok(0);
    }
    for slug in &slugs {
        let s = Spec::load(&paths, slug)?;
        let g0 = gate::previous(&paths, slug, "G0").map(|r| r.verdict.glyph().to_string());
        let g1 = gate::previous(&paths, slug, "G1").map(|r| r.verdict.glyph().to_string());
        println!(
            "  {:<28} {:>2} criteria   G0 {:<8} G1 {:<8} {}",
            slug,
            s.criteria.len(),
            g0.unwrap_or_else(|| "—".into()),
            g1.unwrap_or_else(|| "—".into()),
            s.front.status
        );
    }
    Ok(0)
}

/// Print the authoring prompt, for pasting into whichever agent you like.
pub fn print_prompt(slug: &str) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let s = Spec::load(&paths, slug)?;
    println!("{}", prompt(&s.front.id, slug, &humanise(slug), &s.front.scope, &cfg));
    Ok(0)
}

fn author_with(paths: &Paths, cmd: &str, prompt: &str) -> Result<String> {
    let mut parts = cmd.split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>();
    let program = parts.remove(0);
    let mut child = std::process::Command::new(&program)
        .args(&parts)
        .current_dir(&paths.repo)
        .env("KEEL_REPO", &paths.repo)
        .env("KEEL_STORE", paths.store())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    child.stdin.as_mut().unwrap().write_all(prompt.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("spec.cmd `{cmd}` exited with {}", out.status);
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    if text.trim().is_empty() {
        bail!("spec.cmd `{cmd}` produced no output");
    }
    Ok(text)
}

/// The instruction an agent needs to write a spec that can pass G0.
///
/// It states the gate's rules explicitly rather than hoping for good taste —
/// P1 again: guardrails beat guidance, and where guidance is unavoidable it
/// should at least describe the guardrail.
fn prompt(id: &str, slug: &str, title: &str, scope: &[String], cfg: &Config) -> String {
    format!(
        "Write `.keel/specs/{slug}/spec.md` for: {title}\n\
         \n\
         Output the complete file and nothing else. It must satisfy keel's G0 gate:\n\
         \n\
         1. YAML front matter with: id: {id}, slug: {slug}, schema: {SPEC_SCHEMA},\n\
         \x20  status: draft, scope (globs this change may touch), and\n\
         \x20  budget: {{ criteria: <= {}, lines: <= {} }}.\n\
         2. At most {} acceptance criteria, each a `### AC-n <short title>` heading.\n\
         3. Each criterion states ONE requirement in EARS form — exactly one of:\n\
         \x20    THE SYSTEM SHALL <response>\n\
         \x20    WHEN <trigger> THE SYSTEM SHALL <response>\n\
         \x20    WHILE <state> THE SYSTEM SHALL <response>\n\
         \x20    IF <condition> THEN THE SYSTEM SHALL <response>\n\
         \x20    WHERE <feature> THE SYSTEM SHALL <response>\n\
         \x20  `THE SYSTEM SHALL` is upper case. Never write \"should\".\n\
         4. Each criterion is followed by at least one `oracle:` line, one of:\n\
         \x20    oracle: cmd `<shell command>` exit <code>\n\
         \x20    oracle: test <test identifier>\n\
         \x20    oracle: schema <schema path> validates <target path>\n\
         \x20    oracle: doctest <path>\n\
         \x20    oracle: human <what a reviewer must judge>\n\
         \x20  Prefer a runnable oracle. Use `human` only when nothing can be run,\n\
         \x20  and say precisely what is being judged.\n\
         5. No vague words in criteria: no \"appropriate\", \"efficient\", \"robust\",\n\
         \x20  \"handle\", \"support\", \"reasonable\", \"fast\", \"should\", \"etc\".\n\
         \x20  Name observable behaviour and concrete numbers instead.\n\
         6. Keep the whole file under {} lines.\n\
         \n\
         Declared scope: {}\n",
        cfg.spec.max_criteria,
        cfg.spec.max_lines,
        cfg.spec.max_criteria,
        cfg.spec.max_lines,
        scope.join(", ")
    )
}

fn template(id: &str, slug: &str, title: &str, scope: &[String], cfg: &Config) -> String {
    let scope_yaml: String = scope.iter().map(|s| format!("  - \"{s}\"\n")).collect();
    format!(
        "---\n\
         id: {id}\n\
         slug: {slug}\n\
         schema: {SPEC_SCHEMA}\n\
         status: draft\n\
         scope:\n{scope_yaml}\
         budget:\n\
         \x20 criteria: {}\n\
         \x20 lines: 120\n\
         verified_at: {}\n\
         ---\n\
         \n\
         # {title}\n\
         \n\
         ## Context\n\
         \n\
         _Why this change, and what is true today that should not be._\n\
         \n\
         ## Acceptance criteria\n\
         \n\
         Each criterion is one EARS sentence plus at least one runnable oracle.\n\
         G0 fails this spec until that is true of every criterion — including this one,\n\
         which is a placeholder and is supposed to fail.\n\
         \n\
         ### AC-1 Replace this with one observable behaviour\n\
         \n\
         WHEN <trigger> THE SYSTEM SHALL <observable response>.\n\
         \n\
         oracle: cmd `<command that proves it>` exit 0\n\
         \n\
         ## Out of scope\n\
         \n\
         _What this change deliberately does not do._\n",
        cfg.spec.max_criteria.min(8),
        today()
    )
}

fn next_spec_id(paths: &Paths) -> Result<String> {
    let mut max = 0usize;
    for slug in spec::list(paths)? {
        if let Ok(s) = Spec::load(paths, &slug)
            && let Some(n) = s.front.id.rsplit('-').next().and_then(|d| d.parse::<usize>().ok())
        {
            max = max.max(n);
        }
    }
    Ok(format!("SPEC-{:04}", max + 1))
}

fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

fn humanise(slug: &str) -> String {
    let mut out = slug.replace('-', " ");
    if let Some(c) = out.get_mut(0..1) {
        c.make_ascii_uppercase();
    }
    out
}
