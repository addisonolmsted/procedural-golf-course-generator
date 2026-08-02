//! Headless batch render of campaign tiles with their landform overlays —
//! the reviewable form of what tile-lab shows interactively.
//!
//! `cargo run -p tile-lab --example tile_snapshots --release -- <out_dir> [campaign_out]`

#[path = "../src/data.rs"]
mod data;

use std::path::{Path, PathBuf};

use course_world::math::Vec2;

const IMG_PX: u32 = 900;

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let dest = PathBuf::from(args.next().expect("usage: <out_dir> [campaign_out]"));
    let root = args.next().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/macro_campaign/out")
    });
    std::fs::create_dir_all(&dest)?;

    for (arch, entries) in data::scan_tiles(&root) {
        for e in entries {
            let d = data::load_tile(&root, &e)?;
            let mut img = course_viz::render_height_grid_nan(
                &d.height,
                IMG_PX,
                course_viz::finite_range(&d.height),
                [92, 92, 100],
            );
            if let Some(classes) = &d.classes {
                for (mask, color) in [
                    (1u8, [255, 0, 255, 160]),      // nodata
                    (128, [180, 30, 30, 130]),      // developed (OSM)
                    (32, [200, 60, 200, 90]),       // fill flats
                    (2, [64, 132, 244, 130]),       // channels
                    (4, [235, 140, 50, 80]),        // ridges (geomorphon)
                    (64, [235, 140, 50, 150]),      // ridges (accepted)
                    (8, [90, 200, 220, 140]),       // basins accepted
                    (16, [120, 120, 120, 110]),     // basins rejected
                ] {
                    course_viz::overlay_class_mask(&mut img, classes, mask, color);
                }
            }
            if let Some(r) = &d.regions {
                for sc in &r.scarps {
                    if sc.centerline_m.len() < 2 {
                        continue;
                    }
                    let pts: Vec<Vec2> =
                        sc.centerline_m.iter().map(|p| Vec2::new(p[0], p[1])).collect();
                    let (c, thick) = if sc.accepted {
                        ([220, 60, 60, 255], 1)
                    } else {
                        ([170, 120, 120, 255], 0)
                    };
                    course_viz::draw_polyline(&mut img, &pts, c, thick);
                }
                for b in &r.basins {
                    let c = if b.accepted { [90, 200, 220, 255] } else { [140, 140, 140, 255] };
                    course_viz::draw_circle(
                        &mut img,
                        Vec2::new(b.center_m[0], b.center_m[1]),
                        b.radius_m,
                        c,
                        0,
                    );
                }
                for t in &r.transects {
                    let ctr = Vec2::new(t.center_m[0], t.center_m[1]);
                    let dir = Vec2::new(t.perp_xy[0], t.perp_xy[1]);
                    course_viz::draw_polyline(
                        &mut img,
                        &[ctr, ctr + dir * t.hw_m],
                        [255, 240, 120, 255],
                        0,
                    );
                }
                for cl in &r.valley_centerlines {
                    if cl.pts_m.len() > 1 {
                        let pts: Vec<Vec2> =
                            cl.pts_m.iter().map(|p| Vec2::new(p[0], p[1])).collect();
                        course_viz::draw_polyline(&mut img, &pts, [40, 240, 200, 255], 1);
                    }
                }
            }
            let path: PathBuf = Path::new(&dest).join(format!("{arch}_{}.png", e.id));
            img.save(&path).map_err(std::io::Error::other)?;
            let n_basin = d
                .regions
                .as_ref()
                .map_or(0, |r| r.basins.iter().filter(|b| b.accepted).count());
            println!(
                "{}  basins {n_basin}  {}",
                path.display(),
                d.knobs
                    .as_ref()
                    .and_then(|k| k.knobs.get("relief_amp_m").copied().flatten())
                    .map_or("—".to_string(), |v| format!("relief {v:.1} m"))
            );
        }
    }
    Ok(())
}
