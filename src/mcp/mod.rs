//! An MCP server over the retrieval layer (PLAN.md §4.8).
//!
//! > Exposed twice: as a CLI (for scripting and for gates) and as an MCP server
//! > (so Claude Code, Kiro and Copilot all get the same view).
//!
//! JSON-RPC 2.0 over stdio, one message per line. Every tool here is the same
//! call the CLI makes, so there is exactly one retrieval implementation and no
//! way for the two surfaces to drift apart.

use crate::config::Config;
use crate::paths::Paths;
use crate::retrieve::{Retriever, budget};
use anyhow::Result;
use serde_json::{Value, json};
use std::io::{BufRead, Write};

/// The protocol revision keel implements. A client asking for a different one
/// still gets served — the surface used here has been stable across revisions —
/// but the reply says what keel actually speaks.
const PROTOCOL_VERSION: &str = "2024-11-05";

pub fn serve() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let lines = stdin.lock().lines();

    for line in lines {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                respond(&mut stdout, error(None, -32700, &format!("parse error: {e}")))?;
                continue;
            }
        };

        // A notification has no id and must not be answered at all.
        let id = request.get("id").cloned();
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(json!({}));

        let reply = match method {
            "initialize" => Some(result(id.clone(), initialize(&params))),
            "tools/list" => Some(result(id.clone(), json!({ "tools": tool_definitions() }))),
            "tools/call" => Some(match call_tool(&params) {
                Ok(v) => result(id.clone(), v),
                // A failed tool call is a *result* carrying isError, not a
                // protocol error: the model needs to read what went wrong.
                Err(e) => result(
                    id.clone(),
                    json!({
                        "content": [{ "type": "text", "text": format!("{e:#}") }],
                        "isError": true
                    }),
                ),
            }),
            "ping" => Some(result(id.clone(), json!({}))),
            _ if id.is_none() => None, // notification: initialized, cancelled, …
            _ => Some(error(id.clone(), -32601, &format!("unknown method `{method}`"))),
        };

        if let Some(r) = reply {
            respond(&mut stdout, r)?;
        }
    }
    Ok(())
}

fn respond(out: &mut impl Write, value: Value) -> Result<()> {
    writeln!(out, "{}", serde_json::to_string(&value)?)?;
    out.flush()?;
    Ok(())
}

fn result(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": result })
}

fn error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null),
            "error": { "code": code, "message": message } })
}

fn initialize(_params: &Value) -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "keel", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Navigate by structure, not by reading files. Start with \
outline or symbol; call source only when you need a body, and expect large \
bodies to require a justification. blast_radius answers \"what else does this \
touch?\" before you edit."
    })
}

pub fn tool_definitions() -> Vec<Value> {
    let str_arg = |desc: &str| json!({ "type": "string", "description": desc });
    vec![
        json!({
            "name": "outline",
            "description": "A file's skeleton: every symbol's signature, kind, line range and doc — without any bodies. Use this before reading a file.",
            "inputSchema": { "type": "object", "required": ["path"],
                "properties": { "path": str_arg("Repo-relative file path") } }
        }),
        json!({
            "name": "symbol",
            "description": "Where a symbol is defined, with its signature and doc. Never returns the body.",
            "inputSchema": { "type": "object", "required": ["name"],
                "properties": { "name": str_arg("Symbol name, exact") } }
        }),
        json!({
            "name": "source",
            "description": "A symbol's body. The expensive call: prefer outline or symbol first. A body over the configured line limit requires `justify`.",
            "inputSchema": { "type": "object", "required": ["name"], "properties": {
                "name": str_arg("Symbol name, exact"),
                "nth": { "type": "integer", "description": "Which definition, when the name is defined more than once (default 1)" },
                "justify": str_arg("Why the whole body is needed; recorded in the run trajectory")
            } }
        }),
        json!({
            "name": "refs",
            "description": "Every file that uses a symbol, with use counts.",
            "inputSchema": { "type": "object", "required": ["name"],
                "properties": { "name": str_arg("Symbol name, exact") } }
        }),
        json!({
            "name": "importers",
            "description": "Which files import a given path.",
            "inputSchema": { "type": "object", "required": ["path"],
                "properties": { "path": str_arg("Repo-relative file path") } }
        }),
        json!({
            "name": "blast_radius",
            "description": "What else a change touches: the files reachable from a set of path globs by walking imports backwards.",
            "inputSchema": { "type": "object", "required": ["scope"], "properties": {
                "scope": { "type": "array", "items": { "type": "string" },
                           "description": "Path globs, e.g. src/api/**" },
                "depth": { "type": "integer", "description": "Hops to walk (default 2)" }
            } }
        }),
        json!({
            "name": "slice",
            "description": "Everything one task needs and nothing else: its criteria, the outline of each file it touches, its downstream impact and its budget.",
            "inputSchema": { "type": "object", "required": ["task"], "properties": {
                "task": str_arg("Task id, e.g. T-1"),
                "slug": str_arg("Spec slug; optional when there is only one spec")
            } }
        }),
    ]
}

