//! Compact binary form of the atlas (`assets/atlas.bin`).
//!
//! Elevations round-trip losslessly: they are stored exactly as the source
//! encodes them — u16 decimeters above `emin`. Fixed little-endian layout,
//! versioned by magic, no compression (≈3.6 MB for 27 courses).

use crate::{Atlas, Course, Hole, StyleGroup, GRID_N};
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

// GATLAS2 adds the tree-canopy mask after the water mask (per course).
const MAGIC: &[u8; 8] = b"GATLAS2\n";

/// Where the packed atlas lives in this repo. Falls back to a CWD-relative
/// path so binaries also work outside a cargo checkout.
pub fn default_bin_path() -> PathBuf {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("assets/atlas.bin"));
    match repo {
        Some(p) if p.exists() => p,
        _ => PathBuf::from("assets/atlas.bin"),
    }
}

pub fn save(atlas: &Atlas, path: &Path) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut w = BufWriter::new(File::create(path)?);
    w.write_all(MAGIC)?;
    write_u32(&mut w, atlas.courses.len() as u32)?;
    for c in &atlas.courses {
        write_str(&mut w, &c.key)?;
        write_str(&mut w, &c.label)?;
        write_str(&mut w, &c.arch)?;
        w.write_all(&[c.group.code()])?;
        for v in [c.wm, c.hm, c.emin, c.emax] {
            write_f64(&mut w, v)?;
        }
        write_f64(&mut w, c.waterpct as f64)?;
        write_f64(&mut w, c.treepct as f64)?;
        for v in c.bbox {
            write_f64(&mut w, v)?;
        }
        for i in 0..GRID_N * GRID_N {
            write_u16(&mut w, decimeters(c, i))?;
        }
        // Water then tree mask, each packed MSB-first (source encoding).
        write_mask(&mut w, &c.water)?;
        write_mask(&mut w, &c.trees)?;
        write_u32(&mut w, c.holes.len() as u32)?;
        for h in &c.holes {
            write_u32(&mut w, h.ref_no)?;
            write_u32(&mut w, h.par.map(|p| p + 1).unwrap_or(0))?; // 0 = none
            write_u32(&mut w, h.pts_ll.len() as u32)?;
            for &(lat, lon) in &h.pts_ll {
                write_f64(&mut w, lat)?;
                write_f64(&mut w, lon)?;
            }
        }
    }
    w.flush()
}

pub fn load(path: &Path) -> io::Result<Atlas> {
    let mut r = BufReader::new(File::open(path)?);
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(bad("not an atlas.bin (bad magic)"));
    }
    let n_courses = read_u32(&mut r)? as usize;
    let n2 = GRID_N * GRID_N;
    let mut courses = Vec::with_capacity(n_courses);
    for _ in 0..n_courses {
        let key = read_str(&mut r)?;
        let label = read_str(&mut r)?;
        let arch = read_str(&mut r)?;
        let mut g = [0u8; 1];
        r.read_exact(&mut g)?;
        let group = StyleGroup::from_code(g[0]).ok_or_else(|| bad("bad style group"))?;
        let wm = read_f64(&mut r)?;
        let hm = read_f64(&mut r)?;
        let emin = read_f64(&mut r)?;
        let emax = read_f64(&mut r)?;
        let waterpct = read_f64(&mut r)? as f32;
        let treepct = read_f64(&mut r)? as f32;
        let mut bbox = [0f64; 4];
        for v in &mut bbox {
            *v = read_f64(&mut r)?;
        }
        let mut ebytes = vec![0u8; 2 * n2];
        r.read_exact(&mut ebytes)?;
        let mut heights = Vec::with_capacity(n2);
        for i in 0..n2 {
            let v = u16::from_le_bytes([ebytes[2 * i], ebytes[2 * i + 1]]);
            heights.push((emin + v as f64 / 10.0) as f32);
        }
        let water = read_mask(&mut r, n2)?;
        let trees = read_mask(&mut r, n2)?;
        let n_holes = read_u32(&mut r)? as usize;
        let mut holes = Vec::with_capacity(n_holes);
        for _ in 0..n_holes {
            let ref_no = read_u32(&mut r)?;
            let par_raw = read_u32(&mut r)?;
            let n_pts = read_u32(&mut r)? as usize;
            let mut pts = Vec::with_capacity(n_pts);
            for _ in 0..n_pts {
                let lat = read_f64(&mut r)?;
                let lon = read_f64(&mut r)?;
                pts.push((lat, lon));
            }
            holes.push(Hole {
                ref_no,
                par: if par_raw == 0 { None } else { Some(par_raw - 1) },
                pts_ll: pts,
            });
        }
        courses.push(Course {
            key,
            label,
            arch,
            group,
            wm,
            hm,
            emin,
            emax,
            heights,
            water,
            trees,
            waterpct,
            treepct,
            bbox,
            holes,
        });
    }
    let fingerprint = fingerprint(&courses);
    Ok(Atlas {
        courses,
        fingerprint,
    })
}

