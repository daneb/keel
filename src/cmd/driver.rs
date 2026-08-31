//! `keel driver list|check|scaffold` — the drivers available, whether they
//! conform, and getting the reference scripts onto disk in the first place.

use crate::config::Config;
use crate::driver::conform;
use crate::paths::Paths;
use anyhow::Result;

pub fn scaffold(force: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    for line in crate::driver::scaffold(&paths, &cfg, force)? {
        println!("{line}");
    }
    println!("\nrun `keel driver list` to see what is reachable, or `keel run <slug> --driver <id>`.");
    Ok(0)
}

pub fn list() -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    if cfg.drivers.is_empty() {
        println!("  no drivers configured");
        return Ok(0);
    }
    for d in &cfg.drivers {
        let bin = paths.repo.join(&d.cmd);
        let reachable = bin.is_file() || which(&d.cmd);
        println!(
            "  {:<14} {:<34} {:<8} {}",
            d.id,
            d.cmd,
            if d.default { "default" } else { "" },
            if reachable { "" } else { "NOT FOUND" }
        );
    }
    println!("\n  `keel driver check <id>` runs the conformance suite");
    Ok(0)
}

pub fn set_default(id: String) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    crate::driver::set_default(&paths, &cfg, &id)?;
    println!("default driver set to `{id}`");
    Ok(0)
}

fn which(cmd: &str) -> bool {
    let program = cmd.split_whitespace().next().unwrap_or(cmd);
    std::process::Command::new("sh")
        .args(["-c", &format!("command -v {program}")])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn check(id: Option<String>, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;

    let targets: Vec<&crate::config::Driver> = match &id {
        Some(id) => vec![
            cfg.drivers
                .iter()
                .find(|d| &d.id == id)
                .ok_or_else(|| anyhow::anyhow!("no driver `{id}` in .keel/keel.toml"))?,
        ],
        None => cfg.drivers.iter().collect(),
    };

    let mut worst = crate::gate::Verdict::Pass;
    let mut reports = Vec::new();

    for d in targets {
        let c = conform::check(&paths, d)?;
        if !json {
            println!("driver `{}` — {}\n", c.driver, c.verdict.glyph());
            for check in &c.checks {
                println!("{}", check.line());
            }
            if let Some(r) = &c.result {
                println!("\n  reported: {} — {}", r.status_str(), r.detail.clone().unwrap_or_default());
            }
            println!();
        }
        if c.verdict == crate::gate::Verdict::Fail {
            worst = crate::gate::Verdict::Fail;
        } else if c.verdict == crate::gate::Verdict::Blocked && worst == crate::gate::Verdict::Pass {
            worst = crate::gate::Verdict::Blocked;
        }
        reports.push(serde_json::json!({
            "driver": c.driver,
            "verdict": c.verdict,
            "checks": c.checks,
        }));
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&reports)?);
    } else {
        println!("conformance: {}", worst.glyph());
        if worst == crate::gate::Verdict::Blocked {
            println!("blocked is not failed: a driver keel could not reach says nothing about the contract.");
        }
    }
    Ok(worst.exit_code())
}
