//! Headless stage-01 snapshots: the reviewable/CI form of the lab.
//!   cargo run -p stage-lab --release --example framing_snapshots
//!
//! Emits to crates/stage-lab/out/ (gitignored):
//!   framing_<archetype>_<seed>.png         — detail schematics, seeds {1, 7, 42}
//!   framing_implied_<archetype>_<seed>.png — illustrative implied terrain
//!   framing_gallery_<archetype>.png        — 4×4 forced contact sheet, seeds 0..16
//!   framing_gallery_natural.png            — 4×4 natural-draw contact sheet

#[path = "../src/data.rs"]
mod data;
#[path = "../src/render.rs"]
mod render;

use course_seed::RunIdentity;
use course_spec::ArchetypeId;
use data::{build_case, Case};
use image::RgbaImage;
use render::{render_framing_schematic, render_implied_terrain, ImpliedRelief};

const DETAIL_PX: u32 = 900;
/// Finer than the interactive app — snapshots are the reviewable form.
const PREVIEW_RES_M: f64 = 8.0;
const THUMB_PX: u32 = 220;
const GRID: u32 = 4;
const PAD: u32 = 6;

fn contact_sheet(thumbs: &[RgbaImage]) -> RgbaImage {
    let side = GRID * THUMB_PX + (GRID + 1) * PAD;
    let mut sheet = RgbaImage::from_pixel(side, side, image::Rgba([30, 30, 30, 255]));
    for (i, t) in thumbs.iter().enumerate().take((GRID * GRID) as usize) {
        let (gx, gy) = (i as u32 % GRID, i as u32 / GRID);
        let x = PAD + gx * (THUMB_PX + PAD);
        let y = PAD + gy * (THUMB_PX + PAD);
        image::imageops::replace(&mut sheet, t, x as i64, y as i64);
    }
    sheet
}

/// Preview amplitudes come from θ, never the artifact.
fn relief_of(c: &Case) -> ImpliedRelief {
    ImpliedRelief {
        ridge_relief_m: c.spec.param("framing.topo_ridge_relief_m"),
        trunk_carve_m: c.spec.param("framing.topo_trunk_carve_m"),
        step_riser_m: c.spec.param("framing.topo_step_riser_m"),
        province_relief_m: c.spec.param("framing.province_relief_m"),
    }
}

fn main() {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("out");
    std::fs::create_dir_all(&out).expect("create out dir");
    let empty = std::collections::BTreeMap::new();
    let case = |seed: u64, forced: Option<ArchetypeId>| {
        build_case(RunIdentity::from_seed(seed), forced, &empty).expect("case")
    };

    for arch in ArchetypeId::ALL {
        for seed in [1u64, 7, 42] {
            let c = case(seed, Some(arch));
            let img = render_framing_schematic(&c.framing, DETAIL_PX);
            let path = out.join(format!("framing_{}_{seed}.png", arch.key()));
            img.save(&path).expect("save detail png");

            let implied = render_implied_terrain(
                &c.framing,
                &relief_of(&c),
                DETAIL_PX,
                PREVIEW_RES_M,
                true,
            );
            let path = out.join(format!("framing_implied_{}_{seed}.png", arch.key()));
            implied.save(&path).expect("save implied png");
        }
        let thumbs: Vec<RgbaImage> = (0..u64::from(GRID * GRID))
            .map(|seed| render_framing_schematic(&case(seed, Some(arch)).framing, THUMB_PX))
            .collect();
        let path = out.join(format!("framing_gallery_{}.png", arch.key()));
        contact_sheet(&thumbs).save(&path).expect("save gallery png");
        println!("wrote {} detail + gallery", arch.key());
    }

    let thumbs: Vec<RgbaImage> = (0..u64::from(GRID * GRID))
        .map(|seed| render_framing_schematic(&case(seed, None).framing, THUMB_PX))
        .collect();
    contact_sheet(&thumbs)
        .save(out.join("framing_gallery_natural.png"))
        .expect("save natural gallery png");
    println!("wrote natural gallery -> {}", out.display());
}
