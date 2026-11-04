//! SPEC-0020: an oracle must fail without the change it claims to verify.

mod support;

use support::Repo;

/// A driver that creates `src/api/done` and a test script that checks for it.
fn driver() -> String {
    "#!/bin/sh\ncat > /dev/null\n\
     echo done > \"$KEEL_REPO/src/api/done\"\n\
     mkdir -p \"$KEEL_REPO/tests\"\n\
     echo 'test -f src/api/done' > \"$KEEL_REPO/tests/check.sh\"\n\
     echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"src/api/done\",\"tests/check.sh\"]}'\n"
        .to_string()
}

/// A repo whose single criterion carries `oracles`, run with `driver()`.
fn run_with(name: &str, oracles: &str, tweak: impl FnOnce(&Repo)) -> (Repo, String) {
    let r = Repo::bare(name);
    r.write_spec_with_oracle(oracles);
    let tasks = r.read(".keel/specs/demo/tasks.md");
    r.ok(&["gate", "g0", "demo"]);
    r.ok(&["approve", "demo", "--stage", "spec"]);
    r.ok(&["plan", "demo"]);
    r.set_rollback("git revert the merge");
    r.write(".keel/specs/demo/tasks.md", &tasks);
    r.ok(&["gate", "g1", "demo"]);
    r.install_driver("worker", &driver());
    tweak(&r);
    let (_, out) = r.run(&["run", "demo"]);
    (r, out)
}

fn report(r: &Repo) -> String {
    r.read(&format!(".keel/runs/{}/evidence/oracle-sensitivity.json", r.latest_run()))
}

const SENSITIVE: &str = "oracle: cmd `sh tests/check.sh` exit 0";

#[test]
fn executable_oracles_are_rerun_in_a_base_worktree_with_test_files_carried_over() {
    let (r, out) = run_with("rerun", SENSITIVE, |_| {});
    // check.sh exists only because it was carried over; the change itself
    // (src/api/done) is absent at the base, so the oracle fails there.
    assert!(out.contains("oracle-sensitivity"), "{out}");
    assert!(out.contains("1 oracle(s) fail at the base"), "{out}");
    assert!(report(&r).contains("\"assessment\": \"sensitive\""));
}

#[test]
fn an_oracle_passing_without_the_change_fails_sensitivity() {
    let (r, out) = run_with("insensitive", "oracle: cmd `true` exit 0", |_| {});
    assert!(out.contains("FAIL") && out.contains("oracle-sensitivity"), "{out}");
    assert!(out.contains("AC-1"), "the criterion is not named:\n{out}");
    assert!(report(&r).contains("\"insensitive\""));
}

#[test]
fn a_guard_oracle_must_pass_at_the_base_and_on_the_change() {
    let (r, out) = run_with("guard-ok", "oracle: guard cmd `true` exit 0", |_| {});
    assert!(!out.contains("FAIL  oracle-sensitivity"), "{out}");
    assert!(report(&r).contains("\"held\""));

    let (r, out) = run_with("guard-bad", "oracle: guard cmd `sh tests/check.sh` exit 0", |_| {});
    assert!(out.contains("FAIL") && out.contains("oracle-sensitivity"), "{out}");
    assert!(report(&r).contains("\"broken\""));
}

#[test]
fn an_unbuildable_base_blocks_sensitivity_rather_than_passing_it() {
    // The build passes on the change (done exists) and fails at the base.
    let (_r, out) = run_with("unbuildable", SENSITIVE, |r| {
        r.edit_config(|cfg| {
            cfg["verify"]["build"] = toml::Value::String("test -f src/api/done".into());
        });
    });
    assert!(out.contains("BLOCKED  oracle-sensitivity"), "{out}");
    assert!(out.contains("at the base commit"), "{out}");
}

#[test]
fn oracles_keel_cannot_isolate_are_listed_as_not_assessed() {
    let oracles = format!("{SENSITIVE}\noracle: human a reviewer reads the output");
    let (r, out) = run_with("not-assessed", &oracles, |_| {});
    assert!(out.contains("not assessed"), "{out}");
    assert!(report(&r).contains("\"assessment\": \"not assessed\""));
}

fn mutation_tool(r: &Repo, advisory: Option<bool>) {
    r.write("mutate.sh", "#!/bin/sh\ncat tests/fixtures/mutation.json\n");
    r.write(
        "tests/fixtures/mutation.json",
        include_str!("fixtures/mutation/cargo-mutants-sample.json"),
    );
    r.edit_config(|cfg| {
        let mut m = toml::value::Table::new();
        m.insert("cmd".into(), toml::Value::String("sh mutate.sh".into()));
        if let Some(a) = advisory {
            m.insert("advisory".into(), toml::Value::Boolean(a));
        }
        cfg.as_table_mut().unwrap().insert("mutation".into(), toml::Value::Table(m));
    });
}

#[test]
fn survived_mutants_in_scope_become_mutation_findings() {
    let (_r, out) = run_with("mutants", SENSITIVE, |r| mutation_tool(r, None));
    assert!(out.contains("mutation"), "{out}");
    assert!(out.contains("src/api/mod.rs:7"), "survived mutant not reported:\n{out}");
    assert!(!out.contains("src/api/mod.rs:3"), "a killed mutant was reported:\n{out}");
}

#[test]
fn mutation_is_advisory_by_default_and_blocks_only_when_configured() {
    let (_r, out) = run_with("advisory", SENSITIVE, |r| mutation_tool(r, None));
    assert!(!out.contains("FAIL    mutation") && !out.contains("FAIL  mutation"), "{out}");
    let (_r, out) = run_with("blocking", SENSITIVE, |r| mutation_tool(r, Some(false)));
    assert!(out.contains("mutation") && out.contains("FAIL"), "{out}");
    assert!(out.contains("src/api/mod.rs:7"), "{out}");
}

#[test]
fn the_cargo_mutants_adapter_emits_mutationresult_and_blocks_when_absent() {
    let r = Repo::bare("adapter");
    assert!(r.exists(".keel/mutation/cargo-mutants"), "keel init did not write the adapter");
    // An empty PATH-less environment has no cargo-mutants to find.
    let out = std::process::Command::new("/bin/sh")
        .arg(r.dir.join(".keel/mutation/cargo-mutants"))
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema"], "keel.mutationresult/1");
}
