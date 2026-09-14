//! T — texture. The sub-64 m fabric, quilted from real lidar.
//!
//! **The measured gap.** `fine/band` is the sub-64 m detail riding on the dune
//! band. Real sandhills carries 0.34-0.36; the untextured surface carries
//! 0.21-0.26. That shortfall is the "crumpled paper" character a reviewer kept
//! reporting across three rounds of dune-tier tuning, and no dune dial reaches
//! it — it sits below the dune band entirely.
//!
//! **Conditioning is wind-relative.** Patches are keyed on local slope, TPI
//! (hollow / flank / crest) and **aspect relative to the wind**, not absolute
//! aspect: a windward ramp and a lee face are different fabrics, and which
//! compass direction they happen to face is not the point. That is also why
//! this is a fresh pack rather than `dictionary_v2.bin`, which keys partly on
//! distance-to-channel — meaningless on ground with no drainage.
//!
//! **Composition is additive.** Texture is a residual: it is ADDED to the
//! surface and never replaces it, so the dune geometry the previous stages
//! measured into band cannot be moved by this one. A test pins it.

use course_seed::DetRng;
use rayon::prelude::*;
use course_world::grid::Grid;
use course_world::math;

use crate::draw::Descriptors;
use crate::wind::{macro_spec, RES_M};

/// SPACK1 — the patch pack, as built by `tools/aeolian/build_patchpack.py`.
pub struct PatchPack {
    pub patch: usize,
    n_slope: usize,
    n_tpi: usize,
    n_aspect: usize,
    slope_edges: Vec<f32>,
    tpi_edges: Vec<f32>,
    /// (start, count) per conditioning bucket.
    index: Vec<(u32, u32)>,
    /// All patches back to back, millimetres.
    data: Vec<i16>,
    scale: f32,
}

