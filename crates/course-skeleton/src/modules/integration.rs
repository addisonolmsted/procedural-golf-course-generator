//! The drainage-integration dial: how completely the network connects.
//! Heathland runs it negative (density ~0.07) for a deranged network that
//! does not reach base level.
//!
//! Purely structural, expressed in two places the engine already has:
//! - **network derangement** (in `tributary::build`): a negative dial drops
//!   a deterministic fraction of sub-trunk channels before they exist;
//! - **incision scale** (here): shallower valleys for deranged ground, so
//!   what channels remain read as swales among kettles, not canyons.

/// Fraction of sub-trunk channels to drop, from the signed dial.
pub fn derangement(integration: f64) -> f64 {
    (-integration).clamp(0.0, 1.0) * 0.75
}

/// Incision multiplier from the signed dial: full at ≥0, down to 0.35 at −1.
pub fn incision_scale(integration: f64) -> f64 {
    if integration >= 0.0 {
        1.0
    } else {
        1.0 + integration * 0.65
    }
}
