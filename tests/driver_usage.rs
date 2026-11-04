//! Oracles for SPEC-0019 `driver-usage`.

mod support;

use serde_json::Value;
use std::process::Command;
use support::Repo;

fn driver(usage: Option<&str>) -> String {
    let usage = usage.map(|u| format!(",\"usage\":{u}")).unwrap_or_default();
    format!(
        "#!/bin/sh\ncat > /dev/null\nprintf 'pub fn serve() {{}}\\n// edited\\n' > \"$KEEL_REPO/src/api/mod.rs\"\n\
         echo '{{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"src/api/mod.rs\"]{usage}}}'\n"
    )
}

const USAGE: &str = r#"{"input_tokens":100,"output_tokens":50,"cache_read_tokens":10,"cache_write_tokens":5,"cost_usd":0.25}"#;

fn usage_events(r: &Repo, id: &str) -> Vec<Value> {
    let lines = r.read(&format!(".keel/runs/{id}/trajectory.jsonl"));
    let all = lines.lines().map(|l| serde_json::from_str::<Value>(l).unwrap());
    all.filter(|e| e["kind"] == "driver_usage").collect()
}

#[test]
fn driver_result_parses_a_usage_object_and_stays_backward_compatible() {
    let r = Repo::ready("usage-parse");
    let quiet = |usage: &str| {
        format!("#!/bin/sh\ncat > /dev/null\necho '{{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\"{usage}}}'\n")
    };
    r.install_driver("with-usage", &quiet(&format!(",\"usage\":{USAGE}")));
    assert_eq!(r.run(&["driver", "check", "with-usage"]).0, 0);

    r.install_driver("without", &quiet(""));
    assert_eq!(r.run(&["driver", "check", "without"]).0, 0);
}

#[test]
fn claude_code_driver_maps_cli_json_usage_into_the_result() {
    use std::os::unix::fs::PermissionsExt;
    let dir = support::unique_dir("usage-claude");
    let (bin, repo, args) = (dir.join("bin"), dir.join("repo"), dir.join("args.txt"));
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&repo).unwrap();
    let fake = format!(
        "#!/bin/sh\ncat > /dev/null\necho \"$@\" > {}\n\
         echo '{{\"result\":\"done\",\"total_cost_usd\":0.5,\"usage\":{{\"input_tokens\":7,\"output_tokens\":3,\
         \"cache_read_input_tokens\":11,\"cache_creation_input_tokens\":13}}}}'\n",
        args.display()
    );
    std::fs::write(bin.join("claude"), fake).unwrap();
    std::fs::set_permissions(bin.join("claude"), std::fs::Permissions::from_mode(0o755)).unwrap();
    Command::new("git").args(["init", "-q"]).current_dir(&repo).status().unwrap();

    std::fs::write(dir.join("task.json"), r#"{"schema":"keel.drivertask/1","run":"r","spec":"s","prompt":"hi","scope":[],"budget_lines":0,"repo":"."}"#).unwrap();
    let out = Command::new("sh")
        .arg(format!("{}/assets/drivers/claude-code", env!("CARGO_MANIFEST_DIR")))
        .current_dir(&repo)
        .env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()))
        .stdin(std::fs::File::open(dir.join("task.json")).unwrap())
        .output()
        .unwrap();
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();

    assert!(std::fs::read_to_string(&args).unwrap().contains("--output-format json"));
    assert_eq!(result["status"], "ok", "{result}");
    let want = serde_json::json!({"input_tokens":7,"output_tokens":3,"cache_read_tokens":11,"cache_write_tokens":13,"cost_usd":0.5});
    assert_eq!(result["usage"], want);
}

#[test]
fn each_driver_call_appends_a_driver_usage_event() {
    let r = Repo::ready("usage-event");
    r.install_driver("metered", &driver(Some(USAGE)));
    r.run(&["run", "demo"]);
    let evs = usage_events(&r, &r.latest_run());
    assert_eq!(evs.len(), 1, "{evs:?}");
    let e = &evs[0];
    assert_eq!((e["driver"].as_str(), e["source"].as_str(), e["cost_basis"].as_str()), (Some("metered"), Some("reported"), Some("reported")));
    assert_eq!((e["input_tokens"].as_u64(), e["output_tokens"].as_u64(), e["cost_usd"].as_f64()), (Some(100), Some(50), Some(0.25)));
}

