//! `keel serve` — a read-only view of `.keel/`, on loopback, for one operator.
//!
//! # What this is not
//!
//! It is not a daemon: it runs in the foreground, holds no state, writes
//! nothing, and dies with Ctrl-C. It is not an API: the JSON under `/api/run/`
//! is this page's private wire, not a contract. And it is not a place to act —
//! there is no route that mutates, because a gate verdict and a human approval
//! are deliberate CLI acts bound to an artefact hash, and a button in a browser
//! is neither.
//!
//! # The shape of the defence
//!
//! Read `SECURITY.md`'s "keel serve opens a listening socket" section for the
//! reasoning; this is the summary of what the code must keep true.
//!
//! - **Loopback only, and `Host` is validated.** Binding to `127.0.0.1` is not
//!   sufficient on its own: any page the operator visits can re-point its own
//!   hostname at loopback once its DNS TTL expires and then read this server
//!   same-origin. Rejecting an unexpected `Host` is what actually stops that.
//! - **Read-only is enforced in the parser**, not here — see [`crate::http`].
//! - **Exactly one route touches disk by name**, and it serves only names
//!   `read_dir` returned, then canonicalises to prove containment. Everything
//!   else is compiled in with `include_str!`. Do not add a `/static/*` route.
//! - **The CSP forbids the network**, which turns "no CDN, no web fonts, no
//!   telemetry" from a policy into something the browser enforces.

use crate::http::{self, Request, Response};
use crate::paths::Paths;
use crate::report::Report;
use anyhow::{Context, Result};
use std::io::{BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const INDEX_HTML: &str = include_str!("../../assets/ui/index.html");
const APP_CSS: &str = include_str!("../../assets/ui/app.css");
const APP_JS: &str = include_str!("../../assets/ui/app.js");

/// The page may run its own script and style and talk to its own origin, and
/// nothing else. `connect-src 'self'` is what makes the offline promise
/// enforceable rather than aspirational.
const CSP_APP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
                       img-src 'self' data:; connect-src 'self'; base-uri 'none'; \
                       form-action 'none'; frame-ancestors 'none'";
/// Everything that is not the page itself, including untrusted evidence bytes.
const CSP_DATA: &str = "default-src 'none'; base-uri 'none'; form-action 'none'; \
                        frame-ancestors 'none'";

/// Evidence logs run to tens of megabytes. Serve the tail, because the failure
/// is at the end of a test log.
const MAX_EVIDENCE_BYTES: u64 = 2 * 1024 * 1024;
/// Concurrent connections. The read timeout is what actually reaps an idle
/// speculative preconnect; this is the second bound, sized so a real burst —
/// the evidence view fetches once per run — never reaches it.
const MAX_THREADS: usize = 64;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
/// Floor on recomputing the change token, so N open tabs cost one walk.
const VERSION_TTL: Duration = Duration::from_millis(250);

struct Server {
    paths: Paths,
    port: u16,
    version: Mutex<Option<(Instant, String)>>,
}

/// Serve until interrupted. Takes an already-bound listener so a test can own
/// the port.
pub fn serve(paths: Paths, listener: TcpListener) -> Result<()> {
    // `main` restores SIGPIPE to its default so `keel status | head` works. For
    // a server that default is fatal: a browser abandoning a request mid-body —
    // a refresh during a slow evidence read, which is routine — would kill the
    // process. Writes to a dead socket must come back as errors instead.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }

    let port = listener.local_addr()?.port();
    let server = Arc::new(Server { paths, port, version: Mutex::new(None) });
    let live = Arc::new(AtomicUsize::new(0));

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if live.load(Ordering::Relaxed) >= MAX_THREADS {
            // Shed rather than queue: an unbounded backlog is how a preconnect
            // storm turns into a thread leak.
            shed(&stream);
            continue;
        }
        let server = Arc::clone(&server);
        let live = Arc::clone(&live);
        live.fetch_add(1, Ordering::Relaxed);
        std::thread::spawn(move || {
            handle(&server, stream);
            live.fetch_sub(1, Ordering::Relaxed);
        });
    }
    Ok(())
}

