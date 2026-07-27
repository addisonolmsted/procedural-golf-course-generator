//! Determinism kit: seeded, domain-split RNG.
//!
//! Rules this enforces:
//! - RNG is ChaCha8, seeded from `(seed, domain-label)` via blake3, so every
//!   subsystem draws from an independent, reproducible stream. Real pipeline
//!   steps open streams via [`crate::RunIdentity::stream`] with a name
//!   registered in [`crate::streams`]; `DetRng::new` is the escape hatch for
//!   fixtures and tests.
//! - Float draws use an explicit 53-bit mantissa mapping, so the u64->f64
//!   step is identical on every platform (no reliance on a crate's float
//!   sampling).

use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// Personalization tag folded into every stream key. Changing this rekeys
/// every stream in the pipeline — never change it without a pipeline_version
/// bump and a golden re-bless.
const DOMAIN_TAG: &[u8] = b"course-seed/detrng/v1";

/// Deterministic RNG bound to one seed + domain. Clone to fork a stream at a
/// known point (both copies then produce the same sequence — fork explicitly).
#[derive(Clone)]
pub struct DetRng {
    inner: ChaCha8Rng,
}

impl DetRng {
    /// Derive a stream from a 64-bit seed and a domain label. Different
    /// labels => independent streams. The label length is hashed too, so
    /// distinct labels can never collide by concatenation.
    pub fn new(seed: u64, domain: &[u8]) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(DOMAIN_TAG);
        h.update(&seed.to_le_bytes());
        h.update(&(domain.len() as u64).to_le_bytes());
        h.update(domain);
        let seed32: [u8; 32] = *h.finalize().as_bytes();
        DetRng {
            inner: ChaCha8Rng::from_seed(seed32),
        }
    }

    pub fn next_u32(&mut self) -> u32 {
        self.inner.next_u32()
    }

    pub fn next_u64(&mut self) -> u64 {
        self.inner.next_u64()
    }

    /// The value at 0-based index `n` of the remaining stream: consumes
    /// `n + 1` draws and returns the last one.
    pub fn nth_u64(&mut self, n: u64) -> u64 {
        for _ in 0..n {
            self.next_u64();
        }
        self.next_u64()
    }

    /// Uniform f64 in [0, 1) with 53 bits of mantissa. Platform-independent.
    pub fn next_f64(&mut self) -> f64 {
        // Top 53 bits of a u64, scaled into [0,1). 2^-53 is exact in f64.
        (self.next_u64() >> 11) as f64 * (1.0 / ((1u64 << 53) as f64))
    }

    /// Uniform f64 in [lo, hi). Caller ensures lo <= hi.
    pub fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }

    /// Uniform index in [0, n). Returns 0 if n == 0.
    ///
    /// Uses plain modulo, so there is bias of order n/2^64 — immeasurable
    /// for any realistic pipeline n (millions), but do NOT use this where
    /// exact uniformity matters at very large n; add a rejection-sampling
    /// variant instead (a new method, since changing this one would rekey
    /// every stream that draws from it).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = DetRng::new(42, b"macro");
        let mut b = DetRng::new(42, b"macro");
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn domain_split_diverges() {
        let mut a = DetRng::new(42, b"macro");
        let mut b = DetRng::new(42, b"route");
        // Overwhelmingly likely to differ in the first few draws.
        let diff = (0..8).any(|_| a.next_u64() != b.next_u64());
        assert!(diff);
    }

    #[test]
    fn f64_unit_interval_and_53bit() {
        let mut r = DetRng::new(7, b"x");
        for _ in 0..10_000 {
            let v = r.next_f64();
            assert!((0.0..1.0).contains(&v));
            // The mantissa mapping means v * 2^53 is an exact integer.
            let scaled = v * (1u64 << 53) as f64;
            assert_eq!(scaled, scaled.trunc());
        }
    }

    #[test]
    fn nth_u64_matches_manual_advance() {
        let mut a = DetRng::new(9, b"nth");
        let mut b = a.clone();
        let manual = {
            for _ in 0..5 {
                b.next_u64();
            }
            b.next_u64()
        };
        assert_eq!(a.nth_u64(5), manual);
    }

    /// Pinned literals lock the exact stream so any change to the RNG
    /// construction (tag, hashing, cipher) is caught immediately.
    /// Bless only on an intentional RNG-contract change (pipeline_version bump).
    const GOLDEN_U64: [u64; 4] = [
        0x446d62a334c74271,
        0x39e1d02fc765e86c,
        0xf54a517bc4d1438d,
        0xf7549c7efd9de17a,
    ];
    const GOLDEN_F64: [f64; 2] = [0.267294087262397, 0.22610188642956752];

    #[test]
    fn golden_stream_u64() {
        let mut r = DetRng::new(1, b"golden");
        let got = [r.next_u64(), r.next_u64(), r.next_u64(), r.next_u64()];
        assert_eq!(got, GOLDEN_U64);
    }

    #[test]
    fn golden_stream_f64() {
        let mut r = DetRng::new(1, b"golden");
        let got = [r.next_f64(), r.next_f64()];
        assert_eq!(got, GOLDEN_F64);
    }
}