#[derive(Debug)]
pub enum PackError {
    Io(std::io::Error),
    Format(&'static str),
}

impl From<std::io::Error> for PackError {
    fn from(e: std::io::Error) -> Self {
        PackError::Io(e)
    }
}

fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn rd_f32(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

impl PatchPack {
    pub fn load(path: &std::path::Path) -> Result<Self, PackError> {
        let b = std::fs::read(path)?;
        if b.len() < 32 || &b[..7] != b"SPACK1\0" {
            return Err(PackError::Format("not a SPACK1 file"));
        }
        // version u8 | patch u16 | n_slope u8 | n_tpi u8 | n_aspect u8 | pad
        // | n_buckets u32 | n_patches u32 | scale f32
        let patch = u16::from_le_bytes([b[8], b[9]]) as usize;
        let (n_slope, n_tpi, n_aspect) = (b[10] as usize, b[11] as usize, b[12] as usize);
        let n_buckets = rd_u32(&b, 14) as usize;
        let n_patches = rd_u32(&b, 18) as usize;
        let scale = rd_f32(&b, 22);
        let mut o = 26;
        let mut slope_edges = Vec::new();
        for _ in 0..n_slope.saturating_sub(1) {
            slope_edges.push(rd_f32(&b, o));
            o += 4;
        }
        let mut tpi_edges = Vec::new();
        for _ in 0..n_tpi.saturating_sub(1) {
            tpi_edges.push(rd_f32(&b, o));
            o += 4;
        }
        let mut index = Vec::with_capacity(n_buckets);
        for _ in 0..n_buckets {
            index.push((rd_u32(&b, o), rd_u32(&b, o + 4)));
            o += 8;
        }
        let need = n_patches * patch * patch;
        if b.len() < o + need * 2 {
            return Err(PackError::Format("truncated patch data"));
        }
        let mut data = Vec::with_capacity(need);
        for i in 0..need {
            data.push(i16::from_le_bytes([b[o + i * 2], b[o + i * 2 + 1]]));
        }
        Ok(PatchPack { patch, n_slope, n_tpi, n_aspect, slope_edges, tpi_edges,
                       index, data, scale })
    }

    fn bucket(&self, slope: f64, tpi: f64, aspect_rel: f64) -> usize {
        let s = self.slope_edges.iter().filter(|e| slope >= **e as f64).count();
        let t = self.tpi_edges.iter().filter(|e| tpi >= **e as f64).count();
        let a = ((aspect_rel.rem_euclid(std::f64::consts::TAU)
            / std::f64::consts::TAU) * self.n_aspect as f64) as usize % self.n_aspect;
        ((s.min(self.n_slope - 1) * self.n_tpi + t.min(self.n_tpi - 1)) * self.n_aspect + a)
            .min(self.index.len() - 1)
    }

    /// One patch's sample, in metres. `k` selects within the bucket.
    fn sample(&self, bucket: usize, k: usize, y: usize, x: usize) -> f64 {
        let (start, count) = {
            let mut b = bucket;
            // The edge-truncation filter can empty a rare bucket (measured:
            // fluvial steep-crest bins, where Carolina's steep ground is all
            // creek bank). Fall back across the aspect ring rather than
            // pasting silence -- an untextured band is worse than an
            // aspect-mismatched patch.
            if self.index[b].1 == 0 {
                let ring = b - b % self.n_aspect;
                for a in 0..self.n_aspect {
                    if self.index[ring + a].1 > 0 {
                        b = ring + a;
                        break;
                    }
                }
            }
            self.index[b]
        };
        if count == 0 {
            return 0.0;
        }
        let p = start as usize + k % count as usize;
        self.data[(p * self.patch + y) * self.patch + x] as f64 / self.scale as f64
    }
}

/// Texture resolution: 2 m, the corpus's own grid and the resolution the
/// patches were cut at.
pub const TEX_RES_M: f64 = 2.0;

/// Quilt texture onto a macro surface, returning the 2 m textured heightfield.
///
/// Overlap-add with a raised-cosine window, which is the standard way to keep
/// patch seams out of the result. `docs/biomes/sandhills.md` open question 2
/// asks whether slip faces survive that blending; `slip_face_survives_the_quilt`
/// is the test that answers it.
pub fn quilt(rng: &mut DetRng, pack: &PatchPack, macro_h: &Grid<f64>, d: &Descriptors)
    -> Grid<f64>
{
    quilt_core(rng, pack, macro_h, d, None)
}

/// Measured texture amplitude vs position across the valley (`TPROF1`,
/// from `tools/aeolian/texture_by_position.py`).
///
/// The review asked the right question — real texture plainly differs
/// between floor, wall and ridge — and the answer is measured, not guessed:
/// the finest band runs 1.25x its tile mean on the channel down to 0.88 at
/// the divide, and the coarser bands flatten out (1.11 -> 0.99). So fine
/// detail concentrates near water while the broad fabric barely cares. The
/// gate this replaces was a linear ramp to 0.45, both too strong and the
/// same for every band.
pub struct TexProfile {
    u_bins: usize,
    /// `g[band][u]` — gain against that band's own tile mean.
    g: Vec<Vec<f64>>,
}

impl TexProfile {
    pub fn load(path: &std::path::Path) -> std::io::Result<TexProfile> {
        let txt = std::fs::read_to_string(path)?;
        let mut u_bins = 12usize;
        let mut g = Vec::new();
        for line in txt.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('#') || l == "TPROF1" {
                continue;
            }
            let mut it = l.split_whitespace();
            match it.next() {
                Some("u_bins") => u_bins = it.next().unwrap().parse().unwrap(),
                Some("g") => g.push(it.map(|v| v.parse().unwrap()).collect()),
                _ => {}
            }
        }
        Ok(TexProfile { u_bins, g })
    }

    /// Gain for a band at valley position `u`, linear between bins.
    fn gain(&self, band: usize, u: f64) -> f64 {
        let row = &self.g[band.min(self.g.len() - 1)];
        let x = (u.clamp(0.0, 1.0) * (self.u_bins - 1) as f64).min((self.u_bins - 1) as f64);
        let i = (x as usize).min(self.u_bins - 2);
        let f = x - i as f64;
        row[i] * (1.0 - f) + row[i + 1] * f
    }
}

/// The fluvial quilt (review 2026-08-26: "texture shows grain along the
/// tributaries and trunk based on proximity"). Two departures from the
/// aeolian quilt, both carried by `fields`:
///   * patches are conditioned on aspect relative to the LOCAL CHANNEL
///     TANGENT (the pack was rebuilt the same way — each real patch keyed
///     against its own tile's drainage grain), so the pasted fabric runs
///     along the valleys;
///   * the position gate is CHANNEL PROXIMITY, not megaform height: full
///     fabric in and near the valleys, easing to `texture_floor` on the
///     open interfluves.
pub fn quilt_fluvial(rng: &mut DetRng, pack: &PatchPack, macro_h: &Grid<f64>,
                     grain: &Grid<f64>, prox: &Grid<f64>, d: &Descriptors)
    -> Grid<f64>
{
    quilt_core(rng, pack, macro_h, d, Some((grain, prox)))
}

/// The fluvial quilt with the MEASURED position profile and the roughness
/// variation restored.
///
/// Overlap-add averages four patches over every cell, and averaging
/// exemplars is what turns pasted lidar into uniform fuzz: measured, real
/// tiles' local roughness varies 55% across a tile while ours varied 20%
/// (0.37x). Amplitude and spectrum were both already right — what was
/// missing was that roughness is PATCHY. Two corrections, in this order:
/// the measured per-band gain against valley position, then a two-octave
/// supply field (the `belt_patchiness` idiom from surface.rs) calibrated so
/// the variation statistic matches the corpus.
/// Texture amplitude as a function of RELIEF POSITION, measured on the 65 NC
/// tiles by `tools/aeolian/texture_by_relief.py`.
///
/// This is a different axis from `TexProfile`'s `u`. `u` is planform — how
/// far a point sits between its channel and its divide — and a point can be
/// far from any channel and still be LOW, out on a broad flat interfluve.
/// Review 2026-08-26 read the difference as one of height, and the corpus
/// agrees emphatically: the bottom third of the relief carries 1.09-1.15x a
/// tile's mean band RMS and the top third 0.81-0.89x, in every band from
/// 2-4 m to 32-64 m. Our own tiles measured almost FLAT above the lowest
/// bin (0.92-1.00), which is why the high ground did not read as calmer.
///
/// The curve is not monotone at the bottom: bin 0 sits below bin 1 in the
/// coarser bands, because the very lowest ground is the alluvial valley
/// FLOOR, which is flat. The lower slope just above it is the roughest
/// ground on the tile. Keeping the whole curve rather than two endpoints is
/// what preserves that.
pub struct ReliefProfile {
    bins: usize,
    /// `g[band][bin]` — gain against that band's own tile mean.
    g: Vec<Vec<f64>>,
}

impl ReliefProfile {
    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        let txt = std::fs::read_to_string(path)?;
        let mut lines = txt.lines();
        let tag = lines.next().unwrap_or("").trim().to_string();
        if tag != "RPROF1" {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                                           "not an RPROF1 table"));
        }
        let mut hdr = lines.next().unwrap_or("").split_whitespace();
        let nband: usize = hdr.next().unwrap_or("0").parse().unwrap_or(0);
        let bins: usize = hdr.next().unwrap_or("0").parse().unwrap_or(0);
        let _ = lines.next();                       // band labels
        let mut g = Vec::with_capacity(nband);
        for _ in 0..nband {
            let row: Vec<f64> = lines.next().unwrap_or("").split_whitespace()
                .filter_map(|v| v.parse().ok()).collect();
            g.push(row);
        }
        Ok(ReliefProfile { bins, g })
    }

    /// Gain for `band` at normalised relief `h` in [0,1], linear between bin
    /// centres and flat outside them.
    pub fn gain(&self, band: usize, h: f64) -> f64 {
        let row = match self.g.get(band) {
            Some(r) if !r.is_empty() => r,
            _ => return 1.0,
        };
        let t = (h.clamp(0.0, 1.0) * self.bins as f64 - 0.5).clamp(0.0, (self.bins - 1) as f64);
        let i = t.floor() as usize;
        let f = t - i as f64;
        let a = row[i.min(row.len() - 1)];
        let b = row[(i + 1).min(row.len() - 1)];
        a + (b - a) * f
    }
}

