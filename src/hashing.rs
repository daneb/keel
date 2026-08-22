//! Content hashing. Two hashes matter in Phase 0:
//!
//! * the **body hash** of a projection — detects a hand-edit (drift), and
//! * the **store hash** of everything a projection is rendered from — detects
//!   a projection that is merely out of date (stale).
//!
//! Keeping them distinct is what lets `keel store check` say *which* of the two
//! happened, which is the difference between "re-render" and "a human wrote
//! something you are about to throw away".

use sha2::{Digest, Sha256};

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The 12-char prefix used in projection headers. Full hashes are unreadable in
/// a diff; 48 bits is ample for detecting an edit that nobody is hiding.
pub fn short(hex: &str) -> &str {
    &hex[..hex.len().min(12)]
}

/// Hash an ordered set of (path, content) pairs. Path is included so that a
/// rename is a change even when the bytes are identical.
pub struct SetHasher {
    inner: Sha256,
}

impl SetHasher {
    pub fn new() -> Self { Self { inner: Sha256::new() } }

    pub fn add(&mut self, path: &str, content: &[u8]) {
        self.inner.update(path.as_bytes());
        self.inner.update([0u8]);
        self.inner.update((content.len() as u64).to_le_bytes());
        self.inner.update(content);
        self.inner.update([0u8]);
    }

    pub fn finish(self) -> String {
        self.inner.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl Default for SetHasher {
    fn default() -> Self { Self::new() }
}
