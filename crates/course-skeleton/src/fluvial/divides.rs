//! Divides derived from the flow field — never authored. An authored divide
//! that disagrees with the flow field is the exact failure mode of the
//! retired generator (stage-02 doc, responsibility 4).
//!
//! Basin labels are CATCHMENTS: chase receivers until the flow path first
//! reaches a channel cell — the label is that channel's id — or a terminal
//! (pit / border outlet) for cells that never reach the network. Labelling
//! by terminal instead puts a boundary along every trunk (left bank and
//! right bank exit through different border cells), i.e. divides ON
//! channels — the exact artifact this formulation removes.

use course_world::grid::GridSpec;
use course_world::math::Vec2;

/// Catchment label per cell. `channel_of[lin]` is the channel id + 1 for
/// channel cells, 0 otherwise. Terminal labels are offset above channel ids.
pub fn basin_labels(rec: &[i64], channel_of: &[u32]) -> Vec<u32> {
    let n = rec.len();
    const OFFSET: u32 = 1 << 24;
    let mut label = vec![u32::MAX; n];
    let mut path: Vec<usize> = Vec::new();
    for start in 0..n {
        if label[start] != u32::MAX {
            continue;
        }
        path.clear();
        let mut cur = start;
        let terminal;
        loop {
            if channel_of[cur] > 0 {
                terminal = channel_of[cur] - 1;
                break;
            }
            if label[cur] != u32::MAX {
                terminal = label[cur];
                break;
            }
            path.push(cur);
            let r = rec[cur];
            if r < 0 {
                terminal = OFFSET + cur as u32;
                break;
            }
            cur = r as usize;
        }
        for &p in &path {
            label[p] = terminal;
        }
        if label[start] == u32::MAX {
            label[start] = terminal;
        }
    }
    label
}

/// Divide mask: a cell any of whose 4-neighbours drains to a different
/// catchment (4-connectivity keeps divides thin). Channel cells never count
/// — adjacent catchments meet ON the stream at every confluence, and a
/// channel is definitionally not a divide.
pub fn divide_mask(labels: &[u32], channel_of: &[u32], spec: &GridSpec) -> Vec<bool> {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let mut mask = vec![false; nx * ny];
    for y in 0..ny {
        for x in 0..nx {
            let lin = y * nx + x;
            if channel_of[lin] > 0 {
                continue;
            }
            let l = labels[lin];
            let differs = |nlin: usize| labels[nlin] != l && channel_of[nlin] == 0;
            if (x + 1 < nx && differs(lin + 1)) || (y + 1 < ny && differs(lin + nx)) {
                mask[lin] = true;
            }
        }
    }
    mask
}

/// Greedy chains through the divide cells, longest-first, world metres.
/// Chains shorter than `min_len_m` are dropped.
pub fn polylines(mask: &[bool], spec: &GridSpec, min_len_m: f64) -> Vec<Vec<Vec2>> {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let cell = spec.cell_size;
    let mut visited = vec![false; nx * ny];
    let mut chains: Vec<Vec<Vec2>> = Vec::new();
    const D8: [(i64, i64); 8] = [
        (0, 1),
        (1, 0),
        (0, -1),
        (-1, 0),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ];
    for start in 0..nx * ny {
        if !mask[start] || visited[start] {
            continue;
        }
        // walk in both directions from the seed to build one chain
        let mut chain: Vec<usize> = vec![start];
        visited[start] = true;
        for pass in 0..2 {
            let mut cur = start;
            loop {
                let (y, x) = ((cur / nx) as i64, (cur % nx) as i64);
                let mut next = None;
                for (dy, dx) in D8 {
                    let (yy, xx) = (y + dy, x + dx);
                    if yy < 0 || xx < 0 || yy >= ny as i64 || xx >= nx as i64 {
                        continue;
                    }
                    let nlin = yy as usize * nx + xx as usize;
                    if mask[nlin] && !visited[nlin] {
                        next = Some(nlin);
                        break;
                    }
                }
                let Some(nlin) = next else { break };
                visited[nlin] = true;
                if pass == 0 {
                    chain.push(nlin);
                } else {
                    chain.insert(0, nlin);
                }
                // Where the mask runs 2 cells thick (diagonal label
                // boundaries) the greedy walk went up one side and back
                // down the other — the review's ridge HAIRPINS. Marking
                // the departed cell's mask-neighbours visited blocks the
                // parallel return path; forward motion is unaffected.
                for (dy, dx) in D8 {
                    let (yy, xx) = (y + dy, x + dx);
                    if yy < 0 || xx < 0 || yy >= ny as i64 || xx >= nx as i64 {
                        continue;
                    }
                    let side = yy as usize * nx + xx as usize;
                    if side != nlin && mask[side] {
                        visited[side] = true;
                    }
                }
                cur = nlin;
            }
        }
        if (chain.len() as f64) * cell >= min_len_m {
            chains.push(
                chain
                    .iter()
                    .map(|&lin| {
                        Vec2::new(
                            ((lin % nx) as f64 + 0.5) * cell,
                            ((lin / nx) as f64 + 0.5) * cell,
                        )
                    })
                    .collect(),
            );
        }
    }
    chains.sort_by(|a, b| b.len().cmp(&a.len()));
    chains.truncate(24);
    chains
}