#[test]
fn a_driver_that_reports_nothing_is_counted_as_unreported_not_as_zero() {
    let r = Repo::ready("usage-unreported");
    r.install_driver("silent", &driver(None));
    r.run(&["run", "demo"]);
    assert_eq!(usage_events(&r, &r.latest_run())[0]["source"], "unreported");

    let out = r.ok(&["metrics"]);
    assert!(out.contains("0 tokens spent (reported by the driver), 1 call(s) unreported"), "{out}");
}

#[test]
fn metrics_prints_injected_and_spent_as_separate_never_summed_figures() {
    let r = Repo::ready("usage-separate");
    r.install_driver("metered", &driver(Some(USAGE)));
    r.run(&["run", "demo"]);
    let out = r.ok(&["metrics"]);

    let line = |n: &str| out.lines().find(|l| l.contains(n)).unwrap_or_else(|| panic!("no `{n}` line:\n{out}"));
    let (injected, spent) = (line("injected (estimated by keel)"), line("spent (reported by the driver)"));
    let num = |l: &str| l.split_whitespace().next().unwrap().parse::<usize>().unwrap();
    assert_eq!(num(spent), 165);
    let sum = (num(injected) + num(spent)).to_string();
    assert!(!out.split_whitespace().any(|w| w == sum), "the sum {sum} was printed:\n{out}");
}

#[test]
fn spec_cost_totals_every_run_and_shows_the_share_spent_on_red_runs() {
    let r = Repo::ready("usage-red");
    r.install_driver("metered", &driver(Some(USAGE)));
    r.run(&["run", "demo"]);

    // The second run goes red at G2: the build command fails.
    r.edit_config(|cfg| cfg["verify"]["build"] = "false".into());
    assert_ne!(r.run(&["run", "demo"]).0, 0);

    let report: Value = serde_json::from_str(&r.ok(&["report", "--json"])).unwrap();
    let spec = report["specs"].as_array().unwrap().iter().find(|s| s["slug"] == "demo").unwrap();
    assert_eq!(spec["spent"]["tokens"], 330);
    assert_eq!(spec["spent_tokens_on_red_runs"], 165);
    assert_eq!(spec["red_share"], 0.5);
}

#[test]
fn price_table_cost_is_labelled_estimated_and_never_overrides_a_reported_cost() {
    let r = Repo::ready("usage-price");
    r.edit_config(|cfg| {
        let p = toml::toml! { driver = "metered"  as_of = "2026-11-01"  input_per_mtok = 3.0  output_per_mtok = 15.0 };
        cfg.as_table_mut().unwrap().insert("price".into(), toml::Value::Array(vec![p.into()]));
    });

    r.install_driver("metered", &driver(Some(r#"{"input_tokens":1000000,"output_tokens":1000000}"#)));
    r.run(&["run", "demo"]);
    let e = usage_events(&r, &r.latest_run()).remove(0);
    assert_eq!(e["cost_usd"], 18.0);
    assert_eq!(e["cost_basis"], "estimated from price table dated 2026-11-01");

    r.install_driver("metered", &driver(Some(USAGE)));
    r.run(&["run", "demo"]);
    let e = usage_events(&r, &r.latest_run()).remove(0);
    assert_eq!(e["cost_usd"], 0.25);
    assert_eq!(e["cost_basis"], "reported");
}

#[test]
fn reviewer_usage_is_recorded_against_g2_5() {
    use std::os::unix::fs::PermissionsExt;
    let r = Repo::ready("usage-reviewer");
    r.install_driver("metered", &driver(None));
    r.write(
        "tests/reviewer.sh",
        "#!/bin/sh\ncat > /dev/null\n\
         echo '{\"schema\":\"keel.reviewresult/1\",\"findings\":[],\"usage\":{\"input_tokens\":40,\"output_tokens\":2}}'\n",
    );
    let script = r.dir.join("tests/reviewer.sh");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    r.edit_config(|cfg| {
        let rv = toml::toml! { id = "critic"  cmd = (script.display().to_string()) };
        cfg.as_table_mut().unwrap().insert("review".into(), toml::Value::Array(vec![rv.into()]));
    });
    r.run(&["run", "demo"]);

    let evs = usage_events(&r, &r.latest_run());
    let e = evs.iter().find(|e| e["gate"] == "G2.5").unwrap_or_else(|| panic!("{evs:?}"));
    assert_eq!((e["driver"].as_str(), e["input_tokens"].as_u64(), e["source"].as_str()), (Some("critic"), Some(40), Some("reported")));
}
