//! The structural mode — the one categorical the whole crate branches on.
//!
//! Both Sandhills landscapes are deep permeable sand under an aeolian surface
//! mantle. What differs is where the relief came from: Nebraska's is
//! CONSTRUCTIONAL (built by wind, positive crest lines) and Carolina's is
//! EROSIONAL (cut by blackwater creeks, negative channel lines).
//!
//! This is deliberately a MODE and not a dial. At one end the structural object
//! is a crest, at the other a channel; a 0.5 blend is ground that neither wind
//! nor water made — the "8/8 metrics in band, visually fake" zone. Variety
//! comes from a continuum INSIDE each mode.
//!
//! See `docs/sandhills/README.md` §2.

/// Which structure generator builds the tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Nebraska Sandhills: stabilized aeolian dune trains, no drainage network
    /// at all. Structure = crest lines, composed as a MAX against the interdune
    /// datum. Any water is allogenic (spring-fed rivers crossing the field) or
    /// a water table intersecting an interdune floor.
    Aeolian,
    /// Carolina Sandhills: an old sand cap on the Fall Line, dissected into
    /// broad flat-topped interfluves by low-gradient blackwater creeks.
    /// Structure = channel lines, composed as a MIN. Carries a low-amplitude
    /// relict dune mantle on the interfluves.
    Fluvial,
}

impl Mode {
    pub const ALL: [Mode; 2] = [Mode::Aeolian, Mode::Fluvial];

    /// The stable key used in file names, corpus keys and reports.
    ///
    /// These match the CORPUS keys deliberately: `sandhills` is the Nebraska
    /// tile set that has existed since v2, and `sandhills_nc` is the Carolina
    /// set added by attempt 5. Renaming the former would move every published
    /// 203-tile band and every `proxy_thresholds.json` row.
    pub const fn key(self) -> &'static str {
        match self {
            Mode::Aeolian => "sandhills",
            Mode::Fluvial => "sandhills_nc",
        }
    }

    pub fn from_key(k: &str) -> Option<Mode> {
        match k {
            "sandhills" => Some(Mode::Aeolian),
            "sandhills_nc" => Some(Mode::Fluvial),
            _ => None,
        }
    }

    /// Does this mode grow a drainage network? The aeolian answer is NO, and
    /// it is a gate, not a dial: `drainage_density ≈ 0` is measured on the
    /// finished surface, never assumed.
    pub const fn grows_channels(self) -> bool {
        matches!(self, Mode::Fluvial)
    }

    /// Sign of the structure lines against the datum. Dunes are deposited
    /// (max-composition); valleys are cut (min-composition).
    pub const fn deposits(self) -> bool {
        matches!(self, Mode::Aeolian)
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_are_distinct() {
        for m in Mode::ALL {
            assert_eq!(Mode::from_key(m.key()), Some(m));
        }
        assert_ne!(Mode::Aeolian.key(), Mode::Fluvial.key());
        assert_eq!(Mode::from_key("piedmont"), None);
    }

    #[test]
    fn the_two_modes_are_opposites() {
        // The mode exists to carry exactly this split. If a future edit makes
        // both modes agree on either axis, the mode has stopped earning itself.
        assert_ne!(Mode::Aeolian.grows_channels(), Mode::Fluvial.grows_channels());
        assert_ne!(Mode::Aeolian.deposits(), Mode::Fluvial.deposits());
    }

    #[test]
    fn aeolian_grows_no_channels() {
        assert!(!Mode::Aeolian.grows_channels());
    }
}