/// Review 2026-08-26 asked for the relief effect to be pushed BEYOND what the
/// corpus shows — less amplitude on the high ground, and the transition moved
/// DOWN so more of the tile reads as calm upland and the roughness is confined
/// to the low ground.
///
/// These two are therefore deliberately NOT measured values, unlike the RPROF1
/// table they shape, which stays a faithful record of the corpus. Keeping the
/// exaggeration separate from the measurement is the point: the table can be
/// re-measured without disturbing the taste call, and the taste call can be
/// dialled without corrupting the data.
///
/// * `SHIFT < 1` remaps relief so the high-ground gains arrive at lower
///   ground — that is the "transition to lower reliefs".
/// * `STRENGTH > 1` exaggerates the curve's departure from flat.
const RELIEF_SHIFT: f64 = 0.60;
const RELIEF_STRENGTH: f64 = 1.90;

/// The shaped relief gain, BEFORE normalisation.
fn relief_raw(rp: &ReliefProfile, band: usize, h: f64) -> f64 {
    rp.gain(band, h.clamp(0.0, 1.0).powf(RELIEF_SHIFT)).powf(RELIEF_STRENGTH)
}

pub fn quilt_fluvial_v2(rng: &mut DetRng, pack: &PatchPack, macro_h: &Grid<f64>,
                        grain: &Grid<f64>, uf: &Grid<f64>, prof: &TexProfile,
                        rprof: Option<&ReliefProfile>, d: &Descriptors) -> Grid<f64>
{
    let base = macro_h;
    let unit = Grid::filled(uf.spec, 1.0f64);
    let timing = std::env::var("SAND_TIME").is_ok();
    let mut t0 = std::time::Instant::now();
    let lap = |tag: &str, t0: &mut std::time::Instant| {
        if timing {
            eprintln!("    time   quilt/{tag:9} {:7.1} ms", t0.elapsed().as_secs_f64() * 1e3);
            *t0 = std::time::Instant::now();
        }
    };
    let out = quilt_core(rng, pack, base, d, Some((grain, &unit)));
    let spec = out.spec;
    lap("core", &mut t0);

    // the macro at 2 m, once: the same Catmull-Rom sample `quilt_core` took
    // (this was evaluated three times over the grid, 2/3 of the fluvial
    // quilt's 670 ms -- measured with SAND_TIME, 2026-09-14)
    let nxu = spec.nx as usize;
    let mut base2 = vec![0.0f64; spec.len()];
    base2.par_chunks_mut(nxu).enumerate().for_each(|(y, row)| {
        for (x, v) in row.iter_mut().enumerate() {
            *v = catmull_rom_2d(base, spec.world_of(x as u32, y as u32));
        }
    });
    // the pasted residual, split into a fine and a coarse half
    let mut resid = vec![0.0f64; spec.len()];
    for i in 0..spec.len() {
        resid[i] = out.data[i] - base2[i];
    }
    let mut coarse = resid.clone();
    // ~12 m split. A separable BOX (13 cells, two passes) did this and its
    // Dirichlet nulls at 26/k m sit on the axes -- the same kernel family
    // that once cut the "boxy" creek. A gaussian of the same sigma
    // (sqrt((13^2-1)/12) per pass, two passes: 5.3 cells) is isotropic.
    lap("catmull", &mut t0);
    gauss_blur(spec, &mut coarse, 5.3);
    lap("gauss", &mut t0);

    // --- normalised relief, from the MACRO surface --------------------
    // Taken from `base`, not from the textured field, so the texture cannot
    // feed back into its own conditioning. 2nd-98th percentile, matching how
    // the corpus curve was measured.
    let mut hrel = vec![0.5f64; spec.len()];
    if rprof.is_some() {
        // the two percentiles by selection, not a full sort of 2.25 M cells
        // (that sort was ~300 ms of the fluvial quilt; same values exactly)
        let mut srt = base2.clone();
        let n = srt.len();
        let (klo, khi) = ((n as f64 * 0.02) as usize, (n as f64 * 0.98) as usize);
        let lo = *srt.select_nth_unstable_by(klo, |a, b| a.partial_cmp(b).unwrap()).1;
        let hi = *srt.select_nth_unstable_by(khi, |a, b| a.partial_cmp(b).unwrap()).1;
        let span = (hi - lo).max(1e-6);
        for i in 0..spec.len() {
            hrel[i] = ((base2[i] - lo) / span).clamp(0.0, 1.0);
        }
    }

    lap("hrel", &mut t0);
    // --- normalise the relief curve ON THE LOW THIRD ------------------
    // Not on the tile mean: a mean-preserving rescale only moves energy from
    // the tops into the valleys, so the high ground never actually loses any
    // (review 2026-08-26 asked for it to). Not on a single anchor point
    // either — that was tried and it dropped the low ground by 45% as well,
    // because removing the mean-preserving rescale removed a compensating
    // boost with it. Dividing by the curve's own mean over the low third
    // pins that third at exactly the amplitude it had before any relief
    // conditioning, and lets everything above it fall away.
    let (mut nf, mut nc) = (1.0f64, 1.0f64);
    if let Some(rp) = rprof {
        let (mut sf, mut sc, mut cnt) = (0.0, 0.0, 0usize);
        for i in 0..spec.len() {
            if hrel[i] < 1.0 / 3.0 {
                sf += relief_raw(rp, 1, hrel[i]);
                sc += relief_raw(rp, 3, hrel[i]);
                cnt += 1;
            }
        }
        if cnt > 0 {
            nf = (sf / cnt as f64).max(1e-3);
            nc = (sc / cnt as f64).max(1e-3);
        }
    }

    let (sp1, sp2) = (rng.next_u32(), rng.next_u32());
    let mut z = Grid::filled(spec, 0.0f64);
    let mut inc = vec![0.0f64; spec.len()];
    // `uf` is the 2 m valley coordinate on this same grid (lib.rs builds it
    // on `tex.spec`); a bilinear read at its own node is the node
    let same_spec = uf.spec.nx == spec.nx && uf.spec.ny == spec.ny
        && uf.spec.cell_size == spec.cell_size
        && uf.spec.origin.x == spec.origin.x && uf.spec.origin.y == spec.origin.y;
    let (resid_r, coarse_r, hrel_r) = (&resid, &coarse, &hrel);
    inc.par_chunks_mut(nxu).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let i = y * nxu + x;
            let p = spec.world_of(x as u32, y as u32);
            let u = if same_spec { uf.data[i] } else { uf.bilinear(p) };
            // measured: band 0 is the finest, band 2 the broad fabric
            let gf = prof.gain(0, u);
            // MEASURED 2026-08-25 against the 65-tile NC corpus at 2 m:
            // band 4-8 m sat at 0.98x of the real median and 128-256 m at
            // 0.97x, but everything between ran 1.14-1.23x, peaking at
            // 16-32 m — the "graininess" of the review. The excess is
            // therefore in the COARSE half of the quilt, not the fine one,
            // so the trim goes here and the fine gain is left alone.
            let gc = prof.gain(2, u) * 0.82;
            // roughness is patchy — the statistic the blending destroyed
            // the 190 m octave is the one that reads as patterned grain on
            // the high ground (measured: tops carried 1.37x real in the
            // 150-250 m band), so it is thinned there
            let top_m = math::smoothstep(0.70, 0.96, u);
            let sup = 1.0
                + d.tex_patchy
                    * (course_world::noise::perlin2(p.x / 520.0, p.y / 520.0, sp1)
                        + 0.70 * (1.0 - 0.60 * top_m)
                            * course_world::noise::perlin2(
                                p.x / 190.0, p.y / 190.0, sp2)
                        + 0.40 * course_world::noise::perlin2(
                            p.x / 80.0, p.y / 80.0, sp1.wrapping_add(7)));
            let sup = sup.max(0.12);
            // The measured profile already thins the fabric toward the
            // divide; review 2026-08-25 asked for the high ground calmer
            // still, and the zone measurement had room for it (our tops sat
            // at the top of the real range, our floors at the bottom).
            let top = math::smoothstep(0.72, 0.97, u);
            let calm = 1.0 - 0.45 * top;
            // Relief conditioning, applied SEPARATELY to the two halves so
            // it changes character and not only amplitude: the corpus shows
            // the low ground holding relatively more fine energy (its
            // fine/coarse ratio is 1.24 against 0.95 up high), which is the
            // "bumps and pocks" of the review, while the high ground keeps
            // its broad swells and loses the grain.
            let (rf, rc) = match rprof {
                Some(rp) => (relief_raw(rp, 1, hrel_r[i]) / nf,
                             relief_raw(rp, 3, hrel_r[i]) / nc),
                None => (1.0, 1.0),
            };
            let fine = resid_r[i] - coarse_r[i];
            *o = calm * sup * (gf * rf * fine + gc * rc * coarse_r[i]);
        }
    });

    lap("gains", &mut t0);
    // --- SOFT-CLAMP the pasted increment -------------------------------
    // The quilt pastes 2 m residual cut from real NC tiles, and those tiles
    // carry man-made cuts — road prisms, borrow pits, unmasked pond edges
    // (a known Stage-0 corpus gap). Pasted onto our ground they print as
    // isolated 4-5 m gouges with 75-90% walls in terrain the macro stage
    // left at 0-3%: measured at seed 121, (764,2172) and (2442,1248), where
    // the macro surface is smooth and only the textured one has the pit.
    // These are ~16 sigma against a band std of 0.26 m, so a soft knee at
    // 3.5 sigma leaves ordinary fabric untouched (tanh is within 1% of the
    // identity below ~1 sigma) and squashes only the imported scars. A hard
    // clip would replace one sharp edge with another; tanh cannot.
    // No RMS renormalisation here. An earlier version rescaled the increment
    // to the RMS it would have had without the relief curve, so that only the
    // DISTRIBUTION moved; review 2026-08-26 asked for the high ground to lose
    // amplitude outright, which a mean-preserving rescale cannot deliver. The
    // anchor in `relief_gain` holds the low ground instead, so the tile
    // amplitude falls — a deliberate departure from the band calibration.
    let mut acc = 0.0;
    for v in inc.iter() {
        acc += v * v;
    }
    let sigma = (acc / inc.len() as f64).sqrt().max(1e-6);
    let knee = 3.5 * sigma;
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let i = spec.index(x, y);
            // Catmull-Rom, the same spline `quilt_core` upsampled through.
            // This was `base.bilinear`: the C0 8 m node lines printed as a
            // crosshatch on every fluvial tile (measured 2026-09-14, axis
            // power 2x the diagonal at 8 m, the bilinear phase signature),
            // and the residual above carried the Catmull-minus-bilinear
            // lattice and re-amplified it.
            z.data[i] = base2[i] + knee * (inc[i] / knee).tanh();
        }
    }
    lap("clamp", &mut t0);
    z
}

