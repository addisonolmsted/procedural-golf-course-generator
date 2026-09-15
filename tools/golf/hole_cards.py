"""Per-hole cards for qualitative review of a routed nine.

  python3 tools/golf/hole_cards.py <dump_dir> <records.jsonl> <out.html> \
      --seeds 900092,900185,900001 [--prefix m_]

The routed records carry a line of play, not a built hole: greens are points
with a pad radius and the green shapes / bunkers are S8. So these cards show
THE LAND AND THE LINE, which is what S5/S6 decide.

Elevation on these tiles is not uniformly subtle -- centreline range runs
~2 m on a flat fluvial hole to ~26 m on a dune hole -- so no single shading
scale serves both. Every plan here is therefore DETRENDED: the tee->green
chord plane is subtracted and what is left is coloured, which makes a 1.5 m
swale read as strongly as a 15 m dune. Raw 1 m contours (index every 5 m)
are drawn over it so absolute height is still legible.

Per hole:
  * detrended plan, contours, water, the route, the dogleg angle;
  * what the ground hides, from the back tee and from the last full-shot
    station (LZ2 on a par 5, LZ1 on a par 4, the tee on a par 3), by
    ray-sweep line of sight at EYE_M above ground;
  * the centreline profile at a stated vertical exaggeration, with the
    chord drawn and the ground above / below it filled (these are exactly
    `prof_chord` and `carry_dip`, the terms the router scores), water
    carries marked;
  * cross sections at each landing zone and at the green, +-SECTION_M, with
    the cross slope, which says whether a landing zone is a shelf or a kick;
  * the hole's own scored terms.

Per course, one overview card first: the nine lines on the tile with the
play window and clubhouse, and the nine profiles at a shared scale so the
rhythm of the round is visible.
"""
import sys, io, json, base64, pathlib, math
import numpy as np
import scipy.ndimage as ndi
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.gridspec import GridSpec
from matplotlib.patches import Circle, Rectangle

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parents[0] / "macro_campaign"))
from route_audit import Terrain, resample                      # noqa: E402

PAR_COL = {3: "#3caa50", 4: "#3c6edc", 5: "#eb8c28"}
PAD_M = 70.0                 # crop margin around everything the hole draws
EYE_M = 1.7                  # eye height for the line-of-sight sweeps
SECTION_M = 45.0             # half width of a cross section
CROSS_SLOPE_M = 30.0         # cross slope is measured over +-30 m
CORRIDOR_M = 60.0            # the colour scale is set by the residual THIS near the line of play
WATER_RGB = "#0d4f86"
PROFILE_STEP_M = 4.0
N_RAYS = 1440                # ray sweep: 1.3 m spacing at 300 m, finer than the 2 m grid
DOGLEG_MIN_LEG_M = 20.0      # the corpus dogleg filter (shot_profiles.py)


# ------------------------------------------------------------------ terrain --

def crop(t, pts, pad=PAD_M):
    """(z, wet, y0, x0) of the 2 m tile around `pts` with `pad` metres of margin."""
    p = np.asarray(pts, float)
    ny, nx = t.z.shape
    i0 = max(0, int((p[:, 0].min() - pad) / t.c)); i1 = min(ny, int((p[:, 0].max() + pad) / t.c) + 1)
    j0 = max(0, int((p[:, 1].min() - pad) / t.c)); j1 = min(nx, int((p[:, 1].max() + pad) / t.c) + 1)
    return t.z[i0:i1, j0:j1], t.wet[i0:i1, j0:j1], i0 * t.c, j0 * t.c


def detrend(z, y0, x0, c, tee, green, ztee, zgreen):
    """`z` minus the tee->green chord plane (tilted along the play axis)."""
    ny, nx = z.shape
    yy = y0 + np.arange(ny)[:, None] * c
    xx = x0 + np.arange(nx)[None, :] * c
    v = (green[0] - tee[0], green[1] - tee[1])
    L2 = v[0] * v[0] + v[1] * v[1]
    if L2 < 1e-9:
        return z - ztee
    tpar = ((yy - tee[0]) * v[0] + (xx - tee[1]) * v[1]) / L2
    return z - (ztee + (zgreen - ztee) * tpar)


