"""Frozen-store access: iterate courses, load surfaces + masks + meta.

Wraps the dtm_atlas readers (read-only) and verifies MANIFEST.sha256 at
startup — Stage 1 consumes the byte-frozen Stage-0 store; any drift aborts
(store changes are deliberate Stage-0 re-freezes, never silent).
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field

import numpy as np
import yaml

from . import _REPO  # noqa: F401  (path side effects)
from dtm_atlas import config as atlas_config          # noqa: E402
from dtm_atlas import grids as atlas_grids            # noqa: E402
from dtm_atlas import meta as atlas_meta              # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, ".."))     # tools/dtm_metrics
OUT = os.path.join(ROOT, "out")
NODATA = atlas_config.NODATA


def load_config() -> dict:
    with open(os.path.join(HERE, "config.yaml")) as f:
        return yaml.safe_load(f)


def store_dir() -> str:
    return atlas_config.STORE


def verify_store_or_die() -> None:
    """Abort unless the store matches its MANIFEST (byte-frozen input)."""
    rc = atlas_meta.verify(write=False)
    if rc != 0:
        raise SystemExit(
            "Stage-0 store does not match MANIFEST.sha256 — Stage 1 refuses "
            "to run on a drifted store. Re-freeze deliberately via "
            "`python3 -m dtm_atlas verify --write` after triage."
        )


def course_keys() -> list[str]:
    return sorted(
        k for k in os.listdir(store_dir())
        if os.path.isdir(os.path.join(store_dir(), k))
    )


@dataclass
class Course:
    key: str
    meta: dict
    cell: float
    naturalized: np.ndarray
    raw: np.ndarray
    valid: np.ndarray          # has data
    clean: np.ndarray          # valid & ~inpaint
    inpaint: np.ndarray
    interior: np.ndarray
    exterior: np.ndarray
    water: np.ndarray
    masks: dict = field(default_factory=dict)   # lazily-loaded extra classes

    @property
    def exterior_inpaint_frac(self) -> float:
        return float(self.meta.get("margin", {}).get("exterior_inpaint_frac", 1.0))

    @property
    def clean_frac(self) -> float:
        v = int(self.valid.sum())
        return float(self.clean.sum()) / v if v else 0.0


def load_course(key: str) -> Course:
    d = os.path.join(store_dir(), key)
    meta = atlas_meta.read_meta(os.path.join(d, "meta.json"))
    nat, _t, _e = atlas_grids.read_gtiff(os.path.join(d, "naturalized.tif"))
    raw, _t2, _e2 = atlas_grids.read_gtiff(os.path.join(d, "raw.tif"))

    def m(name: str) -> np.ndarray:
        arr, _tt, _ee = atlas_grids.read_gtiff(os.path.join(d, "masks", f"{name}.tif"))
        return arr.astype(bool)

    valid = raw != NODATA
    inpaint = m("inpaint")
    return Course(
        key=key,
        meta=meta,
        cell=float(meta["grid"]["res_m"]),
        naturalized=nat.astype(np.float64),
        raw=raw.astype(np.float64),
        valid=valid,
        clean=valid & ~inpaint,
        inpaint=inpaint,
        interior=m("boundary_interior"),
        exterior=m("exterior"),
        water=m("water"),
    )
