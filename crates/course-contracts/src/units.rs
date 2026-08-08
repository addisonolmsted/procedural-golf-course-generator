//! Unit conventions and the direction/axis discipline.
//!
//! From `docs/01-conventions.md`: a **direction** is in `[0, 2π)` — it points
//! somewhere (flow, wind, tilt). An **axis** is in `[0, π)` — it has no head
//! or tail (grain, strata strike). Mixing them is the single most common
//! cross-stage bug in this codebase's history, so the reduction functions and
//! validators live here, once.
//!
//! Fields crossing a contract boundary carry unit suffixes (`_m`, `_rad`,
//! `_m2`, `_m3`, `_mps`); dimensionless dials are `[0, 1]`, clamped, never
//! wrapped.

use crate::error::ContractError;

pub const TAU: f64 = std::f64::consts::TAU;
pub const PI: f64 = std::f64::consts::PI;

/// Reduce an angle into the direction range `[0, 2π)`.
pub fn normalize_direction(rad: f64) -> f64 {
    let r = rad % TAU;
    if r < 0.0 {
        r + TAU
    } else {
        r
    }
}

/// Reduce an angle into the axis range `[0, π)`.
pub fn normalize_axis(rad: f64) -> f64 {
    let r = rad % PI;
    if r < 0.0 {
        r + PI
    } else {
        r
    }
}

/// Validate a direction field: finite and already in `[0, 2π)`.
pub fn check_direction(what: &'static str, rad: f64) -> Result<(), ContractError> {
    if !rad.is_finite() || !(0.0..TAU).contains(&rad) {
        return Err(ContractError::invariant(
            what,
            format!("direction must be finite in [0, 2\u{3c0}), got {rad}"),
        ));
    }
    Ok(())
}

/// Validate an axis field: finite and already in `[0, π)`.
pub fn check_axis(what: &'static str, rad: f64) -> Result<(), ContractError> {
    if !rad.is_finite() || !(0.0..PI).contains(&rad) {
        return Err(ContractError::invariant(
            what,
            format!("axis must be finite in [0, \u{3c0}), got {rad}"),
        ));
    }
    Ok(())
}

/// Validate a dimensionless dial: finite and in `[0, 1]`.
pub fn check_unit(what: &'static str, v: f64) -> Result<(), ContractError> {
    if !v.is_finite() || !(0.0..=1.0).contains(&v) {
        return Err(ContractError::invariant(
            what,
            format!("must be finite in [0, 1], got {v}"),
        ));
    }
    Ok(())
}

/// Validate a finite scalar (any range).
pub fn check_finite(what: &'static str, v: f64) -> Result<(), ContractError> {
    if !v.is_finite() {
        return Err(ContractError::invariant(
            what,
            format!("must be finite, got {v}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_normalization() {
        assert_eq!(normalize_direction(0.0), 0.0);
        assert!((normalize_direction(-0.5) - (TAU - 0.5)).abs() < 1e-12);
        assert!((normalize_direction(TAU + 1.0) - 1.0).abs() < 1e-12);
        assert!(normalize_direction(TAU) < 1e-12);
    }

    #[test]
    fn axis_normalization() {
        assert!((normalize_axis(PI + 0.25) - 0.25).abs() < 1e-12);
        assert!((normalize_axis(-0.25) - (PI - 0.25)).abs() < 1e-12);
    }

    #[test]
    fn checks_reject_out_of_range() {
        assert!(check_direction("d", TAU).is_err());
        assert!(check_direction("d", -0.1).is_err());
        assert!(check_axis("a", PI).is_err());
        assert!(check_unit("u", 1.01).is_err());
        assert!(check_unit("u", f64::NAN).is_err());
        assert!(check_finite("f", f64::INFINITY).is_err());
        assert!(check_direction("d", 1.0).is_ok());
        assert!(check_axis("a", 1.0).is_ok());
        assert!(check_unit("u", 1.0).is_ok());
    }
}
