//! S3 acceptance: determinism, taper, biome-blindness.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn run(seed: u64, biome: BiomeId) -> course_amplify::Amplified {
    let dict = course_amplify::dictionary::Dictionary::load(std::path::Path::new(
        "../../assets/dictionary_v2.bin",
    ))
    .expect("dictionary");
    let id = RunIdentity::from_seed(seed);
    let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
    let c1 = course_primitives::generate(&spec, &id);
    let sk = course_skeleton::generate(&spec, &c1, &id);
    course_amplify::generate(&spec, &sk, &dict, &id)
}

#[test]
fn determinism_double_run() {
    let a = run(7, BiomeId::Piedmont);
    let b = run(7, BiomeId::Piedmont);
    assert_eq!(a.height.data, b.height.data, "same seed must be byte-identical");
}

#[test]
fn taper_holds_channel_profile() {
    // Channel cells must move less than tolerance between base and
    // amplified: the taper floor bounds it structurally, this pins it.
    let dict = course_amplify::dictionary::Dictionary::load(std::path::Path::new(
        "../../assets/dictionary_v2.bin",
    ))
    .unwrap();
    for biome in [BiomeId::Piedmont, BiomeId::RiverValley] {
        let id = RunIdentity::from_seed(11);
        let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let amp = course_amplify::generate(&spec, &sk, &dict, &id);
        // Reference = the >=64 m lowpass of the base: after the P2 dry run
        // the raw base's sub-64 m staircase was excluded from the output
        // everywhere (it was the reviewer's "ribbed cuts" tell), so the
        // profile the taper defends is the smoothed one. CHANNEL_TOL_M
        // bounds the clamp; the margin above it covers bilinear sampling
        // across the 2 m cells at a channel edge.
        let n2 = sk.height.spec.nx as usize;
        let lp64 = {
            let sigma = course_amplify::conditioning::SIGMA_PER_L * 64.0 / 2.0;
            let box_w = ((sigma * 1.153) as usize * 2 + 1).max(3);
            let mut lp = sk.height.data.clone();
            for _ in 0..3 {
                lp = course_amplify::synth::box_filter(&lp, n2, box_w);
            }
            lp
        };
        let n8 = sk.flow_distance.spec.nx as usize;
        let mut worst = 0.0f64;
        for y8 in 0..n8 {
            for x8 in 0..n8 {
                if sk.flow_distance.data[y8 * n8 + x8] > 0.0 {
                    continue; // only channel cells
                }
                let p = sk.flow_distance.spec.world_of(x8 as u32, y8 as u32);
                let x2 = ((p.x / 2.0) as usize).min(n2 - 1);
                let y2 = ((p.y / 2.0) as usize).min(n2 - 1);
                let dz = (amp.height.data[y2 * n2 + x2] - lp64[y2 * n2 + x2]).abs();
                worst = worst.max(dz);
            }
        }
        assert!(
            worst < course_amplify::CHANNEL_TOL_M + 0.15,
            "{biome:?}: channel cell moved {worst:.2} m from the smoothed base"
        );
    }
}

#[test]
fn no_biome_branch_in_crate() {
    // The biome may only select the dictionary shelf. Any other biome
    // reference in the crate is a branch waiting to happen.
    let src = concat!(
        include_str!("../src/lib.rs"),
        include_str!("../src/synth.rs"),
        include_str!("../src/blend.rs"),
        include_str!("../src/polish.rs"),
        include_str!("../src/conditioning.rs"),
    );
    for name in ["piedmont", "sandhills", "heathland", "river_valley", "great_plains", "hill_country"] {
        assert!(
            !src.to_lowercase().contains(name),
            "biome name `{name}` appears in course-amplify source"
        );
    }
}

#[test]
fn deranged_and_integrated_get_different_texture_stats() {
    // Conditioning demonstrably works: two biomes with the same code path
    // must land different band statistics (their dictionaries differ).
    let a = run(21, BiomeId::HillCountry);
    let b = run(21, BiomeId::RiverValley);
    assert!(
        (a.fine_std_m - b.fine_std_m).abs() > 0.1,
        "hc {} vs rv {} fine_std should differ",
        a.fine_std_m,
        b.fine_std_m
    );
}

