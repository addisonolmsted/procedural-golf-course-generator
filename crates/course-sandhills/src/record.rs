//! A0 — what a Sandhills tile draws.
//!
//! Every range carries its provenance, in the shape `docs/sandhills/README.md`
//! §3 requires:
//!   `corpus`     measured in Phase 2 (`docs/sandhills/02-dune-targets.md`)
//!   `literature` `docs/network-first/04-landform-literature.md`
//!   `golf`       the macro-is-designed decision (03-macro-is-designed.md)
//!   `guess`      neither, and therefore the first thing to challenge when
//!                something looks wrong
//!
//! THE FORM CLASS IS A MIXTURE, NOT A DIAL. Orientation order over the 44 kept
//! Nebraska tiles is bimodal at Ashman D 3.96 with modes at A 0.339 / 0.756;
//! hill country over the same instrument is unimodal (D 1.88), so this is a
//! property of dune country and not of the statistic. A uniform continuum dial
//! would spend most of its range on ground that does not exist.

use crate::Mode;

/// A closed range a descriptor is drawn from.
#[derive(Clone, Copy, Debug)]
pub struct Range {
    pub lo: f64,
    pub hi: f64,
}

impl Range {
    pub const fn new(lo: f64, hi: f64) -> Self {
        Range { lo, hi }
    }
    pub const fn fixed(v: f64) -> Self {
        Range { lo: v, hi: v }
    }
    pub fn contains(&self, v: f64) -> bool {
        v >= self.lo - 1e-9 && v <= self.hi + 1e-9
    }
}

/// Which of the two dune populations a tile is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormClass {
    /// Barchanoid ridge trains — strongly oriented, long crests, the
    /// archetype's signature. Corpus A p50 0.767.
    Train,
    /// Domal / parabolic mound fields — weakly oriented. Corpus A p50 0.356.
    /// A real dune form, not a degraded train.
    Mound,
}

/// The per-class dune parameters. Both classes run the SAME generator; only
/// these numbers differ, which is what keeps the mixture honest — a mound
/// field is a dune field with a short coherence length, not a special case.
#[derive(Clone, Copy, Debug)]
pub struct FormSpec {
    /// Target spectral orientation order over the 400–1600 m band.
    /// `corpus`: train 0.767, mound 0.356 (02-dune-targets.md §3).
    pub orientation_order: Range,
    /// Crest spacing — the dominant wavelength. `corpus`: train 1301 m,
    /// mound 1184 m. Literature agrees at km scale for barchanoid ridges.
    pub wavelength_m: Range,
    /// Peak-to-trough dune relief.
    ///
    /// `golf` bounds this, NOT the corpus. 04-landform-literature.md is
    /// explicit that the clean-tile screen kept the gentle tiles — measured
    /// risers 7.4 m against barchans averaging 41 m and barchanoid ridges
    /// 90–150 m — so "literature governs the aeolian generator", and golf
    /// bounds playable dune relief to 10–35 m. The corpus band relief
    /// (train 29.2 m, mound 19.1 m) is a plausibility check, not a target.
    pub dune_relief_m: Range,
    /// How far the wind direction wanders across the tile, radians —
    /// the standard deviation of the local azimuth about the paleowind.
    ///
    /// **This is the dial that separates the two form classes.** It sets the
    /// measured orientation order and, through the curl it introduces, the
    /// crest defect density. At zero the field is a wallpaper of parallel
    /// stripes with no defect possible at any amplitude. Calibrated against
    /// the corpus per class by `--example wander_ladder`.
    pub wind_wander_rad: Range,
    /// Correlation length of that wander. `guess`.
    pub wind_wander_m: Range,
    /// Windward ramp angle, degrees. `literature`: stabilized dune stoss
    /// slopes run 5–15°.
    pub stoss_deg: Range,
    /// Lee face angle, degrees. `literature`: the angle of repose for dry
    /// sand is 30–34°, but these dunes are grass-anchored and degraded, so
    /// the shipped range is lower. Golf caps it — a 33° face is unplayable.
    pub lee_deg: Range,
    /// Share of the wavelength the stoss ramp occupies. `literature`: a
    /// barchanoid profile is strongly asymmetric, stoss 0.6–0.8 of the
    /// repeat.
    pub stoss_share: Range,
    /// Blowouts per km². `golf` — these are the bunkers.
    pub blowout_km2: Range,
}

/// One archetype record. Sandhills only, for now: attempt 5 builds ONE
/// archetype and the second one gets its own crate.
#[derive(Clone, Copy, Debug)]
pub struct Record {
    /// P(this tile is an aeolian/Nebraska tile) vs fluvial/Carolina.
    ///
    /// `guess` — a DESIGN weight, deliberately not a corpus frequency. Both
    /// modes are the archetype and a player should see both.
    pub p_aeolian: f64,
    /// P(form class is Train | aeolian mode).
    ///
    /// `golf`, NOT `corpus`. The measured mixture is 8/44 = 0.18, but that
    /// describes the protected land the clean-tile screen kept, not the dune
    /// country a player should see — and trains are the archetype's
    /// signature. Raising it is a design decision with a written reason,
    /// which is exactly what 03-macro-is-designed licenses.
    pub p_train: f64,
    pub train: FormSpec,
    pub mound: FormSpec,

