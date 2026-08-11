//! Generates the S1 class-legibility session material (workplan C5, the S1
//! exit test in docs/03-success-indicators.md):
//!
//!   out/c1_legibility/labeled/<class>_<k>.png   — study sheet, labeled
//!   out/c1_legibility/blind/c1_<a..r>.png       — 18 shuffled UNLABELED
//!                                                 implied-terrain renders
//!                                                 (3 per class, no overlays)
//!   out/c1_legibility/blind/answer_key.json     — do not open until after
//!
//! Session: study the labeled sheets, then name each blind image's class.
//! Gate: >= 80% correct. Run:
//!   cargo run -p stage-lab --release --example c1_legibility
use course_contracts::biome::{StructureClass, WindowClass};
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use std::path::Path;

fn case(seed: u64, class: WindowClass) -> course_contracts::contracts::primitive_field::PrimitiveField {
    let id = RunIdentity::from_seed(seed);
    let mut spec = SiteSpec::generate_builtin(id, &SpecOverridesV2::default());
    spec.structure_class = StructureClass::new(
        class,
        spec.structure_class.provinces,
        spec.structure_class.boundary_kind,
    )
    .unwrap();
    course_primitives::generate(&spec, &id)
}

fn class_key(w: WindowClass) -> &'static str {
    match w {
        WindowClass::ValleyFloor => "valley_floor",
        WindowClass::Interfluve => "interfluve",
        WindowClass::EscarpmentFace => "escarpment_face",
        WindowClass::BasinMargin => "basin_margin",
        WindowClass::PiedmontSlope => "piedmont_slope",
        WindowClass::TerraceFlight => "terrace_flight",
    }
}

use stage_lab::render_v2::{render_c1, C1View};

fn main() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("out/c1_legibility");
    let labeled = base.join("labeled");
    let blind = base.join("blind");
    std::fs::create_dir_all(&labeled).unwrap();
    std::fs::create_dir_all(&blind).unwrap();

    // Labeled study sheets: 2 per class, overlays ON (learn the vocabulary).
    for &class in &WindowClass::ALL {
        for k in 0..2u64 {
            let c1 = case(2100 + k, class);
            let img = render_c1(&c1, C1View::Implied, 640, true);
            img.save(labeled.join(format!("{}_{k}.png", class_key(class)))).unwrap();
        }
    }

    // Blind set: 3 per class, overlays OFF (glyphs would leak the answer),
    // seeds disjoint from the study sheets. Deterministic shuffle.
    let mut entries = Vec::new();
    for (ci, &class) in WindowClass::ALL.iter().enumerate() {
        for k in 0..3u64 {
            entries.push((class, 2200 + ci as u64 * 3 + k));
        }
    }
    // Fixed permutation via a small LCG (no external RNG in examples).
    let mut order: Vec<usize> = (0..entries.len()).collect();
    let mut state: u64 = 0x6C62272E07BB0142;
    for i in (1..order.len()).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let j = (state >> 33) as usize % (i + 1);
        order.swap(i, j);
    }
    let mut key = std::collections::BTreeMap::new();
    for (slot, &idx) in order.iter().enumerate() {
        let (class, seed) = entries[idx];
        let name = format!("c1_{}.png", (b'a' + slot as u8) as char);
        let c1 = case(seed, class);
        let img = render_c1(&c1, C1View::Implied, 640, false);
        img.save(blind.join(&name)).unwrap();
        key.insert(name, class_key(class));
    }
    std::fs::write(
        blind.join("answer_key.json"),
        serde_json::to_string_pretty(&key).unwrap(),
    )
    .unwrap();
    println!(
        "wrote {} labeled + {} blind images to {}",
        WindowClass::ALL.len() * 2,
        entries.len(),
        base.display()
    );
}
