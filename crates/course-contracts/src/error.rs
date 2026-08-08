//! The one error type every contract constructor and loader returns.
//!
//! Loud by design (`ARCHITECTURE.md` invariant 4): a violated invariant or a
//! malformed artifact fails with a specific, named error — never a silent
//! default.

use std::fmt;

/// Why a contract could not be constructed or loaded.
#[derive(Debug)]
pub enum ContractError {
    /// A field violated a contract invariant. `what` names the field, `why`
    /// states the violated rule.
    Invariant { what: &'static str, why: String },
    /// An artifact on disk failed to parse or was internally inconsistent.
    Malformed { what: String },
    /// A version field did not match the current contract version.
    Version {
        what: &'static str,
        found: u32,
        expected: u32,
    },
    /// Filesystem failure while reading or writing an artifact.
    Io(std::io::Error),
    /// A stored blake3 content hash did not match the bytes on disk.
    HashMismatch { file: String },
}

impl ContractError {
    pub fn invariant(what: &'static str, why: impl Into<String>) -> Self {
        ContractError::Invariant {
            what,
            why: why.into(),
        }
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContractError::Invariant { what, why } => {
                write!(f, "contract invariant violated: {what}: {why}")
            }
            ContractError::Malformed { what } => write!(f, "malformed artifact: {what}"),
            ContractError::Version {
                what,
                found,
                expected,
            } => write!(f, "{what}: version {found}, expected {expected}"),
            ContractError::Io(e) => write!(f, "artifact io: {e}"),
            ContractError::HashMismatch { file } => {
                write!(f, "content hash mismatch: {file}")
            }
        }
    }
}

impl std::error::Error for ContractError {}

impl From<std::io::Error> for ContractError {
    fn from(e: std::io::Error) -> Self {
        ContractError::Io(e)
    }
}

/// Serde helper: `[u8; 32]` digests as lowercase hex strings in JSON.
pub mod hex32 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        let mut out = String::with_capacity(64);
        for b in v {
            out.push_str(&format!("{b:02x}"));
        }
        s.serialize_str(&out)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(serde::de::Error::custom("expected 64 hex chars"));
        }
        let mut out = [0u8; 32];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hi = (chunk[0] as char).to_digit(16).unwrap() as u8;
            let lo = (chunk[1] as char).to_digit(16).unwrap() as u8;
            out[i] = hi << 4 | lo;
        }
        Ok(out)
    }
}
