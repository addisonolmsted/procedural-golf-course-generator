//! The provenance thread: the structural metadata carried alongside every
//! field from C1 through C2.
//!
//! `grain_axis_rad` and `wind_azimuth_rad` pass through the pipeline
//! **bit-identically** — S5 may not recompute or refine them. That single
//! shared origin is what keeps S9's micro detail aligned with S2's macro
//! structure nine stages later; when it drifts, no one stage looks wrong, the
//! terrain just stops reading as one place. See `docs/01-conventions.md` and
//! `docs/contracts/C1-primitives-to-kernel.md`.

use crate::error::ContractError;
use crate::units::{check_axis, check_direction, check_finite, check_unit};
use serde::{Deserialize, Serialize};

/// The edge (or corner) of the box where water leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    N,
    E,
    S,
    W,
    CornerNe,
    CornerNw,
    CornerSe,
    CornerSw,
}

/// Where water leaves the box and at what elevation (local datum). The one
/// piece of C1 a kernel may not reinterpret.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BaseLevel {
    pub edge: Edge,
    pub elev_m: f64,
}

/// One stratum in the stack, ordered top-down.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stratum {
    pub thickness_m: f64,
    /// Differential erosion resistance, `[0, 1]`.
    pub hardness: f64,
    pub dip_rad: f64,
    /// Strike is an AXIS in `[0, π)`.
    pub strike_axis_rad: f64,
}

/// Why water is where it is — carried to C2 so S10 can dress a kettle
/// differently from an oxbow without asking S4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterPlaneOrigin {
    /// No standing water on this course (legal: Sandhills, often Great
    /// Plains).
    None,
    WaterTable,
    Floodplain,
    ClosedBasin,
}

/// The structural metadata block of contract C1, passed through unchanged.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StructureMeta {
    /// AXIS in `[0, π)` — the material/structural fabric. The origin of every
    /// oriented feature in the pipeline.
    pub grain_axis_rad: f64,
    /// `[0, 1]`; 0 = isotropic.
    pub grain_strength: f64,
    pub base_level: BaseLevel,
    /// DIRECTION in `[0, 2π)` — the prevailing wind. Always declared, even by
    /// biomes that run the aeolian machinery at zero.
    pub wind_azimuth_rad: f64,
    /// May be empty (Heathland, Sandhills) — the empty case is legal.
    pub strata: Vec<Stratum>,
}

impl StructureMeta {
    /// Validate every range. Called by every contract constructor that embeds
    /// this block, so a violated range cannot travel.
    pub fn validate(&self) -> Result<(), ContractError> {
        check_axis("meta.grain_axis_rad", self.grain_axis_rad)?;
        check_unit("meta.grain_strength", self.grain_strength)?;
        check_finite("meta.base_level.elev_m", self.base_level.elev_m)?;
        check_direction("meta.wind_azimuth_rad", self.wind_azimuth_rad)?;
        for (i, s) in self.strata.iter().enumerate() {
            if !s.thickness_m.is_finite() || s.thickness_m <= 0.0 {
                return Err(ContractError::invariant(
                    "meta.strata.thickness_m",
                    format!("stratum {i}: must be finite and > 0, got {}", s.thickness_m),
                ));
            }
            check_unit("meta.strata.hardness", s.hardness)?;
            check_finite("meta.strata.dip_rad", s.dip_rad)?;
            check_axis("meta.strata.strike_axis_rad", s.strike_axis_rad)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> StructureMeta {
        StructureMeta {
            grain_axis_rad: 1.0,
            grain_strength: 0.5,
            base_level: BaseLevel {
                edge: Edge::S,
                elev_m: -12.0,
            },
            wind_azimuth_rad: 4.0,
            strata: vec![Stratum {
                thickness_m: 5.0,
                hardness: 0.8,
                dip_rad: 0.02,
                strike_axis_rad: 1.2,
            }],
        }
    }

    #[test]
    fn valid_meta_passes() {
        assert!(meta().validate().is_ok());
    }

    #[test]
    fn empty_strata_is_legal() {
        let mut m = meta();
        m.strata.clear();
        assert!(m.validate().is_ok());
    }

    #[test]
    fn ranges_are_enforced() {
        let mut m = meta();
        m.grain_axis_rad = crate::units::PI; // axis range is half-open
        assert!(m.validate().is_err());

        let mut m = meta();
        m.wind_azimuth_rad = -0.1;
        assert!(m.validate().is_err());

        let mut m = meta();
        m.strata[0].thickness_m = 0.0;
        assert!(m.validate().is_err());

        let mut m = meta();
        m.strata[0].hardness = 1.5;
        assert!(m.validate().is_err());
    }
}
