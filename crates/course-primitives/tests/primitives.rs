//! Stage S1 acceptance suite (`docs/stages/stage-01-macro-primitives.md`).

use course_contracts::biome::{BiomeId, BoundaryKind, StructureClass, WindowClass};
use course_contracts::metadata::Edge;
use course_primitives::generate;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_world::math::Vec2;
use course_world::world::EXTENT_M;

fn spec_for(seed: u64) -> SiteSpec {
    SiteSpec::generate_builtin(RunIdentity::from_seed(seed), &SpecOverridesV2::default())
}

/// A spec with a chosen class/provinces, everything else from the seed draw.
fn spec_with_class(seed: u64, window: WindowClass, provinces: u8) -> SiteSpec {
    let mut s = spec_for(seed);
    s.structure_class = StructureClass::new(
        window,
        provinces,
        (provinces == 2).then_some(BoundaryKind::Scarp),
    )
    .unwrap();
    if provinces == 2 {
        s.dials.insert("primitives.province_relief_m".into(), 6.0);
    }
    s
}

#[test]
fn determinism_double_run() {
    for seed in [1, 7, 42] {
        let s = spec_for(seed);
        let id = RunIdentity::from_seed(seed);
        let a = generate(&s, &id);
        let b = generate(&s, &id);
        assert_eq!(a, b);
    }
}

#[test]
fn tilt_is_monotone_toward_the_edge() {
    for seed in [1, 2, 3, 4, 5] {
        let s = spec_for(seed);
        let c1 = generate(&s, &RunIdentity::from_seed(seed));
        let g = c1.grid;
        // The along-coordinate direction for the drawn edge:
        let dir = match c1.meta.base_level.edge {
            Edge::N => (0i64, 1i64),
            Edge::S => (0, -1),
            Edge::E => (1, 0),
            Edge::W => (-1, 0),
            _ => unreachable!("v1 recipe draws cardinal edges only"),
        };
        // Moving one node toward the edge must never increase tilt.
        for y in 1..g.ny - 1 {
            for x in 1..g.nx - 1 {
                let here = *c1.tilt.get(x, y);
                let there = *c1.tilt.get(
                    (x as i64 + dir.0) as u32,
                    (y as i64 + dir.1) as u32,
                );
                assert!(there <= here + 1e-9, "tilt rises toward the edge at ({x},{y})");
            }
        }
    }
}

#[test]
fn relief_is_band_limited() {
    // Sub-band content check. NOTE: the blur window must sit well BELOW the
    // band edge — a 400 m box blur has a NULL at 400 m wavelength, so legal
    // band-edge content would read as leakage. A ~136 m window attenuates
    // >=400 m modes by >5x while passing genuine sub-band violations.
    let s = spec_for(1);
    let c1 = generate(&s, &RunIdentity::from_seed(1));
    let g = c1.grid;
    let r = 8i64; // half-window: 17 cells * 8 m = 136 m
    let mut hp_energy = 0.0;
    let mut energy = 0.0;
    let mut count = 0usize;
    // Sample on a stride for speed; interior only so the window fits.
    let mut y = r as u32;
    while y < g.ny - r as u32 {
        let mut x = r as u32;
        while x < g.nx - r as u32 {
            let mut sum = 0.0;
            let mut n = 0.0;
            let mut yy = y as i64 - r;
            while yy <= y as i64 + r {
                let mut xx = x as i64 - r;
                while xx <= x as i64 + r {
                    sum += *c1.relief.get(xx as u32, yy as u32);
                    n += 1.0;
                    xx += 2;
                }
                yy += 2;
            }
            let low = sum / n;
            let v = *c1.relief.get(x, y);
            hp_energy += (v - low) * (v - low);
            energy += v * v;
            count += 1;
            x += 7;
        }
        y += 7;
    }
    assert!(count > 100);
    let ratio = (hp_energy / energy.max(1e-12)).sqrt();
    assert!(ratio < 0.30, "high-pass energy ratio {ratio} — sub-400 m content leaked");
}

#[test]
fn classes_are_structurally_distinct() {
    // The class-legibility precondition, quantified AT THE SAME SEED: with
    // frame and noise phases identical, any field difference is pure class
    // shape. Every class pair must differ materially in the accommodation
    // field (class-shaped, phase-free) or the relief field.
    let classes = WindowClass::ALL;
    let field = |class| {
        let s = spec_with_class(11, class, 1);
        let c1 = generate(&s, &RunIdentity::from_seed(11));
        (c1.accommodation, c1.relief)
    };
    let rms = |a: &course_world::Grid<f64>, b: &course_world::Grid<f64>| {
        let mut e = 0.0;
        for i in 0..a.data.len() {
            e += (a.data[i] - b.data[i]).powi(2);
        }
        (e / a.data.len() as f64).sqrt()
    };
    let fields: Vec<_> = classes.iter().map(|&c| field(c)).collect();
    for i in 0..classes.len() {
        for j in i + 1..classes.len() {
            let d_acc = rms(&fields[i].0, &fields[j].0);
            let d_rel = rms(&fields[i].1, &fields[j].1);
            assert!(
                d_acc > 0.05 || d_rel > 1.0,
                "{:?} vs {:?}: accommodation RMS {d_acc:.4}, relief RMS {d_rel:.3} —                  classes are not structurally distinct",
                classes[i],
                classes[j]
            );
        }
    }
}

