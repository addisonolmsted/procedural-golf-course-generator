//! Stage S1 — macro structure. Realizes the drawn window class as the site's
//! predisposition — tilt, band-limited relief, hardness, accommodation, the
//! structural fabric, and the discontinuity list — and emits contract C1.
//!
//! S1 does not place landforms: it places the conditions under which a kernel
//! will place landforms. The categorical window class and the discontinuities
//! are the multiplicative variety carriers (variation *in kind*); everything
//! here is built from smooth primitives at wavelengths >= 400 m so the C1
//! band-limit holds by construction.
//!
//! Stage doc: `docs/stages/stage-01-macro-primitives.md`.
//! Output contract: `docs/contracts/C1-primitives-to-kernel.md`.

mod classes;
mod discontinuity;
mod generate;

pub use generate::generate;

/// S1's construction-recipe version (C1's own version is the contract's).
pub const PRIMITIVES_VERSION: u32 = 1;
