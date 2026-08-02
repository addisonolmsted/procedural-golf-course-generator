"""Structural metrics — how the surface is ORGANIZED, not how big its parts are.

The knob set describes feature sizes and counts. It can be entirely in range
while the terrain looks nothing like the corpus, because it says nothing
about how space is filled. These metrics close that gap, and they are
computed identically on real tiles and on generated ones so `compare` can
put them side by side.

The one that matters most is `dist_to_channel_p50_m`: in every real
archetype it is 104-120 m regardless of relief, because a drainage network
fills space to a characteristic length. It is also robust to channel width
and to the accumulation threshold, which drainage density is not.
"""

import numpy as np
from scipy import ndimage
from skimage.morphology import skeletonize

# Windows for local relief. 100-400 m is the scale of a golf hole, which is
# exactly the band a plane-plus-a-few-lozenges surface leaves empty.
RELIEF_WINDOWS_M = (100.0, 200.0, 400.0)
# Drainage density is reported at three thresholds because the SHAPE of the
# curve is diagnostic: a real network keeps adding low-order tributaries as
# the threshold drops, a synthetic trunk-and-fan has nothing to add.
DENSITY_THRESHOLD_MULTIPLIERS = (0.25, 1.0, 4.0)
NEAR_CHANNEL_M = 150.0


def _plane_residual_frac(z: np.ndarray, cell: float, relief: float) -> float:
    """Fraction of the tile within +-10% of relief of its best-fit plane —
    i.e. how much of the surface is just the regional tilt."""
    if relief <= 0:
        return 1.0
    ny, nx = z.shape
    yy, xx = np.mgrid[0:ny:8, 0:nx:8]
    zz = z[::8, ::8]
    ok = np.isfinite(zz)
    if ok.sum() < 16:
        return float("nan")
    a = np.column_stack(
        [xx[ok].ravel() * cell, yy[ok].ravel() * cell, np.ones(int(ok.sum()))]
    )
    sol, *_ = np.linalg.lstsq(a, zz[ok].ravel(), rcond=None)
    yy2, xx2 = np.mgrid[0:ny, 0:nx]
    plane = sol[0] * xx2 * cell + sol[1] * yy2 * cell + sol[2]
    return float((np.abs(z - plane) <= 0.10 * relief).mean())


def _local_relief(z: np.ndarray, cell: float, window_m: float) -> float:
    """Median (max - min) in a moving window — relief available at the scale
    a hole is played at."""
    k = max(int(round(window_m / cell)), 3)
    hi = ndimage.maximum_filter(z, size=k, mode="nearest")
    lo = ndimage.minimum_filter(z, size=k, mode="nearest")
    return float(np.median(hi - lo))


def _density_km_km2(mask: np.ndarray, cell: float) -> float:
    """Channel length per unit area, from a SKELETONIZED mask.

    Counting mask cells directly (the old `extras` field) measures channel
    AREA, so a wide channel inflates the number and it is not comparable to
    a polyline density or across surfaces with different channel widths.
    """
    if not mask.any():
        return 0.0
    skel = skeletonize(mask)
    length_km = skel.sum() * cell / 1000.0
    area_km2 = mask.size * cell * cell / 1e6
    return float(length_km / area_km2)


def structure_metrics(
    z: np.ndarray,
    cell: float,
    acc: np.ndarray,
    channels: np.ndarray,
    channel_area_m2: float,
) -> dict:
    """Organization metrics for one tile.

    `acc` is the drainage-area field (m^2) and `channels` the accepted
    channel mask, both from the caller's flow routing, so real and generated
    surfaces go through identical code.

    The two families deliberately use different masks:

    - distance-to-channel uses the ACCEPTED mask, because it asks what
      drainage the terrain actually presents (a lake body is genuinely not
      a channel, and graded road ditches are not either);
    - the density series uses the RAW accumulation thresholds, because its
      job is the threshold sweep — whether low-order tributaries keep
      appearing as the cut-off drops — and that signature must not be
      confounded by the lake/development acceptance policy, which differs
      between real tiles and generated ones.
    """
    out: dict[str, float] = {}
    finite = z[np.isfinite(z)]
    relief = float(np.percentile(finite, 99) - np.percentile(finite, 1)) if finite.size else 0.0

    # --- space filling -------------------------------------------------
    if channels.any():
        dist = ndimage.distance_transform_edt(~channels, sampling=cell)
        out["dist_to_channel_p25_m"] = float(np.percentile(dist, 25))
        out["dist_to_channel_p50_m"] = float(np.percentile(dist, 50))
        out["dist_to_channel_p75_m"] = float(np.percentile(dist, 75))
        out["dist_to_channel_p90_m"] = float(np.percentile(dist, 90))
        out["frac_within_150m"] = float((dist <= NEAR_CHANNEL_M).mean())
    else:
        for k in ("p25", "p50", "p75", "p90"):
            out[f"dist_to_channel_{k}_m"] = float("nan")
        out["frac_within_150m"] = 0.0

    # --- drainage density vs threshold (the low-order signature) -------
    for mult in DENSITY_THRESHOLD_MULTIPLIERS:
        tag = f"{mult:g}x".replace(".", "p")
        out[f"drainage_density_{tag}"] = _density_km_km2(
            acc >= mult * channel_area_m2, cell
        )

    # --- relief organization -------------------------------------------
    for w in RELIEF_WINDOWS_M:
        out[f"local_relief_{int(w)}m"] = _local_relief(z, cell, w)
    out["plane_residual_frac"] = _plane_residual_frac(z, cell, relief)
    gy, gx = np.gradient(z, cell)
    out["slope_median"] = float(np.nanmedian(np.hypot(gx, gy)))

    # Relief inside the routable core vs the whole tile. Reported, not
    # gated: it is the number that will show whether the core cap is
    # making generated cores flatter than real playable ground.
    c0, c1 = int(750 / cell), int(2250 / cell)
    zc = z[c0:c1, c0:c1]
    zc = zc[np.isfinite(zc)]
    if zc.size and relief > 0:
        core = float(np.percentile(zc, 99) - np.percentile(zc, 1))
        out["core_relief_m"] = core
        out["core_relief_ratio"] = core / relief
    else:
        out["core_relief_m"] = float("nan")
        out["core_relief_ratio"] = float("nan")
    return out


def ridge_metrics(cls10: np.ndarray, cell10: float, n_components: float) -> dict:
    """Interfluve coverage from the geomorphon classification.

    Area fraction is the discriminating one: real piedmont has 39% of the
    tile on a convex ridge/spur. That cannot be produced by placing a
    handful of ridge objects at any amplitude — it takes a space-filling
    network whose interfluves are the residual.
    """
    from dtm_primitives import geomorphons

    mask = geomorphons.ridge_mask(cls10)
    return {
        "ridge_mask_area_frac": float(mask.mean()),
        "ridge_component_count": float(n_components),
    }