#[test]
fn permuted_patch_order_is_bit_identical() {
    // The stage doc's strongest determinism requirement, tested directly:
    // fill the patch-choice table in shuffled order and the surface must
    // be BIT-identical, because choice is a pure per-position function and
    // composition gathers per cell in canonical order.
    use course_amplify::conditioning::Conditioning;
    use course_amplify::synth;
    let dict = course_amplify::dictionary::Dictionary::load(std::path::Path::new(
        "../../assets/dictionary_v2.bin",
    ))
    .unwrap();
    let id = RunIdentity::from_seed(5);
    let spec = SiteSpec::generate_builtin(
        id,
        &SpecOverridesV2 { forced_biome: Some(BiomeId::Piedmont) },
    );
    let c1 = course_primitives::generate(&spec, &id);
    let sk = course_skeleton::generate(&spec, &c1, &id);
    let n8 = sk.flow_distance.spec.nx as usize;
    let base8 = {
        let mut g = course_world::grid::Grid::filled(sk.flow_distance.spec, 0.0f64);
        for y in 0..n8 {
            for x in 0..n8 {
                let p = g.spec.world_of(x as u32, y as u32);
                g.data[y * n8 + x] = sk.height.bilinear(p);
            }
        }
        g
    };
    let cond = Conditioning::compute(&base8, &sk.flow_distance.data);
    let level = &dict.biomes["piedmont"]["mid"];
    let origins: Vec<usize> = {
        // mirror synth::patch_grid
        let mut xs: Vec<usize> = (0..)
            .map(|i| i * 16)
            .take_while(|&x| x + 32 <= n8)
            .collect();
        if *xs.last().unwrap() + 32 != n8 {
            xs.push(n8 - 32);
        }
        xs
    };
    let np = origins.len();
    let mut fwd: Vec<(usize, usize)> = Vec::new();
    for a in 0..np {
        for b in 0..np {
            fwd.push((a, b));
        }
    }
    let mut rev = fwd.clone();
    rev.reverse();
    // interleave a third order: odd positions first
    let mut odd: Vec<(usize, usize)> = fwd.iter().copied().filter(|(a, b)| (a + b) % 2 == 1).collect();
    odd.extend(fwd.iter().copied().filter(|(a, b)| (a + b) % 2 == 0));
    let seed = id.stream_seed();
    let a = synth::choose_patches(&dict, level, synth::Band::Mid, &cond, 1, 32, &origins, &fwd, seed);
    let b = synth::choose_patches(&dict, level, synth::Band::Mid, &cond, 1, 32, &origins, &rev, seed);
    let c = synth::choose_patches(&dict, level, synth::Band::Mid, &cond, 1, 32, &origins, &odd, seed);
    for i in 0..a.len() {
        let key = |o: &Option<synth::Chosen>| {
            o.as_ref().map(|ch| {
                (
                    ch.patch.src_tile.clone(),
                    ch.patch.heights[0].to_bits(),
                    ch.amp_p50.to_bits(),
                    // θ is part of the choice — a rotation that depended
                    // on fill order would slip past a patch-only key
                    ch.theta.to_bits(),
                )
            })
        };
        assert_eq!(key(&a[i]), key(&b[i]), "position {i}: forward vs reversed");
        assert_eq!(key(&a[i]), key(&c[i]), "position {i}: forward vs interleaved");
    }
    // and the end-to-end surface is byte-identical run to run (the gather
    // makes summation order canonical; nothing else is order-sensitive)
    let s1 = course_amplify::generate(&spec, &sk, &dict, &id);
    let s2 = course_amplify::generate(&spec, &sk, &dict, &id);
    assert_eq!(
        s1.height.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        s2.height.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
    );
}

#[test]
fn golden_amplified_seed_1() {
    // FNV over the amplified surface, piedmont + heathland, seed 1.
    fn fnv(data: &[f64]) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for v in data {
            for b in v.to_bits().to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
        h
    }
    let mut got = String::new();
    for biome in [BiomeId::Piedmont, BiomeId::Heathland] {
        let amp = run(1, biome);
        got += &format!("{biome:?} {:016x}
", fnv(&amp.height.data));
    }
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write("tests/golden_amplified_seed_1.txt", &got).unwrap();
        return;
    }
    let want = include_str!("golden_amplified_seed_1.txt");
    assert_eq!(got, want, "S3 golden diverged — re-bless if intended");
}

