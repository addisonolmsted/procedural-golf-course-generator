//! The `plasticity` scalar: 0 = terrain dictates routing, 1 = routing dictates
//! terrain via earthmoving. Governs S6 feasibility strictness and the S7
//! grading budget. Carried from S0 through C2 and C3 so those artifacts stay
//! self-sufficient. See `docs/00-architecture.md`.

use crate::error::ContractError;
use crate::units::check_unit;
use serde::{Deserialize, Serialize};

/// A validated `[0, 1]` plasticity value. Construction is the only place the
/// range is checked; everything downstream may trust it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Plasticity(f64);

impl Plasticity {
    pub fn new(v: f64) -> Result<Self, ContractError> {
        check_unit("plasticity", v)?;
        Ok(Plasticity(v))
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_enforced_at_construction() {
        assert!(Plasticity::new(0.0).is_ok());
        assert!(Plasticity::new(1.0).is_ok());
        assert!(Plasticity::new(-0.01).is_err());
        assert!(Plasticity::new(1.01).is_err());
        assert!(Plasticity::new(f64::NAN).is_err());
    }

    #[test]
    fn serde_is_transparent() {
        let p = Plasticity::new(0.25).unwrap();
        let j = serde_json::to_string(&p).unwrap();
        assert_eq!(j, "0.25");
        let q: Plasticity = serde_json::from_str(&j).unwrap();
        assert_eq!(p, q);
    }
}
