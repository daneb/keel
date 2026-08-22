//! Oracles for SPEC-0002 `agent-driver`.

mod support;

use support::Repo;

fn events(r: &Repo, id: &str) -> Vec<serde_json::Value> {
    r.read(&format!(".keel/runs/{id}/trajectory.jsonl"))
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// AC-1 — the task goes in on stdin as one JSON object, the result comes out on
/// stdout as one JSON object.
#[test]
fn task_in_result_out() {
    let r = Repo::ready("drv-roundtrip");
    // Echo the task straight back out so the test can inspect what keel sent.
    r.install_driver(
        "echo",
        "#!/bin/sh\ncat > \"$KEEL_REPO/received-task.json\"\n\
         echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"src/api/mod.rs\"]}'\n",
    );
    r.run(&["run", "demo", "--task", "T-1"]);

    let sent: serde_json::Value =
        serde_json::from_str(&r.read("received-task.json")).expect("the task was not valid JSON");
    assert_eq!(sent["schema"], "keel.drivertask/1");
    assert_eq!(sent["spec"], "demo");
    assert_eq!(sent["task"], "T-1");
    assert!(sent["prompt"].as_str().unwrap().contains("Acceptance criteria"),
        "the spec did not reach the driver");
    assert_eq!(sent["scope"][0], "src/api/**");
    assert_eq!(sent["budget_lines"], 120);

    let id = r.latest_run();
    let result: serde_json::Value =
        serde_json::from_str(&r.read(&format!(".keel/runs/{id}/evidence/driver.json"))).unwrap();
    assert_eq!(result["status"], "ok");
    assert_eq!(result["files_changed"][0], "src/api/mod.rs");
}

/// AC-2 — IF a driver cannot be started THEN the verdict is `blocked` and no
/// agentic failure is recorded.
#[test]
fn unstartable_driver_is_blocked() {
    let r = Repo::ready("drv-unstartable");
    r.edit_config(|cfg| {
        let mut d = toml::value::Table::new();
        d.insert("id".into(), toml::Value::String("ghost".into()));
        d.insert("cmd".into(), toml::Value::String("./definitely-not-here".into()));
        d.insert("default".into(), toml::Value::Boolean(true));
        d.insert("timeout_secs".into(), toml::Value::Integer(5));
        cfg.as_table_mut().unwrap()
            .insert("driver".into(), toml::Value::Array(vec![toml::Value::Table(d)]));
    });

    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 3, "an unstartable driver must block, not fail:\n{out}");
    assert!(out.contains("BLOCKED"), "{out}");

    let id = r.latest_run();
    let evs = events(&r, &id);
    let result = evs.iter().find(|e| e["kind"] == "driver_result").expect("no driver_result event");
    assert_eq!(result["status"], "blocked");
    assert!(
        result["detail"].as_str().unwrap().contains("could not start"),
        "the reason was not recorded: {result}"
    );
    // A blocked driver is not an agentic failure, so no gate should have judged
    // work that never happened.
    assert!(
        !evs.iter().any(|e| e["kind"] == "gate"),
        "gates ran against a driver that never started"
    );
}

/// AC-3 — IF the driver prints something that is not `keel.driverresult/1` THEN
/// keel names the offending field.
#[test]
fn invalid_result_names_the_field() {
    let r = Repo::ready("drv-invalid");
    r.install_driver(
        "bad",
        "#!/bin/sh\ncat > /dev/null\n\
         echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"probably\"}'\n",
    );
    let (code, out) = r.run(&["run", "demo"]);
    assert_eq!(code, 3, "{out}");

    let id = r.latest_run();
    let evs = events(&r, &id);
    let result = evs.iter().find(|e| e["kind"] == "driver_result").unwrap();
    let detail = result["detail"].as_str().unwrap();
    assert!(detail.contains("field `status`"), "the offending field is not named: {detail}");
}

/// AC-4 — the bundled driver contract round-trips: keel invokes the command
/// named by `driver.cmd`, passes the task on stdin, and parses stdout.
#[test]
fn claude_code_driver_round_trips() {
    let r = Repo::ready("drv-claude");
    // Stand in for the real CLI: read the task, make the change it describes,
    // report what changed. This is all a driver is required to do.
    r.install_driver(
        "claude-code",
        "#!/bin/sh\n\
         task=$(cat)\n\
         printf 'pub fn serve() { /* limited */ }\\n' > \"$KEEL_REPO/src/api/mod.rs\"\n\
         echo \"$task\" | grep -q 'keel.drivertask/1' || exit 9\n\
         echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"src/api/mod.rs\"],\"tokens\":42}'\n",
    );
    let (_, out) = r.run(&["run", "demo"]);
    assert!(out.contains("driver ok"), "the driver did not complete:\n{out}");

    assert!(
        r.read("src/api/mod.rs").contains("limited"),
        "the driver's change did not land in the working tree"
    );
    let id = r.latest_run();
    let result: serde_json::Value =
        serde_json::from_str(&r.read(&format!(".keel/runs/{id}/evidence/driver.json"))).unwrap();
    assert_eq!(result["status"], "ok");
    assert_eq!(result["tokens"], 42);
}

/// AC-5 — WHEN a driver is invoked THE SYSTEM SHALL append `driver_call` and
/// `driver_result` events.
#[test]
fn invocation_is_recorded() {
    let r = Repo::ready("drv-recorded");
    r.install_driver("noop", &support::noop_driver());
    r.run(&["run", "demo"]);

    let id = r.latest_run();
    let evs = events(&r, &id);

    let call = evs.iter().find(|e| e["kind"] == "driver_call").expect("no driver_call event");
    let result = evs.iter().find(|e| e["kind"] == "driver_result").expect("no driver_result event");
    assert_eq!(call["driver"], "noop");
    assert!(call["prompt_tokens"].as_u64().unwrap() > 0, "no prompt cost recorded");
    assert_eq!(result["driver"], "noop");
    assert_eq!(result["status"], "ok");

    let call_seq = call["seq"].as_u64().unwrap();
    let result_seq = result["seq"].as_u64().unwrap();
    assert!(call_seq < result_seq, "the result was recorded before the call");
}

/// AC-6 — WHEN a driver exceeds its timeout THE SYSTEM SHALL terminate it and
/// record `blocked`.
#[test]
fn timeout_terminates_and_blocks() {
    let r = Repo::ready("drv-timeout");
    r.install_driver("slow", "#!/bin/sh\ncat > /dev/null\nsleep 60\n");
    r.edit_config(|cfg| {
        cfg["driver"][0]["timeout_secs"] = toml::Value::Integer(1);
    });

    let started = std::time::Instant::now();
    let (code, out) = r.run(&["run", "demo"]);
    let elapsed = started.elapsed();

    assert_eq!(code, 3, "a timed-out driver must block, not fail:\n{out}");
    assert!(
        elapsed < std::time::Duration::from_secs(30),
        "keel waited {elapsed:?} for a driver with a 1s timeout"
    );

    let id = r.latest_run();
    let evs = events(&r, &id);
    let result = evs.iter().find(|e| e["kind"] == "driver_result").expect("no driver_result event");
    assert_eq!(result["status"], "blocked");
    assert!(
        result["detail"].as_str().unwrap().contains("timeout"),
        "the timeout was not the recorded reason: {result}"
    );
}
