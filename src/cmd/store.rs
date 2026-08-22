//! `keel store render|check|reconcile`.

use crate::config::Config;
use crate::paths::Paths;
use crate::projection::{self, drift};
use crate::store::frontmatter::FrontMatter;
use crate::store::{self, StoreDoc, today};
use anyhow::{Result, bail};

pub fn render(dry_run: bool, only: Option<String>) -> Result<()> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let store_hash = store::store_hash_with_shared(&paths, &cfg)?;

    let mut any = false;
    for adapter in projection::enabled_adapters(&cfg) {
        if only.as_ref().is_some_and(|id| &adapter.id != id) {
            continue;
        }
        any = true;
        let rendered = projection::render(&paths, &cfg, adapter)?;

        // A `Drift` file is a human's work. Refuse to silently overwrite it.
        let existing = drift::check_adapter(&paths, adapter, &store_hash)?;
        if matches!(existing.state, drift::State::Drift | drift::State::Foreign) && !dry_run {
            println!(
                "  {:<8} SKIPPED {} — {} ({})",
                adapter.id, adapter.out, existing.state.glyph(), existing.detail
            );
            println!("           run `keel store reconcile {}` to capture the edit first", adapter.out);
            continue;
        }

        let flag = if rendered.trimmed { " (trimmed to budget)" } else { "" };
        if dry_run {
            println!("  {:<8} would write {} — {}/{} lines{}",
                adapter.id, adapter.out, rendered.lines, rendered.budget, flag);
        } else {
            projection::write(&rendered, &store_hash)?;
            println!("  {:<8} {} — {}/{} lines{}",
                adapter.id, adapter.out, rendered.lines, rendered.budget, flag);
        }
    }
    if !any {
        if let Some(id) = only {
            bail!("no enabled adapter with id `{id}`");
        }
        println!("  no adapters enabled");
    }
    Ok(())
}

/// Exit code 1 on any blocking state, so this works unchanged as a hook and,
/// in Phase 1, as a G0 check.
pub fn check(json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let store_hash = store::store_hash_with_shared(&paths, &cfg)?;
    let reports = drift::check_all(&paths, &cfg, &store_hash)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "schema": "keel.storecheck/1",
            "store": crate::hashing::short(&store_hash),
            "projections": reports,
        }))?);
    } else {
        for r in &reports {
            let budget = match r.lines {
                Some(l) => format!(" [{}/{}]", l, r.budget),
                None => String::new(),
            };
            let over = if r.over_budget { " OVER BUDGET" } else { "" };
            println!("  {:<8} {:<10} {}{}{}", r.state.glyph(), r.adapter, r.path, budget, over);
            if r.state != drift::State::Ok {
                println!("           {}", r.detail);
            }
        }
    }

    let blocking = reports.iter().any(|r| r.state.is_blocking() || r.over_budget);
    if blocking && !json {
        let drifted: Vec<&str> = reports.iter()
            .filter(|r| matches!(r.state, drift::State::Drift | drift::State::Foreign))
            .map(|r| r.path.as_str())
            .collect();
        let renderable = reports.iter().any(|r| {
            matches!(r.state, drift::State::Stale | drift::State::Missing) || r.over_budget
        });
        println!();
        // Reconcile first: it is the step that cannot be undone by rendering.
        if !drifted.is_empty() {
            println!("  fix: keel store reconcile {}", drifted.join(" "));
        }
        if renderable {
            println!("  fix: keel store render");
        }
    }
    Ok(if blocking { 1 } else { 0 })
}

/// Capture a hand-edit out of a generated file and back into the store, then
/// restore the projection. The edit lands in `store/inbox/` rather than being
/// merged automatically: a projection cannot be reverse-mapped to its sources,
/// and guessing would lose the very content this exists to protect.
pub fn reconcile(targets: Vec<String>) -> Result<()> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let store_hash = store::store_hash_with_shared(&paths, &cfg)?;

    let selected: Vec<_> = if targets.is_empty() {
        cfg.adapters.iter().filter(|a| a.enabled).collect()
    } else {
        let mut v = Vec::new();
        for t in &targets {
            let found = cfg.adapters.iter()
                .find(|a| &a.id == t || &a.out == t || a.out.ends_with(t.trim_start_matches("./")));
            match found {
                Some(a) => v.push(a),
                None => bail!("`{t}` is not a known adapter id or output path"),
            }
        }
        v
    };

    let mut captured = 0;
    for adapter in selected {
        let report = drift::check_adapter(&paths, adapter, &store_hash)?;
        if !matches!(report.state, drift::State::Drift | drift::State::Foreign) {
            if !targets.is_empty() {
                println!("  {:<8} nothing to reconcile ({})", adapter.id, report.state.glyph());
            }
            continue;
        }

        let path = paths.repo.join(&adapter.out);
        let content = std::fs::read_to_string(&path)?;
        let body = drift::parse(&content).map(|(_, b)| b.to_string()).unwrap_or(content);

        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let inbox = paths.inbox().join(format!("{}-{}.md", adapter.id, stamp));
        let front = FrontMatter {
            id: Some(format!("INBOX-{}-{}", adapter.id, stamp)),
            scope: Some("repo".into()),
            owner: Some("human".into()),
            verified_at: Some(today()),
            sources: vec![adapter.out.clone()],
            ..Default::default()
        };
        let note = format!(
            "# Reconciled from `{}`\n\n\
             This is the content that was found in a generated projection. keel cannot\n\
             know which steering file it belongs in, so it is parked here. Fold the parts\n\
             worth keeping into `.keel/store/steering/`, then delete this file.\n\n\
             ---\n\n{}\n",
            adapter.out, body.trim_end()
        );
        StoreDoc::write(&inbox, &front, &note)?;
        println!("  {:<8} captured {} → {}", adapter.id, adapter.out, paths.rel(&inbox).display());
        captured += 1;

        let rendered = projection::render(&paths, &cfg, adapter)?;
        projection::write(&rendered, &store_hash)?;
        println!("           restored {} from the store", adapter.out);
    }

    if captured == 0 {
        println!("  nothing to reconcile");
    } else {
        println!(
            "\n  {} edit{} parked in {}. Fold them into steering, then `keel store render`.",
            captured,
            if captured == 1 { "" } else { "s" },
            paths.rel(&paths.inbox()).display()
        );
    }
    Ok(())
}
