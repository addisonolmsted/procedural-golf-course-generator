//! The five named physiographic archetypes. The archetype id is carried end to
//! end (it is the art layer's key) and selects the parameter prior plus a few
//! fixed switches that are modes, not knobs.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchetypeId {
    /// Aeolian, sandy, high infiltration — effectively no surface hydrology.
    Sandhills,
    /// Rolling fluvial terrain + creek corridor, clay soils, high canopy.
    Piedmont,
    /// Near-zero relief, water table at the surface, lakes as primary feature.
    FloridaLowland,
    /// Closed depressions (kettles), mid relief, open canopy.
    GlacialMoraine,
    /// High relief, terraces and scarps, severe routability constraints.
    MountainBench,
}

/// How stage 5 treats water. A mode, not a knob: it changes which parameters
/// are meaningful, so it is a fixed per-archetype switch rather than a prior
/// sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    pub const ALL: [ArchetypeId; 5] = [
        ArchetypeId::Sandhills,
        ArchetypeId::Piedmont,
        ArchetypeId::FloridaLowland,
        ArchetypeId::GlacialMoraine,
        ArchetypeId::MountainBench,
    ];

    /// Stable snake_case key: prior-file key, artifact-dir slug, art-layer id.
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

    pub fn label(self) -> &'static str {
        match self {
            ArchetypeId::Sandhills => "Sandhills",
            ArchetypeId::Piedmont => "Piedmont creek corridor",
            ArchetypeId::FloridaLowland => "Florida lowland",
            ArchetypeId::GlacialMoraine => "Glacial moraine / heathland",
            ArchetypeId::MountainBench => "Mountain bench-and-valley",
        }
    }

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
    }

    #[test]
    fn serde_uses_snake_case_keys() {
        let j = serde_json::to_string(&ArchetypeId::FloridaLowland).unwrap();
        assert_eq!(j, "\"florida_lowland\"");
    }
}
