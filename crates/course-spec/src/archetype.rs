//! The named physiographic archetypes. The archetype id is carried end to
//! end (it is the art layer's key) and selects the parameter prior plus one
//! fixed switch ([`HydrologyMode`]) that is a mode, not a knob.
//!
//! Extension rule: a NEW archetype is one enum variant here — the exhaustive
//! matches force `key`/`label`/`hydrology_mode` to be filled in — plus an
//! entry in `data/archetype_priors.json` (its knob tables and selection
//! `weight`). Everything else about an archetype is data.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchetypeId {
    /// Aeolian dune trains, sandy, high infiltration — no surface hydrology.
    Sandhills,
    /// Rolling fluvial terrain + creek corridor, clay soils, closed canopy.
    Piedmont,
    /// Near-zero relief, water table at the surface, lakes primary.
    FloridaLowland,
    /// Kettled moraine / heathland: closed depressions, mid relief, open.
    GlacialMoraine,
    /// High relief, benches/scarps/terraces, mountain streams.
    MountainBench,
}

/// How step 05 treats water. A mode, not a knob: it changes which parameters
/// are meaningful, so it is a fixed per-archetype switch rather than a prior
/// sample — and it is THE one mode switch downstream steps may read
/// (ARCHITECTURE.md invariant 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HydrologyMode {
    /// Rainfall runs off and carves; streams are the organizing feature.
    Fluvial,
    /// Rainfall infiltrates; an EMPTY hydro graph is the expected output.
    Infiltrated,
    /// A water-table surface intersects the terrain; lakes/wetlands dominate.
    WaterTable,
    /// Closed depressions are preserved (not breached) and may hold ponds.
    Kettle,
    /// Fluvial with high erodibility contrast: scarps, talus, bench streams.
    Alpine,
}

impl ArchetypeId {
    /// Every archetype, in the CODE-DECLARED selection order: the weighted
    /// archetype draw walks this array, so its order is part of the RNG
    /// contract (append new archetypes; reordering is a `pipeline_version`
    /// bump + golden re-bless).
    pub const ALL: [ArchetypeId; 5] = [
        ArchetypeId::Sandhills,
        ArchetypeId::Piedmont,
        ArchetypeId::FloridaLowland,
        ArchetypeId::GlacialMoraine,
        ArchetypeId::MountainBench,
    ];

    /// Stable snake_case key: prior-file key, artifact slug, art-layer id.
    pub fn key(self) -> &'static str {
        match self {
            ArchetypeId::Sandhills => "sandhills",
            ArchetypeId::Piedmont => "piedmont",
            ArchetypeId::FloridaLowland => "florida_lowland",
            ArchetypeId::GlacialMoraine => "glacial_moraine",
            ArchetypeId::MountainBench => "mountain_bench",
        }
    }

    pub fn from_key(k: &str) -> Option<ArchetypeId> {
        ArchetypeId::ALL.into_iter().find(|a| a.key() == k)
    }

    /// Human-facing name (viewer/tooling).
    pub fn label(self) -> &'static str {
        match self {
            ArchetypeId::Sandhills => "Sandhills",
            ArchetypeId::Piedmont => "Piedmont creek corridor",
            ArchetypeId::FloridaLowland => "Florida lowland",
            ArchetypeId::GlacialMoraine => "Glacial moraine / heathland",
            ArchetypeId::MountainBench => "Mountain bench-and-valley",
        }
    }

    /// The fixed per-archetype water mode (see [`HydrologyMode`]).
    pub fn hydrology_mode(self) -> HydrologyMode {
        match self {
            ArchetypeId::Sandhills => HydrologyMode::Infiltrated,
            ArchetypeId::Piedmont => HydrologyMode::Fluvial,
            ArchetypeId::FloridaLowland => HydrologyMode::WaterTable,
            ArchetypeId::GlacialMoraine => HydrologyMode::Kettle,
            ArchetypeId::MountainBench => HydrologyMode::Alpine,
        }
    }
}

impl std::fmt::Display for ArchetypeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_round_trips() {
        for a in ArchetypeId::ALL {
            assert_eq!(ArchetypeId::from_key(a.key()), Some(a));
        }
        assert_eq!(ArchetypeId::from_key("volcanic"), None);
    }

    #[test]
    fn serde_uses_snake_case_keys() {
        let j = serde_json::to_string(&ArchetypeId::FloridaLowland).unwrap();
        assert_eq!(j, "\"florida_lowland\"");
        let j = serde_json::to_string(&HydrologyMode::WaterTable).unwrap();
        assert_eq!(j, "\"water_table\"");
    }

    /// Pins the archetype→mode mapping (a contract, not an implementation
    /// detail: downstream steps key behavior off the mode).
    #[test]
    fn hydrology_mapping_golden() {
        let got: Vec<HydrologyMode> = ArchetypeId::ALL
            .iter()
            .map(|a| a.hydrology_mode())
            .collect();
        assert_eq!(
            got,
            [
                HydrologyMode::Infiltrated,
                HydrologyMode::Fluvial,
                HydrologyMode::WaterTable,
                HydrologyMode::Kettle,
                HydrologyMode::Alpine,
            ]
        );
    }
}
