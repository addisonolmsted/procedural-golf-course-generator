//! Artifact save/load + the per-course manifest.
//!
//! Layout of a course directory (`output/courses/<slug>/`):
//!   stage02_course_spec.json
//!   stage03_macro_skeleton.json            (structure graph + sidecar list)
//!   stage03_macro_skeleton.base_height.cgrid
//!   …
//!   manifest.json
//!
//! Each artifact = one `<prefix>.json` plus zero or more `<prefix>.<field>.cgrid`
//! sidecars, where `<prefix> = stage{NN}_{KIND}`. The manifest records every
//! artifact's version and a blake3 content hash over its files (JSON first,
//! then sidecars in name order), so staleness and provenance are checkable
//! without loading anything.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::gridio;
use crate::spec::CourseSpec;
use crate::stages::*;

/// A stage output that can live in the artifact store.
pub trait Artifact: Sized {
    /// snake_case kind, unique across the pipeline (e.g. `"macro_skeleton"`).
    const KIND: &'static str;
    /// Stage number (file-name prefix + manifest ordering).
    const STAGE: u8;
    /// Bump on any shape change of THIS artifact.
    const VERSION: u32;

    fn save(&self, dir: &Path) -> io::Result<()>;
    fn load(dir: &Path) -> io::Result<Self>;

    fn prefix() -> String {
        format!("stage{:02}_{}", Self::STAGE, Self::KIND)
    }

    fn json_path(dir: &Path) -> PathBuf {
        dir.join(format!("{}.json", Self::prefix()))
    }

    fn sidecar_path(dir: &Path, field: &str) -> PathBuf {
        dir.join(format!("{}.{}.cgrid", Self::prefix(), field))
    }

    /// All files belonging to this artifact currently present in `dir`,
    /// JSON first then sidecars in name order (the hash order).
    fn files(dir: &Path) -> io::Result<Vec<PathBuf>> {
        let mut out = vec![Self::json_path(dir)];
        let side_prefix = format!("{}.", Self::prefix());
        let mut sidecars: Vec<PathBuf> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.extension().is_some_and(|x| x == "cgrid")
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(&side_prefix))
            })
            .collect();
        sidecars.sort();
        out.extend(sidecars);
        Ok(out)
    }

    /// blake3 over this artifact's files, in [`Artifact::files`] order.
    fn content_hash(dir: &Path) -> io::Result<String> {
        let mut h = blake3::Hasher::new();
        for f in Self::files(dir)? {
            h.update(&std::fs::read(&f)?);
        }
        Ok(h.finalize().to_hex().to_string())
    }
}

fn save_json<T: Serialize>(path: &Path, v: &T) -> io::Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(v).expect("artifact serializes"))
}

fn load_json<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    serde_json::from_str(&std::fs::read_to_string(path)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub stage: u8,
    pub version: u32,
    pub files: Vec<String>,
    pub blake3: String,
}

/// `manifest.json` — one entry per artifact present, keyed by KIND.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Manifest {
    pub pipeline_version: u32,
    pub entries: BTreeMap<String, ManifestEntry>,
}