def corridor(z, y0, x0, c, spine, r=CORRIDOR_M):
    """Cells within `r` of the line of play."""
    ny, nx = z.shape
    mark = np.zeros((ny, nx), bool)
    q = resample(np.asarray(spine, float), c)
    ii = np.clip(np.round((q[:, 0] - y0) / c), 0, ny - 1).astype(int)
    jj = np.clip(np.round((q[:, 1] - x0) / c), 0, nx - 1).astype(int)
    mark[ii, jj] = True
    return ndi.distance_transform_edt(~mark, sampling=c) <= r


def visible(z, y0, x0, c, obs, eye=EYE_M):
    """Boolean line-of-sight mask over the crop from `obs`, by radial sweep."""
    ny, nx = z.shape
    oi, oj = (obs[0] - y0) / c, (obs[1] - x0) / c
    if not (0 <= oi < ny and 0 <= oj < nx):
        return np.ones(z.shape, bool)
    oz = float(z[int(round(oi)), int(round(oj))]) + eye
    rs = np.arange(c, math.hypot(ny * c, nx * c) + c, c)
    th = np.arange(N_RAYS) * (2 * math.pi / N_RAYS)
    yy = obs[0] + rs[None, :] * np.sin(th[:, None])
    xx = obs[1] + rs[None, :] * np.cos(th[:, None])
    fi, fj = (yy - y0) / c, (xx - x0) / c
    inside = (fi >= 0) & (fi <= ny - 1) & (fj >= 0) & (fj <= nx - 1)
    ii = np.clip(np.round(fi), 0, ny - 1).astype(int)
    jj = np.clip(np.round(fj), 0, nx - 1).astype(int)
    ang = (z[ii, jj] - oz) / rs[None, :]
    run = np.maximum.accumulate(ang, axis=1)
    prev = np.concatenate([np.full((N_RAYS, 1), -np.inf), run[:, :-1]], axis=1)
    vis = ang >= prev - 1e-9
    flat_v = np.zeros(ny * nx, bool); flat_h = np.zeros(ny * nx, bool)
    idx = (ii * nx + jj)[inside]
    np.logical_or.at(flat_v, idx, vis[inside])
    np.logical_or.at(flat_h, idx, True)
    out = flat_v.reshape(ny, nx); hit = flat_h.reshape(ny, nx)
    out[int(round(oi)), int(round(oj))] = True
    hit[int(round(oi)), int(round(oj))] = True
    if not hit.all():      # rays never land on a few cells: take the nearest one's verdict
        _, (gi, gj) = ndi.distance_transform_edt(~hit, return_indices=True)
        out = out[gi, gj]
    return out


def sees(t, a, b, eye=EYE_M):
    """Is `b` visible from `a` over the 2 m ground?"""
    d = math.hypot(b[0] - a[0], b[1] - a[1])
    if d < 1e-6:
        return True
    n = max(4, int(d / t.c))
    s = np.linspace(0.0, 1.0, n + 1)
    ys = a[0] + (b[0] - a[0]) * s; xs = a[1] + (b[1] - a[1]) * s
    z = np.array([t.z2(y, x) for y, x in zip(ys, xs)])
    z0 = z[0] + eye; z1 = z[-1] + eye
    los = z0 + (z1 - z0) * s
    return bool((z[1:-1] <= los[1:-1] + 1e-9).all())


# ------------------------------------------------------------------ geometry --

def arc_profile(t, poly, step=PROFILE_STEP_M):
    """(arc, z) sampled along a polyline on the 2 m ground."""
    q = resample(np.asarray(poly, float), step)
    z = np.array([t.z2(*p) for p in q])
    d = np.concatenate([[0.0], np.cumsum(np.hypot(*np.diff(q, axis=0).T))])
    return d, z, q


