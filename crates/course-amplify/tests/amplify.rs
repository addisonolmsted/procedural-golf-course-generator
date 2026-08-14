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
        let n8 = sk.flow_distance.spec.nx as usize;
        let mut worst = 0.0f64;
        for y8 in 0..n8 {
            for x8 in 0..n8 {
                if sk.flow_distance.data[y8 * n8 + x8] > 0.0 {
                    continue; // only channel cells
                }
                let p = sk.flow_distance.spec.world_of(x8 as u32, y8 as u32);
                let dz = (amp.height.bilinear(p) - sk.height.bilinear(p)).abs();
                worst = worst.max(dz);
            }
        }
        assert!(
            worst < 1.2,
            "{biome:?}: channel cell moved {worst:.2} m under amplification"
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
        let key = |o: &Option<(&course_amplify::dictionary::Patch, f64)>| {
            o.as_ref().map(|(p, amp)| (p.src_tile.clone(), p.heights[0].to_bits(), amp.to_bits()))
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
