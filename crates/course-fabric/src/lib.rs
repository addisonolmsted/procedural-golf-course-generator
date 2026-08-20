//! Steps 7-8 — texture from the dictionary, then drainage-consistent re-conditioning.
//!
//! Attempt 4, pipeline step(s) **7-8**. See `docs/network-first/README.md`.
//!
//! **Owns:** surface fabric and amplitude (step 7), then a short monotone-preserving pass that makes the fabric drain (step 8). Step 8 is a FINISHER, never the author.
//!
//! **Expected failures** (discipline rule 6 — as binding as the targets):
//! nothing is excused here — this is where the energy-distance gate applies.
//!
//! DEPENDENCY RULE: this crate may not depend on `course-skeleton`,
//! `course-primitives`, `course-amplify` or `course-transforms`. Enforced by
//! `tools/no_old_deps.sh`.
