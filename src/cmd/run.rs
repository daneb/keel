//! `keel run` — execute a task through an agent, capture everything, gate it.
//!
//! The order matters: the run directory and its trajectory exist *before* the
//! driver is invoked, so a driver that hangs, crashes or lies still leaves a
//! record. A run you can only reconstruct when it succeeded is not evidence.

use crate::config::Config;
use crate::driver::{self, DriverStatus, DriverTask};
use crate::gate::{self, Verdict};
use crate::paths::Paths;
use crate::plan::{Plan, Tasks};
use crate::run::Run;
use crate::spec::Spec;
use crate::store::{self, StoreDoc};
use crate::trajectory::{Payload, Trajectory, event::estimate_tokens};
use anyhow::{Result, bail};
use std::time::Instant;

pub struct Options {
    pub slug: Option<String>,
    pub task: Option<String>,
    pub driver: Option<String>,
    /// Gate an existing working tree instead of invoking an agent.
    pub no_driver: bool,
    pub json: bool,
}

pub fn run(opts: Options) -> Result<i32> {
    let started = Instant::now();
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let slug = crate::cmd::gate::resolve_slug(&paths, opts.slug)?;
    let spec = Spec::load(&paths, &slug)?;
    let plan = Plan::load(&paths, &slug).ok();
    let tasks = Tasks::load(&paths, &slug).ok();

    // A run against an unbuildable spec produces evidence of nothing.
    match gate::previous(&paths, &slug, "G1") {
        Some(r) if r.verdict == Verdict::Pass => {}
        Some(r) => bail!(
            "G1 is {} for `{slug}` — fix the plan before running (`keel gate g1 {slug}`)",
            r.verdict.glyph()
        ),
        None => bail!("G1 has not run for `{slug}` — run `keel gate g1 {slug}` first"),
    }

    let store_hash = store::store_hash(&paths)?;
    let selected_driver = if opts.no_driver {
        None
    } else {
        Some(driver::select(&cfg, opts.driver.as_deref())?)
    };

    let mut run = Run::create(
        &paths,
        &slug,
        opts.task.clone(),
        selected_driver.map(|d| d.id.clone()),
        &store_hash,
    )?;
    let mut traj = run.open_trajectory()?;
    println!("run {}\n", run.meta.id);

    traj.append(Payload::RunStart {
        spec: slug.clone(),
        task: opts.task.clone(),
        driver: selected_driver.map(|d| d.id.clone()),
        keel_version: env!("CARGO_PKG_VERSION").to_string(),
        store_hash: store_hash.clone(),
    })?;

    // --- context, recorded as it is assembled --------------------------------
    let prompt = build_prompt(&paths, &spec, tasks.as_ref(), opts.task.as_deref(), &mut traj)?;

    // --- the agent -----------------------------------------------------------
    if let Some(d) = selected_driver {
        let task = DriverTask::new(
            &run.meta.id,
            &slug,
            opts.task.clone(),
            prompt,
            spec.front.scope.clone(),
            spec.front.budget.lines,
            paths.repo.to_string_lossy().to_string(),
        );
        traj.append(Payload::DriverCall {
            driver: d.id.clone(),
            task: opts.task.clone(),
            prompt_tokens: estimate_tokens(&task.prompt),
        })?;

        println!("  driver {} …", d.id);
        let inv = driver::run(&paths, d, &task);
        traj.append(Payload::DriverResult {
            driver: d.id.clone(),
            status: inv.result.status_str().to_string(),
            files_changed: Some(inv.result.files_changed.len()),
            detail: inv.result.detail.clone(),
        })?;
        run.write_evidence(
            "driver.json",
            &serde_json::to_string_pretty(&inv.result)?,
        )?;
        if !inv.stderr.is_empty() {
            run.write_evidence("driver-stderr.txt", &inv.stderr)?;
        }
        println!(
            "  driver {} in {:.1}s{}",
            inv.result.status_str(),
            inv.elapsed.as_secs_f64(),
            inv.result.detail.as_ref().map(|d| format!(" — {d}")).unwrap_or_default()
        );

        if inv.result.status == DriverStatus::Blocked {
            // Blocked is not failed. Record it, stop, and do not pretend the
            // gates said anything about work that never happened.
            traj.append(Payload::RunEnd {
                verdict: "blocked".into(),
                duration_ms: started.elapsed().as_millis() as u64,
            })?;
            run.finish("blocked")?;
            println!("\nrun BLOCKED — the driver could not run; the gates did not execute");
            return Ok(Verdict::Blocked.exit_code());
        }
    } else {
        println!("  no driver (--no-driver): gating the working tree as it stands");
    }

    // --- the gates -----------------------------------------------------------
    let mut verdicts = Vec::new();
    for name in ["G2", "G2.5", "G3"] {
        let result = match name {
            "G2" => gate::g2::run(&paths, &cfg, &spec, plan.as_ref(), &run, &mut traj)?,
            "G2.5" => gate::g25::run(&paths, &cfg, &spec, &run)?,
            _ => gate::g3::run(&paths, &cfg, &spec, &run)?,
        };
        let path = result.write(&run.gates_dir())?;
        traj.append(Payload::Gate {
            gate: result.gate.clone(),
            verdict: result.verdict.glyph().to_lowercase(),
            result: format!("gates/{}.json", result.gate),
        })?;

        if !opts.json {
            println!("\n{} — {}", result.gate, slug);
            for c in &result.checks {
                println!("{}", c.line());
            }
            let (p, f, b) = result.counts();
            println!("{} {} — {p} passed, {f} failed, {b} blocked", result.gate, result.verdict.glyph());
        }
        let _ = path;
        verdicts.push(result.verdict);

        // G3 asks a human; there is no point asking once G2 has failed.
        if result.verdict == Verdict::Fail && name != "G3" {
            println!("\nstopping: {name} failed, so later gates would be judging work that is not ready");
            break;
        }
    }

    let overall = if verdicts.contains(&Verdict::Fail) {
        Verdict::Fail
    } else if verdicts.contains(&Verdict::Blocked) {
        Verdict::Blocked
    } else {
        Verdict::Pass
    };

    traj.append(Payload::RunEnd {
        verdict: overall.glyph().to_lowercase(),
        duration_ms: started.elapsed().as_millis() as u64,
    })?;
    run.finish(&overall.glyph().to_lowercase())?;

    if opts.json {
        println!("{}", serde_json::json!({
            "run": run.meta.id,
            "spec": slug,
            "verdict": overall.glyph().to_lowercase(),
            "gates": run.gate_results()?.iter().map(|r| {
                serde_json::json!({ "gate": r.gate, "verdict": r.verdict })
            }).collect::<Vec<_>>(),
        }));
    } else {
        println!("\nrun {} — {}", run.meta.id, overall.glyph());
        println!(
            "recorded: {} events in {}",
            traj.next_seq().saturating_sub(1),
            paths.rel(&run.trajectory_path()).display()
        );
        println!("evidence: {}", paths.rel(&run.dir).display());
        println!("bundle:   keel export {}", run.meta.id);
    }
    Ok(overall.exit_code())
}