/// FNV-1a over course keys + their exact u16 elevation streams. Identical for
/// an atlas whether it came from the HTML or the packed bin.
pub fn fingerprint(courses: &[Course]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    let mut eat = |b: u8| {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    };
    for c in courses {
        for &b in c.key.as_bytes() {
            eat(b);
        }
        for i in 0..GRID_N * GRID_N {
            let [a, b] = decimeters(c, i).to_le_bytes();
            eat(a);
            eat(b);
        }
    }
    h
}

/// A cell's elevation as stored: u16 decimeters above `emin`.
fn decimeters(c: &Course, i: usize) -> u16 {
    ((c.heights[i] as f64 - c.emin) * 10.0).round().clamp(0.0, 65535.0) as u16
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

/// Write a 0/1 mask packed MSB-first (8 cells per byte).
fn write_mask<W: Write>(w: &mut W, mask: &[u8]) -> io::Result<()> {
    let mut byte = 0u8;
    for (i, &m) in mask.iter().enumerate() {
        byte = (byte << 1) | (m & 1);
        if i % 8 == 7 {
            w.write_all(&[byte])?;
            byte = 0;
        }
    }
    Ok(())
}

/// Read an `n2`-cell mask packed MSB-first.
fn read_mask<R: Read>(r: &mut R, n2: usize) -> io::Result<Vec<u8>> {
    let mut bytes = vec![0u8; n2 / 8];
    r.read_exact(&mut bytes)?;
    let mut mask = vec![0u8; n2];
    for (i, &b) in bytes.iter().enumerate() {
        for k in 0..8 {
            mask[i * 8 + k] = (b >> (7 - k)) & 1;
        }
    }
    Ok(mask)
}

fn write_u16<W: Write>(w: &mut W, v: u16) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}
fn write_u32<W: Write>(w: &mut W, v: u32) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}
fn write_f64<W: Write>(w: &mut W, v: f64) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}
fn write_str<W: Write>(w: &mut W, s: &str) -> io::Result<()> {
    let b = s.as_bytes();
    write_u32(w, b.len() as u32)?;
    w.write_all(b)
}

fn read_u32<R: Read>(r: &mut R) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}
fn read_f64<R: Read>(r: &mut R) -> io::Result<f64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(f64::from_le_bytes(b))
}
fn read_str<R: Read>(r: &mut R) -> io::Result<String> {
    let len = read_u32(r)? as usize;
    if len > 1 << 20 {
        return Err(bad("string too long"));
    }
    let mut b = vec![0u8; len];
    r.read_exact(&mut b)?;
    String::from_utf8(b).map_err(|_| bad("bad utf8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_course() -> Course {
        let n2 = GRID_N * GRID_N;
        let heights: Vec<f32> = (0..n2).map(|i| 100.0 + (i % 977) as f32 / 10.0).collect();
        let water: Vec<u8> = (0..n2).map(|i| (i % 13 == 0) as u8).collect();
        let trees: Vec<u8> = (0..n2).map(|i| (i % 7 == 0) as u8).collect();
        Course {
            key: "k".into(),
            label: "L".into(),
            arch: "A, 1900".into(),
            group: StyleGroup::Mountain,
            wm: 1500.0,
            hm: 1200.0,
            emin: 100.0,
            emax: 197.6,
            heights,
            water,
            trees,
            waterpct: 1.5,
            treepct: 30.0,
            bbox: [1.0, 2.0, 3.0, 4.0],
            holes: vec![Hole {
                ref_no: 1,
                par: Some(4),
                pts_ll: vec![(40.5, -79.8), (40.6, -79.7)],
            }],
        }
    }

    #[test]
    fn roundtrip_is_lossless() {
        let atlas = Atlas {
            fingerprint: 0,
            courses: vec![tiny_course()],
        };
        let dir = std::env::temp_dir().join("golf-atlas-test");
        let path = dir.join("rt.bin");
        save(&atlas, &path).unwrap();
        let back = load(&path).unwrap();
        let (a, b) = (&atlas.courses[0], &back.courses[0]);
        assert_eq!(a.key, b.key);
        assert_eq!(a.group, b.group);
        assert_eq!(a.heights, b.heights); // exact: decimeter grid both ways
        assert_eq!(a.water, b.water);
        assert_eq!(a.trees, b.trees);
        assert_eq!(a.holes.len(), b.holes.len());
        assert_eq!(a.holes[0].par, b.holes[0].par);
        assert_eq!(a.holes[0].pts_ll, b.holes[0].pts_ll);
        assert_eq!(back.fingerprint, fingerprint(&atlas.courses));
        std::fs::remove_file(&path).ok();
    }
}
