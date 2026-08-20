//! Steps 1-3 — structural template, trunk placement, macro FIELDS.
//!
//! Attempt 4, pipeline step(s) **1-3**. See `docs/network-first/README.md`.
//!
//! **Owns:** the drainage pattern and the macro constraints it implies; 1-3 trunk mouths on the base edge; grain, resistance bands, escarpment traces, relief predisposition. FIELDS, NOT TERRAIN — no heightfield exists until step 6.
//!
//! **Expected failures** (discipline rule 6 — as binding as the targets):
//! every terrain metric. The M1 test is blind legibility: a reviewer names the drainage pattern from a field render.
//!
//! DEPENDENCY RULE: this crate may not depend on `course-skeleton`,
//! `course-primitives`, `course-amplify` or `course-transforms`. Enforced by
//! `tools/no_old_deps.sh`.
