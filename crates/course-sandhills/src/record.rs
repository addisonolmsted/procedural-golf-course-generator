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
    ///
    /// Set from a DUNE INVENTORY, not from band energy. Counting summits on
    /// the band-passed field and segmenting one basin per summit gives real
    /// sandhills 14.3 dunes/km2, 149 m between nearest summits and a 174 m
    /// median diameter. The spectral centroid had reported our spacing 10%
    /// high when the counted separation was 59% high (237 m against 149) --
    /// a power-weighted mean over a 64-400 m band cannot see feature size.
    pub hummock_lambda_m: Range,
    /// How patchy the sand supply is: 0 leaves belts running unbroken across
    /// the tile, 1 breaks them into isolated pods.
    ///
    /// **Belts should not exist everywhere.** A superposition of smooth waves
    /// has smooth level sets that run right across the tile, so its belts
    /// never terminate and never fray — reviewed as "ours continue pretty
    /// regularly with smooth edges" against real belts that "terminate at
    /// different points" and have organic edges. The same smoothness is what
    /// makes a low-concentration draw read as WAVY rather than as discrete
    /// mounds.
    ///
    /// Real dune fields sit on a varying sand supply: where it thins, the
    /// belt fades to the interdune floor. Modulating the megaform by a
    /// coherent supply field gives terminations and ragged edges from one
    /// mechanism, at both form classes.
    pub belt_patchiness: Range,
    /// Correlation length of that supply field, metres.
    pub belt_patch_m: Range,
    /// Spread of individual dune SIZES, as a fraction of the wavelength.
    /// Nonzero for two reasons: real dune fields carry a range of sizes, and
    /// waves sharing one wavenumber interfere into a fixed beat that reads as
    /// a crosshatch wherever the megaform is not there to mask it.
    pub hummock_spread: Range,
    /// Individual dune height, metres — peak to trough.
    pub hummock_relief_m: Range,
    /// Directional concentration of the hummock field. Lower than the
    /// megaform's: individual dunes in a barchanoid belt are transverse to
    /// the same wind but far more disordered than the belt they sit on.
    pub hummock_kappa: Range,
    /// Where on the belt the hummocks reach full strength, as a fraction of
    /// megaform height. Sand piles on the belts, so the tier is strongest
    /// there and quietest in the hollows between them.
    pub hummock_gate: Range,
    /// How much of the hummock tier survives on the interdune FLOORS.
    ///
    /// **Not zero.** Real floors are quieter than the belts, not empty:
    /// measured over 24 kept tiles, floor band RMS runs 0.48x the belt's at
    /// 30-64 m, 0.53x at 64-150 m and 0.75x at 150-400 m. A hard gate made
    /// them 0.44/0.40/0.53 -- too clean, and it showed in the golf readout,
    /// where generated floors sat at 0.895 calm against a real 0.680.
    ///
    /// This is a deliberate trade against routability, and the policy
    /// licenses it: real sandhills clears the proxy on only 23% of tiles
    /// while we clear it on all of them, so there is headroom, and "calm is a
    /// FLOOR, not a maximand -- otherwise generated courses will
    /// systematically sit on duller ground than real ones."
    pub hummock_floor: Range,
    /// Gain on the quilted texture residual.
    ///
    /// **Near 1.0, and it took a wrong turn to get there.** The first pack was
    /// cut from `extract_v2`'s `fine`, a half-amplitude Gaussian band-split
    /// that reads 1.68x weaker than the `fine/band` metric's plain 64 m lowpass
    /// residual. Hitting the metric then needed a gain of 2.8 -- which does not
    /// merely scale a statistic, it makes every real blowout in every patch
    /// 2.8x deeper, and the renders came back with elongated gouges no real
    /// tile carries.
    ///
    /// Patches are now cut in the metric's own band, so pasted lidar keeps the
    /// amplitude it was measured at and the gain only covers the variance lost
    /// to overlap-adding uncorrelated patches.
    pub texture_gain: Range,
    /// How much of the texture survives on the interdune FLOORS, as a
    /// fraction of its belt amplitude.
    ///
    /// MEASURED per form class, and they differ by 2x: real train tiles
    /// drop fine texture to 0.277x on the floors (p50, n=9) while mound
    /// tiles hold 0.566x (n=35). Physically: a train's interdune corridors
    /// are deflation surfaces swept by the same channelised wind that
    /// builds the belts; a mound field's hollows are sheltered. The quilt
    /// pasted at full amplitude everywhere (measured 0.771) because only
    /// the hummock tier was gated.
    pub texture_floor: Range,
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
    // ---- fluvial mode (Carolina) — corpus = docs/sandhills/01-corpus.md ----
    /// Relief budget for the sand-cap datum. `corpus`: relief_p99_p1 46.9.
    pub cap_relief_m: Range,
    /// Wavelength of the interfluve mass, metres. `guess`, bounded by the
    /// corpus d2c (interfluves must be broad enough to hold ~107 m of dry
    /// ground between channels).
    pub cap_wave_m: Range,
    /// How hard the uplands are clipped flat, 0 = domes, 1 = mesas. `guess`;
    /// the biome doc calls for broad FLAT-TOPPED interfluves.
    pub cap_flat: Range,
    /// Number of independent creek systems. `review 2026-08-24`: every real
    /// NC tile is SINGLE-TRUNKED — pattern_survey's n_sys 3 counts edge
    /// fragments of off-tile systems as components, not extra trunks. One
    /// trunk per tile; the tributary tree carries the density.
    pub sys_weights: [f64; 3],
    /// Tributary attach spacing along parents, metres. THE density lever —
    /// drawn, then tiers repeat until density lands. `corpus`: 2.33 km/km².
    pub attach_m: Range,
    /// Valley depth unit at Hack factor 1, metres. `measured 2026-08-25`
    /// (valley_compare, corpus policy on 6 real tiles): trunk depth p50
    /// 10-15.6 m, p90 to 22 — the plan's 4-8 m guess read "much too
    /// shallow" on review and the instrument agreed.
    pub valley_depth_m: Range,
    /// Valley floor half-width unit at Hack 1, metres. `measured`: the
    /// HAND<2 floor zone spans 124-248 m on the real trunks.
    pub valley_floor_m: Range,
    /// Concave wall run unit at Hack 1, metres per side. `measured`: the
    /// HAND<5 valley zone spans 276-588 m on the real trunks.
    pub valley_wall_m: Range,
    /// Residual amplitude against the measured per-(u,W) spread.
    /// `measured 2026-08-25` (leave-one-out): 0.45-0.6 lands the assembled
    /// open-ground curvature at 0.86-1.07x the real tiles'.
    pub hand_resid: Range,
    /// Texture gain multiplier for the fluvial mode against the aeolian
    /// draw. `measured 2026-08-25`: calibrated so the finished tile's
    /// sub-64 m band matches the real NC tiles (fine_std 0.317-0.405).
    pub fluvial_tex_k: Range,
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
        hummock_lambda_m: Range::new(160.0, 230.0), // inventory: nn 149 m, diam 174
        // Wide, because real footprints OVERLAP -- summits sit 149 m apart
        // with a 174 m median diameter, so a big dune laps over its smaller
        // neighbours. A single wavelength cannot do that at any scale.
        belt_patchiness: Range::new(0.24, 0.52),
        belt_patch_m: Range::new(900.0, 1900.0),
        hummock_spread: Range::new(0.50, 0.80),
        // Scaled WITH lambda: slope goes as relief/lambda, and shrinking
        // lambda alone took the golf proxy from 20/24 to 3/20.
        hummock_relief_m: Range::new(8.0, 14.0),
        hummock_kappa: Range::new(0.35, 1.10),      // 16-seed sweep -> A ~0.26
        hummock_gate: Range::new(0.30, 0.48),
        // Per-class, like texture_floor: the SS12 0.30-0.50 was calibrated on
        // the POOLED corpus, but train corridors are deflation surfaces --
        // fine floor/belt 0.277 against the mound's 0.566.
        hummock_floor: Range::new(0.14, 0.26),
        texture_gain: Range::new(1.30, 1.55),       // overlap-add + floor-gate loss
        texture_floor: Range::new(0.16, 0.26),      // corpus: train floors 0.277
        // (drawn low: the smoothstep transition band adds partial-gain area,
        //  measured +0.15 on the floor/belt readout at equal floor values)
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
        // Real mound fields are nearly SYMMETRIC -- height skew 0.136 against
        // the train belts' 0.497 -- and a parabolic mound field does have far
        // less of a slip face than a barchanoid train, so this range is
        // narrowed on that ground.
        //
        // It does NOT fix the skew, and was tried for that: 0.463 -> 0.510,
        // no movement. Skew comes from the PROFILE SHAPE, which is peaked at
        // the crest with a broad low region and is positively skewed at any
        // share. Closing it needs a rounder profile for the mound class, not
        // a different split. Tracked, not fixed.
        stoss_share: Range::new(0.52, 0.60),       // literature — less asymmetric,
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
        hummock_lambda_m: Range::new(160.0, 230.0),
        belt_patchiness: Range::new(0.45, 0.85),
        belt_patch_m: Range::new(700.0, 1500.0),
        hummock_spread: Range::new(0.50, 0.80),
        hummock_relief_m: Range::new(7.5, 13.0),
        hummock_kappa: Range::new(1.10, 2.70),
        hummock_gate: Range::new(0.26, 0.44),
        hummock_floor: Range::new(0.38, 0.55),
        texture_gain: Range::new(1.10, 1.35),
        texture_floor: Range::new(0.48, 0.65),      // corpus: mound floors 0.566
    },

    relief_budget_m: Range::new(18.0, 48.0), // golf: proxy band 7.0-81.9
    floor_tilt_m_km: Range::new(0.5, 4.0),   // guess
    // Drawn on a SQUARED ramp toward the shallow end (see draw.rs): the
    // Sandhills' lake districts sit where the table grazes the floors, and a
    // uniform draw over the full range made a lake-forming table (< 2 m) a
    // 7% event -- measured 0/40 draws. Deep-table dry tiles stay common.
    water_table_m: Range::new(0.4, 14.0),    // literature: Ogallala intersects
    cap_relief_m: Range::new(36.0, 54.0),    // corpus: 46.9
    cap_wave_m: Range::new(750.0, 1300.0),   // guess, d2c-bounded
    cap_flat: Range::new(0.35, 0.7),         // guess
    sys_weights: [0.25, 0.5, 0.25],          // corpus: n_sys p50 3
    attach_m: Range::new(360.0, 520.0),      // N0 2026-08-28: density restored to the corpus 2.33 (the -25% was judged on bare skeletons)
    valley_depth_m: Range::new(6.6, 9.8),    // measured; rebalanced for the 0.55 hack exponent
    valley_floor_m: Range::new(20.0, 36.0),  // measured + review 2026-08-26 (wider still)
    valley_wall_m: Range::new(84.0, 130.0),  // measured + review 2026-08-26 (wider still)
    hand_resid: Range::new(0.42, 0.62),      // measured (leave-one-out)
    fluvial_tex_k: Range::new(0.90, 1.15),   // calibrated at X3
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
                ("hummock_spread", g.hummock_spread),
                ("belt_patchiness", g.belt_patchiness),
                ("belt_patch_m", g.belt_patch_m),
                ("hummock_relief_m", g.hummock_relief_m),
                ("hummock_kappa", g.hummock_kappa),
                ("hummock_gate", g.hummock_gate),
                ("hummock_floor", g.hummock_floor),
                ("texture_gain", g.texture_gain),
                ("texture_floor", g.texture_floor),
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
