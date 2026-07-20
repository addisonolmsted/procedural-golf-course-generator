"""Config loading + campaign fingerprint + output paths."""

from __future__ import annotations

import hashlib
import json
import os

import yaml

from . import OUT, REPO

_HERE = os.path.dirname(os.path.abspath(__file__))


def load_config() -> dict:
    with open(os.path.join(_HERE, "config.yaml")) as f:
        return yaml.safe_load(f)


def cfg_hash(cfg: dict) -> str:
    """Fingerprint stamped into every artifact so threshold drift is visible."""
    return "sha256:" + hashlib.sha256(
        json.dumps(cfg, sort_keys=True, default=list).encode()
    ).hexdigest()[:16]


def repo_path(rel: str) -> str:
    return os.path.join(REPO, rel)


def out_path(*parts: str) -> str:
    p = os.path.join(OUT, *parts)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    return p


def save_json(path: str, obj) -> None:
    """Byte-stable JSON: sorted keys, fixed float rounding upstream."""
    with open(path, "w") as f:
        json.dump(obj, f, indent=1, sort_keys=True)
        f.write("\n")
