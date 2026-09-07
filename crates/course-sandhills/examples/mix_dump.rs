//! Dump a MIXED run: each seed's mode comes from its own coin (`draw::site`
//! with nothing forced), and only tiles that carry a creek or river are kept.
//!
//!   mix_dump <out_dir> <first_seed> <keep_n> [--stride k --offset j] [--seeds a,b,c]
//!
//! Walks seeds first_seed + offset, + offset + stride, ... until `keep_n`
//! tiles with a river line are written. Files: `m_<seed>.cgrid`,
//! `m_<seed>.water.cgrid`, `m_<seed>.creek.txt`, and a line per kept seed
//! appended to `index_<offset>.txt` as `<seed> <mode> <lake_frac>`.
//! Stride/offset let several processes share one range without touching
//! the same seeds.
use course_sandhills::{assemble::HandProfile, build_fluvial_textured, build_full, draw,
                       texture, Mode};
use course_seed::RunIdentity;
use course_world::gridio;
use std::io::Write;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    let first: u64 = a[1].parse().unwrap();
    let keep_n: usize = a[2].parse().unwrap();
    let opt = |k: &str, d: u64| -> u64 {
        a.iter().position(|x| x == k).map(|i| a[i + 1].parse().unwrap()).unwrap_or(d)
    };
    let (stride, offset) = (opt("--stride", 1), opt("--offset", 0));
    // `--seeds a,b,c`: exactly these seeds, kept or skipped on the same rule
    let only: Option<Vec<u64>> = a.iter().position(|x| x == "--seeds")
        .map(|i| a[i + 1].split(',').map(|v| v.parse().unwrap()).collect());
    let prof = HandProfile::load(std::path::Path::new(
        "assets/sandhills_hand_profile.txt")).expect("hand profile");
    let fpack = texture::PatchPack::load(std::path::Path::new(
        "assets/sandhills_patches_fluvial.bin")).expect("fluvial pack");
    let apack = texture::PatchPack::load(std::path::Path::new(
        "assets/sandhills_patches_aeolian.bin")).expect("aeolian pack");
    let mut index = std::fs::OpenOptions::new().create(true).append(true)
        .open(out.join(format!("index_{offset}.txt"))).unwrap();

    let mut kept = 0usize;
    let mut seed = first + offset;
    let mut tried = 0usize;
    while kept < keep_n {
        if let Some(list) = &only {
            if tried >= list.len() {
                break;
            }
            seed = list[tried];
        }
        tried += 1;
        let id = RunIdentity::from_seed(seed);
        let mode = draw::site(&id, None, None).mode;
        let (height, water, river, lake_frac) = match mode {
            Mode::Aeolian => {
                let t = build_full(&id, &apack, None);
                (t.height, t.water, t.river, t.lake_frac)
            }
            Mode::Fluvial => {
                let (_, _, _, tex, w, _) = build_fluvial_textured(&id, &prof, &fpack);
                (tex, w.surface, w.river, w.lake_frac)
            }
        };
        if let Some(r) = river.filter(|r| r.len() > 1) {
            gridio::write_grid_f32(&out.join(format!("m_{seed}.cgrid")), &height).unwrap();
            gridio::write_grid_f32(&out.join(format!("m_{seed}.water.cgrid")), &water).unwrap();
            let txt: String = r.iter().map(|p| format!("{:.2} {:.2}\n", p.x, p.y)).collect();
            std::fs::write(out.join(format!("m_{seed}.creek.txt")), txt).unwrap();
            writeln!(index, "{seed} {} {lake_frac:.4}",
                     match mode { Mode::Aeolian => "aeolian", Mode::Fluvial => "fluvial" }).unwrap();
            kept += 1;
            println!("seed {seed}: {mode:?}, kept ({kept}/{keep_n}, {tried} tried)");
        } else {
            println!("seed {seed}: {mode:?}, no river -- skipped");
        }
        seed += stride;
    }
}
