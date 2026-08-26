//! A0 — the draw. Seed in, `Descriptors` out.
//!
//! Transcript discipline, carried from attempt 4 because it was hard-won:
//! a descriptor with a degenerate range STILL consumes one `next_f64()`, so
//! that fixing one descriptor does not rekey every descriptor after it. New
//! descriptors are APPENDED, never inserted.

use course_seed::{DetRng, RunIdentity};

use crate::record::{record, FormClass, FormSpec, Range, Record};
use crate::rng::{stream, DRAW};
use crate::Mode;

/// Everything the later stages read. One flat struct: the stages are the
/// generator, and threading a dozen small structs between them buys nothing.
#[derive(Clone, Copy, Debug)]
pub struct Descriptors {
    pub mode: Mode,
    pub form: FormClass,

    // ---- dune form (aeolian) ----
    pub orientation_order: f64,
    pub wavelength_m: f64,
    pub dune_relief_m: f64,
    /// Paleowind azimuth, radians. AXIAL: crests run perpendicular to it.
    pub wind_rad: f64,
    pub wind_wander_rad: f64,
    pub wind_wander_m: f64,
    pub kappa: f64,
    pub stoss_deg: f64,
    pub lee_deg: f64,
    pub stoss_share: f64,
    pub blowout_km2: f64,
    pub hummock_lambda_m: f64,
    pub hummock_spread: f64,
    pub belt_patchiness: f64,
    pub belt_patch_m: f64,
    pub hummock_relief_m: f64,
    pub hummock_kappa: f64,
    pub hummock_gate: f64,
    pub hummock_floor: f64,
    pub texture_gain: f64,
    pub texture_floor: f64,

    // ---- shared ----
    pub relief_budget_m: f64,
    pub floor_tilt_m_km: f64,
    /// Tilt azimuth of the interdune datum, radians.
    pub floor_tilt_rad: f64,
    pub water_table_m: f64,
    // fluvial mode
    pub cap_relief_m: f64,
    pub cap_wave_m: f64,
    pub cap_flat: f64,
    pub n_sys: u32,
    pub attach_m: f64,
    pub valley_depth_m: f64,
    pub valley_floor_m: f64,
    pub valley_wall_m: f64,
    pub hand_resid: f64,
    pub fluvial_tex_k: f64,
    pub tex_patchy: f64,
    pub valley_sharp: f64,
    pub divide_warp: f64,
    pub upland_relief_m: f64,
    pub allogenic_river: bool,
}

fn draw(p: &mut DetRng, r: Range) -> f64 {
    // Consumes a draw even when lo == hi. See the module doc.
    let u = p.next_f64();
    r.lo + (r.hi - r.lo) * u
}

