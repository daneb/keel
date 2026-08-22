//! Shared harness for the Phase 2 integration tests.
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::{Command, Output};

pub const BIN: &str = env!("CARGO_BIN_EXE_keel");

/// Unique per call, not merely per nanosecond: the clock is coarse enough on
/// some platforms that two tests starting together share a directory.
pub fn unique_dir(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "keel-p2-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

pub struct Repo {
    pub dir: PathBuf,
}

impl Repo {
    /// A repo with a spec that has already passed G0/G1, ready to `keel run`.
    pub fn ready(name: &str) -> Self {
        let r = Self::bare(name);
        r.write_spec();
        r.ok(&["gate", "g0", "demo"]);
        r.ok(&["approve", "demo", "--stage", "spec"]);
        r.ok(&["plan", "demo"]);
        r.set_rollback("git revert the merge");
        r.write_tasks();
        r.ok(&["gate", "g1", "demo"]);
        r
    }

    pub fn bare(name: &str) -> Self {
        let dir = unique_dir(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let r = Self { dir };
        r.git(&["init", "-q"]);
        r.git(&["config", "user.email", "t@example.com"]);
        r.git(&["config", "user.name", "Test Person"]);
        r.write("Cargo.toml", "[package]\nname = \"demo\"\n");
        r.write("src/main.rs", "mod api;\nfn main() { api::serve(); }\n");
        r.write("src/api/mod.rs", "pub fn serve() {}\n");
        r.write("tests/limit.rs", "#[test]\nfn respects_config() {}\n");
        r.ok(&["init", "--yes"]);
        // A base commit, so the diff has something to be taken against.
        r.git(&["add", "-A"]);
        r.git(&["commit", "-q", "-m", "base"]);
        r.configure_verify();
        r
    }

    /// Cheap, always-green verify commands so tests exercise keel, not cargo.
    ///
    /// Edited through the toml parser rather than appended: `keel init` already
    /// writes `[verify]` and `[oracle]`, and a second copy is a duplicate-key
    /// parse error rather than an override.
    pub fn configure_verify(&self) {
        self.edit_config(|cfg| {
            let t = cfg.as_table_mut().unwrap();
            let mut verify = toml::value::Table::new();
            for k in ["build", "test", "lint"] {
                verify.insert(k.into(), toml::Value::String("true".into()));
            }
            t.insert("verify".into(), toml::Value::Table(verify));

            let mut oracle = toml::value::Table::new();
            oracle.insert("test_cmd".into(), toml::Value::String("true".into()));
            oracle.insert("doctest_cmd".into(), toml::Value::String("true".into()));
            t.insert("oracle".into(), toml::Value::Table(oracle));
        });
    }

    /// Read, mutate and rewrite `.keel/keel.toml`.
    pub fn edit_config(&self, f: impl FnOnce(&mut toml::Value)) {
        let mut cfg: toml::Value = toml::from_str(&self.read(".keel/keel.toml")).unwrap();
        f(&mut cfg);
        self.write(".keel/keel.toml", &toml::to_string_pretty(&cfg).unwrap());
    }

    pub fn write(&self, rel: &str, content: &str) -> &Self {
        let p = self.dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
        self
    }

    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.dir.join(rel))
            .unwrap_or_else(|e| panic!("reading {rel}: {e}"))
    }

    pub fn exists(&self, rel: &str) -> bool {
        self.dir.join(rel).exists()
    }

    pub fn keel(&self, args: &[&str]) -> Output {
        Command::new(BIN).args(args).current_dir(&self.dir).output().expect("running keel")
    }

    pub fn ok(&self, args: &[&str]) -> String {
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

    pub fn run(&self, args: &[&str]) -> (i32, String) {
        let out = self.keel(args);
        (
            out.status.code().unwrap_or(-1),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    }

    pub fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.dir)
            .output()
            .expect("running git");
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    pub fn write_spec(&self) {
        self.write(
            ".keel/specs/demo/spec.md",
            "---\n\
             id: SPEC-0001\n\
             slug: demo\n\
             schema: keel.spec/1\n\
             status: draft\n\
             scope:\n  - \"src/api/**\"\n  - \"tests/**\"\n\
             budget:\n  criteria: 6\n  lines: 120\n\
             ---\n\n\
             # Demo\n\n\
             ## Acceptance criteria\n\n\
             ### AC-1 Requests over the limit are rejected\n\n\
             WHEN a client exceeds 100 requests per minute THE SYSTEM SHALL respond with HTTP 429.\n\n\
             oracle: cmd `true` exit 0\n\n\
             ### AC-2 The limit is configurable\n\n\
             WHERE `rate_limit.rpm` is set THE SYSTEM SHALL use that value.\n\n\
             oracle: test tests/limit.rs::respects_config\n",
        );
    }

    /// A spec whose single criterion carries the given oracle line, for
    /// exercising one oracle kind end to end.
    pub fn write_spec_with_oracle(&self, oracle: &str) {
        self.write(
            ".keel/specs/demo/spec.md",
            &format!(
                "---\n\
                 id: SPEC-0001\nslug: demo\nschema: keel.spec/1\nstatus: draft\n\
                 scope:\n  - \"src/api/**\"\n  - \"schemas/**\"\n  - \"*.json\"\n\
                 budget:\n  criteria: 6\n  lines: 120\n---\n\n\
                 # Demo\n\n## Acceptance criteria\n\n\
                 ### AC-1 The document conforms\n\n\
                 WHEN the document is written THE SYSTEM SHALL keep it conformant.\n\n{oracle}\n"
            ),
        );
        self.write(
            ".keel/specs/demo/tasks.md",
            "---\nid: TASKS-0001\nslug: demo\nschema: keel.tasks/1\n---\n\n\
             # Tasks\n\n### T-1 Keep it conformant\n- criteria: AC-1\n\
             - files: src/api/mod.rs\n- budget: 60\n- exit: the oracle passes\n",
        );
    }

    pub fn write_tasks(&self) {
        self.write(
            ".keel/specs/demo/tasks.md",
            "---\nid: TASKS-0001\nslug: demo\nschema: keel.tasks/1\n---\n\n\
             # Tasks\n\n\
             ### T-1 Add the limiter\n\
             - criteria: AC-1, AC-2\n\
             - files: src/api/mod.rs\n\
             - budget: 60\n\
             - exit: `cargo test --test limit` exits 0\n",
        );
    }

    /// Fill in the rollback `keel plan` leaves empty.
    ///
    /// Idempotent: re-running `keel plan` preserves a rollback that is already
    /// there, so a test that re-plans must not fail for finding its own value.
    pub fn set_rollback(&self, text: &str) {
        let p = ".keel/specs/demo/plan.md";
        let content = self.read(p);
        if !content.contains("rollback: ''") && !content.contains("rollback: \"\"") {
            assert!(content.contains("rollback:"), "no rollback field at all in:\n{content}");
            return;
        }
        let replaced = content
            .replace("rollback: ''", &format!("rollback: '{text}'"))
            .replace("rollback: \"\"", &format!("rollback: '{text}'"));
        self.write(p, &replaced);
    }

    /// Install a driver script that prints `result` on stdout.
    pub fn install_driver(&self, id: &str, script: &str) {
        let path = self.dir.join(format!(".keel/drivers/{id}"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = std::fs::metadata(&path).unwrap().permissions();
            p.set_mode(0o755);
            std::fs::set_permissions(&path, p).unwrap();
        }
        // Replace the driver list entirely so only ours is present.
        let id = id.to_string();
        self.edit_config(|cfg| {
            let mut d = toml::value::Table::new();
            d.insert("id".into(), toml::Value::String(id.clone()));
            d.insert("cmd".into(), toml::Value::String(format!(".keel/drivers/{id}")));
            d.insert("default".into(), toml::Value::Boolean(true));
            d.insert("timeout_secs".into(), toml::Value::Integer(5));
            cfg.as_table_mut()
                .unwrap()
                .insert("driver".into(), toml::Value::Array(vec![toml::Value::Table(d)]));
        });
    }

    /// Re-render projections after touching the store, so an unrelated
    /// `store-drift` failure does not mask what a test is actually asserting.
    pub fn sync_store(&self) {
        self.ok(&["store", "render"]);
    }

    pub fn latest_run(&self) -> String {
        self.ok(&["runs", "--latest"]).trim().to_string()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A driver script that reports success without touching anything.
pub fn noop_driver() -> String {
    "#!/bin/sh\ncat > /dev/null\n\
     echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[],\"tokens\":10}'\n"
        .to_string()
}
