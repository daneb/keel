//! A very small HTTP/1.1 server, for `keel serve` and nothing else.
//!
//! This module knows nothing about keel. It parses a request and writes a
//! response; what those mean is [`crate::serve`]'s problem.
//!
//! # Why this is hand-rolled
//!
//! Not to save weight — a crate that bundles all of SQLite and seven
//! tree-sitter grammars saves nothing meaningful by avoiding `tiny_http`. It is
//! hand-rolled because the server needs no keep-alive, no streaming, no
//! chunked encoding and no TLS, which makes the surface small enough to read in
//! one sitting and audit in another. `src/mcp` hand-rolls JSON-RPC for the same
//! reason. **If keep-alive, streaming or TLS is ever wanted, take the
//! dependency instead of growing this.**
//!
//! # What it refuses, and why that is structural
//!
//! - Anything but `GET` and `HEAD` is rejected here, before routing, so
//!   "read-only" is a property of the parser rather than a promise about the
//!   routes.
//! - A request carrying a body, a non-zero `Content-Length`, or any
//!   `Transfer-Encoding` is rejected outright. That removes request smuggling
//!   as a class rather than defending against it case by case.
//! - Every length is bounded, so a peer cannot make the server allocate.
//! - Responses always carry `Content-Length` and always close. Knowing the
//!   length up front is only possible because bodies are capped by the caller,
//!   so those two decisions travel together.

use std::io::{BufRead, Read, Write};

/// Longest request line accepted, including the method and target.
const MAX_REQUEST_LINE: usize = 8 * 1024;
/// Longest single header line.
const MAX_HEADER_LINE: usize = 8 * 1024;
/// Total header bytes accepted.
const MAX_HEADERS_BYTES: usize = 16 * 1024;
/// Most headers accepted.
const MAX_HEADERS: usize = 64;

/// Why a request could not be served, and the status that says so.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Unparseable, or a shape this server refuses. 400.
    Malformed(&'static str),
    /// The request line or headers exceeded a limit. 431.
    TooLarge,
    /// Not `GET` or `HEAD`. 405.
    MethodNotAllowed,
    /// The peer went away or the socket timed out. Only the kind is kept:
    /// `std::io::Error` is not comparable, and nothing here needs more than
    /// "the conversation ended".
    Io(std::io::ErrorKind),
}

impl Error {
    pub fn status(&self) -> u16 {
        match self {
            Self::Malformed(_) => 400,
            Self::TooLarge => 431,
            Self::MethodNotAllowed => 405,
            Self::Io(_) => 400,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.kind())
    }
}

#[derive(Debug)]
pub struct Request {
    pub method: String,
    /// Percent-decoded path. Any query string is split off and discarded — no
    /// route takes a parameter, and keeping one invites a route that does.
    pub path: String,
    /// Header names lowercased; values trimmed.
    headers: Vec<(String, String)>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn is_head(&self) -> bool {
        self.method == "HEAD"
    }
}

/// Read one line, bounded, and strip its CRLF.
fn read_line(reader: &mut impl BufRead, max: usize) -> Result<String, Error> {
    let mut buf = Vec::new();
    let mut limited = reader.take(max as u64 + 1);
    limited.read_until(b'\n', &mut buf)?;
    if buf.len() > max {
        return Err(Error::TooLarge);
    }
    if buf.last() != Some(&b'\n') {
        // No terminator within the limit, or the peer closed mid-line.
        if buf.is_empty() {
            return Err(Error::Malformed("empty request"));
        }
        return Err(Error::Malformed("unterminated line"));
    }
    buf.pop();
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    String::from_utf8(buf).map_err(|_| Error::Malformed("request is not UTF-8"))
}

/// Decode `%xx` once. Rejects a NUL byte and anything that is not UTF-8.
///
/// Decoding exactly once matters: a value that still contains an escape after
/// decoding must stay escaped, or `%252e%252e` becomes `..` for whoever looks
/// at it next.
fn percent_decode(s: &str) -> Result<String, Error> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(Error::Malformed("truncated percent-escape"));
            }
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3])
                .map_err(|_| Error::Malformed("bad percent-escape"))?;
            let b = u8::from_str_radix(hex, 16).map_err(|_| Error::Malformed("bad percent-escape"))?;
            if b == 0 {
                return Err(Error::Malformed("NUL in path"));
            }
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| Error::Malformed("path is not UTF-8"))
}

