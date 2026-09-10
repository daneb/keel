//! `keel serve` — open the read-only view and block until interrupted.

use crate::paths::Paths;
use anyhow::Result;
use std::io::Write;

pub fn run(port: u16) -> Result<i32> {
    // Ignore SIGPIPE before touching any pipe or socket. `main` set it back to
    // the default (fatal) for ordinary commands, so `keel status | head`
    // exits quietly — but this command keeps running long after anyone has
    // read its stdout, and later writes to client sockets that may already be
    // gone. Neither a closed stdout nor a closed client connection may take
    // the whole server down, and arming this after the first print below is
    // too late for a reader (a test harness, `| head`) that stops early:
    // that print can itself race a closed pipe.
    crate::serve::ignore_sigpipe();

    let paths = Paths::require_init()?;
    // A taken port fails loudly rather than scanning upward: an operator who
    // has to guess which port they got is an operator who will eventually trust
    // the wrong process.
    let listener = crate::serve::bind(port)?;
    let addr = listener.local_addr()?;

    // Best-effort: with SIGPIPE ignored, `println!`'s own broken-pipe panic is
    // the only thing left that could take the server down if stdout is
    // already closed, so write it out directly and ignore the error.
    //
    // The literal address, never `localhost` — the server rejects any other
    // Host, which is what stops a page the operator visits from reading this
    // one after a DNS rebind.
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", crate::ui::bold(&format!("keel serve  http://{addr}/")));
    let _ = writeln!(
        out,
        "{}",
        crate::ui::dim("read-only · loopback only · Ctrl-C to stop")
    );

    crate::serve::serve(paths, listener)?;
    Ok(0)
}
