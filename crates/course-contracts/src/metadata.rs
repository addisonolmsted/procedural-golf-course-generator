//! The provenance thread carried alongside every field: flow direction, wind
//! azimuth, and water-plane origin. Downstream stages orient detail by this
//! rather than re-deriving it, which is what keeps grain aligned from S2 to
//! S10. See `docs/01-conventions.md`.
