//! The analytic swept-primitive vocabulary, ported from
//! `terrain-v2:golf-landform`. Each primitive is authored by parameters and
//! evaluated as a pure function of a world point — resolution never enters.
//!
//! Port notes (vs the terrain-v2 originals):
//! - geometry/ease/noise substrate now comes from `course-world`;
//! - the meander's internal RNG is `course-seed`'s `DetRng` (same
//!   blake3+ChaCha8 design, different domain tag — streams re-blessed here).

pub mod bluff;
pub mod bowl;
pub mod meander;
pub mod valley;
