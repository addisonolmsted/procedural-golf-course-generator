//! Parse the Parkland Atlas HTML into an [`Atlas`].
//!
//! The HTML embeds `window.COURSES = {...}` — a JSON object whose per-course
//! payload is: base64 elevation (`b64`, little-endian u16 decimeters above
//! `emin`), base64 tree/water bitmasks (`tb64`/`wb64`, MSB-first), survey
//! extents, and hole polylines. This module runs once, at pack time.

use crate::{binfmt, Atlas, Course, Hole, StyleGroup, GRID_N};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::Deserialize;
use std::collections::HashMap;

/// The atlas's own grouping (mirrors the `GROUPS` constant in its JS), in
/// display order.
const GROUP_KEYS: [(&str, StyleGroup); 27] = [
    ("doral", StyleGroup::Lowland),
    ("sawgrass", StyleGroup::Lowland),
    ("olympia", StyleGroup::Lowland),
    ("bayhill", StyleGroup::Lowland),
    ("harbourtown", StyleGroup::Lowland),
    ("brookline", StyleGroup::Lowland),
    ("eastlake", StyleGroup::Rolling),
    ("quail", StyleGroup::Rolling),
    ("cherryhills", StyleGroup::Rolling),
    ("wingedfoot", StyleGroup::Rolling),
    ("firestone", StyleGroup::Rolling),
    ("colonial", StyleGroup::Rolling),
    ("valhalla", StyleGroup::Rolling),
    ("bethpage", StyleGroup::Rolling),
    ("augusta", StyleGroup::Rolling),
    ("wentworth", StyleGroup::Rolling),
    ("golfnational", StyleGroup::Rolling),
    ("congressional", StyleGroup::Rolling),
    ("valderrama", StyleGroup::Rolling),
    ("riviera", StyleGroup::Mountain),
    ("oakmont", StyleGroup::Mountain),
    ("chapultepec", StyleGroup::Mountain),
    ("crans", StyleGroup::Mountain),
    ("jasper", StyleGroup::Mountain),
    ("greenbrier", StyleGroup::Mountain),
    ("capilano", StyleGroup::Mountain),
    ("banff", StyleGroup::Mountain),
];

#[derive(Deserialize)]
struct RawHole {
    #[serde(rename = "ref")]
    ref_no: u32,
    par: Option<u32>,
    pts: Vec<[f64; 2]>,
}

#[derive(Deserialize)]
struct RawCourse {
    label: String,
    #[serde(default)]
    arch: String,
    bbox: [f64; 4],
    wb64: String,
    /// Tree-canopy bitmask (ESA WorldCover class 10). Optional: absent in the
    /// oldest atlas HTML — treated as all-dry so those courses still load.
    #[serde(default)]
    tb64: String,
    treepct: f32,
    waterpct: f32,
    wm: f64,
    hm: f64,
    emin: f64,
    emax: f64,
    b64: String,
    holes: Vec<RawHole>,
}

pub fn parse_html(html: &str) -> Result<Atlas, String> {
    let marker = "window.COURSES=";
    let start = html
        .find(marker)
        .ok_or_else(|| format!("no `{marker}` found — is this the atlas HTML?"))?
        + marker.len();
    let rest = &html[start..];
    let end = rest
        .find("</script>")
        .ok_or_else(|| "unterminated COURSES script".to_string())?;
    let json = rest[..end].trim().trim_end_matches(';');

    let mut raw: HashMap<String, RawCourse> =
        serde_json::from_str(json).map_err(|e| format!("COURSES JSON: {e}"))?;

    let mut courses = Vec::with_capacity(raw.len());
    for (key, group) in GROUP_KEYS {
        if let Some(rc) = raw.remove(key) {
            courses.push(build_course(key.to_string(), group, rc)?);
        }
    }
    // Courses the grouping table doesn't know (a regenerated atlas): classify
    // by survey relief using the atlas's own band edges.
    let mut extra: Vec<(String, RawCourse)> = raw.drain().collect();
    extra.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, rc) in extra {
        let relief = rc.emax - rc.emin;
        let group = if relief < 30.0 {
            StyleGroup::Lowland
        } else if relief < 80.0 {
            StyleGroup::Rolling
        } else {
            StyleGroup::Mountain
        };
        courses.push(build_course(key, group, rc)?);
    }

    let fingerprint = binfmt::fingerprint(&courses);
    Ok(Atlas {
        courses,
        fingerprint,
    })
}

fn build_course(key: String, group: StyleGroup, rc: RawCourse) -> Result<Course, String> {
    let n2 = GRID_N * GRID_N;

    let ebytes = B64
        .decode(rc.b64.as_bytes())
        .map_err(|e| format!("{key}: elevation base64: {e}"))?;
    if ebytes.len() != 2 * n2 {
        return Err(format!(
            "{key}: elevation payload is {} bytes, expected {}",
            ebytes.len(),
            2 * n2
        ));
    }
    let mut heights = Vec::with_capacity(n2);
    for i in 0..n2 {
        let v = u16::from_le_bytes([ebytes[2 * i], ebytes[2 * i + 1]]);
        heights.push((rc.emin + v as f64 / 10.0) as f32);
    }

    let water = decode_mask(&key, &rc.wb64)?;
    let trees = if rc.tb64.is_empty() {
        vec![0u8; n2]
    } else {
        decode_mask(&key, &rc.tb64)?
    };

    let holes = rc
        .holes
        .into_iter()
        .map(|h| Hole {
            ref_no: h.ref_no,
            par: h.par,
            pts_ll: h.pts.into_iter().map(|p| (p[0], p[1])).collect(),
        })
        .collect();

    Ok(Course {
        key,
        label: rc.label,
        arch: rc.arch,
        group,
        wm: rc.wm,
        hm: rc.hm,
        emin: rc.emin,
        emax: rc.emax,
        heights,
        water,
        trees,
        waterpct: rc.waterpct,
        treepct: rc.treepct,
        bbox: rc.bbox,
        holes,
    })
}

/// Unpack an MSB-first bitmask into one byte per cell (0/1).
fn decode_mask(key: &str, b64: &str) -> Result<Vec<u8>, String> {
    let n2 = GRID_N * GRID_N;
    let bytes = B64
        .decode(b64.as_bytes())
        .map_err(|e| format!("{key}: mask base64: {e}"))?;
    if bytes.len() != n2 / 8 {
        return Err(format!(
            "{key}: mask payload is {} bytes, expected {}",
            bytes.len(),
            n2 / 8
        ));
    }
    let mut mask = vec![0u8; n2];
    for (i, &b) in bytes.iter().enumerate() {
        for k in 0..8 {
            mask[i * 8 + k] = (b >> (7 - k)) & 1;
        }
    }
    Ok(mask)
}
