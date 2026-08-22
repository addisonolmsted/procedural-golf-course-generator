//! The section engine: a trunk carries a PROGRAM of zones per side, not a
//! formula. Zones are the old WindowClass vocabulary as cross-valley
//! segments — Floor, Slope (piedmont slope), Scarp (escarpment face),
//! Bench (terrace tread), Crown (interfluve). Every parameter is drawn per
//! SIDE and modulated smoothly ALONG the trunk; scarp/bench presence can be
//! gated by hardness, so the strata column drives where the bluffs are.

use course_draw::records::{ZoneKind, ZoneSpec};
use course_draw::Descriptors;
use course_seed::DetRng;
use course_template::fields::Strata;
use course_world::math;
use course_world::noise;

/// Arc spacing of the parameter stations.
pub const STATION_M: f64 = 150.0;

/// One side of one trunk: drawn zone parameters + per-station gates.
pub struct SideProgram {
    /// Per zone: drawn width, drawn rise (constant per side)...
    w: Vec<f64>,
    r: Vec<f64>,
    kind: Vec<ZoneKind>,
    /// ...and per station: the smooth presence gate [0,1] and the width
    /// modulation factor.
    gate: Vec<Vec<f64>>,
    wmod: Vec<Vec<f64>>,
    /// Floor half-width per station (arc-varying, per side).
    pub floor_hw: Vec<f64>,
    /// Per-zone mean gate over all stations (the far-field value).
    gate_mean: Vec<f64>,
    n_stations: usize,
    total_arc: f64,
}

