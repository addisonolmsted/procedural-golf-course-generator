"""Archetype-membership cull: does a tile actually exhibit the archetype?

Being inside the right region unit does not make a tile an instance of the
archetype. Hiawatha National Forest contains both pitted kettle ground and
the flat outwash plain between the pits; Big South Fork contains both
terraced plateau margins and sheer gorge walls; protected Florida contains
both wet prairie and a sandy karst ridge. Fitting all of them together
measures "land inside the unit", not the landform the generator is meant to
reproduce — and the generator's job is the landform, because a course is
always sited on the characteristic ground, never on the filler between it.

So each archetype declares its DEFINING FEATURE and a floor for it. Tiles
below the floor are excluded on character, with the reason recorded, exactly
like the development cull.

This does condition the fit on the very quantity being fitted, which is
worth being explicit about: it is a membership test, not tuning. The floors
are set where the corpus is visibly bimodal (kettled 17-44 vs plain 1-3;
benched 1-10 vs gorge 0), not to move a median onto a target. Read the
`FEATURES` table as "what makes this archetype itself".
"""

import json
import pathlib

OUT = pathlib.Path(__file__).resolve().parent.parent / "out"

# archetype -> (knob, floor, description used in the exclusion reason)
FEATURES = {
    # Kettles ARE the moraine archetype; plain outwash is a different
    # landform that happens to share a glacial origin.
    "glacial_moraine": ("basin_count", 10, "outwash plain, not kettled moraine"),
    # The archetype is literally "mountain BENCH" — terraces and scarps.
    # A sheer gorge has more relief and no benches at all.
    "mountain_bench": ("bench_count", 1, "gorge without benches"),
    # Defined by near-zero relief with surface water. A karst upland with
    # 18-31 m of relief is a different landform.
    "florida_lowland": ("relief_amp_m", None, "karst upland, not flat lowland"),
}
# Upper bounds, where the defining feature is the ABSENCE of something.
CEILINGS = {"florida_lowland": ("relief_amp_m", 12.0)}


def run(dry_run: bool = False):
    p = OUT / "exclude.json"
    doc = json.loads(p.read_text()) if p.exists() else {"version": 1, "tiles": []}
    have = {(t["archetype"], t["tile"]) for t in doc["tiles"]}
    added = []
    for arch_dir in sorted((OUT / "extract").iterdir()):
        if not arch_dir.is_dir():
            continue
        arch = arch_dir.name
        floor = FEATURES.get(arch)
        ceil = CEILINGS.get(arch)
        if floor is None and ceil is None:
            continue
        for f in sorted(arch_dir.glob("*.json")):
            if f.name.endswith(".regions.json"):
                continue
            tile = f.stem
            if (arch, tile) in have:
                continue
            knobs = json.loads(f.read_text())["knobs"]
            reason = None
            if floor is not None and floor[1] is not None:
                knob, minimum, desc = floor
                v = knobs.get(knob)
                if v is not None and v < minimum:
                    reason = f"character: {desc} ({knob} {v:.0f} < {minimum})"
            if reason is None and ceil is not None:
                knob, maximum = ceil
                v = knobs.get(knob)
                if v is not None and v > maximum:
                    desc = FEATURES[arch][2]
                    reason = f"character: {desc} ({knob} {v:.1f} > {maximum})"
            if reason:
                added.append({"archetype": arch, "tile": tile, "reason": reason})

    for rec in added:
        print(f"  [cull] {rec['archetype']}/{rec['tile']}: {rec['reason']}")
    if dry_run:
        print(f"\n{len(added)} tile(s) would be culled (dry run)")
        return added
    if added:
        doc["tiles"].extend(added)
        doc["tiles"].sort(key=lambda t: (t["archetype"], t["tile"]))
        p.write_text(json.dumps(doc, indent=1) + "\n")
    print(f"\ncharacter-culled {len(added)} tile(s) -> {p}")
    return added
