"""Fame tier seeding: tier 3 iconic, tier 2 notable, tier 1 everything else.

Seeding rule (documented per plan):
  tier 3  parkland REFERENCE ∩ US (their curated_key) + the hand list of
          championship/top-100 venues below
  tier 2  curated courses whose parkland architect field matches a noted
          architect — written as SUGGESTIONS for human review
  tier 1  default

Output: fame_tiers.json {course_id: tier} + fame_review.txt listing the
tier-2 suggestions with their architect strings. Human edits the json; this
script never overwrites an existing file (hand edits are the ground truth).
"""

from __future__ import annotations

import json
import re

from . import config, registry

# dtm_atlas/uscourses.py REFERENCE_US
REFERENCE_US_KEYS = {
    "augusta", "bayhill", "bethpage", "brookline", "cherryhills", "colonial",
    "congressional", "doral", "eastlake", "firestone", "greenbrier", "oakmont",
    "olympia", "quail", "riviera", "sawgrass", "valhalla", "wingedfoot",
}
# hand list: majors venues / consensus top-100 that aren't in REFERENCE_US
ICONIC_KEYS = {
    "pinehurst2", "pebble", "shinnecock", "oakhill", "merion", "baltusrol",
    "medinah", "southernhills", "hazeltine", "muirfieldvillage",
    "oaklandhills", "seminole", "pinevalley", "prairiedunes", "sandhillsgc",
}
NOTED_ARCHITECTS = re.compile(
    r"Ross|Tillinghast|MacKenzie|Mackenzie|Maxwell|Flynn|Raynor|Macdonald|"
    r"Coore|Crenshaw|Doak|Trent Jones|Nicklaus|Dye|Banks|Emmet|Travis|"
    r"Colt|Alison|Langford|Thompson|Park", re.IGNORECASE)


def seed() -> dict:
    dest = config.PKG / "fame_tiers.json"
    if dest.exists():
        return json.loads(dest.read_text())
    tiers: dict[str, int] = {}
    review = []
    for r in registry.all_records():
        cid = r["course_id"]
        key = r.get("curated_key", "")
        arch = r.get("architect", "")
        if key in REFERENCE_US_KEYS or key in ICONIC_KEYS:
            tiers[cid] = 3
        elif arch and NOTED_ARCHITECTS.search(arch):
            tiers[cid] = 2
            review.append(f"{cid:44s} tier 2?  {arch}")
        else:
            tiers[cid] = 1
    dest.write_text(json.dumps(tiers, indent=1, sort_keys=True))
    (config.PKG / "fame_review.txt").write_text(
        "# tier-2 suggestions (architect match) — edit fame_tiers.json\n"
        + "\n".join(review) + "\n")
    return tiers


def apply() -> dict:
    """Push fame_tiers.json into the registry records."""
    tiers = json.loads((config.PKG / "fame_tiers.json").read_text())
    counts = {1: 0, 2: 0, 3: 0}
    for r in registry.all_records():
        t = int(tiers.get(r["course_id"], 1))
        if r.get("fame_tier") != t:
            r["fame_tier"] = t
            registry.write(r)
        counts[t] += 1
    registry.write_index()
    return counts
