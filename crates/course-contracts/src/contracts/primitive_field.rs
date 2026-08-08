//! Contract C1 — macro primitives → skeleton kernel.
//!
//! C1 describes the site's *predisposition*, never landforms: no field here is
//! named ridge, valley, channel, or bench, and a different kernel given the
//! same C1 must be able to build something structurally unrelated. Full
//! field-by-field semantics: `docs/contracts/C1-primitives-to-kernel.md`.

use crate::error::{hex32, ContractError};
use crate::metadata::StructureMeta;
use course_world::{Grid, GridSpec};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Bumped on any field addition/removal, range change, unit change, or grid
/// rung change — together with `PIPELINE_VERSION`.
pub const C1_VERSION: u32 = 1;

/// `relief` must be mean-zero over the core to within this (metres).
pub const RELIEF_MEAN_TOL_M: f64 = 0.01;

/// The C1 artifact: four full-grid predisposition fields plus the structural
/// metadata thread.
#[derive(Clone, Debug, PartialEq)]
pub struct PrimitiveField {
    pub c1_version: u32,
    pub seed: u64,
    pub spec_digest: [u8; 32],
    /// All four fields share this spec (the 8 m rung in production; any rung
    /// in fixtures — resolution consistency is a cross-cutting rule).
    pub grid: GridSpec,
    /// m — the regional plane. Monotone toward `meta.base_level.edge` (S1's
    /// acceptance tests own that check; the constructor owns finiteness).
    pub tilt: Grid<f64>,
    /// m — low-frequency elevation predisposition, mean-zero over the core.
    pub relief: Grid<f64>,
    /// `[0, 1]` differential erosion resistance.
    pub hardness: Grid<f64>,
    /// `[0, 1]` room for deposition.
    pub accommodation: Grid<f64>,
    pub meta: StructureMeta,
}