/// Draw a site from a seed. `forced_mode` overrides the mode WITHOUT moving
/// the transcript — the coin is still flipped, so forcing a mode for a test
/// or a gallery does not change any other descriptor for that seed.
pub fn site(id: &RunIdentity, forced_mode: Option<Mode>, forced_form: Option<FormClass>)
    -> Descriptors
{
    let rec: Record = record(Mode::Aeolian);
    let mut p = stream(id, DRAW);

    let mode = {
        let coin = p.next_f64();
        forced_mode.unwrap_or(if coin < rec.p_aeolian { Mode::Aeolian } else { Mode::Fluvial })
    };
    let form = {
        let coin = p.next_f64();
        forced_form.unwrap_or(if coin < rec.p_train { FormClass::Train } else { FormClass::Mound })
    };
    let f: &FormSpec = rec.form(form);

    Descriptors {
        mode,
        form,
        orientation_order: draw(&mut p, f.orientation_order),
        wavelength_m: draw(&mut p, f.wavelength_m),
        dune_relief_m: draw(&mut p, f.dune_relief_m),
        wind_rad: draw(&mut p, Range::new(0.0, std::f64::consts::PI)),
        wind_wander_rad: draw(&mut p, f.wind_wander_rad),
        wind_wander_m: draw(&mut p, f.wind_wander_m),
        kappa: draw(&mut p, f.kappa),
        stoss_deg: draw(&mut p, f.stoss_deg),
        lee_deg: draw(&mut p, f.lee_deg),
        stoss_share: draw(&mut p, f.stoss_share),
        blowout_km2: draw(&mut p, f.blowout_km2),
        hummock_lambda_m: draw(&mut p, f.hummock_lambda_m),
        hummock_spread: draw(&mut p, f.hummock_spread),
        belt_patchiness: draw(&mut p, f.belt_patchiness),
        belt_patch_m: draw(&mut p, f.belt_patch_m),
        hummock_relief_m: draw(&mut p, f.hummock_relief_m),
        hummock_kappa: draw(&mut p, f.hummock_kappa),
        hummock_gate: draw(&mut p, f.hummock_gate),
        hummock_floor: draw(&mut p, f.hummock_floor),
        texture_gain: draw(&mut p, f.texture_gain),
        texture_floor: draw(&mut p, f.texture_floor),
        relief_budget_m: draw(&mut p, rec.relief_budget_m),
        floor_tilt_m_km: draw(&mut p, rec.floor_tilt_m_km),
        floor_tilt_rad: draw(&mut p, Range::new(0.0, std::f64::consts::TAU)),
        water_table_m: {
            // squared ramp: shallow tables (lake country) get more mass
            let u = p.next_f64();
            rec.water_table_m.lo + (rec.water_table_m.hi - rec.water_table_m.lo) * u * u
        },
        allogenic_river: {
            let c = p.next_f64();
            c < rec.p_allogenic_river
        },
        cap_relief_m: draw(&mut p, rec.cap_relief_m),
        cap_wave_m: draw(&mut p, rec.cap_wave_m),
        cap_flat: draw(&mut p, rec.cap_flat),
        n_sys: {
            // Single-trunked (review 2026-08-24). The weights coin is still
            // burned so every later draw keeps its value.
            let _legacy = p.next_f64();
            1
        },
        attach_m: draw(&mut p, rec.attach_m),
        // appended AFTER every earlier draw: aeolian byte-stability and the
        // passed network seeds both depend on the draw order above
        valley_depth_m: draw(&mut p, rec.valley_depth_m),
        valley_floor_m: draw(&mut p, rec.valley_floor_m),
        valley_wall_m: draw(&mut p, rec.valley_wall_m),
        hand_resid: draw(&mut p, rec.hand_resid),
        fluvial_tex_k: draw(&mut p, rec.fluvial_tex_k),
        tex_patchy: draw(&mut p, rec.tex_patchy),
        valley_sharp: draw(&mut p, rec.valley_sharp),
        divide_warp: draw(&mut p, rec.divide_warp),
        upland_relief_m: draw(&mut p, rec.upland_relief_m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: u64) -> RunIdentity {
        RunIdentity::from_seed(s)
    }

    #[test]
    fn draw_is_deterministic() {
        for s in 0..32 {
            let a = site(&id(s), None, None);
            let b = site(&id(s), None, None);
            assert_eq!(a.wavelength_m, b.wavelength_m);
            assert_eq!(a.form, b.form);
            assert_eq!(a.allogenic_river, b.allogenic_river);
        }
    }

    #[test]
    fn forcing_does_not_move_the_transcript() {
        // The whole point of flipping the coin and then discarding it.
        for s in 0..32 {
            let free = site(&id(s), None, None);
            let forced = site(&id(s), Some(free.mode), Some(free.form));
            assert_eq!(free.wavelength_m, forced.wavelength_m);
            assert_eq!(free.wind_rad, forced.wind_rad);
            assert_eq!(free.water_table_m, forced.water_table_m);
        }
    }

    #[test]
    fn descriptors_land_inside_their_records() {
        let rec = record(Mode::Aeolian);
        for s in 0..200 {
            let d = site(&id(s), None, None);
            let f = rec.form(d.form);
            assert!(f.orientation_order.contains(d.orientation_order));
            assert!(f.wavelength_m.contains(d.wavelength_m));
            assert!(f.dune_relief_m.contains(d.dune_relief_m));
            assert!(f.stoss_deg.contains(d.stoss_deg));
            assert!(f.lee_deg.contains(d.lee_deg));
            assert!(f.stoss_share.contains(d.stoss_share));
            assert!(rec.relief_budget_m.contains(d.relief_budget_m));
            assert!(d.wind_rad >= 0.0 && d.wind_rad <= std::f64::consts::PI);
        }
    }

    #[test]
    fn the_form_mixture_is_bimodal_not_uniform() {
        // 500 seeds must produce TWO clusters of orientation order with a gap
        // between them -- the corpus measured Ashman D 3.96, and a uniform
        // dial would fill the gap. This is the test that would fail if someone
        // "simplified" the mixture back into a continuum.
        let mut a: Vec<f64> = (0..500)
            .map(|s| site(&id(s), Some(Mode::Aeolian), None).orientation_order)
            .collect();
        a.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let rec = record(Mode::Aeolian);
        let gap_lo = rec.mound.orientation_order.hi;
        let gap_hi = rec.train.orientation_order.lo;
        let in_gap = a.iter().filter(|v| **v > gap_lo && **v < gap_hi).count();
        assert_eq!(in_gap, 0, "{in_gap} seeds landed between the two modes ({gap_lo}..{gap_hi})");
        assert!(a.iter().any(|v| *v <= gap_lo), "no mound tiles drawn");
        assert!(a.iter().any(|v| *v >= gap_hi), "no train tiles drawn");
    }

    #[test]
    fn form_class_frequency_tracks_the_record() {
        let n = 4000;
        let trains = (0..n)
            .filter(|s| site(&id(*s as u64), Some(Mode::Aeolian), None).form == FormClass::Train)
            .count();
        let p = trains as f64 / n as f64;
        let want = record(Mode::Aeolian).p_train;
        assert!((p - want).abs() < 0.03, "p_train measured {p:.3}, record says {want:.3}");
    }

    #[test]
    fn both_modes_are_drawn() {
        let n = 500;
        let aeolian = (0..n)
            .filter(|s| site(&id(*s as u64), None, None).mode == Mode::Aeolian)
            .count();
        assert!(aeolian > 0 && aeolian < n, "the mode coin is stuck at {aeolian}/{n}");
    }
}
