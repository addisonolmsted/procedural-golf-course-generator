//! Biome and structural-class identifiers, as data.
//!
//! Stages never branch on these ids (`ARCHITECTURE.md` invariant 6) — they
//! select records: kernel, exemplar pool, dials, targets, preset, plasticity.
//! See `docs/biomes/README.md`.

use crate::error::ContractError;
use serde::{Deserialize, Serialize};

/// A biome pack. The base six are Heartland; future packs plug in behind the
/// labeled seams (`docs/biomes/future-biomes.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pack {
    Heartland,
}

/// The six Heartland biomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BiomeId {
    Piedmont,
    GreatPlains,
    RiverValley,
    Sandhills,
    Heathland,
    HillCountry,
}

impl BiomeId {
    pub const ALL: [BiomeId; 6] = [
        BiomeId::Piedmont,
        BiomeId::GreatPlains,
        BiomeId::RiverValley,
        BiomeId::Sandhills,
        BiomeId::Heathland,
        BiomeId::HillCountry,
    ];

    pub fn pack(self) -> Pack {
        Pack::Heartland
    }

    pub fn key(self) -> &'static str {
        match self {
            BiomeId::Piedmont => "piedmont",
            BiomeId::GreatPlains => "great_plains",
            BiomeId::RiverValley => "river_valley",
            BiomeId::Sandhills => "sandhills",
            BiomeId::Heathland => "heathland",
            BiomeId::HillCountry => "hill_country",
        }
    }
}

/// Where in an implied larger landscape the 3 km window sits — the
/// categorical variety draw (`docs/stages/stage-00-archetype-draw.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowClass {
    ValleyFloor,
    Interfluve,
    EscarpmentFace,
    BasinMargin,
    PiedmontSlope,
    TerraceFlight,
}

impl WindowClass {
    pub const ALL: [WindowClass; 6] = [
        WindowClass::ValleyFloor,
        WindowClass::Interfluve,
        WindowClass::EscarpmentFace,
        WindowClass::BasinMargin,
        WindowClass::PiedmontSlope,
        WindowClass::TerraceFlight,
    ];
}

/// The kind of structural discontinuity a two-province site carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryKind {
    Scarp,
    ValleyWall,
    MaterialContact,
}

/// The categorical structural draw: window class + province configuration.
///
/// Invariant: `provinces == 2` ⟺ `boundary_kind.is_some()`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StructureClass {
    pub window: WindowClass,
    pub provinces: u8,
    pub boundary_kind: Option<BoundaryKind>,
}

impl StructureClass {
    pub fn new(
        window: WindowClass,
        provinces: u8,
        boundary_kind: Option<BoundaryKind>,
    ) -> Result<Self, ContractError> {
        match (provinces, boundary_kind.is_some()) {
            (1, false) | (2, true) => Ok(StructureClass {
                window,
                provinces,
                boundary_kind,
            }),
            (1, true) => Err(ContractError::invariant(
                "structure_class",
                "one province cannot carry a boundary",
            )),
            (2, false) => Err(ContractError::invariant(
                "structure_class",
                "two provinces require a boundary_kind",
            )),
            (n, _) => Err(ContractError::invariant(
                "structure_class",
                format!("provinces must be 1 or 2, got {n}"),
            )),
        }
    }
}

/// A tile in a biome's exemplar pool — the real ground whose residual patches
/// supply a course's texture. Recorded by id so a generated course is
/// traceable to real terrain.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExemplarId(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structure_class_invariant() {
        assert!(StructureClass::new(WindowClass::Interfluve, 1, None).is_ok());
        assert!(
            StructureClass::new(WindowClass::Interfluve, 2, Some(BoundaryKind::Scarp)).is_ok()
        );
        assert!(StructureClass::new(WindowClass::Interfluve, 2, None).is_err());
        assert!(
            StructureClass::new(WindowClass::Interfluve, 1, Some(BoundaryKind::Scarp)).is_err()
        );
        assert!(StructureClass::new(WindowClass::Interfluve, 3, None).is_err());
    }

    #[test]
    fn snake_case_keys_are_stable() {
        assert_eq!(
            serde_json::to_string(&BiomeId::GreatPlains).unwrap(),
            "\"great_plains\""
        );
        assert_eq!(
            serde_json::to_string(&WindowClass::EscarpmentFace).unwrap(),
            "\"escarpment_face\""
        );
        assert_eq!(BiomeId::GreatPlains.key(), "great_plains");
    }
}