/// [1 2 1]/4 along x then y; edges clamp. A null at the 2-cell period.
fn binomial3(spec: course_world::grid::GridSpec, a: &mut Vec<f64>) {
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let nxu = spec.nx as usize;
    let src = a.clone();
    a.par_chunks_mut(nxu).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let x = x as i64;
            let at = |xx: i64| src[spec.index(xx.clamp(0, nx - 1) as u32, y as u32)];
            *o = 0.25 * at(x - 1) + 0.5 * at(x) + 0.25 * at(x + 1);
        }
    });
    let src = a.clone();
    a.par_chunks_mut(nxu).enumerate().for_each(|(y, row)| {
        let y = y as i64;
        for (x, o) in row.iter_mut().enumerate() {
            let at = |yy: i64| src[spec.index(x as u32, yy.clamp(0, ny - 1) as u32)];
            *o = 0.25 * at(y - 1) + 0.5 * at(y) + 0.25 * at(y + 1);
        }
    });
}

/// Separable gaussian, `sigma` in cells, kernel to 3 sigma; edges clamp.
fn gauss_blur(spec: course_world::grid::GridSpec, a: &mut Vec<f64>, sigma: f64) {
    let r = (3.0 * sigma).ceil() as i64;
    let k: Vec<f64> = (-r..=r).map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp()).collect();
    let ks: f64 = k.iter().sum();
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let nxu = spec.nx as usize;
    let src = a.clone();
    let k_r = &k;
    a.par_chunks_mut(nxu).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let mut s = 0.0;
            for (j, kk) in k_r.iter().enumerate() {
                let xx = (x as i64 + j as i64 - r).clamp(0, nx - 1) as u32;
                s += kk * src[spec.index(xx, y as u32)];
            }
            *o = s / ks;
        }
    });
    let src = a.clone();
    a.par_chunks_mut(nxu).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let mut s = 0.0;
            for (j, kk) in k_r.iter().enumerate() {
                let yy = (y as i64 + j as i64 - r).clamp(0, ny - 1) as u32;
                s += kk * src[spec.index(x as u32, yy)];
            }
            *o = s / ks;
        }
    });
}

