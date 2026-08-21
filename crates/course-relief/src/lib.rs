//! Steps 5-6 — the macro surface and the tier-cut loop (terrain-first
//! restructure, 2026-08-21).
//!
//! Attempt 4. See `docs/network-first/README.md` and 03-macro-is-designed §8:
//! **the macro is designed for gameplay; the corpus informs it.**
//!
//! **Owns:** the heightmap. Built FROM the trunks and fields (envelope
//! composition — no second authority, no double cuts), then each tributary
//! tier is grown ON the surface and its catena cut INTO it.
//!
//! **Expected failures** (discipline rule 6): all texture and roughness
//! metrics — texture is step 7's job. Judged on: bed preservation, the golf
//! slope readout, archetype legibility of the macro forms.
//!
//! DEPENDENCY RULE: this crate may not depend on `course-skeleton`,
//! `course-primitives`, `course-amplify` or `course-transforms`. Enforced by
//! `tools/no_old_deps.sh`.

pub mod macro_surface;

pub use macro_surface::{MacroSurface, RES_M};

use course_draw::rng::stream;
use course_seed::RunIdentity;

/// Build the macro surface for a seed: template -> trunks -> envelope.
pub fn build_macro(id: &RunIdentity, forced: Option<course_draw::Archetype>) -> (
    course_template::Template,
    Vec<course_network::TrunkPath>,
    MacroSurface,
    course_draw::Descriptors,
) {
    let draw = course_draw::generate(id, forced);
    let t = course_template::build(id, &draw);
    let net = course_network::build(id, &t);
    let mut rng = stream(id, course_draw::rng::RELIEF_MACRO);
    let ms = macro_surface::build(&mut rng, &t, &draw.d, &net.trunks);
    (t, net.trunks, ms, draw.d)
}