impl Manifest {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join("manifest.json")
    }

    pub fn load(dir: &Path) -> io::Result<Manifest> {
        load_json(&Self::path(dir))
    }

    pub fn write(&self, dir: &Path) -> io::Result<()> {
        save_json(&Self::path(dir), self)
    }

    /// Record artifact `A` (already saved into `dir`).
    pub fn record<A: Artifact>(&mut self, dir: &Path) -> io::Result<()> {
        self.pipeline_version = crate::PIPELINE_VERSION;
        let files = A::files(dir)?
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        self.entries.insert(
            A::KIND.to_string(),
            ManifestEntry {
                stage: A::STAGE,
                version: A::VERSION,
                files,
                blake3: A::content_hash(dir)?,
            },
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Artifact impls
// ---------------------------------------------------------------------------

impl Artifact for CourseSpec {
    const KIND: &'static str = "course_spec";
    const STAGE: u8 = 2;
    const VERSION: u32 = 1;

    fn save(&self, dir: &Path) -> io::Result<()> {
        save_json(&Self::json_path(dir), self)
    }

    fn load(dir: &Path) -> io::Result<Self> {
        load_json(&Self::json_path(dir))
    }
}

/// JSON part of [`MacroSkeleton`] (grids ride as sidecars).
#[derive(Serialize, Deserialize)]
struct MacroSkeletonMeta {
    structure: StructureGraph,
}

impl Artifact for MacroSkeleton {
    const KIND: &'static str = "macro_skeleton";
    const STAGE: u8 = 3;
    const VERSION: u32 = 1;

    fn save(&self, dir: &Path) -> io::Result<()> {
        save_json(&Self::json_path(dir), &MacroSkeletonMeta { structure: self.structure.clone() })?;
        gridio::write_grid_f32(&Self::sidecar_path(dir, "base_height"), &self.base_height)?;
        gridio::write_grid_f32(&Self::sidecar_path(dir, "floor_damp"), &self.fields.floor_damp)?;
        gridio::write_grid_f32(&Self::sidecar_path(dir, "slope_gain"), &self.fields.slope_gain)?;
        gridio::write_grid_f32(&Self::sidecar_path(dir, "grain_dir_rad"), &self.fields.grain_dir_rad)?;
        gridio::write_grid_f32(&Self::sidecar_path(dir, "valley_dist_m"), &self.fields.valley_dist_m)?;
        gridio::write_grid_f32(&Self::sidecar_path(dir, "core_protect"), &self.fields.core_protect)
    }

    fn load(dir: &Path) -> io::Result<Self> {
        let meta: MacroSkeletonMeta = load_json(&Self::json_path(dir))?;
        Ok(MacroSkeleton {
            base_height: gridio::read_grid_f32(&Self::sidecar_path(dir, "base_height"))?,
            fields: SkeletonFields {
                floor_damp: gridio::read_grid_f32(&Self::sidecar_path(dir, "floor_damp"))?,
                slope_gain: gridio::read_grid_f32(&Self::sidecar_path(dir, "slope_gain"))?,
                grain_dir_rad: gridio::read_grid_f32(&Self::sidecar_path(dir, "grain_dir_rad"))?,
                valley_dist_m: gridio::read_grid_f32(&Self::sidecar_path(dir, "valley_dist_m"))?,
                core_protect: gridio::read_grid_f32(&Self::sidecar_path(dir, "core_protect"))?,
            },
            structure: meta.structure,
        })
    }
}

impl Artifact for ComposedHeight {
    const KIND: &'static str = "composed_height";
    const STAGE: u8 = 4;
    const VERSION: u32 = 1;

    fn save(&self, dir: &Path) -> io::Result<()> {
        save_json(&Self::json_path(dir), &serde_json::json!({}))?;
        gridio::write_grid_f32(&Self::sidecar_path(dir, "height"), &self.height)
    }

    fn load(dir: &Path) -> io::Result<Self> {
        Ok(ComposedHeight { height: gridio::read_grid_f32(&Self::sidecar_path(dir, "height"))? })
    }
}

/// JSON part of [`HydroBundle`].
#[derive(Serialize, Deserialize)]
struct HydroBundleMeta {
    graph: HydroGraph,
}

impl Artifact for HydroBundle {
    const KIND: &'static str = "hydro_bundle";
    const STAGE: u8 = 5;
    const VERSION: u32 = 1;

    fn save(&self, dir: &Path) -> io::Result<()> {
        save_json(&Self::json_path(dir), &HydroBundleMeta { graph: self.graph.clone() })?;
        gridio::write_grid_f32(&Self::sidecar_path(dir, "height"), &self.height)?;
        gridio::write_grid_f32(&Self::sidecar_path(dir, "zfill"), &self.flow.zfill)?;
        gridio::write_grid_f32(&Self::sidecar_path(dir, "flow_area_m2"), &self.flow.flow_area_m2)?;
        gridio::write_indices_u32(&Self::sidecar_path(dir, "rcv"), &self.height.spec, &self.flow.rcv)?;
        gridio::write_indices_u32(&Self::sidecar_path(dir, "order"), &self.height.spec, &self.flow.order)
    }

    fn load(dir: &Path) -> io::Result<Self> {
        let meta: HydroBundleMeta = load_json(&Self::json_path(dir))?;
        let height = gridio::read_grid_f32(&Self::sidecar_path(dir, "height"))?;
        let (_, rcv) = gridio::read_indices_u32(&Self::sidecar_path(dir, "rcv"))?;
        let (_, order) = gridio::read_indices_u32(&Self::sidecar_path(dir, "order"))?;
        Ok(HydroBundle {
            flow: FlowModel {
                rcv,
                order,
                zfill: gridio::read_grid_f32(&Self::sidecar_path(dir, "zfill"))?,
                flow_area_m2: gridio::read_grid_f32(&Self::sidecar_path(dir, "flow_area_m2"))?,
            },
            height,
            graph: meta.graph,
        })
    }
}

impl Artifact for CoverMap {
    const KIND: &'static str = "cover_map";
    const STAGE: u8 = 6;
    const VERSION: u32 = 1;

    fn save(&self, dir: &Path) -> io::Result<()> {
        save_json(&Self::json_path(dir), &serde_json::json!({}))?;
        gridio::write_grid_u8(&Self::sidecar_path(dir, "class"), &self.class)?;
        gridio::write_grid_f32(&Self::sidecar_path(dir, "canopy"), &self.canopy)
    }

    fn load(dir: &Path) -> io::Result<Self> {
        Ok(CoverMap {
            class: gridio::read_grid_u8(&Self::sidecar_path(dir, "class"))?,
            canopy: gridio::read_grid_f32(&Self::sidecar_path(dir, "canopy"))?,
        })
    }
}

impl Artifact for GateReport {
    const KIND: &'static str = "gate_report";
    const STAGE: u8 = 7;
    const VERSION: u32 = 1;

    fn save(&self, dir: &Path) -> io::Result<()> {
        save_json(&Self::json_path(dir), self)
    }

    fn load(dir: &Path) -> io::Result<Self> {
        load_json(&Self::json_path(dir))
    }
}

impl Artifact for RoutingPlan {
    const KIND: &'static str = "routing_plan";
    const STAGE: u8 = 8;
    const VERSION: u32 = 1;

    fn save(&self, dir: &Path) -> io::Result<()> {
        save_json(&Self::json_path(dir), self)
    }

    fn load(dir: &Path) -> io::Result<Self> {
        load_json(&Self::json_path(dir))
    }
}

/// JSON part of [`EarthworksDelta`]: features + patch count (patch grids ride
/// as numbered sidecars `patch000…`).
#[derive(Serialize, Deserialize)]
struct EarthworksMeta {
    features: Vec<EarthFeature>,
    patch_count: u32,
}

impl Artifact for EarthworksDelta {
    const KIND: &'static str = "earthworks_delta";
    const STAGE: u8 = 9;
    const VERSION: u32 = 1;

    fn save(&self, dir: &Path) -> io::Result<()> {
        save_json(
            &Self::json_path(dir),
            &EarthworksMeta {
                features: self.features.clone(),
                patch_count: self.patches.len() as u32,
            },
        )?;
        for (i, p) in self.patches.iter().enumerate() {
            gridio::write_grid_f32(&Self::sidecar_path(dir, &format!("patch{i:03}")), &p.delta_m)?;
        }
        Ok(())
    }

    fn load(dir: &Path) -> io::Result<Self> {
        let meta: EarthworksMeta = load_json(&Self::json_path(dir))?;
        let mut patches = Vec::with_capacity(meta.patch_count as usize);
        for i in 0..meta.patch_count {
            patches.push(HeightPatch {
                delta_m: gridio::read_grid_f32(&Self::sidecar_path(dir, &format!("patch{i:03}")))?,
            });
        }
        Ok(EarthworksDelta { patches, features: meta.features })
    }
}

/// JSON part of [`DrainageReport`].
#[derive(Serialize, Deserialize)]
struct DrainageMeta {
    pass: bool,
    metrics: BTreeMap<String, f64>,
    notes: Vec<String>,
    repair_count: u32,
}

impl Artifact for DrainageReport {
    const KIND: &'static str = "drainage_report";
    const STAGE: u8 = 10;
    const VERSION: u32 = 1;

    fn save(&self, dir: &Path) -> io::Result<()> {
        save_json(
            &Self::json_path(dir),
            &DrainageMeta {
                pass: self.pass,
                metrics: self.metrics.clone(),
                notes: self.notes.clone(),
                repair_count: self.repair_patches.len() as u32,
            },
        )?;
        for (i, p) in self.repair_patches.iter().enumerate() {
            gridio::write_grid_f32(&Self::sidecar_path(dir, &format!("repair{i:03}")), &p.delta_m)?;
        }
        Ok(())
    }

    fn load(dir: &Path) -> io::Result<Self> {
        let meta: DrainageMeta = load_json(&Self::json_path(dir))?;
        let mut repair_patches = Vec::with_capacity(meta.repair_count as usize);
        for i in 0..meta.repair_count {
            repair_patches.push(HeightPatch {
                delta_m: gridio::read_grid_f32(&Self::sidecar_path(dir, &format!("repair{i:03}")))?,
            });
        }
        Ok(DrainageReport {
            pass: meta.pass,
            metrics: meta.metrics,
            notes: meta.notes,
            repair_patches,
        })
    }
}
