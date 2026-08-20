//! The six Heartland archetypes and what each one draws.

use core::fmt;

/// Which structure generator step 1 selects. **Not every archetype has a
/// drainage network** — sandhills has no surface water at all, so its
/// structure is aeolian and the network generator would have nothing to do.
/// See `docs/network-first/02-drainage-patterns.md` §6.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StructureKind {
    /// A drainage tree. Five of six archetypes.
    Fluvial,
    /// Dune trains. Ridge-and-hollow lines with no flow on them.
    Aeolian,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Archetype {
    Piedmont,
    GreatPlains,
    RiverValley,
    HillCountry,
    Heathland,
    Sandhills,
}

impl Archetype {
    pub const ALL: [Archetype; 6] = [
        Archetype::Piedmont,
        Archetype::GreatPlains,
        Archetype::RiverValley,
        Archetype::HillCountry,
        Archetype::Heathland,
        Archetype::Sandhills,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Archetype::Piedmont => "piedmont",
            Archetype::GreatPlains => "great_plains",
            Archetype::RiverValley => "river_valley",
            Archetype::HillCountry => "hill_country",
            Archetype::Heathland => "heathland",
            Archetype::Sandhills => "sandhills",
        }
    }

    pub fn from_key(k: &str) -> Option<Archetype> {
        Archetype::ALL.into_iter().find(|a| a.key() == k)
    }

    pub fn structure(self) -> StructureKind {
        match self {
            Archetype::Sandhills => StructureKind::Aeolian,
            _ => StructureKind::Fluvial,
        }
    }
}

impl fmt::Display for Archetype {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}
