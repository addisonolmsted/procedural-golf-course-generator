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
    /// Crest spacing. `corpus`: train 1301 m, mound 1184 m; literature agrees
    /// at km scale for barchanoid ridges.
    ///
    /// Drawn ~10% ABOVE the target because the ruler is a power-weighted mean
    /// over the 400-1600 m band, which biases toward the short end: a field
    /// built at a drawn 1250 m measured 1146 m through `spectral_order`. The
    /// range is set so the MEASURED number lands on the corpus, since that is
    /// the number the gate reads.
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
    /// Directional CONCENTRATION of the wave superposition (von Mises kappa).
    ///
    /// **This is the dial that separates the two form classes.** High kappa
    /// pulls every wave onto one axis — a train, and a single-wave field is
    /// its limit. Low kappa spreads them and the dominant axis dissolves — a
    /// mound field, at unchanged wavelength.
    ///
    /// It replaces `wind_wander_rad` in that role, which could not do it:
    /// however much ONE plane wave wobbles it still has one axis, and
    /// orientation order measures exactly that axis. Measured cap for the
    /// single-wave field was A 0.622 against the 0.356 mound target, and
    /// pushing past it collapsed the wavelength instead. `wind_wander_rad`
    /// survives as the WITHIN-axis bend, which is what makes crests terminate
    /// and merge. Calibrated by `--example kappa_ladder`.
    pub kappa: Range,
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

    // ---- the hummock tier: individual dunes riding on the ridge belts ----
    //
    // MEASURED GAP (docs/sandhills/02-dune-targets.md §10). Real Nebraska is
    // two-scale: broad ~1.3 km ridge belts separated by flat interdune
    // valleys, and the belts are PACKED with individual 50-150 m dunes. The
    // megaform alone reads as smooth swells. Band RMS, real against
    // megaform-only generated: 64-150 m 1.10 vs 0.48 (2.3x short),
    // 150-400 m 2.80 vs 1.99 (1.4x). None of the gate statistics could see
    // it -- orientation order, dominant wavelength and band relief all
    // measure 400-1600 m.
    /// Individual dune spacing, metres.
    pub hummock_lambda_m: Range,
    /// Individual dune height, metres — peak to trough.
    pub hummock_relief_m: Range,
    /// Directional concentration of the hummock field. Lower than the
    /// megaform's: individual dunes in a barchanoid belt are transverse to
    /// the same wind but far more disordered than the belt they sit on.
    pub hummock_kappa: Range,
    /// Where on the belt the hummocks start, as a fraction of megaform
    /// height. **This is what keeps the interdune floors flat** — sand piles
    /// on the belts, and the flat floors between them are the ground a course
    /// is routed on. Gate below this, full above `gate + 0.30`.
    pub hummock_gate: Range,
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
        wavelength_m: Range::new(1200.0, 1520.0),  // corpus lam_dom p50 1301
        dune_relief_m: Range::new(14.0, 35.0),     // golf-bounded (lit: 41-150 m)
        wind_wander_rad: Range::new(0.20, 0.40),   // guess
        wind_wander_m: Range::new(1800.0, 3200.0), // guess
        // 8-20, raised from 5-12. The old floor admitted direction spreads
        // up to 0.605 rad, which is MOUND territory (mounds run 0.8-1.9) and
        // produced a visible crosshatch weave -- seed 19 measured 11.3
        // orientation families against 6.6-8.2 for a well-drawn train. One
        // change fixes two things: it keeps trains reading as trains, and it
        // lifts the train orientation order, which was the tracked residual.
        kappa: Range::new(8.0, 20.0),
        stoss_deg: Range::new(5.0, 12.0),          // literature
        lee_deg: Range::new(16.0, 26.0),           // literature, golf-capped
        stoss_share: Range::new(0.66, 0.80),       // literature
        blowout_km2: Range::new(1.5, 5.0),         // golf
        // MEASURED 2026-08-22 over the 64-400 m band of the kept corpus,
        // split by the form class the megaform band assigns. Real train belts
        // read A 0.261 / lambda 216 m / band relief 11.40; real mound belts
        // read 0.291 / 228 / 10.34. The two are NEARLY IDENTICAL, so the
        // hummock dials are the same for both classes -- individual dunes do
        // not care whether the belt under them is a train or a mound field.
        // The form class distinguishes the BELTS, not the dunes on them.
        hummock_lambda_m: Range::new(190.0, 290.0), // corpus: lambda p50 216-228
        hummock_relief_m: Range::new(12.0, 21.0),   // 16-seed sweep -> band relief ~11
        hummock_kappa: Range::new(0.35, 1.10),      // 16-seed sweep -> A ~0.26
        hummock_gate: Range::new(0.30, 0.48),       // golf: floors stay flat
    },
    mound: FormSpec {
        orientation_order: Range::new(0.22, 0.48), // corpus: p50 .356
        wavelength_m: Range::new(1000.0, 1480.0),  // corpus lam_dom p50 1184
        dune_relief_m: Range::new(10.0, 24.0),     // golf-bounded
        wind_wander_rad: Range::new(0.55, 1.10),   // guess — the low-A mode
        wind_wander_m: Range::new(700.0, 1500.0),  // guess
        kappa: Range::new(0.45, 1.40),             // near the 1/sqrt(8)
        // isotropic floor of 0.354, which IS the mound target -- N_WAVES was
        // chosen to put the floor there (wind.rs::N_WAVES). Sits ABOVE the
        // ladder's own optimum because the two dials COMPOUND: the mound class
        // also draws the high wander (0.55-1.10 rad), and within-axis bending
        // suppresses orientation order further. At the ladder's kappa the
        // measured A came out 0.234, well under target.
        stoss_deg: Range::new(4.0, 10.0),          // literature
        lee_deg: Range::new(12.0, 20.0),           // literature, golf-capped
        stoss_share: Range::new(0.58, 0.70),       // literature — less asymmetric,
        // but floored at 0.58: below that the profile stops being a dune. A
        // near-equal share puts the stoss peak slope above the lee's, which
        // inverts the slip face. Found by `the_profile_is_asymmetric...`.
        blowout_km2: Range::new(2.0, 6.5),         // golf
        // Same lambda and relief as the train class -- real train and mound
        // belts carry near-identical dunes (A 0.261 vs 0.291, lambda 216 vs
        // 228). The CONCENTRATION differs, and higher, because the dials
        // COMPOUND: a mound belt is itself built from spread waves, so
        // hummocks at the train's kappa inherited that disorder on top of
        // their own and measured A 0.127 against a 0.291 target.
        hummock_lambda_m: Range::new(190.0, 290.0),
        hummock_relief_m: Range::new(11.0, 19.0),
        hummock_kappa: Range::new(1.10, 2.70),
        hummock_gate: Range::new(0.26, 0.44),
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
                ("kappa", g.kappa),
                ("stoss_deg", g.stoss_deg),
                ("lee_deg", g.lee_deg),
                ("stoss_share", g.stoss_share),
                ("blowout_km2", g.blowout_km2),
                ("hummock_lambda_m", g.hummock_lambda_m),
                ("hummock_relief_m", g.hummock_relief_m),
                ("hummock_kappa", g.hummock_kappa),
                ("hummock_gate", g.hummock_gate),
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
    fn the_concentration_ranges_do_not_overlap() {
        // kappa IS the form class. If a future edit lets the ranges overlap,
        // a "train" seed can draw a mound field and the mixture stops meaning
        // anything.
        assert!(SANDHILLS.train.kappa.lo > SANDHILLS.mound.kappa.hi,
                "train kappa {:?} overlaps mound {:?}",
                SANDHILLS.train.kappa, SANDHILLS.mound.kappa);
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
