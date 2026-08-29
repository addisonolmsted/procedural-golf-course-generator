"""Course records: one JSON per course polygon + a sorted jsonl index.

Course id = "{slug}-{osmtype}{osmid}" — the OSM polygon id is globally unique
and stable, which retires the millidegree tile key (collides at ~111 m,
west-hemisphere-only)."""

from __future__ import annotations

import json
import re

from . import config


def slugify(name: str) -> str:
    s = re.sub(r"[^a-z0-9]+", "", (name or "course").lower())
    return s[:24] or "course"


def course_id(name: str, osm_type: str, osm_id: int) -> str:
    return f"{slugify(name)}-{osm_type[0]}{osm_id}"


def path(cid: str):
    return config.REGISTRY / f"{cid}.json"


def write(rec: dict) -> None:
    config.REGISTRY.mkdir(parents=True, exist_ok=True)
    path(rec["course_id"]).write_text(
        json.dumps(rec, sort_keys=True, separators=(",", ":")))


def load(cid: str) -> dict:
    return json.loads(path(cid).read_text())


def all_records() -> list[dict]:
    if not config.REGISTRY.is_dir():
        return []
    return [json.loads(p.read_text())
            for p in sorted(config.REGISTRY.glob("*.json"))]


def write_index() -> int:
    recs = all_records()
    lines = []
    for r in recs:
        lines.append(json.dumps({
            "course_id": r["course_id"], "name": r["name"],
            "fame_tier": r.get("fame_tier", 1), "source": r["source"],
            "region_tag": r["region_tag"], "n_greens": len(r["greens"]),
            "status": r["status"], "centroid_ll": r["centroid_ll"],
        }, sort_keys=True))
    (config.OUT / "courses.jsonl").write_text("\n".join(lines) + "\n")
    return len(recs)