impl SideProgram {
    /// Draw one side's program. `hardness_probe(arc, z)` samples the strata.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        rng: &mut DetRng,
        spec: &[ZoneSpec],
        d: &Descriptors,
        strata: &Strata,
        bed_z_at: &dyn Fn(f64) -> f64,
        pos_at: &dyn Fn(f64) -> course_world::math::Vec2,
        total_arc: f64,
        width_var: f64,
    ) -> SideProgram {
        let n_st = (total_arc / STATION_M).ceil().max(2.0) as usize + 1;
        let base_fhw = d.floor_hw_m * d.floor_widen;
        let s_floor = rng.next_u32();

        let mut w = Vec::new();
        let mut r = Vec::new();
        let mut kind = Vec::new();
        let mut gate: Vec<Vec<f64>> = Vec::new();
        let mut wmod: Vec<Vec<f64>> = Vec::new();

        for z in spec {
            let zw = rng.range_f64(z.w_m.lo, z.w_m.hi);
            let zr = rng.range_f64(z.rise_m.lo, z.rise_m.hi);
            let s_gate = rng.next_u32();
            let s_wmod = rng.next_u32();
            // PER-SIDE PREVALENCE draw: some seeds are nearly bluff-free,
            // some carry long runs — review: "the entire valley edge is a
            // bluff on nearly every seed" was gate_p acting uniformly.
            let prevalence = 0.30 + 0.75 * rng.next_f64();
            let thr = 1.0 - (z.gate_p * prevalence).min(0.95);
            // scarps gate over LONG stretches; other zones vary faster
            let gate_lam = if z.kind == ZoneKind::Scarp { 1700.0 } else { 900.0 };
            let mut g_st = Vec::with_capacity(n_st);
            let mut w_st = Vec::with_capacity(n_st);
            // running elevation guess for the hardness probe: half the
            // accumulated rise so far — coarse, but the gate only needs to
            // know roughly which stratum this zone sits in
            let z_guess: f64 = r.iter().sum::<f64>() + zr * 0.5;
            for si in 0..n_st {
                let arc = si as f64 * STATION_M;
                // smooth gate noise, ~900 m wavelength along the trunk
                let nv = noise::perlin1(arc / gate_lam, s_gate) * 0.5 + 0.5;
                let mut input = nv;
                if z.hard_gate > 0.0 {
                    let p = pos_at(arc.min(total_arc));
                    let hard = strata.hardness_at(p, bed_z_at(arc.min(total_arc)) + z_guess);
                    input = nv * (1.0 - z.hard_gate) + hard * z.hard_gate;
                }
                g_st.push(math::smoothstep(thr - 0.12, thr + 0.12, input));
                let wm = 1.0 + width_var * noise::perlin1(arc / 1300.0, s_wmod);
                w_st.push(wm.max(0.25));
            }
            w.push(zw);
            r.push(zr);
            kind.push(z.kind);
            gate.push(g_st);
            wmod.push(w_st);
        }

        let floor_hw = (0..n_st)
            .map(|si| {
                let arc = si as f64 * STATION_M;
                (base_fhw * (1.0 + width_var * noise::perlin1(arc / 1500.0, s_floor))).max(8.0)
            })
            .collect();

        let gate_mean = gate
            .iter()
            .map(|g| g.iter().sum::<f64>() / g.len().max(1) as f64)
            .collect();
        SideProgram { w, r, kind, gate, wmod, floor_hw, gate_mean, n_stations: n_st, total_arc }
    }

    fn station(&self, arc: f64) -> (usize, usize, f64) {
        let f = (arc.clamp(0.0, self.total_arc) / STATION_M)
            .min(self.n_stations as f64 - 1.001);
        let i = f.floor() as usize;
        let t = math::smoothstep(0.0, 1.0, f - i as f64);
        (i, (i + 1).min(self.n_stations - 1), t)
    }

    pub fn floor_hw_at(&self, arc: f64) -> f64 {
        let (i, j, t) = self.station(arc);
        self.floor_hw[i] * (1.0 - t) + self.floor_hw[j] * t
    }

    /// Rise above the bed at cross-distance `dist`, arc `arc`. Monotone in
    /// dist by construction: every zone climbs or holds.
    ///
    /// ARC-SENSITIVITY FADES WITH DISTANCE: the arc coordinate jumps across
    /// the medial axis (behind bends and ends), and any arc-varying
    /// parameter prints that jump as a cliff — the third member of the seam
    /// family after distance (Gaussian) and side (signed-lateral blend).
    /// Beyond ~1400 m the gates and widths hold their arc-means, so the
    /// jump has nothing to print.
    pub fn rise(&self, arc: f64, dist: f64) -> f64 {
        let fhw = self.floor_hw_at(arc);
        let mut u = dist - fhw;
        if u <= 0.0 {
            return 0.0;
        }
        let fade = (1.0 - (dist - 350.0).max(0.0) / 650.0).clamp(0.0, 1.0);
        let (i, j, t) = self.station(arc);
        let mut acc = 0.0;
        for zi in 0..self.w.len() {
            let g_arc = self.gate[zi][i] * (1.0 - t) + self.gate[zi][j] * t;
            let g = self.gate_mean[zi] * (1.0 - fade) + g_arc * fade;
            if g <= 0.01 {
                continue;
            }
            let wm_arc = self.wmod[zi][i] * (1.0 - t) + self.wmod[zi][j] * t;
            let wm = 1.0 * (1.0 - fade) + wm_arc * fade;
            let zw = (self.w[zi] * wm * g).max(4.0);
            let zr = self.r[zi] * g;
            if u <= zw {
                let f = (u / zw).clamp(0.0, 1.0);
                let shape = match self.kind[zi] {
                    // slope: smooth ramp
                    ZoneKind::Slope => math::smoothstep(0.0, 1.0, f),
                    // scarp: the climb concentrated mid-face
                    ZoneKind::Scarp => math::smoothstep(0.36, 0.64, f),
                    // bench: near-flat with its small tilt spread evenly
                    ZoneKind::Bench => f,
                };
                return acc + zr * shape;
            }
            acc += zr;
            u -= zw;
        }
        // crown: gentle continuation toward the cap (handled by caller's cap)
        acc + u * 0.015
    }

    /// Total programmed rise at an arc (for the cap and for probes).
    pub fn total_rise(&self, arc: f64) -> f64 {
        let (i, j, t) = self.station(arc);
        (0..self.w.len())
            .map(|zi| self.r[zi] * (self.gate[zi][i] * (1.0 - t) + self.gate[zi][j] * t))
            .sum()
    }
}
