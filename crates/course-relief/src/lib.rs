//! Steps 5-6 — the macro surface and the tier-cut loop (terrain-first
//! restructure, 2026-08-21).
//!
//! Attempt 4. See `docs/network-first/README.md` and 03-macro-is-designed §8:
//! **the macro is designed for gameplay; the corpus informs it.**
//!
//! **Owns:** the heightmap. Built FROM the trunks and fields (envelope
//! composition — no second authority, no double cuts), then each tributary
//! tier is grown ON the surface and its catena cut INTO it.
//!
//! **Expected failures** (discipline rule 6): all texture and roughness
//! metrics — texture is step 7's job. Judged on: bed preservation, the golf
//! slope readout, archetype legibility of the macro forms.
//!
//! DEPENDENCY RULE: this crate may not depend on `course-skeleton`,
//! `course-primitives`, `course-amplify` or `course-transforms`. Enforced by
//! `tools/no_old_deps.sh`.

pub mod macro_surface;
pub mod section;
pub mod trib_cut;

pub use macro_surface::{MacroSurface, RES_M};

use course_draw::rng::stream;
use course_seed::RunIdentity;

/// Build the macro surface for a seed: template -> trunks -> envelope ->
/// tribs grown ON the surface -> tier catena cuts (T2).
pub fn build_macro(id: &RunIdentity, forced: Option<course_draw::Archetype>) -> (
    course_template::Template,
    Vec<course_network::TrunkPath>,
    Vec<course_network::Trib>,
    MacroSurface,
    course_draw::Descriptors,
) {
    let draw = course_draw::generate(id, forced);
    let t = course_template::build(id, &draw);
    let net = course_network::build(id, &t);
    let mut rng = stream(id, course_draw::rng::RELIEF_MACRO);
    // kernel selection by STRUCTURE KIND (data, not biome): the fluvial
    // engine or the aeolian one — the sanctioned seam.
    let mut ms = match draw.structure {
        course_draw::StructureKind::Aeolian => macro_surface::build_aeolian(&mut rng, &t, &draw.d),
        course_draw::StructureKind::Fluvial => macro_surface::build(&mut rng, &t, &draw.d, &net.trunks),
    };
    // T2: tributaries climb the REAL surface, then cut into it. Aeolian
    // tiles have no trunks, so grow_tribs_on is a no-op there.
    let (tribs, _, _, _) =
        course_network::grow_tribs_on(id, &t, &net.trunks, &ms.height, None);
    trib_cut::carve(&mut ms.height, &tribs, &draw.d);
    (t, net.trunks, tribs, ms, draw.d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_draw::Archetype;

    #[test]
    fn deterministic() {
        let id = RunIdentity::from_seed(7);
        let (_, _, _, a, _) = build_macro(&id, Some(Archetype::HillCountry));
        let (_, _, _, b, _) = build_macro(&id, Some(Archetype::HillCountry));
        assert_eq!(a.height.data, b.height.data);
    }

    #[test]
    fn trunk_beds_are_preserved() {
        // The surface along a trunk must sit ON the trunk's long profile:
        // the floor is flat (rise 0 inside floor_hw), the interfluve ramp is
        // zero there, and the bed smoothing is gentle. Tolerance covers the
        // 8 m grid + the C1 smoothing of the bed reference.
        for seed in [1u64, 2, 5, 9] {
            for a in [Archetype::Piedmont, Archetype::RiverValley, Archetype::HillCountry] {
                let id = RunIdentity::from_seed(seed);
                let (_, trunks, _, ms, _) = build_macro(&id, Some(a));
                // junction neighbourhoods are exempt: the ADAPTIVE softmin
                // knee deliberately merges the two floors broadly there
                // (the "stamped on top of each other" fix), which lifts a
                // bed by a few metres within ~400 m of the junction.
                let junctions: Vec<_> = trunks
                    .iter()
                    .filter(|t| t.joins.is_some())
                    .map(|t| t.pts[0])
                    .collect();
                for tk in &trunks {
                    // GEOMETRIC handoff exemption: a joining trunk defers to
                    // its primary wherever their axes are within ~620 m, so
                    // its independently-built bed is not authoritative there
                    // (hydrology re-cuts the channel later)
                    let primary = tk.joins.map(|j| &trunks[j as usize]);
                    for (i, p) in tk.pts.iter().enumerate().step_by(8) {
                        if let Some(pr) = primary {
                            let sep = pr.pts.iter().fold(f64::MAX, |m, q| m.min(q.distance(*p)));
                            if sep < 650.0 {
                                continue;
                            }
                        }
                        if junctions.iter().any(|j| j.distance(*p) < 400.0) {
                            continue;
                        }
                        let zs = ms.height.bilinear(*p);
                        let zb = tk.z[i];
                        // ABOVE-bed tolerance 4 m: where two trunks' valleys
                        // genuinely overlap, the softmin col lifts one bed a
                        // few metres — physical (floors merge), and hydrology
                        // re-cuts the channel later. BELOW bed stays strict.
                        assert!(
                            (zs - zb).abs() < 4.0,
                            "{a} seed {seed}: surface {zs:.2} vs bed {zb:.2} at ({:.0},{:.0})",
                            p.x, p.y
                        );
                        assert!(zs >= zb - 1.0, "{a} seed {seed}: surface below bed");
                    }
                }
            }
        }
    }

    #[test]
    fn centerlines_are_smooth() {
        // The user's own diagnostic, made permanent: walk every trunk
        // centerline; no surface step over 1.2 m per 20 m point. The
        // convergent-handoff bug printed 10.16 m here before the
        // wrapped-section fix.
        for seed in [2u64, 11, 23, 37, 58, 71, 90] {
            for a in [Archetype::Piedmont, Archetype::GreatPlains,
                      Archetype::RiverValley, Archetype::HillCountry] {
                let id = RunIdentity::from_seed(seed);
                let (_, trunks, _, ms, _) = build_macro(&id, Some(a));
                for (ti, tk) in trunks.iter().enumerate() {
                    let mut prev = ms.height.bilinear(tk.pts[0]);
                    for (i, p) in tk.pts.iter().enumerate().skip(1) {
                        let z = ms.height.bilinear(*p);
                        assert!(
                            (z - prev).abs() < 1.2,
                            "{a} seed {seed} trunk {ti}: {:.2} m step at pt {i}",
                            (z - prev).abs()
                        );
                        prev = z;
                    }
                }
            }
        }
    }

    #[test]
    fn trib_cuts_only_cut() {
        // T2 invariant: the tier machinery may only LOWER the surface.
        // Rebuild the pre-cut macro through the same streams and compare.
        for (seed, a) in [(2u64, Archetype::HillCountry), (37, Archetype::GreatPlains),
                          (11, Archetype::RiverValley)] {
            let id = RunIdentity::from_seed(seed);
            let draw = course_draw::generate(&id, Some(a));
            let t = course_template::build(&id, &draw);
            let net = course_network::build(&id, &t);
            let mut rng = stream(&id, course_draw::rng::RELIEF_MACRO);
            let base = macro_surface::build(&mut rng, &t, &draw.d, &net.trunks);
            let (_, _, tribs, ms, _) = build_macro(&id, Some(a));
            assert!(!tribs.is_empty(), "{a} seed {seed}: no tribs grown");
            let mut cut_cells = 0u32;
            for (i, v) in ms.height.data.iter().enumerate() {
                assert!(
                    *v <= base.height.data[i] + 1e-9,
                    "{a} seed {seed}: cut RAISED cell {i}"
                );
                if *v < base.height.data[i] - 0.05 {
                    cut_cells += 1;
                }
            }
            assert!(cut_cells > 500, "{a} seed {seed}: only {cut_cells} cells cut");
        }
    }

    #[test]
    fn zero_trunk_tiles_still_roll() {
        // heathland draws 0 trunks ~55% of the time; the macro must fall
        // back to the relief field, not a plane (measured: 0.0 m before).
        let mut any = false;
        for seed in 0..20u64 {
            let id = RunIdentity::from_seed(seed);
            let (_, trunks, _, ms, _) = build_macro(&id, Some(Archetype::Heathland));
            if trunks.is_empty() {
                any = true;
                let (mut lo, mut hi) = (f64::MAX, f64::MIN);
                for v in &ms.height.data {
                    lo = lo.min(*v);
                    hi = hi.max(*v);
                }
                assert!(hi - lo > 1.0, "seed {seed}: flat zero-trunk tile");
            }
        }
        assert!(any, "no zero-trunk heathland seed in 0..20");
    }
}
