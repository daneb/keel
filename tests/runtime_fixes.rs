//! Oracles for SPEC-0015 `runtime-fixes`.

mod support;

use serde_json::Value;
use support::unique_dir;

fn action() -> Value {
    serde_yaml::from_str(include_str!("../runtime/action.yml")).unwrap()
}

fn steps() -> Vec<Value> {
    action()["runs"]["steps"].as_array().unwrap().clone()
}

fn step_named(name: &str) -> (usize, Value) {
    steps()
        .into_iter()
        .enumerate()
        .find(|(_, s)| s["name"].as_str().is_some_and(|n| n.starts_with(name)))
        .unwrap_or_else(|| panic!("no step named {name}"))
}

/// AC-1 — gVisor from the pinned release's tarball, verified before use.
#[test]
fn gvisor_comes_from_a_pinned_verified_tarball() {
    assert_eq!(action()["inputs"]["gvisor-version"]["default"], "20260921.0");
    let (_, step) = step_named("Install gVisor");
    assert_eq!(step["env"]["GVISOR_VERSION"], "${{ inputs.gvisor-version }}");
    let s = step["run"].as_str().unwrap();
    assert!(!s.contains("${{"), "a value is interpolated into the script:\n{s}");
    assert!(s.contains("releases/release/${GVISOR_VERSION}/$(uname -m)"), "{s}");
    assert!(!s.contains("/latest/"), "still pulls latest:\n{s}");
    let at = |n: &str| s.find(n).unwrap_or_else(|| panic!("missing {n}:\n{s}"));
    let (tarball, digest) = (at("/gvisor.tar.zstd\" -o"), at("/gvisor.tar.zstd.sha512\""));
    let (check, extract, install) =
        (at("sha512sum -c gvisor.tar.zstd.sha512"), at("tar --zstd -xf"), at("runsc install"));
    assert!(tarball < check && digest < check, "checked before downloading:\n{s}");
    assert!(check < extract && extract < install, "used before it was checked:\n{s}");
    assert!(s.contains("KEEL_OCI_RUNTIME=runc"), "no fallback");
}

/// Run the skip step's script in a repository whose last commit is as given,
/// returning what it wrote for KEEL_SKIP.
fn skip_decision(name: &str, author: &str, files: &[&str]) -> String {
    let dir = unique_dir(name);
    std::fs::create_dir_all(&dir).unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git").args(args).current_dir(&dir).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    git(&["init", "-q"]);
    std::fs::write(dir.join("README.md"), "base\n").unwrap();
    git(&["add", "-A"]);
    git(&["-c", "user.name=Dane", "-c", "user.email=d@e", "commit", "-q", "-m", "base"]);
    for f in files {
        let p = dir.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "x").unwrap();
    }
    git(&["add", "-A", "-f"]);
    git(&["-c", &format!("user.name={author}"), "-c", "user.email=a@b", "commit", "-q", "-m", "change"]);

    let (_, step) = step_named("Skip the runtime's own bundle commit");
    let env = dir.join("github_env");
    std::fs::write(&env, "").unwrap();
    let out = std::process::Command::new("bash")
        .args(["-c", step["run"].as_str().unwrap()])
        .current_dir(&dir)
        .env("GITHUB_ENV", &env)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let written = std::fs::read_to_string(&env).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    written.trim().to_string()
}

/// AC-3 — its own bundle commit is skipped; anything else is gated, and every
/// later step obeys the decision.
#[test]
fn the_action_skips_its_own_bundle_commit() {
    let bot = "github-actions[bot]";
    assert_eq!(skip_decision("skip-own", bot, &[".keel/bundles/keel-1.tar.gz"]), "KEEL_SKIP=true");
    assert_eq!(
        skip_decision("skip-bot-code", bot, &[".keel/bundles/keel-1.tar.gz", "src/lib.rs"]),
        "KEEL_SKIP=false",
        "a bot commit that changes code must still be gated"
    );
    assert_eq!(
        skip_decision("skip-human", "Dane", &[".keel/bundles/keel-1.tar.gz"]),
        "KEEL_SKIP=false",
        "a human's bundle-only commit must still be gated"
    );

    let (at, _) = step_named("Skip the runtime's own bundle commit");
    assert!(steps()[0]["uses"].as_str().is_some_and(|u| u.starts_with("actions/checkout@")));
    assert!(at > 0, "the skip decision needs the checkout's history");
    for (i, s) in steps().iter().enumerate().skip(at + 1) {
        assert_eq!(s["if"], "env.KEEL_SKIP != 'true'", "step {i} ({}) is not guarded", s["name"]);
    }
}
