use std::io::{self, Write};

use blake2::digest::{Update, VariableOutput};
use blake2::Blake2bVar;

use super::types::CombatExactStateKey;

/// Domain-separated durable encoding for exact combat identity V2.
///
/// The typed key remains free to change its in-memory packing and ordinary
/// `Hash` implementation. Persisted identities depend only on its explicit
/// serde projection and this versioned domain, never on `Debug` output or
/// Rust's process-local hashing details.
const EXACT_IDENTITY_DOMAIN_V2: &[u8] = b"sts-sim/combat-exact-state/v2\0canonical-json\0";

pub(super) fn combat_exact_identity_v2(key: &CombatExactStateKey) -> [u8; 32] {
    let mut hasher = Blake2bVar::new(32).expect("32-byte BLAKE2b output is valid");
    hasher.update(EXACT_IDENTITY_DOMAIN_V2);
    serde_json::to_writer(DigestWriter(&mut hasher), key)
        .expect("exact combat identity should serialize deterministically");
    let mut digest = [0; 32];
    hasher
        .finalize_variable(&mut digest)
        .expect("digest buffer matches configured BLAKE2b output");
    digest
}

struct DigestWriter<'a>(&'a mut Blake2bVar);

impl Write for DigestWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