#[test]
fn discontinuity_steps_the_fields_and_spans_the_box() {
    let s = spec_with_class(3, WindowClass::Interfluve, 2);
    let c1 = generate(&s, &RunIdentity::from_seed(3));
    assert_eq!(c1.meta.discontinuities.len(), 1);
    let d = &c1.meta.discontinuities[0];
    // Enters AND exits: both endpoints on the box boundary.
    for p in [d.curve[0], *d.curve.last().unwrap()] {
        let on = p.x.abs() < 1e-6
            || (p.x - EXTENT_M).abs() < 1e-6
            || p.y.abs() < 1e-6
            || (p.y - EXTENT_M).abs() < 1e-6;
        assert!(on, "curve endpoint {p:?} not on the box boundary");
    }
    // Hardness steps across the boundary: compare means well off each side.
    let a = d.curve[0];
    let b = *d.curve.last().unwrap();
    let dir = Vec2::new(b.x - a.x, b.y - a.y);
    let len = (dir.x * dir.x + dir.y * dir.y).sqrt();
    let n = Vec2::new(-dir.y / len, dir.x / len);
    let mid = Vec2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let sample = |off: f64| {
        let p = Vec2::new(mid.x + n.x * off, mid.y + n.y * off);
        let g = c1.grid;
        let x = ((p.x - g.origin.x) / g.cell_size).round().clamp(0.0, (g.nx - 1) as f64) as u32;
        let y = ((p.y - g.origin.y) / g.cell_size).round().clamp(0.0, (g.ny - 1) as f64) as u32;
        *c1.hardness.get(x, y)
    };
    let step = (sample(600.0) - sample(-600.0)).abs();
    assert!(step > 0.15, "hardness step across the boundary is only {step:.3}");
}

#[test]
fn single_province_has_no_discontinuities() {
    let s = spec_with_class(4, WindowClass::Interfluve, 1);
    let c1 = generate(&s, &RunIdentity::from_seed(4));
    assert!(c1.meta.discontinuities.is_empty());
}

#[test]
fn wind_is_bit_copied_from_the_spec() {
    let s = spec_for(6);
    let c1 = generate(&s, &RunIdentity::from_seed(6));
    assert_eq!(
        c1.meta.wind_azimuth_rad.to_bits(),
        s.descriptors.wind_azimuth_rad.to_bits(),
        "one wind system: sculpting azimuth must be the spec's, bit-identical"
    );
}

#[test]
fn empty_strata_biomes_stay_empty() {
    let mut s = spec_for(8);
    s.biome = BiomeId::Sandhills;
    s.descriptors.strata.clear();
    let c1 = generate(&s, &RunIdentity::from_seed(8));
    assert!(c1.meta.strata.is_empty());
}

#[test]
fn golden_c1_hashes() {
    // Golden per biome: FNV over the field bits + the discontinuity count.
    // Re-bless by running with UPDATE_GOLDEN=1 (prints the new table).
    use course_world::world::fnv_f64;
    let mut got = String::new();
    for (biome, seed) in [
        (BiomeId::Piedmont, 1u64),
        (BiomeId::Heathland, 1),
        (BiomeId::Sandhills, 1),
    ] {
        let s = SiteSpec::generate_builtin(
            RunIdentity::from_seed(seed),
            &SpecOverridesV2 {
                forced_biome: Some(biome),
            },
        );
        let c1 = generate(&s, &RunIdentity::from_seed(seed));
        got.push_str(&format!(
            "{} {:016x} {:016x} {:016x} {:016x} {}\n",
            biome.key(),
            fnv_f64(&c1.tilt.data),
            fnv_f64(&c1.relief.data),
            fnv_f64(&c1.hardness.data),
            fnv_f64(&c1.accommodation.data),
            c1.meta.discontinuities.len(),
        ));
    }
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        println!("{got}");
        return;
    }
    let want = include_str!("golden_c1_hashes.txt");
    assert_eq!(got, want, "re-bless: UPDATE_GOLDEN=1 cargo test -p course-primitives golden -- --nocapture");
}