/// Catmull-Rom (bicubic, C1) sample of a grid at a world point; edges clamp.
fn catmull_rom_2d(g: &Grid<f64>, world: math::Vec2) -> f64 {
    let s = &g.spec;
    let gx = ((world.x - s.origin.x) / s.cell_size).clamp(0.0, (s.nx - 1) as f64);
    let gy = ((world.y - s.origin.y) / s.cell_size).clamp(0.0, (s.ny - 1) as f64);
    let (x0, y0) = (gx.floor() as i64, gy.floor() as i64);
    let (tx, ty) = (gx - x0 as f64, gy - y0 as f64);
    let w = |t: f64| -> [f64; 4] {
        let t2 = t * t;
        let t3 = t2 * t;
        [-0.5 * t3 + t2 - 0.5 * t,
         1.5 * t3 - 2.5 * t2 + 1.0,
         -1.5 * t3 + 2.0 * t2 + 0.5 * t,
         0.5 * t3 - 0.5 * t2]
    };
    let (wx, wy) = (w(tx), w(ty));
    let at = |a: i64, b: i64| -> f64 {
        *g.get(a.clamp(0, s.nx as i64 - 1) as u32, b.clamp(0, s.ny as i64 - 1) as u32)
    };
    let mut v = 0.0;
    for j in 0..4 {
        let mut row = 0.0;
        for i in 0..4 {
            row += wx[i] * at(x0 - 1 + i as i64, y0 - 1 + j as i64);
        }
        v += wy[j] * row;
    }
    v
}

