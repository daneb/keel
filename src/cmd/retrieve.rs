//! `keel outline|symbol|source|refs|importers|slice|bench`.

use crate::config::Config;
use crate::paths::Paths;
use crate::retrieve::{Answer, Retriever, budget};
use anyhow::Result;

fn emit(a: Answer, degraded: Option<&String>, json: bool) -> Result<i32> {
    if json {
        println!("{}", serde_json::to_string_pretty(&a)?);
        return Ok(0);
    }
    if let Some(why) = degraded {
        eprintln!("keel: {why} — answering textually\n");
    }
    print!("{}", a.text);
    if !a.text.ends_with('\n') {
        println!();
    }
    eprintln!("\n[{:?} · {} tokens{}]", a.source, a.tokens, if a.truncated { " · truncated" } else { "" });
    Ok(0)
}

fn open() -> Result<(Paths, Config, Retriever)> {
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let r = Retriever::open(&paths)?;
    Ok((paths, cfg, r))
}

pub fn outline(path: String, json: bool) -> Result<i32> {
    let (_, cfg, r) = open()?;
    let a = r.outline(&path)?.fit(budget::for_stage(&cfg, "outline"));
    emit(a, r.degraded.as_ref(), json)
}

pub fn symbol(name: String, json: bool) -> Result<i32> {
    let (_, cfg, r) = open()?;
    let a = r.symbol(&name)?.fit(budget::for_stage(&cfg, "symbol"));
    emit(a, r.degraded.as_ref(), json)
}

pub fn source(name: String, nth: usize, justify: Option<String>, json: bool) -> Result<i32> {
    let (paths, cfg, r) = open()?;
    let (a, lines) = r.source(&name, nth)?;
    // Progressive disclosure has a price: pulling a body is the expensive call,
    // and a large one has to be justified on the record.
    budget::account_for_read(&paths, &cfg, &name, lines, justify.as_deref())?;
    emit(a, r.degraded.as_ref(), json)
}

pub fn refs(name: String, json: bool) -> Result<i32> {
    let (_, cfg, r) = open()?;
    let a = r.refs(&name)?.fit(budget::for_stage(&cfg, "refs"));
    emit(a, r.degraded.as_ref(), json)
}

pub fn importers(path: String, json: bool) -> Result<i32> {
    let (_, cfg, r) = open()?;
    let a = r.importers(&path)?.fit(budget::for_stage(&cfg, "importers"));
    emit(a, r.degraded.as_ref(), json)
}

pub fn slice(slug: Option<String>, task: String, json: bool) -> Result<i32> {
    let (paths, cfg, r) = open()?;
    let slug = crate::cmd::gate::resolve_slug(&paths, slug)?;
    let a = r.slice(&slug, &task, budget::for_stage(&cfg, "slice"))?;
    emit(a, r.degraded.as_ref(), json)
}
