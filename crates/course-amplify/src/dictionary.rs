//! The patch dictionary: REAL terrain patches (not a basis — the spike
//! falsified drawn-coefficient PCA) per conditioning bucket, baked offline
//! by `tools/dictionary/build.py` to `assets/dictionary_v2.bin`.
//!
//! Layout: `b"CDIC"` + u32 format version + u32 header length + JSON header
//! + raw i16 patch block. Each patch is PATCH×PATCH i16 heights with a
//! per-patch f32 scale (`height = i16 * scale`); gradients are recomputed
//! at load. The JSON header carries per-biome/per-level bucket edges,
//! per-bucket amplitude stats + radial-PSD equalizer targets, per-patch
//! source-tile provenance, and the held-out tile lists F3 certifies on.
//!
//! Fingerprint interlock mirrors the envelope's: the asset's blake3 must
//! match `assets/dictionary_v2.fingerprint` (committed alongside), so a
//! stale or hand-edited asset fails loudly at load, never silently.

use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug)]
pub enum DictError {
    Io(std::io::Error),
    Format(String),
    Fingerprint { found: String, expected: String },
}

impl std::fmt::Display for DictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DictError::Io(e) => write!(f, "dictionary io: {e}"),
            DictError::Format(m) => write!(f, "dictionary format: {m}"),
            DictError::Fingerprint { found, expected } => write!(
                f,
                "dictionary fingerprint mismatch: found {found}, expected {expected} — \
                 re-bake via tools/dictionary/build.py"
            ),
        }
    }
}

impl std::error::Error for DictError {}

#[derive(serde::Deserialize)]
struct HeaderPatch {
    src: String,
    scale: f64,
    offset: usize,
    cond: Vec<f64>,
    #[serde(default)]
    borrowed: bool,
}

#[derive(serde::Deserialize)]
struct HeaderBucket {
    amp_p25: f64,
    amp_p50: f64,
    amp_p75: f64,
    equalizer: Vec<f64>,
    n_src_tiles: usize,
    patches: Vec<HeaderPatch>,
}

#[derive(serde::Deserialize)]
struct HeaderLevel {
    edges: Vec<Vec<f64>>,
    cell_m: f64,
    buckets: BTreeMap<String, HeaderBucket>,
}

#[derive(serde::Deserialize)]
struct Header {
    format_version: u32,
    patch: usize,
    biomes: BTreeMap<String, BTreeMap<String, HeaderLevel>>,
    holdout_tiles: BTreeMap<String, Vec<String>>,
}

/// One dequantized patch: PATCH×PATCH heights (row-major, metres) plus
/// provenance and the conditioning vector it was harvested under.
pub struct Patch {
    pub heights: Vec<f32>,
    pub src_tile: String,
    pub cond: [f64; 4],
    /// True when curation borrowed this patch from a conditioning-adjacent
    /// bucket to satisfy the >=5-source-tile diversity floor — its `cond`
    /// maps to its ORIGIN bucket, not this one.
    pub borrowed: bool,
    /// Dominant GRAIN axis of this patch's texture, [0, π). Computed at
    /// load from the stored heights (pure function of them — nothing
    /// baked, so borrowed patches carry correct axes too).
    pub axis_rad: f64,
    /// Doubled-angle resultant length in [0, 1]: 0 = isotropic texture
    /// (axis meaningless), 1 = perfectly striped.
    pub coherence: f64,
}

/// Dominant grain axis + coherence of a PATCH×PATCH height tile.
///
/// Structure tensor via doubled angles: each interior cell contributes
/// its gradient energy at twice the gradient angle, so opposite
/// gradients reinforce instead of cancelling (axes, not directions —
/// every mean here is circular by construction). The GRADIENT axis is
/// PERPENDICULAR to the grain: ridges running along x have gradients
/// along y. The +π/2 below is that flip — it lives here and only here.
pub fn patch_axis(heights: &[f32], patch: usize) -> (f64, f64) {
    let (mut c2, mut s2, mut g2) = (0.0f64, 0.0f64, 0.0f64);
    for y in 1..patch - 1 {
        for x in 1..patch - 1 {
            let gx = (heights[y * patch + x + 1] - heights[y * patch + x - 1]) as f64 * 0.5;
            let gy = (heights[(y + 1) * patch + x] - heights[(y - 1) * patch + x]) as f64 * 0.5;
            c2 += gx * gx - gy * gy;
            s2 += 2.0 * gx * gy;
            g2 += gx * gx + gy * gy;
        }
    }
    if g2 <= 1e-12 {
        return (0.0, 0.0);
    }
    let grad_axis = 0.5 * libm::atan2(s2, c2);
    let axis = (grad_axis + std::f64::consts::FRAC_PI_2).rem_euclid(std::f64::consts::PI);
    let coherence = (c2 * c2 + s2 * s2).sqrt() / g2;
    (axis, coherence)
}

