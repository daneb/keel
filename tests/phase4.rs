//! End-to-end tests for Phase 4: the retrieval service.
//!
//! Encodes the Phase 4 exit criterion from PLAN.md §5 — a measured token drop
//! on a fixed set of tasks — plus the property the plan is most insistent
//! about: the index is an accelerator, never a dependency.

mod support;

use support::Repo;

/// A repo with enough structure for retrieval to have something to say.
fn indexed(name: &str) -> Repo {
    let r = Repo::bare(name);
    r.write(
        "src/core.rs",
        "/// Guards a route.\npub struct Guard { pub id: u32 }\n\n\
         impl Guard {\n    /// Checks the guard.\n    pub fn check(&self) -> bool { true }\n}\n\n\
         pub fn helper() -> u32 { 1 }\n",
    );
    r.write(
        "src/api/mod.rs",
        "use crate::core::Guard;\n\npub fn serve() { let g = Guard { id: helper() }; g.check(); }\n\
         fn helper() -> u32 { 2 }\n",
    );
    r.write("src/main.rs", "mod api;\nmod core;\nfn main() { api::serve(); }\n");
    r.ok(&["map"]);
    r
}

// ---------------------------------------------------------------------------
// the queries
// ---------------------------------------------------------------------------

#[test]
fn outline_gives_signatures_without_bodies() {
    let r = indexed("outline");
    let out = r.ok(&["outline", "src/core.rs"]);
    assert!(out.contains("pub struct Guard"), "{out}");
    assert!(out.contains("pub fn check"), "{out}");
    assert!(out.contains("Checks the guard."), "the doc was dropped:\n{out}");
    // The point of an outline is what it leaves out.
    assert!(!out.contains("true }"), "the body leaked into the outline:\n{out}");
}

#[test]
fn symbol_locates_a_definition_without_its_body() {
    let r = indexed("symbol");
    let out = r.ok(&["symbol", "Guard"]);
    assert!(out.contains("src/core.rs:2"), "{out}");
    assert!(out.contains("pub struct Guard"), "{out}");
    assert!(out.contains("Guards a route."), "{out}");
}

#[test]
fn source_returns_the_body_on_demand() {
    let r = indexed("source");
    let out = r.ok(&["source", "check"]);
    assert!(out.contains("pub fn check(&self) -> bool { true }"), "{out}");
}

#[test]
fn a_name_defined_twice_says_so_rather_than_picking_quietly() {
    let r = indexed("ambiguous");
    let out = r.ok(&["source", "helper"]);
    assert!(
        out.contains("defined in 2 places"),
        "an ambiguous symbol was resolved silently:\n{out}"
    );
    assert!(out.contains("--nth"), "the answer does not say how to reach the others:\n{out}");

    // Each one is individually reachable, and they differ.
    let first = r.ok(&["source", "helper", "--nth", "1"]);
    let second = r.ok(&["source", "helper", "--nth", "2"]);
    assert_ne!(first, second, "--nth returned the same definition twice");

    // Out of range is an error, not a wrap-around.
    let (code, err) = r.run(&["source", "helper", "--nth", "9"]);
    assert_ne!(code, 0, "{err}");
    assert!(err.contains("--nth 1..2"), "{err}");
}

#[test]
fn refs_finds_uses_and_excludes_the_definition() {
    let r = indexed("refs");
    let out = r.ok(&["refs", "Guard"]);
    assert!(out.contains("src/api/mod.rs"), "the use site is missing:\n{out}");
    assert!(out.contains("use(s) across"), "{out}");
}

#[test]
fn importers_walks_the_import_graph() {
    let r = indexed("importers");
    let out = r.ok(&["importers", "src/core.rs"]);
    assert!(out.contains("src/api/mod.rs"), "the importer is missing:\n{out}");
}

#[test]
fn every_answer_reports_its_token_cost() {
    let r = indexed("tokens");
    let out = r.ok(&["symbol", "Guard", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v["tokens"].as_u64().unwrap() > 0, "no token cost reported");
    assert_eq!(v["source"], "index");
}

// ---------------------------------------------------------------------------
// the index is an accelerator, never a dependency
// ---------------------------------------------------------------------------

#[test]
fn every_query_falls_back_to_text_when_the_index_is_gone() {
    let r = indexed("fallback");
    std::fs::remove_file(r.dir.join(".keel/store/map/index.sqlite")).unwrap();

    for (args, expect) in [
        (vec!["outline", "src/core.rs"], "Guard"),
        (vec!["symbol", "Guard"], "Guard"),
        (vec!["refs", "Guard"], "Guard"),
        (vec!["importers", "src/core.rs"], "core"),
    ] {
        let mut with_json = args.clone();
        with_json.push("--json");
        let out = r.ok(&with_json);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["source"], "ripgrep", "{args:?} did not fall back");
        assert!(
            v["text"].as_str().unwrap().contains(expect),
            "{args:?} fell back but answered nothing useful: {}",
            v["text"]
        );
    }
}

#[test]
fn a_textual_answer_says_it_is_textual() {
    // Silently degrading from symbols to grep is how an agent ends up
    // confidently wrong about a codebase.
    let r = indexed("labelled");
    std::fs::remove_file(r.dir.join(".keel/store/map/index.sqlite")).unwrap();
    let out = r.ok(&["outline", "src/core.rs"]);
    assert!(out.contains("not parsed"), "{out}");
    let (_, stderr) = r.run(&["symbol", "Guard"]);
    assert!(stderr.contains("no index"), "the degradation was not announced:\n{stderr}");
}