#[test]
fn orientation_machinery_is_honest() {
    // (a) sign guard: synthetic ridges RUNNING ALONG X (height varies
    // only with y) must report grain axis ~0, not π/2 — the classic
    // gradient-vs-grain flip bug.
    let mut ridges = vec![0.0f32; 32 * 32];
    for y in 0..32usize {
        for x in 0..32usize {
            ridges[y * 32 + x] = libm::sinf(y as f32 * 0.8);
        }
    }
    let (axis, coh) = course_amplify::dictionary::patch_axis(&ridges, 32);
    assert!(
        axis.min(std::f64::consts::PI - axis) < 0.05,
        "ridges along x must yield grain axis ~0, got {axis}"
    );
    assert!(coh > 0.95, "perfect stripes must be near-fully coherent, got {coh}");

    // (b) isotropic noise → low coherence (axis untrustworthy)
    let mut h = 0x9E3779B97F4A7C15u64;
    let mut noise = vec![0.0f32; 32 * 32];
    for v in noise.iter_mut() {
        h = (h ^ (h >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        *v = ((h >> 40) as f32 / (1 << 24) as f32) - 0.5;
    }
    let (_, ncoh) = course_amplify::dictionary::patch_axis(&noise, 32);
    assert!(ncoh < 0.4, "white noise must read near-isotropic, got {ncoh}");
}

#[test]
fn rv_texture_follows_the_grain() {
    // Integration on real terrain: a river-valley seed must rotate a
    // meaningful share of mid-band positions, and every applied rotation
    // must move the patch's axis TOWARD the local target axis.
    //
    // No cross-biome isotropy claim here on purpose: the current S2
    // heathland base carries real low-amplitude parallel-ridge fabric
    // (reviewer-tracked, ROUGH_FLOOR candidate fix), so the tensor
    // honestly reports grain there today. The gate's response to
    // isotropy is proven on controlled data in
    // `coherence_gate_responds_to_fabric` below.
    use course_amplify::conditioning::Conditioning;
    use course_amplify::synth;
    let dict = course_amplify::dictionary::Dictionary::load(std::path::Path::new(
        "../../assets/dictionary_v2.bin",
    ))
    .unwrap();
    let id = RunIdentity::from_seed(3);
    let spec = SiteSpec::generate_builtin(
        id,
        &SpecOverridesV2 { forced_biome: Some(BiomeId::RiverValley) },
    );
    let c1 = course_primitives::generate(&spec, &id);
    let sk = course_skeleton::generate(&spec, &c1, &id);
    let n8 = sk.flow_distance.spec.nx as usize;
    let base8 = {
        let mut g = course_world::grid::Grid::filled(sk.flow_distance.spec, 0.0f64);
        for y in 0..n8 {
            for x in 0..n8 {
                let p = g.spec.world_of(x as u32, y as u32);
                g.data[y * n8 + x] = sk.height.bilinear(p);
            }
        }
        g
    };
    let cond = Conditioning::compute(&base8, &sk.flow_distance.data);
    let level = &dict.biomes[spec.biome.key()]["mid"];
    let (chosen, origins) = choose_all(&dict, level, &cond, n8, id.stream_seed());
    let np = origins.len();
    let n = chosen.iter().flatten().count();
    let rot = chosen.iter().flatten().filter(|c| c.theta != 0.0).count();
    // Floor sits under the measured honest level (~0.27) as a
    // regression tripwire. It was 0.3 while the border rim walls were
    // still in the PRESENTED height: the ~200 m construction walls
    // manufactured artificial gradient grain along every border that
    // patches dutifully rotated to. Stripping the walls (Carved::
    // pre_rim) removed that fake alignment demand.
    assert!(
        rot as f64 / n.max(1) as f64 > 0.22,
        "river valley must rotate a meaningful share, got {}/{n}",
        rot
    );
    let (mut better, mut checked) = (0usize, 0usize);
    for (i, c) in chosen.iter().enumerate() {
        let Some(ch) = c else { continue };
        if ch.theta == 0.0 {
            continue;
        }
        let (pyi, pxi) = (i / np, i % np);
        let (taxis, _) = cond.patch_axis(origins[pyi], origins[pxi], 32);
        let dist = |a: f64| {
            let mut d = (taxis - a).rem_euclid(std::f64::consts::PI);
            if d > std::f64::consts::FRAC_PI_2 {
                d -= std::f64::consts::PI;
            }
            d.abs()
        };
        checked += 1;
        if dist(ch.patch.axis_rad + ch.theta) <= dist(ch.patch.axis_rad) + 1e-9 {
            better += 1;
        }
    }
    assert!(
        better as f64 / checked.max(1) as f64 > 0.95,
        "rotation must move patch axes TOWARD the target ({better}/{checked})"
    );
}

#[test]
fn coherence_gate_responds_to_fabric() {
    // The gate proven on data we control: a grooved surface (real
    // kilometre-scale fabric) must rotate nearly everywhere; an
    // isotropic bump field must stay nearly untouched (θ = 0.0 exactly,
    // the legacy path). This is the "must not force anisotropy where
    // there is none" acceptance, tested without any biome assumption.
    use course_amplify::conditioning::{gaussian_blur, Conditioning};
    use course_amplify::synth;
    let dict = course_amplify::dictionary::Dictionary::load(std::path::Path::new(
        "../../assets/dictionary_v2.bin",
    ))
    .unwrap();
    let id = RunIdentity::from_seed(9);
    let spec = SiteSpec::generate_builtin(
        id,
        &SpecOverridesV2 { forced_biome: Some(BiomeId::Piedmont) },
    );
    let c1 = course_primitives::generate(&spec, &id);
    let sk = course_skeleton::generate(&spec, &c1, &id);
    let n8 = sk.flow_distance.spec.nx as usize;
    let level = &dict.biomes["piedmont"]["mid"];
    let dist_far = vec![500.0f64; n8 * n8];

    // grooves: ridges running along x at ~250 m wavelength
    let mut grooves = course_world::grid::Grid::filled(sk.flow_distance.spec, 0.0f64);
    for y in 0..n8 {
        for x in 0..n8 {
            grooves.data[y * n8 + x] = 3.0 * libm::sin(y as f64 * 8.0 * std::f64::consts::TAU / 250.0)
                + 0.3 * libm::cos(x as f64 * 8.0 * std::f64::consts::TAU / 700.0);
        }
    }
    let cond_g = Conditioning::compute(&grooves, &dist_far);
    let (chosen_g, _) = choose_all(&dict, level, &cond_g, n8, id.stream_seed());
    let ng = chosen_g.iter().flatten().count();
    let rg = chosen_g.iter().flatten().filter(|c| c.theta != 0.0).count();

    // isotropic bumps: hash noise blurred to ~150 m blobs
    let mut bumps = course_world::grid::Grid::filled(sk.flow_distance.spec, 0.0f64);
    let mut h = 0x51D2C4A7u64;
    for v in bumps.data.iter_mut() {
        h = (h ^ (h >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        h ^= h >> 27;
        *v = ((h >> 40) as f64 / (1u64 << 24) as f64 - 0.5) * 10.0;
    }
    let bumps = gaussian_blur(&bumps, 150.0 / 8.0 * 0.4);
    let cond_b = Conditioning::compute(&bumps, &dist_far);
    let (chosen_b, _) = choose_all(&dict, level, &cond_b, n8, id.stream_seed());
    let nb = chosen_b.iter().flatten().count();
    let rb = chosen_b.iter().flatten().filter(|c| c.theta != 0.0).count();

    let fg = rg as f64 / ng.max(1) as f64;
    let fb = rb as f64 / nb.max(1) as f64;
    assert!(fg > 0.8, "grooved fabric must rotate nearly everywhere, got {fg:.2}");
    assert!(fb < 0.15, "isotropic bumps must stay nearly untouched, got {fb:.2}");
}

fn choose_all<'d>(
    dict: &'d course_amplify::dictionary::Dictionary,
    level: &'d course_amplify::dictionary::Level,
    cond: &course_amplify::conditioning::Conditioning,
    n8: usize,
    seed: u64,
) -> (Vec<Option<course_amplify::synth::Chosen<'d>>>, Vec<usize>) {
    use course_amplify::synth;
    let origins: Vec<usize> = {
        let mut xs: Vec<usize> = (0..).map(|i| i * 16).take_while(|&x| x + 32 <= n8).collect();
        if *xs.last().unwrap() + 32 != n8 {
            xs.push(n8 - 32);
        }
        xs
    };
    let np = origins.len();
    let mut pos = Vec::new();
    for a in 0..np {
        for b in 0..np {
            pos.push((a, b));
        }
    }
    let chosen = synth::choose_patches(
        dict,
        level,
        synth::Band::Mid,
        cond,
        1,
        32,
        &origins,
        &pos,
        seed,
    );
    (chosen, origins)
}