def max_dogleg(spine):
    """Largest turn at an interior vertex with both legs > 20 m, degrees."""
    sp = np.asarray(spine, float)
    best = 0.0
    for i in range(1, len(sp) - 1):
        u, v = sp[i] - sp[i - 1], sp[i + 1] - sp[i]
        nu, nv = np.hypot(*u), np.hypot(*v)
        if nu > DOGLEG_MIN_LEG_M and nv > DOGLEG_MIN_LEG_M:
            best = max(best, math.degrees(math.acos(np.clip(u @ v / (nu * nv), -1, 1))))
    return best


def stations(h):
    """The full-shot stations of a hole: the landing zones, then the green."""
    return [(ly, lx) for (ly, lx, _r) in h["lzs"]] + [tuple(h["green"])]


# ------------------------------------------------------------------- drawing --

def plan(ax, t, h, z, wet, y0, x0, title, res=None, vmax=None):
    ny, nx = z.shape
    ext = (x0, x0 + (nx - 1) * t.c, y0, y0 + (ny - 1) * t.c)
    if res is not None:
        ax.imshow(res, extent=ext, origin="lower", cmap="RdBu_r", vmin=-vmax, vmax=vmax, interpolation="bilinear")
    else:
        gy, gx = np.gradient(z, t.c)
        sl = np.arctan(np.hypot(gx, gy)); asp = np.arctan2(-gx, gy)
        sh = np.clip(np.sin(np.pi / 4) * np.cos(sl) + np.cos(np.pi / 4) * np.sin(sl) * np.cos(np.deg2rad(315) - asp), 0, 1)
        ax.imshow(sh, extent=ext, origin="lower", cmap="gray", vmin=0, vmax=1.15)
    yy = y0 + np.arange(ny) * t.c; xx = x0 + np.arange(nx) * t.c
    X, Y = np.meshgrid(xx, yy)
    lo, hi = math.floor(z.min()), math.ceil(z.max())
    if hi - lo > 0:
        ax.contour(X, Y, z, levels=np.arange(lo, hi + 1, 1.0), colors="k", linewidths=0.3, alpha=0.35)
        idx = np.arange(math.floor(lo / 5) * 5, hi + 5, 5.0)
        ax.contour(X, Y, z, levels=idx, colors="k", linewidths=0.7, alpha=0.5)
    if wet.any():
        ax.imshow(np.ma.masked_where(~wet, np.ones_like(z)), extent=ext, origin="lower",
                  cmap=matplotlib.colors.ListedColormap([WATER_RGB]), vmin=0, vmax=1, alpha=1.0)
        ax.contour(X, Y, wet.astype(float), levels=[0.5], colors="#062d4e", linewidths=0.9)
    route(ax, h)
    ty, tx = h["tees"][0]; gy, gx = h["green"]
    for (py, px, lab) in ((ty, tx, "T"), (gy, gx, "G")):
        ax.annotate(lab, (px, py), fontsize=8.5, fontweight="bold", ha="center", va="center",
                    xytext=(12, 10), textcoords="offset points", color="#111", zorder=11,
                    bbox=dict(fc="white", ec="#555", lw=0.4, alpha=0.85, pad=1.2))
    ax.set_xlim(ext[0], ext[1]); ax.set_ylim(ext[2], ext[3])
    ax.set_aspect("equal"); ax.set_xticks([]); ax.set_yticks([])
    ax.set_title(title, fontsize=8.5, pad=3)


def route(ax, h, lw=2.4):
    col = PAR_COL.get(h["par"], "#888")
    sp = np.asarray(h["spine"], float)
    ax.plot(sp[:, 1], sp[:, 0], color=col, lw=lw, zorder=5, solid_capstyle="round")
    for (ly, lx, lr) in h["lzs"]:
        ax.add_patch(Circle((lx, ly), lr, fill=False, ec=col, lw=1.3, zorder=6))
    for k, (ty, tx) in enumerate(h["tees"]):
        g = (h.get("tee_graded") or [False] * 5)[k] if k < len(h.get("tee_graded") or []) else False
        ax.add_patch(Rectangle((tx - 3.5, ty - 3.5), 7, 7, color="#e8c76e" if g else "#8fe08f",
                               ec="#333", lw=0.4, zorder=7))
    for b in (h.get("bridges") or []):
        if b.get("kind", "spine") == "walk":
            continue
        ax.plot([b["a"][1], b["b"][1]], [b["a"][0], b["b"][0]], color="#d81f1f", lw=4, zorder=8,
                solid_capstyle="butt")
    gy, gx = h["green"]
    ax.plot([gx], [gy], marker="o", ms=9, mfc=col, mec="#1a1a1a", mew=1.1, zorder=9)