    // ---- shared ----
    /// Total relief budget, metres. `golf`: must clear the proxy's 7.0 m
    /// relief floor and stay under its 81.9 m ceiling.
    pub relief_budget_m: Range,
    /// Interdune floor tilt, m/km — the regional slope the dune field sits
    /// on. `guess`.
    pub floor_tilt_m_km: Range,
    /// Depth from the interdune floor to the water table, metres. Drives
    /// interdune lakes and wet meadows. `literature`: the Nebraska Sandhills
    /// sit on the Ogallala and the table intersects the surface across much
    /// of the field — thousands of interdune lakes.
    pub water_table_m: Range,
    /// P(a spring-fed river crosses the tile). `literature`: the Dismal,
    /// Middle Loup and Snake are groundwater-sourced and cut ACROSS the dune
    /// field; they do not drain it. Sand Hills CC sits near the Middle Loup.
    pub p_allogenic_river: f64,
}

/// The measured operating point. See `docs/sandhills/02-dune-targets.md`.
pub const SANDHILLS: Record = Record {
    p_aeolian: 0.5,   // guess — even split until archetype weights are set
    p_train: 0.45,    // golf — measured 0.18; trains are the signature

    train: FormSpec {
        orientation_order: Range::new(0.62, 0.86), // corpus: p50 .767, best tile .843
        wavelength_m: Range::new(1100.0, 1400.0),  // corpus: p50 1301
        dune_relief_m: Range::new(14.0, 35.0),     // golf-bounded (lit: 41-150 m)
        wind_wander_rad: Range::new(0.20, 0.40),   // guess
        wind_wander_m: Range::new(1800.0, 3200.0), // guess
        stoss_deg: Range::new(5.0, 12.0),          // literature
        lee_deg: Range::new(16.0, 26.0),           // literature, golf-capped
        stoss_share: Range::new(0.66, 0.80),       // literature
        blowout_km2: Range::new(1.5, 5.0),         // golf
    },
    mound: FormSpec {
        orientation_order: Range::new(0.22, 0.48), // corpus: p50 .356
        wavelength_m: Range::new(900.0, 1350.0),   // corpus: p50 1184
        dune_relief_m: Range::new(10.0, 24.0),     // golf-bounded
        wind_wander_rad: Range::new(0.55, 1.10),   // guess — the low-A mode
        wind_wander_m: Range::new(700.0, 1500.0),  // guess
        stoss_deg: Range::new(4.0, 10.0),          // literature
        lee_deg: Range::new(12.0, 20.0),           // literature, golf-capped
        stoss_share: Range::new(0.58, 0.70),       // literature — less asymmetric,
        // but floored at 0.58: below that the profile stops being a dune. A
        // near-equal share puts the stoss peak slope above the lee's, which
        // inverts the slip face. Found by `the_profile_is_asymmetric...`.
        blowout_km2: Range::new(2.0, 6.5),         // golf
    },

    relief_budget_m: Range::new(18.0, 48.0), // golf: proxy band 7.0-81.9
    floor_tilt_m_km: Range::new(0.5, 4.0),   // guess
    water_table_m: Range::new(0.5, 22.0),    // literature: Ogallala intersects
    p_allogenic_river: 0.30,                 // literature
};

impl Record {
    pub const fn form(&self, c: FormClass) -> &FormSpec {
        match c {
            FormClass::Train => &self.train,
            FormClass::Mound => &self.mound,
        }
    }
}

/// The record for an archetype. One archetype for now, by design.
pub const fn record(_m: Mode) -> Record {
    SANDHILLS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_range_is_ordered_and_positive_where_it_must_be() {
        let r = SANDHILLS;
        for (name, g) in [("train", &r.train), ("mound", &r.mound)] {
            for (k, v) in [
                ("orientation_order", g.orientation_order),
                ("wavelength_m", g.wavelength_m),
                ("dune_relief_m", g.dune_relief_m),
                ("wind_wander_rad", g.wind_wander_rad),
                ("wind_wander_m", g.wind_wander_m),
                ("stoss_deg", g.stoss_deg),
                ("lee_deg", g.lee_deg),
                ("stoss_share", g.stoss_share),
                ("blowout_km2", g.blowout_km2),
            ] {
                assert!(v.lo <= v.hi, "{name}.{k}: lo {} > hi {}", v.lo, v.hi);
                assert!(v.lo >= 0.0, "{name}.{k}: negative lo");
            }
            assert!(g.stoss_share.hi < 1.0, "{name}: stoss cannot be the whole repeat");
            assert!(g.lee_deg.lo > g.stoss_deg.hi,
                    "{name}: the lee face must be steeper than the stoss ramp — \
                     that asymmetry IS the dune");
        }
    }

    #[test]
    fn trains_are_more_oriented_and_longer_waved_than_mounds() {
        // The measured split: A .767 vs .356. If a future edit collapses this,
        // the mixture has stopped meaning anything.
        let r = SANDHILLS;
        assert!(r.train.orientation_order.lo > r.mound.orientation_order.hi,
                "the two form classes must not overlap in orientation order");
        assert!(r.train.stoss_share.lo > r.mound.stoss_share.lo,
                "trains are the more asymmetric form");
    }

    #[test]
    fn relief_clears_the_golf_proxy_band() {
        // proxy_thresholds.json: relief_p95_p5 band [7.033, 81.854].
        let r = SANDHILLS;
        assert!(r.relief_budget_m.lo >= 7.033, "under the proxy relief floor");
        assert!(r.relief_budget_m.hi <= 81.854, "over the proxy relief ceiling");
        for g in [&r.train, &r.mound] {
            assert!(g.dune_relief_m.hi <= 35.0,
                    "golf bounds playable dune relief to 35 m (04-landform-literature)");
        }
    }

    #[test]
    fn probabilities_are_probabilities() {
        let r = SANDHILLS;
        for (n, p) in [("p_aeolian", r.p_aeolian), ("p_train", r.p_train),
                       ("p_allogenic_river", r.p_allogenic_river)] {
            assert!((0.0..=1.0).contains(&p), "{n} = {p}");
        }
    }
}