/// Assemble the instruction, recording every injection as it happens.
///
/// P5's invariant is that anything reaching the model is reconstructable from
/// the stream — which means the injections have to be recorded *here*, as the
/// prompt is built, not summarised afterwards.
fn build_prompt(
    paths: &Paths,
    spec: &Spec,
    tasks: Option<&Tasks>,
    task_id: Option<&str>,
    traj: &mut Trajectory,
) -> Result<String> {
    let mut prompt = String::new();
    prompt.push_str(&format!(
        "Implement the task below in {}.\n\nFollow the house rules. Stay inside the declared scope.\n\n",
        paths.repo.display()
    ));

    let inject = |traj: &mut Trajectory, label: &str, source: &str, body: &str, prompt: &mut String| -> Result<()> {
        if body.trim().is_empty() {
            return Ok(());
        }
        prompt.push_str(&format!("## {label}\n\n{}\n\n", body.trim()));
        traj.append(Payload::Inject {
            source: source.to_string(),
            tokens: estimate_tokens(body),
            bytes: Some(body.len()),
        })?;
        Ok(())
    };

    for (label, path) in [
        ("House rules", paths.conventions()),
        ("Stack and constraints", paths.tech()),
        ("Repository map", paths.structure()),
    ] {
        if let Some(doc) = StoreDoc::read_optional(&path)? {
            let rel = paths.rel(&path).to_string_lossy().to_string();
            inject(traj, label, &rel, doc.body_without_title(), &mut prompt)?;
        }
    }

    // Lessons are injected by keel, selected by scope and stage — never left
    // for the agent to find. Documentation was the first recovery move in only
    // 5.4% of observed failure episodes, so a lesson on a shelf is unread.
    //
    // A lesson that compiles into a gate check is deliberately *not* injected:
    // it is already enforced, and injecting it would spend context re-stating
    // something that cannot be violated without failing G2.
    let lessons = crate::lesson::list(paths)?;
    let selected = crate::lesson::for_injection(&lessons, "implement", &spec.front.scope);
    let mut ledger = crate::lesson::usage::Ledger::load(paths)?;
    for lesson in &selected {
        let rel = paths.rel(&lesson.path).to_string_lossy().to_string();
        inject(
            traj,
            &format!("Lesson {} ({})", lesson.front.id, lesson.front.scope),
            &rel,
            &lesson.body,
            &mut prompt,
        )?;
        ledger.record_injection(&lesson.front.id);
    }
    ledger.save(paths)?;

    // The spec itself.
    let spec_path = Spec::path_for(paths, &spec.front.slug);
    let spec_body = std::fs::read_to_string(&spec_path)?;
    inject(
        traj,
        "Specification",
        &paths.rel(&spec_path).to_string_lossy(),
        &spec_body,
        &mut prompt,
    )?;

    // The task, if one was named.
    if let (Some(tasks), Some(id)) = (tasks, task_id) {
        let Some(t) = tasks.tasks.iter().find(|t| t.id == id) else {
            bail!("no task `{id}` in tasks.md");
        };
        let body = format!(
            "**{} {}**\n\n- criteria: {}\n- files: {}\n- budget: {} lines\n- done when: {}\n",
            t.id,
            t.title,
            t.criteria.join(", "),
            t.files.join(", "),
            t.budget.unwrap_or(0),
            t.exit.clone().unwrap_or_default()
        );
        inject(traj, "Task", &format!("tasks.md#{id}"), &body, &mut prompt)?;
    }

    Ok(prompt)
}