def vis_panel(ax, t, h, z, wet, y0, x0, obs, label):
    ny, nx = z.shape
    ext = (x0, x0 + (nx - 1) * t.c, y0, y0 + (ny - 1) * t.c)
    gy_, gx_ = np.gradient(z, t.c)
    sl = np.arctan(np.hypot(gx_, gy_)); asp = np.arctan2(-gx_, gy_)
    sh = np.clip(np.sin(np.pi / 4) * np.cos(sl) + np.cos(np.pi / 4) * np.sin(sl) * np.cos(np.deg2rad(315) - asp), 0, 1)
    ax.imshow(sh, extent=ext, origin="lower", cmap="gray", vmin=0, vmax=1.2)
    v = visible(z, y0, x0, t.c, obs)
    ax.imshow(np.ma.masked_where(v, np.ones_like(z)), extent=ext, origin="lower",
              cmap=matplotlib.colors.ListedColormap(["#3a3a46"]), vmin=0, vmax=1, alpha=0.62)
    route(ax, h, lw=1.6)
    ax.plot([obs[1]], [obs[0]], marker="*", ms=11, mfc="#ffd23f", mec="#222", mew=0.6, zorder=10)
    ax.set_xlim(ext[0], ext[1]); ax.set_ylim(ext[2], ext[3])
    ax.set_aspect("equal"); ax.set_xticks([]); ax.set_yticks([])
    hidden = float(1.0 - v.mean())
    ax.set_title(f"{label}: {100 * hidden:.0f} % blind", fontsize=7.5, pad=2)
    return hidden


def profile(ax, t, h, exag_target=10.0):
    d, z, q = arc_profile(t, h["spine"])
    chord = z[0] + (z[-1] - z[0]) * d / max(d[-1], 1e-9)
    col = PAR_COL.get(h["par"], "#888")
    ax.fill_between(d, chord, z, where=z >= chord, color="#d98b6a", alpha=0.75, lw=0, zorder=2)
    ax.fill_between(d, chord, z, where=z < chord, color="#6a9cd9", alpha=0.75, lw=0, zorder=2)
    ax.plot(d, z, color="#2a2a2a", lw=1.5, zorder=4)
    ax.plot(d, chord, color="#555", lw=1.0, ls="--", zorder=3)
    # stations along the arc
    sp = np.asarray(h["spine"], float)
    seg = np.concatenate([[0.0], np.cumsum(np.hypot(*np.diff(sp, axis=0).T))])
    for k, s in enumerate(seg[1:-1], start=1):
        ax.axvline(s, color=col, lw=1.1, alpha=0.85, zorder=5)
        ax.annotate(f"LZ{k}", (s, 0.94), xycoords=("data", "axes fraction"), fontsize=7,
                    ha="center", va="top", color=col,
                    bbox=dict(fc="white", ec="none", alpha=0.7, pad=0.8))
    for b in (h.get("bridges") or []):
        if b.get("kind", "spine") == "walk":
            continue
        ia = int(np.argmin(np.hypot(q[:, 0] - b["a"][0], q[:, 1] - b["a"][1])))
        ib = int(np.argmin(np.hypot(q[:, 0] - b["b"][0], q[:, 1] - b["b"][1])))
        lo, hi = sorted((d[ia], d[ib]))
        ax.axvspan(lo, hi, color="#d81f1f", alpha=0.3, zorder=1)
    rng = max(z.max() - z.min(), 1.0)
    pad = 0.18 * rng
    ax.set_xlim(0, d[-1]); ax.set_ylim(z.min() - pad, z.max() + pad)
    ax.set_box_aspect(1.0 / 3.4)
    exag = (d[-1] / (rng + 2 * pad)) * (1.0 / 3.4)
    ax.tick_params(labelsize=7); ax.grid(alpha=0.2, lw=0.5)
    ax.set_xlabel("arc, m", fontsize=7, labelpad=1)
    ax.set_ylabel("z, m", fontsize=7, labelpad=1)
    above = float((z - chord).max()); below = float((chord - z).max())
    ax.set_title(f"centreline profile, vertical exaggeration {exag:.0f}x  ·  "
                 f"above chord {above:.1f} m, below {below:.1f} m", fontsize=8, pad=3)
    return above, below