impl PrimitiveField {
    /// Construct with every C1 invariant enforced. This is the only way to
    /// make one, so downstream code never re-checks.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        seed: u64,
        spec_digest: [u8; 32],
        grid: GridSpec,
        tilt: Grid<f64>,
        relief: Grid<f64>,
        hardness: Grid<f64>,
        accommodation: Grid<f64>,
        meta: StructureMeta,
    ) -> Result<Self, ContractError> {
        let pf = PrimitiveField {
            c1_version: C1_VERSION,
            seed,
            spec_digest,
            grid,
            tilt,
            relief,
            hardness,
            accommodation,
            meta,
        };
        pf.validate()?;
        Ok(pf)
    }

    fn validate(&self) -> Result<(), ContractError> {
        if self.c1_version != C1_VERSION {
            return Err(ContractError::Version {
                what: "c1_version",
                found: self.c1_version,
                expected: C1_VERSION,
            });
        }
        for (name, g) in [
            ("tilt", &self.tilt),
            ("relief", &self.relief),
            ("hardness", &self.hardness),
            ("accommodation", &self.accommodation),
        ] {
            if g.spec != self.grid {
                return Err(ContractError::invariant(
                    "grid",
                    format!("{name} spec differs from the shared grid spec"),
                ));
            }
            if g.data.iter().any(|v| !v.is_finite()) {
                return Err(ContractError::invariant(
                    "fields",
                    format!("{name} contains a non-finite value"),
                ));
            }
        }
        for (name, g) in [
            ("hardness", &self.hardness),
            ("accommodation", &self.accommodation),
        ] {
            if g.data.iter().any(|v| !(0.0..=1.0).contains(v)) {
                return Err(ContractError::invariant(
                    "fields",
                    format!("{name} out of [0, 1]"),
                ));
            }
        }
        // relief mean-zero over the core (C1 invariant 3). Fixture-scale grids
        // may not intersect the core; then the whole grid is the reference.
        let mut sum = 0.0;
        let mut n = 0usize;
        for y in 0..self.grid.ny {
            for x in 0..self.grid.nx {
                let p = self.grid.world_of(x, y);
                if course_world::world::in_core(p) {
                    sum += *self.relief.get(x, y);
                    n += 1;
                }
            }
        }
        if n == 0 {
            sum = self.relief.data.iter().sum();
            n = self.relief.data.len().max(1);
        }
        let mean = sum / n as f64;
        if mean.abs() > RELIEF_MEAN_TOL_M {
            return Err(ContractError::invariant(
                "relief",
                format!("core mean {mean:.4} m exceeds ±{RELIEF_MEAN_TOL_M} m"),
            ));
        }
        self.meta.validate()
    }

    // ---------------------------------------------------------- artifact io

    /// Write as files (`ARCHITECTURE.md` invariant 4): `primitives.json` for
    /// structure + one CGRID1 per field, blake3 content hashes in the JSON.
    pub fn write_dir(&self, dir: &Path) -> Result<(), ContractError> {
        std::fs::create_dir_all(dir)?;
        let mut hashes = Vec::new();
        for (name, g) in [
            ("tilt", &self.tilt),
            ("relief", &self.relief),
            ("hardness", &self.hardness),
            ("accommodation", &self.accommodation),
        ] {
            let path = dir.join(format!("{name}.cgrid"));
            course_world::gridio::write_grid_f32(&path, g)?;
            let bytes = std::fs::read(&path)?;
            hashes.push((name.to_string(), blake3::hash(&bytes).to_hex().to_string()));
        }
        let header = C1Header {
            c1_version: self.c1_version,
            seed: self.seed,
            spec_digest: self.spec_digest,
            grid: self.grid,
            meta: self.meta.clone(),
            grids: hashes
                .into_iter()
                .map(|(name, blake3)| GridRef { name, blake3 })
                .collect(),
        };
        let json = serde_json::to_string_pretty(&header)
            .map_err(|e| ContractError::Malformed { what: e.to_string() })?;
        std::fs::write(dir.join("primitives.json"), json)?;
        Ok(())
    }

    /// Load from files with loud validation: hash check, then every C1
    /// invariant. NOTE: CGRID1 stores f32, so grids round-trip through f32 —
    /// re-validation (incl. the relief mean) runs on what was actually stored.
    pub fn read_dir(dir: &Path) -> Result<Self, ContractError> {
        let json = std::fs::read_to_string(dir.join("primitives.json"))?;
        let header: C1Header =
            serde_json::from_str(&json).map_err(|e| ContractError::Malformed {
                what: format!("primitives.json: {e}"),
            })?;
        if header.c1_version != C1_VERSION {
            return Err(ContractError::Version {
                what: "c1_version",
                found: header.c1_version,
                expected: C1_VERSION,
            });
        }
        let mut fields = std::collections::BTreeMap::new();
        for gr in &header.grids {
            let path = dir.join(format!("{}.cgrid", gr.name));
            let bytes = std::fs::read(&path)?;
            if blake3::hash(&bytes).to_hex().to_string() != gr.blake3 {
                return Err(ContractError::HashMismatch {
                    file: format!("{}.cgrid", gr.name),
                });
            }
            fields.insert(gr.name.clone(), course_world::gridio::read_grid_f32(&path)?);
        }
        let mut take = |name: &str| {
            fields.remove(name).ok_or_else(|| ContractError::Malformed {
                what: format!("missing grid `{name}`"),
            })
        };
        let pf = PrimitiveField {
            c1_version: header.c1_version,
            seed: header.seed,
            spec_digest: header.spec_digest,
            grid: header.grid,
            tilt: take("tilt")?,
            relief: take("relief")?,
            hardness: take("hardness")?,
            accommodation: take("accommodation")?,
            meta: header.meta,
        };
        pf.validate()?;
        Ok(pf)
    }
}

#[derive(Serialize, Deserialize)]
struct C1Header {
    c1_version: u32,
    seed: u64,
    #[serde(with = "hex32")]
    spec_digest: [u8; 32],
    grid: GridSpec,
    meta: StructureMeta,
    grids: Vec<GridRef>,
}

#[derive(Serialize, Deserialize)]
struct GridRef {
    name: String,
    blake3: String,
}
