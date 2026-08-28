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

/// Exponent on the water-table ramp. See `water_table_m` in `site()`.
const WATER_RAMP: f64 = 1.0;

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
    pub upland_relief_m: f64,
    pub divide_wander: f64,
    pub n_bays: u32,
    pub p_valley_creek: f64,
    pub allogenic_river: bool,
    /// Nominal channel geometry width, metres. Two classes: a creek, which
    /// may run in a deep gorge, and a river, which gets a correspondingly
    /// shallower valley (`gorge::build`). The rendered water is ~1.2 m wider
    /// than this -- see the draw site -- and lands on 3-5 m and 9-15 m.
    pub river_w_m: f64,
    /// The steeper Pinehurst-end draw. See `Record::p_upland`.
    pub upland: bool,
    /// Exponent on the valley coordinate before the measured profile is read
    /// (`assemble.rs`). Above 1 it holds the floor flat further out and the
    /// wall arrives later, so it widens floor and valley WITHOUT changing the
    /// depth the profile ends at. Lowering it is therefore the one lever that
    /// steepens a tile without deepening it, which is exactly what the
    /// Pinehurst gap needed once relief was already on target.
    pub floor_p: f64,
    /// Megaform body-shaping exponent (aeolian). 1.0 = the raw normalised
    /// field; above 1 narrows the sand bodies laterally and flattens the
    /// interdune ground -- see `surface::shape_body`. Set per form class,
    /// NOT drawn, so the transcript is untouched.
    pub body_p: f64,
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

    let mut ds = Descriptors {
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
            // Ramp, not uniform: shallow tables (lake country) get more mass,
            // because a uniform draw made a lake-forming table a 7% event.
            //
            // Softened from u^2 to u^1.15 (review 2026-08-27: slightly less
            // water). The exponent is the right lever because water_table_m
            // acts TWICE in the same direction -- it sets the table plane
            // (water.rs) and the pan depth cap (blowout.rs), so wet fraction
            // responds to it roughly quadratically. Softening the ramp moves
            // the median table deeper without touching either endpoint or
            // either mechanism's calibration.
            let u = p.next_f64();
            rec.water_table_m.lo
                + (rec.water_table_m.hi - rec.water_table_m.lo) * u.powf(WATER_RAMP)
        },
        river_w_m: 4.0,          // set at the tail, after the upland block
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
        upland_relief_m: draw(&mut p, rec.upland_relief_m),
        divide_wander: draw(&mut p, rec.divide_wander),
        p_valley_creek: rec.p_valley_creek,
        n_bays: {
            let w = rec.bay_weights;
            let u = p.next_f64() * (w[0] + w[1] + w[2] + w[3]);
            if u < w[0] { 0 }
            else if u < w[0] + w[1] { 1 }
            else if u < w[0] + w[1] + w[2] { 2 }
            else { 3 }
        },
        upland: false,
        floor_p: 1.60,
        body_p: match form { FormClass::Mound => 1.6, FormClass::Train => 1.0 },
    };

    // --- the Pinehurst variant, appended at the transcript TAIL ------------
    // One coin after every existing draw, so nothing above it re-keys. It
    // shifts dials already drawn rather than drawing new ones, which keeps
    // the addition to a single `next_f64()`.
    //
    // What it shifts is set by measurement, not by taste: over 19,714 seeds,
    // distance to Pinehurst correlates with valley_depth_m (-0.20),
    // cap_relief_m (-0.12) and valley_sharp (-0.12) and with nothing else
    // (|r| < 0.03 for attach_m, cap_flat, cap_wave, divide_wander,
    // upland_relief_m). Deeper valleys, more cap relief and a sharper valley
    // edge are the three axes that carry it.
    if p.next_f64() < rec.p_upland {
        ds.valley_depth_m += 2.1;
        ds.cap_relief_m += 10.0;
        ds.valley_sharp = (ds.valley_sharp + 0.14).min(0.98);
        // Relief was already within 0.4 m of Pinehurst after the three shifts
        // above, but median slope was still half a point short. Dropping the
        // floor exponent narrows the flat floor and brings the wall in, which
        // raises slope at constant depth.
        ds.floor_p = 1.18;
        ds.upland = true;
    }

    // Appended after the upland block, which is the current tail of the DRAW
    // transcript. BOTH draws are taken on either branch -- a branch that took
    // one draw and a branch that took two would slide every later descriptor
    // by one on half the seeds.
    let wide = p.next_f64() < 0.38;
    let wu = p.next_f64();
    // Nominal geometry, trimmed ~1.2 m below the target band: the wetting
    // test carries a 0.32-cell tolerance each side to keep a narrow channel
    // connected on the 2 m grid, so the RENDERED water runs that much wider
    // than the number here. Measured on the output, not assumed.
    ds.river_w_m = if wide { 7.8 + 6.0 * wu } else { 1.8 + 2.0 * wu };

    ds
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
