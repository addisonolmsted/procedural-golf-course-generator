"""Macro variety audit: does generated within-biome macro variety span what
the real corpus spans?

Per tile/seed, on the 8 m field: macro-band (400-1600 m) orientation order
A, power-weighted dominant wavelength, and band relief (DoG bandpass,
p95-p5). Identical code path for real tiles and generated dumps.
"""
import sys
import pathlib
import numpy as np
from scipy import ndimage

ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
sys.path.insert(0, str(ROOT / "tools" / "macro_campaign"))
from macro_campaign import cgrid, extract_v2  # noqa: E402

GEN_DIR = pathlib.Path(sys.argv[1])
BIOMES = ["piedmont", "great_plains", "river_valley", "hill_country", "heathland", "sandhills"]


def metrics(z8, cell8=8.0):
    n = z8.shape[0]
    # spectrum stats on the Hann-windowed field
    w = np.hanning(n)
    W = np.outer(w, w)
    F = np.fft.fftshift(np.fft.fft2((z8 - z8.mean()) * W))
    P = np.abs(F) ** 2
    f = np.fft.fftshift(np.fft.fftfreq(n, d=cell8))
    fy, fx = np.meshgrid(f, f, indexing="ij")
    fr = np.hypot(fx, fy)
    lam = np.where(fr > 0, 1.0 / np.maximum(fr, 1e-12), np.inf)
    band = (lam >= 400) & (lam <= 1600)
    Pb = P[band]
    theta = np.arctan2(fy, fx)[band]
    c = np.sum(Pb * np.exp(2j * theta)) / np.sum(Pb)
    A = float(np.abs(c))
    lam_dom = float(np.sum(Pb * lam[band]) / np.sum(Pb))
    # band relief via DoG (no window distortion), same constants both sides
    s_hi = 400.0 / np.pi / cell8
    s_lo = 1600.0 / np.pi / cell8
    zb = ndimage.gaussian_filter(z8, s_hi) - ndimage.gaussian_filter(z8, s_lo)
    relief = float(np.percentile(zb, 95) - np.percentile(zb, 5))
    return A, lam_dom, relief


def q(v, p):
    return float(np.percentile(v, p)) if len(v) else float("nan")


def main():
    print(f"{'biome':13s} {'src':4s} {'n':>3s} | {'A p10':>6s} {'p50':>6s} {'p90':>6s} | "
          f"{'lam p10':>7s} {'p50':>6s} {'p90':>6s} | {'rel p10':>7s} {'p50':>6s} {'p90':>6s}")
    for biome in BIOMES:
        rows = {"real": [], "gen": []}
        for tid in [t for a, t in extract_v2.kept_tiles() if a == biome]:
            z, (_, _, cell) = cgrid.read_f32(
                extract_v2.OUT / "tiles" / biome / f"{tid}.cgrid")
            z = z.astype(np.float64)
            if not np.isfinite(z).all():
                m = np.isfinite(z)
                if m.mean() < 0.9:
                    continue
                z = np.where(m, z, np.nanmean(z[m]))
            rows["real"].append(metrics(z[::4, ::4], cell * 4))
        for f in sorted(GEN_DIR.glob(f"{biome}_*.f32")):
            z8 = np.frombuffer(f.read_bytes(), dtype=np.float32).reshape(375, 375).astype(np.float64)
            rows["gen"].append(metrics(z8))
        for src in ("real", "gen"):
            r = rows[src]
            A = [x[0] for x in r]
            L = [x[1] for x in r]
            R = [x[2] for x in r]
            print(f"{biome:13s} {src:4s} {len(r):3d} | {q(A,10):6.3f} {q(A,50):6.3f} {q(A,90):6.3f} | "
                  f"{q(L,10):7.0f} {q(L,50):6.0f} {q(L,90):6.0f} | {q(R,10):7.1f} {q(R,50):6.1f} {q(R,90):6.1f}")
        print()


if __name__ == "__main__":
    main()
