//! What a bed stack actually does to golf: TREAD WIDTH.
//!
//! A resistant bed makes a riser; the soft bed beneath it makes the tread. On
//! a slope of grade s, a soft bed of thickness T outcrops as a tread
//! `T / s` metres wide. That horizontal number -- not the bed thickness -- is
//! what decides whether a bench can hold a fairway.
//!
//! Slopes are the measured corpus medians and p90s
//! (tools/macro_campaign/... outlet_probe, 203 clean tiles):
//!   piedmont 11.4/25.8  great_plains 5.4/12.6  river_valley 0.9/3.4
//!   hill_country 18.7/33.0  heathland 1.8/10.7  sandhills 9.7/22.9  (%)
//!
//! Golf reference widths: fairway 30-45 m, green complex 35-45 m, tee 10-15 m.

use course_draw::{records, Archetype};

fn slopes(a: Archetype) -> (f64, f64) {
    match a {
        Archetype::Piedmont => (0.114, 0.258),
        Archetype::GreatPlains => (0.054, 0.126),
        Archetype::RiverValley => (0.009, 0.034),
        Archetype::HillCountry => (0.187, 0.330),
        Archetype::Heathland => (0.018, 0.107),
        Archetype::Sandhills => (0.097, 0.229),
    }
}

fn main() {
    println!("{:<14} {:>10} {:>13} {:>18} {:>18}  {}",
             "archetype", "riser m", "soft bed m", "tread @ p50 slope", "tread @ p90 slope", "verdict");
    for a in Archetype::ALL {
        let r = records::record(a);
        if r.riser_m.hi <= 0.01 {
            println!("{:<14} {:>10} {:>13} {:>18} {:>18}  no strata", a.key(), "-", "-", "-", "-");
            continue;
        }
        // Only risers at or above the bench threshold make a contact at all
        // -- below it the trace is never emitted. Scoring the whole record
        // range including riser ~ 0 gave piedmont a spurious "too narrow".
        let riser_lo = r.riser_m.lo.max(course_template::SCARP_MIN_RISER_M);
        if riser_lo > r.riser_m.hi {
            println!("{:<14} {:>10} {:>13} {:>18} {:>18}  risers below the {} m bench threshold",
                     a.key(), "-", "-", "-", "-", course_template::SCARP_MIN_RISER_M);
            continue;
        }
        // hard bed 0.9-1.6x riser (the riser itself), soft bed 2.2-4.5x
        let (soft_lo, soft_hi) = (riser_lo * 2.2, r.riser_m.hi * 4.5);
        let (s50, s90) = slopes(a);
        let (t50l, t50h) = (soft_lo / s50, soft_hi / s50);
        let (t90l, t90h) = (soft_lo / s90, soft_hi / s90);
        // Judge on TYPICAL ground (p50). Judging on p90 slope called piedmont
        // too narrow at 29-59 m treads, which is a fairway on most of the tile.
        let verdict = if t50h < 30.0 {
            "TOO NARROW - corduroy; no bench holds a fairway"
        } else if t50l > 400.0 {
            "TOO WIDE - one bench per tile; reads as unstepped"
        } else if t50l < 30.0 {
            "marginal - only the widest benches hold a fairway"
        } else if t90l < 30.0 {
            "fairway on most benches; tees and greens on the steep ones"
        } else {
            "every bench holds a fairway"
        };
        println!("{:<14} {:>4.1}-{:<5.1} {:>6.1}-{:<6.1} {:>8.0}-{:<9.0} {:>8.0}-{:<9.0}  {}",
                 a.key(), riser_lo, r.riser_m.hi, soft_lo, soft_hi,
                 t50l, t50h, t90l, t90h, verdict);
    }
}