def section(ax, t, h, centre, axis, label, span=None):
    n = (-axis[1], axis[0])
    off = np.arange(-SECTION_M, SECTION_M + t.c, t.c)
    ys = centre[0] + n[0] * off; xs = centre[1] + n[1] * off
    z = np.array([t.z2(y, x) for y, x in zip(ys, xs)])
    col = PAR_COL.get(h["par"], "#888")
    ax.plot(off, z, color="#2a2a2a", lw=1.3)
    ax.fill_between(off, z.min() - 40.0, z, color="#cfc6ae", alpha=0.6, lw=0)
    ax.axvline(0, color=col, lw=1.4)
    zl = float(np.interp(-CROSS_SLOPE_M, off, z)); zr = float(np.interp(CROSS_SLOPE_M, off, z))
    tilt = (zr - zl) / (2 * CROSS_SLOPE_M) * 100.0
    ax.set_xlim(-SECTION_M, SECTION_M)
    sp = span or max(4.0, z.max() - z.min() + 1.0)
    mid = 0.5 * (z.max() + z.min())
    ax.set_ylim(mid - sp / 2, mid + sp / 2)
    ax.set_box_aspect(0.62)
    ax.tick_params(labelsize=6.5); ax.grid(alpha=0.2, lw=0.4)
    ax.set_title(f"{label}  ·  cross slope {tilt:+.1f} %", fontsize=7.5, pad=2)
    ax.set_xlabel("offset, m", fontsize=6.5, labelpad=1)


