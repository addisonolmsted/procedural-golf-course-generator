"""Axis-aligned lattice check: is there a crosshatch in the height data?

  python3 tools/aeolian/axis_power.py <dir-or-cgrid> [<dir-or-cgrid> ...] [--prefix m_] [--n 20]

Per tile, on the 2-64 m residual (z minus a 64 m gaussian), the ratio of
spectral power within +-8 deg of the x/y axes to the power within +-8 deg of
the diagonals, per wavelength band. Isotropic ground gives ~1.0 in every
band; a grid-locked pattern shows as an excess in the band of its period.
Also the overlap-add ripple: RMS of the 2-16 m band phase-averaged by cell
index modulo 24 (the 48 m patch pitch at 2 m), max/min over the 24 phases.

Reference (2026-09-14, real 2 m lidar): Dismal River 1.03 / 0.92, Sand Hills
1.09 / 1.00, Ballyneal 1.12 / 0.96 at 4-6 / 6-10 m. Ours before the fix:
aeolian 16-22x at 4-4.5 m, 3.5-5x at 7.6-8.4 m; fluvial 3-4x and ~2x.
"""
import sys, glob, pathlib
import numpy as np
from scipy import ndimage as ndi
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[0] / "macro_campaign"))
from macro_campaign import cgrid                      # noqa: E402

BANDS = [(4, 4.5), (4.5, 5), (6, 7), (7.6, 8.4), (8.4, 9.5), (11, 14), (24, 40), (40, 52)]
PITCH = 24


def axis_ratios(z, c):
    z = z.astype(float); n = min(z.shape); z = z[:n, :n]
    z = np.where(np.isfinite(z), z, np.nanmean(z))
    res = z - ndi.gaussian_filter(z, 64 / c / 2.355)
    res = res * np.hanning(n)[:, None] * np.hanning(n)[None, :]
    P = np.abs(np.fft.fftshift(np.fft.fft2(res))) ** 2
    fr = np.fft.fftshift(np.fft.fftfreq(n, c)); fy, fx = np.meshgrid(fr, fr, indexing="ij")
    f = np.hypot(fx, fy); ang = np.degrees(np.arctan2(fy, fx)) % 90
    axis = (ang < 8) | (ang > 82); diag = np.abs(ang - 45) < 8
    out = {}
    for lo, hi in BANDS:
        band = (f > 1 / hi) & (f < 1 / lo)
        out[f"{lo}-{hi}"] = float(P[band & axis].mean() / max(P[band & diag].mean(), 1e-30))
    return out


def pitch_ripple(z, c):
    z = z.astype(float); z = np.where(np.isfinite(z), z, np.nanmean(z))
    band = ndi.gaussian_filter(z, 1.0) - ndi.gaussian_filter(z, 16 / c / 2.355)
    e = band ** 2
    px = np.array([e[:, k::PITCH].mean() for k in range(PITCH)])
    py = np.array([e[k::PITCH, :].mean() for k in range(PITCH)])
    return float(np.sqrt(px.max() / px.min())), float(np.sqrt(py.max() / py.min()))


def tiles(args, prefix):
    for a in args:
        p = pathlib.Path(a)
        if p.is_dir():
            for f in sorted(p.glob(f"{prefix}*.cgrid")):
                if not f.name.endswith(".water.cgrid"):
                    yield f
        else:
            yield p


def main():
    a = sys.argv[1:]
    prefix, n_max = "m_", 20
    if "--prefix" in a: i = a.index("--prefix"); prefix = a[i + 1]; a = a[:i] + a[i + 2:]
    if "--n" in a: i = a.index("--n"); n_max = int(a[i + 1]); a = a[:i] + a[i + 2:]
    rows = []
    for f in list(tiles(a, prefix))[:n_max]:
        z, (_, _, c) = cgrid.read_f32(str(f))
        r = axis_ratios(z, c); rx, ry = pitch_ripple(z, c)
        rows.append((f.stem, r, rx, ry))
        print(f"{f.stem:28s} " + " ".join(f"{k}:{v:5.2f}" for k, v in r.items()) + f"  ripple x/y {rx:.3f}/{ry:.3f}", flush=True)
    if len(rows) > 1:
        print("median".ljust(28) + " " + " ".join(f"{k}:{np.median([r[1][k] for r in rows]):5.2f}" for k in rows[0][1])
              + f"  ripple x/y {np.median([r[2] for r in rows]):.3f}/{np.median([r[3] for r in rows]):.3f}")


if __name__ == "__main__":
    main()