/// Refuse a connection over capacity, without resetting it.
///
/// Closing a socket that still has an unread request in its receive buffer
/// makes the kernel send RST, which discards the response the peer never got to
/// read — so the client sees a connection reset rather than the 503 that
/// explains itself. Draining first is what makes the refusal legible.
fn shed(stream: &TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
    let mut reader = BufReader::new(stream);
    let _ = http::parse_request(&mut reader);
    let mut out = stream;
    let _ = respond(&mut out, &error_response(503, "too many connections"), false);
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

fn handle(server: &Server, stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let started = Instant::now();

    let mut reader = BufReader::new(&stream);
    let (response, method, path, head_only) = match http::parse_request(&mut reader) {
        Ok(req) => {
            let method = req.method.clone();
            let path = req.path.clone();
            let head_only = req.is_head();
            (route(server, &req), method, path, head_only)
        }
        Err(e) => {
            let status = e.status();
            // A timed-out or abandoned socket is not worth a reply.
            if matches!(e, http::Error::Io(_)) {
                return;
            }
            (error_response(status, "bad request"), "-".into(), "-".into(), false)
        }
    };

    let mut out = &stream;
    let bytes = respond(&mut out, &response, head_only).unwrap_or(0);
    eprintln!(
        "{method} {path} {} {bytes}b {}ms",
        response.status,
        started.elapsed().as_millis()
    );
}

fn respond(w: &mut impl Write, r: &Response, head_only: bool) -> std::io::Result<usize> {
    http::write_response(w, r, head_only)
}

fn error_response(status: u16, message: &str) -> Response {
    Response::new(
        status,
        "application/json; charset=utf-8",
        serde_json::json!({ "error": message }).to_string().into_bytes(),
    )
    .with_header("Content-Security-Policy", CSP_DATA)
}

// ---------------------------------------------------------------------------
// routing
// ---------------------------------------------------------------------------

fn route(server: &Server, req: &Request) -> Response {
    if let Some(rejection) = check_origin(server, req) {
        return rejection;
    }
    match req.path.as_str() {
        "/" | "/index.html" => {
            Response::html(INDEX_HTML).with_header("Content-Security-Policy", CSP_APP)
        }
        "/app.css" => Response::new(200, "text/css; charset=utf-8", APP_CSS.as_bytes().to_vec())
            .with_header("Content-Security-Policy", CSP_APP),
        "/app.js" => Response::new(
            200,
            "text/javascript; charset=utf-8",
            APP_JS.as_bytes().to_vec(),
        )
        .with_header("Content-Security-Policy", CSP_APP),
        "/api/version" => json_route(|| Ok(serde_json::json!({ "version": version(server)? }))),
        "/api/overview" => json_route(|| {
            let r = Report::build(&server.paths, None)?;
            Ok(serde_json::to_value(r)?)
        }),
        p if p.starts_with("/api/spec/") => {
            let slug = &p["/api/spec/".len()..];
            if !is_safe_segment(slug) {
                return error_response(404, "no such spec");
            }
            json_route(|| {
                let r = Report::build(&server.paths, Some(slug))?;
                Ok(serde_json::to_value(r)?)
            })
        }
        p if p.starts_with("/api/run/") => run_route(server, &p["/api/run/".len()..]),
        _ => error_response(404, "no such route"),
    }
}

/// Build a JSON body, mapping any assembly failure to 503 rather than 500.
///
/// A checkout to a branch without `.keel/` should degrade to "not available
/// right now", not read as a bug in the server.
fn json_route(build: impl FnOnce() -> Result<serde_json::Value>) -> Response {
    match build().and_then(|v| Ok(serde_json::to_string(&v)?)) {
        Ok(body) => Response::json(body).with_header("Content-Security-Policy", CSP_DATA),
        Err(e) => error_response(503, &format!("{e:#}")),
    }
}

/// `<run-id>` or `<run-id>/evidence/<name>`.
fn run_route(server: &Server, rest: &str) -> Response {
    let (id, tail) = match rest.split_once('/') {
        Some((id, tail)) => (id, Some(tail)),
        None => (rest, None),
    };
    if !is_run_id(id) {
        return error_response(404, "no such run");
    }

    match tail {
        None => json_route(|| {
            let run = crate::run::Run::load(&server.paths, id)?;
            let scan = crate::trajectory::scan(&run.trajectory_path())
                .unwrap_or(crate::trajectory::Scan { events: vec![], anomalies: vec![] });
            Ok(serde_json::json!({
                "run": run.meta,
                "gates": run.gate_results().unwrap_or_default(),
                "events": scan.events,
                "anomalies": scan.anomalies,
                "evidence": evidence_listing(&run),
            }))
        }),
        Some(t) => match t.strip_prefix("evidence/") {
            Some(name) => evidence_route(server, id, name),
            None => error_response(404, "no such route"),
        },
    }
}

fn evidence_listing(run: &crate::run::Run) -> Vec<serde_json::Value> {
    let dir = run.dir.join("evidence");
    let Ok(entries) = std::fs::read_dir(&dir) else { return vec![] };
    let mut out: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let bytes = e.metadata().ok()?.len();
            Some(serde_json::json!({ "name": name, "bytes": bytes }))
        })
        .collect();
    out.sort_by_key(|v| v["name"].as_str().unwrap_or_default().to_string());
    out
}

