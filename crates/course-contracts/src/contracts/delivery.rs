//! Contract C0 — course delivery: the bundle handed to the frontend/game
//! team.
//!
//! Manifest + plain axis-aligned raster layers, self-describing and
//! deterministic. The `prevailing_wind` is byte-identical to the vector the
//! terrain was sculpted with — one wind system, terrain and gameplay never
//! disagreeing about which way it blows. Several format choices are
//! TBD-frontend (`docs/contracts/C0-delivery.md`); the types carry the
//! pipeline defaults.

use crate::biome::{BiomeId, ExemplarId, StructureClass};
use crate::contracts::routing_substrate::{check_play_window, Rect, PLAY_LONG_MAX_M, PLAY_SHORT_MIN_M};
use crate::error::ContractError;
use crate::units::check_direction;
use serde::{Deserialize, Serialize};

pub const C0_VERSION: u32 = 1;

/// The prevailing wind: sculpting azimuth + mean speed, exported for
/// gameplay. Runtime wind may gust and vary, but its mean is this vector.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrevailingWind {
    /// DIRECTION `[0, 2π)` — where the wind blows toward.
    pub azimuth_rad: f64,
    pub speed_mps: f64,
}

/// World-frame facts the consumer must not have to guess.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldDesc {
    pub extent_m: f64,
    /// "SW" — the origin corner.
    pub origin: String,
    /// "east" / "north" — axis senses.
    pub x: String,
    pub y: String,
}

impl Default for WorldDesc {
    fn default() -> Self {
        WorldDesc {
            extent_m: course_world::world::EXTENT_M,
            origin: "SW".into(),
            x: "east".into(),
            y: "north".into(),
        }
    }
}

/// Encoding of a quantized raster layer; absent for raw f32.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerEncoding {
    pub scale: f64,
    pub offset: f64,
}

/// One raster layer in the bundle. Every layer is a plain axis-aligned grid;
/// `resolution_m` is a ladder rung (8 / 2 / 0.5) so layers nest exactly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayerEntry {
    pub name: String,
    pub path: String,
    /// e.g. "f32le", "png16", "u8".
    pub format: String,
    pub resolution_m: f64,
    pub extent: Rect,
    pub dtype: String,
    pub encoding: Option<LayerEncoding>,
    pub blake3: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub exemplars: Vec<ExemplarId>,
    pub envelope_fingerprint: String,
    pub dictionary_fingerprint: String,
}

/// `manifest.json` — the index of a course bundle. Always read first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CourseManifest {
    pub c0_version: u32,
    pub seed: u64,
    pub pipeline_version: u32,
    pub biome: BiomeId,
    pub structure_class: StructureClass,
    pub play_window: Rect,
    pub world: WorldDesc,
    pub datum_note: String,
    pub prevailing_wind: PrevailingWind,
    pub layers: Vec<LayerEntry>,
    pub provenance: Provenance,
}

impl CourseManifest {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.c0_version != C0_VERSION {
            return Err(ContractError::Version {
                what: "c0_version",
                found: self.c0_version,
                expected: C0_VERSION,
            });
        }
        check_direction("prevailing_wind.azimuth_rad", self.prevailing_wind.azimuth_rad)?;
        if !self.prevailing_wind.speed_mps.is_finite() || self.prevailing_wind.speed_mps < 0.0 {
            return Err(ContractError::invariant(
                "prevailing_wind.speed_mps",
                "must be finite and >= 0",
            ));
        }
        // C0 does not carry the biome preset, so validate the window against
        // its own side lengths -- the bounds check inside catches nonsense.
        {
            let sx = self.play_window.max.x - self.play_window.min.x;
            let sy = self.play_window.max.y - self.play_window.min.y;
            let long = sx.max(sy).clamp(PLAY_SHORT_MIN_M, PLAY_LONG_MAX_M);
            let short = sx.min(sy).clamp(PLAY_SHORT_MIN_M, long);
            check_play_window(&self.play_window, long, short)?;
        }
        const RUNGS: [f64; 3] = [8.0, 2.0, 0.5];
        for l in &self.layers {
            if !RUNGS.iter().any(|r| (l.resolution_m - r).abs() < 1e-9) {
                return Err(ContractError::invariant(
                    "layers.resolution_m",
                    format!("layer `{}`: {} m is not a ladder rung", l.name, l.resolution_m),
                ));
            }
        }
        Ok(())
    }
}