fn quilt_core(rng: &mut DetRng, pack: &PatchPack, macro_h: &Grid<f64>,
              d: &Descriptors, fields: Option<(&Grid<f64>, &Grid<f64>)>)
    -> Grid<f64>
{
    let mspec = macro_spec();
    let n = (course_world::world::EXTENT_M / TEX_RES_M).round() as u32 + 1;
    let spec = course_world::grid::GridSpec::new(
        course_world::math::Vec2::ZERO, TEX_RES_M, n, n);

    // Upsample the macro to 2 m. Bilinear here would print a gradient jump on
    // every 8 m node -- the exact defect heartland measured at a 749x curvature
    // spike -- so this goes through the grid's bilinear ONCE and is then
    // smoothed, rather than being sampled per texture cell from a coarse grid.
    // The macro comes up to 2 m through a Catmull-Rom spline, not bilinear.
    // Bilinear is C0: on a steep wall the 8 m nodes print as facets, and
    // the mixed-run screen's "step" class (a 0.8 m departure from the 3x3
    // mean) sat on those walls on 155/155 aeolian tiles -- the real tiles
    // have none. Catmull-Rom is C1 through the same nodes, so the walls
    // are smooth between them and nothing else moves by more than the
    // node spacing's curvature allows.
    let mut out = Grid::filled(spec, 0.0f64);
    let nxu = spec.nx as usize;
    out.data.par_chunks_mut(nxu).enumerate().for_each(|(y, row)| {
        for (x, v) in row.iter_mut().enumerate() {
            *v = catmull_rom_2d(macro_h, spec.world_of(x as u32, y as u32));
        }
    });

    // Conditioning fields, computed on the macro at ITS own resolution.
    let (mut slope, mut tpi, mut aspect) =
        (vec![0.0f64; mspec.len()], vec![0.0f64; mspec.len()], vec![0.0f64; mspec.len()]);
    let lp = lowpass(macro_h, 3);
    // Megaform position for the floor gate: percentile-normalised height, the
    // same axis the hummock gate keys on. Texture fades toward
    // `texture_floor` in the interdune lows -- real train corridors are
    // deflation surfaces and carry 0.277x the belt's fine texture.
    let mut mpos = vec![0.0f64; mspec.len()];
    {
        let mut sorted: Vec<f64> = macro_h.data.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = sorted.len();
        for i in 0..mspec.len() {
            let z = macro_h.data[i];
            let r = sorted.partition_point(|v| *v < z);
            mpos[i] = r as f64 / n as f64;
        }
    }
    for y in 1..mspec.ny - 1 {
        for x in 1..mspec.nx - 1 {
            let i = mspec.index(x, y);
            let gx = (macro_h.get(x + 1, y) - macro_h.get(x - 1, y)) / (2.0 * RES_M);
            let gy = (macro_h.get(x, y + 1) - macro_h.get(x, y - 1)) / (2.0 * RES_M);
            slope[i] = (gx * gx + gy * gy).sqrt();
            aspect[i] = math::atan2(gy, gx);
            tpi[i] = macro_h.get(x, y) - lp[i];
        }
    }

    let p = pack.patch;
    let step = p / 2;
    let win = window(p);
    let mut acc = vec![0.0f64; spec.len()];
    let mut wsq = vec![0.0f64; spec.len()];
    let k0 = rng.next_u32() as usize;
    // The lattice phase. Patches were laid on a `step` grid anchored at world
    // (0, 0) on every seed, so the pitch was world-locked across the whole
    // corpus of tiles; the phase now comes off the draw already taken (no
    // new draw), one offset per seed. The pitch itself is dealt with below.
    let (dx, dy) = ((k0 % step) as i64, ((k0 / step) % step) as i64);

    let mut tile = 0usize;
    let mut y0 = -dy;
    while y0 < spec.ny as i64 {
        let mut x0 = -dx;
        while x0 < spec.nx as i64 {
            // Condition on the macro at this patch's centre.
            let wc = spec.world_of((x0 + step as i64).clamp(0, spec.nx as i64 - 1) as u32,
                                   (y0 + step as i64).clamp(0, spec.ny as i64 - 1) as u32);
            let mx = ((wc.x / RES_M).round() as i64).clamp(0, mspec.nx as i64 - 1) as u32;
            let my = ((wc.y / RES_M).round() as i64).clamp(0, mspec.ny as i64 - 1) as u32;
            let mi = mspec.index(mx, my);
            let refd = match fields {
                Some((grain, _)) => grain.data[mi],
                None => d.wind_rad,
            };
            let b = pack.bucket(slope[mi], tpi[mi], aspect[mi] - refd);
            // NOT a min-error patch selection. Image quilting's
            // best-match rule was tried here to restore v1's exemplar
            // character (review 2026-08-25) and FAILED on this material:
            // choosing the candidate that agrees with its neighbours makes
            // neighbouring patches self-similar, and their shared elongated
            // features chain up — the tile came back covered in worm-like
            // ridges, far worse than the fuzz it was meant to cure. Carolina
            // wall fabric is directional enough that "most agreeable" and
            // "most repetitive" are the same patch. Pseudo-random selection
            // it is; the character has to come from the LANDFORM instead.
            // A hash of (draw, tile), not an arithmetic progression: the
            // old `k0 + tile * 2654435761` stepped k by a constant modulo
            // the bucket's count, and in a thin bucket that put the SAME
            // patch under neighbouring windows, which are 50 % overlapped --
            // correlated, so the overlap-add's RMS rippled at the pitch
            // however it was normalised (measured 2026-09-14: 1.13 left of
            // the 1.30 after the normalisation below; independent patches
            // give 1.01).
            let k = {
                let mut h = (k0 as u64) ^ (tile as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                h ^= h >> 31;
                h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
                h ^= h >> 29;
                (h >> 16) as usize
            };
            // The patch's own mean comes off. The pack is a 64 m high-pass of
            // the TILE it was cut from, not of each patch, so every 96 m patch
            // carries a DC of its own -- and DC laid on the lattice prints the
            // lattice.
            let mut mean = 0.0;
            for yy in 0..p {
                for xx in 0..p {
                    mean += pack.sample(b, k, yy, xx);
                }
            }
            mean /= (p * p) as f64;
            for yy in 0..p {
                let gy = y0 + yy as i64;
                if gy < 0 || gy >= spec.ny as i64 {
                    continue;
                }
                for xx in 0..p {
                    let gx = x0 + xx as i64;
                    if gx < 0 || gx >= spec.nx as i64 {
                        continue;
                    }
                    let w = win[yy] * win[xx];
                    let gi = spec.index(gx as u32, gy as u32);
                    acc[gi] += w * (pack.sample(b, k, yy, xx) - mean);
                    wsq[gi] += w * w;
                }
            }
            tile += 1;
            x0 += step as i64;
        }
        y0 += step as i64;
    }

    // The gate is read BILINEARLY off the 8 m macro. Read nearest, it was a
    // staircase with 8 m treads in x and y, and since it multiplies the
    // residual by a 3-6x swing every tread printed as a crosshatch: measured
    // 2026-09-14, axis-aligned spectral power 3.5-5x the diagonal at 8 m on
    // aeolian tiles, sitting on the two cells straddling each block edge and
    // gone wherever the gate saturates. Real lidar measures 1.0.
    let mpos_g = Grid { spec: mspec, data: mpos };
    // The residual, RMS-normalised (see below), then a 3-tap binomial in x
    // and in y. The packs are cut from 3DEP tiles decimated 1 m -> 2 m by
    // bilinear resampling with no prefilter, and that gridding leaves an
    // axis-aligned lattice at the 2-cell period in the SOURCE (real NC and
    // NE tiles measure 1.5x the diagonal at 4-4.5 m; the fluvial fine gain
    // then took ours to 2.4x). [1 2 1]/4 has a null at the 2-cell period
    // and costs 4 % of the 2-16 m band RMS (measured).
    // `OVERLAP_ADD_RMS`: the Hann overlap-add's RMS, averaged over the
    // lattice phase, was 0.76 of one patch's (mean of sqrt(w^2 + (1-w)^2)
    // squared for the x*y product), and `texture_gain` was calibrated to
    // the corpus band ON TOP of that loss. Normalising per cell removes
    // the ripple; the same mean loss is applied as a constant so the band
    // calibration holds (measured without it: fine 2-16 m RMS +17-19 %).
    const OVERLAP_ADD_RMS: f64 = 0.76;
    let mut res = vec![0.0f64; spec.len()];
    res.par_iter_mut().enumerate().for_each(|(i, v)| {
        if wsq[i] > 1e-12 {
            *v = OVERLAP_ADD_RMS * acc[i] / wsq[i].sqrt();
        }
    });
    binomial3(spec, &mut res);
    let (res_r, wsq_r, mpos_r) = (&res, &wsq, &mpos_g);
    out.data.par_chunks_mut(nxu).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let i = y * nxu + x;
            if wsq_r[i] <= 1e-12 {
                continue;
            }
            let w = spec.world_of(x as u32, y as u32);
            let g = match fields {
                // proximity gate: prox is 1 in the valley zone, 0 far out
                Some((_, prox)) => d.texture_floor
                    + (1.0 - d.texture_floor) * prox.bilinear(w).clamp(0.0, 1.0),
                None => d.texture_floor
                    + (1.0 - d.texture_floor)
                        * math::smoothstep(0.30, 0.62, mpos_r.bilinear(w)),
            };
            // Normalised by sqrt(sum w^2), not sum w: the patches are
            // independent samples, so the overlap-add's RMS goes as the root
            // of the squared weights. A Hann pair sums to 1 (the mean is
            // flat) but its squares swing 0.5..1 -- 0.25..1 as the x*y
            // product -- and that printed the 48 m pitch as a checker on
            // every tile (measured 2026-09-14: the RMS profile by cell mod 24
            // matched sqrt(w^2 + (1-w)^2) at r = 0.86-0.97, both modes).
            *o += d.texture_gain * g * res_r[i];
        }
    });
    out
}

