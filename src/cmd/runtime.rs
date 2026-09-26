//! `keel runtime fold|attest` — a runtime host's side of the evidence chain
//! (ADR-0001, ADR-0002), run on the host, never inside the sandbox.

use crate::runtime::host;
use anyhow::Result;
use std::path::Path;

pub fn fold(sink: String, chain: String, writer: String) -> Result<i32> {
    let n = host::fold(Path::new(&sink), Path::new(&chain), &writer)?;
    println!("folded {n} sink line(s) into {chain} as {writer}");
    Ok(0)
}

pub fn attest(inspect: String, chain: String, out: String, runtime: String) -> Result<i32> {
    host::attest(Path::new(&inspect), Path::new(&chain), Path::new(&out), &runtime)?;
    println!("attested {out} and recorded it in {chain}");
    Ok(0)
}
