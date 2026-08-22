//! End-to-end tests over the real binary, on a real polyglot repository.
//!
//! These encode the Phase 0 exit criteria from PLAN.md §5:
//!   * every projection lands under its budget,
//!   * the drift check catches a hand-edit, and
//!   * a hand-edit is never silently overwritten.

use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_keel");

/// Unique per call, not merely per nanosecond: the clock is coarse enough on
/// some platforms that two tests starting together share a directory, which is
/// a flake that costs an afternoon to diagnose.
fn unique_dir(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "keel-it-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

struct Repo {
    dir: PathBuf,
}

impl Repo {
    fn new(name: &str) -> Self {
        let dir = unique_dir(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Pin the repo root so discovery cannot wander up into the real tree.
        std::fs::create_dir_all(dir.join(".git").join("hooks")).unwrap();
        Self { dir }
    }

    fn write(&self, rel: &str, content: &str) -> &Self {
        let p = self.dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
        self
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.dir.join(rel)).unwrap()
    }

    fn keel(&self, args: &[&str]) -> Output {
        Command::new(BIN)
            .args(args)
            .current_dir(&self.dir)
            .output()
            .expect("running keel")
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.keel(args);
        assert!(
            out.status.success(),
            "keel {args:?} failed ({}):\n{}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    fn code(&self, args: &[&str]) -> i32 {
        self.keel(args).status.code().unwrap_or(-1)
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn polyglot(name: &str) -> Repo {
    let r = Repo::new(name);
    r.write("Cargo.toml", "[package]\nname = \"demo\"\n")
        .write(
            "src/main.rs",
            "mod api;\n\n/// Entry point.\nfn main() { api::serve(); }\n",
        )
        .write(
            "src/api/mod.rs",
            "use crate::core::auth::Guard;\n\n/// Serves requests.\npub fn serve() {}\n\npub struct Router { pub n: usize }\n",
        )
        .write(
            "src/core/auth.rs",
            "/// Protects a route.\npub struct Guard;\n\nimpl Guard {\n    pub fn check(&self) -> bool { true }\n}\n",
        )
        .write(
            "web/index.ts",
            "import { helper } from './helper';\n\nexport interface Options { debug: boolean }\n\nexport function boot(o: Options) { helper(); }\n",
        )
        .write("web/helper.ts", "export function helper() {}\n")
        .write(
            "py/pkg/__init__.py",
            "from .util import compute\n\nclass Engine:\n    \"\"\"Runs the thing.\"\"\"\n    def run(self):\n        return compute()\n",
        )
        .write("py/pkg/util.py", "def compute():\n    \"\"\"Computes.\"\"\"\n    return 1\n")
        .write(
            "svc/main.go",
            "package main\n\nimport \"fmt\"\n\ntype Server struct{ Port int }\n\nfunc Run() { fmt.Println(\"go\") }\n",
        )
        .write(
            "app/Main.java",
            "package app;\n\npublic class Main {\n    public static void main(String[] a) {}\n}\n",
        );
    r
}

#[test]
fn init_indexes_every_supported_language() {
    let r = polyglot("init");
    let out = r.ok(&["init", "--yes"]);
    assert!(out.contains("files ·"), "no map report in:\n{out}");

    let structure = r.read(".keel/store/steering/structure.md");
    for expected in ["src/main.rs", "web/index.ts", "py/pkg", "svc/main.go", "app/Main.java"] {
        assert!(structure.contains(expected), "structure.md missing {expected}:\n{structure}");
    }
    for symbol in ["Router", "Guard", "boot", "Engine", "Server"] {
        assert!(structure.contains(symbol), "structure.md missing symbol {symbol}");
    }
    // Docs make it through extraction into the map.
    assert!(structure.contains("Serves requests."), "doc comment lost");
}

#[test]
fn every_projection_lands_under_its_budget() {
    let r = polyglot("budget");
    r.ok(&["init", "--yes"]);
    let cfg: toml::Value = toml::from_str(&r.read(".keel/keel.toml")).unwrap();
    let adapters = cfg["adapter"].as_array().unwrap();
    assert!(!adapters.is_empty());

    for a in adapters {
        let out = a["out"].as_str().unwrap();
        let budget = a["budget"].as_integer().unwrap() as usize;
        let content = r.read(out);
        // Body only: the two-line provenance header is not part of the budget.
        let body_lines = content.lines().skip_while(|l| l.trim_start().starts_with("<!--")).count();
        assert!(
            body_lines <= budget,
            "{out} is {body_lines} lines against a budget of {budget}"
        );
        assert!(content.starts_with("<!-- keel:generated"), "{out} has no provenance header");
    }
    assert_eq!(r.code(&["store", "check"]), 0, "freshly rendered store should be clean");
}

#[test]
fn drift_is_caught_and_never_silently_overwritten() {
    let r = polyglot("drift");
    r.ok(&["init", "--yes"]);
    assert_eq!(r.code(&["store", "check"]), 0);

    let before = r.read("CLAUDE.md");
    let edited = format!("{before}\n- A rule a human added by hand.\n");
    std::fs::write(r.dir.join("CLAUDE.md"), &edited).unwrap();

    let out = r.keel(&["store", "check"]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(out.status.code(), Some(1), "drift must fail the check:\n{text}");
    assert!(text.contains("DRIFT"), "drift not reported:\n{text}");

    // Rendering must refuse to destroy the edit.
    let render = r.ok(&["store", "render"]);
    assert!(render.contains("SKIPPED"), "render did not skip the drifted file:\n{render}");
    assert_eq!(
        r.read("CLAUDE.md"),
        edited,
        "render overwrote a hand-edited projection"
    );

    // Reconcile parks the edit in the store and restores the projection.
    let rec = r.ok(&["store", "reconcile", "CLAUDE.md"]);
    assert!(rec.contains("captured"), "{rec}");
    let inbox: Vec<PathBuf> = std::fs::read_dir(r.dir.join(".keel/store/inbox"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(inbox.len(), 1, "expected exactly one parked edit");
    let parked = std::fs::read_to_string(&inbox[0]).unwrap();
    assert!(
        parked.contains("A rule a human added by hand."),
        "the human's edit was lost:\n{parked}"
    );
    assert_eq!(r.code(&["store", "check"]), 0, "check should be clean after reconcile");
}

#[test]
fn store_edits_make_projections_stale_not_drifted() {
    let r = polyglot("stale");
    r.ok(&["init", "--yes"]);

    let conventions = r.dir.join(".keel/store/steering/conventions.md");
    let mut c = std::fs::read_to_string(&conventions).unwrap();
    c.push_str("\n- Prefer `?` over `unwrap()`.\n");
    std::fs::write(&conventions, c).unwrap();

    let out = r.keel(&["store", "check"]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(out.status.code(), Some(1));
    assert!(text.contains("stale"), "expected stale, got:\n{text}");
    assert!(!text.contains("DRIFT"), "store edit must not read as drift:\n{text}");

    r.ok(&["store", "render"]);
    assert_eq!(r.code(&["store", "check"]), 0);
    assert!(
        r.read("CLAUDE.md").contains("unwrap()"),
        "the new convention did not reach the projection"
    );
}

#[test]
fn a_foreign_file_is_reported_rather_than_clobbered() {
    let r = polyglot("foreign");
    r.write("CLAUDE.md", "# My own hand-written context\n\nDo not lose this.\n");
    r.ok(&["init", "--yes"]);

    assert_eq!(
        r.read("CLAUDE.md"),
        "# My own hand-written context\n\nDo not lose this.\n",
        "keel overwrote a file it did not create"
    );
    let out = r.keel(&["store", "check"]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(out.status.code(), Some(1));
    assert!(text.contains("foreign"), "expected a foreign report:\n{text}");
}

#[test]
fn the_map_is_rebuildable_and_deterministic() {
    let r = polyglot("determinism");
    r.ok(&["init", "--yes"]);
    let first = r.read(".keel/store/steering/structure.md");
    r.ok(&["map"]);
    let second = r.read(".keel/store/steering/structure.md");
    assert_eq!(first, second, "two identical maps differ");
}

#[test]
fn a_lower_budget_produces_a_smaller_map() {
    let r = polyglot("mapbudget");
    r.ok(&["init", "--yes"]);
    r.ok(&["map", "--budget", "40"]);
    let small = r.read(".keel/store/steering/structure.md");
    assert!(
        small.lines().count() <= 40 + 8, // + front matter
        "map ignored --budget: {} lines",
        small.lines().count()
    );
}

#[test]
fn the_pre_commit_hook_blocks_when_keel_cannot_run() {
    let r = polyglot("hook");
    r.ok(&["init", "--yes"]);
    r.ok(&["hook", "install"]);

    let hook = r.dir.join(".git/hooks/pre-commit");
    assert!(hook.exists(), "hook not written");
    let body = std::fs::read_to_string(&hook).unwrap();
    assert!(body.contains("store check"), "hook does not call store check");
    assert!(
        body.contains("exit 1"),
        "a hook that cannot find keel must fail, not skip:\n{body}"
    );

    // Installing twice is a no-op, and uninstall leaves nothing behind.
    r.ok(&["hook", "install"]);
    r.ok(&["hook", "uninstall"]);
    assert!(
        !std::fs::read_to_string(&hook).map(|b| b.contains("store check")).unwrap_or(false),
        "uninstall left the keel block behind"
    );
}

#[test]
fn commands_refuse_to_run_before_init() {
    let r = Repo::new("uninit");
    std::fs::create_dir_all(r.dir.join(".git")).unwrap();
    for args in [vec!["map"], vec!["status"], vec!["store", "check"]] {
        let out = r.keel(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?} should have errored");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("keel init"),
            "{args:?} did not point at `keel init`"
        );
    }
}