/// Raised cosine (Hann). Sums to a constant at half-overlap, so the MEAN of
/// the overlap-add carries no ripple at the patch pitch; the RMS does (the
/// squares sum to 0.5..1), which `quilt_core` normalises out.
fn window(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| {
            let t = (i as f64 + 0.5) / n as f64;
            0.5 - 0.5 * math::cos(std::f64::consts::TAU * t)
        })
        .collect()
}

fn lowpass(g: &Grid<f64>, passes: usize) -> Vec<f64> {
    let spec = g.spec;
    let mut v = g.data.clone();
    let mut t = v.clone();
    for _ in 0..passes {
        for y in 1..spec.ny - 1 {
            for x in 1..spec.nx - 1 {
                let i = spec.index(x, y);
                t[i] = (v[i] * 4.0
                    + v[spec.index(x + 1, y)] + v[spec.index(x - 1, y)]
                    + v[spec.index(x, y + 1)] + v[spec.index(x, y - 1)]) / 8.0;
            }
        }
        std::mem::swap(&mut v, &mut t);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{draw, record::FormClass, rng, surface, wind, Mode};
    use course_seed::RunIdentity;

    fn pack() -> Option<PatchPack> {
        PatchPack::load(std::path::Path::new("../../assets/sandhills_patches_aeolian.bin"))
            .ok()
            .or_else(|| PatchPack::load(std::path::Path::new("assets/sandhills_patches_aeolian.bin")).ok())
    }

    fn build(seed: u64) -> Option<(Grid<f64>, Grid<f64>, Descriptors)> {
        let p = pack()?;
        let id = RunIdentity::from_seed(seed);
        let d = draw::site(&id, Some(Mode::Aeolian), Some(FormClass::Train));
        let mut r = rng::stream(&id, rng::WIND);
        let f = wind::build(&mut r, d.wind_rad, d.wavelength_m, d.wind_wander_rad,
                            d.wind_wander_m, d.kappa, 0.0);
        let mut hr = rng::stream(&id, rng::HUMMOCK);
        let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m, d.wind_wander_rad,
                             d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
        let mut br = rng::stream(&id, rng::PATCHY);
        let sf = surface::build(&mut br, &f, &hw, &d);
        let mut tr = rng::stream(&id, rng::TEXTURE);
        let tex = quilt(&mut tr, &p, &sf.height, &d);
        Some((sf.height, tex, d))
    }

    #[test]
    fn texture_is_additive_and_leaves_the_dune_band_alone() {
        // Texture is a RESIDUAL. The dune geometry the previous stages were
        // measured into band -- spacing, count, footprint, orientation -- must
        // not move when fabric is laid over it, or every one of those
        // calibrations is invalidated by this stage.
        let Some((macro_h, tex, _)) = build(5) else { return };
        let ms = macro_h.spec;
        let ts = tex.spec;
        let mut num = 0.0f64;
        let mut den = 0.0f64;
        for y in (8..ms.ny - 8).step_by(5) {
            for x in (8..ms.nx - 8).step_by(5) {
                let w = ms.world_of(x, y);
                let tx = ((w.x / TEX_RES_M).round() as u32).min(ts.nx - 1);
                let ty = ((w.y / TEX_RES_M).round() as u32).min(ts.ny - 1);
                num += (tex.get(tx, ty) - macro_h.get(x, y)).abs();
                den += 1.0;
            }
        }
        let mean_dev = num / den;
        // The residual is order 1 m; the surface is order 30 m. If this stage
        // were moving the macro rather than riding on it, the deviation would
        // be a large fraction of the relief.
        let relief = surface::relief(&macro_h);
        assert!(mean_dev < relief * 0.12,
                "texture moved the macro by {mean_dev:.2} m on {relief:.1} m of relief");
    }

    #[test]
    fn every_conditioning_bucket_has_patches() {
        // A bucket with no patches silently pastes nothing, which would leave
        // an untextured band wherever that condition occurs.
        let Some(p) = pack() else { return };
        for (i, (_, count)) in p.index.iter().enumerate() {
            assert!(*count > 0, "conditioning bucket {i} is empty");
        }
    }

    #[test]
    fn the_quilt_is_deterministic() {
        let Some((_, a, _)) = build(9) else { return };
        let Some((_, b, _)) = build(9) else { return };
        assert_eq!(a.data, b.data);
    }
}