/// Parse one request. Never reads a body, because a body is never allowed.
pub fn parse_request(reader: &mut impl BufRead) -> Result<Request, Error> {
    let line = read_line(reader, MAX_REQUEST_LINE)?;
    let mut parts = line.split(' ');
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().ok_or(Error::Malformed("no request target"))?;
    let version = parts.next().ok_or(Error::Malformed("no HTTP version"))?;
    if parts.next().is_some() {
        return Err(Error::Malformed("extra tokens in request line"));
    }
    if !version.starts_with("HTTP/1.") {
        return Err(Error::Malformed("unsupported HTTP version"));
    }
    // Read-only is decided here, before anything looks at the path.
    if method != "GET" && method != "HEAD" {
        return Err(Error::MethodNotAllowed);
    }

    let raw_path = match target.split_once('?') {
        Some((p, _)) => p,
        None => target,
    };
    if !raw_path.starts_with('/') {
        return Err(Error::Malformed("request target is not an absolute path"));
    }
    let path = percent_decode(raw_path)?;

    let mut headers = Vec::new();
    let mut total = 0usize;
    loop {
        let line = read_line(reader, MAX_HEADER_LINE)?;
        if line.is_empty() {
            break;
        }
        total += line.len();
        if total > MAX_HEADERS_BYTES || headers.len() >= MAX_HEADERS {
            return Err(Error::TooLarge);
        }
        let (name, value) = line
            .split_once(':')
            .ok_or(Error::Malformed("header without a colon"))?;
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }

    let req = Request { method, path, headers };

    // A body is never legal here, and refusing both framing headers outright
    // removes request smuggling rather than trying to reconcile them.
    if req.header("transfer-encoding").is_some() {
        return Err(Error::Malformed("a body is not allowed"));
    }
    if let Some(len) = req.header("content-length")
        && len.trim() != "0"
    {
        return Err(Error::Malformed("a body is not allowed"));
    }

    Ok(req)
}

pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type: content_type.to_string(),
            headers: Vec::new(),
            body,
        }
    }

    pub fn json(body: String) -> Self {
        Self::new(200, "application/json; charset=utf-8", body.into_bytes())
    }

    pub fn html(body: &str) -> Self {
        Self::new(200, "text/html; charset=utf-8", body.as_bytes().to_vec())
    }

    /// Always `text/plain`, never sniffed from a filename — evidence bytes are
    /// untrusted, and letting the browser guess is how they become script.
    pub fn plain(status: u16, body: Vec<u8>) -> Self {
        Self::new(status, "text/plain; charset=utf-8", body)
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

/// Hardening that is correct for every response this server can produce.
///
/// `Content-Security-Policy` is deliberately not here: the app page needs
/// `'self'` for its own script and style, while everything else needs
/// `'none'`, so each response states its own.
const ALWAYS: &[(&str, &str)] = &[
    ("X-Content-Type-Options", "nosniff"),
    ("Referrer-Policy", "no-referrer"),
    ("Cache-Control", "no-store"),
    ("Vary", "Origin"),
];

/// Write a response and close. `head_only` suppresses the body but keeps the
/// `Content-Length` a GET would have reported.
pub fn write_response(w: &mut impl Write, r: &Response, head_only: bool) -> std::io::Result<usize> {
    let mut head = format!("HTTP/1.1 {} {}\r\n", r.status, reason(r.status));
    head.push_str(&format!("Content-Type: {}\r\n", r.content_type));
    head.push_str(&format!("Content-Length: {}\r\n", r.body.len()));
    // No keep-alive state machine, and no `Accept-Ranges`: a server may ignore
    // `Range`, and this one does.
    head.push_str("Connection: close\r\n");
    for (k, v) in ALWAYS {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    for (k, v) in &r.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");

    w.write_all(head.as_bytes())?;
    if !head_only {
        w.write_all(&r.body)?;
    }
    w.flush()?;
    Ok(if head_only { 0 } else { r.body.len() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    fn parse(raw: &str) -> Result<Request, Error> {
        parse_request(&mut BufReader::new(raw.as_bytes()))
    }

    /// `Request` is deliberately not `PartialEq` — nothing but a test would
    /// compare two of them — so error cases assert on the error alone.
    fn parse_err(raw: &str) -> Error {
        match parse(raw) {
            Err(e) => e,
            Ok(r) => panic!("expected a refusal, parsed {r:?}"),
        }
    }

    #[test]
    fn parses_a_plain_get() {
        let r = parse("GET /api/overview HTTP/1.1\r\nHost: 127.0.0.1:7717\r\n\r\n").unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.path, "/api/overview");
        assert_eq!(r.header("host"), Some("127.0.0.1:7717"));
    }

    #[test]
    fn a_query_string_never_reaches_the_path() {
        let r = parse("GET /api/run/x?from=5&limit=10 HTTP/1.1\r\n\r\n").unwrap();
        assert_eq!(r.path, "/api/run/x");
    }

    #[test]
    fn header_names_are_matched_case_insensitively() {
        let r = parse("GET / HTTP/1.1\r\nHOST: x\r\nOrIgIn: y\r\n\r\n").unwrap();
        assert_eq!(r.header("host"), Some("x"));
        assert_eq!(r.header("origin"), Some("y"));
    }

    #[test]
    fn every_mutating_method_is_refused_before_routing() {
        for m in ["POST", "PUT", "DELETE", "PATCH", "OPTIONS", "TRACE", "CONNECT"] {
            let raw = format!("{m} / HTTP/1.1\r\n\r\n");
            assert_eq!(parse_err(&raw), Error::MethodNotAllowed, "{m} was accepted");
        }
        assert_eq!(Error::MethodNotAllowed.status(), 405);
    }

    #[test]
    fn head_is_allowed() {
        assert!(parse("HEAD / HTTP/1.1\r\n\r\n").unwrap().is_head());
    }

    #[test]
    fn a_body_is_refused_however_it_is_framed() {
        assert_eq!(
            parse_err("GET / HTTP/1.1\r\nContent-Length: 12\r\n\r\n"),
            Error::Malformed("a body is not allowed")
        );
        assert_eq!(
            parse_err("GET / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"),
            Error::Malformed("a body is not allowed")
        );
        // An explicit zero is honest and harmless.
        assert!(parse("GET / HTTP/1.1\r\nContent-Length: 0\r\n\r\n").is_ok());
    }

    #[test]
    fn an_oversized_request_line_is_refused_not_buffered() {
        let raw = format!("GET /{} HTTP/1.1\r\n\r\n", "a".repeat(MAX_REQUEST_LINE));
        assert_eq!(parse_err(&raw), Error::TooLarge);
        assert_eq!(Error::TooLarge.status(), 431);
    }

    #[test]
    fn too_many_headers_is_refused() {
        let mut raw = String::from("GET / HTTP/1.1\r\n");
        for i in 0..MAX_HEADERS + 5 {
            raw.push_str(&format!("X-Pad-{i}: v\r\n"));
        }
        raw.push_str("\r\n");
        assert_eq!(parse_err(&raw), Error::TooLarge);
    }

    #[test]
    fn percent_escapes_are_decoded_exactly_once() {
        let r = parse("GET /api/run/a%2Fb HTTP/1.1\r\n\r\n").unwrap();
        assert_eq!(r.path, "/api/run/a/b");
        // Double-encoded stays encoded, so a later decode cannot resurrect it.
        let r = parse("GET /%252e%252e HTTP/1.1\r\n\r\n").unwrap();
        assert_eq!(r.path, "/%2e%2e");
    }

    #[test]
    fn a_nul_byte_in_the_path_is_refused() {
        assert_eq!(parse_err("GET /a%00b HTTP/1.1\r\n\r\n"), Error::Malformed("NUL in path"));
    }

    #[test]
    fn a_target_that_is_not_an_absolute_path_is_refused() {
        assert!(parse("GET http://evil/ HTTP/1.1\r\n\r\n").is_err());
        assert!(parse("GET * HTTP/1.1\r\n\r\n").is_err());
    }

    #[test]
    fn a_response_always_closes_and_states_its_length() {
        let mut out = Vec::new();
        let r = Response::json("{\"a\":1}".into());
        write_response(&mut out, &r, false).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(s.contains("Content-Length: 7\r\n"));
        assert!(s.contains("Connection: close\r\n"));
        assert!(s.contains("X-Content-Type-Options: nosniff\r\n"));
        assert!(!s.contains("Accept-Ranges"), "range support was advertised");
        assert!(s.ends_with("{\"a\":1}"));
    }

    #[test]
    fn head_keeps_the_headers_and_drops_the_body() {
        let r = Response::json("{\"a\":1}".into());
        let mut get = Vec::new();
        let mut head = Vec::new();
        write_response(&mut get, &r, false).unwrap();
        write_response(&mut head, &r, true).unwrap();

        let head = String::from_utf8(head).unwrap();
        assert!(head.contains("Content-Length: 7\r\n"), "HEAD lied about the length");
        assert!(head.ends_with("\r\n\r\n"), "HEAD wrote a body");
        assert!(get.len() > head.len());
    }
}