fn call_tool(params: &Value) -> Result<Value> {
    let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let paths = Paths::require_init()?;
    let cfg = Config::load(&paths.config())?;
    let r = Retriever::open(&paths)?;

    let string = |key: &str| -> Result<String> {
        args.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("`{name}` requires a string `{key}`"))
    };
    let number = |key: &str, default: usize| -> usize {
        args.get(key).and_then(|v| v.as_u64()).map(|n| n as usize).unwrap_or(default)
    };

    let answer = match name {
        "outline" => r.outline(&string("path")?)?.fit(budget::for_stage(&cfg, "outline")),
        "symbol" => r.symbol(&string("name")?)?.fit(budget::for_stage(&cfg, "symbol")),
        "refs" => r.refs(&string("name")?)?.fit(budget::for_stage(&cfg, "refs")),
        "importers" => r.importers(&string("path")?)?.fit(budget::for_stage(&cfg, "importers")),
        "source" => {
            let sym = string("name")?;
            let (a, lines) = r.source(&sym, number("nth", 1))?;
            let justify = args.get("justify").and_then(|v| v.as_str());
            budget::account_for_read(&paths, &cfg, &sym, lines, justify)?;
            a.fit(0)
        }
        "slice" => {
            let slug = args.get("slug").and_then(|v| v.as_str()).map(|s| s.to_string());
            let slug = crate::cmd::gate::resolve_slug(&paths, slug)?;
            r.slice(&slug, &string("task")?, budget::for_stage(&cfg, "slice"))?
        }
        "blast_radius" => {
            let scope: Vec<String> = args
                .get("scope")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|s| s.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            if scope.is_empty() {
                anyhow::bail!("`blast_radius` requires a non-empty `scope` array");
            }
            let index = crate::map::db::Index::open(&paths.index_db())?;
            let radius = crate::map::blast::compute(&index, &scope, number("depth", cfg.plan.blast_depth))?;
            let mut text = format!(
                "{} file(s), {} lines at depth {}\n",
                radius.impact.len(), radius.impact_lines, radius.depth
            );
            for i in &radius.impact {
                text.push_str(&format!(
                    "{}  {}  {} lines\n",
                    if i.depth == 0 { "scope".into() } else { format!("+{}", i.depth) },
                    i.path, i.lines
                ));
            }
            crate::retrieve::Answer::from_index("blast_radius", text)
                .fit(budget::for_stage(&cfg, "blast_radius"))
        }
        other => anyhow::bail!("unknown tool `{other}`"),
    };

    let mut text = answer.text;
    if let Some(why) = &r.degraded {
        text = format!("[{why} — this answer is textual, not structural]\n\n{text}");
    }
    Ok(json!({ "content": [{ "type": "text", "text": text }] }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_declares_a_usable_schema() {
        for t in tool_definitions() {
            let name = t["name"].as_str().expect("a tool with no name");
            assert!(!t["description"].as_str().unwrap_or("").is_empty(), "{name} has no description");
            let schema = &t["inputSchema"];
            assert_eq!(schema["type"], "object", "{name}");
            let required = schema["required"].as_array().expect("no required list");
            for r in required {
                let key = r.as_str().unwrap();
                assert!(
                    schema["properties"].get(key).is_some(),
                    "{name} requires `{key}` but does not declare it"
                );
            }
        }
    }

    #[test]
    fn the_tool_set_matches_the_plan() {
        let names: Vec<String> = tool_definitions()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        for expected in ["outline", "symbol", "source", "refs", "importers", "blast_radius", "slice"] {
            assert!(names.contains(&expected.to_string()), "§4.8 lists {expected}; it is missing");
        }
    }

    #[test]
    fn initialize_reports_tool_capability() {
        let v = initialize(&json!({}));
        assert_eq!(v["protocolVersion"], PROTOCOL_VERSION);
        assert!(v["capabilities"]["tools"].is_object());
        assert_eq!(v["serverInfo"]["name"], "keel");
    }

    #[test]
    fn results_and_errors_carry_the_request_id() {
        assert_eq!(result(Some(json!(7)), json!({}))["id"], 7);
        assert_eq!(error(Some(json!("abc")), -1, "x")["id"], "abc");
        assert_eq!(error(None, -1, "x")["id"], Value::Null);
    }
}
