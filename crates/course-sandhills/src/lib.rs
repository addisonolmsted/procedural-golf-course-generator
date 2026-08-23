//! Attempt 5 — the Sandhills archetype, built alone.
//!
//! See `docs/sandhills/README.md`. One archetype, two structural modes
//! ([`Mode::Aeolian`] = Nebraska dune trains, [`Mode::Fluvial`] = the Carolina
//! sand cap), unified by the sand mantle and gated separately.
//!
//! **Owns:** every stage from the archetype draw to the finished 2 m
//! heightfield, for this archetype only. Routing and green-siting are not in
//! scope.
//!
//! **The architectural spine:** a structure generator emits LINES; one surface
//! synthesiser hangs ground off a line set given a section program and a
//! composition sign. Dunes are positive, valleys negative. The synthesiser is
//! shared; the structure generators are not. That is what makes the next
//! archetype cheap.
//!
//! **Expected failures** (discipline rule 6): before the texture stage exists,
//! every texture and roughness metric fails and is supposed to. The aeolian
//! mode fails every drainage metric permanently and correctly.
//!
//! DEPENDENCY RULE: this crate may depend on `course-world` and `course-seed`
//! and NOTHING else — not the retired v2 pipeline crates, and not attempt 4's
//! generator crates (`course-draw`, `course-template`, `course-network`,
//! `course-relief`). Enforced by `tools/no_old_deps.sh`. Anything wanted must
//! be added to the allowlist in `docs/sandhills/README.md` §3 first, then
//! COPIED with a provenance comment naming its source commit.

pub mod draw;
pub mod mode;
pub mod record;
pub mod rng;
pub mod surface;
pub mod wind;

pub use draw::Descriptors;
pub use mode::Mode;
pub use record::FormClass;
