//! `keel serve` — the first listening socket keel has ever opened.
//!
//! Every test here pins a promise from SECURITY.md's "keel serve opens a
//! listening socket" section. They are named for the promise rather than the
//! mechanism, because the mechanism is allowed to change and the promise is
//! not.

mod support;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use support::{BIN, Repo};

/// A `keel serve` on an ephemeral port, killed when the test ends.
struct Server {
    child: Child,
    addr: String,
}

impl Server {
    fn start(repo: &Repo) -> Self {
        let mut child = Command::new(BIN)
            .args(["serve", "--port", "0"])
            .current_dir(&repo.dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawning keel serve");

        // The first line is the URL it bound. stdout is a pipe, so it carries
        // no escape sequences to strip.
        let mut out = BufReader::new(child.stdout.take().expect("stdout"));
        let mut line = String::new();
        out.read_line(&mut line).expect("reading the bind line");
        let addr = line
            .split("http://")
            .nth(1)
            .and_then(|s| s.split('/').next())
            .unwrap_or_else(|| panic!("no URL in {line:?}"))
            .trim()
            .to_string();
        Self { child, addr }
    }

    /// Send a raw request and return `(status, headers, body)`.
    fn raw(&self, request: &str) -> (u16, String, String) {
        let mut s = TcpStream::connect(&self.addr).expect("connecting");
        s.write_all(request.replace("{HOST}", &self.addr).as_bytes())
            .expect("writing");
        s.flush().unwrap();
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).expect("reading");
        let text = String::from_utf8_lossy(&raw).to_string();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let status = head
            .lines()
            .next()
            .and_then(|l| l.split(' ').nth(1))
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        (status, head.to_string(), body.to_string())
    }