/// `keel replay <run>` — print a run's stream in sequence order.
pub fn replay(id: Option<String>, json: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    let id = crate::run::resolve(&paths, id)?;
    let run = Run::load(&paths, &id)?;
    let events = crate::trajectory::read(&run.trajectory_path())?;

    for e in &events {
        if json {
            println!("{}", e.one_line()?);
        } else {
            println!("{}", e.summary());
        }
    }
    if !json {
        println!(
            "\n{} events · {} tokens injected · gates: {}",
            events.len(),
            crate::trajectory::token_total(&events),
            crate::trajectory::gate_verdicts(&events)
                .iter()
                .map(|(g, v)| format!("{g} {v}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(0)
}

/// `keel runs` — what has been run.
pub fn list(latest_only: bool) -> Result<i32> {
    let paths = Paths::require_init()?;
    if latest_only {
        match crate::run::latest(&paths)? {
            Some(id) => println!("{id}"),
            None => bail!("no runs yet"),
        }
        return Ok(0);
    }
    let ids = crate::run::list(&paths)?;
    if ids.is_empty() {
        println!("  no runs yet — `keel run <spec>`");
        return Ok(0);
    }
    for id in ids {
        let r = Run::load(&paths, &id)?;
        println!(
            "  {:<20} {:<20} {:<8} {}",
            r.meta.id,
            r.meta.spec,
            r.meta.verdict.clone().unwrap_or_else(|| "…".into()),
            r.meta.driver.clone().unwrap_or_else(|| "-".into())
        );
    }
    Ok(0)
}

/// `keel export <run>` and `keel export --verify <bundle>`.
pub fn export(target: Option<String>, verify: Option<String>, out: Option<String>) -> Result<i32> {
    let paths = Paths::require_init()?;

    if let Some(archive) = verify {
        let path = std::path::PathBuf::from(&archive);
        let v = crate::evidence::verify(&path)?;
        if v.is_intact() {
            println!(
                "  intact — {} member(s), run {}, spec {}",
                v.manifest.members.len(),
                v.manifest.run,
                v.manifest.spec
            );
            return Ok(0);
        }
        for m in &v.tampered {
            println!("  TAMPERED  {m}");
        }
        for m in &v.missing {
            println!("  MISSING   {m}");
        }
        for m in &v.unlisted {
            println!("  UNLISTED  {m}");
        }
        bail!("{} does not match its manifest", path.display());
    }

    let id = crate::run::resolve(&paths, target)?;
    let run = Run::load(&paths, &id)?;
    crate::evidence::write_schema(&paths)?;
    let archive = crate::evidence::export(&paths, &run, out.as_deref().map(std::path::Path::new))?;
    // stdout is the path and nothing else, so it composes with other tools.
    println!("{}", archive.display());
    Ok(0)
}
