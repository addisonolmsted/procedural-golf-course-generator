//! `CGRID1` — the binary sidecar format for big per-cell fields. JSON carries
//! structure; grids ride next to it in flat little-endian binaries so a 1501²
//! heightfield is ~9 MB instead of ~50 MB of decimal text.
//!
//! Layout: magic `b"CGRID1\0"`, version u8, dtype u8 (0=f32, 1=u32, 2=u8),
//! origin_x f64, origin_y f64, cell_size f64, nx u32, ny u32, payload
//! (nx·ny row-major values). All little-endian. f64 grids are stored as f32:
//! elevation in metres has ~0.1 mm precision at f32 — lossy but far below
//! anything the pipeline is sensitive to; golden hashes are therefore taken
//! over the in-memory f64 fields BEFORE store round-trips, never after.

use std::io::{self, Read, Write};
use std::path::Path;

use golf_core::{Grid, GridSpec, Vec2};

const MAGIC: &[u8; 7] = b"CGRID1\0";
const VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dtype {
    F32 = 0,
    U32 = 1,
    U8 = 2,
}

fn write_header(w: &mut impl Write, spec: &GridSpec, dtype: Dtype) -> io::Result<()> {
    w.write_all(MAGIC)?;
    w.write_all(&[VERSION, dtype as u8])?;
    w.write_all(&spec.origin.x.to_le_bytes())?;
    w.write_all(&spec.origin.y.to_le_bytes())?;
    w.write_all(&spec.cell_size.to_le_bytes())?;
    w.write_all(&spec.nx.to_le_bytes())?;
    w.write_all(&spec.ny.to_le_bytes())?;
    Ok(())
}

fn read_header(r: &mut impl Read, want: Dtype) -> io::Result<GridSpec> {
    let mut magic = [0u8; 7];
    r.read_exact(&mut magic)?;
    let mut vd = [0u8; 2];
    r.read_exact(&mut vd)?;
    if &magic != MAGIC || vd[0] != VERSION {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad CGRID1 header"));
    }
    if vd[1] != want as u8 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("CGRID1 dtype mismatch: file {}, expected {}", vd[1], want as u8),
        ));
    }
    let mut f = [0u8; 8];
    let mut rd_f64 = |r: &mut dyn Read| -> io::Result<f64> {
        r.read_exact(&mut f)?;
        Ok(f64::from_le_bytes(f))
    };
    let (ox, oy, cs) = (rd_f64(r)?, rd_f64(r)?, rd_f64(r)?);
    let mut u = [0u8; 4];
    r.read_exact(&mut u)?;
    let nx = u32::from_le_bytes(u);
    r.read_exact(&mut u)?;
    let ny = u32::from_le_bytes(u);
    Ok(GridSpec::new(Vec2 { x: ox, y: oy }, cs, nx, ny))
}

pub fn write_grid_f32(path: &Path, g: &Grid<f64>) -> io::Result<()> {
    let mut w = io::BufWriter::new(std::fs::File::create(path)?);
    write_header(&mut w, &g.spec, Dtype::F32)?;
    for v in &g.data {
        w.write_all(&(*v as f32).to_le_bytes())?;
    }
    Ok(())
}

pub fn read_grid_f32(path: &Path) -> io::Result<Grid<f64>> {
    let mut r = io::BufReader::new(std::fs::File::open(path)?);
    let spec = read_header(&mut r, Dtype::F32)?;
    let mut data = Vec::with_capacity(spec.len());
    let mut b = [0u8; 4];
    for _ in 0..spec.len() {
        r.read_exact(&mut b)?;
        data.push(f32::from_le_bytes(b) as f64);
    }
    Ok(Grid::from_data(spec, data))
}

pub fn write_grid_u8(path: &Path, g: &Grid<u8>) -> io::Result<()> {
    let mut w = io::BufWriter::new(std::fs::File::create(path)?);
    write_header(&mut w, &g.spec, Dtype::U8)?;
    w.write_all(&g.data)?;
    Ok(())
}

pub fn read_grid_u8(path: &Path) -> io::Result<Grid<u8>> {
    let mut r = io::BufReader::new(std::fs::File::open(path)?);
    let spec = read_header(&mut r, Dtype::U8)?;
    let mut data = vec![0u8; spec.len()];
    r.read_exact(&mut data)?;
    Ok(Grid::from_data(spec, data))
}

/// Per-cell u32 indices (e.g. the D8 receiver array) with the grid spec they
/// index into.
pub fn write_indices_u32(path: &Path, spec: &GridSpec, idx: &[u32]) -> io::Result<()> {
    assert_eq!(idx.len(), spec.len(), "index array must be one entry per cell");
    let mut w = io::BufWriter::new(std::fs::File::create(path)?);
    write_header(&mut w, spec, Dtype::U32)?;
    for v in idx {
        w.write_all(&v.to_le_bytes())?;
    }
    Ok(())
}

pub fn read_indices_u32(path: &Path) -> io::Result<(GridSpec, Vec<u32>)> {
    let mut r = io::BufReader::new(std::fs::File::open(path)?);
    let spec = read_header(&mut r, Dtype::U32)?;
    let mut data = Vec::with_capacity(spec.len());
    let mut b = [0u8; 4];
    for _ in 0..spec.len() {
        r.read_exact(&mut b)?;
        data.push(u32::from_le_bytes(b));
    }
    Ok((spec, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> GridSpec {
        GridSpec::new(Vec2 { x: 1.0, y: 2.0 }, 4.0, 3, 2)
    }

    #[test]
    fn f32_round_trip() {
        let g = Grid::from_data(spec(), vec![0.0, 1.5, -3.25, 1e4, 0.125, 7.0]);
        let dir = std::env::temp_dir().join("cgrid_test_f32");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("g.cgrid");
        write_grid_f32(&p, &g).unwrap();
        let r = read_grid_f32(&p).unwrap();
        assert_eq!(r.spec, g.spec);
        assert_eq!(r.data, g.data); // values chosen f32-exact
    }

    #[test]
    fn u8_and_u32_round_trip() {
        let g = Grid::from_data(spec(), vec![0u8, 1, 2, 3, 4, 5]);
        let dir = std::env::temp_dir().join("cgrid_test_u8");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("g.cgrid");
        write_grid_u8(&p, &g).unwrap();
        assert_eq!(read_grid_u8(&p).unwrap().data, g.data);

        let idx: Vec<u32> = (0..6).collect();
        let p2 = dir.join("i.cgrid");
        write_indices_u32(&p2, &spec(), &idx).unwrap();
        let (s2, r2) = read_indices_u32(&p2).unwrap();
        assert_eq!(s2, spec());
        assert_eq!(r2, idx);
    }

    #[test]
    fn dtype_mismatch_is_an_error() {
        let g = Grid::from_data(spec(), vec![0u8; 6]);
        let dir = std::env::temp_dir().join("cgrid_test_mismatch");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("g.cgrid");
        write_grid_u8(&p, &g).unwrap();
        assert!(read_grid_f32(&p).is_err());
    }
}