def fig_to_uri(fig, quality=80):
    buf = io.BytesIO()
    fig.savefig(buf, format="png", dpi=100, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    from PIL import Image
    buf.seek(0)
    im = Image.open(buf).convert("RGB")
    out = io.BytesIO(); im.save(out, "WEBP", quality=quality, method=4)
    return "data:image/webp;base64," + base64.b64encode(out.getvalue()).decode()


# --------------------------------------------------------------------- cards --

def hole_card(t, rec, k):
    h = rec["holes"][k]
    pts = list(h["spine"]) + list(h["tees"]) + [h["green"]] + [(ly, lx) for (ly, lx, _r) in h["lzs"]]
    z, wet, y0, x0 = crop(t, pts)
    tee = tuple(h["tees"][0]); green = tuple(h["green"])
    res = detrend(z, y0, x0, t.c, tee, green, t.z2(*tee), t.z2(*green))
    corr = corridor(z, y0, x0, t.c, h["spine"])
    vmax = float(max(1.5, min(6.0, np.percentile(np.abs(res[corr]), 97))))

    fig = plt.figure(figsize=(13.2, 9.0))
    gs = GridSpec(3, 4, figure=fig, height_ratios=[1.15, 0.95, 0.72],
                  hspace=0.26, wspace=0.16)
    ax_plan = fig.add_subplot(gs[0:2, 0:2])
    plan(ax_plan, t, h, z, wet, y0, x0,
         f"detrended: height minus the tee-to-green chord, ±{vmax:.1f} m  ·  contours 1 m, index 5 m",
         res=res, vmax=vmax)
    sp = np.asarray(h["spine"], float)
    dog = max_dogleg(h["spine"])
    ax_plan.annotate(f"dogleg {dog:.0f}°", xy=(0.015, 0.02), xycoords="axes fraction",
                     fontsize=8, color="#222", bbox=dict(fc="white", ec="none", alpha=0.75, pad=1.6))

    st = stations(h)
    last = tuple(st[-2]) if len(st) > 1 else tee
    ax_v1 = fig.add_subplot(gs[0, 2])
    hid_t = vis_panel(ax_v1, t, h, z, wet, y0, x0, tee, "hidden from the back tee")
    ax_v2 = fig.add_subplot(gs[0, 3])
    hid_l = vis_panel(ax_v2, t, h, z, wet, y0, x0, last,
                      "hidden from the last full-shot station" if len(st) > 1 else "hidden from the tee (par 3)")

    ax_p = fig.add_subplot(gs[1, 2:4])
    above, below = profile(ax_p, t, h)

    axes = [fig.add_subplot(gs[2, i]) for i in range(3)]
    secs = []
    for i, (ly, lx, _r) in enumerate(h["lzs"]):
        secs.append(((ly, lx), f"landing zone {i + 1}"))
    secs.append((green, "green"))
    spans = []
    for centre, _lab in secs[:3]:
        j = int(np.argmin([np.hypot(*(np.asarray(centre) - p)) for p in sp]))
        j = min(max(j, 1), len(sp) - 1)
        ax_v = sp[j] - sp[j - 1]; ax_v = ax_v / max(np.hypot(*ax_v), 1e-9)
        nrm = (-ax_v[1], ax_v[0])
        off = np.arange(-SECTION_M, SECTION_M + t.c, t.c)
        zz = np.array([t.z2(centre[0] + nrm[0] * o, centre[1] + nrm[1] * o) for o in off])
        spans.append(float(zz.max() - zz.min()))
    span = max(4.0, max(spans) + 1.0) if spans else 4.0
    for ax, (centre, label) in zip(axes, secs[:3]):
        j = int(np.argmin([np.hypot(*(np.asarray(centre) - p)) for p in sp]))
        j = min(max(j, 1), len(sp) - 1)
        axis = sp[j] - sp[j - 1]; axis = axis / max(np.hypot(*axis), 1e-9)
        section(ax, t, h, centre, axis, label, span=span)
    fig.text(0.008, 0.30, f"sections share a {span:.0f} m vertical window", fontsize=7, color="#555")
    for ax in axes[len(secs[:3]):]:
        ax.axis("off")

    ax_t = fig.add_subplot(gs[2, 3]); ax_t.axis("off")
    tm = h.get("terms") or {}
    keys = ["green", "setting", "approach", "lz", "length", "line_flow", "prof_chord",
            "carry_dip", "hazard", "carry", "edge", "tee"]
    rows = [f"{a:<11s}{tm[a]:+.2f}" for a in keys if a in tm]
    green_seen = sees(t, last, green)
    lz_seen = sees(t, tee, tuple(st[0])) if len(st) > 1 else True
    txt = (f"green site: {h.get('kind', '?')}\n"
           f"blind ground: {100 * hid_t:.0f} % from the tee, {100 * hid_l:.0f} % from the last station\n"
           f"landing zone visible from the tee: {'yes' if lz_seen else 'NO'}\n"
           f"green visible from the last station: {'yes' if green_seen else 'NO'}\n\n"
           + "\n".join(rows))
    ax_t.text(0.0, 1.0, txt, va="top", ha="left", fontsize=7.4, family="monospace")

    d, z_, _ = arc_profile(t, h["spine"])
    net = float(z_[-1] - z_[0])
    fig.suptitle(f"{rec['seed']}  {rec['mode']}  ·  hole {k + 1}, par {h['par']}, {h['length_m']:.0f} m"
                 f"  ·  net {net:+.1f} m  ·  above chord {above:.1f} m, dip below {below:.1f} m"
                 f"  ·  dogleg {dog:.0f}°", fontsize=11, y=0.985)
    cap = (f'<b>hole {k + 1}</b> · par {h["par"]} · {h["length_m"]:.0f} m · {h.get("kind", "?")} green site · '
           f'net {net:+.1f} m · above chord {above:.1f} m · dip {below:.1f} m · dogleg {dog:.0f}°')
    return fig_to_uri(fig), cap


def course_card(t, rec):
    wy, wx, hh, ww = rec["window_m"]
    pts = ([p for h in rec["holes"] for p in h["spine"]] + [rec["clubhouse"]]
           + [(wy, wx), (wy + hh, wx + ww)])
    z, wet, y0, x0 = crop(t, pts, pad=40.0)
    fig = plt.figure(figsize=(13.2, 7.0))
    gs = GridSpec(3, 6, figure=fig, hspace=0.45, wspace=0.28)
    ax = fig.add_subplot(gs[:, 0:3])
    ny, nx = z.shape
    ext = (x0, x0 + (nx - 1) * t.c, y0, y0 + (ny - 1) * t.c)
    gy_, gx_ = np.gradient(z, t.c)
    sl = np.arctan(np.hypot(gx_, gy_)); asp = np.arctan2(-gx_, gy_)
    sh = np.clip(np.sin(np.pi / 4) * np.cos(sl) + np.cos(np.pi / 4) * np.sin(sl) * np.cos(np.deg2rad(315) - asp), 0, 1)
    ax.imshow(sh, extent=ext, origin="lower", cmap="gray", vmin=0, vmax=1.2)
    if wet.any():
        ax.imshow(np.ma.masked_where(~wet, np.ones_like(z)), extent=ext, origin="lower",
                  cmap=matplotlib.colors.ListedColormap([WATER_RGB]), vmin=0, vmax=1, alpha=1.0)
    ax.add_patch(Rectangle((wx, wy), ww, hh, fill=False, ec="#222", lw=0.9, ls=(0, (4, 3))))
    for k, h in enumerate(rec["holes"]):
        route(ax, h, lw=2.0)
        gy, gx = h["green"]
        ax.annotate(str(k + 1), (gx, gy), fontsize=8, ha="left", va="bottom",
                    xytext=(5, 4), textcoords="offset points", color="#111")
    cy, cx = rec["clubhouse"]
    ax.plot([cx], [cy], marker="o", ms=10, mfc="#e03c3c", mec="#222", mew=0.8, zorder=10)
    ax.set_xlim(ext[0], ext[1]); ax.set_ylim(ext[2], ext[3])
    ax.set_aspect("equal"); ax.set_xticks([]); ax.set_yticks([])
    ax.set_title("the nine, the play window, the clubhouse", fontsize=9, pad=4)

    profs = [arc_profile(t, h["spine"]) for h in rec["holes"]]
    span = max(float(p[1].max() - p[1].min()) for p in profs)
    span = max(span, 4.0)
    for k, (d, zz, _q) in enumerate(profs):
        axp = fig.add_subplot(gs[k // 3, 3 + k % 3])
        h = rec["holes"][k]
        ch = zz[0] + (zz[-1] - zz[0]) * d / max(d[-1], 1e-9)
        axp.fill_between(d, ch, zz, where=zz >= ch, color="#d98b6a", alpha=0.8, lw=0)
        axp.fill_between(d, ch, zz, where=zz < ch, color="#6a9cd9", alpha=0.8, lw=0)
        axp.plot(d, zz, color="#2a2a2a", lw=1.0)
        axp.plot(d, ch, color="#666", lw=0.7, ls="--")
        mid = 0.5 * (zz.max() + zz.min())
        axp.set_ylim(mid - span * 0.6, mid + span * 0.6)
        axp.set_xlim(0, d[-1])
        axp.set_box_aspect(0.42)
        axp.tick_params(labelsize=6)
        axp.set_title(f"{k + 1}  par {h['par']}  {h['length_m']:.0f} m", fontsize=7, pad=1.5,
                      color=PAR_COL.get(h["par"], "#333"))
    fig.suptitle(f"{rec['seed']}  {rec['mode']}  ·  pars {''.join(str(p) for p in rec['pars'])}"
                 f"  ·  {rec['total_length_m']:.0f} m  ·  score {rec['score']:.1f}"
                 f"  ·  profiles share a {span:.0f} m vertical window", fontsize=11, y=0.99)
    cap = (f'<b>{rec["seed"]}</b> {rec["mode"]} · pars {"".join(str(p) for p in rec["pars"])} '
           f'({sum(rec["pars"])}) · {rec["total_length_m"]:.0f} m · score {rec["score"]:.1f} · '
           f'coverage {100 * (rec.get("coverage") or 0):.0f} %')
    return fig_to_uri(fig), cap


CSS = """
:root{--bg:#f6f4ee;--panel:#fffdf8;--ink:#22231f;--muted:#5f5e57;--rule:#ddd8c9;--accent:#8a3b2a}
@media (prefers-color-scheme:dark){:root:not([data-theme="light"]){--bg:#191a17;--panel:#22231f;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72}}
:root[data-theme="dark"]{--bg:#191a17;--panel:#22231f;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72}
body{background:var(--bg);color:var(--ink);font-family:"IBM Plex Sans",system-ui,sans-serif;margin:0;padding:22px 26px 56px}
h1{font-size:1.45rem;margin:0 0 6px} h2{font-size:1.05rem;margin:30px 0 10px;color:var(--accent)}
p{color:var(--muted);max-width:86ch;margin:0 0 14px}
figure{margin:0 0 16px;background:var(--panel);border:1px solid var(--rule);border-radius:3px;padding:8px}
figure img{width:100%;display:block}
figcaption{margin-top:6px;font-size:12px;color:var(--muted);font-family:ui-monospace,monospace}
figcaption b{color:var(--ink)}
"""


def main():
    a = sys.argv[1:]
    prefix, seeds = "m_", None
    for k in ("--prefix", "--seeds"):
        if k in a:
            i = a.index(k); v = a[i + 1]; a = a[:i] + a[i + 2:]
            if k == "--prefix": prefix = v
            else: seeds = [int(s) for s in v.split(",")]
    dump, recs, out = pathlib.Path(a[0]), [json.loads(l) for l in open(a[1])], a[2]
    by = {r["seed"]: r for r in recs if r.get("routed")}
    seeds = seeds or [sorted(by.values(), key=lambda r: -r["score"])[0]["seed"]]
    parts = []
    for s in seeds:
        rec = by[s]
        t = Terrain(dump, prefix, s)
        uri, cap = course_card(t, rec)
        parts.append(f'<h2>{s} · {rec["mode"]}</h2><figure><img src="{uri}" alt="course"><figcaption>{cap}</figcaption></figure>')
        for k in range(len(rec["holes"])):
            uri, cap = hole_card(t, rec, k)
            parts.append(f'<figure><img src="{uri}" alt="hole {k + 1}"><figcaption>{cap}</figcaption></figure>')
        print(f"  {s}: {len(rec['holes'])} holes")
    html = f"""<title>Hole cards</title>
<style>{CSS}</style>
<h1>Hole cards</h1>
<p>The routed line on the land, not a built hole: greens are points with a pad radius, and green shapes and bunkers are S8.
Every plan is <b>detrended</b> &mdash; the tee-to-green chord plane is subtracted and the residual coloured, warm above and cool below &mdash;
so a 1.5&nbsp;m swale reads as strongly as a 15&nbsp;m dune; raw contours are 1&nbsp;m with an index line every 5&nbsp;m.
The two small panels grey out ground the golfer cannot see, from the back tee and from the last full-shot station.
The profile is the centreline against its own chord at the stated vertical exaggeration: warm fill is ground the shot plays over, cool fill is a carry.
Cross sections are &plusmn;45&nbsp;m with the cross slope over the middle 60&nbsp;m. Tee boxes are squares, amber where the ground needs grading;
landing zones are circles at their radius; red bars are water carries.</p>
{''.join(parts)}"""
    open(out, "w").write(html)
    print(f"wrote {out}: {len(parts)} cards, {pathlib.Path(out).stat().st_size / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
