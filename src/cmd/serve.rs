//! `keel serve` — open the read-only view and block until interrupted.

use crate::paths::Paths;
use anyhow::Result;

pub fn run(port: u16) -> Result<i32> {
    let paths = Paths::require_init()?;
    // A taken port fails loudly rather than scanning upward: an operator who
    // has to guess which port they got is an operator who will eventually trust
    // the wrong process.
    let listener = crate::serve::bind(port)?;
    let addr = listener.local_addr()?;

    // The literal address, never `localhost` — the server rejects any other
    // Host, which is what stops a page the operator visits from reading this
    // one after a DNS rebind.
    println!("{}", crate::ui::bold(&format!("keel serve  http://{addr}/")));
    println!(
        "{}",
        crate::ui::dim("read-only · loopback only · Ctrl-C to stop")
    );
    crate::serve::serve(paths, listener)?;
    Ok(0)
}
