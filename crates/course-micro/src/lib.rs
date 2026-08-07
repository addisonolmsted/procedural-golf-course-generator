//! Stage S9 — the micro re-pass. Corridor-resolution band-limited detail,
//! oriented by the metadata thread rather than sprinkled isotropically:
//! swale fields, scroll arcs, aeolian rumple, terracettes. Also emits the
//! firmness and TWI surface fields the playability sim reads.
//!
//! Stage doc: `docs/stages/stage-09-micro-repass.md`.

pub mod aeolian;
pub mod bands;
pub mod scroll;
pub mod surface;
pub mod swale;
pub mod terracette;