#[test]
fn an_index_from_an_older_schema_is_not_trusted() {
    let r = indexed("schema");
    let db = r.dir.join(".keel/store/map/index.sqlite");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute("UPDATE meta SET value = 'keel.index/0' WHERE key = 'schema'", [])
        .unwrap();
    drop(conn);

    let out = r.ok(&["symbol", "Guard", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["source"], "ripgrep", "a stale-schema index was used as if current");
}

// ---------------------------------------------------------------------------
// the budget governor
// ---------------------------------------------------------------------------

#[test]
fn a_large_body_needs_a_justification() {
    let r = Repo::bare("justify");
    let big: String = std::iter::once("pub fn huge() {\n".to_string())
        .chain((0..400).map(|i| format!("    let _x{i} = {i};\n")))
        .chain(std::iter::once("}\n".to_string()))
        .collect();
    r.write("src/api/big.rs", &big);
    r.ok(&["map"]);
    r.edit_config(|cfg| {
        cfg["retrieve"]["max_unjustified_lines"] = toml::Value::Integer(50);
    });

    let (code, out) = r.run(&["source", "huge"]);
    assert_ne!(code, 0, "a 400-line body was returned with no justification:\n{out}");
    assert!(out.contains("keel outline"), "the refusal must name the cheaper path:\n{out}");

    let out = r.ok(&["source", "huge", "--justify", "tracing a panic through it"]);
    assert!(out.contains("let _x0"), "a justified read was still refused:\n{out}");
}

#[test]
fn answers_are_fitted_to_the_token_budget() {
    let r = indexed("budget");
    r.edit_config(|cfg| {
        cfg["retrieve"]["query_tokens"] = toml::Value::Integer(20);
    });
    let out = r.ok(&["refs", "Guard", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(
        v["tokens"].as_u64().unwrap() <= 20,
        "the budget was exceeded: {} tokens", v["tokens"]
    );
}

// ---------------------------------------------------------------------------
// incremental reindex
// ---------------------------------------------------------------------------

#[test]
fn unchanged_files_are_reused_and_changed_ones_are_not() {
    let r = indexed("incremental");
    let out = r.ok(&["map", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let total = v["files"].as_u64().unwrap();
    assert_eq!(v["reused"].as_u64().unwrap(), total, "nothing was reused: {v}");

    // Touch one file; it must be re-parsed and the rest reused.
    r.write("src/core.rs", "pub struct Guard { pub id: u32 }\npub fn brand_new() {}\n");
    let out = r.ok(&["map", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["reused"].as_u64().unwrap(), total - 1, "the edited file was reused: {v}");

    // And the new symbol is actually in the index.
    assert!(r.ok(&["symbol", "brand_new"]).contains("src/core.rs"));

    // --full ignores the cache entirely.
    let out = r.ok(&["map", "--full", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["reused"].as_u64().unwrap(), 0, "--full reused the old index: {v}");
}

// ---------------------------------------------------------------------------
// MCP
// ---------------------------------------------------------------------------

#[test]
fn the_mcp_server_handshakes_lists_tools_and_answers() {
    use std::io::Write;
    let r = indexed("mcp");

    let mut child = std::process::Command::new(support::BIN)
        .arg("mcp")
        .current_dir(&r.dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        for line in [
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"outline","arguments":{"path":"src/core.rs"}}}"#,
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"missing","arguments":{}}}"#,
        ] {
            writeln!(stdin, "{line}").unwrap();
        }
    }
    let out = child.wait_with_output().unwrap();
    let replies: Vec<serde_json::Value> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();

    // The notification must not be answered.
    assert_eq!(replies.len(), 4, "a notification drew a reply: {replies:#?}");

    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "keel");
    let tools: Vec<&str> = replies[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for expected in ["outline", "symbol", "source", "refs", "importers", "blast_radius", "slice"] {
        assert!(tools.contains(&expected), "{expected} is not exposed: {tools:?}");
    }

    let text = replies[2]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("pub struct Guard"), "{text}");

    // An unknown tool is a result carrying isError, not a protocol error: the
    // model has to be able to read what went wrong.
    assert_eq!(replies[3]["result"]["isError"], true);
    assert!(replies[3]["error"].is_null(), "a tool failure became a protocol error");
}

// ---------------------------------------------------------------------------
// the exit criterion
// ---------------------------------------------------------------------------

#[test]
fn the_benchmark_reports_a_ratio_and_holds_the_target() {
    // Run against keel's own repository, which is what the fixed task set
    // describes; a synthetic fixture would measure nothing.
    let out = std::process::Command::new(support::BIN)
        .args(["bench", "--json"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(out.status.success(), "bench failed: {}", String::from_utf8_lossy(&out.stderr));

    let v: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(v["tasks"].as_array().unwrap().len(), 5, "the plan fixes five tasks");
    let ratio = v["ratio"].as_f64().unwrap();
    assert!(
        ratio >= 3.0,
        "retrieval saved only {ratio:.1}×; Phase 4 accepts nothing below 3×"
    );
    for t in v["tasks"].as_array().unwrap() {
        assert!(
            t["retrieval_tokens"].as_u64().unwrap() > 0,
            "a task measured zero retrieval cost, which cannot be right: {t}"
        );
    }
}
