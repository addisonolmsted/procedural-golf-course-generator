//! The on-disk step-03 artifact: a `macro_skeleton/` directory.
//!
//! Contract files (downstream steps read ONLY these):
//!   structure.json                — the structure graph
//!   base_height.cgrid + 5 fields  — CGRID1 sidecars (f64 stored as f32)
//!   meta.json                     — provenance + in-memory f64 grid hashes
//! Advisory sidecars (lab/tooling only — downstream MUST NOT read them):
//!   plan.json                     — the staged planner IR
//!   config.json                   — the analytic config (re-rasterizable)
//!
//! Golden hashes are taken over the in-memory f64 grids BEFORE the lossy f32
//! store round-trip (the CGRID1 convention), and recorded in meta.json.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path as FsPath;

use course_spec::CourseSpec;
use course_world::gridio;
use course_world::world::fnv_f64;
use course_world::Grid;
use serde::{Deserialize, Serialize};

use crate::plan::MacroPlan;
use crate::skeleton::{MacroSkeleton, SkeletonFields, StructureGraph};
use crate::{MacroConfig, MacroResult};

pub const GRID_NAMES: [&str; 6] = [
    "base_height",
    "floor_damp",
    "slope_gain",
    "grain_dir_rad",
    "valley_dist_m",
    "core_protect",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MacroMeta {
    pub macro_version: u32,
    pub res_m: f64,
    pub seed: u64,
    pub archetype: String,
    pub prior_fingerprint: String,
    /// FNV-1a over each grid's in-memory f64 bits (pre-serialization).
    pub grid_fnv: BTreeMap<String, u64>,
}

fn grids_of(s: &MacroSkeleton) -> [(&'static str, &Grid<f64>); 6] {
    [
        ("base_height", &s.base_height),
        ("floor_damp", &s.fields.floor_damp),
        ("slope_gain", &s.fields.slope_gain),
        ("grain_dir_rad", &s.fields.grain_dir_rad),
        ("valley_dist_m", &s.fields.valley_dist_m),
        ("core_protect", &s.fields.core_protect),
    ]
}

/// Write the full artifact directory (created if absent, files overwritten).
pub fn write_dir(dir: &FsPath, spec: &CourseSpec, result: &MacroResult) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let s = &result.skeleton;
    let mut grid_fnv = BTreeMap::new();
    for (name, g) in grids_of(s) {
        grid_fnv.insert(name.to_string(), fnv_f64(&g.data));
        gridio::write_grid_f32(&dir.join(format!("{name}.cgrid")), g)?;
    }
    let meta = MacroMeta {
        macro_version: crate::MACRO_VERSION,
        res_m: s.base_height.spec.cell_size,
        seed: spec.identity.seed,
        archetype: serde_json::to_value(spec.archetype)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default(),
        prior_fingerprint: spec.prior_fingerprint.clone(),
        grid_fnv,
    };
    let write_json = |name: &str, body: String| fs::write(dir.join(name), body);
    write_json("meta.json", serde_json::to_string(&meta)?)?;
    write_json(
        "structure.json",
        serde_json::to_string(&result.skeleton.structure)?,
    )?;
    write_json("plan.json", serde_json::to_string(&result.plan)?)?;
    write_json("config.json", serde_json::to_string(&result.config)?)?;
    Ok(())
}

/// Load the contract part (skeleton + meta). Fails loudly on any mismatch
/// between meta's grid list and the sidecars present.
pub fn load_skeleton(dir: &FsPath) -> io::Result<(MacroSkeleton, MacroMeta)> {
    let meta: MacroMeta = serde_json::from_str(&fs::read_to_string(dir.join("meta.json"))?)?;
    if meta.macro_version != crate::MACRO_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("macro_version {} != {}", meta.macro_version, crate::MACRO_VERSION),
        ));
    }
    let structure: StructureGraph =
        serde_json::from_str(&fs::read_to_string(dir.join("structure.json"))?)?;
    let read = |name: &str| gridio::read_grid_f32(&dir.join(format!("{name}.cgrid")));
    let skeleton = MacroSkeleton {
        base_height: read("base_height")?,
        fields: SkeletonFields {
            floor_damp: read("floor_damp")?,
            slope_gain: read("slope_gain")?,
            grain_dir_rad: read("grain_dir_rad")?,
            valley_dist_m: read("valley_dist_m")?,
            core_protect: read("core_protect")?,
        },
        structure,
    };
    Ok((skeleton, meta))
}

/// Load an advisory sidecar (lab/tooling use only).
pub fn load_plan(dir: &FsPath) -> io::Result<MacroPlan> {
    Ok(serde_json::from_str(&fs::read_to_string(
        dir.join("plan.json"),
    )?)?)
}

pub fn load_config(dir: &FsPath) -> io::Result<MacroConfig> {
    MacroConfig::from_json(&fs::read_to_string(dir.join("config.json"))?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// FNV-1a over every file in the directory (sorted by name; name bytes then
/// content bytes) — the determinism double-run comparator.
pub fn dir_fnv(dir: &FsPath) -> io::Result<u64> {
    let mut names: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut h = 1469598103934665603u64;
    let mut fold = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(1099511628211);
        }
    };
    for n in &names {
        fold(n.as_bytes());
        fold(&fs::read(dir.join(n))?);
    }
    Ok(h)
}
