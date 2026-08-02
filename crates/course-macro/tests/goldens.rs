//! Per-archetype golden pins: the base-height hash at 32 m for one fixed
//! seed each. Any change to the planner's draw order, placement math, the
//! primitive engine, or the prior file shifts these — re-bless ONLY as a
//! deliberate MACRO_VERSION / prior event, together with the step-03 doc.

use std::collections::BTreeMap;

use course_seed::RunIdentity;
use course_spec::{ArchetypeId, CourseSpec, SpecOverrides};
use course_world::world::fnv_f64;
use course_macro::generate_skeleton;

fn hash_for(arch: ArchetypeId) -> u64 {
    let overrides = SpecOverrides {
        forced_archetype: Some(arch),
        params: BTreeMap::new(),
    };
    let spec = CourseSpec::generate_builtin(RunIdentity::from_seed(1), &overrides).unwrap();
    let res = generate_skeleton(&spec, 32.0);
    fnv_f64(&res.skeleton.base_height.data)
}

#[test]
fn golden_base_height_seed_1() {
    let got: Vec<(ArchetypeId, u64)> =
        ArchetypeId::ALL.into_iter().map(|a| (a, hash_for(a))).collect();
    let want = GOLDEN;
    for (i, (a, h)) in got.iter().enumerate() {
        assert_eq!(
            *h, want[i].1,
            "{a:?}: golden drifted (got {h}) — re-bless deliberately: {got:?}"
        );
    }
}

// Blessed 2026-07-26 (initial planner, placeholder-2 prior).
// Re-blessed 2026-07-27 (MACRO_VERSION 2): campaign-pilot-1 prior + the
// effective-tilt re-anchoring in lower() — bowl rims, ridge crests, trunk
// floor/fall now track the budget solve's tilt scaling.
// Re-blessed 2026-07-28 (MACRO_VERSION 3): campaign-pilot-2 prior (repaired
// valley-fall + major-ridge estimators, new ridge_crest_hw_m knob), the ridge
// de-mesa shaping that reads it, and the bowl early-out (ULP-level shift on
// bowl archetypes only — piedmont/mountain, which place no bowls, are
// unchanged from the de-mesa bless, which is the expected signature).
// Re-blessed 2026-07-30 (campaign-pilot-3): the exemplar corpus was rebuilt
// onto protected land after the OSM screen showed the old tiles were 5-64%
// developed, so every fitted table moved. No planner change.
// Re-blessed 2026-07-31 (campaign-pilot-4): detector sensitivity — ridge
// counts roughly tripled (ridge transects were using a valley-sized 300 m
// half-length on interfluves 200-600 m apart) and bench_count now counts
// contour-parallel cascade LEVELS instead of scarp segments. Florida is
// unchanged because it places neither, which is the expected signature.
// Re-blessed 2026-08-01 (M1 perf): exact early-outs on Valley/Ridge, the
// mirror of the Bowl cull. The bounds are provably sound (pinned by
// `prims::valley::bound_tests`), so the only difference is arithmetic —
// `smin`/`smax` on their saturated branch evaluate `s + (z - s)`, which is
// algebraically z but drifts by an ULP; returning z verbatim is the more
// accurate of the two. Magnitude pinned by `valley_early_out_is_ulp_scale`.
// Florida is unchanged (it places neither valleys nor ridges), the expected
// signature. No semantic change: 2.7-2.9x faster raster.
// Re-blessed 2026-08-01 (MACRO_VERSION 4, M3): the core-localized budget
// solve. ONLY MountainBench moves, and that is the whole diagnostic: at seed
// 1 the other four archetypes' raw cores already fit their caps, so the solve
// returns with `core_tread`/`core_relax`/`incision_scale` at exactly 1.0 and
// lowers a bit-identical config (pinned by
// `budget::an_unbound_plan_keeps_both_knobs_at_one`). Mountain seed 1 is the
// one that binds — 45.0 m cap against a 60.6 m raw core — and it is exactly
// the seed the old solve mangled worst, landing at 17.2 m.
// Re-blessed 2026-08-02 (campaign-m4 prior, M4): all five move, which is the
// expected signature — every archetype's `core_relief_cap_m` changed, and the
// cap is an input to trunk incision, the tangent-routing decision and the
// budget solve alike. The caps now come from 12 REAL golf courses per region
// instead of an authored guess; they rose 1.2x (piedmont) to 2.2x
// (sandhills). The prior also gained ten network knobs, but those are inert
// until M5 reads them and cannot be what moved these hashes.
// Re-blessed 2026-08-02 (campaign-m5 prior + the weighted-rise core lever):
// two deliberate changes at once. The prior gains
// `landform.drainage_density_fine` (the M5 grower's size target), which
// reshuffles every sampled knob; and `ResolvedValley::apply` now weights the
// wall RISE rather than the composed surface when the core relax is engaged.
// The second is a correctness change, not a tuning one: with a grown network
// the drain floors are unavoidably INSIDE the core, and blending the composed
// surface toward the terrain lifts a floor relative to its neighbours, which
// is a ponded drain. Scaling the rise has zero effect at rise == 0, so floors
// are untouched by construction — pinned by
// `core_weight_tests::weighted_rise_never_moves_the_floor`.
// Re-blessed 2026-08-02 (BISECT_ITERS 16 -> 8): ONLY MountainBench moves,
// which is the diagnostic — seed 1 is the single archetype whose cap binds,
// so it is the only one where the bisection runs at all. 8 halvings resolve a
// knob to ~4e-3, far below what the 16 m preview grid can distinguish; the
// halving is for M5, where ~125 valleys make each preview evaluation the
// dominant cost of `plan()`.
const GOLDEN: [(ArchetypeId, u64); 5] = [
    (ArchetypeId::Sandhills, 8370229133792043578),
    (ArchetypeId::Piedmont, 3289670748771664032),
    (ArchetypeId::FloridaLowland, 4664041539211031053),
    (ArchetypeId::GlacialMoraine, 1763254431363810948),
    (ArchetypeId::MountainBench, 12320263730992946308),
];
