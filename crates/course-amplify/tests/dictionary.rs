//! Load-time interlock + structure sanity for the baked dictionary asset.
use course_amplify::dictionary::Dictionary;

#[test]
fn asset_loads_verifies_and_is_populated() {
    let d = Dictionary::load(std::path::Path::new("../../assets/dictionary_v2.bin"))
        .expect("asset loads and fingerprint matches");
    assert_eq!(d.patch, 32);
    assert_eq!(d.biomes.len(), 6, "all six biomes present");
    for (biome, levels) in &d.biomes {
        for name in ["mid", "fine"] {
            let l = levels.get(name).unwrap_or_else(|| panic!("{biome} missing {name}"));
            assert!(!l.buckets.is_empty(), "{biome}/{name} has buckets");
            let n: usize = l.buckets.values().map(|b| b.patches.len()).sum();
            assert!(n >= 300, "{biome}/{name} has {n} patches (expected hundreds+)");
            for b in l.buckets.values() {
                assert!(b.amp_p50 > 0.0);
                assert_eq!(b.equalizer.len(), 16);
                for p in &b.patches {
                    assert_eq!(p.heights.len(), 32 * 32);
                    assert!(p.heights.iter().all(|v| v.is_finite()));
                }
            }
        }
        // every held-out tile is genuinely absent from the harvest
        let holds = &d.holdout_tiles[biome];
        assert!(holds.len() >= 3);
        for l in levels.values() {
            for b in l.buckets.values() {
                for p in &b.patches {
                    assert!(!holds.contains(&p.src_tile),
                        "{biome}: held-out tile {} leaked into the harvest", p.src_tile);
                }
            }
        }
    }
    // bucket lookup round-trip: every stored patch's cond maps to a real bucket
    for levels in d.biomes.values() {
        for l in levels.values() {
            for (bid, b) in &l.buckets {
                for p in b.patches.iter().filter(|p| !p.borrowed) {
                    let computed = Dictionary::bucket_of(l, &p.cond);
                    assert_eq!(computed, *bid, "cond -> bucket round-trip");
                }
            }
        }
    }
}