    fn get(&self, path: &str) -> (u16, String, String) {
        self.raw(&format!("GET {path} HTTP/1.1\r\nHost: {{HOST}}\r\n\r\n"))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn working_driver() -> String {
    "#!/bin/sh\ncat > /dev/null\n\
     printf 'pub fn serve() { /* limited */ }\\n' > \"$KEEL_REPO/src/api/mod.rs\"\n\
     printf '#[test]\\nfn respects_config() { assert_eq!(1 + 1, 2); }\\n' > \"$KEEL_REPO/tests/limit.rs\"\n\
     echo '{\"schema\":\"keel.driverresult/1\",\"status\":\"ok\",\"files_changed\":[\"src/api/mod.rs\",\"tests/limit.rs\"]}'\n"
        .to_string()
}

fn repo_with_a_run(name: &str) -> Repo {
    let r = Repo::ready(name);
    r.install_driver("worker", &working_driver());
    r.ok(&["approve", "demo", "--stage", "plan"]);
    r.run(&["run", "demo"]);
    r
}

// ---------------------------------------------------------------------------
// it serves
// ---------------------------------------------------------------------------

#[test]
fn the_page_and_its_assets_are_compiled_in() {
    let r = Repo::bare("serve-assets");
    let s = Server::start(&r);
    for (path, ctype) in [
        ("/", "text/html"),
        ("/app.css", "text/css"),
        ("/app.js", "text/javascript"),
    ] {
        let (status, head, _) = s.get(path);
        assert_eq!(status, 200, "{path} was not served");
        assert!(head.contains(ctype), "{path} had the wrong type:\n{head}");
    }
}

#[test]
fn the_overview_carries_the_report() {
    let r = repo_with_a_run("serve-overview");
    let s = Server::start(&r);
    let (status, _, body) = s.get("/api/overview");
    assert_eq!(status, 200);
    let v: serde_json::Value = serde_json::from_str(&body).expect("invalid JSON");
    assert_eq!(v["schema"], "keel.report/1");
    assert_eq!(v["specs"][0]["slug"], "demo");
}

#[test]
fn insights_carries_the_executive_summary() {
    let r = repo_with_a_run("serve-insights");
    let s = Server::start(&r);
    let (status, _, body) = s.get("/api/insights");
    assert_eq!(status, 200, "{body}");
    let v: serde_json::Value = serde_json::from_str(&body).expect("invalid JSON");
    assert_eq!(v["schema"], "keel.insights/1");
    assert!(v["overview"]["runs_total"].as_u64().unwrap() > 0);
    assert!(!v["specs"].as_array().unwrap().is_empty());
    assert!(!v["checks"].as_array().unwrap().is_empty());
    assert!(!v["trend"].as_array().unwrap().is_empty(), "no week bucket for a real run: {v}");
}

#[test]
fn a_run_detail_carries_its_events_gates_and_evidence_listing() {
    let r = repo_with_a_run("serve-run");
    let s = Server::start(&r);
    let id = r.latest_run();

    let (status, _, body) = s.get(&format!("/api/run/{id}"));
    assert_eq!(status, 200, "{body}");
    let v: serde_json::Value = serde_json::from_str(&body).expect("invalid JSON");
    assert_eq!(v["run"]["id"], id.as_str());
    assert!(!v["events"].as_array().unwrap().is_empty());
    assert!(!v["evidence"].as_array().unwrap().is_empty());
}

#[test]
fn the_change_token_moves_when_the_repository_does() {
    let r = repo_with_a_run("serve-version");
    let s = Server::start(&r);
    let first: serde_json::Value = serde_json::from_str(&s.get("/api/version").2).unwrap();

    // Past the memoisation floor, then touch something under .keel/specs.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let p = ".keel/specs/demo/spec.md";
    r.write(p, &format!("{}\n", r.read(p)));

    let second: serde_json::Value = serde_json::from_str(&s.get("/api/version").2).unwrap();
    assert_ne!(first["version"], second["version"], "a change went unnoticed");
}

// ---------------------------------------------------------------------------
// it refuses
// ---------------------------------------------------------------------------

/// The DNS-rebinding defence. Loopback binding alone does not make a local
/// server private: a page the operator visits can re-point its own hostname at
/// 127.0.0.1 and read this one same-origin. Rejecting an unexpected Host is
/// what actually stops that.
#[test]
fn a_request_carrying_someone_elses_host_is_refused() {
    let r = Repo::bare("serve-rebind");
    let s = Server::start(&r);
    let (status, _, _) = s.raw("GET /api/overview HTTP/1.1\r\nHost: evil.example\r\n\r\n");
    assert_eq!(status, 403, "DNS rebinding was not defended");

    let (status, _, _) = s.raw("GET /api/overview HTTP/1.1\r\n\r\n");
    assert_eq!(status, 403, "a missing Host was accepted");
}

#[test]
fn a_cross_origin_request_is_refused_and_no_cors_header_is_ever_sent() {
    let r = Repo::bare("serve-cors");
    let s = Server::start(&r);
    let (status, _, _) = s.raw(
        "GET /api/overview HTTP/1.1\r\nHost: {HOST}\r\nOrigin: http://evil.example\r\n\r\n",
    );
    assert_eq!(status, 403);

    let (_, head, _) = s.get("/api/overview");
    assert!(
        !head.to_lowercase().contains("access-control-allow-origin"),
        "a CORS header was sent:\n{head}"
    );
}

/// Read-only is a property of the parser, not a promise about the routes.
#[test]
fn every_mutating_method_is_refused() {
    let r = repo_with_a_run("serve-readonly");
    let s = Server::start(&r);
    for method in ["POST", "PUT", "DELETE", "PATCH", "OPTIONS"] {
        let (status, _, _) =
            s.raw(&format!("{method} /api/overview HTTP/1.1\r\nHost: {{HOST}}\r\n\r\n"));
        assert_eq!(status, 405, "{method} was not refused");
    }
}

#[test]
fn a_request_with_a_body_is_refused() {
    let r = Repo::bare("serve-body");
    let s = Server::start(&r);
    let (status, _, _) =
        s.raw("GET / HTTP/1.1\r\nHost: {HOST}\r\nContent-Length: 5\r\n\r\nhello");
    assert_eq!(status, 400);
}

#[test]
fn the_evidence_route_refuses_traversal_and_symlinks() {
    let r = repo_with_a_run("serve-traversal");
    let s = Server::start(&r);
    let id = r.latest_run();

    for attempt in [
        "..%2f..%2f..%2fetc%2fpasswd",
        "..%2frun.json",
        "%2e%2e%2frun.json",
        ".",
        "..",
    ] {
        let (status, _, _) = s.get(&format!("/api/run/{id}/evidence/{attempt}"));
        assert_eq!(status, 404, "traversal via {attempt:?} was not refused");
    }

    // A hostile driver can write into evidence/ — SECURITY.md says so — and a
    // symlink is the vector an allowlist alone would miss.
    let link = r.dir.join(format!(".keel/runs/{id}/evidence/leak.txt"));
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc/passwd", &link).unwrap();
    let (status, _, body) = s.get(&format!("/api/run/{id}/evidence/leak.txt"));
    assert_eq!(status, 404, "a symlink out of the run was followed");
    assert!(!body.contains("root:"), "a symlink leaked its target");
}

#[test]
fn a_run_id_that_is_not_a_run_id_is_refused() {
    let r = Repo::bare("serve-runid");
    let s = Server::start(&r);
    for bad in ["..", "..%2f..", "not-a-run", "2026-09-09-zzz"] {
        let (status, _, _) = s.get(&format!("/api/run/{bad}"));
        assert_eq!(status, 404, "{bad:?} was accepted as a run id");
    }
}

#[test]
fn there_is_no_general_static_route() {
    let r = Repo::bare("serve-static");
    let s = Server::start(&r);
    for path in ["/static/app.js", "/.keel/keel.toml", "/Cargo.toml", "/../Cargo.toml"] {
        let (status, _, _) = s.get(path);
        assert_eq!(status, 404, "{path} was served");
    }
}

// ---------------------------------------------------------------------------
// how it answers
// ---------------------------------------------------------------------------

/// The CSP is what turns "no CDN, no web fonts, no telemetry" from a policy
/// into something the browser enforces.
#[test]
fn every_response_is_hardened() {
    let r = repo_with_a_run("serve-headers");
    let s = Server::start(&r);
    let id = r.latest_run();
    let paths = [
        "/".to_string(),
        "/app.js".to_string(),
        "/api/overview".to_string(),
        "/api/insights".to_string(),
        "/api/version".to_string(),
        format!("/api/run/{id}"),
        format!("/api/run/{id}/evidence/build.txt"),
        "/nope".to_string(),
    ];
    for path in paths {
        let (_, head, _) = s.get(&path);
        let lower = head.to_lowercase();
        assert!(lower.contains("content-security-policy:"), "{path} has no CSP:\n{head}");
        assert!(lower.contains("x-content-type-options: nosniff"), "{path}:\n{head}");
        assert!(lower.contains("referrer-policy: no-referrer"), "{path}:\n{head}");
        assert!(lower.contains("connection: close"), "{path}:\n{head}");
        assert!(!lower.contains("accept-ranges"), "{path} advertised ranges");
    }
}

#[test]
fn evidence_is_served_as_plain_text_whatever_it_is_called() {
    let r = repo_with_a_run("serve-evidence-type");
    let s = Server::start(&r);
    let id = r.latest_run();
    // `oracles.json` would be sniffed as JSON if the type came from the name.
    let (status, head, _) = s.get(&format!("/api/run/{id}/evidence/oracles.json"));
    assert_eq!(status, 200);
    assert!(
        head.to_lowercase().contains("content-type: text/plain"),
        "evidence type was taken from the filename:\n{head}"
    );
}

#[test]
fn a_large_evidence_file_is_capped_and_keeps_its_tail() {
    let r = repo_with_a_run("serve-cap");
    let id = r.latest_run();
    // The failure in a test log is at the end, so the tail is the useful half.
    let filler = "x".repeat(64);
    let mut big = String::with_capacity(3 * 1024 * 1024);
    while big.len() < 3 * 1024 * 1024 {
        big.push_str(&filler);
        big.push('\n');
    }
    big.push_str("THE-ACTUAL-FAILURE\n");
    r.write(&format!(".keel/runs/{id}/evidence/test.txt"), &big);

    let s = Server::start(&r);
    let (status, head, body) = s.get(&format!("/api/run/{id}/evidence/test.txt"));
    assert_eq!(status, 200);
    assert!(head.contains("X-Keel-Truncated: true"), "no truncation marker:\n{head}");
    assert!(head.contains("X-Keel-Total-Bytes:"), "no true size:\n{head}");
    assert!(body.len() <= 2 * 1024 * 1024 + 16, "cap not applied: {}", body.len());
    assert!(body.contains("THE-ACTUAL-FAILURE"), "the head was kept instead of the tail");
}

#[test]
fn head_reports_the_length_a_get_would_send_but_no_body() {
    let r = Repo::bare("serve-head");
    let s = Server::start(&r);
    let (status, head, body) = s.raw("HEAD / HTTP/1.1\r\nHost: {HOST}\r\n\r\n");
    assert_eq!(status, 200);
    assert!(head.contains("Content-Length:"), "{head}");
    assert!(body.is_empty(), "HEAD returned a body");

    let (_, _, get_body) = s.get("/");
    assert!(!get_body.is_empty(), "GET returned nothing to compare against");
}

#[test]
fn concurrent_connections_are_all_answered() {
    let r = repo_with_a_run("serve-concurrent");
    let s = Server::start(&r);
    let addr = s.addr.clone();
    let handles: Vec<_> = (0..20)
        .map(|_| {
            let addr = addr.clone();
            std::thread::spawn(move || {
                let mut c = TcpStream::connect(&addr).expect("connecting");
                c.write_all(
                    format!("GET /api/overview HTTP/1.1\r\nHost: {addr}\r\n\r\n").as_bytes(),
                )
                .unwrap();
                let mut out = String::new();
                c.read_to_string(&mut out).unwrap();
                out.lines().next().unwrap_or_default().to_string()
            })
        })
        .collect();
    for h in handles {
        let line = h.join().expect("a connection thread panicked");
        assert!(
            line.contains("200") || line.contains("503"),
            "unexpected status line {line:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// the page itself
// ---------------------------------------------------------------------------

/// Evidence bytes are attacker-influenced by keel's own threat model, and the
/// origin serving them can read the whole `.keel/` tree. A comment asking for
/// `textContent` survives about two edits; this does not.
#[test]
fn the_page_never_assigns_disk_content_to_inner_html() {
    let js = include_str!("../assets/ui/app.js");
    // The dot is deliberate: this looks for property access, so the file can
    // still explain in prose why it does not use these.
    assert!(
        !js.contains(".innerHTML"),
        "assets/ui/app.js uses innerHTML — build nodes and set textContent instead"
    );
    assert!(!js.contains(".outerHTML"), "assets/ui/app.js uses outerHTML");
    assert!(
        !js.contains("insertAdjacentHTML("),
        "assets/ui/app.js uses insertAdjacentHTML"
    );
}

/// The CSP forbids the network, so a stray CDN reference would be a broken page
/// rather than a leak — but it would still be a broken page.
#[test]
fn the_page_loads_nothing_from_the_network() {
    for (name, src) in [
        ("index.html", include_str!("../assets/ui/index.html")),
        ("app.css", include_str!("../assets/ui/app.css")),
        ("app.js", include_str!("../assets/ui/app.js")),
    ] {
        assert!(!src.contains("//fonts."), "{name} references a font host");
        assert!(!src.contains("cdn."), "{name} references a CDN");
        assert!(!src.contains("https://"), "{name} references an external URL");
    }
}
