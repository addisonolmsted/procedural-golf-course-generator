//! Step 0 — archetype draw: biome, structural class, site descriptors.
//!
//! Attempt 4, pipeline step(s) **0**. See `docs/network-first/README.md`.
//!
//! **Owns:** the categorical draw and the continuous descriptors; nothing spatial.
//!
//! **Expected failures** (discipline rule 6 — as binding as the targets):
//! every terrain metric — this stage emits no geometry.
//!
//! DEPENDENCY RULE: this crate may not depend on `course-skeleton`,
//! `course-primitives`, `course-amplify` or `course-transforms`. Enforced by
//! `tools/no_old_deps.sh`.