pub struct Bucket {
    pub amp_p25: f64,
    pub amp_p50: f64,
    pub amp_p75: f64,
    pub equalizer: Vec<f64>,
    pub n_src_tiles: usize,
    pub patches: Vec<Patch>,
}

pub struct Level {
    /// Quantile edges per conditioning dim (slope, tpi, relief_pos,
    /// log1p dist-to-channel) — bucket id = mixed-radix index over them.
    pub edges: Vec<Vec<f64>>,
    pub cell_m: f64,
    pub buckets: BTreeMap<u32, Bucket>,
}

pub struct Dictionary {
    pub patch: usize,
    pub biomes: BTreeMap<String, BTreeMap<String, Level>>,
    pub holdout_tiles: BTreeMap<String, Vec<String>>,
    pub fingerprint: String,
}

impl Dictionary {
    /// Load and verify against the committed fingerprint sidecar.
    pub fn load(asset: &Path) -> Result<Dictionary, DictError> {
        let bytes = std::fs::read(asset).map_err(DictError::Io)?;
        let expected = std::fs::read_to_string(asset.with_extension("fingerprint"))
            .map_err(DictError::Io)?;
        let expected = expected.trim().to_string();
        let found = blake3::hash(&bytes).to_hex().to_string();
        if found != expected {
            return Err(DictError::Fingerprint { found, expected });
        }
        Self::parse(&bytes, found)
    }

    fn parse(bytes: &[u8], fingerprint: String) -> Result<Dictionary, DictError> {
        if bytes.len() < 12 || &bytes[0..4] != b"CDIC" {
            return Err(DictError::Format("bad magic".into()));
        }
        let ver = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if ver != 1 {
            return Err(DictError::Format(format!("format_version {ver} != 1")));
        }
        let hlen = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let header: Header = serde_json::from_slice(&bytes[12..12 + hlen])
            .map_err(|e| DictError::Format(format!("header json: {e}")))?;
        if header.format_version != 1 {
            return Err(DictError::Format("header/container version mismatch".into()));
        }
        let blob = &bytes[12 + hlen..];
        let psz = header.patch * header.patch;
        let mut biomes = BTreeMap::new();
        for (bname, levels) in header.biomes {
            let mut lmap = BTreeMap::new();
            for (lname, l) in levels {
                let mut buckets = BTreeMap::new();
                for (bid, b) in l.buckets {
                    let bid: u32 = bid
                        .parse()
                        .map_err(|_| DictError::Format(format!("bucket id {bid}")))?;
                    let mut patches = Vec::with_capacity(b.patches.len());
                    for p in &b.patches {
                        let end = p.offset + psz * 2;
                        if end > blob.len() {
                            return Err(DictError::Format("patch offset out of range".into()));
                        }
                        let mut heights = Vec::with_capacity(psz);
                        for c in blob[p.offset..end].chunks_exact(2) {
                            let v = i16::from_le_bytes([c[0], c[1]]);
                            heights.push(v as f32 * p.scale as f32);
                        }
                        let mut cond = [0.0f64; 4];
                        for (i, v) in p.cond.iter().take(4).enumerate() {
                            cond[i] = *v;
                        }
                        let (axis_rad, coherence) = patch_axis(&heights, header.patch);
                        patches.push(Patch {
                            heights,
                            src_tile: p.src.clone(),
                            cond,
                            borrowed: p.borrowed,
                            axis_rad,
                            coherence,
                        });
                    }
                    buckets.insert(
                        bid,
                        Bucket {
                            amp_p25: b.amp_p25,
                            amp_p50: b.amp_p50,
                            amp_p75: b.amp_p75,
                            equalizer: b.equalizer.clone(),
                            n_src_tiles: b.n_src_tiles,
                            patches,
                        },
                    );
                }
                lmap.insert(
                    lname,
                    Level {
                        edges: l.edges,
                        cell_m: l.cell_m,
                        buckets,
                    },
                );
            }
            biomes.insert(bname, lmap);
        }
        Ok(Dictionary {
            patch: header.patch,
            biomes,
            holdout_tiles: header.holdout_tiles,
            fingerprint,
        })
    }

    /// Mixed-radix bucket id for a conditioning vector, identical to the
    /// builder's `bucket_id` — the runtime lookup S3 uses.
    pub fn bucket_of(level: &Level, cond: &[f64; 4]) -> u32 {
        let mut idx = 0u32;
        for (k, e) in level.edges.iter().enumerate() {
            let mut b = 0u32;
            for edge in e {
                // strict less-than count — matches numpy searchsorted
                // side='left' in the builder exactly (ties bin LOW)
                if cond[k] > *edge {
                    b += 1;
                }
            }
            idx = idx * (e.len() as u32 + 1) + b;
        }
        idx
    }
}