/// The only route that reads a file the caller named.
///
/// Three independent defences, because one is not enough on a filesystem that
/// is case-insensitive and Unicode-normalising:
///
/// 1. the name must be a single safe segment;
/// 2. it must match, by exact string equality, something `read_dir` actually
///    returned — an allowlist, which is traversal-proof by construction where a
///    `..` blocklist is not;
/// 3. the resolved path must canonicalise to somewhere inside the evidence
///    directory — this is what catches a **symlink**, which SECURITY.md already
///    grants a hostile driver could plant here.
fn evidence_route(server: &Server, id: &str, name: &str) -> Response {
    if !is_safe_segment(name) {
        return error_response(404, "no such evidence file");
    }
    let Ok(run) = crate::run::Run::load(&server.paths, id) else {
        return error_response(404, "no such run");
    };
    let dir = run.dir.join("evidence");

    let listed = std::fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_str() == Some(name));
    if !listed {
        return error_response(404, "no such evidence file");
    }

    let (Ok(root), Ok(target)) = (dir.canonicalize(), dir.join(name).canonicalize()) else {
        return error_response(404, "no such evidence file");
    };
    if !target.starts_with(&root) || !target.is_file() {
        return error_response(404, "no such evidence file");
    }

    match read_tail(&target) {
        Ok((body, total)) => {
            let truncated = total > body.len() as u64;
            // Never sniffed from the extension: these bytes are untrusted.
            let mut r = Response::plain(200, body)
                .with_header("Content-Security-Policy", CSP_DATA)
                .with_header("X-Keel-Total-Bytes", &total.to_string());
            if truncated {
                r = r.with_header("X-Keel-Truncated", "true");
            }
            r
        }
        Err(_) => error_response(404, "no such evidence file"),
    }
}

/// Read at most `MAX_EVIDENCE_BYTES`, taking the **tail** — a test log's
/// failure is at the end, so the head is the useless half.
fn read_tail(path: &std::path::Path) -> std::io::Result<(Vec<u8>, u64)> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    let total = f.metadata()?.len();
    if total > MAX_EVIDENCE_BYTES {
        f.seek(SeekFrom::Start(total - MAX_EVIDENCE_BYTES))?;
    }
    let mut buf = Vec::new();
    f.take(MAX_EVIDENCE_BYTES).read_to_end(&mut buf)?;
    Ok((buf, total))
}

// ---------------------------------------------------------------------------
// origin
// ---------------------------------------------------------------------------

/// Reject a request whose `Host` is not one this server was reached by.
///
/// This is the DNS-rebinding defence. `keel serve` prints its URL with the
/// literal address for the same reason: an operator who never types a hostname
/// never trips over the check.
fn check_origin(server: &Server, req: &Request) -> Option<Response> {
    let expected = [
        format!("127.0.0.1:{}", server.port),
        format!("[::1]:{}", server.port),
    ];
    match req.header("host") {
        Some(h) if expected.iter().any(|e| e == h) => {}
        _ => return Some(error_response(403, "unexpected Host")),
    }
    // Belt and braces on top of the Host check; no CORS header is ever sent.
    if let Some(origin) = req.header("origin") {
        let ok = expected.iter().any(|e| origin == format!("http://{e}"));
        if !ok {
            return Some(error_response(403, "cross-origin request"));
        }
    }
    None
}

/// One path segment, no separators, no dot-traversal, no control characters.
fn is_safe_segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s != "."
        && s != ".."
        && !s.contains('/')
        && !s.contains('\\')
        && !s.chars().any(|c| c.is_control())
}

/// `YYYY-MM-DD-xxx`, the shape `gate::run_id` produces.
fn is_run_id(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 14
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
        && b[10] == b'-'
        && b[11..].iter().all(u8::is_ascii_hexdigit)
}

// ---------------------------------------------------------------------------
// change token
// ---------------------------------------------------------------------------

/// A cheap hash of what `.keel/` looks like right now.
///
/// Deliberately not the store hash: that reads and digests every file, and a
/// tab polling it forever is the wrong shape. `stat` alone is enough, and
/// hashing the length alongside the timestamp catches a second write inside the
/// same coarse mtime tick — which is otherwise a silently stale page.
fn version(server: &Server) -> Result<String> {
    let mut cache = server.version.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, ref token)) = *cache
        && at.elapsed() < VERSION_TTL
    {
        return Ok(token.clone());
    }

    let mut hasher = crate::hashing::SetHasher::new();
    for root in [server.paths.specs(), server.paths.runs()] {
        stamp(&root, 3, &mut hasher);
    }
    let token = crate::hashing::short(&hasher.finish()).to_string();
    *cache = Some((Instant::now(), token.clone()));
    Ok(token)
}

fn stamp(dir: &std::path::Path, depth: usize, hasher: &mut crate::hashing::SetHasher) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut rows: Vec<(String, String)> = Vec::new();
    for e in entries.filter_map(|e| e.ok()) {
        let path = e.path();
        let Ok(meta) = e.metadata() else { continue };
        if meta.is_dir() {
            stamp(&path, depth - 1, hasher);
            continue;
        }
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        rows.push((path.display().to_string(), format!("{modified}:{}", meta.len())));
    }
    rows.sort();
    for (path, stamp) in rows {
        hasher.add(&path, stamp.as_bytes());
    }
}

/// Bind loopback and serve. There is deliberately no `--host`: a flag that
/// widens the bind is a flag that eventually ends up in an alias.
pub fn bind(port: u16) -> Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port)).with_context(|| {
        format!("binding 127.0.0.1:{port} — is another `keel serve` already running?")
    })
}
