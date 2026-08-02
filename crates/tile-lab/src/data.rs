//! Campaign artifact IO: the tile store written by `tools/macro_campaign`.
//!
//! Every measured value is `Option<f64>` — the extractor emits JSON `null`
//! where a detector found nothing (no channels ⇒ no fall gradient), and a
//! missing measurement is exactly what the QA viewer needs to show.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use course_world::gridio;
use course_world::Grid;
use serde::{Deserialize, Serialize};

/// One fetched tile in the campaign store.
#[derive(Clone, Debug)]
pub struct TileEntry {
    pub archetype: String,
    pub id: String,
    pub cgrid: PathBuf,
}

/// `out/tiles/<archetype>/<id>.cgrid` → archetype → tiles (sorted).
pub fn scan_tiles(out_root: &Path) -> BTreeMap<String, Vec<TileEntry>> {
    let mut map: BTreeMap<String, Vec<TileEntry>> = BTreeMap::new();
    let Ok(archs) = std::fs::read_dir(out_root.join("tiles")) else {
        return map;
    };
    let mut arch_dirs: Vec<_> = archs.filter_map(Result::ok).map(|e| e.path()).collect();
    arch_dirs.sort();
    for dir in arch_dirs {
        if !dir.is_dir() {
            continue;
        }
        let archetype = dir.file_name().unwrap().to_string_lossy().to_string();
        let Ok(files) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<TileEntry> = files
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "cgrid"))
            .map(|p| TileEntry {
                archetype: archetype.clone(),
                id: p.file_stem().unwrap().to_string_lossy().to_string(),
                cgrid: p,
            })
            .collect();
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        if !entries.is_empty() {
            map.insert(archetype, entries);
        }
    }
    map
}

// ---------------------------------------------------------------------------
// regions.json — what the detectors saw, in world metres (y up)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Regions {
    #[serde(default)]
    pub tilt: Option<Tilt>,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub ridges: Vec<RidgeRegion>,
    #[serde(default)]
    pub basins: Vec<Basin>,
    #[serde(default)]
    pub scarps: Vec<Scarp>,
    #[serde(default)]
    pub transects: Vec<Transect>,
    #[serde(default)]
    pub valley_centerlines: Vec<Centerline>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Tilt {
    pub grade: f64,
    pub downhill_xy: Option<[f64; 2]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Channel {
    pub diag_m: f64,
    pub bbox_m: [f64; 4],
    #[serde(default)]
    pub fall_grad: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RidgeRegion {
    pub kind: String,
    #[serde(default)]
    pub diag_m: Option<f64>,
    #[serde(default)]
    pub bbox_m: Option<[f64; 4]>,
    #[serde(default)]
    pub centerline_m: Vec<[f64; 2]>,
    #[serde(default)]
    pub prominence_m: Option<f64>,
    #[serde(default)]
    pub len_m: Option<f64>,
    #[serde(default)]
    pub crest_hw_m: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Basin {
    pub center_m: [f64; 2],
    pub radius_m: f64,
    pub depth_m: f64,
    #[serde(default)]
    pub ecc: Option<f64>,
    pub accepted: bool,
}

/// A traced terrace edge (blufffit). Replaced the old axis-aligned "bench
/// band", which could not represent a curved scarp.
#[derive(Clone, Debug, Deserialize)]
pub struct Scarp {
    #[serde(default)]
    pub centerline_m: Vec<[f64; 2]>,
    pub height_m: f64,
    pub face_grad: f64,
    pub length_m: f64,
    pub accepted: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Transect {
    pub center_m: [f64; 2],
    pub perp_xy: [f64; 2],
    pub hw_m: f64,
    #[serde(default)]
    pub wall_grade: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Centerline {
    #[serde(default)]
    pub pts_m: Vec<[f64; 2]>,
    #[serde(default)]
    pub top_width_m: Option<f64>,
    #[serde(default)]
    pub meander: Option<Meander>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Meander {
    #[serde(default)]
    pub intensity: Option<f64>,
    #[serde(default)]
    pub wavelength_mult: Option<f64>,
    #[serde(default)]
    pub sinuosity: Option<f64>,
}

/// `<id>.json` — the per-tile knob record the prior fit consumes.
#[derive(Clone, Debug, Deserialize)]
pub struct KnobRecord {
    #[serde(default)]
    pub extract_version: Option<u32>,
    #[serde(default)]
    pub shape_version: Option<u32>,
    pub knobs: BTreeMap<String, Option<f64>>,
    #[serde(default)]
    pub extras: BTreeMap<String, Option<f64>>,
    #[serde(default)]
    pub valid_frac: Option<f64>,
}

/// Everything the viewer needs for one tile. Extract artifacts are optional:
/// a freshly fetched tile renders as bare terrain until `extract` runs.
pub struct TileData {
    pub height: Grid<f64>,
    pub classes: Option<Grid<u8>>,
    pub regions: Option<Regions>,
    pub knobs: Option<KnobRecord>,
}

pub fn load_tile(out_root: &Path, e: &TileEntry) -> io::Result<TileData> {
    let height = gridio::read_grid_f32(&e.cgrid)?;
    let ex = out_root.join("extract").join(&e.archetype);
    let classes = gridio::read_grid_u8(&ex.join(format!("{}.classes.cgrid", e.id))).ok();
    let regions = read_json(&ex.join(format!("{}.regions.json", e.id)));
    let knobs = read_json(&ex.join(format!("{}.json", e.id)));
    Ok(TileData { height, classes, regions, knobs })
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(err) => {
            eprintln!("tile-lab: {} — {err}", path.display());
            None
        }
    }
}

// ---------------------------------------------------------------------------
// exclude.json — the QA cull list, honored by fit_knobs.collect()
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExcludedTile {
    pub archetype: String,
    pub tile: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Excluded {
    pub version: u32,
    pub tiles: Vec<ExcludedTile>,
}

impl Default for Excluded {
    fn default() -> Self {
        Excluded { version: 1, tiles: Vec::new() }
    }
}

impl Excluded {
    pub fn contains(&self, archetype: &str, tile: &str) -> bool {
        self.tiles
            .iter()
            .any(|t| t.archetype == archetype && t.tile == tile)
    }

    pub fn reason(&self, archetype: &str, tile: &str) -> Option<&str> {
        self.tiles
            .iter()
            .find(|t| t.archetype == archetype && t.tile == tile)
            .map(|t| t.reason.as_str())
    }

    pub fn set(&mut self, archetype: &str, tile: &str, reason: &str) {
        self.remove(archetype, tile);
        self.tiles.push(ExcludedTile {
            archetype: archetype.to_string(),
            tile: tile.to_string(),
            reason: reason.to_string(),
        });
        self.tiles.sort_by(|a, b| {
            (a.archetype.as_str(), a.tile.as_str()).cmp(&(b.archetype.as_str(), b.tile.as_str()))
        });
    }

    pub fn remove(&mut self, archetype: &str, tile: &str) {
        self.tiles
            .retain(|t| !(t.archetype == archetype && t.tile == tile));
    }
}

pub fn exclude_path(out_root: &Path) -> PathBuf {
    out_root.join("exclude.json")
}

pub fn load_exclude(out_root: &Path) -> Excluded {
    read_json(&exclude_path(out_root)).unwrap_or_default()
}

pub fn save_exclude(out_root: &Path, e: &Excluded) -> io::Result<()> {
    let text = serde_json::to_string_pretty(e).map_err(io::Error::other)?;
    std::fs::write(exclude_path(out_root), text + "\n")
}
